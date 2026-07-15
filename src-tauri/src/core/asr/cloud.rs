use async_trait::async_trait;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use hmac::{Hmac, Mac};
use reqwest::multipart::{Form, Part};
use reqwest::{Client, StatusCode, Url};
use serde::{Deserialize, Serialize};
use sha1::Sha1;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex as StdMutex, OnceLock, Weak};
use std::time::Duration;

use super::vad::{detect_speech, SpeechSlice, SAMPLE_RATE};
use super::{AsrCapabilities, AsrEngine, AsrModelRef, ProgressSink, ProgressUpdate, TranscribeJob};
use crate::core::subtitle::{Cue, SubtitleTrack};
use crate::error::{FinalSubError, Result};

pub const CLOUD_ASR_ENGINE_ID: &str = "cloud-asr";
pub const CLOUD_ASR_MODEL_ID: &str = "openai-compatible";
pub const CLOUD_ASR_SECRET_PROVIDER: &str = "cloud-asr-openai-compatible";
pub const CLOUD_ASR_ELEVENLABS_SECRET_PROVIDER: &str = "cloud-asr-elevenlabs";
pub const CLOUD_ASR_DEEPGRAM_SECRET_PROVIDER: &str = "cloud-asr-deepgram";
pub const CLOUD_ASR_GLADIA_SECRET_PROVIDER: &str = "cloud-asr-gladia";
pub const CLOUD_ASR_VOLCENGINE_SECRET_PROVIDER: &str = "cloud-asr-volcengine";
pub const CLOUD_ASR_TENCENT_SECRET_PROVIDER: &str = "cloud-asr-tencent";
pub const CLOUD_ASR_ALIYUN_SECRET_PROVIDER: &str = "cloud-asr-aliyun";
pub const CLOUD_ASR_XFYUN_SECRET_PROVIDER: &str = "cloud-asr-xfyun";

const MAX_CHUNK_SECONDS: usize = 300;
const MAX_PROVIDER_ERROR_CHARS: usize = 2_048;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CloudAsrProtocol {
    OpenAiCompatible,
    ElevenLabs,
    Deepgram,
    Gladia,
    Volcengine,
    Tencent,
    Aliyun,
    Xfyun,
}

impl CloudAsrProtocol {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::OpenAiCompatible => "openai-compatible",
            Self::ElevenLabs => "elevenlabs",
            Self::Deepgram => "deepgram",
            Self::Gladia => "gladia",
            Self::Volcengine => "volcengine",
            Self::Tencent => "tencent",
            Self::Aliyun => "aliyun",
            Self::Xfyun => "xfyun",
        }
    }

    pub const fn secret_provider(self) -> &'static str {
        match self {
            Self::OpenAiCompatible => CLOUD_ASR_SECRET_PROVIDER,
            Self::ElevenLabs => CLOUD_ASR_ELEVENLABS_SECRET_PROVIDER,
            Self::Deepgram => CLOUD_ASR_DEEPGRAM_SECRET_PROVIDER,
            Self::Gladia => CLOUD_ASR_GLADIA_SECRET_PROVIDER,
            Self::Volcengine => CLOUD_ASR_VOLCENGINE_SECRET_PROVIDER,
            Self::Tencent => CLOUD_ASR_TENCENT_SECRET_PROVIDER,
            Self::Aliyun => CLOUD_ASR_ALIYUN_SECRET_PROVIDER,
            Self::Xfyun => CLOUD_ASR_XFYUN_SECRET_PROVIDER,
        }
    }

    pub const fn display_name(self) -> &'static str {
        match self {
            Self::OpenAiCompatible => "OpenAI Compatible",
            Self::ElevenLabs => "ElevenLabs",
            Self::Deepgram => "Deepgram",
            Self::Gladia => "Gladia",
            Self::Volcengine => "Volcengine",
            Self::Tencent => "Tencent Cloud",
            Self::Aliyun => "Alibaba Cloud",
            Self::Xfyun => "iFlytek",
        }
    }

    pub const fn required_secret_fields(self) -> &'static [&'static str] {
        match self {
            Self::Tencent | Self::Aliyun | Self::Xfyun => &["accountId", "apiKey", "apiSecret"],
            _ => &["apiKey"],
        }
    }
}

pub fn parse_protocol(raw: &str) -> Result<CloudAsrProtocol> {
    match raw.trim() {
        "openai-compatible" => Ok(CloudAsrProtocol::OpenAiCompatible),
        "elevenlabs" => Ok(CloudAsrProtocol::ElevenLabs),
        "deepgram" => Ok(CloudAsrProtocol::Deepgram),
        "gladia" => Ok(CloudAsrProtocol::Gladia),
        "volcengine" => Ok(CloudAsrProtocol::Volcengine),
        "tencent" => Ok(CloudAsrProtocol::Tencent),
        "aliyun" => Ok(CloudAsrProtocol::Aliyun),
        "xfyun" => Ok(CloudAsrProtocol::Xfyun),
        value => Err(FinalSubError::Validation(format!(
            "不支持的云端 ASR 协议：{value}"
        ))),
    }
}

pub fn secret_provider_for_protocol(raw: &str) -> Result<&'static str> {
    Ok(parse_protocol(raw)?.secret_provider())
}

#[derive(Clone)]
pub struct CloudAsrConfig {
    pub protocol: CloudAsrProtocol,
    pub endpoint: String,
    pub model: String,
    pub api_key: String,
    pub api_secret: Option<String>,
    pub account_id: Option<String>,
    pub timeout_seconds: u32,
    pub retry_times: u32,
    pub request_concurrency: u32,
    pub request_interval_ms: u64,
    pub proxy_url: Option<String>,
    pub state_dir: Option<PathBuf>,
}

pub struct CloudAsrEngine {
    config: CloudAsrConfig,
    transcription_url: Url,
    client: Client,
    vad_model_path: PathBuf,
    aliyun_token: tokio::sync::Mutex<Option<AliyunToken>>,
    request_gate: Arc<ProviderRequestGate>,
}

#[derive(Default)]
struct ProviderRequestGateState {
    in_flight: usize,
    last_started: Option<tokio::time::Instant>,
}

#[derive(Default)]
struct ProviderRequestGate {
    state: StdMutex<ProviderRequestGateState>,
    notify: tokio::sync::Notify,
}

struct ProviderRequestPermit {
    gate: Arc<ProviderRequestGate>,
}

impl Drop for ProviderRequestPermit {
    fn drop(&mut self) {
        let mut state = lock_unpoisoned(&self.gate.state);
        state.in_flight = state.in_flight.saturating_sub(1);
        drop(state);
        self.gate.notify.notify_waiters();
    }
}

impl ProviderRequestGate {
    async fn acquire(
        self: &Arc<Self>,
        concurrency: u32,
        interval_ms: u64,
        cancel_rx: Option<&mut tokio::sync::watch::Receiver<bool>>,
    ) -> Result<ProviderRequestPermit> {
        let limit = concurrency.max(1) as usize;
        let interval = Duration::from_millis(interval_ms);
        let mut cancel_rx = cancel_rx;
        loop {
            if cancel_rx
                .as_ref()
                .map(|receiver| *receiver.borrow())
                .unwrap_or(false)
            {
                return Err(FinalSubError::Validation("任务已取消".into()));
            }

            // Register before checking the state so a permit release cannot be lost
            // between the check and the asynchronous wait.
            let notified = self.notify.notified();
            tokio::pin!(notified);
            let wait_until = {
                let mut state = lock_unpoisoned(&self.state);
                let now = tokio::time::Instant::now();
                let earliest_start = state
                    .last_started
                    .map(|started| started + interval)
                    .unwrap_or(now);
                if state.in_flight < limit && now >= earliest_start {
                    state.in_flight += 1;
                    state.last_started = Some(now);
                    return Ok(ProviderRequestPermit { gate: self.clone() });
                }
                (state.in_flight < limit).then_some(earliest_start)
            };

            match (wait_until, cancel_rx.as_deref_mut()) {
                (Some(deadline), Some(receiver)) => {
                    let delay = tokio::time::sleep_until(deadline);
                    tokio::pin!(delay);
                    tokio::select! {
                        _ = &mut notified => {}
                        _ = &mut delay => {}
                        changed = receiver.changed() => {
                            if changed.is_err() || *receiver.borrow() {
                                return Err(FinalSubError::Validation("任务已取消".into()));
                            }
                        }
                    }
                }
                (Some(deadline), None) => {
                    let delay = tokio::time::sleep_until(deadline);
                    tokio::pin!(delay);
                    tokio::select! {
                        _ = &mut notified => {}
                        _ = &mut delay => {}
                    }
                }
                (None, Some(receiver)) => {
                    tokio::select! {
                        _ = &mut notified => {}
                        changed = receiver.changed() => {
                            if changed.is_err() || *receiver.borrow() {
                                return Err(FinalSubError::Validation("任务已取消".into()));
                            }
                        }
                    }
                }
                (None, None) => notified.await,
            }
        }
    }
}

static PROVIDER_REQUEST_GATES: OnceLock<StdMutex<HashMap<String, Weak<ProviderRequestGate>>>> =
    OnceLock::new();

