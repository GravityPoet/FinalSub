use async_trait::async_trait;
use std::path::{Path, PathBuf};

use super::{AsrCapabilities, AsrEngine, AsrModelRef, ProgressSink, ProgressUpdate, TranscribeJob};
use crate::core::subtitle::{Cue, SubtitleTrack};
use crate::error::{FinalSubError, Result};

pub const PARAKEET_MODEL_ID: &str = "parakeet-tdt-0.6b-v2";
pub const PARAKEET_ARCHIVE_DIR: &str = "sherpa-onnx-nemo-parakeet-tdt-0.6b-v2-int8";
pub(super) const REQUIRED_MODEL_FILES: &[&str] = &[
    "encoder.int8.onnx",
    "decoder.int8.onnx",
    "joiner.int8.onnx",
    "tokens.txt",
];

/// In-process Parakeet engine backed by the Rust sherpa-onnx binding.
///
/// The persisted engine id remains `parakeet-mlx` for compatibility with saved
/// settings and task history, but the runtime no longer invokes MLX, Python, or uv.
pub struct ParakeetNativeEngine {
    models_dir: PathBuf,
    vad_model_path: PathBuf,
}

impl ParakeetNativeEngine {
    pub fn new(models_dir: PathBuf, vad_model_path: PathBuf) -> Self {
        Self {
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
            .unwrap_or_else(|| self.models_dir.join(PARAKEET_MODEL_ID))
    }

    pub fn is_model_installed_at(model_dir: &Path) -> bool {
        REQUIRED_MODEL_FILES
            .iter()
            .all(|name| model_dir.join(name).is_file())
    }
}

#[async_trait]
impl AsrEngine for ParakeetNativeEngine {
    fn id(&self) -> &'static str {
        "parakeet-mlx"
    }

    fn capabilities(&self) -> AsrCapabilities {
        AsrCapabilities {
            supports_streaming: false,
            supported_languages: vec!["en".into(), "auto".into()],
            requires_model_download: true,
        }
    }

