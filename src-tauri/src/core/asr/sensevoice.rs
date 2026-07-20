use super::vad::{detect_speech, SAMPLE_RATE};
use super::{AsrCapabilities, AsrEngine, AsrModelRef, ProgressSink, ProgressUpdate, TranscribeJob};
use crate::core::subtitle::{Cue, SubtitleTrack};
use crate::error::{FinalSubError, Result};
use async_trait::async_trait;
use std::path::{Path, PathBuf};

pub const SENSEVOICE_MODEL_ID: &str = "sensevoice-small";
pub const SENSEVOICE_ARCHIVE_DIR: &str = "sherpa-onnx-sense-voice-zh-en-ja-ko-yue-int8-2025-09-09";
const MAX_SEGMENT_SECONDS: usize = 30;

pub struct SenseVoiceEngine {
    models_dir: PathBuf,
    vad_model_path: PathBuf,
}

impl SenseVoiceEngine {
    pub fn new(models_dir: PathBuf, vad_model_path: PathBuf) -> Self {
        Self {
            models_dir,
            vad_model_path,
        }
    }

    fn model_dir(&self) -> PathBuf {
        self.models_dir.join(SENSEVOICE_MODEL_ID)
    }

    fn is_model_installed(&self) -> bool {
        Self::is_model_installed_at(&self.model_dir())
    }

    pub fn is_model_installed_at(dir: &Path) -> bool {
        dir.join("model.onnx").is_file() && dir.join("tokens.txt").is_file()
    }
}

fn clean_sensevoice_text(text: &str) -> String {
    let mut cleaned = String::new();
    let mut in_tag = false;
    for c in text.chars() {
        if c == '<' {
            in_tag = true;
        } else if c == '>' && in_tag {
            in_tag = false;
        } else if !in_tag {
            cleaned.push(c);
        }
    }
    cleaned.trim().to_string()
}

fn split_sentences(text: &str) -> Vec<String> {
    let mut sentences = Vec::new();
    let mut current = String::new();
    for c in text.chars() {
        current.push(c);
        if c == '。' || c == '？' || c == '！' || c == '；' || c == '\n' {
            let s = current.trim();
            if !s.is_empty() {
                sentences.push(s.to_string());
            }
            current.clear();
        }
    }
    let s = current.trim();
    if !s.is_empty() {
        sentences.push(s.to_string());
    }
    sentences
}

/// 优先用 sherpa 返回的 token 级时间戳重建字幕；拿不到（None 或长度不匹配）
/// 时回退到基于字数均摊总时长的估算。
fn build_cues(
    raw_text: &str,
    tokens: &[String],
    timestamps: Option<&[f32]>,
    duration_ms: u64,
    max_subtitle_chars: i32,
) -> Vec<Cue> {
    if let Some(ts) = timestamps {
        if !ts.is_empty() && ts.len() == tokens.len() {
            let cues = build_cues_from_tokens(tokens, ts, duration_ms, max_subtitle_chars);
            if !cues.is_empty() {
                return cues;
            }
        }
    }
    build_cues_even(raw_text, duration_ms)
}

fn build_segment_cues(
    raw_text: &str,
    tokens: &[String],
    timestamps: Option<&[f32]>,
    segment_start_ms: u64,
    segment_end_ms: u64,
    max_subtitle_chars: i32,
) -> Vec<Cue> {
    if segment_end_ms <= segment_start_ms {
        return Vec::new();
    }
    let duration_ms = segment_end_ms - segment_start_ms;
    let mut cues = build_cues(
        raw_text,
        tokens,
        timestamps,
        duration_ms,
        max_subtitle_chars,
    );
    for cue in &mut cues {
        let start = segment_start_ms.saturating_add(cue.start_ms);
        let end = segment_start_ms.saturating_add(cue.end_ms);
        cue.start_ms = start.min(segment_end_ms - 1);
        cue.end_ms = end.min(segment_end_ms).max(cue.start_ms + 1);
    }
    cues
}

/// SenseVoice token 用 sentencepiece 风格的 `▁` 表词边界；中文为单字 token。
fn normalize_token(token: &str) -> String {
    token.replace('\u{2581}', " ")
}

fn ends_sentence(token: &str) -> bool {
    token
        .chars()
        .last()
        .map(|c| matches!(c, '。' | '？' | '！' | '；' | '.' | '?' | '!' | ';'))
        .unwrap_or(false)
}

