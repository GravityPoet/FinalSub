use async_trait::async_trait;
use std::path::PathBuf;

use super::{AsrCapabilities, AsrEngine, AsrModelRef, ProgressSink, ProgressUpdate, TranscribeJob};
use crate::core::subtitle::SubtitleTrack;
use crate::error::{FinalSubError, Result};

pub struct ParakeetMlxEngine {
    uv_bin: PathBuf,
    transcribe_script: PathBuf,
    cache_root: PathBuf,
    ffmpeg_path: Option<PathBuf>,
}

impl ParakeetMlxEngine {
    pub fn new(
        uv_bin: PathBuf,
        transcribe_script: PathBuf,
        cache_root: PathBuf,
        ffmpeg_path: Option<PathBuf>,
    ) -> Self {
        Self {
            uv_bin,
            transcribe_script,
            cache_root,
            ffmpeg_path,
        }
    }
}

#[async_trait]
impl AsrEngine for ParakeetMlxEngine {
    fn id(&self) -> &'static str {
        "parakeet-mlx"
    }

    fn capabilities(&self) -> AsrCapabilities {
        AsrCapabilities {
            supports_streaming: false,
            supported_languages: vec!["en".into(), "auto".into()],
            requires_model_download: false,
        }
    }

    async fn prepare(&self, _model: &AsrModelRef) -> Result<()> {
        if !self.uv_bin.exists() {
            return Err(FinalSubError::Validation(
                "uv 未找到。请安装 uv：https://docs.astral.sh/uv/".into(),
            ));
        }
        if !self.transcribe_script.exists() {
            return Err(FinalSubError::Validation(format!(
                "Parakeet 转录脚本未找到：{}",
                self.transcribe_script.display()
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
        let lang = job.language.as_deref().unwrap_or("auto");
        if !matches!(lang, "auto" | "en" | "english") {
            return Err(FinalSubError::Validation(format!(
                "Parakeet v2 仅支持英文转录，当前语言：{lang}"
            )));
        }

        progress
            .send(ProgressUpdate {
                progress: 0.1,
                message: "正在准备 Parakeet 环境...".into(),
            })
            .await
            .ok();

        let mut cmd = tokio::process::Command::new(&self.uv_bin);
        cmd.args([
            "run",
            "--python",
            "3.11",
            "--with",
            "parakeet-mlx",
            "--with",
            "huggingface-hub",
            "python",
            self.transcribe_script.to_str().unwrap_or(""),
            "--audio",
            &job.audio_path,
            "--output",
            &job.output_path,
            "--model",
            "mlx-community/parakeet-tdt-0.6b-v2",
            "--cache-root",
            self.cache_root.to_str().unwrap_or(""),
            "--source-language",
            lang,
        ]);

        if let Some(ref ffmpeg) = self.ffmpeg_path {
            let path_val = std::env::var("PATH").unwrap_or_default();
            cmd.env(
                "PATH",
                format!(
                    "{}:{}",
                    ffmpeg.parent().unwrap_or(ffmpeg).display(),
                    path_val
                ),
            );
            cmd.env("FFMPEG_PATH", ffmpeg.to_string_lossy().to_string());
        }

        let hf_home = self.cache_root.join("huggingface");
        cmd.env("HF_HOME", hf_home.to_string_lossy().to_string());
        cmd.env(
            "HF_HUB_CACHE",
            self.cache_root
                .join("huggingface/hub")
                .to_string_lossy()
                .to_string(),
        );
        cmd.env(
            "PARAKEET_CACHE_ROOT",
            self.cache_root.to_string_lossy().to_string(),
        );

        progress
            .send(ProgressUpdate {
                progress: 0.2,
                message: "正在转录（首次运行可能需要下载模型）...".into(),
            })
            .await
            .ok();

        cmd.kill_on_drop(true);

        let output_fut = cmd.output();
        tokio::pin!(output_fut);

        let output = if let Some(mut rx) = cancel_rx {
            tokio::select! {
                res = &mut output_fut => {
                    res.map_err(|e| FinalSubError::Validation(format!("运行 Parakeet 失败：{e}")))?
                }
                _ = rx.changed() => {
                    if *rx.borrow() {
                        return Err(FinalSubError::Validation("任务已取消".into()));
                    }
                    loop {
                        tokio::select! {
                            res = &mut output_fut => {
                                break res.map_err(|e| FinalSubError::Validation(format!("运行 Parakeet 失败：{e}")))?;
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
                .map_err(|e| FinalSubError::Validation(format!("运行 Parakeet 失败：{e}")))?
        };

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            let msg = if stderr.contains("error") {
                stderr.to_string()
            } else {
                format!("{stdout}\n{stderr}")
            };
            return Err(FinalSubError::Validation(format!(
                "Parakeet 转录失败：{msg}"
            )));
        }

        progress
            .send(ProgressUpdate {
                progress: 0.9,
                message: "正在解析字幕...".into(),
            })
            .await
            .ok();

        let srt_content = tokio::fs::read_to_string(&job.output_path)
            .await
            .map_err(|e| FinalSubError::Validation(format!("读取 SRT 输出失败：{e}")))?;

        let track = SubtitleTrack::from_srt(&srt_content)?;

        progress
            .send(ProgressUpdate {
                progress: 1.0,
                message: format!("Parakeet 转录完成，共 {} 条字幕", track.len()),
            })
            .await
            .ok();

        Ok(track)
    }
}

pub fn default_uv_bin() -> PathBuf {
    // 优先尊重用户 PATH 环境（brew / 官方安装器 / cargo / asdf 等任意来源）
    if let Ok(path_var) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path_var) {
            let cand = dir.join("uv");
            if cand.exists() {
                return cand;
            }
        }
    }
    // PATH 未命中时回退常见安装位置（不偏向某个包管理器）
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(home) = std::env::var("HOME") {
        candidates.push(PathBuf::from(&home).join(".local/bin/uv"));
        candidates.push(PathBuf::from(&home).join(".cargo/bin/uv"));
    }
    candidates.push(PathBuf::from("/opt/homebrew/bin/uv"));
    candidates.push(PathBuf::from("/usr/local/bin/uv"));
    for p in &candidates {
        if p.exists() {
            return p.clone();
        }
    }
    // 最终回退，交由运行时 PATH 解析
    PathBuf::from("uv")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capabilities_english_only() {
        let engine = ParakeetMlxEngine::new(
            PathBuf::from("/usr/bin/uv"),
            PathBuf::from("/script.py"),
            PathBuf::from("/cache"),
            None,
        );
        let caps = engine.capabilities();
        assert!(!caps.supports_streaming);
        assert!(caps.supported_languages.contains(&"en".to_string()));
        assert!(caps.supported_languages.contains(&"auto".to_string()));
        assert!(!caps.requires_model_download);
    }

    #[test]
    fn default_uv_bin_returns_uv_path() {
        let bin = default_uv_bin();
        assert!(bin.to_string_lossy().contains("uv"));
    }
}
