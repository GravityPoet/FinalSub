use async_trait::async_trait;
use std::path::PathBuf;
use super::{AsrCapabilities, AsrEngine, AsrModelRef, ProgressSink, ProgressUpdate, TranscribeJob};
use crate::core::subtitle::SubtitleTrack;
use crate::error::{FinalSubError, Result};

pub struct CustomCommandEngine {
    whisper_command: String,
    models_dir: PathBuf,
}

impl CustomCommandEngine {
    pub fn new(whisper_command: String, models_dir: PathBuf) -> Self {
        Self {
            whisper_command,
            models_dir,
        }
    }
}

fn split_arguments(cmd: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut in_double_quote = false;
    let mut in_single_quote = false;
    let mut escaped = false;
    
    for c in cmd.chars() {
        if escaped {
            current.push(c);
            escaped = false;
        } else if c == '\\' {
            escaped = true;
        } else if c == '"' && !in_single_quote {
            in_double_quote = !in_double_quote;
        } else if c == '\'' && !in_double_quote {
            in_single_quote = !in_single_quote;
        } else if c.is_whitespace() && !in_double_quote && !in_single_quote {
            let s = current.trim();
            if !s.is_empty() {
                args.push(s.to_string());
            }
            current.clear();
        } else {
            current.push(c);
        }
    }
    let s = current.trim();
    if !s.is_empty() {
        args.push(s.to_string());
    }
    args
}

#[async_trait]
impl AsrEngine for CustomCommandEngine {
    fn id(&self) -> &'static str {
        "custom-command"
    }

    fn capabilities(&self) -> AsrCapabilities {
        AsrCapabilities {
            supports_streaming: false,
            supported_languages: vec!["auto".into()],
            requires_model_download: false,
        }
    }

    async fn prepare(&self, _model: &AsrModelRef) -> Result<()> {
        if self.whisper_command.trim().is_empty() {
            return Err(FinalSubError::Validation(
                "自定义命令为空，请先在设置中配置自定义 ASR 命令。".into()
            ));
        }
        Ok(())
    }

    async fn transcribe(
        &self,
        job: TranscribeJob,
        progress: ProgressSink,
        _cancel_rx: Option<tokio::sync::watch::Receiver<bool>>,
    ) -> Result<SubtitleTrack> {
        let raw_cmd = self.whisper_command.trim();
        if raw_cmd.is_empty() {
            return Err(FinalSubError::Validation("自定义命令为空".into()));
        }

        progress.send(ProgressUpdate {
            progress: 0.1,
            message: "正在准备自定义转录命令...".into(),
        }).await.ok();

        let raw_args = split_arguments(raw_cmd);
        if raw_args.is_empty() {
            return Err(FinalSubError::Validation("解析自定义命令得到的参数列表为空".into()));
        }

        let model_path = self.models_dir.join(&job.model.model_id);
        let output_prefix = job.output_path.strip_suffix(".srt").unwrap_or(&job.output_path);

        let mut processed_args = Vec::new();
        for arg in raw_args {
            let mut new_arg = arg
                .replace("{input}", &job.audio_path)
                .replace("{output}", &job.output_path)
                .replace("{output_prefix}", output_prefix)
                .replace("{model}", &model_path.to_string_lossy());
            
            if let Some(ref lang) = job.language {
                new_arg = new_arg.replace("{language}", lang);
            } else {
                new_arg = new_arg.replace("{language}", "auto");
            }
            processed_args.push(new_arg);
        }

        progress.send(ProgressUpdate {
            progress: 0.3,
            message: format!("正在执行自定义命令: {}...", processed_args[0]).into(),
        }).await.ok();

        let mut cmd = tokio::process::Command::new(&processed_args[0]);
        if processed_args.len() > 1 {
            cmd.args(&processed_args[1..]);
        }

        let output = cmd.output().await.map_err(|e| {
            FinalSubError::Validation(format!("执行自定义命令失败: {}", e))
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(FinalSubError::Validation(format!(
                "自定义命令执行失败 (代码 {:?}): {}",
                output.status.code(),
                stderr
            )));
        }

        progress.send(ProgressUpdate {
            progress: 0.9,
            message: "自定义命令执行成功，正在读取字幕...".into(),
        }).await.ok();

        let srt_path = std::path::Path::new(&job.output_path);
        if !srt_path.exists() {
            return Err(FinalSubError::Validation(format!(
                "自定义命令执行成功，但未找到输出字幕文件：{}",
                job.output_path
            )));
        }

        let srt_content = tokio::fs::read_to_string(srt_path).await.map_err(|e| {
            FinalSubError::Validation(format!("读取输出字幕文件失败: {}", e))
        })?;

        SubtitleTrack::from_srt(&srt_content)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_arguments() {
        // Simple arguments
        let args = split_arguments("whisper-cli -m model.bin -f input.wav");
        assert_eq!(args, vec!["whisper-cli", "-m", "model.bin", "-f", "input.wav"]);

        // Quoted arguments
        let args = split_arguments("whisper-cli -m \"my model path.bin\" -f 'my input.wav'");
        assert_eq!(args, vec!["whisper-cli", "-m", "my model path.bin", "-f", "my input.wav"]);

        // Escaped whitespace
        let args = split_arguments("whisper-cli -m my\\ model\\ path.bin");
        assert_eq!(args, vec!["whisper-cli", "-m", "my model path.bin"]);

        // Shell controls parsed as words, demonstrating that no shell execution context is created
        let args = split_arguments("whisper-cli; rm -rf /");
        assert_eq!(args, vec!["whisper-cli;", "rm", "-rf", "/"]);

        let args = split_arguments("whisper-cli && echo 'hello'");
        assert_eq!(args, vec!["whisper-cli", "&&", "echo", "hello"]);
    }
}