/// 基于真实 token 时间戳切分字幕：遇句末标点或累计到约 28 字时断句。
fn build_cues_from_tokens(
    tokens: &[String],
    timestamps: &[f32],
    duration_ms: u64,
    max_subtitle_chars: i32,
) -> Vec<Cue> {
    let mut cues = Vec::new();
    let mut cur = String::new();
    let mut cur_start: Option<u64> = None;

    for (i, tok) in tokens.iter().enumerate() {
        let t = tok.trim();
        // 跳过 <|zh|> / <|EMO_HAPPY|> 等元信息 token 与空 token。
        if t.is_empty() || (t.starts_with("<|") && t.ends_with("|>")) {
            continue;
        }
        let start_ms = (timestamps[i].max(0.0) * 1000.0) as u64;
        let normalized_token = normalize_token(t);
        if !cur.trim().is_empty() {
            let mut candidate = cur.clone();
            candidate.push_str(&normalized_token);
            if crate::core::subtitle::exceeds_custom_subtitle_width(
                candidate.trim(),
                max_subtitle_chars,
            ) {
                let start = cur_start.unwrap_or(start_ms);
                let end = start_ms.max(start + 1);
                cues.push(Cue {
                    index: (cues.len() + 1) as u32,
                    start_ms: start,
                    end_ms: end,
                    text: cur.trim().to_string(),
                });
                cur.clear();
                cur_start = Some(end);
            }
        }
        if cur_start.is_none() {
            cur_start = Some(start_ms);
        }
        cur.push_str(&normalized_token);

        if ends_sentence(t)
            || crate::core::subtitle::should_break_for_width(
                cur.trim(),
                max_subtitle_chars,
                cur.chars()
                    .filter(|character| !character.is_whitespace())
                    .count()
                    >= 28,
            )
        {
            let text = cur.trim().to_string();
            if !text.is_empty() {
                let start = cur_start.unwrap_or(start_ms);
                let end = timestamps
                    .get(i + 1)
                    .map(|n| ((*n).max(0.0) * 1000.0) as u64)
                    .unwrap_or(duration_ms)
                    .max(start + 500);
                cues.push(Cue {
                    index: (cues.len() + 1) as u32,
                    start_ms: start,
                    end_ms: end,
                    text,
                });
            }
            cur.clear();
            cur_start = None;
        }
    }

    let tail = cur.trim();
    if !tail.is_empty() {
        let start = cur_start.unwrap_or(0);
        let end = duration_ms.max(start + 500);
        cues.push(Cue {
            index: (cues.len() + 1) as u32,
            start_ms: start,
            end_ms: end,
            text: tail.to_string(),
        });
    }
    cues
}

/// 无时间戳时的降级：清洗 tag、分句、按字数均摊总时长。
fn build_cues_even(raw_text: &str, duration_ms: u64) -> Vec<Cue> {
    let cleaned = clean_sensevoice_text(raw_text);
    let sentences = split_sentences(&cleaned);
    let total_chars: usize = sentences.iter().map(|s| s.chars().count()).sum();

    let mut cues = Vec::new();
    let mut current_ms = 0u64;
    for (i, sentence) in sentences.into_iter().enumerate() {
        let char_count = sentence.chars().count();
        let raw_duration = if total_chars > 0 {
            (char_count as u64 * duration_ms) / total_chars as u64
        } else {
            0
        };
        // 钳制到 1~8 秒，且不超过剩余时长，但至少 1 秒以保证 end > start。
        let remaining = duration_ms.saturating_sub(current_ms).max(1000);
        let duration = raw_duration.clamp(1000, 8000).min(remaining);
        let end_ms = current_ms + duration;
        cues.push(Cue {
            index: (i + 1) as u32,
            start_ms: current_ms,
            end_ms,
            text: sentence,
        });
        current_ms = end_ms;
    }
    cues
}

