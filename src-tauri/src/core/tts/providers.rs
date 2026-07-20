use kothok_edge_tts::{EdgeTts, Engine, TtsEvent};
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use reqwest::{Client, Response, Url};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::AsyncWriteExt;

use crate::core::secrets;
use crate::core::tts::TtsSynthesisResult;
use crate::error::{FinalSubError, Result};

use super::volcengine;

const PROVIDER_STORE_VERSION: u32 = 1;
const MAX_PROVIDERS: usize = 32;
const MAX_TEXT_BYTES: usize = 20_000;
const MAX_AUDIO_BYTES: usize = 64 * 1024 * 1024;
const MAX_ERROR_BYTES: usize = 16 * 1024;
const EDGE_PROVIDER_ENDPOINT: &str =
    "https://speech.platform.bing.com/consumer/speech/synthesize/readaloud";
const EDGE_OUTAGE_HINT: &str =
    "Edge TTS 是免费试用通道，依赖非公开 Read Aloud 接口，可能随时断供；请切换本地模型或 OpenAI 兼容服务。";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TtsProviderProtocol {
    OpenaiCompatible,
    AzureSpeech,
    Elevenlabs,
    EdgeTts,
    Volcengine,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TtsProviderProfile {
    pub id: String,
    pub name: String,
    pub protocol: TtsProviderProtocol,
    pub endpoint: String,
    pub model: String,
    pub voice: String,
    pub region: String,
    pub resource_id: String,
    pub text_upload_consent: bool,
    pub timeout_seconds: u32,
    pub request_concurrency: u32,
}

impl Default for TtsProviderProfile {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: "OpenAI TTS 1".into(),
            protocol: TtsProviderProtocol::OpenaiCompatible,
            endpoint: "https://api.openai.com/v1".into(),
            model: "gpt-4o-mini-tts".into(),
            voice: "alloy".into(),
            region: String::new(),
            resource_id: String::new(),
            text_upload_consent: false,
            timeout_seconds: 60,
            request_concurrency: 1,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct SaveTtsProviderRequest {
    pub id: Option<String>,
    pub name: String,
    pub protocol: TtsProviderProtocol,
    pub endpoint: String,
    pub model: String,
    pub voice: String,
    pub region: String,
    #[serde(default)]
    pub resource_id: String,
    pub text_upload_consent: bool,
    pub timeout_seconds: u32,
    pub request_concurrency: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CloudTtsSynthesisRequest {
    pub provider_id: String,
    pub text: String,
    pub voice: Option<String>,
    pub speed: Option<f32>,
    pub output_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CloudVoiceSummary {
    pub provider_id: String,
    pub voice_id: String,
    pub name: String,
    pub engine: String,
}

#[derive(Debug, Deserialize)]
struct ElevenlabsVoiceRecord {
    voice_id: String,
    name: String,
    #[serde(default)]
    category: String,
}

#[derive(Debug, Deserialize)]
struct ElevenlabsVoicesPage {
    #[serde(default)]
    voices: Vec<ElevenlabsVoiceRecord>,
    #[serde(default)]
    has_more: bool,
    next_page_token: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ElevenlabsCreateVoiceResponse {
    voice_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProviderStore {
    version: u32,
    profiles: Vec<TtsProviderProfile>,
}

impl Default for ProviderStore {
    fn default() -> Self {
        Self {
            version: PROVIDER_STORE_VERSION,
            profiles: Vec::new(),
        }
    }
}

fn store_path(app_config_dir: &Path) -> PathBuf {
    app_config_dir.join("tts").join("providers.json")
}

fn load_store(app_config_dir: &Path) -> Result<ProviderStore> {
    let path = store_path(app_config_dir);
    if !path.exists() {
        return Ok(ProviderStore::default());
    }
    let content = std::fs::read_to_string(path)?;
    let store: ProviderStore = serde_json::from_str(&content)?;
    if store.version != PROVIDER_STORE_VERSION {
        return Err(FinalSubError::Validation(format!(
            "不支持的 TTS 服务配置版本：{}",
            store.version
        )));
    }
    for profile in &store.profiles {
        validate_profile(profile)?;
    }
    Ok(store)
}

fn save_store(app_config_dir: &Path, store: &ProviderStore) -> Result<()> {
    if store.profiles.len() > MAX_PROVIDERS {
        return Err(FinalSubError::Validation(format!(
            "在线 TTS 服务不能超过 {MAX_PROVIDERS} 个"
        )));
    }
    let path = store_path(app_config_dir);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension("json.tmp");
    std::fs::write(&temporary, serde_json::to_string_pretty(store)?)?;
    std::fs::rename(&temporary, path)?;
    Ok(())
}

fn validate_id(id: &str) -> Result<()> {
    uuid::Uuid::parse_str(id)
        .map(|_| ())
        .map_err(|_| FinalSubError::Validation("TTS 服务实例 ID 无效".into()))
}

fn validate_http_url(raw: &str, label: &str) -> Result<Url> {
    let parsed = Url::parse(raw.trim())
        .map_err(|_| FinalSubError::Validation(format!("{label} 必须是有效 URL")))?;
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
        return Err(FinalSubError::Validation(format!(
            "{label} 只支持 http:// 或 https:// 地址"
        )));
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(FinalSubError::Validation(format!(
            "{label} 不能在 URL 中包含账号或密码"
        )));
    }
    Ok(parsed)
}

fn azure_endpoint(profile: &TtsProviderProfile) -> Result<String> {
    let explicit = profile.endpoint.trim();
    if !explicit.is_empty() {
        return Ok(validate_http_url(explicit, "Azure TTS Endpoint")?
            .to_string()
            .trim_end_matches('/')
            .to_string());
    }
    let region = profile.region.trim();
    if region.is_empty()
        || !region
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    {
        return Err(FinalSubError::Validation(
            "Azure Speech 需要有效的 Region".into(),
        ));
    }
    Ok(format!(
        "https://{region}.tts.speech.microsoft.com/cognitiveservices/v1"
    ))
}

pub fn resolved_provider_endpoint(profile: &TtsProviderProfile) -> Result<String> {
    match profile.protocol {
        TtsProviderProtocol::AzureSpeech => azure_endpoint(profile),
        TtsProviderProtocol::OpenaiCompatible | TtsProviderProtocol::Elevenlabs => {
            Ok(validate_http_url(&profile.endpoint, "TTS Endpoint")?
                .to_string()
                .trim_end_matches('/')
                .to_string())
        }
        TtsProviderProtocol::EdgeTts => Ok(EDGE_PROVIDER_ENDPOINT.to_string()),
        TtsProviderProtocol::Volcengine => Ok(volcengine::VOLC_TTS_URL.to_string()),
    }
}

pub fn provider_secret_id(profile_id: &str) -> String {
    format!("tts-provider-{profile_id}")
}

fn validate_profile(profile: &TtsProviderProfile) -> Result<()> {
    validate_id(&profile.id)?;
    let name = profile.name.trim();
    if name.is_empty() || name.len() > 80 || name.chars().any(char::is_control) {
        return Err(FinalSubError::Validation(
            "TTS 服务实例名称不能为空、不能超过 80 字节或包含控制字符".into(),
        ));
    }
    let model = profile.model.trim();
    if !matches!(
        profile.protocol,
        TtsProviderProtocol::AzureSpeech
            | TtsProviderProtocol::EdgeTts
            | TtsProviderProtocol::Volcengine
    ) && (model.is_empty() || model.len() > 160 || model.chars().any(char::is_control))
    {
        return Err(FinalSubError::Validation("TTS 模型名称无效".into()));
    }
    if profile.protocol == TtsProviderProtocol::EdgeTts {
        if !profile.endpoint.trim().is_empty() || !model.is_empty() {
            return Err(FinalSubError::Validation(
                "Edge TTS 免费试用档不需要 Endpoint 或模型名称".into(),
            ));
        }
        if !profile.region.trim().is_empty() {
            validate_edge_locale(profile.region.trim())?;
        }
    }
    if profile.protocol == TtsProviderProtocol::Volcengine {
        if !profile.endpoint.trim().is_empty() || !profile.model.trim().is_empty() {
            return Err(FinalSubError::Validation(
                "豆包 TTS 使用固定官方 Endpoint，不需要自定义 Endpoint 或模型名称".into(),
            ));
        }
        let resource_id = if profile.resource_id.trim().is_empty() {
            volcengine::DEFAULT_RESOURCE_ID
        } else {
            profile.resource_id.trim()
        };
        if !volcengine::is_valid_resource_id(resource_id) {
            return Err(FinalSubError::Validation(
                "豆包 TTS 资源版本必须是 seed-tts-2.0、seed-tts-1.0 或 seed-tts-1.0-concurr".into(),
            ));
        }
    }
    validate_voice(&profile.voice)?;
    resolved_provider_endpoint(profile)?;
    if !(5..=300).contains(&profile.timeout_seconds) {
        return Err(FinalSubError::Validation(
            "TTS 请求超时必须在 5-300 秒之间".into(),
        ));
    }
    if !(1..=8).contains(&profile.request_concurrency) {
        return Err(FinalSubError::Validation(
            "TTS 请求并发必须在 1-8 之间".into(),
        ));
    }
    Ok(())
}

fn validate_voice(value: &str) -> Result<String> {
    let voice = value.trim();
    if voice.is_empty() || voice.len() > 200 || voice.chars().any(char::is_control) {
        return Err(FinalSubError::Validation("TTS 音色 ID 无效".into()));
    }
    Ok(voice.to_string())
}

fn validate_edge_locale(value: &str) -> Result<String> {
    let locale = value.trim();
    let mut parts = locale.split('-');
    let language = parts.next().unwrap_or_default();
    let region = parts.next().unwrap_or_default();
    if locale.is_empty()
        || locale.len() > 35
        || locale.chars().any(char::is_control)
        || !(2..=3).contains(&language.len())
        || !language.bytes().all(|byte| byte.is_ascii_alphabetic())
        || !(2..=3).contains(&region.len())
        || !(region.bytes().all(|byte| byte.is_ascii_alphabetic())
            || region.bytes().all(|byte| byte.is_ascii_digit()))
        || parts.any(|part| {
            part.is_empty()
                || part.len() > 8
                || !part.bytes().all(|byte| byte.is_ascii_alphanumeric())
        })
    {
        return Err(FinalSubError::Validation(
            "Edge TTS 语言区域必须是类似 zh-CN 或 en-US 的标识".into(),
        ));
    }
    Ok(locale.to_string())
}

fn edge_locale(profile_locale: &str, voice: &str) -> String {
    if !profile_locale.trim().is_empty() {
        return profile_locale.trim().to_string();
    }
    let mut parts = voice.split('-');
    let inferred = match (parts.next(), parts.next()) {
        (Some(language), Some(region)) => format!("{language}-{region}"),
        _ => String::new(),
    };
    validate_edge_locale(&inferred).unwrap_or_else(|_| "en-US".into())
}

fn edge_rate(speed: f32) -> String {
    let percent = ((speed.clamp(0.5, 3.0) - 1.0) * 100.0).round() as i32;
    format!("{percent:+}%")
}

pub fn list_providers(app_config_dir: &Path) -> Result<Vec<TtsProviderProfile>> {
    Ok(load_store(app_config_dir)?.profiles)
}

pub fn save_provider(
    app_config_dir: &Path,
    request: SaveTtsProviderRequest,
) -> Result<TtsProviderProfile> {
    let profile = TtsProviderProfile {
        id: request
            .id
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
        name: request.name.trim().to_string(),
        protocol: request.protocol,
        endpoint: request.endpoint.trim().trim_end_matches('/').to_string(),
        model: request.model.trim().to_string(),
        voice: request.voice.trim().to_string(),
        region: request.region.trim().to_string(),
        resource_id: request.resource_id.trim().to_string(),
        text_upload_consent: request.text_upload_consent,
        timeout_seconds: request.timeout_seconds,
        request_concurrency: request.request_concurrency,
    };
    if profile.protocol == TtsProviderProtocol::Volcengine {
        // 豆包是品牌型单例，Endpoint/模型由协议固定，避免用户保存出一份
        // 看似不同但实际指向同一资源的重复实例。
        let mut normalized = profile.clone();
        normalized.endpoint.clear();
        normalized.model.clear();
        if normalized.resource_id.trim().is_empty() {
            normalized.resource_id = volcengine::DEFAULT_RESOURCE_ID.into();
        }
        validate_profile(&normalized)?;
        let mut store = load_store(app_config_dir)?;
        if let Some(existing) = store
            .profiles
            .iter_mut()
            .find(|existing| existing.id == normalized.id)
        {
            *existing = normalized.clone();
        } else {
            if store
                .profiles
                .iter()
                .any(|existing| existing.protocol == TtsProviderProtocol::Volcengine)
            {
                return Err(FinalSubError::Validation(
                    "豆包 TTS 只允许配置一个实例；请编辑现有实例".into(),
                ));
            }
            if store.profiles.len() >= MAX_PROVIDERS {
                return Err(FinalSubError::Validation(format!(
                    "在线 TTS 服务不能超过 {MAX_PROVIDERS} 个"
                )));
            }
            store.profiles.push(normalized.clone());
        }
        save_store(app_config_dir, &store)?;
        return Ok(normalized);
    }
    validate_profile(&profile)?;
    let mut store = load_store(app_config_dir)?;
    if let Some(existing) = store
        .profiles
        .iter_mut()
        .find(|existing| existing.id == profile.id)
    {
        *existing = profile.clone();
    } else {
        if store.profiles.len() >= MAX_PROVIDERS {
            return Err(FinalSubError::Validation(format!(
                "在线 TTS 服务不能超过 {MAX_PROVIDERS} 个"
            )));
        }
        store.profiles.push(profile.clone());
    }
    save_store(app_config_dir, &store)?;
    Ok(profile)
}

pub fn delete_provider(app_config_dir: &Path, provider_id: &str) -> Result<()> {
    validate_id(provider_id)?;
    let mut store = load_store(app_config_dir)?;
    let index = store
        .profiles
        .iter()
        .position(|profile| profile.id == provider_id)
        .ok_or_else(|| FinalSubError::Validation("TTS 服务实例不存在".into()))?;
    let profile = store.profiles.remove(index);
    save_store(app_config_dir, &store)?;
    if profile.protocol != TtsProviderProtocol::EdgeTts {
        let endpoint = resolved_provider_endpoint(&profile)?;
        let secret_id = provider_secret_id(&profile.id);
        secrets::delete_provider_secret(&secret_id, &endpoint, "apiKey")
            .map_err(FinalSubError::Validation)?;
        if profile.protocol == TtsProviderProtocol::Volcengine {
            for field in ["cloneAppId", "cloneAccessToken"] {
                secrets::delete_provider_secret(&secret_id, &endpoint, field)
                    .map_err(FinalSubError::Validation)?;
            }
        }
    }
    Ok(())
}

pub(crate) fn find_provider(
    app_config_dir: &Path,
    provider_id: &str,
) -> Result<TtsProviderProfile> {
    validate_id(provider_id)?;
    load_store(app_config_dir)?
        .profiles
        .into_iter()
        .find(|profile| profile.id == provider_id)
        .ok_or_else(|| FinalSubError::Validation("TTS 服务实例不存在".into()))
}

fn elevenlabs_voice_endpoint(
    app_config_dir: &Path,
    provider_id: &str,
) -> Result<(TtsProviderProfile, String, String)> {
    let profile = find_provider(app_config_dir, provider_id)?;
    if profile.protocol != TtsProviderProtocol::Elevenlabs {
        return Err(FinalSubError::Validation(
            "该在线 TTS 实例不是 ElevenLabs，不能用于即时声音克隆".into(),
        ));
    }
    let endpoint = resolved_provider_endpoint(&profile)?;
    let parsed = Url::parse(&endpoint)
        .map_err(|_| FinalSubError::Validation("ElevenLabs Endpoint 无效".into()))?;
    if parsed.scheme() != "https"
        || parsed.query().is_some()
        || parsed.fragment().is_some()
        || !endpoint.ends_with("/v1")
    {
        return Err(FinalSubError::Validation(
            "ElevenLabs 声音克隆要求使用 HTTPS 且 Endpoint 以 /v1 结尾".into(),
        ));
    }
    let api_key =
        secrets::get_provider_secret(&provider_secret_id(&profile.id), &endpoint, "apiKey")
            .map_err(FinalSubError::Validation)?
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| {
                FinalSubError::Validation("该 ElevenLabs 实例尚未保存 API Key".into())
            })?;
    Ok((profile, endpoint, api_key))
}

fn cloud_voice_error(provider: &str, status: u16, detail: &str) -> FinalSubError {
    let hint = match status {
        401 | 403 => "请检查 API Key、Voices 权限与当前套餐",
        413 => "参考音频过大，请缩短素材后重试",
        422 => "参考音频未通过平台校验，请改用更清晰的单人录音",
        429 => "请求额度或音色槽位受限，请稍后重试或检查套餐",
        _ => "请检查服务状态与音色配置",
    };
    FinalSubError::Validation(format!(
        "{provider} 返回 HTTP {status}{}。{hint}",
        if detail.is_empty() {
            String::new()
        } else {
            format!("：{detail}")
        }
    ))
}

async fn cloud_voice_response(
    response: Response,
    provider: &str,
    success_limit: usize,
) -> Result<Vec<u8>> {
    let status = response.status();
    let limit = if status.is_success() {
        success_limit
    } else {
        MAX_ERROR_BYTES
    };
    let bytes = response_bytes_limited(response, limit, Arc::new(AtomicBool::new(false))).await?;
    if !status.is_success() {
        return Err(cloud_voice_error(
            provider,
            status.as_u16(),
            &sanitize_provider_detail(&bytes),
        ));
    }
    Ok(bytes)
}

fn sanitize_remote_voice_name(raw: &str, fallback: &str) -> String {
    let value = raw
        .chars()
        .filter(|character| !character.is_control())
        .take(80)
        .collect::<String>();
    let value = value.trim();
    if value.is_empty() {
        fallback.to_string()
    } else {
        value.to_string()
    }
}

pub async fn create_elevenlabs_voice(
    app_config_dir: &Path,
    provider_id: &str,
    name: &str,
    reference_audio_path: &Path,
    remove_background_noise: bool,
) -> Result<String> {
    let (profile, endpoint, api_key) = elevenlabs_voice_endpoint(app_config_dir, provider_id)?;
    let metadata = std::fs::metadata(reference_audio_path)?;
    if !reference_audio_path.is_file()
        || metadata.len() == 0
        || metadata.len() > MAX_AUDIO_BYTES as u64
    {
        return Err(FinalSubError::Validation(
            "ElevenLabs 参考音频为空或超过 64 MB".into(),
        ));
    }
    let audio = std::fs::read(reference_audio_path)?;
    let audio_part = reqwest::multipart::Part::bytes(audio)
        .file_name("reference.wav")
        .mime_str("audio/wav")
        .map_err(|error| {
            FinalSubError::Validation(format!("无法创建 ElevenLabs 音频表单：{error}"))
        })?;
    let form = reqwest::multipart::Form::new()
        .text("name", name.to_string())
        .text(
            "remove_background_noise",
            remove_background_noise.to_string(),
        )
        .part("files", audio_part);
    let client = http_client(profile.timeout_seconds.max(120))?;
    let response = client
        .post(format!("{endpoint}/voices/add"))
        .header("xi-api-key", api_key)
        .multipart(form)
        .send()
        .await
        .map_err(|error| {
            FinalSubError::Validation(format!("ElevenLabs 声音克隆请求失败：{error}"))
        })?;
    let bytes = cloud_voice_response(response, "ElevenLabs 声音克隆", 256 * 1024).await?;
    let payload: ElevenlabsCreateVoiceResponse = serde_json::from_slice(&bytes)
        .map_err(|_| FinalSubError::Validation("ElevenLabs 声音克隆响应缺少 voice_id".into()))?;
    validate_voice(&payload.voice_id)
}

pub async fn list_cloud_voices(
    app_config_dir: &Path,
    provider_id: &str,
) -> Result<Vec<CloudVoiceSummary>> {
    let (profile, endpoint, api_key) = elevenlabs_voice_endpoint(app_config_dir, provider_id)?;
    let base = endpoint
        .strip_suffix("/v1")
        .ok_or_else(|| FinalSubError::Validation("ElevenLabs Endpoint 必须以 /v1 结尾".into()))?;
    let client = http_client(profile.timeout_seconds.min(60))?;
    let mut next_page_token: Option<String> = None;
    let mut voices = Vec::new();
    let mut seen = HashSet::new();

    for page_index in 0..10 {
        let mut url = Url::parse(&format!("{base}/v2/voices"))
            .map_err(|_| FinalSubError::Validation("无法构造 ElevenLabs 音色列表地址".into()))?;
        {
            let mut query = url.query_pairs_mut();
            query.append_pair("category", "cloned");
            query.append_pair("page_size", "100");
            query.append_pair("include_total_count", "false");
            if let Some(token) = next_page_token.as_deref() {
                query.append_pair("next_page_token", token);
            }
        }
        let response = client
            .get(url)
            .header("xi-api-key", &api_key)
            .send()
            .await
            .map_err(|error| {
                FinalSubError::Validation(format!("读取 ElevenLabs 云端音色失败：{error}"))
            })?;
        let bytes = cloud_voice_response(response, "ElevenLabs 音色列表", 2 * 1024 * 1024).await?;
        let page: ElevenlabsVoicesPage = serde_json::from_slice(&bytes).map_err(|error| {
            FinalSubError::Validation(format!("ElevenLabs 音色列表响应无效：{error}"))
        })?;
        for voice in page.voices {
            let voice_id = match validate_voice(&voice.voice_id) {
                Ok(value) => value,
                Err(_) => continue,
            };
            if (!voice.category.is_empty() && voice.category != "cloned")
                || !seen.insert(voice_id.clone())
            {
                continue;
            }
            voices.push(CloudVoiceSummary {
                provider_id: provider_id.to_string(),
                name: sanitize_remote_voice_name(&voice.name, &voice_id),
                voice_id,
                engine: "elevenlabs".into(),
            });
        }
        if !page.has_more {
            return Ok(voices);
        }
        next_page_token = page
            .next_page_token
            .filter(|token| !token.trim().is_empty());
        if next_page_token.is_none() {
            return Err(FinalSubError::Validation(
                "ElevenLabs 音色列表声明还有下一页，但没有返回分页令牌".into(),
            ));
        }
        if page_index == 9 {
            return Err(FinalSubError::Validation(
                "ElevenLabs 云端音色超过 1000 个，请先在平台缩小数量后重试".into(),
            ));
        }
    }
    Ok(voices)
}

pub async fn delete_elevenlabs_voice(
    app_config_dir: &Path,
    provider_id: &str,
    voice_id: &str,
) -> Result<()> {
    let (profile, endpoint, api_key) = elevenlabs_voice_endpoint(app_config_dir, provider_id)?;
    let voice_id = validate_voice(voice_id)?;
    let encoded = utf8_percent_encode(&voice_id, NON_ALPHANUMERIC);
    let client = http_client(profile.timeout_seconds.min(60))?;
    let response = client
        .delete(format!("{endpoint}/voices/{encoded}"))
        .header("xi-api-key", api_key)
        .send()
        .await
        .map_err(|error| {
            FinalSubError::Validation(format!("删除 ElevenLabs 云端音色失败：{error}"))
        })?;
    let _ = cloud_voice_response(response, "ElevenLabs 删除音色", 256 * 1024).await?;
    Ok(())
}

fn validate_synthesis_request(request: &CloudTtsSynthesisRequest) -> Result<(String, f32)> {
    let text = request.text.trim();
    if text.is_empty() || text.len() > MAX_TEXT_BYTES || text.contains('\0') {
        return Err(FinalSubError::Validation(format!(
            "配音文本不能为空、不能包含空字符，且不能超过 {MAX_TEXT_BYTES} 字节"
        )));
    }
    let speed = request.speed.unwrap_or(1.0);
    if !speed.is_finite() || !(0.25..=4.0).contains(&speed) {
        return Err(FinalSubError::Validation(
            "云端配音速度必须在 0.25-4.0 之间".into(),
        ));
    }
    Ok((text.to_string(), speed))
}

fn output_paths(raw: &str) -> Result<(PathBuf, PathBuf)> {
    let output = PathBuf::from(raw.trim());
    if !output.is_absolute() || output.extension().and_then(|value| value.to_str()) != Some("wav") {
        return Err(FinalSubError::Validation(
            "云端 TTS 输出必须是绝对路径的 .wav 文件".into(),
        ));
    }
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temporary = output.with_extension("wav.generating");
    Ok((output, temporary))
}

fn http_client(timeout_seconds: u32) -> Result<Client> {
    Client::builder()
        .timeout(Duration::from_secs(timeout_seconds as u64))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|error| FinalSubError::Validation(format!("无法创建 TTS HTTP 客户端：{error}")))
}

fn format_size_limit(limit: usize) -> String {
    if limit >= 1024 * 1024 {
        format!("{} MB", limit / 1024 / 1024)
    } else {
        format!("{} KB", limit / 1024)
    }
}

async fn response_bytes_limited(
    mut response: Response,
    limit: usize,
    cancelled: Arc<AtomicBool>,
) -> Result<Vec<u8>> {
    if response
        .content_length()
        .is_some_and(|length| length > limit as u64)
    {
        return Err(FinalSubError::Validation(format!(
            "TTS 响应超过 {} 限制",
            format_size_limit(limit)
        )));
    }
    let mut bytes = Vec::new();
    loop {
        let chunk = tokio::select! {
            result = response.chunk() => result
                .map_err(|error| FinalSubError::Validation(format!("读取 TTS 响应失败：{error}")))?,
            _ = wait_cancelled(cancelled.clone()) => {
                return Err(FinalSubError::Validation("配音已取消".into()));
            }
        };
        let Some(chunk) = chunk else {
            break;
        };
        if bytes.len() + chunk.len() > limit {
            return Err(FinalSubError::Validation(format!(
                "TTS 响应超过 {} 限制",
                format_size_limit(limit)
            )));
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

fn sanitize_provider_detail(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes)
        .chars()
        .filter(|ch| !ch.is_control() || matches!(*ch, '\n' | '\t'))
        .take(2_000)
        .collect::<String>()
        .trim()
        .to_string()
}

async fn response_or_error(
    response: Response,
    provider_name: &str,
    cancelled: Arc<AtomicBool>,
) -> Result<Vec<u8>> {
    let status = response.status();
    if status.is_success() {
        return response_bytes_limited(response, MAX_AUDIO_BYTES, cancelled).await;
    }
    let detail = response_bytes_limited(response, MAX_ERROR_BYTES, cancelled)
        .await
        .unwrap_or_default();
    let detail = sanitize_provider_detail(&detail);
    let hint = match status.as_u16() {
        401 | 403 => "请检查 API Key、服务权限与资源区域",
        429 => "请求并发或额度受限，请降低并发或稍后重试",
        _ => "请检查 Endpoint、模型与音色配置",
    };
    Err(FinalSubError::Validation(format!(
        "{provider_name} 返回 HTTP {}{}。{hint}",
        status.as_u16(),
        if detail.is_empty() {
            String::new()
        } else {
            format!("：{detail}")
        }
    )))
}

async fn wait_cancelled(cancelled: Arc<AtomicBool>) {
    while !cancelled.load(Ordering::Relaxed) {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

async fn send_cancelable(
    request: reqwest::RequestBuilder,
    cancelled: Arc<AtomicBool>,
) -> Result<Response> {
    tokio::select! {
        response = request.send() => response.map_err(|error| FinalSubError::Validation(format!("TTS 网络请求失败：{error}"))),
        _ = wait_cancelled(cancelled) => Err(FinalSubError::Validation("配音已取消".into())),
    }
}

fn xml_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn azure_ssml(text: &str, voice: &str, speed: f32) -> String {
    let locale = voice.split('-').take(2).collect::<Vec<_>>().join("-");
    let locale = if locale.matches('-').count() == 1 {
        locale
    } else {
        "en-US".into()
    };
    let spoken = if (speed - 1.0).abs() < 0.005 {
        xml_escape(text)
    } else {
        let percent = ((speed.clamp(0.5, 2.0) - 1.0) * 100.0).round() as i32;
        format!(
            "<prosody rate=\"{percent:+}%\">{}</prosody>",
            xml_escape(text)
        )
    };
    format!(
        "<speak version=\"1.0\" xml:lang=\"{}\"><voice name=\"{}\">{spoken}</voice></speak>",
        xml_escape(&locale),
        xml_escape(voice)
    )
}

async fn synthesize_openai(
    client: &Client,
    profile: &TtsProviderProfile,
    api_key: &str,
    text: &str,
    voice: &str,
    speed: f32,
    cancelled: Arc<AtomicBool>,
) -> Result<Vec<u8>> {
    let endpoint = resolved_provider_endpoint(profile)?;
    let url = format!("{endpoint}/audio/speech");
    let request = client
        .post(url)
        .bearer_auth(api_key)
        .json(&serde_json::json!({
            "model": profile.model,
            "input": text,
            "voice": voice,
            "speed": speed,
            "response_format": "wav"
        }));
    let response = send_cancelable(request, cancelled.clone()).await?;
    response_or_error(response, "OpenAI 兼容 TTS", cancelled).await
}

async fn synthesize_azure(
    client: &Client,
    profile: &TtsProviderProfile,
    api_key: &str,
    text: &str,
    voice: &str,
    speed: f32,
    cancelled: Arc<AtomicBool>,
) -> Result<Vec<u8>> {
    let request = client
        .post(resolved_provider_endpoint(profile)?)
        .header("Ocp-Apim-Subscription-Key", api_key)
        .header("Content-Type", "application/ssml+xml")
        .header("X-Microsoft-OutputFormat", "riff-24khz-16bit-mono-pcm")
        .header("User-Agent", "FinalSub")
        .body(azure_ssml(text, voice, speed));
    let response = send_cancelable(request, cancelled.clone()).await?;
    response_or_error(response, "Azure Speech TTS", cancelled).await
}

async fn synthesize_elevenlabs(
    client: &Client,
    profile: &TtsProviderProfile,
    api_key: &str,
    text: &str,
    voice: &str,
    speed: f32,
    cancelled: Arc<AtomicBool>,
) -> Result<Vec<u8>> {
    let endpoint = resolved_provider_endpoint(profile)?;
    let voice = utf8_percent_encode(voice, NON_ALPHANUMERIC);
    let url = format!("{endpoint}/text-to-speech/{voice}?output_format=pcm_24000");
    let request = client
        .post(url)
        .header("xi-api-key", api_key)
        .json(&serde_json::json!({
            "text": text,
            "model_id": profile.model,
            "voice_settings": { "speed": speed.clamp(0.7, 1.2) }
        }));
    let response = send_cancelable(request, cancelled.clone()).await?;
    response_or_error(response, "ElevenLabs TTS", cancelled).await
}

async fn synthesize_volcengine(
    client: &Client,
    profile: &TtsProviderProfile,
    api_key: &str,
    text: &str,
    voice: &str,
    speed: f32,
    cancelled: Arc<AtomicBool>,
) -> Result<Vec<u8>> {
    let text_chars = volcengine::text_char_count(text);
    if !volcengine::text_is_within_limit(text) {
        return Err(FinalSubError::Validation(format!(
            "豆包 TTS 单条文本不能超过 {} 个字符（当前 {text_chars} 个）；请拆分该行字幕",
            volcengine::MAX_TEXT_CHARS
        )));
    }
    let resource_id = volcengine::resource_id_for_voice(voice, &profile.resource_id);
    let request_id = uuid::Uuid::new_v4().to_string();
    let mut request = client.post(volcengine::VOLC_TTS_URL);
    for (name, value) in volcengine::build_headers(api_key, &resource_id, &request_id) {
        request = request.header(name, value);
    }
    let request = request.json(&volcengine::build_request_body(text, voice, Some(speed)));
    let response = send_cancelable(request, cancelled.clone()).await?;
    let status = response.status();
    // 错误响应也可能以 chunked JSON 返回，但不应享受音频响应的 64 MB
    // 内存预算；16 KB 已足够保留业务码与可行动提示。
    let response_limit = if status.is_success() {
        MAX_AUDIO_BYTES
    } else {
        MAX_ERROR_BYTES
    };
    let body = response_bytes_limited(response, response_limit, cancelled).await?;
    let body_text = String::from_utf8_lossy(&body);
    let parsed = volcengine::parse_stream(&body_text);
    if !status.is_success() || parsed.error_code.is_some() {
        return Err(FinalSubError::Validation(volcengine::error_hint(
            status.as_u16(),
            parsed.error_code,
            if parsed.message.is_empty() {
                &body_text
            } else {
                &parsed.message
            },
        )));
    }
    if parsed.end_code != Some(20_000_000) {
        return Err(FinalSubError::Validation(
            "豆包 TTS 流式响应未正常结束，请稍后重试".into(),
        ));
    }
    if parsed.pcm.is_empty() || !parsed.pcm.len().is_multiple_of(2) {
        return Err(FinalSubError::Validation(
            "豆包 TTS 返回了空的或不完整的 PCM 音频".into(),
        ));
    }
    Ok(parsed.pcm)
}

fn edge_failure(detail: impl AsRef<str>) -> FinalSubError {
    let detail = sanitize_provider_detail(detail.as_ref().as_bytes());
    let suffix = if detail.is_empty() {
        String::new()
    } else {
        format!("：{detail}")
    };
    FinalSubError::Validation(format!("Edge TTS 合成失败{suffix}。{EDGE_OUTAGE_HINT}"))
}

fn collect_edge_audio(events: Vec<TtsEvent>) -> Result<Vec<u8>> {
    let mut audio = Vec::new();
    for event in events {
        let TtsEvent::Audio(chunk) = event else {
            continue;
        };
        let next_len = audio
            .len()
            .checked_add(chunk.len())
            .ok_or_else(|| FinalSubError::Validation("Edge TTS 音频过大".into()))?;
        if next_len > MAX_AUDIO_BYTES {
            return Err(FinalSubError::Validation(format!(
                "Edge TTS 音频超过 {} 限制",
                format_size_limit(MAX_AUDIO_BYTES)
            )));
        }
        audio.extend_from_slice(&chunk);
    }
    if audio.is_empty() {
        return Err(edge_failure("服务端没有返回音频"));
    }
    Ok(audio)
}

async fn synthesize_edge(
    profile: &TtsProviderProfile,
    text: &str,
    voice: &str,
    speed: f32,
    cancelled: Arc<AtomicBool>,
) -> Result<Vec<u8>> {
    if cancelled.load(Ordering::Relaxed) {
        return Err(FinalSubError::Validation("配音已取消".into()));
    }
    // kothok-edge-tts 使用 rustls ring provider；重复调用是幂等的。
    kothok_edge_tts::init_tls();
    let locale = edge_locale(&profile.region, voice);
    let rate = edge_rate(speed);
    let synthesis = EdgeTts.synthesize(text, voice, &rate, &locale);
    let result = tokio::select! {
        result = tokio::time::timeout(
            Duration::from_secs(profile.timeout_seconds as u64),
            synthesis,
        ) => result,
        _ = wait_cancelled(cancelled.clone()) => {
            return Err(FinalSubError::Validation("配音已取消".into()));
        }
    };
    let events = match result {
        Ok(Ok(events)) => events,
        Ok(Err(error)) => return Err(edge_failure(error.to_string())),
        Err(_) => {
            return Err(edge_failure(format!(
                "请求超过 {} 秒",
                profile.timeout_seconds
            )));
        }
    };
    if cancelled.load(Ordering::Relaxed) {
        return Err(FinalSubError::Validation("配音已取消".into()));
    }
    collect_edge_audio(events)
}

fn write_pcm_wav(path: &Path, pcm: &[u8], sample_rate: u32) -> Result<u64> {
    if pcm.is_empty() || !pcm.len().is_multiple_of(2) {
        return Err(FinalSubError::Validation(
            "云端 TTS 返回的 PCM 音频为空或长度无效".into(),
        ));
    }
    let data_len =
        u32::try_from(pcm.len()).map_err(|_| FinalSubError::Validation("PCM 音频过大".into()))?;
    let riff_len = 36_u32
        .checked_add(data_len)
        .ok_or_else(|| FinalSubError::Validation("PCM 音频过大".into()))?;
    let mut wav = Vec::with_capacity(pcm.len() + 44);
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&riff_len.to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16_u32.to_le_bytes());
    wav.extend_from_slice(&1_u16.to_le_bytes());
    wav.extend_from_slice(&1_u16.to_le_bytes());
    wav.extend_from_slice(&sample_rate.to_le_bytes());
    wav.extend_from_slice(&(sample_rate * 2).to_le_bytes());
    wav.extend_from_slice(&2_u16.to_le_bytes());
    wav.extend_from_slice(&16_u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_len.to_le_bytes());
    wav.extend_from_slice(pcm);
    std::fs::write(path, wav)?;
    Ok((pcm.len() as u128 * 1000 / 2 / sample_rate as u128) as u64)
}

fn wav_duration_ms(bytes: &[u8]) -> Option<u64> {
    if bytes.len() < 44 || &bytes[..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return None;
    }
    let mut offset = 12_usize;
    let mut byte_rate = None;
    let mut data_len = None;
    while offset.checked_add(8)? <= bytes.len() {
        let id = &bytes[offset..offset + 4];
        let size = u32::from_le_bytes(bytes[offset + 4..offset + 8].try_into().ok()?) as usize;
        let start = offset + 8;
        let end = start.checked_add(size)?;
        if end > bytes.len() {
            return None;
        }
        if id == b"fmt " && size >= 12 {
            byte_rate = Some(u32::from_le_bytes(
                bytes[start + 8..start + 12].try_into().ok()?,
            ));
        } else if id == b"data" {
            data_len = Some(size as u64);
        }
        offset = end + (size % 2);
    }
    let rate = byte_rate?.max(1) as u64;
    Some(data_len? * 1000 / rate)
}

async fn transcode_to_wav(
    ffmpeg_path: &Path,
    audio: &[u8],
    output: &Path,
    temporary: &Path,
    cancelled: Arc<AtomicBool>,
) -> Result<u64> {
    let source = output.with_extension("tts-source");
    let mut source_file = tokio::fs::File::create(&source).await?;
    source_file.write_all(audio).await?;
    source_file.flush().await?;
    drop(source_file);
    let mut child = tokio::process::Command::new(ffmpeg_path)
        .args(["-hide_banner", "-loglevel", "error", "-y", "-i"])
        .arg(&source)
        .args([
            "-vn",
            "-ac",
            "1",
            "-ar",
            "24000",
            "-c:a",
            "pcm_s16le",
            "-f",
            "wav",
        ])
        .arg(temporary)
        .kill_on_drop(true)
        .spawn()
        .map_err(|error| FinalSubError::Validation(format!("无法启动 FFmpeg：{error}")))?;
    let status = tokio::select! {
        result = child.wait() => result.map_err(FinalSubError::Io)?,
        _ = wait_cancelled(cancelled) => {
            let _ = child.kill().await;
            let _ = tokio::fs::remove_file(&source).await;
            let _ = tokio::fs::remove_file(temporary).await;
            return Err(FinalSubError::Validation("配音已取消".into()));
        }
    };
    let _ = tokio::fs::remove_file(&source).await;
    if !status.success() {
        let _ = tokio::fs::remove_file(temporary).await;
        return Err(FinalSubError::Validation(
            "云端 TTS 音频转为 PCM WAV 失败".into(),
        ));
    }
    let wav = tokio::fs::read(temporary).await?;
    wav_duration_ms(&wav).ok_or_else(|| FinalSubError::Validation("转码后的 WAV 文件无效".into()))
}

pub async fn synthesize_cloud(
    app_config_dir: &Path,
    ffmpeg_path: &Path,
    request: CloudTtsSynthesisRequest,
    cancelled: Arc<AtomicBool>,
) -> Result<TtsSynthesisResult> {
    let (text, speed) = validate_synthesis_request(&request)?;
    let profile = find_provider(app_config_dir, &request.provider_id)?;
    if !profile.text_upload_consent {
        return Err(FinalSubError::Validation(
            "该在线 TTS 实例尚未授权发送配音文本".into(),
        ));
    }
    let voice = validate_voice(
        request
            .voice
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(&profile.voice),
    )?;
    let (output, temporary) = output_paths(&request.output_path)?;
    let audio = match profile.protocol {
        TtsProviderProtocol::EdgeTts => {
            synthesize_edge(&profile, &text, &voice, speed, cancelled.clone()).await?
        }
        TtsProviderProtocol::OpenaiCompatible => {
            let endpoint = resolved_provider_endpoint(&profile)?;
            let api_key =
                secrets::get_provider_secret(&provider_secret_id(&profile.id), &endpoint, "apiKey")
                    .map_err(FinalSubError::Validation)?
                    .filter(|value| !value.trim().is_empty())
                    .ok_or_else(|| {
                        FinalSubError::Validation("该在线 TTS 实例尚未保存 API Key".into())
                    })?;
            let client = http_client(profile.timeout_seconds)?;
            synthesize_openai(
                &client,
                &profile,
                &api_key,
                &text,
                &voice,
                speed,
                cancelled.clone(),
            )
            .await?
        }
        TtsProviderProtocol::AzureSpeech => {
            let endpoint = resolved_provider_endpoint(&profile)?;
            let api_key =
                secrets::get_provider_secret(&provider_secret_id(&profile.id), &endpoint, "apiKey")
                    .map_err(FinalSubError::Validation)?
                    .filter(|value| !value.trim().is_empty())
                    .ok_or_else(|| {
                        FinalSubError::Validation("该在线 TTS 实例尚未保存 API Key".into())
                    })?;
            let client = http_client(profile.timeout_seconds)?;
            synthesize_azure(
                &client,
                &profile,
                &api_key,
                &text,
                &voice,
                speed,
                cancelled.clone(),
            )
            .await?
        }
        TtsProviderProtocol::Elevenlabs => {
            let endpoint = resolved_provider_endpoint(&profile)?;
            let api_key =
                secrets::get_provider_secret(&provider_secret_id(&profile.id), &endpoint, "apiKey")
                    .map_err(FinalSubError::Validation)?
                    .filter(|value| !value.trim().is_empty())
                    .ok_or_else(|| {
                        FinalSubError::Validation("该在线 TTS 实例尚未保存 API Key".into())
                    })?;
            let client = http_client(profile.timeout_seconds)?;
            synthesize_elevenlabs(
                &client,
                &profile,
                &api_key,
                &text,
                &voice,
                speed,
                cancelled.clone(),
            )
            .await?
        }
        TtsProviderProtocol::Volcengine => {
            let endpoint = resolved_provider_endpoint(&profile)?;
            let api_key =
                secrets::get_provider_secret(&provider_secret_id(&profile.id), &endpoint, "apiKey")
                    .map_err(FinalSubError::Validation)?
                    .filter(|value| !value.trim().is_empty())
                    .ok_or_else(|| {
                        FinalSubError::Validation("该豆包 TTS 实例尚未保存 API Key".into())
                    })?;
            let client = http_client(profile.timeout_seconds)?;
            synthesize_volcengine(
                &client,
                &profile,
                &api_key,
                &text,
                &voice,
                speed,
                cancelled.clone(),
            )
            .await?
        }
    };
    if cancelled.load(Ordering::Relaxed) {
        return Err(FinalSubError::Validation("配音已取消".into()));
    }
    let duration_ms = if matches!(
        profile.protocol,
        TtsProviderProtocol::Elevenlabs | TtsProviderProtocol::Volcengine
    ) {
        write_pcm_wav(&temporary, &audio, volcengine::VOLC_TTS_SAMPLE_RATE)?
    } else {
        transcode_to_wav(ffmpeg_path, &audio, &output, &temporary, cancelled.clone()).await?
    };
    // 临时文件与目标位于同一目录；macOS 的 rename 会原子替换旧产物。
    std::fs::rename(&temporary, &output)?;
    Ok(TtsSynthesisResult {
        output_path: output.to_string_lossy().to_string(),
        sample_rate: 24_000,
        duration_ms,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn request(protocol: TtsProviderProtocol) -> SaveTtsProviderRequest {
        SaveTtsProviderRequest {
            id: None,
            name: "Demo".into(),
            protocol,
            endpoint: match protocol {
                TtsProviderProtocol::OpenaiCompatible => "https://api.openai.com/v1".into(),
                TtsProviderProtocol::AzureSpeech => String::new(),
                TtsProviderProtocol::Elevenlabs => "https://api.elevenlabs.io/v1".into(),
                TtsProviderProtocol::EdgeTts | TtsProviderProtocol::Volcengine => String::new(),
            },
            model: if matches!(
                protocol,
                TtsProviderProtocol::EdgeTts | TtsProviderProtocol::Volcengine
            ) {
                String::new()
            } else {
                "model".into()
            },
            voice: "zh-CN-XiaoxiaoNeural".into(),
            region: if protocol == TtsProviderProtocol::AzureSpeech {
                "japaneast".into()
            } else if protocol == TtsProviderProtocol::EdgeTts {
                "zh-CN".into()
            } else {
                String::new()
            },
            resource_id: if protocol == TtsProviderProtocol::Volcengine {
                volcengine::DEFAULT_RESOURCE_ID.into()
            } else {
                String::new()
            },
            text_upload_consent: true,
            timeout_seconds: 60,
            request_concurrency: 1,
        }
    }

    #[test]
    fn provider_store_roundtrips_and_updates_atomically() {
        let config = TempDir::new().unwrap();
        let first = save_provider(
            config.path(),
            request(TtsProviderProtocol::OpenaiCompatible),
        )
        .unwrap();
        let mut update = request(TtsProviderProtocol::Elevenlabs);
        update.id = Some(first.id.clone());
        update.name = "Updated".into();
        let updated = save_provider(config.path(), update).unwrap();
        let profiles = list_providers(config.path()).unwrap();
        assert_eq!(profiles.len(), 1);
        assert_eq!(profiles[0].id, first.id);
        assert_eq!(updated.name, "Updated");
        assert_eq!(updated.protocol, TtsProviderProtocol::Elevenlabs);
    }

    #[test]
    fn provider_rejects_credentials_embedded_in_url() {
        let config = TempDir::new().unwrap();
        let mut invalid = request(TtsProviderProtocol::OpenaiCompatible);
        let password_separator = char::from(58);
        let host_separator = char::from(64);
        invalid.endpoint =
            format!("https://user{password_separator}placeholder{host_separator}example.com/v1");
        assert!(save_provider(config.path(), invalid).is_err());
    }

    #[test]
    fn synthesis_voice_override_is_bounded_and_has_no_controls() {
        assert_eq!(validate_voice(" alloy ").unwrap(), "alloy");
        assert!(validate_voice("").is_err());
        assert!(validate_voice("bad\nvoice").is_err());
        assert!(validate_voice(&"x".repeat(201)).is_err());
    }

    #[test]
    fn provider_error_detail_drops_controls_and_is_bounded() {
        let detail = sanitize_provider_detail(format!("bad\0{}", "x".repeat(3_000)).as_bytes());
        assert!(!detail.contains('\0'));
        assert_eq!(detail.chars().count(), 2_000);
    }

    #[test]
    fn azure_ssml_escapes_text_and_applies_speed() {
        let ssml = azure_ssml("A & B < C", "zh-CN-XiaoxiaoNeural", 1.3);
        assert!(ssml.contains("rate=\"+30%\""));
        assert!(ssml.contains("A &amp; B &lt; C"));
        assert!(!ssml.contains("A & B"));
    }

    #[test]
    fn raw_pcm_wav_has_valid_header_and_duration() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("demo.wav");
        let pcm = vec![0_u8; 24_000 * 2];
        assert_eq!(write_pcm_wav(&path, &pcm, 24_000).unwrap(), 1000);
        let wav = std::fs::read(path).unwrap();
        assert_eq!(&wav[..4], b"RIFF");
        assert_eq!(wav_duration_ms(&wav), Some(1000));
    }

    #[test]
    fn edge_provider_is_zero_config_and_uses_fixed_endpoint() {
        let profile = save_provider(
            TempDir::new().unwrap().path(),
            request(TtsProviderProtocol::EdgeTts),
        )
        .unwrap();
        assert_eq!(
            resolved_provider_endpoint(&profile).unwrap(),
            EDGE_PROVIDER_ENDPOINT
        );
        assert_eq!(profile.endpoint, "");
        assert_eq!(profile.model, "");
    }

    #[test]
    fn volcengine_provider_is_fixed_endpoint_and_singleton() {
        let config = TempDir::new().unwrap();
        let profile =
            save_provider(config.path(), request(TtsProviderProtocol::Volcengine)).unwrap();
        assert_eq!(profile.endpoint, "");
        assert_eq!(profile.model, "");
        assert_eq!(
            resolved_provider_endpoint(&profile).unwrap(),
            volcengine::VOLC_TTS_URL
        );
        let duplicate = save_provider(config.path(), request(TtsProviderProtocol::Volcengine));
        assert!(duplicate.is_err());
    }

    #[test]
    fn edge_rate_and_locale_are_bounded() {
        assert_eq!(edge_rate(1.0), "+0%");
        assert_eq!(edge_rate(4.0), "+200%");
        assert_eq!(edge_rate(0.25), "-50%");
        assert_eq!(edge_locale("", "zh-CN-XiaoxiaoNeural"), "zh-CN");
        assert_eq!(edge_locale("ja-JP", "en-US-AriaNeural"), "ja-JP");
        assert_eq!(edge_locale("", "unknown"), "en-US");
        assert!(validate_edge_locale("zh").is_err());
        assert!(validate_edge_locale("zh_CN").is_err());
    }

    #[test]
    fn edge_audio_collection_is_bounded_and_ignores_metadata() {
        let events = vec![
            TtsEvent::WordBoundary {
                offset: 0,
                duration: 1,
                text: "hello".into(),
            },
            TtsEvent::Audio(vec![1, 2]),
            TtsEvent::Audio(vec![3, 4]),
            TtsEvent::TurnEnd,
        ];
        assert_eq!(collect_edge_audio(events).unwrap(), vec![1, 2, 3, 4]);
        assert!(collect_edge_audio(vec![TtsEvent::TurnEnd]).is_err());
    }

    #[tokio::test]
    #[ignore = "requires FINALSUB_EDGE_FFMPEG and live Edge Read Aloud access"]
    async fn edge_provider_real_synthesis_writes_pcm_wav() {
        let ffmpeg = PathBuf::from(
            std::env::var("FINALSUB_EDGE_FFMPEG")
                .expect("set FINALSUB_EDGE_FFMPEG to the ffmpeg executable"),
        );
        let config = TempDir::new().unwrap();
        let profile = save_provider(config.path(), request(TtsProviderProtocol::EdgeTts)).unwrap();
        let output = config.path().join("edge-preview.wav");
        let result = synthesize_cloud(
            config.path(),
            &ffmpeg,
            CloudTtsSynthesisRequest {
                provider_id: profile.id,
                text: "Hello from FinalSub.".into(),
                voice: None,
                speed: Some(1.0),
                output_path: output.to_string_lossy().to_string(),
            },
            Arc::new(AtomicBool::new(false)),
        )
        .await
        .unwrap();
        assert_eq!(result.sample_rate, 24_000);
        assert!(result.duration_ms > 0);
        assert!(output.exists());
        assert!(wav_duration_ms(&std::fs::read(output).unwrap()).is_some());
    }
}
