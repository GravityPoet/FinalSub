use async_trait::async_trait;
use std::path::{Path, PathBuf};

use super::vad::{detect_speech, SAMPLE_RATE};
use super::{AsrCapabilities, AsrEngine, AsrModelRef, ProgressSink, ProgressUpdate, TranscribeJob};
use crate::core::subtitle::{Cue, SubtitleTrack};
use crate::error::{FinalSubError, Result};

pub const PARAFORMER_MODEL_ID: &str = "paraformer-zh-int8";
pub const PARAFORMER_ARCHIVE_DIR: &str = "sherpa-onnx-paraformer-zh-int8-2025-10-07";
pub const QWEN3_MODEL_ID: &str = "qwen3-asr-0.6b-int8";
pub const QWEN3_ARCHIVE_DIR: &str = "sherpa-onnx-qwen3-asr-0.6B-int8-2026-03-25";
pub const FIRERED_MODEL_ID: &str = "firered-asr2-ctc-int8";
pub const FIRERED_ARCHIVE_DIR: &str = "sherpa-onnx-fire-red-asr2-ctc-zh_en-int8-2026-02-25";
pub const SILERO_VAD_SHA256: &str =
    "9e2449e1087496d8d4caba907f23e0bd3f78d91fa552479bb9c23ac09cbb1fd6";

const PARAFORMER_FILES: &[&str] = &["model.int8.onnx", "tokens.txt"];
const QWEN3_FILES: &[&str] = &[
    "conv_frontend.onnx",
    "encoder.int8.onnx",
    "decoder.int8.onnx",
    "tokenizer/vocab.json",
    "tokenizer/merges.txt",
    "tokenizer/tokenizer_config.json",
];
const FIRERED_FILES: &[&str] = &["model.int8.onnx", "tokens.txt"];
const MAX_SEGMENT_SECONDS: usize = 55;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SherpaNativeKind {
    Paraformer,
    Qwen3,
    FireRedCtc,
}

impl SherpaNativeKind {
    pub fn engine_id(self) -> &'static str {
        match self {
            Self::Paraformer => "paraformer",
            Self::Qwen3 => "qwen3-asr",
            Self::FireRedCtc => "firered-asr",
        }
    }

    pub fn model_id(self) -> &'static str {
        match self {
            Self::Paraformer => PARAFORMER_MODEL_ID,
            Self::Qwen3 => QWEN3_MODEL_ID,
            Self::FireRedCtc => FIRERED_MODEL_ID,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Paraformer => "Paraformer",
            Self::Qwen3 => "Qwen3-ASR",
            Self::FireRedCtc => "FireRedASR2 CTC",
        }
    }

    fn required_files(self) -> &'static [&'static str] {
        match self {
            Self::Paraformer => PARAFORMER_FILES,
            Self::Qwen3 => QWEN3_FILES,
            Self::FireRedCtc => FIRERED_FILES,
        }
    }

    fn supported_languages(self) -> Vec<String> {
        match self {
            Self::Paraformer => vec!["auto".into(), "zh".into()],
            Self::FireRedCtc => vec!["auto".into(), "zh".into(), "en".into(), "yue".into()],
            Self::Qwen3 => [
                "auto", "zh", "en", "yue", "ja", "ko", "ar", "de", "fr", "es", "pt", "id", "it",
                "ru", "th", "vi", "tr", "hi", "ms", "nl", "sv", "da", "fi", "pl", "cs", "fil",
                "fa", "el", "hu", "mk", "ro",
            ]
            .into_iter()
            .map(str::to_string)
            .collect(),
        }
    }
}

pub struct SherpaNativeEngine {
    kind: SherpaNativeKind,
    models_dir: PathBuf,
    vad_model_path: PathBuf,
}

impl SherpaNativeEngine {
    pub fn new(kind: SherpaNativeKind, models_dir: PathBuf, vad_model_path: PathBuf) -> Self {
        Self {
            kind,
            models_dir,
            vad_model_path,
        }
    }

