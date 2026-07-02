use async_trait::async_trait;
use std::path::PathBuf;
use std::process::Stdio;
use tokio::io::AsyncBufReadExt;

use super::{AsrCapabilities, AsrEngine, AsrModelRef, ProgressSink, ProgressUpdate, TranscribeJob};
use crate::core::subtitle::SubtitleTrack;
use crate::error::{FinalSubError, Result};

#[derive(Debug, Clone)]
pub struct WhisperOptions {
    pub use_vad: bool,
    pub vad_threshold: f64,
    pub vad_min_speech_duration_ms: u32,
    pub vad_min_silence_duration_ms: u32,
    pub vad_max_speech_duration_s: u32,
    pub vad_speech_pad_ms: u32,
    pub vad_samples_overlap: f64,
    pub whisper_command: String,
    pub max_context: i32,
    pub vad_model_path: Option<PathBuf>,
}

impl Default for WhisperOptions {
    fn default() -> Self {
        Self {
            use_vad: false,
            vad_threshold: 0.5,
            vad_min_speech_duration_ms: 250,
            vad_min_silence_duration_ms: 100,
            vad_max_speech_duration_s: 0,
            vad_speech_pad_ms: 0,
            vad_samples_overlap: 0.1,
            whisper_command: String::new(),
            max_context: -1,
            vad_model_path: None,
        }
    }
}

pub struct WhisperCppEngine {
    whisper_bin: PathBuf,
    models_dir: PathBuf,
    options: WhisperOptions,
}

impl WhisperCppEngine {
    pub fn new(whisper_bin: PathBuf, models_dir: PathBuf, options: WhisperOptions) -> Self {
        Self {
            whisper_bin,
            models_dir,
            options,
        }
    }

    fn model_path(&self, model_id: &str) -> PathBuf {
        self.models_dir.join(format!("ggml-{model_id}.bin"))
    }

    fn is_model_downloaded(&self, model_id: &str) -> bool {
        self.model_path(model_id).exists()
    }
}

fn parse_whisper_progress_ratio(line: &str) -> Option<f32> {
    let (_, value) = line.split_once("progress =")?;
    let raw_percent = value.trim().trim_end_matches('%').trim();
    let percent = raw_percent.parse::<f32>().ok()?;
    Some((percent / 100.0).clamp(0.0, 1.0))
}