#[async_trait]
impl AsrEngine for SenseVoiceEngine {
    fn id(&self) -> &'static str {
        "sensevoice"
    }

    fn capabilities(&self) -> AsrCapabilities {
        AsrCapabilities {
            supports_streaming: false,
            supported_languages: vec![
                "auto".into(),
                "zh".into(),
                "en".into(),
                "ja".into(),
                "ko".into(),
                "yue".into(),
            ],
            requires_model_download: true,
        }
    }

    async fn prepare(&self, model: &AsrModelRef) -> Result<()> {
        if model.engine_id != self.id() || model.model_id != SENSEVOICE_MODEL_ID {
            return Err(FinalSubError::Validation(format!(
                "SenseVoice 模型引用不匹配：期望 sensevoice/{SENSEVOICE_MODEL_ID}"
            )));
        }
        if !self.is_model_installed() {
            return Err(FinalSubError::Validation(
                "SenseVoice 模型未安装。请先在模型管理中应用内下载，或导入 model.onnx 与 tokens.txt。".into()
            ));
        }
        if !self.vad_model_path.is_file() {
            return Err(FinalSubError::Validation(format!(
                "SenseVoice 的内置 Silero VAD 资源缺失：{}",
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
        self.prepare(&job.model).await?;
        // 解码前先看一眼取消信号，避免白启动识别器。
        if let Some(rx) = &cancel_rx {
            if *rx.borrow() {
                return Err(FinalSubError::Validation("任务已取消".into()));
            }
        }

        let model_dir = self.model_dir();
        let model_path = model_dir.join("model.onnx");
        let tokens_path = model_dir.join("tokens.txt");
        let audio_path = job.audio_path.clone();
        let vad_model_path = self.vad_model_path.clone();
        let language = job.language.clone().unwrap_or_else(|| "auto".to_string());
        let max_subtitle_chars = job.max_subtitle_chars;
        let worker_progress = progress.clone();
        let worker_cancel = cancel_rx.clone();

        progress
            .send(ProgressUpdate {
                progress: 0.05,
                message: "正在加载 SenseVoice 与 Silero VAD...".into(),
            })
            .await
            .ok();

        let handle =
            tokio::task::spawn_blocking(move || -> std::result::Result<Vec<Cue>, String> {
                let wave = sherpa_onnx::Wave::read(&audio_path)
                    .ok_or_else(|| "读取音频文件失败".to_string())?;
                if wave.sample_rate() != SAMPLE_RATE {
                    return Err(format!(
                        "SenseVoice 需要 16 kHz 单声道 WAV，当前采样率为 {} Hz",
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

                let mut recognizer_config = sherpa_onnx::OfflineRecognizerConfig::default();

                let sense_voice_config = sherpa_onnx::OfflineSenseVoiceModelConfig {
                    model: Some(model_path.to_string_lossy().to_string()),
                    language: Some(language),
                    use_itn: true,
                };

                recognizer_config.model_config.sense_voice = sense_voice_config;
                recognizer_config.model_config.tokens =
                    Some(tokens_path.to_string_lossy().to_string());
                recognizer_config.model_config.num_threads = 2;
                recognizer_config.model_config.provider = Some("cpu".to_string());

                let recognizer = sherpa_onnx::OfflineRecognizer::create(&recognizer_config)
                    .ok_or_else(|| "创建 SenseVoice 识别器失败".to_string())?;

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
                        format!("SenseVoice 未返回第 {} 段识别结果", segment_index + 1)
                    })?;
                    let segment_start_ms = segment.start_sample as u64 * 1_000 / SAMPLE_RATE as u64;
                    let segment_end_ms = (segment.start_sample + segment.samples.len()) as u64
                        * 1_000
                        / SAMPLE_RATE as u64;
                    let mut segment_cues = build_segment_cues(
                        &result.text,
                        &result.tokens,
                        result.timestamps.as_deref(),
                        segment_start_ms,
                        segment_end_ms,
                        max_subtitle_chars,
                    );
                    cues.append(&mut segment_cues);
                    let fraction = (segment_index + 1) as f32 / total as f32;
                    worker_progress
                        .blocking_send(ProgressUpdate {
                            progress: 0.12 + fraction * 0.84,
                            message: format!(
                                "SenseVoice 正在识别语音片段 {}/{}...",
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

        // sherpa 的 decode 是同步阻塞调用，无法像子进程那样中途 kill。
        // 取消只让此处的等待提前返回；后台 blocking 线程会跑完后自然结束
        // （SenseVoice 单次离线解码通常很快）。
        let cues = match cancel_rx {
            Some(mut rx) => {
                tokio::pin!(handle);
                loop {
                    tokio::select! {
                        res = &mut handle => {
                            break res
                                .map_err(|e| FinalSubError::Validation(format!("线程池异常: {e}")))?
                                .map_err(FinalSubError::Validation)?;
                        }
                        changed = rx.changed() => {
                            if changed.is_err() || *rx.borrow() {
                                return Err(FinalSubError::Validation("任务已取消".into()));
                            }
                        }
                    }
                }
            }
            None => handle
                .await
                .map_err(|e| FinalSubError::Validation(format!("线程池异常: {e}")))?
                .map_err(FinalSubError::Validation)?,
        };

        if cues.is_empty() {
            return Err(FinalSubError::Validation(
                "SenseVoice 未识别到任何字幕内容，请确认音频中有人声，或尝试切换语言/模型。".into(),
            ));
        }
        progress
            .send(ProgressUpdate {
                progress: 1.0,
                message: format!("SenseVoice 转录完成，共 {} 条字幕", cues.len()),
            })
            .await
            .ok();
        Ok(SubtitleTrack { cues })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_sensevoice_text() {
        let text = "<|zh|>你好！<|EMO_HAPPY|>今天天气真好。";
        assert_eq!(clean_sensevoice_text(text), "你好！今天天气真好。");

        let text_no_tag = "普通文本无标签";
        assert_eq!(clean_sensevoice_text(text_no_tag), "普通文本无标签");
    }

    #[test]
    fn test_split_sentences() {
        let text = "你好！今天天气真好。我们要去公园吗？";
        let sentences = split_sentences(text);
        assert_eq!(
            sentences,
            vec!["你好！", "今天天气真好。", "我们要去公园吗？"]
        );

        let text_newline = "第一行\n第二行！";
        let sentences_nl = split_sentences(text_newline);
        assert_eq!(sentences_nl, vec!["第一行", "第二行！"]);
    }

    #[test]
    fn build_cues_uses_real_timestamps() {
        // <|zh|> 等 tag token 应被跳过；时间轴取自真实戳而非均摊。
        let tokens: Vec<String> = ["<|zh|>", "你", "好", "。", "再", "见", "。"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let timestamps = vec![0.0_f32, 0.5, 0.8, 1.0, 2.0, 2.3, 2.6];
        let cues = build_cues_from_tokens(&tokens, &timestamps, 5000, 0);
        assert_eq!(cues.len(), 2);
        assert_eq!(cues[0].text, "你好。");
        assert_eq!(cues[0].start_ms, 500); // 首个真实 token "你" 的戳
        assert_eq!(cues[0].end_ms, 2000); // 下一句首 token "再" 的戳
        assert_eq!(cues[1].text, "再见。");
        assert!(cues[1].end_ms > cues[1].start_ms);
    }

    #[test]
    fn build_cues_falls_back_without_timestamps() {
        // 长度不匹配 → 走降级均摊路径，仍产出合法 cue。
        let cues = build_cues("<|zh|>你好。再见。", &[], None, 4000, 0);
        assert_eq!(cues.len(), 2);
        assert!(cues.iter().all(|c| c.end_ms > c.start_ms));
        assert_eq!(cues[0].text, "你好。");
    }

    #[test]
    fn segmented_cues_preserve_vad_offset_and_clamp_to_segment() {
        let tokens = vec!["你".into(), "好".into(), "。".into()];
        let timestamps = [0.2, 0.5, 1.0];
        let cues = build_segment_cues("你好。", &tokens, Some(&timestamps), 12_000, 14_000, 0);
        assert_eq!(cues.len(), 1);
        assert_eq!(cues[0].start_ms, 12_200);
        assert_eq!(cues[0].end_ms, 14_000);
        assert_eq!(cues[0].text, "你好。");
    }

    #[test]
    fn normalize_token_converts_word_boundary() {
        assert_eq!(normalize_token("\u{2581}hello"), " hello");
    }

    #[test]
    fn smart_width_keeps_original_non_whitespace_threshold() {
        let tokens = (0..20).map(|_| "▁a".to_string()).collect::<Vec<_>>();
        let timestamps = (0..20).map(|index| index as f32 * 0.2).collect::<Vec<_>>();
        let cues = build_cues_from_tokens(&tokens, &timestamps, 4_200, 0);
        assert_eq!(cues.len(), 1);
    }

    #[test]
    fn custom_width_splits_on_real_token_time_and_unlimited_keeps_run() {
        let tokens = ["你", "好", "世", "界", "再", "见"]
            .into_iter()
            .map(str::to_string)
            .collect::<Vec<_>>();
        let timestamps = [0.0, 0.2, 0.4, 0.6, 0.8, 1.0];
        let custom = build_cues_from_tokens(&tokens, &timestamps, 1_400, 8);
        assert_eq!(custom.len(), 2);
        assert_eq!(custom[0].text, "你好世界");
        assert_eq!(custom[0].end_ms, 800);

        let unlimited = build_cues_from_tokens(&tokens, &timestamps, 1_400, -1);
        assert_eq!(unlimited.len(), 1);
    }
}