fn lock_unpoisoned<T>(mutex: &StdMutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn provider_request_gate(
    protocol: CloudAsrProtocol,
    transcription_url: &Url,
) -> Arc<ProviderRequestGate> {
    let key = format!("{}\0{}", protocol.as_str(), transcription_url.as_str());
    let registry = PROVIDER_REQUEST_GATES.get_or_init(|| StdMutex::new(HashMap::new()));
    let mut registry = lock_unpoisoned(registry);
    registry.retain(|_, gate| gate.strong_count() > 0);
    if let Some(gate) = registry.get(&key).and_then(Weak::upgrade) {
        return gate;
    }
    let gate = Arc::new(ProviderRequestGate::default());
    registry.insert(key, Arc::downgrade(&gate));
    gate
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PendingCloudJob {
    protocol: String,
    id: String,
    signature_random: Option<String>,
    created_at: i64,
}

impl CloudAsrEngine {
    pub fn new(config: CloudAsrConfig, vad_model_path: PathBuf) -> Result<Self> {
        validate_config(&config)?;
        if !vad_model_path.is_file() {
            return Err(FinalSubError::Validation(format!(
                "云端 ASR 的内置 Silero VAD 资源缺失：{}",
                vad_model_path.display()
            )));
        }
        let transcription_url =
            normalize_transcription_url_for_protocol(config.protocol, &config.endpoint)?;
        let request_gate = provider_request_gate(config.protocol, &transcription_url);
        let mut builder = Client::builder()
            .connect_timeout(Duration::from_secs(15))
            .timeout(Duration::from_secs(config.timeout_seconds as u64))
            .redirect(reqwest::redirect::Policy::none())
            .user_agent("FinalSub cloud-asr/1");
        if let Some(proxy_url) = config
            .proxy_url
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            let proxy = reqwest::Proxy::all(proxy_url).map_err(|error| {
                FinalSubError::Validation(format!("云端 ASR 代理 URL 无效：{error}"))
            })?;
            builder = builder.proxy(proxy);
        }
        let client = builder.build().map_err(|error| {
            FinalSubError::Validation(format!("创建云端 ASR HTTP 客户端失败：{error}"))
        })?;
        Ok(Self {
            config,
            transcription_url,
            client,
            vad_model_path,
            aliyun_token: tokio::sync::Mutex::new(None),
            request_gate,
        })
    }

    fn pending_state_path(&self, audio: &[u8]) -> Option<PathBuf> {
        let state_dir = self.config.state_dir.as_ref()?;
        let mut hasher = Sha256::new();
        hasher.update(self.config.protocol.as_str().as_bytes());
        hasher.update([0]);
        hasher.update(self.config.endpoint.as_bytes());
        hasher.update([0]);
        hasher.update(self.config.model.as_bytes());
        hasher.update([0]);
        hasher.update(audio);
        Some(state_dir.join(format!("{}.json", hex::encode(hasher.finalize()))))
    }

    async fn load_pending_job(
        &self,
        audio: &[u8],
    ) -> std::result::Result<Option<PendingCloudJob>, CloudRequestError> {
        let Some(path) = self.pending_state_path(audio) else {
            return Ok(None);
        };
        let content = match tokio::fs::read_to_string(&path).await {
            Ok(content) => content,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => {
                return Err(CloudRequestError::Transport(format!(
                    "读取云端 ASR 续查状态失败：{error}"
                )))
            }
        };
        let job = match serde_json::from_str::<PendingCloudJob>(&content) {
            Ok(job) => job,
            Err(_) => {
                let _ = tokio::fs::remove_file(&path).await;
                return Ok(None);
            }
        };
        if job.protocol != self.config.protocol.as_str()
            || chrono::Utc::now()
                .timestamp()
                .saturating_sub(job.created_at)
                > 72 * 60 * 60
        {
            let _ = tokio::fs::remove_file(path).await;
            return Ok(None);
        }
        let valid = match self.config.protocol {
            CloudAsrProtocol::Gladia => {
                job.signature_random.is_none() && validate_gladia_job_id(&job.id).is_ok()
            }
            CloudAsrProtocol::Xfyun => {
                validate_xfyun_order_id(&job.id).is_ok()
                    && job
                        .signature_random
                        .as_deref()
                        .is_some_and(|value| validate_xfyun_signature_random(value).is_ok())
            }
            _ => false,
        };
        if !valid {
            let _ = tokio::fs::remove_file(path).await;
            return Ok(None);
        }
        Ok(Some(job))
    }

    async fn save_pending_job(
        &self,
        audio: &[u8],
        job: &PendingCloudJob,
    ) -> std::result::Result<(), CloudRequestError> {
        let Some(path) = self.pending_state_path(audio) else {
            return Ok(());
        };
        let parent = path.parent().ok_or_else(|| {
            CloudRequestError::InvalidResponse("云端 ASR 续查路径无父目录".into())
        })?;
        tokio::fs::create_dir_all(parent).await.map_err(|error| {
            CloudRequestError::Transport(format!("创建云端 ASR 续查目录失败：{error}"))
        })?;
        let temp = parent.join(format!(".pending-{}.tmp", uuid::Uuid::new_v4()));
        let content = serde_json::to_vec(job).map_err(|error| {
            CloudRequestError::InvalidResponse(format!("序列化云端 ASR 续查状态失败：{error}"))
        })?;
        if let Err(error) = tokio::fs::write(&temp, content).await {
            let _ = tokio::fs::remove_file(&temp).await;
            return Err(CloudRequestError::Transport(format!(
                "写入云端 ASR 续查状态失败：{error}"
            )));
        }
        if let Err(error) = tokio::fs::rename(&temp, &path).await {
            let _ = tokio::fs::remove_file(&temp).await;
            return Err(CloudRequestError::Transport(format!(
                "提交云端 ASR 续查状态失败：{error}"
            )));
        }
        Ok(())
    }

    async fn remove_pending_job(&self, audio: &[u8]) {
        if let Some(path) = self.pending_state_path(audio) {
            let _ = tokio::fs::remove_file(path).await;
        }
    }

    async fn send_openai_request(
        &self,
        audio: &[u8],
        language: Option<&str>,
        verbose: bool,
    ) -> std::result::Result<CloudResponse, CloudRequestError> {
        let file = Part::bytes(audio.to_vec())
            .file_name("finalsub-chunk.wav")
            .mime_str("audio/wav")
            .map_err(|error| CloudRequestError::Transport(error.to_string()))?;
        let mut form = Form::new()
            .part("file", file)
            .text("model", self.config.model.clone())
            .text(
                "response_format",
                if verbose { "verbose_json" } else { "json" },
            );
        if verbose {
            form = form
                .text("timestamp_granularities[]", "word")
                .text("timestamp_granularities[]", "segment");
        }
        if let Some(language) = normalize_language(language) {
            form = form.text("language", language);
        }

        let response = self
            .client
            .post(self.transcription_url.clone())
            .bearer_auth(&self.config.api_key)
            .multipart(form)
            .send()
            .await
            .map_err(|error| CloudRequestError::Transport(error.to_string()))?;
        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|error| CloudRequestError::Transport(error.to_string()))?;
        if !status.is_success() {
            return Err(CloudRequestError::Http {
                status,
                body: sanitize_provider_error(&body, &self.config.api_key),
            });
        }
        serde_json::from_str::<CloudResponse>(&body).map_err(|error| {
            CloudRequestError::InvalidResponse(format!("服务返回的 JSON 无法解析：{error}"))
        })
    }

    async fn send_elevenlabs_request(
        &self,
        audio: &[u8],
        language: Option<&str>,
    ) -> std::result::Result<CloudResponse, CloudRequestError> {
        let file = Part::bytes(audio.to_vec())
            .file_name("finalsub-chunk.wav")
            .mime_str("audio/wav")
            .map_err(|error| CloudRequestError::Transport(error.to_string()))?;
        let mut form = Form::new()
            .part("file", file)
            .text("model_id", self.config.model.clone())
            .text("timestamps_granularity", "word")
            .text("tag_audio_events", "false");
        if let Some(language) = normalize_language(language) {
            form = form.text("language_code", language);
        }
        let response = self
            .client
            .post(self.transcription_url.clone())
            .header("xi-api-key", &self.config.api_key)
            .multipart(form)
            .send()
            .await
            .map_err(|error| CloudRequestError::Transport(error.to_string()))?;
        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|error| CloudRequestError::Transport(error.to_string()))?;
        if !status.is_success() {
            return Err(CloudRequestError::Http {
                status,
                body: sanitize_provider_error(&body, &self.config.api_key),
            });
        }
        let response = serde_json::from_str::<ElevenLabsResponse>(&body).map_err(|error| {
            CloudRequestError::InvalidResponse(format!("ElevenLabs 返回的 JSON 无法解析：{error}"))
        })?;
        Ok(response.into_cloud_response())
    }

    async fn send_deepgram_request(
        &self,
        audio: &[u8],
        language: Option<&str>,
    ) -> std::result::Result<CloudResponse, CloudRequestError> {
        let mut url = self.transcription_url.clone();
        {
            let mut query = url.query_pairs_mut();
            query
                .append_pair("model", &self.config.model)
                .append_pair("smart_format", "true")
                .append_pair("utterances", "true");
            if let Some(language) = normalize_language(language) {
                query.append_pair("language", &language);
            } else {
                query.append_pair("detect_language", "true");
            }
        }
        let response = self
            .client
            .post(url)
            .header("Authorization", format!("Token {}", self.config.api_key))
            .header("Content-Type", "audio/wav")
            .body(audio.to_vec())
            .send()
            .await
            .map_err(|error| CloudRequestError::Transport(error.to_string()))?;
        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|error| CloudRequestError::Transport(error.to_string()))?;
        if !status.is_success() {
            return Err(CloudRequestError::Http {
                status,
                body: sanitize_provider_error(&body, &self.config.api_key),
            });
        }
        let response = serde_json::from_str::<DeepgramResponse>(&body).map_err(|error| {
            CloudRequestError::InvalidResponse(format!("Deepgram 返回的 JSON 无法解析：{error}"))
        })?;
        Ok(response.into_cloud_response())
    }

    async fn send_volcengine_request(
        &self,
        audio: &[u8],
    ) -> std::result::Result<CloudResponse, CloudRequestError> {
        let payload = serde_json::json!({
            "user": { "uid": "finalsub" },
            "audio": { "data": BASE64.encode(audio) },
            "request": {
                "model_name": self.config.model.clone(),
                "enable_punc": true,
                "enable_itn": true,
                "enable_ddc": false,
                "show_utterances": true,
            },
        });
        let mut attempt = 0_u32;
        loop {
            let response = match self
                .client
                .post(self.transcription_url.clone())
                .header("X-Api-Key", &self.config.api_key)
                .header("X-Api-Resource-Id", "volc.bigasr.auc_turbo")
                .header("X-Api-Request-Id", uuid::Uuid::new_v4().to_string())
                .header("X-Api-Sequence", "-1")
                .json(&payload)
                .send()
                .await
            {
                Ok(response) => response,
                Err(_error) if attempt < self.config.retry_times => {
                    let delay_ms = 250_u64.saturating_mul(1_u64 << attempt.min(4));
                    attempt += 1;
                    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                    continue;
                }
                Err(error) => return Err(CloudRequestError::Transport(error.to_string())),
            };
            let http_status = response.status();
            let api_status = response
                .headers()
                .get("x-api-status-code")
                .and_then(|value| value.to_str().ok())
                .unwrap_or_default()
                .to_string();
            let api_message = response
                .headers()
                .get("x-api-message")
                .and_then(|value| value.to_str().ok())
                .unwrap_or_default()
                .to_string();
            let body = response
                .text()
                .await
                .map_err(|error| CloudRequestError::Transport(error.to_string()))?;
            if api_status == "20000000" {
                let response =
                    serde_json::from_str::<VolcengineResponse>(&body).map_err(|error| {
                        CloudRequestError::InvalidResponse(format!(
                            "火山引擎返回的 JSON 无法解析：{error}"
                        ))
                    })?;
                return Ok(response.into_cloud_response());
            }
            if api_status == "20000003" {
                return Ok(CloudResponse::default());
            }
            let retryable = http_status == StatusCode::REQUEST_TIMEOUT
                || http_status == StatusCode::TOO_MANY_REQUESTS
                || http_status.is_server_error()
                || api_status.starts_with("550");
            if retryable && attempt < self.config.retry_times {
                let delay_ms = 250_u64.saturating_mul(1_u64 << attempt.min(4));
                attempt += 1;
                tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                continue;
            }
            let details = sanitize_provider_error(
                &format!("code {api_status} {api_message} {body}"),
                &self.config.api_key,
            );
            if http_status.is_success() {
                return Err(CloudRequestError::InvalidResponse(format!(
                    "火山引擎识别失败：{details}"
                )));
            }
            return Err(CloudRequestError::Http {
                status: http_status,
                body: details,
            });
        }
    }

    async fn send_tencent_request(
        &self,
        audio: &[u8],
        language: Option<&str>,
    ) -> std::result::Result<CloudResponse, CloudRequestError> {
        let app_id = self
            .config
            .account_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| CloudRequestError::InvalidResponse("腾讯云 AppID 未配置".into()))?;
        let secret_key = self
            .config
            .api_secret
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| CloudRequestError::InvalidResponse("腾讯云 SecretKey 未配置".into()))?;
        let engine_type = resolve_tencent_engine_type(&self.config.model, language)?;
        let mut attempt = 0_u32;
        loop {
            let timestamp = chrono::Utc::now().timestamp();
            let query = build_tencent_query(&self.config.api_key, &engine_type, timestamp)?;
            let authorization = sign_tencent_request(secret_key, app_id, &query)?;
            let mut url = self.transcription_url.clone();
            url.path_segments_mut()
                .map_err(|_| CloudRequestError::InvalidResponse("腾讯云 endpoint 路径无效".into()))?
                .push(app_id);
            url.set_query(Some(&query));
            let response = match self
                .client
                .post(url)
                .header("Authorization", authorization)
                .header("Content-Type", "application/octet-stream")
                .body(audio.to_vec())
                .send()
                .await
            {
                Ok(response) => response,
                Err(_error) if attempt < self.config.retry_times => {
                    let delay_ms = 250_u64.saturating_mul(1_u64 << attempt.min(4));
                    attempt += 1;
                    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                    continue;
                }
                Err(error) => return Err(CloudRequestError::Transport(error.to_string())),
            };
            let status = response.status();
            let body = response
                .text()
                .await
                .map_err(|error| CloudRequestError::Transport(error.to_string()))?;
            let parsed = serde_json::from_str::<TencentResponse>(&body);
            let code = parsed.as_ref().ok().and_then(TencentResponse::code);
            if code == Some(0) {
                return Ok(parsed
                    .map_err(|error| {
                        CloudRequestError::InvalidResponse(format!(
                            "腾讯云返回的 JSON 无法解析：{error}"
                        ))
                    })?
                    .into_cloud_response());
            }
            let retryable_code =
                code.is_some_and(|value| matches!(value, 4006 | 4008 | 4009 | 5001 | 5002 | 5003));
            if (retryable_code
                || status == StatusCode::TOO_MANY_REQUESTS
                || status.is_server_error())
                && attempt < self.config.retry_times
            {
                let delay_ms = 250_u64.saturating_mul(1_u64 << attempt.min(4));
                attempt += 1;
                tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                continue;
            }
            let message = parsed
                .as_ref()
                .ok()
                .map(|response| response.message.as_str())
                .unwrap_or_default();
            let mut details = sanitize_provider_error(
                &format!("code {} {message} {body}", code.unwrap_or(-1)),
                &self.config.api_key,
            );
            details = details.replace(secret_key, "[redacted]");
            if status.is_success() {
                return Err(CloudRequestError::InvalidResponse(format!(
                    "腾讯云识别失败：{details}"
                )));
            }
            return Err(CloudRequestError::Http {
                status,
                body: details,
            });
        }
    }

    fn aliyun_token_url(&self) -> std::result::Result<Url, CloudRequestError> {
        #[cfg(test)]
        {
            let mut url = self.transcription_url.clone();
            url.set_path("/token");
            url.set_query(None);
            Ok(url)
        }
        #[cfg(not(test))]
        {
            Url::parse("https://nls-meta.cn-shanghai.aliyuncs.com/").map_err(|error| {
                CloudRequestError::InvalidResponse(format!("阿里云 Token endpoint 无效：{error}"))
            })
        }
    }

    async fn get_aliyun_token(
        &self,
        force_refresh: bool,
    ) -> std::result::Result<String, CloudRequestError> {
        if force_refresh {
            *self.aliyun_token.lock().await = None;
        } else if let Some(token) = self.aliyun_token.lock().await.as_ref() {
            if chrono::Utc::now().timestamp() < token.expire_time.saturating_sub(300) {
                return Ok(token.value.clone());
            }
        }
        let access_key_secret = self
            .config
            .api_secret
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                CloudRequestError::InvalidResponse("阿里云 AccessKey Secret 未配置".into())
            })?;
        let timestamp = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
        let query = build_aliyun_token_query(
            self.config.api_key.trim(),
            &uuid::Uuid::new_v4().to_string(),
            &timestamp,
        );
        let signature = sign_aliyun_token_request(access_key_secret, &query)?;
        let mut url = self.aliyun_token_url()?;
        url.set_query(Some(&format!(
            "Signature={}&{query}",
            percent_encode_rfc3986(&signature)
        )));
        let mut attempt = 0_u32;
        let body = loop {
            match self.client.get(url.clone()).send().await {
                Ok(response) => {
                    let status = response.status();
                    let body = response.text().await.map_err(|_| {
                        CloudRequestError::Transport("读取阿里云 CreateToken 响应失败".into())
                    })?;
                    if status.is_success() {
                        break body;
                    }
                    let mut details = sanitize_provider_error(&body, &self.config.api_key);
                    details = details.replace(access_key_secret, "[redacted]");
                    let error = CloudRequestError::Http {
                        status,
                        body: details,
                    };
                    if error.retryable() && attempt < self.config.retry_times {
                        let delay_ms = 250_u64.saturating_mul(1_u64 << attempt.min(4));
                        attempt += 1;
                        tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                        continue;
                    }
                    return Err(error);
                }
                Err(_) if attempt < self.config.retry_times => {
                    let delay_ms = 250_u64.saturating_mul(1_u64 << attempt.min(4));
                    attempt += 1;
                    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                }
                Err(_) => {
                    return Err(CloudRequestError::Transport(
                        "阿里云 CreateToken 网络请求失败".into(),
                    ))
                }
            }
        };
        let response = serde_json::from_str::<AliyunTokenResponse>(&body).map_err(|error| {
            CloudRequestError::InvalidResponse(format!("阿里云 CreateToken 响应无法解析：{error}"))
        })?;
        let token = response.token.ok_or_else(|| {
            CloudRequestError::InvalidResponse("阿里云 CreateToken 未返回 Token".into())
        })?;
        if token.id.trim().is_empty() || token.expire_time <= chrono::Utc::now().timestamp() {
            return Err(CloudRequestError::InvalidResponse(
                "阿里云 CreateToken 返回了空值或过期 Token".into(),
            ));
        }
        *self.aliyun_token.lock().await = Some(AliyunToken {
            value: token.id.clone(),
            expire_time: token.expire_time,
        });
        Ok(token.id)
    }

    async fn send_aliyun_request(
        &self,
        audio: &[u8],
    ) -> std::result::Result<CloudResponse, CloudRequestError> {
        let app_key = self
            .config
            .account_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| CloudRequestError::InvalidResponse("阿里云 Appkey 未配置".into()))?;
        let mut attempt = 0_u32;
        let mut auth_refreshed = false;
        let mut force_token_refresh = false;
        loop {
            let token = self.get_aliyun_token(force_token_refresh).await?;
            force_token_refresh = false;
            let mut url = self.transcription_url.clone();
            url.set_query(Some(&build_aliyun_flash_query(app_key, &token)));
            let response = match self
                .client
                .post(url)
                .header("Content-Type", "application/octet-stream")
                .body(audio.to_vec())
                .send()
                .await
            {
                Ok(response) => response,
                Err(_) if attempt < self.config.retry_times => {
                    let delay_ms = 250_u64.saturating_mul(1_u64 << attempt.min(4));
                    attempt += 1;
                    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                    continue;
                }
                Err(_) => {
                    return Err(CloudRequestError::Transport(
                        "阿里云识别网络请求失败".into(),
                    ))
                }
            };
            let http_status = response.status();
            let body = response
                .text()
                .await
                .map_err(|_| CloudRequestError::Transport("读取阿里云识别响应失败".into()))?;
            let parsed = serde_json::from_str::<AliyunResponse>(&body);
            let status = parsed.as_ref().ok().and_then(AliyunResponse::status);
            if status == Some(20_000_000) {
                return Ok(parsed
                    .map_err(|error| {
                        CloudRequestError::InvalidResponse(format!(
                            "阿里云返回的 JSON 无法解析：{error}"
                        ))
                    })?
                    .into_cloud_response());
            }
            if status == Some(40_270_002) {
                return Ok(CloudResponse::default());
            }
            let auth_error = status.is_some_and(|value| value == 40_000_001 || value == 403)
                || http_status == StatusCode::FORBIDDEN;
            if auth_error && !auth_refreshed {
                auth_refreshed = true;
                force_token_refresh = true;
                continue;
            }
            let retryable_status = status.is_some_and(|value| {
                matches!(
                    value,
                    40_000_004 | 40_000_005 | 50_000_000 | 50_000_001 | 52_010_001
                )
            });
            if (retryable_status
                || http_status == StatusCode::TOO_MANY_REQUESTS
                || http_status.is_server_error())
                && attempt < self.config.retry_times
            {
                let delay_ms = 250_u64.saturating_mul(1_u64 << attempt.min(4));
                attempt += 1;
                tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                continue;
            }
            let message = parsed
                .as_ref()
                .ok()
                .map(|response| response.message.as_str())
                .unwrap_or_default();
            let mut details = sanitize_provider_error(
                &format!("status {} {message} {body}", status.unwrap_or(-1)),
                &self.config.api_key,
            );
            details = details.replace(&token, "[redacted]");
            if let Some(secret) = self.config.api_secret.as_deref() {
                details = details.replace(secret, "[redacted]");
            }
            if http_status.is_success() {
                return Err(CloudRequestError::InvalidResponse(format!(
                    "阿里云识别失败：{details}"
                )));
            }
            return Err(CloudRequestError::Http {
                status: http_status,
                body: details,
            });
        }
    }

    async fn upload_xfyun_order(
        &self,
        audio: &[u8],
        tier: &str,
        signature_random: &str,
    ) -> std::result::Result<XfyunUploadOutcome, CloudRequestError> {
        let app_id = self.xfyun_app_id()?;
        let api_secret = self.xfyun_api_secret()?;
        let mut attempt = 0_u32;
        loop {
            let mut params = std::collections::BTreeMap::new();
            params.insert("appId", app_id.to_string());
            params.insert("accessKeyId", self.config.api_key.trim().to_string());
            params.insert("dateTime", xfyun_datetime());
            params.insert("durationCheckDisable", "true".into());
            params.insert("fileName", "audio.wav".into());
            params.insert("fileSize", audio.len().to_string());
            params.insert("language", tier.to_string());
            params.insert("signatureRandom", signature_random.to_string());
            let query = build_xfyun_query(params);
            let signature = sign_xfyun_request(api_secret, &query)?;
            let mut url = xfyun_request_url(&self.transcription_url, "/v2/upload")?;
            url.set_query(Some(&query));
            let response = match self
                .client
                .post(url)
                .header("Content-Type", "application/octet-stream")
                .header("signature", signature)
                .body(audio.to_vec())
                .send()
                .await
            {
                Ok(response) => response,
                Err(_) if attempt < self.config.retry_times => {
                    let delay_ms = 500_u64.saturating_mul(1_u64 << attempt.min(4));
                    attempt += 1;
                    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                    continue;
                }
                Err(error) => return Err(CloudRequestError::Transport(error.to_string())),
            };
            let http_status = response.status();
            let body = response
                .text()
                .await
                .map_err(|error| CloudRequestError::Transport(error.to_string()))?;
            let parsed = serde_json::from_str::<XfyunResponse>(&body);
            let code = parsed.as_ref().ok().map(XfyunResponse::code);
            match classify_xfyun_code(http_status, code.as_deref()) {
                XfyunCodeClass::Success => {
                    let response = parsed.map_err(|error| {
                        CloudRequestError::InvalidResponse(format!("讯飞上传响应无法解析：{error}"))
                    })?;
                    let content = response.content.ok_or_else(|| {
                        CloudRequestError::InvalidResponse("讯飞上传成功但未返回 content".into())
                    })?;
                    let order_id = content.order_id.trim().to_string();
                    validate_xfyun_order_id(&order_id)?;
                    return Ok(XfyunUploadOutcome {
                        order_id,
                        task_estimate_time_ms: json_number(&content.task_estimate_time).max(0.0)
                            as u64,
                    });
                }
                XfyunCodeClass::Retriable if attempt < self.config.retry_times => {
                    let delay_ms = 500_u64.saturating_mul(1_u64 << attempt.min(4));
                    attempt += 1;
                    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                }
                class => {
                    let details = self.sanitize_xfyun_error(&format!(
                        "HTTP {http_status} code {} {} {body}",
                        code.as_deref().unwrap_or("unknown"),
                        parsed
                            .as_ref()
                            .ok()
                            .map(|response| response.desc_info.as_str())
                            .unwrap_or_default()
                    ));
                    let message = if class == XfyunCodeClass::Auth {
                        format!(
                            "讯飞鉴权失败：{details}。请检查 APPID、APIKey、APISecret 与系统时间"
                        )
                    } else {
                        format!("讯飞上传失败：{details}")
                    };
                    if http_status.is_success() {
                        return Err(CloudRequestError::InvalidResponse(message));
                    }
                    return Err(CloudRequestError::Http {
                        status: http_status,
                        body: message,
                    });
                }
            }
        }
    }

    async fn poll_xfyun_order(
        &self,
        order_id: &str,
        signature_random: &str,
        first_delay_ms: u64,
    ) -> std::result::Result<XfyunPollOutcome, CloudRequestError> {
        validate_xfyun_order_id(order_id)?;
        validate_xfyun_signature_random(signature_random)?;
        let app_id = self.xfyun_app_id()?;
        let api_secret = self.xfyun_api_secret()?;
        if first_delay_ms > 0 {
            tokio::time::sleep(Duration::from_millis(first_delay_ms)).await;
        }
        let deadline = tokio::time::Instant::now() + Duration::from_secs(62 * 60);
        let mut consecutive_failures = 0_u8;
        for query_index in 0..96_usize {
            if tokio::time::Instant::now() >= deadline {
                break;
            }
            let mut params = std::collections::BTreeMap::new();
            params.insert("appId", app_id.to_string());
            params.insert("accessKeyId", self.config.api_key.trim().to_string());
            params.insert("dateTime", xfyun_datetime());
            params.insert("orderId", order_id.to_string());
            params.insert("resultType", "transfer".into());
            params.insert("signatureRandom", signature_random.to_string());
            let query = build_xfyun_query(params);
            let signature = sign_xfyun_request(api_secret, &query)?;
            let mut url = xfyun_request_url(&self.transcription_url, "/v2/getResult")?;
            url.set_query(Some(&query));
            let response = match self
                .client
                .post(url)
                .header("Content-Type", "application/json")
                .header("signature", signature)
                .body("{}")
                .send()
                .await
            {
                Ok(response) => response,
                Err(error) => {
                    consecutive_failures = consecutive_failures.saturating_add(1);
                    if consecutive_failures >= 5 {
                        return Err(CloudRequestError::Transport(format!(
                            "讯飞订单连续 5 次查询失败：{error}"
                        )));
                    }
                    tokio::time::sleep(Duration::from_millis(xfyun_poll_interval_ms(query_index)))
                        .await;
                    continue;
                }
            };
            let http_status = response.status();
            let body = response
                .text()
                .await
                .map_err(|error| CloudRequestError::Transport(error.to_string()))?;
            let parsed = serde_json::from_str::<XfyunResponse>(&body);
            let code = parsed.as_ref().ok().map(XfyunResponse::code);
            if code.as_deref().is_some_and(is_xfyun_order_gone) {
                return Ok(XfyunPollOutcome::OrderGone);
            }
            if code.as_deref() == Some("100013") {
                consecutive_failures = 0;
            } else {
                match classify_xfyun_code(http_status, code.as_deref()) {
                    XfyunCodeClass::Success => {
                        consecutive_failures = 0;
                        let response = parsed.map_err(|error| {
                            CloudRequestError::InvalidResponse(format!(
                                "讯飞查询响应无法解析：{error}"
                            ))
                        })?;
                        let content = response.content.ok_or_else(|| {
                            CloudRequestError::InvalidResponse(
                                "讯飞查询成功但未返回 content".into(),
                            )
                        })?;
                        let status = content
                            .order_info
                            .as_ref()
                            .map(|info| json_number(&info.status) as i64)
                            .unwrap_or(0);
                        let fail_type = content
                            .order_info
                            .as_ref()
                            .map(|info| json_number(&info.fail_type) as i64)
                            .unwrap_or(0);
                        match status {
                            0 | 3 => {}
                            4 if fail_type == 0 => {
                                return Ok(XfyunPollOutcome::Done(extract_xfyun_result(
                                    &content.order_result,
                                )))
                            }
                            4 | -1 if fail_type == 6 => {
                                return Ok(XfyunPollOutcome::Done(CloudResponse::default()))
                            }
                            4 | -1 => {
                                return Err(CloudRequestError::FinalJob(format!(
                                    "讯飞订单失败：{}",
                                    describe_xfyun_fail_type(fail_type)
                                )))
                            }
                            other => {
                                return Err(CloudRequestError::FinalJob(format!(
                                    "讯飞返回未知订单状态：{other}"
                                )))
                            }
                        }
                    }
                    XfyunCodeClass::Auth => {
                        return Err(CloudRequestError::InvalidResponse(format!(
                            "讯飞鉴权失败：{}。请检查 APPID、APIKey、APISecret 与系统时间",
                            self.sanitize_xfyun_error(&format!(
                                "code {} {} {body}",
                                code.as_deref().unwrap_or("unknown"),
                                parsed
                                    .as_ref()
                                    .ok()
                                    .map(|response| response.desc_info.as_str())
                                    .unwrap_or_default()
                            ))
                        )))
                    }
                    XfyunCodeClass::Retriable => {
                        consecutive_failures = consecutive_failures.saturating_add(1);
                        if consecutive_failures >= 5 {
                            return Err(CloudRequestError::Transport(
                                "讯飞订单连续 5 次返回可重试错误".into(),
                            ));
                        }
                    }
                    XfyunCodeClass::Fatal => {
                        return Err(CloudRequestError::FinalJob(format!(
                            "讯飞查询失败：{}",
                            self.sanitize_xfyun_error(&format!(
                                "HTTP {http_status} code {} {} {body}",
                                code.as_deref().unwrap_or("unknown"),
                                parsed
                                    .as_ref()
                                    .ok()
                                    .map(|response| response.desc_info.as_str())
                                    .unwrap_or_default()
                            ))
                        )))
                    }
                }
            }
            tokio::time::sleep(Duration::from_millis(xfyun_poll_interval_ms(query_index))).await;
        }
        Err(CloudRequestError::Transport(
            "讯飞订单仍在处理中；稍后重跑任务将续查原订单，不会重新上传".into(),
        ))
    }

    async fn send_xfyun_request(
        &self,
        audio: &[u8],
        language: Option<&str>,
    ) -> std::result::Result<CloudResponse, CloudRequestError> {
        let tier = normalize_xfyun_tier(&self.config.model);
        validate_xfyun_language(tier, language)?;
        let mut resumed = self.load_pending_job(audio).await?;
        loop {
            let (order_id, signature_random, first_delay_ms) = if let Some(job) = resumed.as_ref() {
                let signature_random = job.signature_random.clone().ok_or_else(|| {
                    CloudRequestError::InvalidResponse("讯飞续查状态缺少签名随机串".into())
                })?;
                validate_xfyun_order_id(&job.id)?;
                validate_xfyun_signature_random(&signature_random)?;
                (job.id.clone(), signature_random, 0)
            } else {
                let signature_random = build_xfyun_random();
                let upload = self
                    .upload_xfyun_order(audio, tier, &signature_random)
                    .await?;
                self.save_pending_job(
                    audio,
                    &PendingCloudJob {
                        protocol: self.config.protocol.as_str().into(),
                        id: upload.order_id.clone(),
                        signature_random: Some(signature_random.clone()),
                        created_at: chrono::Utc::now().timestamp(),
                    },
                )
                .await?;
                (
                    upload.order_id,
                    signature_random,
                    xfyun_first_delay_ms(upload.task_estimate_time_ms),
                )
            };
            match self
                .poll_xfyun_order(&order_id, &signature_random, first_delay_ms)
                .await
            {
                Ok(XfyunPollOutcome::Done(response)) => {
                    self.remove_pending_job(audio).await;
                    return Ok(response);
                }
                Ok(XfyunPollOutcome::OrderGone) if resumed.is_some() => {
                    self.remove_pending_job(audio).await;
                    resumed = None;
                }
                Ok(XfyunPollOutcome::OrderGone) => {
                    self.remove_pending_job(audio).await;
                    return Err(CloudRequestError::FinalJob(format!(
                        "讯飞新建订单 {order_id} 后立即失效"
                    )));
                }
                Err(CloudRequestError::FinalJob(message)) => {
                    self.remove_pending_job(audio).await;
                    return Err(CloudRequestError::FinalJob(message));
                }
                Err(error) => return Err(error),
            }
        }
    }

    fn xfyun_app_id(&self) -> std::result::Result<&str, CloudRequestError> {
        self.config
            .account_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| CloudRequestError::InvalidResponse("讯飞 APPID 未配置".into()))
    }

    fn xfyun_api_secret(&self) -> std::result::Result<&str, CloudRequestError> {
        self.config
            .api_secret
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| CloudRequestError::InvalidResponse("讯飞 APISecret 未配置".into()))
    }

    fn sanitize_xfyun_error(&self, message: &str) -> String {
        let mut sanitized = sanitize_provider_error(message, &self.config.api_key);
        if let Some(value) = self.config.account_id.as_deref() {
            sanitized = sanitized.replace(value, "[redacted]");
        }
        if let Some(value) = self.config.api_secret.as_deref() {
            sanitized = sanitized.replace(value, "[redacted]");
        }
        sanitized
    }

    async fn send_http_with_retries<F>(
        &self,
        mut build: F,
    ) -> std::result::Result<String, CloudRequestError>
    where
        F: FnMut() -> std::result::Result<reqwest::RequestBuilder, CloudRequestError>,
    {
        let mut attempt = 0_u32;
        loop {
            let result = async {
                let response = build()?
                    .send()
                    .await
                    .map_err(|error| CloudRequestError::Transport(error.to_string()))?;
                let status = response.status();
                let body = response
                    .text()
                    .await
                    .map_err(|error| CloudRequestError::Transport(error.to_string()))?;
                if status.is_success() {
                    Ok(body)
                } else {
                    Err(CloudRequestError::Http {
                        status,
                        body: sanitize_provider_error(&body, &self.config.api_key),
                    })
                }
            }
            .await;
            match result {
                Ok(body) => return Ok(body),
                Err(error) if error.retryable() && attempt < self.config.retry_times => {
                    let delay_ms = 250_u64.saturating_mul(1_u64 << attempt.min(4));
                    attempt += 1;
                    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                }
                Err(error) => return Err(error),
            }
        }
    }

    async fn send_gladia_request(
        &self,
        audio: &[u8],
        language: Option<&str>,
    ) -> std::result::Result<CloudResponse, CloudRequestError> {
        let mut resumed = self.load_pending_job(audio).await?;
        loop {
            let job_id = if let Some(job) = resumed.as_ref() {
                validate_gladia_job_id(&job.id)?;
                job.id.clone()
            } else {
                let upload_url = gladia_upload_url(&self.transcription_url)?;
                let upload_body = self
                    .send_http_with_retries(|| {
                        let file = Part::bytes(audio.to_vec())
                            .file_name("finalsub-chunk.wav")
                            .mime_str("audio/wav")
                            .map_err(|error| CloudRequestError::Transport(error.to_string()))?;
                        Ok(self
                            .client
                            .post(upload_url.clone())
                            .header("x-gladia-key", &self.config.api_key)
                            .multipart(Form::new().part("audio", file)))
                    })
                    .await?;
                let upload = serde_json::from_str::<GladiaUploadResponse>(&upload_body).map_err(
                    |error| {
                        CloudRequestError::InvalidResponse(format!(
                            "Gladia 上传响应无法解析：{error}"
                        ))
                    },
                )?;
                if upload.audio_url.trim().is_empty() {
                    return Err(CloudRequestError::InvalidResponse(
                        "Gladia 上传成功但未返回 audio_url".into(),
                    ));
                }

                let mut payload = serde_json::json!({
                    "audio_url": upload.audio_url,
                    "model": self.config.model.clone(),
                });
                if let Some(language) = normalize_language(language) {
                    payload["language_config"] = serde_json::json!({ "languages": [language] });
                }
                let init_body = self
                    .send_http_with_retries(|| {
                        Ok(self
                            .client
                            .post(self.transcription_url.clone())
                            .header("x-gladia-key", &self.config.api_key)
                            .json(&payload))
                    })
                    .await?;
                let init =
                    serde_json::from_str::<GladiaInitResponse>(&init_body).map_err(|error| {
                        CloudRequestError::InvalidResponse(format!(
                            "Gladia 建立任务响应无法解析：{error}"
                        ))
                    })?;
                validate_gladia_job_id(&init.id)?;
                self.save_pending_job(
                    audio,
                    &PendingCloudJob {
                        protocol: self.config.protocol.as_str().into(),
                        id: init.id.clone(),
                        signature_random: None,
                        created_at: chrono::Utc::now().timestamp(),
                    },
                )
                .await?;
                init.id
            };
            match self.poll_gladia_job(&job_id).await {
                Ok(response) => {
                    self.remove_pending_job(audio).await;
                    return Ok(response);
                }
                Err(CloudRequestError::Http { status, .. })
                    if status == StatusCode::NOT_FOUND && resumed.is_some() =>
                {
                    self.remove_pending_job(audio).await;
                    resumed = None;
                }
                Err(CloudRequestError::Http { status, .. }) if status == StatusCode::NOT_FOUND => {
                    self.remove_pending_job(audio).await;
                    return Err(CloudRequestError::InvalidResponse(format!(
                        "Gladia 新建任务 {job_id} 后立即失效"
                    )));
                }
                Err(CloudRequestError::FinalJob(message)) => {
                    self.remove_pending_job(audio).await;
                    return Err(CloudRequestError::FinalJob(message));
                }
                Err(error) => return Err(error),
            }
        }
    }

    async fn poll_gladia_job(
        &self,
        job_id: &str,
    ) -> std::result::Result<CloudResponse, CloudRequestError> {
        let poll_url = gladia_poll_url(&self.transcription_url, job_id)?;
        let deadline =
            tokio::time::Instant::now() + Duration::from_secs(self.config.timeout_seconds as u64);
        loop {
            if tokio::time::Instant::now() >= deadline {
                return Err(CloudRequestError::Transport(format!(
                    "Gladia 任务在 {} 秒内未完成",
                    self.config.timeout_seconds
                )));
            }
            let poll_body = self
                .send_http_with_retries(|| {
                    Ok(self
                        .client
                        .get(poll_url.clone())
                        .header("x-gladia-key", &self.config.api_key))
                })
                .await?;
            let poll = serde_json::from_str::<GladiaPollResponse>(&poll_body).map_err(|error| {
                CloudRequestError::InvalidResponse(format!("Gladia 轮询响应无法解析：{error}"))
            })?;
            match poll.status.as_str() {
                "done" => {
                    return Ok(poll
                        .result
                        .map(GladiaResult::into_cloud_response)
                        .unwrap_or_default())
                }
                "queued" | "processing" => {
                    tokio::time::sleep(Duration::from_secs(2)).await;
                }
                "error" => {
                    return Err(CloudRequestError::FinalJob(format!(
                        "Gladia 转写任务失败（error_code: {}）",
                        poll.error_code
                            .map(|value| value.to_string())
                            .unwrap_or_else(|| "unknown".into())
                    )))
                }
                status => {
                    return Err(CloudRequestError::FinalJob(format!(
                        "Gladia 返回未知任务状态：{status}"
                    )))
                }
            }
        }
    }

    async fn send_request(
        &self,
        audio: &[u8],
        language: Option<&str>,
        verbose: bool,
    ) -> std::result::Result<CloudResponse, CloudRequestError> {
        match self.config.protocol {
            CloudAsrProtocol::OpenAiCompatible => {
                self.send_openai_request(audio, language, verbose).await
            }
            CloudAsrProtocol::ElevenLabs => self.send_elevenlabs_request(audio, language).await,
            CloudAsrProtocol::Deepgram => self.send_deepgram_request(audio, language).await,
            CloudAsrProtocol::Gladia => self.send_gladia_request(audio, language).await,
            CloudAsrProtocol::Volcengine => self.send_volcengine_request(audio).await,
            CloudAsrProtocol::Tencent => self.send_tencent_request(audio, language).await,
            CloudAsrProtocol::Aliyun => self.send_aliyun_request(audio).await,
            CloudAsrProtocol::Xfyun => self.send_xfyun_request(audio, language).await,
        }
    }

    async fn send_with_retries(
        &self,
        audio: &[u8],
        language: Option<&str>,
        verbose: bool,
    ) -> std::result::Result<CloudResponse, CloudRequestError> {
        let mut attempt = 0_u32;
        loop {
            match self.send_request(audio, language, verbose).await {
                Ok(response) => return Ok(response),
                Err(error) if error.retryable() && attempt < self.config.retry_times => {
                    let delay_ms = 250_u64.saturating_mul(1_u64 << attempt.min(4));
                    attempt += 1;
                    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                }
                Err(error) => return Err(error),
            }
        }
    }

    async fn transcribe_chunk(
        &self,
        audio: &[u8],
        language: Option<&str>,
    ) -> std::result::Result<CloudResponse, CloudRequestError> {
        if self.config.protocol == CloudAsrProtocol::Gladia {
            return self.send_gladia_request(audio, language).await;
        }
        if self.config.protocol == CloudAsrProtocol::Volcengine {
            return self.send_volcengine_request(audio).await;
        }
        if self.config.protocol == CloudAsrProtocol::Tencent {
            return self.send_tencent_request(audio, language).await;
        }
        if self.config.protocol == CloudAsrProtocol::Aliyun {
            return self.send_aliyun_request(audio).await;
        }
        if self.config.protocol == CloudAsrProtocol::Xfyun {
            return self.send_xfyun_request(audio, language).await;
        }
        if self.config.protocol != CloudAsrProtocol::OpenAiCompatible {
            return self.send_with_retries(audio, language, false).await;
        }
        let model = self.config.model.to_ascii_lowercase();
        let try_verbose = !model.starts_with("gpt-4o-");
        if !try_verbose {
            return self.send_with_retries(audio, language, false).await;
        }
        match self.send_with_retries(audio, language, true).await {
            Ok(response) => Ok(response),
            Err(error) if error.verbose_unsupported() => {
                self.send_with_retries(audio, language, false).await
            }
            Err(error) => Err(error),
        }
    }
}

