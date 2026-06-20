use async_trait::async_trait;
use std::path::PathBuf;

use super::{AsrCapabilities, AsrEngine, AsrModelRef, ProgressSink, ProgressUpdate, TranscribeJob};
use crate::core::subtitle::SubtitleTrack;
use crate::error::{FinalSubError, Result};

pub struct WhisperCppEngine {
    whisper_bin: PathBuf,
    models_dir: PathBuf,
}

impl WhisperCppEngine {
    pub fn new(whisper_bin: PathBuf, models_dir: PathBuf) -> Self {
        Self {
            whisper_bin,
            models_dir,
        }
    }

    fn model_path(&self, model_id: &str) -> PathBuf {
        self.models_dir.join(format!("ggml-{model_id}.bin"))
    }

    fn is_model_downloaded(&self, model_id: &str) -> bool {
        self.model_path(model_id).exists()
    }
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
        cancel_rx: Option<tokio::sync::watch::Receiver<bool>>,
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
                progress: 0.1,
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
        ];

        if let Some(ref lang) = job.language {
            if lang != "auto" {
                args.push("-l".to_string());
                args.push(lang.clone());
            }
        }

        progress
            .send(ProgressUpdate {
                progress: 0.2,
                message: "正在转录...".into(),
            })
            .await
            .ok();

        let mut cmd = tokio::process::Command::new(&self.whisper_bin);
        cmd.args(&args);
        cmd.kill_on_drop(true);

        let output_fut = cmd.output();
        tokio::pin!(output_fut);

        let output = if let Some(mut rx) = cancel_rx {
            tokio::select! {
                res = &mut output_fut => {
                    res.map_err(|e| FinalSubError::Validation(format!("运行 whisper-cli 失败：{e}")))?
                }
                _ = rx.changed() => {
                    if *rx.borrow() {
                        return Err(FinalSubError::Validation("任务已取消".into()));
                    }
                    loop {
                        tokio::select! {
                            res = &mut output_fut => {
                                break res.map_err(|e| FinalSubError::Validation(format!("运行 whisper-cli 失败：{e}")))?;
                            }
                            change_res = rx.changed() => {
                                if change_res.is_err() || *rx.borrow() {
                                    return Err(FinalSubError::Validation("任务已取消".into()));
                                }
                            }
                        }
                    }
                }
            }
        } else {
            output_fut
                .await
                .map_err(|e| FinalSubError::Validation(format!("运行 whisper-cli 失败：{e}")))?
        };

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(FinalSubError::Validation(format!(
                "whisper-cli 转录失败：{stderr}"
            )));
        }

        progress
            .send(ProgressUpdate {
                progress: 0.9,
                message: "正在解析字幕...".into(),
            })
            .await
            .ok();

        let srt_path = format!("{output_prefix}.srt");
        let srt_content = tokio::fs::read_to_string(&srt_path)
            .await
            .map_err(|e| FinalSubError::Validation(format!("读取 SRT 输出失败：{e}")))?;

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
        );
        assert!(!engine.capabilities().supports_streaming);
        assert!(engine.capabilities().requires_model_download);
    }
}
