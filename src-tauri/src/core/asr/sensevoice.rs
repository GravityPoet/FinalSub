use async_trait::async_trait;
use std::path::PathBuf;
use super::{AsrCapabilities, AsrEngine, AsrModelRef, ProgressSink, ProgressUpdate, TranscribeJob};
use crate::core::subtitle::{SubtitleTrack, Cue};
use crate::error::{FinalSubError, Result};

pub struct SenseVoiceEngine {
    models_dir: PathBuf,
}

impl SenseVoiceEngine {
    pub fn new(models_dir: PathBuf) -> Self {
        Self { models_dir }
    }

    fn model_dir(&self) -> PathBuf {
        self.models_dir.join("sensevoice-small")
    }

    fn is_model_installed(&self) -> bool {
        let dir = self.model_dir();
        dir.join("model.onnx").exists() && dir.join("tokens.txt").exists()
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
) -> Vec<Cue> {
    if let Some(ts) = timestamps {
        if !ts.is_empty() && ts.len() == tokens.len() {
            let cues = build_cues_from_tokens(tokens, ts, duration_ms);
            if !cues.is_empty() {
                return cues;
            }
        }
    }
    build_cues_even(raw_text, duration_ms)
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
fn build_cues_from_tokens(tokens: &[String], timestamps: &[f32], duration_ms: u64) -> Vec<Cue> {
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
        if cur_start.is_none() {
            cur_start = Some(start_ms);
        }
        cur.push_str(&normalize_token(t));

        if ends_sentence(t) || cur.chars().filter(|c| !c.is_whitespace()).count() >= 28 {
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

    async fn prepare(&self, _model: &AsrModelRef) -> Result<()> {
        if !self.is_model_installed() {
            return Err(FinalSubError::Validation(
                "SenseVoice 模型未安装。请在 models 目录下放置 sensevoice-small (包含 model.onnx 和 tokens.txt)。".into()
            ));
        }
        Ok(())
    }

    async fn transcribe(
        &self,
        job: TranscribeJob,
        progress: ProgressSink,
        cancel_rx: Option<tokio::sync::watch::Receiver<bool>>,
    ) -> Result<SubtitleTrack> {
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

        progress.send(ProgressUpdate {
            progress: 0.2,
            message: "正在初始化 SenseVoice 引擎...".into(),
        }).await.ok();

        type DecodeResult = std::result::Result<(String, Vec<String>, Option<Vec<f32>>, u64), String>;
        let handle = tokio::task::spawn_blocking(move || -> DecodeResult {
            let mut recognizer_config = sherpa_onnx::OfflineRecognizerConfig::default();

            let mut sense_voice_config = sherpa_onnx::OfflineSenseVoiceModelConfig::default();
            sense_voice_config.model = Some(model_path.to_string_lossy().to_string());
            sense_voice_config.language = Some(job.language.clone().unwrap_or_else(|| "auto".to_string()));
            sense_voice_config.use_itn = true;

            recognizer_config.model_config.sense_voice = sense_voice_config;
            recognizer_config.model_config.tokens = Some(tokens_path.to_string_lossy().to_string());
            recognizer_config.model_config.num_threads = 2;
            recognizer_config.model_config.provider = Some("cpu".to_string());

            let recognizer = sherpa_onnx::OfflineRecognizer::create(&recognizer_config)
                .ok_or_else(|| "创建 SenseVoice 识别器失败".to_string())?;

            let wave = sherpa_onnx::Wave::read(&audio_path)
                .ok_or_else(|| "读取音频文件失败".to_string())?;

            let stream = recognizer.create_stream();
            stream.accept_waveform(wave.sample_rate() as i32, wave.samples());
            recognizer.decode_multiple_streams(&[&stream]);

            let res = stream.get_result().ok_or_else(|| "识别结果为空".to_string())?;
            let sample_rate = wave.sample_rate().max(1) as u64;
            let duration = (wave.samples().len() as u64 * 1000) / sample_rate;

            Ok((res.text, res.tokens, res.timestamps, duration))
        });

        // sherpa 的 decode 是同步阻塞调用，无法像子进程那样中途 kill。
        // 取消只让此处的等待提前返回；后台 blocking 线程会跑完后自然结束
        // （SenseVoice 单次离线解码通常很快）。
        let (raw_text, tokens, timestamps, duration_ms) = match cancel_rx {
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

        progress.send(ProgressUpdate {
            progress: 0.9,
            message: "SenseVoice 识别完成，正在格式化字幕...".into(),
        }).await.ok();

        let cues = build_cues(&raw_text, &tokens, timestamps.as_deref(), duration_ms);
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
        assert_eq!(sentences, vec!["你好！", "今天天气真好。", "我们要去公园吗？"]);
        
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
        let cues = build_cues_from_tokens(&tokens, &timestamps, 5000);
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
        let cues = build_cues("<|zh|>你好。再见。", &[], None, 4000);
        assert_eq!(cues.len(), 2);
        assert!(cues.iter().all(|c| c.end_ms > c.start_ms));
        assert_eq!(cues[0].text, "你好。");
    }

    #[test]
    fn normalize_token_converts_word_boundary() {
        assert_eq!(normalize_token("\u{2581}hello"), " hello");
    }
}