fn validate_config(config: &CloudAsrConfig) -> Result<()> {
    validate_service_settings(
        config.protocol.as_str(),
        &config.endpoint,
        &config.model,
        config.timeout_seconds,
        config.retry_times,
        config.request_concurrency,
        config.request_interval_ms,
    )?;
    if config.api_key.trim().is_empty() {
        return Err(FinalSubError::Validation(
            "云端 ASR API Key 未配置，请先在模型管理中保存密钥".into(),
        ));
    }
    if matches!(
        config.protocol,
        CloudAsrProtocol::Tencent | CloudAsrProtocol::Aliyun | CloudAsrProtocol::Xfyun
    ) && (config
        .api_secret
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .is_none()
        || config
            .account_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .is_none())
    {
        return Err(FinalSubError::Validation(format!(
            "{} ASR 需要三项完整凭据",
            config.protocol.display_name()
        )));
    }
    Ok(())
}

pub fn validate_service_settings(
    protocol: &str,
    endpoint: &str,
    model: &str,
    timeout_seconds: u32,
    retry_times: u32,
    request_concurrency: u32,
    request_interval_ms: u64,
) -> Result<()> {
    let protocol = parse_protocol(protocol)?;
    normalize_transcription_url_for_protocol(protocol, endpoint)?;
    let model = model.trim();
    if model.is_empty() || model.len() > 200 || model.chars().any(char::is_control) {
        return Err(FinalSubError::Validation(
            "云端 ASR 模型名不能为空、不能包含控制字符，且最长为 200 字节".into(),
        ));
    }
    if !(10..=900).contains(&timeout_seconds) {
        return Err(FinalSubError::Validation(
            "云端 ASR 请求超时必须在 10-900 秒之间".into(),
        ));
    }
    if retry_times > 5 {
        return Err(FinalSubError::Validation(
            "云端 ASR 重试次数不能超过 5".into(),
        ));
    }
    if !(1..=8).contains(&request_concurrency) {
        return Err(FinalSubError::Validation(
            "云端 ASR 服务商并发数必须在 1-8 之间".into(),
        ));
    }
    if request_interval_ms > 60_000 {
        return Err(FinalSubError::Validation(
            "云端 ASR 请求间隔不能超过 60000 ms".into(),
        ));
    }
    Ok(())
}

