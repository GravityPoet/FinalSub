use serde::{Deserialize, Serialize};
use std::ffi::OsStr;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};

use super::parakeet::{build_cues, ParakeetNativeEngine, REQUIRED_MODEL_FILES};
use super::vad::{detect_speech, SAMPLE_RATE};
use super::{ProgressSink, ProgressUpdate};
use crate::core::subtitle::{parse_subtitle_length_mode, SubtitleTrack};
use crate::error::{FinalSubError, Result};

const PARAKEET_WORKER_ARG: &str = "--finalsub-parakeet-worker";
const PROTOCOL_VERSION: u32 = 1;
const MAX_REQUEST_LINE_BYTES: usize = 64 * 1024;
const MAX_RESPONSE_LINE_BYTES: usize = 4 * 1024 * 1024;
const WORKER_START_TIMEOUT: Duration = Duration::from_secs(5);
const WORKER_STOP_TIMEOUT: Duration = Duration::from_secs(2);
const MAX_SEGMENT_SECONDS: usize = 55;

#[derive(Debug, Serialize, Deserialize)]
struct ParakeetWorkerRequest {
    protocol_version: u32,
    request_id: String,
    model_dir: String,
    vad_model_path: String,
    audio_path: String,
    max_subtitle_chars: i32,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
enum ParakeetWorkerEvent {
    Ready {
        protocol_version: u32,
        worker_pid: u32,
    },
    Progress {
        protocol_version: u32,
        request_id: String,
        worker_pid: u32,
        progress: f32,
        message: String,
    },
    Result {
        protocol_version: u32,
        request_id: String,
        worker_pid: u32,
        track: SubtitleTrack,
    },
    Error {
        protocol_version: u32,
        request_id: String,
        worker_pid: u32,
        error: String,
    },
}

fn valid_request_id(value: &str) -> bool {
    !value.is_empty() && value.len() <= 128 && !value.chars().any(char::is_control)
}

fn validated_absolute_path(value: &str, label: &str) -> std::result::Result<PathBuf, String> {
    if value.is_empty() || value.len() > 8 * 1024 || value.contains('\0') {
        return Err(format!("{label}无效"));
    }
    let path = PathBuf::from(value);
    if !path.is_absolute() {
        return Err(format!("{label}必须是绝对路径"));
    }
    Ok(path)
}

fn validate_request(
    request: &ParakeetWorkerRequest,
) -> std::result::Result<(PathBuf, PathBuf, PathBuf), String> {
    if request.protocol_version != PROTOCOL_VERSION {
        return Err(format!(
            "Parakeet worker 协议版本不匹配：期望 {PROTOCOL_VERSION}"
        ));
    }
    if !valid_request_id(&request.request_id) {
        return Err("Parakeet worker 请求 ID 无效".into());
    }
    parse_subtitle_length_mode(request.max_subtitle_chars).map_err(|error| error.to_string())?;

    let model_dir = validated_absolute_path(&request.model_dir, "Parakeet 模型目录")?;
    if !model_dir.is_dir() || !ParakeetNativeEngine::is_model_installed_at(&model_dir) {
        return Err("Parakeet 模型目录不完整".into());
    }
    for name in REQUIRED_MODEL_FILES {
        let metadata = std::fs::metadata(model_dir.join(name))
            .map_err(|_| format!("Parakeet 模型文件不可读：{name}"))?;
        if metadata.len() == 0 {
            return Err(format!("Parakeet 模型文件为空：{name}"));
        }
    }

    let vad_model_path = validated_absolute_path(&request.vad_model_path, "Silero VAD 模型路径")?;
    if !vad_model_path.is_file()
        || vad_model_path.extension().and_then(|value| value.to_str()) != Some("onnx")
    {
        return Err("Silero VAD 模型文件无效".into());
    }

    let audio_path = validated_absolute_path(&request.audio_path, "Parakeet 音频路径")?;
    if !audio_path.is_file()
        || audio_path.extension().and_then(|value| value.to_str()) != Some("wav")
    {
        return Err("Parakeet 仅接受已存在的 WAV 音频".into());
    }
    Ok((model_dir, vad_model_path, audio_path))
}

fn write_event<W: Write + ?Sized>(
    output: &mut W,
    event: &ParakeetWorkerEvent,
) -> std::result::Result<(), ()> {
    let encoded = serde_json::to_vec(event).map_err(|_| ())?;
    if encoded.len() > MAX_RESPONSE_LINE_BYTES {
        return Err(());
    }
    output.write_all(&encoded).map_err(|_| ())?;
    output.write_all(b"\n").map_err(|_| ())?;
    output.flush().map_err(|_| ())
}

fn read_bounded_line(input: &mut impl BufRead, maximum: usize) -> std::io::Result<Option<Vec<u8>>> {
    let mut output = Vec::with_capacity(4 * 1024);
    let mut too_large = false;
    loop {
        let available = input.fill_buf()?;
        if available.is_empty() {
            if output.is_empty() && !too_large {
                return Ok(None);
            }
            break;
        }
        let newline = available.iter().position(|byte| *byte == b'\n');
        let payload_len = newline.unwrap_or(available.len());
        if !too_large {
            let remaining = maximum.saturating_sub(output.len());
            let copied = payload_len.min(remaining);
            output.extend_from_slice(&available[..copied]);
            if copied < payload_len {
                too_large = true;
                output.clear();
            }
        }
        let consumed = newline.map_or(available.len(), |index| index + 1);
        input.consume(consumed);
        if newline.is_some() {
            break;
        }
    }
    if too_large {
        Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "protocol line too large",
        ))
    } else {
        Ok(Some(output))
    }
}