#[async_trait]
impl AsrEngine for WhisperCppEngine {
    fn id(&self) -> &'static str {
        "whisper-cpp"
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
                "fr".into(),
                "de".into(),
                "es".into(),
                "ru".into(),
                "pt".into(),
                "it".into(),
                "nl".into(),
                "pl".into(),
                "tr".into(),
                "ar".into(),
                "vi".into(),
                "th".into(),
                "id".into(),
                "ms".into(),
                "hi".into(),
            ],
            requires_model_download: true,
        }
    }

    async fn prepare(&self, model: &AsrModelRef) -> Result<()> {
        let model_id = &model.model_id;
        if !self.is_model_downloaded(model_id) {
            return Err(FinalSubError::Validation(format!(
                "模型未下载：{model_id}。请先在模型管理页下载。"
            )));
        }
        if !self.whisper_bin.exists() {
            return Err(FinalSubError::Validation(
                "whisper-cli 未找到。请安装 whisper.cpp。".into(),
            ));
        }
        Ok(())
    }

    async fn transcribe(
        &self,
        job: TranscribeJob,
        progress: ProgressSink,
        mut cancel_rx: Option<tokio::sync::watch::Receiver<bool>>,
    ) -> Result<SubtitleTrack> {
        let model_path = self.model_path(&job.model.model_id);
        if !model_path.exists() {
            return Err(FinalSubError::Validation(format!(
                "模型文件不存在：{}",
                model_path.display()
            )));
        }

        progress
            .send(ProgressUpdate {
                progress: 0.01,
                message: "正在启动 whisper-cli...".into(),
            })
            .await
            .ok();

        let output_prefix = job
            .output_path
            .strip_suffix(".srt")
            .unwrap_or(&job.output_path)
            .to_string();

        let mut args = vec![
            "-m".to_string(),
            model_path.to_string_lossy().to_string(),
            "-f".to_string(),
            job.audio_path.clone(),
            "-osrt".to_string(),
            "-of".to_string(),
            output_prefix.clone(),
            "-pp".to_string(),
        ];

        if let Some(ref lang) = job.language {
            if lang != "auto" {
                args.push("-l".to_string());
                args.push(lang.clone());
            }
        }

        if self.options.use_vad {
            if let Some(ref vad_model) = self.options.vad_model_path {
                args.push("--vad".to_string());
                args.push("-vm".to_string());
                args.push(vad_model.to_string_lossy().to_string());

                args.push("-vt".to_string());
                args.push(self.options.vad_threshold.to_string());

                args.push("-vspd".to_string());
                args.push(self.options.vad_min_speech_duration_ms.to_string());

                args.push("-vsd".to_string());
                args.push(self.options.vad_min_silence_duration_ms.to_string());

                if self.options.vad_max_speech_duration_s > 0 {
                    args.push("-vmsd".to_string());
                    args.push(self.options.vad_max_speech_duration_s.to_string());
                }

                args.push("-vp".to_string());
                args.push(self.options.vad_speech_pad_ms.to_string());

                args.push("-vo".to_string());
                args.push(self.options.vad_samples_overlap.to_string());
            }
        }

        if self.options.max_context != -1 {
            args.push("-mc".to_string());
            args.push(self.options.max_context.to_string());
        }

        let mut bin_path = self.whisper_bin.clone();
        if !self.options.whisper_command.trim().is_empty() {
            let custom_path = PathBuf::from(&self.options.whisper_command);
            if custom_path.exists() {
                bin_path = custom_path;
                progress
                    .send(ProgressUpdate {
                        progress: 0.02,
                        message: format!("正在启动自定义 whisper-cli：{}...", bin_path.display()),
                    })
                    .await
                    .ok();
            } else {
                progress
                    .send(ProgressUpdate {
                        progress: 0.02,
                        message: format!(
                            "自定义 whisper-cli 路径不存在：{}，回退到默认路径",
                            self.options.whisper_command
                        ),
                    })
                    .await
                    .ok();
            }
        } else {
            progress
                .send(ProgressUpdate {
                    progress: 0.02,
                    message: "正在转录...".into(),
                })
                .await
                .ok();
        }

        let mut cmd = tokio::process::Command::new(&bin_path);
        cmd.args(&args);
        cmd.stdout(Stdio::null());
        cmd.stderr(Stdio::piped());
        cmd.kill_on_drop(true);

        let mut child = cmd
            .spawn()
            .map_err(|e| FinalSubError::Validation(format!("运行 whisper-cli 失败：{e}")))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| FinalSubError::Validation("无法读取 whisper-cli 进度输出".into()))?;
        let mut stderr_lines = tokio::io::BufReader::new(stderr).lines();
        let mut stderr_buf = String::new();
        let mut stderr_done = false;
        let wait_fut = child.wait();
        tokio::pin!(wait_fut);

        let status = loop {
            tokio::select! {
                line_res = stderr_lines.next_line(), if !stderr_done => {
                    match line_res {
                        Ok(Some(line)) => {
                            if let Some(ratio) = parse_whisper_progress_ratio(&line) {
                                progress
                                    .send(ProgressUpdate {
                                        progress: 0.05 + 0.90 * ratio,
                                        message: format!("正在转录... {:.0}%", ratio * 100.0),
                                    })
                                    .await
                                    .ok();
                            }
                            stderr_buf.push_str(&line);
                            stderr_buf.push('\n');
                        }
                        Ok(None) => {
                            stderr_done = true;
                        }
                        Err(e) => {
                            return Err(FinalSubError::Validation(format!(
                                "读取 whisper-cli 进度输出失败：{e}"
                            )));
                        }
                    }
                }
                status_res = &mut wait_fut => {
                    break status_res
                        .map_err(|e| FinalSubError::Validation(format!("等待 whisper-cli 结束失败：{e}")))?;
                }
                change_res = async {
                    let rx = cancel_rx.as_mut().expect("cancel receiver exists");
                    rx.changed().await.map(|_| *rx.borrow()).unwrap_or(true)
                }, if cancel_rx.is_some() => {
                    if change_res {
                        return Err(FinalSubError::Validation("任务已取消".into()));
                    }
                }
            }
        };

        if !status.success() {
            return Err(FinalSubError::Validation(format!(
                "whisper-cli 转录失败：{stderr_buf}"
            )));
        }

        progress
            .send(ProgressUpdate {
                progress: 0.98,
                message: "正在解析字幕...".into(),
            })
            .await
            .ok();

        let srt_path = format!("{output_prefix}.srt");
        let srt_content = tokio::fs::read_to_string(&srt_path)
            .await
            .map_err(|e| FinalSubError::Validation(format!("读取 SRT 输出失败：{e}")))?;

        if srt_content.trim().is_empty() {
            return Err(FinalSubError::Validation(
                "Whisper 未识别到任何字幕内容，请确认音频中有人声，或尝试切换语言/模型。".into(),
            ));
        }

        let track = SubtitleTrack::from_srt(&srt_content)?;

        progress
            .send(ProgressUpdate {
                progress: 1.0,
                message: format!("转录完成，共 {} 条字幕", track.len()),
            })
            .await
            .ok();

        Ok(track)
    }
}