fn normalize_base_url(raw: &str) -> Result<Url> {
    let mut url = Url::parse(raw.trim())
        .map_err(|error| FinalSubError::Validation(format!("云端 ASR endpoint 无效：{error}")))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(FinalSubError::Validation(
            "云端 ASR endpoint 只支持 http:// 或 https://".into(),
        ));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(FinalSubError::Validation(
            "云端 ASR endpoint 不允许在 URL 中包含用户名或密码".into(),
        ));
    }
    if url.host_str().is_none() {
        return Err(FinalSubError::Validation(
            "云端 ASR endpoint 缺少主机名".into(),
        ));
    }
    url.set_query(None);
    url.set_fragment(None);
    Ok(url)
}

pub fn normalize_transcription_url(raw: &str) -> Result<Url> {
    normalize_transcription_url_for_protocol(CloudAsrProtocol::OpenAiCompatible, raw)
}

pub fn normalize_transcription_url_for_protocol(
    protocol: CloudAsrProtocol,
    raw: &str,
) -> Result<Url> {
    let mut url = normalize_base_url(raw)?;
    #[cfg(not(test))]
    if protocol == CloudAsrProtocol::Tencent
        && (url.scheme() != "https"
            || !url
                .host_str()
                .is_some_and(|host| host.eq_ignore_ascii_case("asr.cloud.tencent.com")))
    {
        return Err(FinalSubError::Validation(
            "腾讯云 ASR endpoint 固定为 https://asr.cloud.tencent.com".into(),
        ));
    }
    #[cfg(not(test))]
    if protocol == CloudAsrProtocol::Aliyun
        && (url.scheme() != "https"
            || !url.host_str().is_some_and(|host| {
                host.eq_ignore_ascii_case("nls-gateway-cn-shanghai.aliyuncs.com")
            }))
    {
        return Err(FinalSubError::Validation(
            "阿里云 ASR endpoint 固定为 https://nls-gateway-cn-shanghai.aliyuncs.com".into(),
        ));
    }
    #[cfg(not(test))]
    if protocol == CloudAsrProtocol::Xfyun
        && (url.scheme() != "https"
            || !url
                .host_str()
                .is_some_and(|host| host.eq_ignore_ascii_case("office-api-ist-dx.iflyaisol.com")))
    {
        return Err(FinalSubError::Validation(
            "讯飞 ASR endpoint 固定为 https://office-api-ist-dx.iflyaisol.com".into(),
        ));
    }
    let path = url.path().trim_end_matches('/');
    let (full_suffix, version_suffix) = match protocol {
        CloudAsrProtocol::OpenAiCompatible => ("/audio/transcriptions", None),
        CloudAsrProtocol::ElevenLabs => ("/v1/speech-to-text", Some("/speech-to-text")),
        CloudAsrProtocol::Deepgram => ("/v1/listen", Some("/listen")),
        CloudAsrProtocol::Gladia => ("/v2/pre-recorded", Some("/pre-recorded")),
        CloudAsrProtocol::Volcengine => ("/api/v3/auc/bigmodel/recognize/flash", None),
        CloudAsrProtocol::Tencent => ("/asr/flash/v1", None),
        CloudAsrProtocol::Aliyun => ("/stream/v1/FlashRecognizer", None),
        CloudAsrProtocol::Xfyun => ("/v2/upload", None),
    };
    let next_path = if path.ends_with(full_suffix) {
        path.to_string()
    } else if protocol == CloudAsrProtocol::OpenAiCompatible {
        if path.is_empty() {
            full_suffix.into()
        } else {
            format!("{path}{full_suffix}")
        }
    } else if (matches!(
        protocol,
        CloudAsrProtocol::ElevenLabs | CloudAsrProtocol::Deepgram
    ) && path.ends_with("/v1"))
        || (protocol == CloudAsrProtocol::Gladia && path.ends_with("/v2"))
    {
        format!("{path}{}", version_suffix.expect("cloud protocol suffix"))
    } else if path.is_empty() {
        full_suffix.into()
    } else {
        format!("{path}{full_suffix}")
    };
    url.set_path(&next_path);
    Ok(url)
}

fn gladia_upload_url(transcription_url: &Url) -> std::result::Result<Url, CloudRequestError> {
    let mut url = transcription_url.clone();
    let base = url
        .path()
        .strip_suffix("/v2/pre-recorded")
        .ok_or_else(|| CloudRequestError::InvalidResponse("Gladia endpoint 路径无效".into()))?;
    url.set_path(&format!("{base}/v2/upload"));
    Ok(url)
}

fn validate_gladia_job_id(job_id: &str) -> std::result::Result<(), CloudRequestError> {
    if job_id.is_empty()
        || job_id.len() > 128
        || !job_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    {
        return Err(CloudRequestError::InvalidResponse(
            "Gladia 返回了无效的任务 ID".into(),
        ));
    }
    Ok(())
}

fn gladia_poll_url(
    transcription_url: &Url,
    job_id: &str,
) -> std::result::Result<Url, CloudRequestError> {
    validate_gladia_job_id(job_id)?;
    let mut url = transcription_url.clone();
    url.path_segments_mut()
        .map_err(|_| CloudRequestError::InvalidResponse("Gladia endpoint 路径无效".into()))?
        .push(job_id);
    Ok(url)
}

fn resolve_tencent_engine_type(
    model: &str,
    language: Option<&str>,
) -> std::result::Result<String, CloudRequestError> {
    let model = model.trim();
    if model.starts_with("16k_") || model.starts_with("8k_") {
        return Ok(model.to_string());
    }
    let tier_large = model.eq_ignore_ascii_case("large");
    let language = normalize_language(language).unwrap_or_else(|| "auto".into());
    let engine = match (tier_large, language.as_str()) {
        (false, "auto") => "16k_zh-PY",
        (false, "zh") => "16k_zh",
        (false, "yue") => "16k_yue",
        (false, "en") => "16k_en",
        (false, "ja") => "16k_ja",
        (false, "ko") => "16k_ko",
        (false, "th") => "16k_th",
        (false, "vi") => "16k_vi",
        (false, "id") => "16k_id",
        (false, "ms") => "16k_ms",
        (false, "fil" | "tl") => "16k_fil",
        (false, "pt") => "16k_pt",
        (false, "tr") => "16k_tr",
        (false, "ar") => "16k_ar",
        (false, "es") => "16k_es",
        (false, "hi") => "16k_hi",
        (false, "fr") => "16k_fr",
        (false, "de") => "16k_de",
        (true, "auto" | "zh" | "yue" | "en") => "16k_zh_en",
        (
            true,
            "ja" | "ko" | "th" | "vi" | "id" | "ms" | "fil" | "tl" | "pt" | "tr" | "ar" | "es"
            | "hi" | "fr" | "de",
        ) => "16k_multi_lang",
        _ => {
            return Err(CloudRequestError::InvalidResponse(format!(
                "腾讯云极速版不支持源语言 {language}，请选择受支持语言或改用其他 ASR"
            )))
        }
    };
    Ok(engine.into())
}

fn build_tencent_query(
    secret_id: &str,
    engine_type: &str,
    timestamp: i64,
) -> std::result::Result<String, CloudRequestError> {
    if !secret_id
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        return Err(CloudRequestError::InvalidResponse(
            "腾讯云 SecretID 包含不支持的字符".into(),
        ));
    }
    let params = std::collections::BTreeMap::from([
        ("convert_num_mode", "1".to_string()),
        ("engine_type", engine_type.to_string()),
        ("filter_punc", "0".to_string()),
        ("first_channel_only", "1".to_string()),
        ("secretid", secret_id.to_string()),
        ("speaker_diarization", "0".to_string()),
        ("timestamp", timestamp.to_string()),
        ("voice_format", "wav".to_string()),
        ("word_info", "1".to_string()),
    ]);
    Ok(params
        .into_iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>()
        .join("&"))
}

fn sign_tencent_request(
    secret_key: &str,
    app_id: &str,
    query: &str,
) -> std::result::Result<String, CloudRequestError> {
    if app_id.is_empty()
        || !app_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        return Err(CloudRequestError::InvalidResponse(
            "腾讯云 AppID 包含不支持的字符".into(),
        ));
    }
    let payload = format!("POSTasr.cloud.tencent.com/asr/flash/v1/{app_id}?{query}");
    let mut mac = Hmac::<Sha1>::new_from_slice(secret_key.as_bytes())
        .map_err(|error| CloudRequestError::InvalidResponse(format!("腾讯云签名失败：{error}")))?;
    mac.update(payload.as_bytes());
    Ok(BASE64.encode(mac.finalize().into_bytes()))
}

fn percent_encode_rfc3986(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.as_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(*byte, b'-' | b'_' | b'.' | b'~') {
            encoded.push(*byte as char);
        } else {
            use std::fmt::Write as _;
            let _ = write!(encoded, "%{byte:02X}");
        }
    }
    encoded
}

fn build_aliyun_token_query(access_key_id: &str, nonce: &str, timestamp: &str) -> String {
    let params = std::collections::BTreeMap::from([
        ("AccessKeyId", access_key_id),
        ("Action", "CreateToken"),
        ("Format", "JSON"),
        ("RegionId", "cn-shanghai"),
        ("SignatureMethod", "HMAC-SHA1"),
        ("SignatureNonce", nonce),
        ("SignatureVersion", "1.0"),
        ("Timestamp", timestamp),
        ("Version", "2019-02-28"),
    ]);
    params
        .into_iter()
        .map(|(key, value)| {
            format!(
                "{}={}",
                percent_encode_rfc3986(key),
                percent_encode_rfc3986(value)
            )
        })
        .collect::<Vec<_>>()
        .join("&")
}

fn sign_aliyun_token_request(
    access_key_secret: &str,
    query: &str,
) -> std::result::Result<String, CloudRequestError> {
    let payload = format!("GET&%2F&{}", percent_encode_rfc3986(query));
    let mut mac = Hmac::<Sha1>::new_from_slice(format!("{access_key_secret}&").as_bytes())
        .map_err(|error| CloudRequestError::InvalidResponse(format!("阿里云签名失败：{error}")))?;
    mac.update(payload.as_bytes());
    Ok(BASE64.encode(mac.finalize().into_bytes()))
}

fn build_aliyun_flash_query(app_key: &str, token: &str) -> String {
    [
        ("appkey", app_key),
        ("token", token),
        ("format", "wav"),
        ("sample_rate", "16000"),
        ("enable_word_level_result", "true"),
        ("enable_inverse_text_normalization", "false"),
        ("first_channel_only", "true"),
    ]
    .into_iter()
    .map(|(key, value)| format!("{key}={}", percent_encode_rfc3986(value)))
    .collect::<Vec<_>>()
    .join("&")
}

fn normalize_language(language: Option<&str>) -> Option<String> {
    let value = language?.trim().to_ascii_lowercase();
    if value.is_empty() || value == "auto" {
        return None;
    }
    Some(value.split(['-', '_']).next().unwrap_or(&value).to_string())
}

fn sanitize_provider_error(body: &str, api_key: &str) -> String {
    let redacted = if api_key.is_empty() {
        body.to_string()
    } else {
        body.replace(api_key, "[redacted]")
    };
    redacted
        .chars()
        .filter(|character| !character.is_control() || character.is_whitespace())
        .take(MAX_PROVIDER_ERROR_CHARS)
        .collect::<String>()
        .trim()
        .to_string()
}

#[derive(Debug)]
enum CloudRequestError {
    Http { status: StatusCode, body: String },
    Transport(String),
    InvalidResponse(String),
    FinalJob(String),
}

impl CloudRequestError {
    fn retryable(&self) -> bool {
        match self {
            Self::Http { status, .. } => {
                *status == StatusCode::REQUEST_TIMEOUT
                    || *status == StatusCode::TOO_MANY_REQUESTS
                    || status.is_server_error()
            }
            Self::Transport(_) => true,
            Self::InvalidResponse(_) | Self::FinalJob(_) => false,
        }
    }

    fn verbose_unsupported(&self) -> bool {
        matches!(
            self,
            Self::Http { status, .. }
                if *status == StatusCode::BAD_REQUEST
                    || *status == StatusCode::UNPROCESSABLE_ENTITY
        )
    }
}

impl std::fmt::Display for CloudRequestError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Http { status, body } if body.is_empty() => {
                write!(formatter, "服务返回 HTTP {status}")
            }
            Self::Http { status, body } => write!(formatter, "服务返回 HTTP {status}：{body}"),
            Self::Transport(message) => write!(formatter, "网络请求失败：{message}"),
            Self::InvalidResponse(message) | Self::FinalJob(message) => {
                formatter.write_str(message)
            }
        }
    }
}

#[derive(Debug, Default, Deserialize)]
struct CloudResponse {
    #[serde(default)]
    text: String,
    #[serde(default)]
    words: Vec<CloudWord>,
    #[serde(default)]
    segments: Vec<CloudSegment>,
}

#[derive(Debug, Deserialize)]
struct CloudWord {
    #[serde(default)]
    word: String,
    start: f64,
    end: f64,
}

#[derive(Debug, Deserialize)]
struct CloudSegment {
    #[serde(default)]
    text: String,
    start: f64,
    end: f64,
}

#[derive(Debug, Deserialize)]
struct ElevenLabsResponse {
    #[serde(default)]
    text: String,
    #[serde(default)]
    words: Vec<ElevenLabsWord>,
}

#[derive(Debug, Deserialize)]
struct ElevenLabsWord {
    #[serde(default)]
    text: String,
    #[serde(default)]
    r#type: String,
    start: Option<f64>,
    end: Option<f64>,
}