fn recognize_request(
    request: &ParakeetWorkerRequest,
    output: &mut impl Write,
) -> std::result::Result<SubtitleTrack, String> {
    let (model_dir, vad_model_path, audio_path) = validate_request(request)?;
    let worker_pid = std::process::id();
    let report = |output: &mut dyn Write,
                  progress: f32,
                  message: String|
     -> std::result::Result<(), String> {
        write_event(
            output,
            &ParakeetWorkerEvent::Progress {
                protocol_version: PROTOCOL_VERSION,
                request_id: request.request_id.clone(),
                worker_pid,
                progress,
                message,
            },
        )
        .map_err(|_| "Parakeet worker 无法发送进度".to_string())
    };

    report(output, 0.06, "正在读取 Parakeet 音频...".into())?;
    let wave = sherpa_onnx::Wave::read(audio_path.to_string_lossy().as_ref())
        .ok_or_else(|| "读取 WAV 音频失败，请确认音频提取结果有效".to_string())?;
    if wave.sample_rate() != SAMPLE_RATE {
        return Err(format!(
            "Parakeet 需要 16 kHz 单声道 WAV，当前采样率为 {} Hz",
            wave.sample_rate()
        ));
    }

    report(output, 0.08, "正在用 Silero VAD 切分长音频...".into())?;
    let segments = detect_speech(wave.samples(), &vad_model_path, MAX_SEGMENT_SECONDS)?;
    if segments.is_empty() {
        return Err("Silero VAD 未检测到可识别的人声".into());
    }

    report(
        output,
        0.10,
        format!(
            "正在加载 Parakeet 模型，共 {} 个语音片段...",
            segments.len()
        ),
    )?;
    let path = |name: &str| model_dir.join(name).to_string_lossy().into_owned();
    let mut config = sherpa_onnx::OfflineRecognizerConfig::default();
    config.model_config.transducer = sherpa_onnx::OfflineTransducerModelConfig {
        encoder: Some(path("encoder.int8.onnx")),
        decoder: Some(path("decoder.int8.onnx")),
        joiner: Some(path("joiner.int8.onnx")),
    };
    config.model_config.tokens = Some(path("tokens.txt"));
    config.model_config.model_type = Some("nemo_transducer".into());
    config.model_config.num_threads = std::thread::available_parallelism()
        .map(|count| count.get().clamp(2, 8) as i32)
        .unwrap_or(2);
    config.model_config.provider = Some("cpu".into());
    config.decoding_method = Some("greedy_search".into());
    let recognizer = sherpa_onnx::OfflineRecognizer::create(&config)
        .ok_or_else(|| "创建 Parakeet 原生识别器失败".to_string())?;

    let total = segments.len();
    let mut cues = Vec::new();
    for (segment_index, segment) in segments.into_iter().enumerate() {
        let stream = recognizer.create_stream();
        stream.accept_waveform(SAMPLE_RATE, &segment.samples);
        recognizer.decode(&stream);
        let result = stream
            .get_result()
            .ok_or_else(|| format!("Parakeet 未返回第 {} 个语音片段的结果", segment_index + 1))?;
        let segment_duration_ms = segment.samples.len() as u64 * 1_000 / SAMPLE_RATE as u64;
        let segment_start_ms = segment.start_sample as u64 * 1_000 / SAMPLE_RATE as u64;
        let mut segment_cues = build_cues(
            &result.text,
            &result.tokens,
            result.timestamps.as_deref(),
            segment_duration_ms,
            request.max_subtitle_chars,
        );
        for cue in &mut segment_cues {
            cue.start_ms = cue.start_ms.saturating_add(segment_start_ms);
            cue.end_ms = cue.end_ms.saturating_add(segment_start_ms);
        }
        cues.append(&mut segment_cues);

        let fraction = (segment_index + 1) as f32 / total as f32;
        report(
            output,
            0.12 + fraction * 0.84,
            format!(
                "Parakeet 正在识别语音片段 {}/{}...",
                segment_index + 1,
                total
            ),
        )?;
    }
    cues.sort_by_key(|cue| (cue.start_ms, cue.end_ms));
    for (index, cue) in cues.iter_mut().enumerate() {
        cue.index = (index + 1) as u32;
    }
    if cues.is_empty() {
        return Err(
            "Parakeet 未识别到字幕内容；该模型仅适用于英文语音，请检查音频或切换引擎".into(),
        );
    }
    Ok(SubtitleTrack { cues })
}