pub fn available_models() -> Vec<(&'static str, &'static str, &'static str)> {
    vec![
        ("large-v3-turbo", "Large V3 Turbo", "1500MB"),
        ("large-v3", "Large V3", "3100MB"),
        ("medium", "Medium", "1500MB"),
        ("small", "Small", "500MB"),
        ("base", "Base", "150MB"),
        ("tiny", "Tiny", "75MB"),
    ]
}

pub fn download_url(model_id: &str, source: &str) -> String {
    let base = match source {
        "hf-mirror" => "https://hf-mirror.com/ggerganov/whisper.cpp/resolve/main",
        _ => "https://huggingface.co/ggerganov/whisper.cpp/resolve/main",
    };
    format!("{base}/ggml-{model_id}.bin")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_path_generation() {
        let engine = WhisperCppEngine::new(
            PathBuf::from("/usr/bin/whisper-cli"),
            PathBuf::from("/models"),
            WhisperOptions::default(),
        );
        assert_eq!(
            engine.model_path("large-v3-turbo"),
            PathBuf::from("/models/ggml-large-v3-turbo.bin")
        );
    }

    #[test]
    fn available_models_count() {
        assert_eq!(available_models().len(), 6);
    }

    #[test]
    fn download_url_hf() {
        let url = download_url("large-v3-turbo", "huggingface");
        assert!(url.contains("huggingface.co"));
        assert!(url.contains("ggml-large-v3-turbo.bin"));
    }

    #[test]
    fn download_url_mirror() {
        let url = download_url("small", "hf-mirror");
        assert!(url.contains("hf-mirror.com"));
    }

    #[test]
    fn capabilities_not_streaming() {
        let engine = WhisperCppEngine::new(
            PathBuf::from("/usr/bin/whisper-cli"),
            PathBuf::from("/models"),
            WhisperOptions::default(),
        );
        assert!(!engine.capabilities().supports_streaming);
        assert!(engine.capabilities().requires_model_download);
    }

    #[test]
    fn parse_whisper_progress_callback() {
        assert_eq!(
            parse_whisper_progress_ratio("whisper_print_progress_callback: progress =  61%"),
            Some(0.61)
        );
        assert_eq!(
            parse_whisper_progress_ratio("whisper_print_progress_callback: progress = 100%"),
            Some(1.0)
        );
        assert_eq!(parse_whisper_progress_ratio("not a progress line"), None);
    }
}