impl ElevenLabsResponse {
    fn into_cloud_response(self) -> CloudResponse {
        let words = self
            .words
            .into_iter()
            .filter(|word| {
                !matches!(word.r#type.as_str(), "spacing" | "audio_event")
                    && !word.text.trim().is_empty()
            })
            .filter_map(|word| {
                Some(CloudWord {
                    word: word.text,
                    start: word.start?,
                    end: word.end?,
                })
            })
            .collect();
        CloudResponse {
            text: self.text,
            words,
            segments: Vec::new(),
        }
    }
}

#[derive(Debug, Deserialize)]
struct DeepgramResponse {
    #[serde(default)]
    results: DeepgramResults,
}

#[derive(Debug, Default, Deserialize)]
struct DeepgramResults {
    #[serde(default)]
    channels: Vec<DeepgramChannel>,
}

#[derive(Debug, Deserialize)]
struct DeepgramChannel {
    #[serde(default)]
    alternatives: Vec<DeepgramAlternative>,
}

#[derive(Debug, Deserialize)]
struct DeepgramAlternative {
    #[serde(default)]
    transcript: String,
    #[serde(default)]
    words: Vec<DeepgramWord>,
}

#[derive(Debug, Deserialize)]
struct DeepgramWord {
    #[serde(default)]
    word: String,
    #[serde(default)]
    punctuated_word: String,
    start: f64,
    end: f64,
}

impl DeepgramResponse {
    fn into_cloud_response(self) -> CloudResponse {
        let Some(alternative) = self
            .results
            .channels
            .into_iter()
            .next()
            .and_then(|channel| channel.alternatives.into_iter().next())
        else {
            return CloudResponse {
                text: String::new(),
                words: Vec::new(),
                segments: Vec::new(),
            };
        };
        let words = alternative
            .words
            .into_iter()
            .map(|word| CloudWord {
                word: if word.punctuated_word.trim().is_empty() {
                    word.word
                } else {
                    word.punctuated_word
                },
                start: word.start,
                end: word.end,
            })
            .collect();
        CloudResponse {
            text: alternative.transcript,
            words,
            segments: Vec::new(),
        }
    }
}

#[derive(Debug, Default, Deserialize)]
struct VolcengineResponse {
    #[serde(default)]
    result: VolcengineResult,
}

#[derive(Debug, Default, Deserialize)]
struct VolcengineResult {
    #[serde(default)]
    text: String,
    #[serde(default)]
    utterances: Vec<VolcengineUtterance>,
}

#[derive(Debug, Deserialize)]
struct VolcengineUtterance {
    #[serde(default)]
    text: String,
    start_time: f64,
    end_time: f64,
    #[serde(default)]
    words: Vec<VolcengineWord>,
}

#[derive(Debug, Deserialize)]
struct VolcengineWord {
    #[serde(default)]
    text: String,
    start_time: f64,
    end_time: f64,
}

impl VolcengineResponse {
    fn into_cloud_response(self) -> CloudResponse {
        let mut words = Vec::new();
        let mut segments = Vec::new();
        for utterance in self.result.utterances {
            let start = utterance.start_time / 1_000.0;
            let end = utterance.end_time / 1_000.0;
            if start.is_finite()
                && end.is_finite()
                && end > start
                && !utterance.text.trim().is_empty()
            {
                segments.push(CloudSegment {
                    text: utterance.text,
                    start,
                    end,
                });
            }
            words.extend(utterance.words.into_iter().filter_map(|word| {
                let start = word.start_time / 1_000.0;
                let end = word.end_time / 1_000.0;
                (start.is_finite()
                    && end.is_finite()
                    && end > start
                    && !word.text.trim().is_empty())
                .then_some(CloudWord {
                    word: word.text,
                    start,
                    end,
                })
            }));
        }
        CloudResponse {
            text: self.result.text,
            words,
            segments,
        }
    }
}

#[derive(Debug, Default, Deserialize)]
struct TencentResponse {
    #[serde(default)]
    code: serde_json::Value,
    #[serde(default)]
    message: String,
    #[serde(default)]
    flash_result: Vec<TencentFlashResult>,
}

#[derive(Debug, Deserialize)]
struct TencentFlashResult {
    #[serde(default)]
    text: String,
    #[serde(default)]
    sentence_list: Vec<TencentSentence>,
}

#[derive(Debug, Deserialize)]
struct TencentSentence {
    #[serde(default)]
    text: String,
    start_time: f64,
    end_time: f64,
    #[serde(default)]
    word_list: Vec<TencentWord>,
}

#[derive(Debug, Deserialize)]
struct TencentWord {
    #[serde(default)]
    word: String,
    start_time: f64,
    end_time: f64,
}

impl TencentResponse {
    fn code(&self) -> Option<i64> {
        self.code
            .as_i64()
            .or_else(|| self.code.as_str()?.parse().ok())
    }

    fn into_cloud_response(self) -> CloudResponse {
        let Some(result) = self.flash_result.into_iter().next() else {
            return CloudResponse::default();
        };
        let mut words = Vec::new();
        let mut segments = Vec::new();
        for sentence in result.sentence_list {
            let start = sentence.start_time / 1_000.0;
            let end = sentence.end_time / 1_000.0;
            if start.is_finite()
                && end.is_finite()
                && end > start
                && !sentence.text.trim().is_empty()
            {
                segments.push(CloudSegment {
                    text: sentence.text,
                    start,
                    end,
                });
            }
            words.extend(sentence.word_list.into_iter().filter_map(|word| {
                let start = word.start_time / 1_000.0;
                let end = word.end_time / 1_000.0;
                (start.is_finite()
                    && end.is_finite()
                    && end > start
                    && !word.word.trim().is_empty())
                .then_some(CloudWord {
                    word: word.word,
                    start,
                    end,
                })
            }));
        }
        CloudResponse {
            text: result.text,
            words,
            segments,
        }
    }
}

#[derive(Debug, Clone)]
struct AliyunToken {
    value: String,
    expire_time: i64,
}

#[derive(Debug, Deserialize)]
struct AliyunTokenResponse {
    #[serde(rename = "Token")]
    token: Option<AliyunTokenPayload>,
}

#[derive(Debug, Deserialize)]
struct AliyunTokenPayload {
    #[serde(rename = "Id", default)]
    id: String,
    #[serde(rename = "ExpireTime", default)]
    expire_time: i64,
}

#[derive(Debug, Default, Deserialize)]
struct AliyunResponse {
    #[serde(default)]
    status: serde_json::Value,
    #[serde(default)]
    message: String,
    flash_result: Option<AliyunFlashResult>,
}

#[derive(Debug, Deserialize)]
struct AliyunFlashResult {
    #[serde(default)]
    sentences: Vec<AliyunSentence>,
}

#[derive(Debug, Deserialize)]
struct AliyunSentence {
    #[serde(default)]
    text: String,
    begin_time: serde_json::Value,
    end_time: serde_json::Value,
    #[serde(default)]
    words: Vec<AliyunWord>,
}

#[derive(Debug, Deserialize)]
struct AliyunWord {
    #[serde(default)]
    text: String,
    #[serde(default)]
    punc: String,
    begin_time: serde_json::Value,
    end_time: serde_json::Value,
}

fn json_number(value: &serde_json::Value) -> f64 {
    value
        .as_f64()
        .or_else(|| value.as_str()?.parse().ok())
        .unwrap_or(f64::NAN)
}

impl AliyunResponse {
    fn status(&self) -> Option<i64> {
        self.status
            .as_i64()
            .or_else(|| self.status.as_str()?.parse().ok())
    }

    fn into_cloud_response(self) -> CloudResponse {
        let Some(result) = self.flash_result else {
            return CloudResponse::default();
        };
        let mut text = String::new();
        let mut words = Vec::new();
        let mut segments = Vec::new();
        for sentence in result.sentences {
            append_word(&mut text, &sentence.text);
            let start = json_number(&sentence.begin_time) / 1_000.0;
            let end = json_number(&sentence.end_time) / 1_000.0;
            if start.is_finite()
                && end.is_finite()
                && end > start
                && !sentence.text.trim().is_empty()
            {
                segments.push(CloudSegment {
                    text: sentence.text,
                    start,
                    end,
                });
            }
            words.extend(sentence.words.into_iter().filter_map(|word| {
                let start = json_number(&word.begin_time) / 1_000.0;
                let end = json_number(&word.end_time) / 1_000.0;
                let value = format!("{}{}", word.text.trim(), word.punc.trim());
                (start.is_finite() && end.is_finite() && end > start && !value.is_empty())
                    .then_some(CloudWord {
                        word: value,
                        start,
                        end,
                    })
            }));
        }
        CloudResponse {
            text,
            words,
            segments,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum XfyunCodeClass {
    Success,
    Auth,
    Retriable,
    Fatal,
}

#[derive(Debug)]
struct XfyunUploadOutcome {
    order_id: String,
    task_estimate_time_ms: u64,
}

enum XfyunPollOutcome {
    Done(CloudResponse),
    OrderGone,
}

#[derive(Debug, Default, Deserialize)]
struct XfyunResponse {
    #[serde(default)]
    code: serde_json::Value,
    #[serde(rename = "descInfo", default)]
    desc_info: String,
    content: Option<XfyunContent>,
}

impl XfyunResponse {
    fn code(&self) -> String {
        if let Some(value) = self.code.as_str() {
            value.trim().to_string()
        } else if self.code.as_i64() == Some(0) {
            "000000".into()
        } else {
            self.code
                .as_i64()
                .map(|value| value.to_string())
                .unwrap_or_default()
        }
    }
}

#[derive(Debug, Default, Deserialize)]
struct XfyunContent {
    #[serde(rename = "orderId", default)]
    order_id: String,
    #[serde(rename = "taskEstimateTime", default)]
    task_estimate_time: serde_json::Value,
    #[serde(rename = "orderInfo")]
    order_info: Option<XfyunOrderInfo>,
    #[serde(rename = "orderResult", default)]
    order_result: serde_json::Value,
}

#[derive(Debug, Default, Deserialize)]
struct XfyunOrderInfo {
    #[serde(default)]
    status: serde_json::Value,
    #[serde(rename = "failType", default)]
    fail_type: serde_json::Value,
}

fn java_url_encode(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.as_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(*byte, b'.' | b'-' | b'*' | b'_') {
            encoded.push(*byte as char);
        } else if *byte == b' ' {
            encoded.push('+');
        } else {
            use std::fmt::Write as _;
            let _ = write!(encoded, "%{byte:02X}");
        }
    }
    encoded
}

fn build_xfyun_query(params: std::collections::BTreeMap<&str, String>) -> String {
    params
        .into_iter()
        .filter(|(_, value)| !value.is_empty())
        .map(|(key, value)| format!("{key}={}", java_url_encode(&value)))
        .collect::<Vec<_>>()
        .join("&")
}

fn sign_xfyun_request(
    api_secret: &str,
    query: &str,
) -> std::result::Result<String, CloudRequestError> {
    let mut mac = Hmac::<Sha1>::new_from_slice(api_secret.as_bytes())
        .map_err(|error| CloudRequestError::InvalidResponse(format!("讯飞签名失败：{error}")))?;
    mac.update(query.as_bytes());
    Ok(BASE64.encode(mac.finalize().into_bytes()))
}

fn xfyun_datetime() -> String {
    chrono::Local::now()
        .format("%Y-%m-%dT%H:%M:%S%z")
        .to_string()
}

fn build_xfyun_random() -> String {
    const ALPHANUMERIC: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    uuid::Uuid::new_v4()
        .as_bytes()
        .iter()
        .map(|byte| ALPHANUMERIC[*byte as usize % ALPHANUMERIC.len()] as char)
        .collect()
}

fn xfyun_request_url(
    transcription_url: &Url,
    suffix: &str,
) -> std::result::Result<Url, CloudRequestError> {
    let mut url = transcription_url.clone();
    let base = url
        .path()
        .strip_suffix("/v2/upload")
        .ok_or_else(|| CloudRequestError::InvalidResponse("讯飞 endpoint 路径无效".into()))?;
    url.set_path(&format!("{base}{suffix}"));
    url.set_query(None);
    Ok(url)
}

fn validate_xfyun_order_id(order_id: &str) -> std::result::Result<(), CloudRequestError> {
    if order_id.is_empty()
        || order_id.len() > 128
        || !order_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(CloudRequestError::InvalidResponse(
            "讯飞返回了无效的订单 ID".into(),
        ));
    }
    Ok(())
}

fn validate_xfyun_signature_random(
    signature_random: &str,
) -> std::result::Result<(), CloudRequestError> {
    if signature_random.len() != 16
        || !signature_random
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric())
    {
        return Err(CloudRequestError::InvalidResponse(
            "讯飞续查状态包含无效的签名随机串".into(),
        ));
    }
    Ok(())
}

fn classify_xfyun_code(http_status: StatusCode, code: Option<&str>) -> XfyunCodeClass {
    if let Some(code) = code.map(str::trim).filter(|value| !value.is_empty()) {
        return match code {
            "000000" | "0" => XfyunCodeClass::Success,
            "000002" | "100007" | "100008" | "100009" => XfyunCodeClass::Auth,
            "100012" | "999999" => XfyunCodeClass::Retriable,
            _ => XfyunCodeClass::Fatal,
        };
    }
    if http_status == StatusCode::TOO_MANY_REQUESTS || http_status.is_server_error() {
        XfyunCodeClass::Retriable
    } else {
        XfyunCodeClass::Fatal
    }
}

fn is_xfyun_order_gone(code: &str) -> bool {
    matches!(code.trim(), "100001" | "100037" | "100039")
}

fn normalize_xfyun_tier(model: &str) -> &'static str {
    if model.trim().eq_ignore_ascii_case("autominor") {
        "autominor"
    } else {
        "autodialect"
    }
}

fn normalize_xfyun_language(language: Option<&str>) -> String {
    let value = language
        .unwrap_or("auto")
        .trim()
        .to_ascii_lowercase()
        .replace('_', "-");
    if value.is_empty() || value == "auto" {
        return "auto".into();
    }
    if value.starts_with("zh-hant") || matches!(value.as_str(), "zh-tw" | "zh-hk") {
        return "zh-hant".into();
    }
    if value.starts_with("yue") {
        return "yue".into();
    }
    value.split('-').next().unwrap_or(&value).to_string()
}

fn validate_xfyun_language(
    tier: &str,
    language: Option<&str>,
) -> std::result::Result<(), CloudRequestError> {
    const DIALECT: &[&str] = &["auto", "zh", "zh-hant", "yue", "en"];
    const MINOR: &[&str] = &[
        "auto", "zh", "zh-hant", "en", "ja", "ko", "ru", "fr", "es", "ar", "de", "th", "vi", "hi",
        "pt", "it", "ms", "id", "fil", "tl", "tr", "el", "cs", "ur", "bn", "ta", "uk", "kk", "uz",
        "pl", "mn", "sw", "ha", "fa", "nl", "sv", "ro", "bg", "ug", "bo",
    ];
    let language = normalize_xfyun_language(language);
    let (current, other) = if tier == "autominor" {
        (MINOR, DIALECT)
    } else {
        (DIALECT, MINOR)
    };
    if current.contains(&language.as_str()) {
        return Ok(());
    }
    if other.contains(&language.as_str()) {
        let target = if tier == "autominor" {
            "autodialect（中英及方言）"
        } else {
            "autominor（37 语种，需向讯飞开通）"
        };
        return Err(CloudRequestError::InvalidResponse(format!(
            "讯飞当前档位 {tier} 不支持源语言 {language}，请切换为 {target}"
        )));
    }
    Err(CloudRequestError::InvalidResponse(format!(
        "讯飞录音文件转写不支持源语言 {language}"
    )))
}

fn describe_xfyun_fail_type(fail_type: i64) -> String {
    match fail_type {
        1 => "音频上传失败（failType 1）".into(),
        2 => "音频转码失败，请检查文件是否损坏或加密（failType 2）".into(),
        3 => "音频识别失败（failType 3）".into(),
        4 => "音频时长超过 5 小时限制（failType 4）".into(),
        5 => "音频时长校验失败（failType 5）".into(),
        value => format!("转写失败（failType {value}）"),
    }
}

#[cfg(not(test))]
fn xfyun_first_delay_ms(task_estimate_time_ms: u64) -> u64 {
    if task_estimate_time_ms == 0 {
        5_000
    } else {
        (task_estimate_time_ms / 2).clamp(3_000, 30_000)
    }
}

#[cfg(test)]
fn xfyun_first_delay_ms(_task_estimate_time_ms: u64) -> u64 {
    0
}

#[cfg(not(test))]
fn xfyun_poll_interval_ms(query_index: usize) -> u64 {
    match query_index {
        0..=5 => 5_000,
        6..=17 => 10_000,
        _ => 45_000,
    }
}

#[cfg(test)]
fn xfyun_poll_interval_ms(_query_index: usize) -> u64 {
    0
}

fn extract_xfyun_result(order_result: &serde_json::Value) -> CloudResponse {
    let parsed = if let Some(value) = order_result.as_str() {
        match serde_json::from_str::<serde_json::Value>(value) {
            Ok(value) => value,
            Err(_) => return CloudResponse::default(),
        }
    } else {
        order_result.clone()
    };
    let Some(lattice) = parsed.get("lattice").and_then(serde_json::Value::as_array) else {
        return CloudResponse::default();
    };
    let mut text = String::new();
    let mut words: Vec<CloudWord> = Vec::new();
    let mut segments = Vec::new();
    for item in lattice {
        let Some(best_value) = item.get("json_1best") else {
            continue;
        };
        let best = if let Some(value) = best_value.as_str() {
            match serde_json::from_str::<serde_json::Value>(value) {
                Ok(value) => value,
                Err(_) => continue,
            }
        } else {
            best_value.clone()
        };
        let Some(st) = best.get("st") else {
            continue;
        };
        let begin_ms = st.get("bg").map(json_number).unwrap_or(f64::NAN);
        let end_ms = st.get("ed").map(json_number).unwrap_or(f64::NAN);
        if !begin_ms.is_finite() || !end_ms.is_finite() || end_ms <= begin_ms {
            continue;
        }
        let sentence_word_start = words.len();
        let mut sentence = String::new();
        for rt in st
            .get("rt")
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
        {
            for word_group in rt
                .get("ws")
                .and_then(serde_json::Value::as_array)
                .into_iter()
                .flatten()
            {
                let Some(token) = word_group
                    .get("cw")
                    .and_then(serde_json::Value::as_array)
                    .and_then(|tokens| tokens.first())
                else {
                    continue;
                };
                let value = token
                    .get("w")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default();
                let word_property = token
                    .get("wp")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default();
                if value.is_empty() || matches!(word_property, "s" | "g") {
                    continue;
                }
                if word_property == "p" {
                    sentence.push_str(value);
                    if words.len() > sentence_word_start {
                        if let Some(previous) = words.last_mut() {
                            previous.word.push_str(value);
                        }
                    }
                    continue;
                }
                append_word(&mut sentence, value);
                let word_begin = word_group.get("wb").map(json_number).unwrap_or(f64::NAN);
                let word_end = word_group.get("we").map(json_number).unwrap_or(f64::NAN);
                if word_begin.is_finite() && word_end.is_finite() && word_end > word_begin {
                    words.push(CloudWord {
                        word: value.into(),
                        start: (begin_ms + word_begin * 10.0) / 1_000.0,
                        end: (begin_ms + word_end * 10.0) / 1_000.0,
                    });
                }
            }
        }
        let sentence = sentence.trim().to_string();
        if !sentence.is_empty() {
            append_word(&mut text, &sentence);
            segments.push(CloudSegment {
                text: sentence,
                start: begin_ms / 1_000.0,
                end: end_ms / 1_000.0,
            });
        }
    }
    CloudResponse {
        text,
        words,
        segments,
    }
}