fn run_worker_stdio() -> i32 {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut input = stdin.lock();
    let mut output = stdout.lock();
    if write_event(
        &mut output,
        &ParakeetWorkerEvent::Ready {
            protocol_version: PROTOCOL_VERSION,
            worker_pid: std::process::id(),
        },
    )
    .is_err()
    {
        return 2;
    }

    let line = match read_bounded_line(&mut input, MAX_REQUEST_LINE_BYTES) {
        Ok(Some(line)) => line,
        Ok(None) => return 0,
        Err(_) => {
            let _ = write_event(
                &mut output,
                &ParakeetWorkerEvent::Error {
                    protocol_version: PROTOCOL_VERSION,
                    request_id: "invalid".into(),
                    worker_pid: std::process::id(),
                    error: "Parakeet worker 请求过大".into(),
                },
            );
            return 0;
        }
    };
    let request = match serde_json::from_slice::<ParakeetWorkerRequest>(&line) {
        Ok(request) => request,
        Err(_) => {
            let _ = write_event(
                &mut output,
                &ParakeetWorkerEvent::Error {
                    protocol_version: PROTOCOL_VERSION,
                    request_id: "invalid".into(),
                    worker_pid: std::process::id(),
                    error: "Parakeet worker 请求格式无效".into(),
                },
            );
            return 0;
        }
    };
    let request_id = request.request_id.clone();
    let event = match recognize_request(&request, &mut output) {
        Ok(track) => ParakeetWorkerEvent::Result {
            protocol_version: PROTOCOL_VERSION,
            request_id,
            worker_pid: std::process::id(),
            track,
        },
        Err(error) => ParakeetWorkerEvent::Error {
            protocol_version: PROTOCOL_VERSION,
            request_id,
            worker_pid: std::process::id(),
            error,
        },
    };
    if write_event(&mut output, &event).is_ok() {
        0
    } else {
        3
    }
}

fn is_worker_invocation<I, S>(args: I) -> bool
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut args = args.into_iter();
    let _executable = args.next();
    args.next()
        .is_some_and(|arg| arg.as_ref() == OsStr::new(PARAKEET_WORKER_ARG))
        && args.next().is_none()
}

pub fn maybe_run_parakeet_worker() -> Option<i32> {
    is_worker_invocation(std::env::args_os()).then(run_worker_stdio)
}

