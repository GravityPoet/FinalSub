use serde::{Deserialize, Serialize};
use std::ffi::OsStr;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::{Mutex, MutexGuard, OwnedSemaphorePermit, Semaphore};

use super::engine::{
    synthesize_local, LocalTtsSynthesisRequest, TtsEngineCache, TtsSynthesisResult,
};
use super::models::{resolve_ready_model_at_path, ReadyTtsModel};
use crate::error::{FinalSubError, Result};

const TTS_WORKER_ARG: &str = "--finalsub-tts-worker";
const PROTOCOL_VERSION: u32 = 1;
const MAX_PROTOCOL_LINE_BYTES: usize = 64 * 1024;
const RESPONSE_POLL_INTERVAL: Duration = Duration::from_millis(50);
const WORKER_START_TIMEOUT: Duration = Duration::from_secs(5);
const WORKER_STOP_TIMEOUT: Duration = Duration::from_secs(2);
const MAX_TTS_WORKERS: usize = 3;

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
enum TtsWorkerRequest {
    Ping {
        protocol_version: u32,
        request_id: String,
    },
    Synthesize {
        protocol_version: u32,
        request_id: String,
        model_id: String,
        model_path: String,
        synthesis: Box<LocalTtsSynthesisRequest>,
    },
    Dispose {
        protocol_version: u32,
        request_id: String,
    },
}

impl TtsWorkerRequest {
    fn request_id(&self) -> &str {
        match self {
            Self::Ping { request_id, .. }
            | Self::Synthesize { request_id, .. }
            | Self::Dispose { request_id, .. } => request_id,
        }
    }

    fn protocol_version(&self) -> u32 {
        match self {
            Self::Ping {
                protocol_version, ..
            }
            | Self::Synthesize {
                protocol_version, ..
            }
            | Self::Dispose {
                protocol_version, ..
            } => *protocol_version,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct TtsWorkerResponse {
    protocol_version: u32,
    request_id: String,
    worker_pid: u32,
    ok: bool,
    result: Option<TtsSynthesisResult>,
    error: Option<String>,
}

impl TtsWorkerResponse {
    fn success(request_id: String, result: Option<TtsSynthesisResult>) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            request_id,
            worker_pid: std::process::id(),
            ok: true,
            result,
            error: None,
        }
    }

    fn failure(request_id: String, error: impl Into<String>) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            request_id,
            worker_pid: std::process::id(),
            ok: false,
            result: None,
            error: Some(error.into()),
        }
    }
}

fn validate_request_envelope(request: &TtsWorkerRequest) -> Result<()> {
    if request.protocol_version() != PROTOCOL_VERSION {
        return Err(FinalSubError::Validation(format!(
            "TTS worker 协议版本不匹配：期望 {PROTOCOL_VERSION}"
        )));
    }
    let request_id = request.request_id();
    if request_id.is_empty() || request_id.len() > 128 || request_id.chars().any(char::is_control) {
        return Err(FinalSubError::Validation("TTS worker 请求 ID 无效".into()));
    }
    Ok(())
}

fn write_worker_response(
    output: &mut impl Write,
    response: &TtsWorkerResponse,
) -> std::result::Result<(), ()> {
    let encoded = serde_json::to_vec(response).map_err(|_| ())?;
    if encoded.len() > MAX_PROTOCOL_LINE_BYTES {
        return Err(());
    }
    output.write_all(&encoded).map_err(|_| ())?;
    output.write_all(b"\n").map_err(|_| ())?;
    output.flush().map_err(|_| ())
}

#[derive(Debug, PartialEq, Eq)]
enum ProtocolLine {
    Eof,
    Data,
    TooLarge,
}