    async fn prepare(&self, model: &AsrModelRef) -> Result<()> {
        let model_dir = self.model_dir(model);
        if !Self::is_model_installed_at(&model_dir) {
            return Err(FinalSubError::Validation(format!(
                "Parakeet 原生模型尚未安装。请先在模型管理下载 {}（目录应包含 encoder.int8.onnx、decoder.int8.onnx、joiner.int8.onnx 和 tokens.txt）：{}",
                PARAKEET_MODEL_ID,
                model_dir.display()
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
        let language = job.language.as_deref().unwrap_or("auto");
        if !matches!(language, "auto" | "en" | "english") {
            return Err(FinalSubError::Validation(format!(
                "Parakeet v2 仅支持英文转录，当前语言：{language}"
            )));
        }
        if cancel_rx.as_ref().is_some_and(|rx| *rx.borrow()) {
            return Err(FinalSubError::Validation("任务已取消".into()));
        }

        let model_dir = self.model_dir(&job.model);
        if !Self::is_model_installed_at(&model_dir) {
            return Err(FinalSubError::Validation(format!(
                "Parakeet 模型文件不完整：{}",
                model_dir.display()
            )));
        }

        progress
            .send(ProgressUpdate {
                progress: 0.05,
                message: "正在加载 Parakeet 原生 ONNX 模型...".into(),
            })
            .await
            .ok();

        let audio_path = PathBuf::from(&job.audio_path);
        let track = super::parakeet_worker::transcribe_isolated(
            &model_dir,
            &self.vad_model_path,
            &audio_path,
            job.max_subtitle_chars,
            progress.clone(),
            cancel_rx,
        )
        .await?;

        progress
            .send(ProgressUpdate {
                progress: 0.98,
                message: "Parakeet 识别完成，正在生成时间轴...".into(),
            })
            .await
            .ok();

        if track.cues.is_empty() {
            return Err(FinalSubError::Validation(
                "Parakeet 未识别到字幕内容。该模型仅适用于英文语音；其他语言请切换 Whisper.cpp 或 SenseVoice。".into(),
            ));
        }

        progress
            .send(ProgressUpdate {
                progress: 1.0,
                message: format!("Parakeet 转录完成，共 {} 条字幕", track.cues.len()),
            })
            .await
            .ok();
        Ok(track)
    }
}

fn normalize_token(token: &str) -> String {
    token.replace('\u{2581}', " ")
}

fn normalize_spaces(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn ends_sentence(text: &str) -> bool {
    text.chars()
        .last()
        .is_some_and(|character| matches!(character, '.' | '?' | '!' | ';' | ':'))
}

fn push_cue(cues: &mut Vec<Cue>, text: &mut String, start_ms: u64, end_ms: u64) {
    let normalized = normalize_spaces(text);
    text.clear();
    if normalized.is_empty() {
        return;
    }
    let previous_end = cues.last().map(|cue| cue.end_ms).unwrap_or(0);
    let start_ms = start_ms.max(previous_end);
    let end_ms = end_ms.max(start_ms + 300);
    cues.push(Cue {
        index: (cues.len() + 1) as u32,
        start_ms,
        end_ms,
        text: normalized,
    });
}

pub(super) fn build_cues(
    full_text: &str,
    tokens: &[String],
    timestamps: Option<&[f32]>,
    duration_ms: u64,
    max_subtitle_chars: i32,
) -> Vec<Cue> {
    if let Some(timestamps) = timestamps {
        if !timestamps.is_empty() && timestamps.len() == tokens.len() {
            let mut cues = Vec::new();
            let mut text = String::new();
            let mut cue_start = 0;
            let mut previous_token_ms = 0;

            for (index, token) in tokens.iter().enumerate() {
                let token_content = token.trim();
                if token_content.is_empty()
                    || (token_content.starts_with('<') && token_content.ends_with('>'))
                {
                    continue;
                }
                let token_ms = (timestamps[index].max(0.0) * 1_000.0) as u64;
                let gap_ms = token_ms.saturating_sub(previous_token_ms);
                if !text.is_empty() && gap_ms >= 800 {
                    push_cue(&mut cues, &mut text, cue_start, token_ms);
                    cue_start = token_ms;
                } else if text.is_empty() {
                    cue_start = token_ms;
                }

                let normalized_token = normalize_token(token);
                if !normalize_spaces(&text).is_empty() {
                    let mut candidate = text.clone();
                    candidate.push_str(&normalized_token);
                    if crate::core::subtitle::exceeds_custom_subtitle_width(
                        &normalize_spaces(&candidate),
                        max_subtitle_chars,
                    ) {
                        push_cue(&mut cues, &mut text, cue_start, token_ms);
                        cue_start = token_ms;
                    }
                }
                text.push_str(&normalized_token);
                previous_token_ms = token_ms;
                let next_ms = timestamps
                    .get(index + 1)
                    .map(|next| (next.max(0.0) * 1_000.0) as u64)
                    .unwrap_or(duration_ms);
                let normalized_text = normalize_spaces(&text);
                let too_long = token_ms.saturating_sub(cue_start) >= 6_000
                    || crate::core::subtitle::should_break_for_width(
                        &normalized_text,
                        max_subtitle_chars,
                        normalized_text.chars().count() >= 84,
                    );
                if ends_sentence(token_content) || too_long {
                    push_cue(&mut cues, &mut text, cue_start, next_ms);
                }
            }
            if !text.trim().is_empty() {
                push_cue(&mut cues, &mut text, cue_start, duration_ms);
            }
            if !cues.is_empty() {
                return cues;
            }
        }
    }

    build_even_cues(full_text, duration_ms, max_subtitle_chars)
}

fn build_even_cues(full_text: &str, duration_ms: u64, max_subtitle_chars: i32) -> Vec<Cue> {
    let normalized = normalize_spaces(full_text);
    if normalized.is_empty() {
        return Vec::new();
    }
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
                current.chars().count() >= 84,
            )
        {
            blocks.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        blocks.push(current);
    }

    let total_chars = blocks
        .iter()
        .map(|block| block.chars().count())
        .sum::<usize>();
    let mut cursor = 0;
    blocks
        .into_iter()
        .enumerate()
        .map(|(index, text)| {
            let share = if total_chars == 0 {
                1_000
            } else {
                (duration_ms * text.chars().count() as u64 / total_chars as u64).max(500)
            };
            let start_ms = cursor;
            let end_ms = (cursor + share).min(duration_ms.max(cursor + 500));
            cursor = end_ms;
            Cue {
                index: (index + 1) as u32,
                start_ms,
                end_ms,
                text,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model_ref() -> AsrModelRef {
        AsrModelRef {
            engine_id: "parakeet-mlx".into(),
            model_id: PARAKEET_MODEL_ID.into(),
            model_path: None,
        }
    }

    #[test]
    fn requires_all_native_model_files() {
        let temp = tempfile::tempdir().unwrap();
        let model_dir = temp.path().join(PARAKEET_MODEL_ID);
        std::fs::create_dir_all(&model_dir).unwrap();
        for name in REQUIRED_MODEL_FILES {
            std::fs::write(model_dir.join(name), b"test").unwrap();
        }
        let engine = ParakeetNativeEngine::new(
            temp.path().to_path_buf(),
            PathBuf::from("/vad/silero_vad.onnx"),
        );
        assert!(ParakeetNativeEngine::is_model_installed_at(
            &engine.model_dir(&model_ref())
        ));
    }

    #[test]
    fn capabilities_are_native_downloaded_and_english_only() {
        let engine = ParakeetNativeEngine::new(
            PathBuf::from("/models"),
            PathBuf::from("/vad/silero_vad.onnx"),
        );
        let capabilities = engine.capabilities();
        assert!(capabilities.requires_model_download);
        assert_eq!(capabilities.supported_languages, vec!["en", "auto"]);
    }

    #[test]
    fn token_timestamps_create_non_overlapping_sentence_cues() {
        let tokens = vec![
            "\u{2581}Hello".into(),
            "\u{2581}world.".into(),
            "\u{2581}Next".into(),
            "\u{2581}line!".into(),
        ];
        let timestamps = [0.0, 0.4, 1.2, 1.6];
        let cues = build_cues(
            "Hello world. Next line!",
            &tokens,
            Some(&timestamps),
            2_200,
            0,
        );
        assert_eq!(cues.len(), 2);
        assert_eq!(cues[0].text, "Hello world.");
        assert_eq!(cues[1].text, "Next line!");
        assert!(cues[0].end_ms <= cues[1].start_ms);
    }

    #[test]
    fn sherpa_decoded_leading_spaces_preserve_english_word_boundaries() {
        let tokens = vec![
            " I".into(),
            " belie".into(),
            "ve".into(),
            " the".into(),
            " ro".into(),
            "le".into(),
            ".".into(),
        ];
        let timestamps = [0.0, 0.2, 0.4, 0.7, 0.9, 1.1, 1.3];
        let cues = build_cues("I believe the role.", &tokens, Some(&timestamps), 1_600, -1);

        assert_eq!(cues.len(), 1);
        assert_eq!(cues[0].text, "I believe the role.");
    }

    #[test]
    fn text_fallback_still_produces_subtitles() {
        let cues = build_cues("One sentence. Another sentence!", &[], None, 4_000, 0);
        assert_eq!(cues.len(), 2);
        assert_eq!(cues[0].index, 1);
        assert!(cues.iter().all(|cue| cue.end_ms > cue.start_ms));
    }

    #[test]
    fn custom_width_uses_token_timestamps_and_unlimited_disables_width_cut() {
        let tokens = vec![
            "\u{2581}Hello".into(),
            "\u{2581}world".into(),
            "\u{2581}again".into(),
        ];
        let timestamps = [0.0, 0.4, 0.8];
        let custom = build_cues("Hello world again", &tokens, Some(&timestamps), 1_600, 8);
        assert_eq!(custom.len(), 3);
        assert_eq!(custom[0].text, "Hello");
        assert_eq!(custom[0].end_ms, 400);
        assert!(custom
            .iter()
            .all(|cue| crate::core::subtitle::subtitle_visual_width(&cue.text) <= 8));

        let unlimited = build_cues("Hello world again", &tokens, Some(&timestamps), 1_600, -1);
        assert_eq!(unlimited.len(), 1);
    }
}