#[derive(Debug, Deserialize)]
struct GladiaUploadResponse {
    #[serde(default)]
    audio_url: String,
}

#[derive(Debug, Deserialize)]
struct GladiaInitResponse {
    #[serde(default)]
    id: String,
}

#[derive(Debug, Deserialize)]
struct GladiaPollResponse {
    #[serde(default)]
    status: String,
    error_code: Option<u16>,
    result: Option<GladiaResult>,
}

#[derive(Debug, Deserialize)]
struct GladiaResult {
    #[serde(default)]
    transcription: GladiaTranscription,
}

#[derive(Debug, Default, Deserialize)]
struct GladiaTranscription {
    #[serde(default)]
    full_transcript: String,
    #[serde(default)]
    utterances: Vec<GladiaUtterance>,
}

#[derive(Debug, Deserialize)]
struct GladiaUtterance {
    #[serde(default)]
    text: String,
    start: f64,
    end: f64,
    #[serde(default)]
    words: Vec<GladiaWord>,
}

#[derive(Debug, Deserialize)]
struct GladiaWord {
    #[serde(default)]
    word: String,
    start: f64,
    end: f64,
}

impl GladiaResult {
    fn into_cloud_response(self) -> CloudResponse {
        let mut words = Vec::new();
        let mut segments = Vec::new();
        for utterance in self.transcription.utterances {
            if utterance.start.is_finite()
                && utterance.end.is_finite()
                && utterance.end > utterance.start
                && !utterance.text.trim().is_empty()
            {
                segments.push(CloudSegment {
                    text: utterance.text,
                    start: utterance.start,
                    end: utterance.end,
                });
            }
            words.extend(utterance.words.into_iter().filter_map(|word| {
                (word.start.is_finite()
                    && word.end.is_finite()
                    && word.end > word.start
                    && !word.word.trim().is_empty())
                .then_some(CloudWord {
                    word: word.word,
                    start: word.start,
                    end: word.end,
                })
            }));
        }
        CloudResponse {
            text: self.transcription.full_transcript,
            words,
            segments,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ChunkRange {
    start_sample: usize,
    end_sample: usize,
}

fn build_chunk_ranges(
    speech: &[SpeechSlice],
    audio_samples: usize,
    max_chunk_samples: usize,
) -> Vec<ChunkRange> {
    let mut ranges = Vec::new();
    let mut current: Option<ChunkRange> = None;
    for segment in speech {
        let start = segment.start_sample.min(audio_samples);
        let end = segment
            .start_sample
            .saturating_add(segment.samples.len())
            .min(audio_samples);
        if end <= start {
            continue;
        }
        match current.as_mut() {
            Some(range) if end.saturating_sub(range.start_sample) <= max_chunk_samples => {
                range.end_sample = range.end_sample.max(end);
            }
            Some(range) => {
                ranges.push(*range);
                *range = ChunkRange {
                    start_sample: start,
                    end_sample: end,
                };
            }
            None => {
                current = Some(ChunkRange {
                    start_sample: start,
                    end_sample: end,
                });
            }
        }
    }
    if let Some(range) = current {
        ranges.push(range);
    }
    ranges
}

fn write_pcm16_wav(path: &Path, samples: &[f32]) -> std::io::Result<()> {
    let data_size = samples
        .len()
        .checked_mul(2)
        .and_then(|size| u32::try_from(size).ok())
        .ok_or_else(|| std::io::Error::other("WAV chunk is too large"))?;
    let mut file = std::fs::File::create(path)?;
    file.write_all(b"RIFF")?;
    file.write_all(&(36_u32 + data_size).to_le_bytes())?;
    file.write_all(b"WAVEfmt ")?;
    file.write_all(&16_u32.to_le_bytes())?;
    file.write_all(&1_u16.to_le_bytes())?;
    file.write_all(&1_u16.to_le_bytes())?;
    file.write_all(&(SAMPLE_RATE as u32).to_le_bytes())?;
    file.write_all(&((SAMPLE_RATE as u32) * 2).to_le_bytes())?;
    file.write_all(&2_u16.to_le_bytes())?;
    file.write_all(&16_u16.to_le_bytes())?;
    file.write_all(b"data")?;
    file.write_all(&data_size.to_le_bytes())?;
    for sample in samples {
        let value = (sample.clamp(-1.0, 1.0) * i16::MAX as f32).round() as i16;
        file.write_all(&value.to_le_bytes())?;
    }
    file.sync_all()?;
    Ok(())
}

struct TempWav(PathBuf);

impl TempWav {
    fn create(parent: &Path, samples: &[f32]) -> std::io::Result<Self> {
        let path = parent.join(format!(".finalsub-cloud-{}.wav", uuid::Uuid::new_v4()));
        write_pcm16_wav(&path, samples)?;
        Ok(Self(path))
    }

    fn bytes(&self) -> std::io::Result<Vec<u8>> {
        std::fs::read(&self.0)
    }
}

impl Drop for TempWav {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn contains_cjk(text: &str) -> bool {
    text.chars().any(
        |character| matches!(character as u32, 0x3400..=0x9fff | 0x3040..=0x30ff | 0xac00..=0xd7af),
    )
}

fn ends_sentence(text: &str) -> bool {
    text.chars().last().is_some_and(|character| {
        matches!(
            character,
            '。' | '？' | '！' | '；' | '.' | '?' | '!' | ';' | ':' | '：'
        )
    })
}

fn split_text_blocks(text: &str) -> Vec<String> {
    let normalized = text.split_whitespace().collect::<Vec<_>>().join(" ");
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
            if ends_sentence(word) || current.chars().count() >= max_chars {
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
        if ends_sentence(&current) || current.chars().count() >= max_chars {
            blocks.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        blocks.push(current);
    }
    blocks
}

fn build_even_cues(text: &str, start_ms: u64, end_ms: u64) -> Vec<Cue> {
    let blocks = split_text_blocks(text);
    if blocks.is_empty() || end_ms <= start_ms {
        return Vec::new();
    }
    let total_weight = blocks
        .iter()
        .map(|block| block.chars().filter(|c| !c.is_whitespace()).count().max(1))
        .sum::<usize>();
    let duration_ms = end_ms - start_ms;
    let block_count = blocks.len();
    let mut cursor = start_ms;
    blocks
        .into_iter()
        .enumerate()
        .map(|(index, text)| {
            let end = if index + 1 == block_count {
                end_ms
            } else {
                let weight = text.chars().filter(|c| !c.is_whitespace()).count().max(1);
                (cursor + duration_ms * weight as u64 / total_weight as u64)
                    .min(end_ms)
                    .max(cursor + 1)
            };
            let cue = Cue {
                index: 0,
                start_ms: cursor,
                end_ms: end,
                text,
            };
            cursor = end;
            cue
        })
        .collect()
}

fn append_word(text: &mut String, word: &str) {
    let word = word.trim();
    if word.is_empty() {
        return;
    }
    let needs_space = text
        .chars()
        .last()
        .zip(word.chars().next())
        .is_some_and(|(left, right)| left.is_ascii_alphanumeric() && right.is_ascii_alphanumeric());
    if needs_space {
        text.push(' ');
    }
    text.push_str(word);
}

fn build_word_cues(words: &[CloudWord], offset_ms: u64, chunk_end_ms: u64) -> Vec<Cue> {
    let mut cues = Vec::new();
    let mut text = String::new();
    let mut start_ms = offset_ms;
    let mut end_ms = offset_ms;
    for word in words {
        if !word.start.is_finite() || !word.end.is_finite() || word.end <= word.start {
            continue;
        }
        if text.is_empty() {
            start_ms = offset_ms + (word.start.max(0.0) * 1_000.0) as u64;
        }
        append_word(&mut text, &word.word);
        end_ms = offset_ms + (word.end.max(0.0) * 1_000.0) as u64;
        let max_chars = if contains_cjk(&text) { 28 } else { 84 };
        if ends_sentence(&word.word) || text.chars().count() >= max_chars {
            let normalized = text.trim().to_string();
            if !normalized.is_empty() && start_ms < chunk_end_ms {
                cues.push(Cue {
                    index: 0,
                    start_ms: start_ms.min(chunk_end_ms - 1),
                    end_ms: end_ms.min(chunk_end_ms).max(start_ms + 1),
                    text: normalized,
                });
            }
            text.clear();
        }
    }
    let normalized = text.trim().to_string();
    if !normalized.is_empty() && start_ms < chunk_end_ms {
        cues.push(Cue {
            index: 0,
            start_ms: start_ms.min(chunk_end_ms - 1),
            end_ms: end_ms.min(chunk_end_ms).max(start_ms + 1),
            text: normalized,
        });
    }
    cues
}

fn response_to_cues(response: CloudResponse, range: ChunkRange) -> Vec<Cue> {
    let offset_ms = range.start_sample as u64 * 1_000 / SAMPLE_RATE as u64;
    let chunk_end_ms = range.end_sample as u64 * 1_000 / SAMPLE_RATE as u64;
    if !response.words.is_empty() {
        let cues = build_word_cues(&response.words, offset_ms, chunk_end_ms);
        if !cues.is_empty() {
            return cues;
        }
    }
    if !response.segments.is_empty() {
        let mut cues = Vec::new();
        for segment in response.segments {
            if !segment.start.is_finite()
                || !segment.end.is_finite()
                || segment.end <= segment.start
            {
                continue;
            }
            let start = offset_ms + (segment.start.max(0.0) * 1_000.0) as u64;
            let end = offset_ms + (segment.end.max(0.0) * 1_000.0) as u64;
            if start >= chunk_end_ms {
                continue;
            }
            let bounded_start = start.min(chunk_end_ms.saturating_sub(1));
            let bounded_end = end.min(chunk_end_ms).max(bounded_start + 1);
            cues.extend(build_even_cues(&segment.text, bounded_start, bounded_end));
        }
        if !cues.is_empty() {
            return cues;
        }
    }
    build_even_cues(&response.text, offset_ms, chunk_end_ms)
}

fn cancelled(cancel: &Option<tokio::sync::watch::Receiver<bool>>) -> bool {
    cancel.as_ref().is_some_and(|receiver| *receiver.borrow())
}

#[async_trait]
impl AsrEngine for CloudAsrEngine {
    fn id(&self) -> &'static str {
        CLOUD_ASR_ENGINE_ID
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
            requires_model_download: false,
        }
    }

    async fn prepare(&self, model: &AsrModelRef) -> Result<()> {
        if model.engine_id != CLOUD_ASR_ENGINE_ID || model.model_id != CLOUD_ASR_MODEL_ID {
            return Err(FinalSubError::Validation(format!(
                "云端 ASR 模型引用不匹配：期望 {CLOUD_ASR_ENGINE_ID}/{CLOUD_ASR_MODEL_ID}"
            )));
        }
        validate_config(&self.config)
    }

    async fn transcribe(
        &self,
        job: TranscribeJob,
        progress: ProgressSink,
        cancel_rx: Option<tokio::sync::watch::Receiver<bool>>,
    ) -> Result<SubtitleTrack> {
        self.prepare(&job.model).await?;
        if cancelled(&cancel_rx) {
            return Err(FinalSubError::Validation("任务已取消".into()));
        }
        progress
            .send(ProgressUpdate {
                progress: 0.03,
                message: "正在使用 Silero VAD 准备云端转写片段…".into(),
            })
            .await
            .ok();

        let audio_path = job.audio_path.clone();
        let vad_model_path = self.vad_model_path.clone();
        let prepared = tokio::task::spawn_blocking(move || {
            let wave = sherpa_onnx::Wave::read(&audio_path)
                .ok_or_else(|| "读取云端 ASR WAV 音频失败".to_string())?;
            if wave.sample_rate() != SAMPLE_RATE {
                return Err(format!(
                    "云端 ASR 需要 16 kHz 单声道 WAV，当前采样率为 {} Hz",
                    wave.sample_rate()
                ));
            }
            let samples = wave.samples().to_vec();
            let speech = detect_speech(&samples, &vad_model_path, MAX_CHUNK_SECONDS)?;
            let ranges = build_chunk_ranges(
                &speech,
                samples.len(),
                SAMPLE_RATE as usize * MAX_CHUNK_SECONDS,
            );
            if ranges.is_empty() {
                return Err("Silero VAD 未检测到可上传的人声".into());
            }
            Ok::<_, String>((samples, ranges))
        })
        .await
        .map_err(|error| FinalSubError::Validation(format!("云端 ASR 线程池异常：{error}")))?
        .map_err(FinalSubError::Validation)?;

        let (samples, ranges) = prepared;
        let parent = Path::new(&job.output_path)
            .parent()
            .ok_or_else(|| FinalSubError::Validation("云端 ASR 输出路径缺少父目录".into()))?;
        let mut cancel = cancel_rx;
        let mut cues = Vec::new();
        let total = ranges.len();
        for (index, range) in ranges.into_iter().enumerate() {
            if cancelled(&cancel) {
                return Err(FinalSubError::Validation("任务已取消".into()));
            }
            let temp = TempWav::create(parent, &samples[range.start_sample..range.end_sample])
                .map_err(|error| {
                    FinalSubError::Validation(format!("创建云端 ASR 临时 WAV 失败：{error}"))
                })?;
            let bytes = temp.bytes().map_err(|error| {
                FinalSubError::Validation(format!("读取云端 ASR 临时 WAV 失败：{error}"))
            })?;
            progress
                .send(ProgressUpdate {
                    progress: 0.08 + index as f32 / total as f32 * 0.86,
                    message: format!("正在上传并识别云端片段 {}/{}…", index + 1, total),
                })
                .await
                .ok();

            let _request_permit = self
                .request_gate
                .acquire(
                    self.config.request_concurrency,
                    self.config.request_interval_ms,
                    cancel.as_mut(),
                )
                .await?;
            let request = self.transcribe_chunk(&bytes, job.language.as_deref());
            tokio::pin!(request);
            let response = if let Some(receiver) = cancel.as_mut() {
                tokio::select! {
                    result = &mut request => result,
                    changed = receiver.changed() => {
                        if changed.is_err() || *receiver.borrow() {
                            return Err(FinalSubError::Validation("任务已取消".into()));
                        }
                        continue;
                    }
                }
            } else {
                request.await
            }
            .map_err(|error| FinalSubError::Validation(format!("云端 ASR 失败：{error}")))?;
            let mut chunk_cues = response_to_cues(response, range);
            cues.append(&mut chunk_cues);
        }

        cues.sort_by_key(|cue| (cue.start_ms, cue.end_ms));
        for (index, cue) in cues.iter_mut().enumerate() {
            cue.index = (index + 1) as u32;
        }
        if cues.is_empty() {
            return Err(FinalSubError::Validation(
                "云端 ASR 未返回字幕内容，请检查模型、语言或音频".into(),
            ));
        }
        progress
            .send(ProgressUpdate {
                progress: 1.0,
                message: format!("云端 ASR 转录完成，共 {} 条字幕", cues.len()),
            })
            .await
            .ok();
        Ok(SubtitleTrack { cues })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    async fn capture_next_request_with_headers(
        listener: &TcpListener,
        response_body: &str,
        extra_headers: &str,
    ) -> Vec<u8> {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request = Vec::new();
        let mut body_offset = None;
        let mut content_length = None;
        loop {
            let mut buffer = [0_u8; 4096];
            let count = socket.read(&mut buffer).await.unwrap();
            assert!(count > 0, "client closed before sending a complete request");
            request.extend_from_slice(&buffer[..count]);
            if body_offset.is_none() {
                if let Some(offset) = request.windows(4).position(|bytes| bytes == b"\r\n\r\n") {
                    let header_end = offset + 4;
                    let headers = String::from_utf8_lossy(&request[..header_end]);
                    content_length = headers.lines().find_map(|line| {
                        let (name, value) = line.split_once(':')?;
                        name.eq_ignore_ascii_case("content-length")
                            .then(|| value.trim().parse::<usize>().ok())
                            .flatten()
                    });
                    if content_length.is_none() && headers.starts_with("GET ") {
                        content_length = Some(0);
                    }
                    body_offset = Some(header_end);
                }
            }
            if let (Some(offset), Some(length)) = (body_offset, content_length) {
                if request.len() >= offset + length {
                    break;
                }
            }
        }
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n{extra_headers}Content-Length: {}\r\nConnection: close\r\n\r\n{}",
            response_body.len(),
            response_body
        );
        socket.write_all(response.as_bytes()).await.unwrap();
        request
    }

    async fn capture_next_request(listener: &TcpListener, response_body: &str) -> Vec<u8> {
        capture_next_request_with_headers(listener, response_body, "").await
    }

    async fn capture_request(listener: TcpListener, response_body: &'static str) -> Vec<u8> {
        capture_next_request(&listener, response_body).await
    }

    fn test_engine(config: CloudAsrConfig) -> CloudAsrEngine {
        let transcription_url =
            normalize_transcription_url_for_protocol(config.protocol, &config.endpoint).unwrap();
        let request_gate = provider_request_gate(config.protocol, &transcription_url);
        CloudAsrEngine {
            transcription_url,
            client: Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .unwrap(),
            config,
            vad_model_path: PathBuf::new(),
            aliyun_token: tokio::sync::Mutex::new(None),
            request_gate,
        }
    }

    #[test]
    fn provider_request_gate_is_shared_per_protocol_and_endpoint() {
        let endpoint = Url::parse("https://gate-shared.example/v1/audio/transcriptions").unwrap();
        let other_endpoint =
            Url::parse("https://gate-isolated.example/v1/audio/transcriptions").unwrap();

        let first = provider_request_gate(CloudAsrProtocol::OpenAiCompatible, &endpoint);
        let same = provider_request_gate(CloudAsrProtocol::OpenAiCompatible, &endpoint);
        let other_protocol = provider_request_gate(CloudAsrProtocol::Deepgram, &endpoint);
        let other_endpoint =
            provider_request_gate(CloudAsrProtocol::OpenAiCompatible, &other_endpoint);

        assert!(Arc::ptr_eq(&first, &same));
        assert!(!Arc::ptr_eq(&first, &other_protocol));
        assert!(!Arc::ptr_eq(&first, &other_endpoint));
    }

    #[tokio::test]
    async fn provider_request_gate_caps_cross_task_concurrency() {
        let gate = Arc::new(ProviderRequestGate::default());
        let first = gate.acquire(2, 0, None).await.unwrap();
        let second = gate.acquire(2, 0, None).await.unwrap();
        let waiting_gate = gate.clone();
        let mut waiter = tokio::spawn(async move { waiting_gate.acquire(2, 0, None).await });

        assert!(tokio::time::timeout(Duration::from_millis(25), &mut waiter)
            .await
            .is_err());
        drop(first);
        let third = tokio::time::timeout(Duration::from_millis(250), waiter)
            .await
            .expect("released permit should wake a queued request")
            .expect("request gate task should not panic")
            .expect("queued request should acquire a permit");

        drop(second);
        drop(third);
    }

    #[tokio::test]
    async fn provider_request_gate_spaces_request_starts_globally() {
        let gate = Arc::new(ProviderRequestGate::default());
        let started = tokio::time::Instant::now();
        let first = gate.acquire(2, 40, None).await.unwrap();
        drop(first);
        let second = gate.acquire(2, 40, None).await.unwrap();

        assert!(started.elapsed() >= Duration::from_millis(35));
        drop(second);
    }

    #[tokio::test]
    async fn provider_request_gate_wait_is_cancellation_aware() {
        let gate = Arc::new(ProviderRequestGate::default());
        let first = gate.acquire(1, 0, None).await.unwrap();
        let (cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);
        let waiting_gate = gate.clone();
        let waiter = tokio::spawn(async move {
            let mut cancel_rx = cancel_rx;
            waiting_gate.acquire(1, 0, Some(&mut cancel_rx)).await
        });

        tokio::task::yield_now().await;
        cancel_tx.send(true).unwrap();
        let result = tokio::time::timeout(Duration::from_millis(250), waiter)
            .await
            .expect("cancellation should wake a queued request")
            .expect("request gate task should not panic");
        let error = match result {
            Ok(_) => panic!("cancelled request must not acquire a permit"),
            Err(error) => error,
        };
        assert!(matches!(error, FinalSubError::Validation(message) if message.contains("取消")));
        drop(first);
    }

    #[test]
    fn endpoint_normalization_accepts_base_or_full_path() {
        assert_eq!(
            normalize_transcription_url("https://api.openai.com/v1")
                .unwrap()
                .as_str(),
            "https://api.openai.com/v1/audio/transcriptions"
        );
        assert_eq!(
            normalize_transcription_url(
                "https://gateway.example/v1/audio/transcriptions?ignored=true#fragment"
            )
            .unwrap()
            .as_str(),
            "https://gateway.example/v1/audio/transcriptions"
        );
        assert!(normalize_transcription_url("ftp://example.com/v1").is_err());
        assert!(normalize_transcription_url("https://mock-user:fake-pass@example.com/v1").is_err());
        assert_eq!(
            normalize_transcription_url_for_protocol(
                CloudAsrProtocol::ElevenLabs,
                "https://api.elevenlabs.io"
            )
            .unwrap()
            .as_str(),
            "https://api.elevenlabs.io/v1/speech-to-text"
        );
        assert_eq!(
            normalize_transcription_url_for_protocol(
                CloudAsrProtocol::Deepgram,
                "https://api.deepgram.com/v1"
            )
            .unwrap()
            .as_str(),
            "https://api.deepgram.com/v1/listen"
        );
        assert_eq!(
            normalize_transcription_url_for_protocol(
                CloudAsrProtocol::Gladia,
                "https://api.gladia.io/v2"
            )
            .unwrap()
            .as_str(),
            "https://api.gladia.io/v2/pre-recorded"
        );
        assert_eq!(
            normalize_transcription_url_for_protocol(
                CloudAsrProtocol::Volcengine,
                "https://openspeech.bytedance.com"
            )
            .unwrap()
            .as_str(),
            "https://openspeech.bytedance.com/api/v3/auc/bigmodel/recognize/flash"
        );
        assert_eq!(
            normalize_transcription_url_for_protocol(
                CloudAsrProtocol::Tencent,
                "https://asr.cloud.tencent.com"
            )
            .unwrap()
            .as_str(),
            "https://asr.cloud.tencent.com/asr/flash/v1"
        );
        assert_eq!(
            normalize_transcription_url_for_protocol(
                CloudAsrProtocol::Aliyun,
                "https://nls-gateway-cn-shanghai.aliyuncs.com"
            )
            .unwrap()
            .as_str(),
            "https://nls-gateway-cn-shanghai.aliyuncs.com/stream/v1/FlashRecognizer"
        );
        assert_eq!(
            normalize_transcription_url_for_protocol(
                CloudAsrProtocol::Xfyun,
                "https://office-api-ist-dx.iflyaisol.com"
            )
            .unwrap()
            .as_str(),
            "https://office-api-ist-dx.iflyaisol.com/v2/upload"
        );
    }

    #[test]
    fn protocol_separates_keychain_provider_namespaces() {
        let openai = parse_protocol("openai-compatible").unwrap();
        let elevenlabs = parse_protocol("elevenlabs").unwrap();
        let deepgram = parse_protocol("deepgram").unwrap();
        let gladia = parse_protocol("gladia").unwrap();
        let volcengine = parse_protocol("volcengine").unwrap();
        let tencent = parse_protocol("tencent").unwrap();
        let aliyun = parse_protocol("aliyun").unwrap();
        let xfyun = parse_protocol("xfyun").unwrap();
        assert_ne!(openai.secret_provider(), elevenlabs.secret_provider());
        assert_ne!(openai.secret_provider(), deepgram.secret_provider());
        assert_ne!(elevenlabs.secret_provider(), deepgram.secret_provider());
        assert_ne!(gladia.secret_provider(), openai.secret_provider());
        assert_ne!(gladia.secret_provider(), elevenlabs.secret_provider());
        assert_ne!(gladia.secret_provider(), deepgram.secret_provider());
        assert_ne!(volcengine.secret_provider(), openai.secret_provider());
        assert_ne!(volcengine.secret_provider(), gladia.secret_provider());
        assert_ne!(tencent.secret_provider(), openai.secret_provider());
        assert_eq!(
            tencent.required_secret_fields(),
            &["accountId", "apiKey", "apiSecret"]
        );
        assert_ne!(aliyun.secret_provider(), tencent.secret_provider());
        assert_eq!(
            aliyun.required_secret_fields(),
            &["accountId", "apiKey", "apiSecret"]
        );
        assert_ne!(xfyun.secret_provider(), aliyun.secret_provider());
        assert_ne!(xfyun.secret_provider(), tencent.secret_provider());
        assert_eq!(
            xfyun.required_secret_fields(),
            &["accountId", "apiKey", "apiSecret"]
        );
        assert!(parse_protocol("unknown").is_err());
    }

    #[test]
    fn chunk_ranges_preserve_original_offsets_and_maximum_duration() {
        let max = SAMPLE_RATE as usize * 300;
        let speech = vec![
            SpeechSlice {
                start_sample: SAMPLE_RATE as usize,
                samples: vec![0.1; SAMPLE_RATE as usize * 10],
            },
            SpeechSlice {
                start_sample: SAMPLE_RATE as usize * 20,
                samples: vec![0.1; SAMPLE_RATE as usize * 10],
            },
            SpeechSlice {
                start_sample: SAMPLE_RATE as usize * 400,
                samples: vec![0.1; SAMPLE_RATE as usize * 10],
            },
        ];
        let ranges = build_chunk_ranges(&speech, SAMPLE_RATE as usize * 500, max);
        assert_eq!(ranges.len(), 2);
        assert_eq!(ranges[0].start_sample, SAMPLE_RATE as usize);
        assert_eq!(ranges[0].end_sample, SAMPLE_RATE as usize * 30);
        assert_eq!(ranges[1].start_sample, SAMPLE_RATE as usize * 400);
    }

    #[test]
    fn tencent_signature_and_language_mapping_match_protocol_vectors() {
        let query = build_tencent_query("AKIDtest", "16k_zh", 1_700_000_000).unwrap();
        assert_eq!(query, "convert_num_mode=1&engine_type=16k_zh&filter_punc=0&first_channel_only=1&secretid=AKIDtest&speaker_diarization=0&timestamp=1700000000&voice_format=wav&word_info=1");
        assert_eq!(
            sign_tencent_request("TestSecretKey", "1300000000", &query).unwrap(),
            "a38vmBf1ujfiJ+tNg9z2viHpnns="
        );
        assert_eq!(
            resolve_tencent_engine_type("standard", Some("ja-JP")).unwrap(),
            "16k_ja"
        );
        assert_eq!(
            resolve_tencent_engine_type("large", Some("ja-JP")).unwrap(),
            "16k_multi_lang"
        );
        assert!(resolve_tencent_engine_type("standard", Some("ru")).is_err());
    }

    #[test]
    fn aliyun_signature_matches_pop_protocol_vector() {
        assert_eq!(percent_encode_rfc3986("a b!*~"), "a%20b%21%2A~");
        let query = build_aliyun_token_query("LTAItest", "nonce-1234", "2026-07-02T00:00:00Z");
        assert_eq!(query, "AccessKeyId=LTAItest&Action=CreateToken&Format=JSON&RegionId=cn-shanghai&SignatureMethod=HMAC-SHA1&SignatureNonce=nonce-1234&SignatureVersion=1.0&Timestamp=2026-07-02T00%3A00%3A00Z&Version=2019-02-28");
        assert_eq!(
            sign_aliyun_token_request("TestSecret", &query).unwrap(),
            "epSSDRaAbN3SlUcKjiHPrS1ke6g="
        );
    }

    #[test]
    fn xfyun_signature_language_and_result_parsing_match_protocol_vectors() {
        assert_eq!(java_url_encode("abc123.-*_"), "abc123.-*_");
        assert_eq!(java_url_encode("a b!~"), "a+b%21%7E");
        let query = build_xfyun_query(std::collections::BTreeMap::from([
            ("signatureRandom", "0123456789abcdef".into()),
            ("appId", "abc".into()),
            ("dateTime", "2026-07-02T10:00:00+0800".into()),
        ]));
        assert_eq!(
            query,
            "appId=abc&dateTime=2026-07-02T10%3A00%3A00%2B0800&signatureRandom=0123456789abcdef"
        );
        assert_eq!(
            sign_xfyun_request("testsecret", &query).unwrap(),
            "TB+lxwHUWgBejbtkyD3TE6qyzxI="
        );
        assert_eq!(normalize_xfyun_tier("AUTOMINOR"), "autominor");
        assert_eq!(normalize_xfyun_tier("unknown"), "autodialect");
        assert!(validate_xfyun_language("autodialect", Some("zh-CN")).is_ok());
        assert!(validate_xfyun_language("autodialect", Some("ja-JP")).is_err());
        assert!(validate_xfyun_language("autominor", Some("ja-JP")).is_ok());

        let best = serde_json::json!({
            "st": {
                "bg": "100",
                "ed": "1000",
                "rt": [{
                    "ws": [
                        {"wb": 0, "we": 20, "cw": [{"w": "北京", "wp": "n"}]},
                        {"wb": 25, "we": 50, "cw": [{"w": "天气", "wp": "n"}]},
                        {"wb": 50, "we": 50, "cw": [{"w": "。", "wp": "p"}]}
                    ]
                }]
            }
        });
        let result = extract_xfyun_result(&serde_json::json!({
            "lattice": [{"json_1best": best.to_string()}]
        }));
        assert_eq!(result.text, "北京天气。");
        assert_eq!(result.words.len(), 2);
        assert_eq!(result.words[1].word, "天气。");
        assert_eq!(result.words[0].start, 0.1);
        assert_eq!(result.words[1].end, 0.6);
        assert_eq!(result.segments[0].end, 1.0);
    }

    #[test]
    fn plain_text_response_uses_original_chunk_timeline() {
        let cues = response_to_cues(
            CloudResponse {
                text: "你好。世界！".into(),
                words: Vec::new(),
                segments: Vec::new(),
            },
            ChunkRange {
                start_sample: SAMPLE_RATE as usize * 5,
                end_sample: SAMPLE_RATE as usize * 9,
            },
        );
        assert_eq!(cues.first().unwrap().start_ms, 5_000);
        assert_eq!(cues.last().unwrap().end_ms, 9_000);
    }

    #[test]
    fn temporary_wav_is_valid_and_removed_on_drop() {
        let dir = tempfile::tempdir().unwrap();
        let path = {
            let temp = TempWav::create(dir.path(), &vec![0.2; SAMPLE_RATE as usize]).unwrap();
            let path = temp.0.clone();
            let wave = sherpa_onnx::Wave::read(path.to_str().unwrap()).unwrap();
            assert_eq!(wave.sample_rate(), SAMPLE_RATE);
            assert_eq!(wave.samples().len(), SAMPLE_RATE as usize);
            path
        };
        assert!(!path.exists());
    }

    #[test]
    fn provider_error_is_bounded_and_keeps_no_control_bytes() {
        let body = format!("bad\u{0}{}", "x".repeat(MAX_PROVIDER_ERROR_CHARS + 20));
        let sanitized = sanitize_provider_error(&body, "provider-secret");
        assert!(sanitized.chars().count() <= MAX_PROVIDER_ERROR_CHARS);
        assert!(!sanitized.contains('\u{0}'));
        assert!(
            !sanitize_provider_error("bad provider-secret", "provider-secret")
                .contains("provider-secret")
        );
    }

    #[tokio::test]
    async fn openai_compatible_request_sends_bearer_and_expected_multipart_fields() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let listener_endpoint = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            let mut body_offset = None;
            let mut content_length = None;
            loop {
                let mut buffer = [0_u8; 4096];
                let count = socket.read(&mut buffer).await.unwrap();
                assert!(count > 0, "client closed before sending a complete request");
                request.extend_from_slice(&buffer[..count]);
                if body_offset.is_none() {
                    if let Some(offset) = request.windows(4).position(|bytes| bytes == b"\r\n\r\n")
                    {
                        let header_end = offset + 4;
                        let headers = String::from_utf8_lossy(&request[..header_end]);
                        content_length = headers.lines().find_map(|line| {
                            let (name, value) = line.split_once(':')?;
                            name.eq_ignore_ascii_case("content-length")
                                .then(|| value.trim().parse::<usize>().ok())
                                .flatten()
                        });
                        body_offset = Some(header_end);
                    }
                }
                if let (Some(offset), Some(length)) = (body_offset, content_length) {
                    if request.len() >= offset + length {
                        break;
                    }
                }
            }

            let request_text = String::from_utf8_lossy(&request);
            let request_lower = request_text.to_ascii_lowercase();
            assert!(request_text.starts_with("POST /v1/audio/transcriptions HTTP/1.1"));
            assert!(request_lower.contains("authorization: bearer test-secret"));
            assert!(request_lower.contains("content-type: multipart/form-data; boundary="));
            let body = &request_text[body_offset.unwrap()..];
            assert!(body.contains("name=\"file\"; filename=\"finalsub-chunk.wav\""));
            assert!(body.contains("name=\"model\""));
            assert!(body.contains("whisper-large-v3-turbo"));
            assert!(body.contains("name=\"response_format\""));
            assert!(body.contains("verbose_json"));
            assert!(body.contains("name=\"timestamp_granularities[]\""));
            assert!(body.contains("name=\"language\""));
            assert!(body.contains("\r\n\r\nzh\r\n"));
            assert!(!body.contains("test-secret"));

            let response_body = r#"{"text":"protocol boundary ok"}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                response_body.len(),
                response_body
            );
            socket.write_all(response.as_bytes()).await.unwrap();
        });