    pub fn model_dir(&self, model: &AsrModelRef) -> PathBuf {
        model
            .model_path
            .as_deref()
            .filter(|path| !path.trim().is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| self.models_dir.join(self.kind.model_id()))
    }

    pub fn is_model_installed_at(kind: SherpaNativeKind, model_dir: &Path) -> bool {
        kind.required_files()
            .iter()
            .all(|name| model_dir.join(name).is_file())
    }

    fn validate_model_ref(&self, model: &AsrModelRef) -> Result<()> {
        if model.engine_id != self.kind.engine_id() || model.model_id != self.kind.model_id() {
            return Err(FinalSubError::Validation(format!(
                "{} 模型引用不匹配：期望 {}/{}，实际 {}/{}",
                self.kind.label(),
                self.kind.engine_id(),
                self.kind.model_id(),
                model.engine_id,
                model.model_id
            )));
        }
        Ok(())
    }

    fn recognizer_config(
        kind: SherpaNativeKind,
        model_dir: &Path,
    ) -> sherpa_onnx::OfflineRecognizerConfig {
        let path = |name: &str| model_dir.join(name).to_string_lossy().to_string();
        let mut config = sherpa_onnx::OfflineRecognizerConfig::default();
        match kind {
            SherpaNativeKind::Paraformer => {
                config.model_config.paraformer = sherpa_onnx::OfflineParaformerModelConfig {
                    model: Some(path("model.int8.onnx")),
                };
                config.model_config.tokens = Some(path("tokens.txt"));
            }
            SherpaNativeKind::Qwen3 => {
                config.model_config.qwen3_asr = sherpa_onnx::OfflineQwen3ASRModelConfig {
                    conv_frontend: Some(path("conv_frontend.onnx")),
                    encoder: Some(path("encoder.int8.onnx")),
                    decoder: Some(path("decoder.int8.onnx")),
                    tokenizer: Some(path("tokenizer")),
                    max_total_len: 512,
                    max_new_tokens: 512,
                    temperature: 1e-6,
                    top_p: 0.8,
                    seed: 42,
                    hotwords: None,
                };
                // Qwen3 uses its tokenizer directory rather than a tokens.txt file.
                config.model_config.tokens = Some(String::new());
            }
            SherpaNativeKind::FireRedCtc => {
                config.model_config.fire_red_asr_ctc =
                    sherpa_onnx::OfflineFireRedAsrCtcModelConfig {
                        model: Some(path("model.int8.onnx")),
                    };
                config.model_config.tokens = Some(path("tokens.txt"));
            }
        }
        config.model_config.num_threads = std::thread::available_parallelism()
            .map(|count| count.get().clamp(2, 8) as i32)
            .unwrap_or(2);
        config.model_config.provider = Some("cpu".into());
        config.decoding_method = Some("greedy_search".into());
        config
    }
}