async fn read_async_bounded_line<R: AsyncBufRead + Unpin>(
    input: &mut R,
    maximum: usize,
) -> std::io::Result<Option<Vec<u8>>> {
    let mut output = Vec::with_capacity(4 * 1024);
    let mut too_large = false;
    loop {
        let available = input.fill_buf().await?;
        if available.is_empty() {
            if output.is_empty() && !too_large {
                return Ok(None);
            }
            break;
        }
        let newline = available.iter().position(|byte| *byte == b'\n');
        let payload_len = newline.unwrap_or(available.len());
        if !too_large {
            let remaining = maximum.saturating_sub(output.len());
            let copied = payload_len.min(remaining);
            output.extend_from_slice(&available[..copied]);
            if copied < payload_len {
                too_large = true;
                output.clear();
            }
        }
        let consumed = newline.map_or(available.len(), |index| index + 1);
        input.consume(consumed);
        if newline.is_some() {
            break;
        }
    }
    if too_large {
        Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "protocol line too large",
        ))
    } else {
        Ok(Some(output))
    }
}

async fn read_async_event(
    output: &mut BufReader<tokio::process::ChildStdout>,
) -> Result<Option<ParakeetWorkerEvent>> {
    let Some(line) = read_async_bounded_line(output, MAX_RESPONSE_LINE_BYTES).await? else {
        return Ok(None);
    };
    Ok(Some(serde_json::from_slice(&line)?))
}

async fn terminate_worker(child: &mut Child) {
    let _ = child.kill().await;
    let _ = tokio::time::timeout(WORKER_STOP_TIMEOUT, child.wait()).await;
}

fn validate_worker_event(
    protocol_version: u32,
    response_request_id: &str,
    worker_pid: u32,
    request_id: &str,
) -> Result<()> {
    if protocol_version != PROTOCOL_VERSION
        || response_request_id != request_id
        || worker_pid == std::process::id()
    {
        return Err(FinalSubError::EngineNotReady(
            "Parakeet worker 返回了无效响应".into(),
        ));
    }
    Ok(())
}