        let config = CloudAsrConfig {
            protocol: CloudAsrProtocol::OpenAiCompatible,
            endpoint: format!("http://{listener_endpoint}/v1"),
            model: "whisper-large-v3-turbo".into(),
            api_key: "test-secret".into(),
            api_secret: None,
            account_id: None,
            timeout_seconds: 10,
            retry_times: 0,
            request_concurrency: 1,
            request_interval_ms: 0,
            proxy_url: None,
            state_dir: None,
        };
        let engine = test_engine(config);
        let response = engine
            .send_request(b"RIFF-finalsub-test", Some("zh-CN"), true)
            .await
            .unwrap();
        assert_eq!(response.text, "protocol boundary ok");
        server.await.unwrap();
    }

    #[tokio::test]
    async fn elevenlabs_request_uses_scoped_key_and_word_timestamps() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let listener_endpoint = listener.local_addr().unwrap();
        let response_body = r#"{"text":"Hello world.","words":[{"text":"Hello","start":0.0,"end":0.5,"type":"word"},{"text":" ","type":"spacing"},{"text":"world.","start":0.6,"end":1.0,"type":"word"}]}"#;
        let server = tokio::spawn(capture_request(listener, response_body));
        let engine = test_engine(CloudAsrConfig {
            protocol: CloudAsrProtocol::ElevenLabs,
            endpoint: format!("http://{listener_endpoint}"),
            model: "scribe_v2".into(),
            api_key: "eleven-secret".into(),
            api_secret: None,
            account_id: None,
            timeout_seconds: 10,
            retry_times: 0,
            request_concurrency: 1,
            request_interval_ms: 0,
            proxy_url: None,
            state_dir: None,
        });
        let response = engine
            .send_request(b"RIFF-elevenlabs-test", Some("en-US"), false)
            .await
            .unwrap();
        let request = server.await.unwrap();
        let request_text = String::from_utf8_lossy(&request);
        let request_lower = request_text.to_ascii_lowercase();
        let body_offset = request
            .windows(4)
            .position(|bytes| bytes == b"\r\n\r\n")
            .unwrap()
            + 4;
        let body = String::from_utf8_lossy(&request[body_offset..]);
        assert!(request_text.starts_with("POST /v1/speech-to-text HTTP/1.1"));
        assert!(request_lower.contains("xi-api-key: eleven-secret"));
        assert!(body.contains("name=\"model_id\""));
        assert!(body.contains("scribe_v2"));
        assert!(body.contains("name=\"timestamps_granularity\""));
        assert!(body.contains("name=\"language_code\""));
        assert!(!body.contains("eleven-secret"));
        assert_eq!(response.words.len(), 2);
        assert_eq!(response.words[1].word, "world.");
    }

    #[tokio::test]
    async fn deepgram_request_uses_token_auth_raw_audio_and_parses_words() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let listener_endpoint = listener.local_addr().unwrap();
        let response_body = r#"{"results":{"channels":[{"alternatives":[{"transcript":"Hello world.","words":[{"word":"hello","punctuated_word":"Hello","start":0.0,"end":0.5},{"word":"world","punctuated_word":"world.","start":0.6,"end":1.0}]}]}]}}"#;
        let server = tokio::spawn(capture_request(listener, response_body));
        let engine = test_engine(CloudAsrConfig {
            protocol: CloudAsrProtocol::Deepgram,
            endpoint: format!("http://{listener_endpoint}"),
            model: "nova-3".into(),
            api_key: "deepgram-secret".into(),
            api_secret: None,
            account_id: None,
            timeout_seconds: 10,
            retry_times: 0,
            request_concurrency: 1,
            request_interval_ms: 0,
            proxy_url: None,
            state_dir: None,
        });
        let audio = b"RIFF-deepgram-test";
        let response = engine
            .send_request(audio, Some("zh-CN"), false)
            .await
            .unwrap();
        let request = server.await.unwrap();
        let body_offset = request
            .windows(4)
            .position(|bytes| bytes == b"\r\n\r\n")
            .unwrap()
            + 4;
        let headers = String::from_utf8_lossy(&request[..body_offset]);
        let headers_lower = headers.to_ascii_lowercase();
        assert!(headers.starts_with("POST /v1/listen?"));
        assert!(headers.contains("model=nova-3"));
        assert!(headers.contains("smart_format=true"));
        assert!(headers.contains("utterances=true"));
        assert!(headers.contains("language=zh"));
        assert!(headers_lower.contains("authorization: token deepgram-secret"));
        assert!(headers_lower.contains("content-type: audio/wav"));
        assert_eq!(&request[body_offset..], audio);
        assert_eq!(response.words.len(), 2);
        assert_eq!(response.words[0].word, "Hello");
        assert_eq!(response.words[1].word, "world.");
    }

    #[tokio::test]
    async fn gladia_request_uploads_initializes_polls_and_parses_words() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let listener_endpoint = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let upload = capture_next_request(
                &listener,
                r#"{"audio_url":"https://api.gladia.io/file/finalsub-test"}"#,
            )
            .await;
            let init = capture_next_request(
                &listener,
                r#"{"id":"job-123","result_url":"https://ignored.example/job-123"}"#,
            )
            .await;
            let poll = capture_next_request(
                &listener,
                r#"{"status":"done","result":{"transcription":{"full_transcript":"Hello world.","utterances":[{"text":"Hello world.","start":0.0,"end":1.0,"words":[{"word":"Hello","start":0.0,"end":0.5},{"word":" world.","start":0.6,"end":1.0}]}]}}}"#,
            )
            .await;
            [upload, init, poll]
        });
        let engine = test_engine(CloudAsrConfig {
            protocol: CloudAsrProtocol::Gladia,
            endpoint: format!("http://{listener_endpoint}"),
            model: "solaria-1".into(),
            api_key: "gladia-secret".into(),
            api_secret: None,
            account_id: None,
            timeout_seconds: 10,
            retry_times: 0,
            request_concurrency: 1,
            request_interval_ms: 0,
            proxy_url: None,
            state_dir: None,
        });
        let response = engine
            .send_request(b"RIFF-gladia-test", Some("en-US"), false)
            .await
            .unwrap();
        let [upload, init, poll] = server.await.unwrap();
        let upload_text = String::from_utf8_lossy(&upload);
        let init_text = String::from_utf8_lossy(&init);
        let poll_text = String::from_utf8_lossy(&poll);
        assert!(upload_text.starts_with("POST /v2/upload HTTP/1.1"));
        assert!(upload_text
            .to_ascii_lowercase()
            .contains("x-gladia-key: gladia-secret"));
        assert!(upload_text.contains("name=\"audio\"; filename=\"finalsub-chunk.wav\""));
        assert!(init_text.starts_with("POST /v2/pre-recorded HTTP/1.1"));
        assert!(init_text.contains("\"model\":\"solaria-1\""));
        assert!(init_text.contains("\"languages\":[\"en\"]"));
        assert!(poll_text.starts_with("GET /v2/pre-recorded/job-123 HTTP/1.1"));
        assert!(!init_text.contains("ignored.example"));
        assert_eq!(response.text, "Hello world.");
        assert_eq!(response.words.len(), 2);
        assert_eq!(response.words[1].word, " world.");
        assert_eq!(response.segments.len(), 1);
    }

    #[tokio::test]
    async fn volcengine_request_uses_api_key_headers_and_millisecond_timestamps() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let listener_endpoint = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            capture_next_request_with_headers(
                &listener,
                r#"{"result":{"text":"你好。","utterances":[{"text":"你好。","start_time":100,"end_time":900,"words":[{"text":"你","start_time":100,"end_time":400},{"text":"好","start_time":450,"end_time":800}]}]}}"#,
                "X-Api-Status-Code: 20000000\r\nX-Api-Message: OK\r\n",
            )
            .await
        });
        let engine = test_engine(CloudAsrConfig {
            protocol: CloudAsrProtocol::Volcengine,
            endpoint: format!("http://{listener_endpoint}"),
            model: "bigmodel".into(),
            api_key: "volc-secret".into(),
            api_secret: None,
            account_id: None,
            timeout_seconds: 10,
            retry_times: 0,
            request_concurrency: 1,
            request_interval_ms: 0,
            proxy_url: None,
            state_dir: None,
        });
        let response = engine
            .send_request(b"RIFF-volc-test", Some("zh-CN"), false)
            .await
            .unwrap();
        let request = server.await.unwrap();
        let request_text = String::from_utf8_lossy(&request);
        let request_lower = request_text.to_ascii_lowercase();
        let body_offset = request
            .windows(4)
            .position(|bytes| bytes == b"\r\n\r\n")
            .unwrap()
            + 4;
        let body = String::from_utf8_lossy(&request[body_offset..]);
        assert!(request_text.starts_with("POST /api/v3/auc/bigmodel/recognize/flash HTTP/1.1"));
        assert!(request_lower.contains("x-api-key: volc-secret"));
        assert!(request_lower.contains("x-api-resource-id: volc.bigasr.auc_turbo"));
        assert!(request_lower.contains("x-api-sequence: -1"));
        assert!(body.contains("\"model_name\":\"bigmodel\""));
        assert!(body.contains("\"show_utterances\":true"));
        assert!(body.contains("UklGRi12b2xjLXRlc3Q="));
        assert!(!body.contains("volc-secret"));
        assert_eq!(response.words.len(), 2);
        assert_eq!(response.words[0].start, 0.1);
        assert_eq!(response.words[1].end, 0.8);
        assert_eq!(response.segments[0].end, 0.9);
    }

    #[tokio::test]
    async fn tencent_request_signs_sorted_query_and_uploads_raw_audio() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let listener_endpoint = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            capture_next_request(
                &listener,
                r#"{"code":0,"message":"success","flash_result":[{"text":"你好。","sentence_list":[{"text":"你好。","start_time":0,"end_time":900,"word_list":[{"word":"你好","start_time":100,"end_time":800}]}]}]}"#,
            )
            .await
        });
        let engine = test_engine(CloudAsrConfig {
            protocol: CloudAsrProtocol::Tencent,
            endpoint: format!("http://{listener_endpoint}"),
            model: "standard".into(),
            api_key: "AKIDtest".into(),
            api_secret: Some("TestSecretKey".into()),
            account_id: Some("1300000000".into()),
            timeout_seconds: 10,
            retry_times: 0,
            request_concurrency: 1,
            request_interval_ms: 0,
            proxy_url: None,
            state_dir: None,
        });
        let audio = b"RIFF-tencent-test";
        let response = engine
            .send_request(audio, Some("zh-CN"), false)
            .await
            .unwrap();
        let request = server.await.unwrap();
        let body_offset = request
            .windows(4)
            .position(|bytes| bytes == b"\r\n\r\n")
            .unwrap()
            + 4;
        let headers = String::from_utf8_lossy(&request[..body_offset]);
        let target = headers
            .lines()
            .next()
            .unwrap()
            .split_whitespace()
            .nth(1)
            .unwrap();
        let (path, query) = target.split_once('?').unwrap();
        assert_eq!(path, "/asr/flash/v1/1300000000");
        assert!(query.contains("engine_type=16k_zh"));
        assert!(query.contains("secretid=AKIDtest"));
        let authorization = headers
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("authorization")
                    .then(|| value.trim())
            })
            .unwrap();
        assert_eq!(
            authorization,
            sign_tencent_request("TestSecretKey", "1300000000", query).unwrap()
        );
        assert_eq!(&request[body_offset..], audio);
        assert_eq!(response.text, "你好。");
        assert_eq!(response.words[0].start, 0.1);
        assert_eq!(response.segments[0].end, 0.9);
    }

    #[tokio::test]
    async fn aliyun_request_fetches_token_then_uploads_raw_audio() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let listener_endpoint = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let token = capture_next_request(
                &listener,
                r#"{"Token":{"Id":"nls-test-token","ExpireTime":4102444800}}"#,
            )
            .await;
            let flash = capture_next_request(
                &listener,
                r#"{"status":20000000,"message":"SUCCESS","flash_result":{"sentences":[{"text":"北京的天气。","begin_time":100,"end_time":1000,"words":[{"text":"北京","punc":"","begin_time":"100","end_time":"500"},{"text":"天气","punc":"。","begin_time":"600","end_time":"1000"}]}]}}"#,
            )
            .await;
            [token, flash]
        });
        let engine = test_engine(CloudAsrConfig {
            protocol: CloudAsrProtocol::Aliyun,
            endpoint: format!("http://{listener_endpoint}"),
            model: "flash".into(),
            api_key: "LTAItest".into(),
            api_secret: Some("TestSecret".into()),
            account_id: Some("appkey-test".into()),
            timeout_seconds: 10,
            retry_times: 0,
            request_concurrency: 1,
            request_interval_ms: 0,
            proxy_url: None,
            state_dir: None,
        });
        let audio = b"RIFF-aliyun-test";
        let response = engine.send_request(audio, None, false).await.unwrap();
        let [token_request, flash_request] = server.await.unwrap();
        let token_text = String::from_utf8_lossy(&token_request);
        let flash_offset = flash_request
            .windows(4)
            .position(|bytes| bytes == b"\r\n\r\n")
            .unwrap()
            + 4;
        let flash_headers = String::from_utf8_lossy(&flash_request[..flash_offset]);
        assert!(token_text.starts_with("GET /token?Signature="));
        assert!(token_text.contains("AccessKeyId=LTAItest"));
        assert!(token_text.contains("Action=CreateToken"));
        assert!(!token_text.contains("TestSecret"));
        assert!(flash_headers.starts_with("POST /stream/v1/FlashRecognizer?"));
        assert!(flash_headers.contains("appkey=appkey-test"));
        assert!(flash_headers.contains("token=nls-test-token"));
        assert!(flash_headers.contains("enable_word_level_result=true"));
        assert_eq!(&flash_request[flash_offset..], audio);
        assert_eq!(response.text, "北京的天气。");
        assert_eq!(response.words[1].word, "天气。");
        assert_eq!(response.words[1].end, 1.0);
    }

    #[tokio::test]
    async fn xfyun_request_uploads_polls_reuses_random_and_removes_pending_state() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let listener_endpoint = listener.local_addr().unwrap();
        let best = serde_json::json!({
            "st": {
                "bg": "100",
                "ed": "1000",
                "rt": [{
                    "ws": [
                        {"wb": 0, "we": 20, "cw": [{"w": "北京", "wp": "n"}]},
                        {"wb": 25, "we": 50, "cw": [{"w": "天气", "wp": "n"}]},
                        {"wb": 50, "we": 50, "cw": [{"w": "。", "wp": "p"}]}
                    ]
                }]
            }
        });
        let order_result = serde_json::json!({
            "lattice": [{"json_1best": best.to_string()}]
        })
        .to_string();
        let poll_body = serde_json::json!({
            "code": "000000",
            "content": {
                "orderInfo": {"status": 4, "failType": 0},
                "orderResult": order_result
            }
        })
        .to_string();
        let server = tokio::spawn(async move {
            let upload = capture_next_request(
                &listener,
                r#"{"code":"000000","content":{"orderId":"order-123","taskEstimateTime":0}}"#,
            )
            .await;
            let poll = capture_next_request(&listener, &poll_body).await;
            [upload, poll]
        });
        let state_dir = tempfile::tempdir().unwrap();
        let engine = test_engine(CloudAsrConfig {
            protocol: CloudAsrProtocol::Xfyun,
            endpoint: format!("http://{listener_endpoint}"),
            model: "autodialect".into(),
            api_key: "xfyun-api-key".into(),
            api_secret: Some("xfyun-api-secret".into()),
            account_id: Some("xfyun-appid".into()),
            timeout_seconds: 10,
            retry_times: 0,
            request_concurrency: 1,
            request_interval_ms: 0,
            proxy_url: None,
            state_dir: Some(state_dir.path().into()),
        });
        let audio = b"RIFF-xfyun-test";
        let pending_path = engine.pending_state_path(audio).unwrap();
        let response = engine
            .send_request(audio, Some("zh-CN"), false)
            .await
            .unwrap();
        let [upload, poll] = server.await.unwrap();
        let upload_text = String::from_utf8_lossy(&upload);
        let poll_text = String::from_utf8_lossy(&poll);
        let upload_target = upload_text
            .lines()
            .next()
            .unwrap()
            .split_whitespace()
            .nth(1)
            .unwrap();
        let poll_target = poll_text
            .lines()
            .next()
            .unwrap()
            .split_whitespace()
            .nth(1)
            .unwrap();
        assert!(upload_target.starts_with("/v2/upload?"));
        assert!(upload_target.contains("accessKeyId=xfyun-api-key"));
        assert!(upload_target.contains("appId=xfyun-appid"));
        assert!(upload_target.contains("durationCheckDisable=true"));
        assert!(upload_target.contains("fileName=audio.wav"));
        assert!(upload_target.contains("language=autodialect"));
        assert!(poll_target.starts_with("/v2/getResult?"));
        assert!(poll_target.contains("orderId=order-123"));
        let upload_random = upload_target
            .split('&')
            .find_map(|part| part.strip_prefix("signatureRandom="))
            .unwrap();
        let poll_random = poll_target
            .split('&')
            .find_map(|part| part.strip_prefix("signatureRandom="))
            .unwrap();
        assert_eq!(upload_random, poll_random);
        for request in [&upload_text, &poll_text] {
            assert!(!request.contains("xfyun-api-secret"));
            let target = request
                .lines()
                .next()
                .unwrap()
                .split_whitespace()
                .nth(1)
                .unwrap();
            let query = target.split_once('?').unwrap().1;
            let signature = request
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("signature").then(|| value.trim())
                })
                .unwrap();
            assert_eq!(
                signature,
                sign_xfyun_request("xfyun-api-secret", query).unwrap()
            );
        }
        assert_eq!(response.text, "北京天气。");
        assert_eq!(response.words[1].word, "天气。");
        assert!(!pending_path.exists());
    }

    #[tokio::test]
    async fn gladia_resumes_persisted_job_without_reuploading() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let listener_endpoint = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            capture_next_request(
                &listener,
                r#"{"status":"done","result":{"transcription":{"full_transcript":"resumed","utterances":[]}}}"#,
            )
            .await
        });
        let state_dir = tempfile::tempdir().unwrap();
        let engine = test_engine(CloudAsrConfig {
            protocol: CloudAsrProtocol::Gladia,
            endpoint: format!("http://{listener_endpoint}"),
            model: "solaria-1".into(),
            api_key: "gladia-secret".into(),
            api_secret: None,
            account_id: None,
            timeout_seconds: 10,
            retry_times: 0,
            request_concurrency: 1,
            request_interval_ms: 0,
            proxy_url: None,
            state_dir: Some(state_dir.path().into()),
        });
        let audio = b"RIFF-gladia-resume";
        let pending_path = engine.pending_state_path(audio).unwrap();
        engine
            .save_pending_job(
                audio,
                &PendingCloudJob {
                    protocol: "gladia".into(),
                    id: "job-resume-1".into(),
                    signature_random: None,
                    created_at: chrono::Utc::now().timestamp(),
                },
            )
            .await
            .unwrap();
        let response = engine.send_request(audio, Some("en"), false).await.unwrap();
        let request = String::from_utf8_lossy(&server.await.unwrap()).to_string();
        assert!(request.starts_with("GET /v2/pre-recorded/job-resume-1 HTTP/1.1"));
        assert_eq!(response.text, "resumed");
        assert!(!pending_path.exists());
    }
}