fn clean_text(text: &str) -> String {
    let mut cleaned = String::new();
    let mut in_tag = false;
    for character in text.chars() {
        match character {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            _ if !in_tag => cleaned.push(character),
            _ => {}
        }
    }
    cleaned.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn normalize_token(token: &str) -> String {
    token.replace('\u{2581}', " ")
}

fn ends_sentence(text: &str) -> bool {
    text.chars().last().is_some_and(|character| {
        matches!(
            character,
            '。' | '？' | '！' | '；' | '.' | '?' | '!' | ';' | ':' | '：'
        )
    })
}

fn contains_cjk(text: &str) -> bool {
    text.chars().any(
        |character| matches!(character as u32, 0x3400..=0x9fff | 0x3040..=0x30ff | 0xac00..=0xd7af),
    )
}

fn split_text_blocks(text: &str, max_subtitle_chars: i32) -> Vec<String> {
    let normalized = clean_text(text);
    if normalized.is_empty() {
        return Vec::new();
    }
    let max_chars = if contains_cjk(&normalized) { 28 } else { 84 };
    if normalized.contains(char::is_whitespace) {
        let mut blocks = Vec::new();
        let mut current = String::new();
        for word in normalized.split_whitespace() {
            if !current.is_empty() {
                current.push(' ');
            }
            current.push_str(word);
            if ends_sentence(word)
                || crate::core::subtitle::should_break_for_width(
                    &current,
                    max_subtitle_chars,
                    current.chars().count() >= max_chars,
                )
            {
                blocks.push(std::mem::take(&mut current));
            }
        }
        if !current.is_empty() {
            blocks.push(current);
        }
        return blocks;
    }

    let mut blocks = Vec::new();
    let mut current = String::new();
    for character in normalized.chars() {
        current.push(character);
        if ends_sentence(&current)
            || crate::core::subtitle::should_break_for_width(
                &current,
                max_subtitle_chars,
                current.chars().count() >= max_chars,
            )
        {
            blocks.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        blocks.push(current);
    }
    blocks
}

fn build_even_cues(text: &str, start_ms: u64, end_ms: u64, max_subtitle_chars: i32) -> Vec<Cue> {
    let blocks = split_text_blocks(text, max_subtitle_chars);
    if blocks.is_empty() || end_ms <= start_ms {
        return Vec::new();
    }
    let total_weight = blocks
        .iter()
        .map(|block| {
            block
                .chars()
                .filter(|character| !character.is_whitespace())
                .count()
                .max(1)
        })
        .sum::<usize>();
    let duration_ms = end_ms - start_ms;
    let mut cursor = start_ms;
    let block_count = blocks.len();
    blocks
        .into_iter()
        .enumerate()
        .map(|(index, text)| {
            let block_end = if index + 1 == block_count {
                end_ms
            } else {
                let weight = text
                    .chars()
                    .filter(|character| !character.is_whitespace())
                    .count()
                    .max(1);
                (cursor + duration_ms * weight as u64 / total_weight as u64)
                    .min(end_ms)
                    .max(cursor + 1)
            };
            let cue = Cue {
                index: (index + 1) as u32,
                start_ms: cursor,
                end_ms: block_end,
                text,
            };
            cursor = block_end;
            cue
        })
        .collect()
}

fn push_timestamp_cue(
    cues: &mut Vec<Cue>,
    text: &mut String,
    start_ms: u64,
    end_ms: u64,
    segment_end_ms: u64,
) {
    let normalized = clean_text(text);
    text.clear();
    if normalized.is_empty() || start_ms >= segment_end_ms {
        return;
    }
    let previous_end = cues.last().map(|cue| cue.end_ms).unwrap_or(start_ms);
    let start_ms = start_ms.max(previous_end).min(segment_end_ms - 1);
    let end_ms = end_ms.min(segment_end_ms).max(start_ms + 1);
    cues.push(Cue {
        index: (cues.len() + 1) as u32,
        start_ms,
        end_ms,
        text: normalized,
    });
}

fn build_segment_cues(
    text: &str,
    tokens: &[String],
    timestamps: Option<&[f32]>,
    segment_start_ms: u64,
    segment_end_ms: u64,
    max_subtitle_chars: i32,
) -> Vec<Cue> {
    if let Some(timestamps) = timestamps {
        if !timestamps.is_empty() && timestamps.len() == tokens.len() {
            let mut cues = Vec::new();
            let mut current = String::new();
            let mut cue_start = segment_start_ms;
            for (index, token) in tokens.iter().enumerate() {
                let token = token.trim();
                if token.is_empty() || (token.starts_with('<') && token.ends_with('>')) {
                    continue;
                }
                let token_ms = segment_start_ms + (timestamps[index].max(0.0) * 1_000.0) as u64;
                let normalized_token = normalize_token(token);
                if !current.trim().is_empty() {
                    let mut candidate = current.clone();
                    candidate.push_str(&normalized_token);
                    if crate::core::subtitle::exceeds_custom_subtitle_width(
                        candidate.trim(),
                        max_subtitle_chars,
                    ) {
                        push_timestamp_cue(
                            &mut cues,
                            &mut current,
                            cue_start,
                            token_ms,
                            segment_end_ms,
                        );
                        cue_start = token_ms.min(segment_end_ms.saturating_sub(1));
                    }
                }
                if current.is_empty() {
                    cue_start = token_ms.min(segment_end_ms.saturating_sub(1));
                }
                current.push_str(&normalized_token);
                let next_ms = timestamps
                    .get(index + 1)
                    .map(|value| segment_start_ms + (value.max(0.0) * 1_000.0) as u64)
                    .unwrap_or(segment_end_ms);
                let max_chars = if contains_cjk(&current) { 28 } else { 84 };
                if ends_sentence(token)
                    || crate::core::subtitle::should_break_for_width(
                        current.trim(),
                        max_subtitle_chars,
                        current.chars().count() >= max_chars,
                    )
                {
                    push_timestamp_cue(&mut cues, &mut current, cue_start, next_ms, segment_end_ms);
                }
            }
            if !current.is_empty() {
                push_timestamp_cue(
                    &mut cues,
                    &mut current,
                    cue_start,
                    segment_end_ms,
                    segment_end_ms,
                );
            }
            if !cues.is_empty() {
                return cues;
            }
        }
    }
    build_even_cues(text, segment_start_ms, segment_end_ms, max_subtitle_chars)
}

#[async_trait]
impl AsrEngine for SherpaNativeEngine {
    fn id(&self) -> &'static str {
        self.kind.engine_id()
    }

    fn capabilities(&self) -> AsrCapabilities {
        AsrCapabilities {
            supports_streaming: false,
            supported_languages: self.kind.supported_languages(),
            requires_model_download: true,
        }
    }

    async fn prepare(&self, model: &AsrModelRef) -> Result<()> {
        self.validate_model_ref(model)?;
        let model_dir = self.model_dir(model);
        if !Self::is_model_installed_at(self.kind, &model_dir) {
            return Err(FinalSubError::Validation(format!(
                "{} 模型尚未完整安装，请先在模型管理下载 {}：{}",
                self.kind.label(),
                self.kind.model_id(),
                model_dir.display()
            )));
        }
        if !self.vad_model_path.is_file() {
            return Err(FinalSubError::Validation(format!(
                "{} 的内置 Silero VAD 资源缺失：{}",
                self.kind.label(),
                self.vad_model_path.display()
            )));
        }
        Ok(())
    }

    async fn transcribe(
        &self,
        job: TranscribeJob,
        progress: ProgressSink,
        cancel_rx: Option<tokio::sync::watch::Receiver<bool>>,
    ) -> Result<SubtitleTrack> {
        self.validate_model_ref(&job.model)?;
        if let Some(language) = job
            .language
            .as_deref()
            .map(str::trim)
            .filter(|language| !language.is_empty())
        {
            let supported = self.kind.supported_languages();
            if !supported.iter().any(|candidate| candidate == language) {
                return Err(FinalSubError::Validation(format!(
                    "{} 不支持语言代码 {language}",
                    self.kind.label()
                )));
            }
        }
        if cancel_rx
            .as_ref()
            .is_some_and(|receiver| *receiver.borrow())
        {
            return Err(FinalSubError::Validation("任务已取消".into()));
        }

        let kind = self.kind;
        let model_dir = self.model_dir(&job.model);
        let vad_model_path = self.vad_model_path.clone();
        let audio_path = job.audio_path.clone();
        let max_subtitle_chars = job.max_subtitle_chars;
        let worker_progress = progress.clone();
        let worker_cancel = cancel_rx.clone();

        progress
            .send(ProgressUpdate {
                progress: 0.04,
                message: format!("正在加载 {} 与 Silero VAD...", kind.label()),
            })
            .await
            .ok();

        let handle =
            tokio::task::spawn_blocking(move || -> std::result::Result<Vec<Cue>, String> {
                let wave = sherpa_onnx::Wave::read(&audio_path)
                    .ok_or_else(|| "读取 WAV 音频失败，请确认音频提取结果有效".to_string())?;
                if wave.sample_rate() != SAMPLE_RATE {
                    return Err(format!(
                        "{} 需要 16 kHz 单声道 WAV，当前采样率为 {} Hz",
                        kind.label(),
                        wave.sample_rate()
                    ));
                }
                let segments = detect_speech(wave.samples(), &vad_model_path, MAX_SEGMENT_SECONDS)?;
                if segments.is_empty() {
                    return Err("Silero VAD 未检测到可识别的人声".into());
                }
                if worker_cancel
                    .as_ref()
                    .is_some_and(|receiver| *receiver.borrow())
                {
                    return Err("任务已取消".into());
                }

                let config = Self::recognizer_config(kind, &model_dir);
                let recognizer = sherpa_onnx::OfflineRecognizer::create(&config)
                    .ok_or_else(|| format!("创建 {} 原生识别器失败", kind.label()))?;
                let total = segments.len();
                let mut cues = Vec::new();
                for (segment_index, segment) in segments.into_iter().enumerate() {
                    if worker_cancel
                        .as_ref()
                        .is_some_and(|receiver| *receiver.borrow())
                    {
                        return Err("任务已取消".into());
                    }
                    let stream = recognizer.create_stream();
                    stream.accept_waveform(SAMPLE_RATE, &segment.samples);
                    recognizer.decode(&stream);
                    let result = stream.get_result().ok_or_else(|| {
                        format!("{} 未返回第 {} 段识别结果", kind.label(), segment_index + 1)
                    })?;
                    let start_ms = segment.start_sample as u64 * 1_000 / SAMPLE_RATE as u64;
                    let end_ms = (segment.start_sample + segment.samples.len()) as u64 * 1_000
                        / SAMPLE_RATE as u64;
                    let mut segment_cues = build_segment_cues(
                        &result.text,
                        &result.tokens,
                        result.timestamps.as_deref(),
                        start_ms,
                        end_ms,
                        max_subtitle_chars,
                    );
                    cues.append(&mut segment_cues);
                    let fraction = (segment_index + 1) as f32 / total as f32;
                    worker_progress
                        .blocking_send(ProgressUpdate {
                            progress: 0.12 + fraction * 0.84,
                            message: format!(
                                "{} 正在识别语音片段 {}/{}...",
                                kind.label(),
                                segment_index + 1,
                                total
                            ),
                        })
                        .ok();
                }
                for (index, cue) in cues.iter_mut().enumerate() {
                    cue.index = (index + 1) as u32;
                }
                Ok(cues)
            });

        let cues = if let Some(mut cancel) = cancel_rx {
            tokio::pin!(handle);
            loop {
                tokio::select! {
                    result = &mut handle => {
                        break result
                            .map_err(|error| FinalSubError::Validation(format!("{} 线程池异常：{error}", self.kind.label())))?
                            .map_err(FinalSubError::Validation)?;
                    }
                    changed = cancel.changed() => {
                        if changed.is_err() || *cancel.borrow() {
                            return Err(FinalSubError::Validation("任务已取消".into()));
                        }
                    }
                }
            }
        } else {
            handle
                .await
                .map_err(|error| {
                    FinalSubError::Validation(format!("{} 线程池异常：{error}", self.kind.label()))
                })?
                .map_err(FinalSubError::Validation)?
        };

        if cues.is_empty() {
            return Err(FinalSubError::Validation(format!(
                "{} 未识别到字幕内容，请确认语言、音频和模型选择",
                self.kind.label()
            )));
        }
        progress
            .send(ProgressUpdate {
                progress: 1.0,
                message: format!("{} 转录完成，共 {} 条字幕", self.kind.label(), cues.len()),
            })
            .await
            .ok();
        Ok(SubtitleTrack { cues })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    #[test]
    fn every_engine_requires_its_complete_model_manifest() {
        for kind in [
            SherpaNativeKind::Paraformer,
            SherpaNativeKind::Qwen3,
            SherpaNativeKind::FireRedCtc,
        ] {
            let temp = tempfile::tempdir().unwrap();
            for file in kind.required_files() {
                let path = temp.path().join(file);
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                std::fs::write(path, b"test").unwrap();
            }
            assert!(SherpaNativeEngine::is_model_installed_at(kind, temp.path()));
            std::fs::remove_file(temp.path().join(kind.required_files()[0])).unwrap();
            assert!(!SherpaNativeEngine::is_model_installed_at(
                kind,
                temp.path()
            ));
        }
    }

    #[test]
    fn recognizer_configs_map_to_the_expected_sherpa_family() {
        let dir = Path::new("/models/current");
        let paraformer = SherpaNativeEngine::recognizer_config(SherpaNativeKind::Paraformer, dir);
        assert_eq!(
            paraformer.model_config.paraformer.model.as_deref(),
            Some("/models/current/model.int8.onnx")
        );
        let qwen = SherpaNativeEngine::recognizer_config(SherpaNativeKind::Qwen3, dir);
        assert_eq!(qwen.model_config.qwen3_asr.max_new_tokens, 512);
        assert_eq!(
            qwen.model_config.qwen3_asr.tokenizer.as_deref(),
            Some("/models/current/tokenizer")
        );
        let firered = SherpaNativeEngine::recognizer_config(SherpaNativeKind::FireRedCtc, dir);
        assert_eq!(
            firered.model_config.fire_red_asr_ctc.model.as_deref(),
            Some("/models/current/model.int8.onnx")
        );
    }

    #[test]
    fn segment_cues_preserve_vad_offsets_and_real_token_timestamps() {
        let tokens = vec!["你".into(), "好".into(), "。".into()];
        let timestamps = [0.1, 0.3, 0.5];
        let cues = build_segment_cues("你好。", &tokens, Some(&timestamps), 5_000, 7_000, 0);
        assert_eq!(cues.len(), 1);
        assert_eq!(cues[0].start_ms, 5_100);
        assert_eq!(cues[0].end_ms, 7_000);
        assert_eq!(cues[0].text, "你好。");
    }

    #[test]
    fn long_unpunctuated_cjk_text_is_split_into_readable_cues() {
        let text = "这是一段没有任何标点但是长度足够长所以需要自动切分成多条字幕避免整段文字覆盖屏幕影响阅读体验的中文文本";
        let cues = build_even_cues(text, 1_000, 11_000, 0);
        assert!(cues.len() >= 2);
        assert_eq!(cues.first().unwrap().start_ms, 1_000);
        assert_eq!(cues.last().unwrap().end_ms, 11_000);
        assert!(cues
            .windows(2)
            .all(|pair| pair[0].end_ms == pair[1].start_ms));
    }

    #[test]
    fn custom_width_uses_real_token_boundaries_and_unlimited_keeps_run() {
        let tokens = vec!["▁Hello".into(), "▁world".into(), "▁again".into()];
        let timestamps = [0.0, 0.4, 0.8];
        let custom = build_segment_cues(
            "Hello world again",
            &tokens,
            Some(&timestamps),
            5_000,
            6_600,
            8,
        );
        assert_eq!(custom.len(), 3);
        assert_eq!(custom[0].text, "Hello");
        assert_eq!(custom[0].end_ms, 5_400);
        assert!(custom
            .iter()
            .all(|cue| crate::core::subtitle::subtitle_visual_width(&cue.text) <= 8));

        let unlimited = build_segment_cues(
            "Hello world again",
            &tokens,
            Some(&timestamps),
            5_000,
            6_600,
            -1,
        );
        assert_eq!(unlimited.len(), 1);
    }

    #[test]
    fn bundled_silero_vad_matches_pinned_digest() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join("vad")
            .join("silero_vad.onnx");
        let bytes = std::fs::read(path).unwrap();
        assert_eq!(hex::encode(Sha256::digest(bytes)), SILERO_VAD_SHA256);
    }
}