pub(super) async fn transcribe_isolated(
    model_dir: &Path,
    vad_model_path: &Path,
    audio_path: &Path,
    max_subtitle_chars: i32,
    progress: ProgressSink,
    mut cancel_rx: Option<tokio::sync::watch::Receiver<bool>>,
) -> Result<SubtitleTrack> {
    let executable = std::env::current_exe().map_err(|error| {
        FinalSubError::EngineNotReady(format!("无法定位 FinalSub 可执行文件：{error}"))
    })?;
    if !executable.is_absolute() {
        return Err(FinalSubError::EngineNotReady(
            "FinalSub 可执行文件路径无效".into(),
        ));
    }
    let mut child = Command::new(&executable)
        .arg(PARAKEET_WORKER_ARG)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|error| {
            FinalSubError::EngineNotReady(format!("无法启动 Parakeet worker：{error}"))
        })?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| FinalSubError::EngineNotReady("无法连接 Parakeet worker 输入".into()))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| FinalSubError::EngineNotReady("无法连接 Parakeet worker 输出".into()))?;
    let mut stdout = BufReader::new(stdout);

    let ready = tokio::time::timeout(WORKER_START_TIMEOUT, read_async_event(&mut stdout))
        .await
        .map_err(|_| FinalSubError::EngineNotReady("Parakeet worker 启动超时".into()))??;
    let Some(ParakeetWorkerEvent::Ready {
        protocol_version,
        worker_pid,
    }) = ready
    else {
        terminate_worker(&mut child).await;
        return Err(FinalSubError::EngineNotReady(
            "Parakeet worker 启动握手失败".into(),
        ));
    };
    if protocol_version != PROTOCOL_VERSION || worker_pid == std::process::id() {
        terminate_worker(&mut child).await;
        return Err(FinalSubError::EngineNotReady(
            "Parakeet worker 启动握手无效".into(),
        ));
    }

    let request_id = uuid::Uuid::new_v4().to_string();
    let request = ParakeetWorkerRequest {
        protocol_version: PROTOCOL_VERSION,
        request_id: request_id.clone(),
        model_dir: model_dir.to_string_lossy().into_owned(),
        vad_model_path: vad_model_path.to_string_lossy().into_owned(),
        audio_path: audio_path.to_string_lossy().into_owned(),
        max_subtitle_chars,
    };
    let mut encoded = serde_json::to_vec(&request)?;
    if encoded.len() > MAX_REQUEST_LINE_BYTES {
        terminate_worker(&mut child).await;
        return Err(FinalSubError::Validation("Parakeet worker 请求过大".into()));
    }
    encoded.push(b'\n');
    stdin.write_all(&encoded).await?;
    stdin.flush().await?;
    drop(stdin);

    loop {
        let event = if let Some(cancel) = cancel_rx.as_mut() {
            tokio::select! {
                event = read_async_event(&mut stdout) => event?,
                changed = cancel.changed() => {
                    if changed.is_err() || *cancel.borrow() {
                        terminate_worker(&mut child).await;
                        return Err(FinalSubError::Validation("任务已取消".into()));
                    }
                    continue;
                }
            }
        } else {
            read_async_event(&mut stdout).await?
        };
        let Some(event) = event else {
            let status = child.wait().await?;
            return Err(FinalSubError::EngineNotReady(format!(
                "Parakeet worker 异常退出（{status}）；当前任务已停止，FinalSub 主界面保持运行"
            )));
        };
        match event {
            ParakeetWorkerEvent::Ready { .. } => {
                terminate_worker(&mut child).await;
                return Err(FinalSubError::EngineNotReady(
                    "Parakeet worker 重复发送启动握手".into(),
                ));
            }
            ParakeetWorkerEvent::Progress {
                protocol_version,
                request_id: response_request_id,
                worker_pid,
                progress: value,
                message,
            } => {
                validate_worker_event(
                    protocol_version,
                    &response_request_id,
                    worker_pid,
                    &request_id,
                )?;
                progress
                    .send(ProgressUpdate {
                        progress: value.clamp(0.0, 1.0),
                        message,
                    })
                    .await
                    .ok();
            }
            ParakeetWorkerEvent::Result {
                protocol_version,
                request_id: response_request_id,
                worker_pid,
                track,
            } => {
                validate_worker_event(
                    protocol_version,
                    &response_request_id,
                    worker_pid,
                    &request_id,
                )?;
                match tokio::time::timeout(WORKER_STOP_TIMEOUT, child.wait()).await {
                    Ok(Ok(status)) if status.success() => return Ok(track),
                    Ok(Ok(status)) => {
                        return Err(FinalSubError::EngineNotReady(format!(
                            "Parakeet worker 返回结果后异常退出（{status}）"
                        )))
                    }
                    Ok(Err(error)) => return Err(error.into()),
                    Err(_) => {
                        terminate_worker(&mut child).await;
                        return Ok(track);
                    }
                }
            }
            ParakeetWorkerEvent::Error {
                protocol_version,
                request_id: response_request_id,
                worker_pid,
                error,
            } => {
                validate_worker_event(
                    protocol_version,
                    &response_request_id,
                    worker_pid,
                    &request_id,
                )?;
                let _ = tokio::time::timeout(WORKER_STOP_TIMEOUT, child.wait()).await;
                return Err(FinalSubError::Worker(error));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worker_mode_requires_the_exact_private_argument() {
        assert!(is_worker_invocation(["finalsub", PARAKEET_WORKER_ARG]));
        assert!(!is_worker_invocation(["finalsub"]));
        assert!(!is_worker_invocation([
            "finalsub",
            "--finalsub-parakeet-worker-extra"
        ]));
        assert!(!is_worker_invocation([
            "finalsub",
            PARAKEET_WORKER_ARG,
            "unexpected"
        ]));
    }

    #[test]
    fn bounded_reader_rejects_large_line_and_drains_it() {
        let mut input = vec![b'x'; 17];
        input.extend_from_slice(b"\n{}\n");
        let mut cursor = std::io::Cursor::new(input);
        assert!(read_bounded_line(&mut cursor, 16).is_err());
        assert_eq!(
            read_bounded_line(&mut cursor, 16).unwrap(),
            Some(b"{}".to_vec())
        );
    }

    #[test]
    fn request_validation_rejects_relative_paths_and_bad_width() {
        let request = ParakeetWorkerRequest {
            protocol_version: PROTOCOL_VERSION,
            request_id: "probe".into(),
            model_dir: "relative-model".into(),
            vad_model_path: "relative-vad".into(),
            audio_path: "relative.wav".into(),
            max_subtitle_chars: 0,
        };
        assert!(validate_request(&request).is_err());
        let bad_width = ParakeetWorkerRequest {
            max_subtitle_chars: 7,
            ..request
        };
        assert!(validate_request(&bad_width).is_err());
    }
}