fn read_protocol_line(
    input: &mut impl BufRead,
    buffer: &mut Vec<u8>,
) -> std::io::Result<ProtocolLine> {
    buffer.clear();
    let mut too_large = false;
    let mut read_any = false;
    loop {
        let available = input.fill_buf()?;
        if available.is_empty() {
            return Ok(if !read_any {
                ProtocolLine::Eof
            } else if too_large {
                ProtocolLine::TooLarge
            } else {
                ProtocolLine::Data
            });
        }
        read_any = true;
        let newline = available.iter().position(|byte| *byte == b'\n');
        let payload_len = newline.unwrap_or(available.len());
        if !too_large {
            let remaining = MAX_PROTOCOL_LINE_BYTES.saturating_sub(buffer.len());
            let copied = payload_len.min(remaining);
            buffer.extend_from_slice(&available[..copied]);
            if copied < payload_len {
                too_large = true;
                buffer.clear();
            }
        }
        let consumed = newline.map_or(available.len(), |index| index + 1);
        input.consume(consumed);
        if newline.is_some() {
            return Ok(if too_large {
                ProtocolLine::TooLarge
            } else {
                ProtocolLine::Data
            });
        }
    }
}

fn run_worker_stdio() -> i32 {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut input = stdin.lock();
    let mut output = stdout.lock();
    let cache: TtsEngineCache = Arc::new(std::sync::Mutex::new(std::collections::HashMap::new()));
    let mut line = Vec::with_capacity(4 * 1024);

    loop {
        let status = match read_protocol_line(&mut input, &mut line) {
            Ok(status) => status,
            Err(_) => return 2,
        };
        match status {
            ProtocolLine::Eof => return 0,
            ProtocolLine::TooLarge => {
                if write_worker_response(
                    &mut output,
                    &TtsWorkerResponse::failure("invalid".into(), "TTS worker 请求过大"),
                )
                .is_err()
                {
                    return 3;
                }
                continue;
            }
            ProtocolLine::Data => {}
        }

        let parsed = serde_json::from_slice::<TtsWorkerRequest>(&line);
        let request = match parsed {
            Ok(request) => request,
            Err(_) => {
                if write_worker_response(
                    &mut output,
                    &TtsWorkerResponse::failure("invalid".into(), "TTS worker 请求格式无效"),
                )
                .is_err()
                {
                    return 3;
                }
                continue;
            }
        };
        let request_id = request.request_id().to_string();
        let response = match validate_request_envelope(&request) {
            Err(error) => TtsWorkerResponse::failure(request_id, error.to_string()),
            Ok(()) => match request {
                TtsWorkerRequest::Ping { request_id, .. } => {
                    TtsWorkerResponse::success(request_id, None)
                }
                TtsWorkerRequest::Synthesize {
                    request_id,
                    model_id,
                    model_path,
                    synthesis,
                    ..
                } => match resolve_ready_model_at_path(&model_id, &model_path).and_then(|model| {
                    synthesize_local(&cache, model, *synthesis, Arc::new(AtomicBool::new(false)))
                }) {
                    Ok(result) => TtsWorkerResponse::success(request_id, Some(result)),
                    Err(error) => TtsWorkerResponse::failure(request_id, error.to_string()),
                },
                TtsWorkerRequest::Dispose { request_id, .. } => {
                    let response = TtsWorkerResponse::success(request_id, None);
                    return if write_worker_response(&mut output, &response).is_ok() {
                        0
                    } else {
                        3
                    };
                }
            },
        };
        if write_worker_response(&mut output, &response).is_err() {
            return 3;
        }
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
        .is_some_and(|arg| arg.as_ref() == OsStr::new(TTS_WORKER_ARG))
        && args.next().is_none()
}

pub fn maybe_run_tts_worker() -> Option<i32> {
    is_worker_invocation(std::env::args_os()).then(run_worker_stdio)
}

struct TtsWorkerProcess {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    response_buffer: Vec<u8>,
    response_too_large: bool,
}

#[derive(Clone)]
pub(crate) struct TtsWorkerManager {
    processes: Arc<Vec<Mutex<Option<TtsWorkerProcess>>>>,
    capacity: Arc<Semaphore>,
    next_slot: Arc<AtomicUsize>,
    executable: Arc<PathBuf>,
}

impl Default for TtsWorkerManager {
    fn default() -> Self {
        let executable = std::env::current_exe().unwrap_or_else(|_| PathBuf::new());
        Self {
            processes: Arc::new((0..MAX_TTS_WORKERS).map(|_| Mutex::new(None)).collect()),
            capacity: Arc::new(Semaphore::new(MAX_TTS_WORKERS)),
            next_slot: Arc::new(AtomicUsize::new(0)),
            executable: Arc::new(executable),
        }
    }
}

impl TtsWorkerManager {
    async fn acquire_capacity(&self, cancelled: &AtomicBool) -> Result<OwnedSemaphorePermit> {
        loop {
            if cancelled.load(Ordering::Relaxed) {
                return Err(FinalSubError::Validation("配音已取消".into()));
            }
            if let Ok(result) = tokio::time::timeout(
                RESPONSE_POLL_INTERVAL,
                self.capacity.clone().acquire_owned(),
            )
            .await
            {
                return result
                    .map_err(|_| FinalSubError::EngineNotReady("本地 TTS worker 池已关闭".into()));
            }
        }
    }

    async fn lock_available_process(
        &self,
        cancelled: &AtomicBool,
    ) -> Result<MutexGuard<'_, Option<TtsWorkerProcess>>> {
        let start = self.next_slot.fetch_add(1, Ordering::Relaxed) % MAX_TTS_WORKERS;
        loop {
            if cancelled.load(Ordering::Relaxed) {
                return Err(FinalSubError::Validation("配音已取消".into()));
            }
            for offset in 0..MAX_TTS_WORKERS {
                let index = (start + offset) % MAX_TTS_WORKERS;
                if let Ok(guard) = self.processes[index].try_lock() {
                    if guard.is_some() {
                        return Ok(guard);
                    }
                }
            }
            for offset in 0..MAX_TTS_WORKERS {
                let index = (start + offset) % MAX_TTS_WORKERS;
                if let Ok(guard) = self.processes[index].try_lock() {
                    return Ok(guard);
                }
            }
            tokio::time::sleep(RESPONSE_POLL_INTERVAL).await;
        }
    }

    async fn send_request(
        process: &mut TtsWorkerProcess,
        request: &TtsWorkerRequest,
    ) -> Result<()> {
        let encoded = serde_json::to_vec(request)?;
        if encoded.len() > MAX_PROTOCOL_LINE_BYTES {
            return Err(FinalSubError::Validation("TTS worker 请求过大".into()));
        }
        process.stdin.write_all(&encoded).await?;
        process.stdin.write_all(b"\n").await?;
        process.stdin.flush().await?;
        Ok(())
    }

    async fn read_response(process: &mut TtsWorkerProcess) -> Result<TtsWorkerResponse> {
        loop {
            let available = process.stdout.fill_buf().await?;
            if available.is_empty() {
                return Err(FinalSubError::EngineNotReady(
                    "本地 TTS worker 已退出".into(),
                ));
            }
            let newline = available.iter().position(|byte| *byte == b'\n');
            let payload_len = newline.unwrap_or(available.len());
            if !process.response_too_large {
                let remaining =
                    MAX_PROTOCOL_LINE_BYTES.saturating_sub(process.response_buffer.len());
                let copied = payload_len.min(remaining);
                process
                    .response_buffer
                    .extend_from_slice(&available[..copied]);
                if copied < payload_len {
                    process.response_too_large = true;
                    process.response_buffer.clear();
                }
            }
            let consumed = newline.map_or(available.len(), |index| index + 1);
            process.stdout.consume(consumed);
            if newline.is_some() {
                if process.response_too_large {
                    process.response_too_large = false;
                    process.response_buffer.clear();
                    return Err(FinalSubError::EngineNotReady(
                        "本地 TTS worker 返回了过大响应".into(),
                    ));
                }
                let line = std::mem::take(&mut process.response_buffer);
                return Ok(serde_json::from_slice(&line)?);
            }
        }
    }

    async fn terminate(process: &mut Option<TtsWorkerProcess>) {
        if let Some(mut worker) = process.take() {
            let _ = worker.child.kill().await;
            let _ = tokio::time::timeout(WORKER_STOP_TIMEOUT, worker.child.wait()).await;
        }
    }

    async fn spawn_worker(&self) -> Result<TtsWorkerProcess> {
        if self.executable.as_os_str().is_empty() || !self.executable.is_absolute() {
            return Err(FinalSubError::EngineNotReady(
                "无法定位 FinalSub 可执行文件，不能启动本地 TTS worker".into(),
            ));
        }
        let mut child = Command::new(self.executable.as_ref())
            .arg(TTS_WORKER_ARG)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|error| {
                FinalSubError::EngineNotReady(format!("无法启动本地 TTS worker：{error}"))
            })?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| FinalSubError::EngineNotReady("无法连接 TTS worker 输入".into()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| FinalSubError::EngineNotReady("无法连接 TTS worker 输出".into()))?;
        let mut process = TtsWorkerProcess {
            child,
            stdin,
            stdout: BufReader::new(stdout),
            response_buffer: Vec::with_capacity(1024),
            response_too_large: false,
        };
        let request_id = uuid::Uuid::new_v4().to_string();
        let ping = TtsWorkerRequest::Ping {
            protocol_version: PROTOCOL_VERSION,
            request_id: request_id.clone(),
        };
        Self::send_request(&mut process, &ping).await?;
        let response =
            tokio::time::timeout(WORKER_START_TIMEOUT, Self::read_response(&mut process))
                .await
                .map_err(|_| FinalSubError::EngineNotReady("本地 TTS worker 启动超时".into()))??;
        if response.protocol_version != PROTOCOL_VERSION
            || response.request_id != request_id
            || !response.ok
            || response.worker_pid == std::process::id()
        {
            let _ = process.child.kill().await;
            return Err(FinalSubError::EngineNotReady(
                "本地 TTS worker 启动握手失败".into(),
            ));
        }
        Ok(process)
    }

    async fn ensure_worker(&self, process: &mut Option<TtsWorkerProcess>) -> Result<()> {
        let must_restart = match process.as_mut() {
            Some(worker) => match worker.child.try_wait() {
                Ok(None) => false,
                Ok(Some(_)) | Err(_) => true,
            },
            None => true,
        };
        if must_restart {
            Self::terminate(process).await;
            *process = Some(self.spawn_worker().await?);
        }
        Ok(())
    }

    fn cleanup_temporary_output(request: &LocalTtsSynthesisRequest) {
        let output = Path::new(request.output_path.trim());
        if output.is_absolute()
            && output.extension().and_then(|value| value.to_str()) == Some("wav")
        {
            let _ = std::fs::remove_file(output.with_extension("wav.generating"));
        }
    }

    pub(crate) async fn synthesize(
        &self,
        model: ReadyTtsModel,
        request: LocalTtsSynthesisRequest,
        cancelled: Arc<AtomicBool>,
    ) -> Result<TtsSynthesisResult> {
        let _capacity = self.acquire_capacity(&cancelled).await?;
        let mut process = self.lock_available_process(&cancelled).await?;
        self.ensure_worker(&mut process).await?;
        let request_id = uuid::Uuid::new_v4().to_string();
        let worker_request = TtsWorkerRequest::Synthesize {
            protocol_version: PROTOCOL_VERSION,
            request_id: request_id.clone(),
            model_id: model.spec.id.to_string(),
            model_path: model.path.to_string_lossy().to_string(),
            synthesis: Box::new(request.clone()),
        };
        if let Some(worker) = process.as_mut() {
            if let Err(error) = Self::send_request(worker, &worker_request).await {
                Self::terminate(&mut process).await;
                Self::cleanup_temporary_output(&request);
                return Err(FinalSubError::EngineNotReady(format!(
                    "本地 TTS worker 通信失败：{error}"
                )));
            }
        }

        loop {
            if cancelled.load(Ordering::Relaxed) {
                Self::terminate(&mut process).await;
                Self::cleanup_temporary_output(&request);
                return Err(FinalSubError::Validation("配音已取消".into()));
            }
            let response = match process.as_mut() {
                Some(worker) => {
                    match tokio::time::timeout(RESPONSE_POLL_INTERVAL, Self::read_response(worker))
                        .await
                    {
                        Ok(response) => response,
                        Err(_) => continue,
                    }
                }
                None => Err(FinalSubError::EngineNotReady(
                    "本地 TTS worker 不可用".into(),
                )),
            };
            let response = match response {
                Ok(response) => response,
                Err(error) => {
                    Self::terminate(&mut process).await;
                    Self::cleanup_temporary_output(&request);
                    return Err(FinalSubError::EngineNotReady(format!(
                        "本地 TTS worker 异常退出；可直接重试：{error}"
                    )));
                }
            };
            if response.protocol_version != PROTOCOL_VERSION
                || response.request_id != request_id
                || response.worker_pid == std::process::id()
            {
                Self::terminate(&mut process).await;
                Self::cleanup_temporary_output(&request);
                return Err(FinalSubError::EngineNotReady(
                    "本地 TTS worker 返回了无效响应；可直接重试".into(),
                ));
            }
            if response.ok {
                return response.result.ok_or_else(|| {
                    FinalSubError::EngineNotReady("本地 TTS worker 未返回音频结果".into())
                });
            }
            return Err(FinalSubError::Worker(
                response.error.unwrap_or_else(|| "本地 TTS 合成失败".into()),
            ));
        }
    }

    async fn stop_process(process: &mut Option<TtsWorkerProcess>) {
        if let Some(worker) = process.as_mut() {
            let request_id = uuid::Uuid::new_v4().to_string();
            let dispose = TtsWorkerRequest::Dispose {
                protocol_version: PROTOCOL_VERSION,
                request_id: request_id.clone(),
            };
            let graceful = Self::send_request(worker, &dispose).await.is_ok()
                && matches!(
                    tokio::time::timeout(WORKER_STOP_TIMEOUT, Self::read_response(worker)).await,
                    Ok(Ok(response))
                        if response.ok
                            && response.request_id == request_id
                            && response.protocol_version == PROTOCOL_VERSION
                );
            if graceful {
                let _ = tokio::time::timeout(WORKER_STOP_TIMEOUT, worker.child.wait()).await;
                *process = None;
                return;
            }
        }
        Self::terminate(process).await;
    }

    pub(crate) async fn trim_idle_workers(&self) {
        for process in self.processes.iter().skip(1) {
            if let Ok(mut process) = process.try_lock() {
                Self::stop_process(&mut process).await;
            }
        }
        self.next_slot.store(0, Ordering::Relaxed);
    }

    pub(crate) async fn stop(&self) {
        let Ok(_capacity) = self
            .capacity
            .clone()
            .acquire_many_owned(MAX_TTS_WORKERS as u32)
            .await
        else {
            return;
        };
        for process in self.processes.iter() {
            let mut process = process.lock().await;
            Self::stop_process(&mut process).await;
        }
        self.next_slot.store(0, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worker_mode_requires_the_exact_private_argument() {
        assert!(is_worker_invocation(["finalsub", TTS_WORKER_ARG]));
        assert!(!is_worker_invocation(["finalsub"]));
        assert!(!is_worker_invocation([
            "finalsub",
            "--finalsub-tts-worker-extra"
        ]));
        assert!(!is_worker_invocation([
            "finalsub",
            TTS_WORKER_ARG,
            "unexpected"
        ]));
    }

    #[test]
    fn protocol_rejects_wrong_version_and_control_ids() {
        let wrong_version = TtsWorkerRequest::Ping {
            protocol_version: PROTOCOL_VERSION + 1,
            request_id: "ping".into(),
        };
        assert!(validate_request_envelope(&wrong_version).is_err());
        let control_id = TtsWorkerRequest::Ping {
            protocol_version: PROTOCOL_VERSION,
            request_id: "bad\nrequest".into(),
        };
        assert!(validate_request_envelope(&control_id).is_err());
    }

    #[test]
    fn protocol_reader_bounds_and_drains_an_oversized_line() {
        let mut input = vec![b'x'; MAX_PROTOCOL_LINE_BYTES + 1];
        input.extend_from_slice(b"\n{}\n");
        let mut cursor = std::io::Cursor::new(input);
        let mut buffer = Vec::new();

        assert_eq!(
            read_protocol_line(&mut cursor, &mut buffer).unwrap(),
            ProtocolLine::TooLarge
        );
        assert!(buffer.is_empty());
        assert_eq!(
            read_protocol_line(&mut cursor, &mut buffer).unwrap(),
            ProtocolLine::Data
        );
        assert_eq!(buffer, b"{}");
    }

    #[test]
    fn manager_reserves_three_lazy_worker_slots() {
        let manager = TtsWorkerManager::default();
        assert_eq!(manager.processes.len(), MAX_TTS_WORKERS);
        assert_eq!(manager.capacity.available_permits(), MAX_TTS_WORKERS);
    }
}
