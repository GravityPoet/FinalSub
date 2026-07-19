use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use argon2::Argon2;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use chacha20poly1305::{
    aead::{Aead, KeyInit, Payload},
    XChaCha20Poly1305, XNonce,
};

use crate::error::Result;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CloudAsrProfile {
    pub id: String,
    pub name: String,
    pub protocol: String,
    pub endpoint: String,
    pub model: String,
    pub upload_consent: bool,
    pub timeout_seconds: u32,
    pub retry_times: u32,
    pub request_concurrency: u32,
    pub request_interval_ms: u64,
}

impl Default for CloudAsrProfile {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            protocol: "openai-compatible".into(),
            endpoint: "https://api.openai.com/v1".into(),
            model: "gpt-4o-transcribe".into(),
            upload_consent: false,
            timeout_seconds: 120,
            retry_times: 1,
            request_concurrency: 1,
            request_interval_ms: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub language: String,
    #[serde(alias = "asrEngine")]
    pub asr_engine: String,
    #[serde(alias = "cloudAsrProtocol")]
    pub cloud_asr_protocol: String,
    #[serde(alias = "cloudAsrEndpoint")]
    pub cloud_asr_endpoint: String,
    #[serde(alias = "cloudAsrModel")]
    pub cloud_asr_model: String,
    #[serde(alias = "cloudAsrUploadConsent")]
    pub cloud_asr_upload_consent: bool,
    #[serde(alias = "cloudAsrTimeoutSeconds")]
    pub cloud_asr_timeout_seconds: u32,
    #[serde(alias = "cloudAsrRetryTimes")]
    pub cloud_asr_retry_times: u32,
    #[serde(alias = "cloudAsrRequestConcurrency")]
    pub cloud_asr_request_concurrency: u32,
    #[serde(alias = "cloudAsrRequestIntervalMs")]
    pub cloud_asr_request_interval_ms: u64,
    #[serde(alias = "cloudAsrActiveProfileId")]
    pub cloud_asr_active_profile_id: String,
    #[serde(alias = "cloudAsrProfiles")]
    pub cloud_asr_profiles: Vec<CloudAsrProfile>,
    #[serde(alias = "modelsPath")]
    pub models_path: String,
    #[serde(alias = "parakeetModelsPath")]
    pub parakeet_models_path: String,
    #[serde(alias = "maxConcurrentTasks")]
    pub max_concurrent_tasks: u32,
    #[serde(alias = "subtitleOutputFormat")]
    pub subtitle_output_format: String,
    #[serde(alias = "sourceLanguage")]
    pub source_language: String,
    #[serde(alias = "targetLanguage")]
    pub target_language: String,
    #[serde(alias = "translateProvider")]
    pub translate_provider: String,
    #[serde(alias = "translateEndpoints")]
    pub translate_endpoints: std::collections::HashMap<String, String>,
    #[serde(alias = "translateModels")]
    pub translate_models: std::collections::HashMap<String, String>,
    #[serde(alias = "translateRetryTimes")]
    pub translate_retry_times: u32,
    #[serde(alias = "translateSystemPrompts")]
    pub translate_system_prompts: std::collections::HashMap<String, String>,
    #[serde(alias = "translateUserPrompts")]
    pub translate_user_prompts: std::collections::HashMap<String, String>,
    #[serde(alias = "translateCustomHeaders")]
    pub translate_custom_headers:
        std::collections::HashMap<String, std::collections::HashMap<String, String>>,
    #[serde(alias = "translateCustomBody")]
    pub translate_custom_body:
        std::collections::HashMap<String, serde_json::Map<String, serde_json::Value>>,
    #[serde(alias = "translateBatchSize")]
    pub translate_batch_size: u32,
    #[serde(alias = "translateConcurrency")]
    pub translate_concurrency: u32,
    #[serde(alias = "translateRequestIntervalMs")]
    pub translate_request_interval_ms: u64,
    #[serde(alias = "proxyEnabled")]
    pub proxy_enabled: bool,
    #[serde(alias = "proxyUrl")]
    pub proxy_url: String,
    #[serde(alias = "useVad", alias = "useVAD")]
    pub use_vad: bool,
    #[serde(alias = "vadThreshold")]
    pub vad_threshold: f64,
    #[serde(alias = "vadMinSpeechDurationMs", alias = "vadMinSpeechDuration")]
    pub vad_min_speech_duration_ms: u32,
    #[serde(alias = "vadMinSilenceDurationMs", alias = "vadMinSilenceDuration")]
    pub vad_min_silence_duration_ms: u32,
    #[serde(alias = "vadMaxSpeechDurationS", alias = "vadMaxSpeechDuration")]
    pub vad_max_speech_duration_s: u32,
    #[serde(alias = "vadSpeechPadMs", alias = "vadSpeechPad")]
    pub vad_speech_pad_ms: u32,
    #[serde(alias = "vadSamplesOverlap")]
    pub vad_samples_overlap: f64,
    #[serde(alias = "checkUpdateOnStartup")]
    pub check_update_on_startup: bool,
    #[serde(alias = "useCustomTempDir")]
    pub use_custom_temp_dir: bool,
    #[serde(alias = "customTempDir")]
    pub custom_temp_dir: String,
    #[serde(alias = "whisperCommand")]
    pub whisper_command: String,
    #[serde(alias = "maxContext")]
    pub max_context: i32,
    /// 是否上报崩溃/错误遥测到 Sentry（分发版隐私 opt-in，默认关闭）。
    #[serde(alias = "enableTelemetry")]
    pub enable_telemetry: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            language: "zh".into(),
            asr_engine: "parakeet-mlx".into(),
            cloud_asr_protocol: "openai-compatible".into(),
            cloud_asr_endpoint: "https://api.openai.com/v1".into(),
            cloud_asr_model: "gpt-4o-transcribe".into(),
            cloud_asr_upload_consent: false,
            cloud_asr_timeout_seconds: 120,
            cloud_asr_retry_times: 1,
            cloud_asr_request_concurrency: 1,
            cloud_asr_request_interval_ms: 0,
            cloud_asr_active_profile_id: String::new(),
            cloud_asr_profiles: Vec::new(),
            models_path: default_models_path(),
            parakeet_models_path: default_parakeet_models_path(),
            max_concurrent_tasks: 1,
            subtitle_output_format: "srt".into(),
            source_language: "auto".into(),
            target_language: "zh".into(),
            translate_provider: String::new(),
            translate_endpoints: std::collections::HashMap::new(),
            translate_models: std::collections::HashMap::new(),
            translate_retry_times: 0,
            translate_system_prompts: std::collections::HashMap::new(),
            translate_user_prompts: std::collections::HashMap::new(),
            translate_custom_headers: std::collections::HashMap::new(),
            translate_custom_body: std::collections::HashMap::new(),
            translate_batch_size: 24,
            translate_concurrency: 1,
            translate_request_interval_ms: 0,
            proxy_enabled: false,
            proxy_url: String::new(),
            use_vad: true,
            vad_threshold: 0.5,
            vad_min_speech_duration_ms: 250,
            vad_min_silence_duration_ms: 100,
            vad_max_speech_duration_s: 0,
            vad_speech_pad_ms: 30,
            vad_samples_overlap: 0.1,
            check_update_on_startup: false,
            use_custom_temp_dir: false,
            custom_temp_dir: String::new(),
            whisper_command: String::new(),
            max_context: -1,
            enable_telemetry: false,
        }
    }
}

fn default_models_path() -> String {
    home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Tools/Local-LLM/whisper-models")
        .to_string_lossy()
        .to_string()
}

fn default_parakeet_models_path() -> String {
    home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Tools/Local-LLM/parakeet-models")
        .to_string_lossy()
        .to_string()
}

fn home_dir() -> Option<PathBuf> {
    std::env::var("HOME")
        .ok()
        .map(PathBuf::from)
        .filter(|p| p.is_dir())
}

pub fn settings_path(app_config_dir: &Path) -> PathBuf {
    app_config_dir.join("settings.json")
}

fn detect_os_language() -> String {
    let locale = sys_locale::get_locale()
        .unwrap_or_else(|| "en".to_string())
        .to_lowercase();
    if locale.starts_with("zh") {
        "zh".to_string()
    } else if locale.starts_with("ja") {
        "ja".to_string()
    } else {
        "en".to_string()
    }
}

pub fn load_settings(app_config_dir: &Path) -> Result<Settings> {
    let path = settings_path(app_config_dir);
    if !path.exists() {
        let s = Settings {
            language: detect_os_language(),
            ..Settings::default()
        };
        save_settings(app_config_dir, &s)?;
        return Ok(s);
    }
    let content = std::fs::read_to_string(&path)?;
    let settings: Settings = serde_json::from_str(&content)?;
    validate_settings(&settings)?;
    Ok(settings)
}

pub fn save_settings(app_config_dir: &Path, settings: &Settings) -> Result<()> {
    validate_settings(settings)?;
    let path = settings_path(app_config_dir);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let content = serde_json::to_string_pretty(settings)?;
    let tmp_path = path.with_extension("json.tmp");
    std::fs::write(&tmp_path, content)?;
    std::fs::rename(&tmp_path, &path)?;
    Ok(())
}

pub fn reset_settings(app_config_dir: &Path) -> Result<Settings> {
    let settings = Settings::default();
    save_settings(app_config_dir, &settings)?;
    Ok(settings)
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ConfigExport {
    pub version: u32,
    pub exported_at: String,
    pub settings: Settings,
}

#[derive(Debug, Serialize, Deserialize)]
struct EncryptedConfigEnvelope {
    version: u32,
    kdf: String,
    cipher: String,
    salt: String,
    nonce: String,
    ciphertext: String,
}

const ENCRYPTED_CONFIG_AAD: &[u8] = b"FinalSub encrypted config v1";

pub fn export_config(app_config_dir: &Path) -> Result<String> {
    let settings = load_settings(app_config_dir)?;
    let export = ConfigExport {
        version: 1,
        exported_at: chrono::Utc::now().to_rfc3339(),
        settings,
    };
    Ok(serde_json::to_string_pretty(&export)?)
}

pub fn import_config(app_config_dir: &Path, json: &str) -> Result<Settings> {
    let export: ConfigExport = serde_json::from_str(json)?;
    if export.version != 1 {
        return Err(crate::error::FinalSubError::Validation(format!(
            "不支持的配置版本：{}",
            export.version
        )));
    }
    validate_settings(&export.settings)?;
    save_settings(app_config_dir, &export.settings)?;
    Ok(export.settings)
}

pub fn export_encrypted_config(app_config_dir: &Path, passphrase: &str) -> Result<String> {
    let plaintext = export_config(app_config_dir)?;
    encrypt_config(&plaintext, passphrase)
}

pub fn import_encrypted_config(
    app_config_dir: &Path,
    encrypted_json: &str,
    passphrase: &str,
) -> Result<Settings> {
    let plaintext = decrypt_config(encrypted_json, passphrase)?;
    import_config(app_config_dir, &plaintext)
}

fn validate_config_passphrase(passphrase: &str) -> Result<()> {
    if passphrase.chars().count() < 8 {
        return Err(crate::error::FinalSubError::Validation(
            "加密配置口令至少需要 8 个字符".into(),
        ));
    }
    Ok(())
}

fn derive_config_key(passphrase: &str, salt: &[u8]) -> Result<[u8; 32]> {
    let mut key = [0_u8; 32];
    Argon2::default()
        .hash_password_into(passphrase.as_bytes(), salt, &mut key)
        .map_err(|_| {
            crate::error::FinalSubError::Validation("无法从口令派生配置加密密钥".into())
        })?;
    Ok(key)
}

fn encrypt_config(plaintext: &str, passphrase: &str) -> Result<String> {
    validate_config_passphrase(passphrase)?;
    let salt = *uuid::Uuid::new_v4().as_bytes();
    let first_nonce = *uuid::Uuid::new_v4().as_bytes();
    let second_nonce = *uuid::Uuid::new_v4().as_bytes();
    let mut nonce = [0_u8; 24];
    nonce[..16].copy_from_slice(&first_nonce);
    nonce[16..].copy_from_slice(&second_nonce[..8]);

    let key = derive_config_key(passphrase, &salt)?;
    let cipher = XChaCha20Poly1305::new((&key).into());
    let ciphertext = cipher
        .encrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: plaintext.as_bytes(),
                aad: ENCRYPTED_CONFIG_AAD,
            },
        )
        .map_err(|_| crate::error::FinalSubError::Validation("加密配置失败".into()))?;
    let envelope = EncryptedConfigEnvelope {
        version: 1,
        kdf: "argon2id".into(),
        cipher: "xchacha20poly1305".into(),
        salt: BASE64.encode(salt),
        nonce: BASE64.encode(nonce),
        ciphertext: BASE64.encode(ciphertext),
    };
    Ok(serde_json::to_string_pretty(&envelope)?)
}

fn decrypt_config(encrypted_json: &str, passphrase: &str) -> Result<String> {
    validate_config_passphrase(passphrase)?;
    let envelope: EncryptedConfigEnvelope = serde_json::from_str(encrypted_json)?;
    if envelope.version != 1 || envelope.kdf != "argon2id" || envelope.cipher != "xchacha20poly1305"
    {
        return Err(crate::error::FinalSubError::Validation(
            "不支持的加密配置格式".into(),
        ));
    }
    let salt = BASE64
        .decode(envelope.salt)
        .map_err(|_| crate::error::FinalSubError::Validation("加密配置 salt 无效".into()))?;
    let nonce = BASE64
        .decode(envelope.nonce)
        .map_err(|_| crate::error::FinalSubError::Validation("加密配置 nonce 无效".into()))?;
    let ciphertext = BASE64
        .decode(envelope.ciphertext)
        .map_err(|_| crate::error::FinalSubError::Validation("加密配置密文无效".into()))?;
    if salt.len() != 16 || nonce.len() != 24 {
        return Err(crate::error::FinalSubError::Validation(
            "加密配置参数长度无效".into(),
        ));
    }

    let key = derive_config_key(passphrase, &salt)?;
    let cipher = XChaCha20Poly1305::new((&key).into());
    let plaintext = cipher
        .decrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: &ciphertext,
                aad: ENCRYPTED_CONFIG_AAD,
            },
        )
        .map_err(|_| {
            crate::error::FinalSubError::Validation("无法解密配置：口令错误或文件已被篡改".into())
        })?;
    String::from_utf8(plaintext)
        .map_err(|_| crate::error::FinalSubError::Validation("解密后的配置不是有效 UTF-8".into()))
}

pub fn validate_settings(settings: &Settings) -> Result<()> {
    if !matches!(settings.language.as_str(), "zh" | "en" | "ja") {
        return Err(crate::error::FinalSubError::Validation(format!(
            "不支持的界面语言：{}",
            settings.language
        )));
    }
    if settings.models_path.trim().is_empty() {
        return Err(crate::error::FinalSubError::Validation(
            "模型路径不能为空".into(),
        ));
    }
    if settings.parakeet_models_path.trim().is_empty() {
        return Err(crate::error::FinalSubError::Validation(
            "Parakeet 模型路径不能为空".into(),
        ));
    }
    crate::core::asr::cloud::validate_service_settings(
        &settings.cloud_asr_protocol,
        &settings.cloud_asr_endpoint,
        &settings.cloud_asr_model,
        settings.cloud_asr_timeout_seconds,
        settings.cloud_asr_retry_times,
        settings.cloud_asr_request_concurrency,
        settings.cloud_asr_request_interval_ms,
    )?;
    if settings.cloud_asr_profiles.len() > 32 {
        return Err(crate::error::FinalSubError::Validation(
            "云端 ASR 配置实例不能超过 32 个".into(),
        ));
    }
    let mut profile_ids = std::collections::HashSet::new();
    for profile in &settings.cloud_asr_profiles {
        if profile.id.is_empty()
            || profile.id.len() > 80
            || !profile
                .id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        {
            return Err(crate::error::FinalSubError::Validation(
                "云端 ASR 配置实例 ID 无效".into(),
            ));
        }
        if !profile_ids.insert(profile.id.as_str()) {
            return Err(crate::error::FinalSubError::Validation(
                "云端 ASR 配置实例 ID 不能重复".into(),
            ));
        }
        let name = profile.name.trim();
        if name.is_empty() || name.len() > 80 || name.chars().any(char::is_control) {
            return Err(crate::error::FinalSubError::Validation(
                "云端 ASR 配置实例名称无效".into(),
            ));
        }
        crate::core::asr::cloud::validate_service_settings(
            &profile.protocol,
            &profile.endpoint,
            &profile.model,
            profile.timeout_seconds,
            profile.retry_times,
            profile.request_concurrency,
            profile.request_interval_ms,
        )?;
    }
    if !settings.cloud_asr_profiles.is_empty()
        && !profile_ids.contains(settings.cloud_asr_active_profile_id.as_str())
    {
        return Err(crate::error::FinalSubError::Validation(
            "当前云端 ASR 配置实例不存在".into(),
        ));
    }
    if settings.max_concurrent_tasks == 0 || settings.max_concurrent_tasks > 8 {
        return Err(crate::error::FinalSubError::Validation(
            "最大并发任务数必须在 1-8 之间".into(),
        ));
    }
    if !matches!(
        settings.subtitle_output_format.as_str(),
        "srt" | "vtt" | "ass" | "lrc" | "txt"
    ) {
        return Err(crate::error::FinalSubError::Validation(format!(
            "不支持的字幕输出格式：{}",
            settings.subtitle_output_format
        )));
    }
    if settings.translate_retry_times > 10 {
        return Err(crate::error::FinalSubError::Validation(
            "翻译重试次数不能超过 10".into(),
        ));
    }
    if !(1..=50).contains(&settings.translate_batch_size) {
        return Err(crate::error::FinalSubError::Validation(
            "翻译批量行数必须在 1-50 之间".into(),
        ));
    }
    if !(1..=8).contains(&settings.translate_concurrency) {
        return Err(crate::error::FinalSubError::Validation(
            "翻译并发数必须在 1-8 之间".into(),
        ));
    }
    if settings.translate_request_interval_ms > 60_000 {
        return Err(crate::error::FinalSubError::Validation(
            "翻译请求间隔不能超过 60000 毫秒".into(),
        ));
    }
    if settings.proxy_enabled {
        let proxy = settings.proxy_url.trim();
        if !(proxy.starts_with("http://") || proxy.starts_with("https://")) {
            return Err(crate::error::FinalSubError::Validation(
                "代理地址必须以 http:// 或 https:// 开头".into(),
            ));
        }
    }
    if settings
        .translate_system_prompts
        .values()
        .chain(settings.translate_user_prompts.values())
        .any(|prompt| prompt.len() > 20_000)
    {
        return Err(crate::error::FinalSubError::Validation(
            "单个翻译提示词不能超过 20000 字节".into(),
        ));
    }
    for headers in settings.translate_custom_headers.values() {
        if headers.len() > 64 {
            return Err(crate::error::FinalSubError::Validation(
                "单个翻译服务最多配置 64 个自定义请求头".into(),
            ));
        }
        for (name, value) in headers {
            if name.len() > 128 || value.len() > 8192 {
                return Err(crate::error::FinalSubError::Validation(
                    "自定义请求头名称或值过长".into(),
                ));
            }
            if matches!(
                name.trim().to_ascii_lowercase().as_str(),
                "host" | "content-length" | "transfer-encoding" | "connection"
            ) {
                return Err(crate::error::FinalSubError::Validation(format!(
                    "不允许覆盖受保护的请求头：{name}"
                )));
            }
            reqwest::header::HeaderName::from_bytes(name.trim().as_bytes()).map_err(|error| {
                crate::error::FinalSubError::Validation(format!("自定义请求头名称无效：{error}"))
            })?;
            reqwest::header::HeaderValue::from_str(value).map_err(|error| {
                crate::error::FinalSubError::Validation(format!("自定义请求头值无效：{error}"))
            })?;
        }
    }
    for body in settings.translate_custom_body.values() {
        if body.len() > 64 {
            return Err(crate::error::FinalSubError::Validation(
                "单个翻译服务最多配置 64 个自定义请求体参数".into(),
            ));
        }
        if serde_json::to_vec(body)?.len() > 64 * 1024 {
            return Err(crate::error::FinalSubError::Validation(
                "单个翻译服务的自定义请求体不能超过 64 KiB".into(),
            ));
        }
    }
    if !settings.vad_threshold.is_finite() || !(0.0..=1.0).contains(&settings.vad_threshold) {
        return Err(crate::error::FinalSubError::Validation(
            "VAD 阈值必须在 0-1 之间".into(),
        ));
    }
    if !settings.vad_samples_overlap.is_finite()
        || !(0.0..=1.0).contains(&settings.vad_samples_overlap)
    {
        return Err(crate::error::FinalSubError::Validation(
            "VAD 样本重叠必须在 0-1 之间".into(),
        ));
    }
    if settings.vad_min_speech_duration_ms > 60_000
        || settings.vad_min_silence_duration_ms > 60_000
        || settings.vad_speech_pad_ms > 5_000
        || settings.vad_max_speech_duration_s > 3_600
    {
        return Err(crate::error::FinalSubError::Validation(
            "VAD 时长参数超出允许范围".into(),
        ));
    }
    if settings.use_custom_temp_dir && settings.custom_temp_dir.trim().is_empty() {
        return Err(crate::error::FinalSubError::Validation(
            "启用自定义临时目录时路径不能为空".into(),
        ));
    }
    if settings.max_context < -1 || settings.max_context > 65_536 {
        return Err(crate::error::FinalSubError::Validation(
            "最大上下文必须为 -1 或 0-65536".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn test_config_dir() -> (TempDir, PathBuf) {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().to_path_buf();
        (tmp, path)
    }

    #[test]
    fn default_settings_roundtrip() {
        let (_tmp, dir) = test_config_dir();
        let settings = Settings::default();
        save_settings(&dir, &settings).unwrap();
        let loaded = load_settings(&dir).unwrap();
        assert_eq!(loaded.language, "zh");
        assert_eq!(loaded.asr_engine, "parakeet-mlx");
        assert_eq!(loaded.max_concurrent_tasks, 1);
        assert_eq!(loaded.subtitle_output_format, "srt");
        assert!(loaded.use_vad);
        assert!(loaded.cloud_asr_profiles.is_empty());
    }

    #[test]
    fn settings_serialize_as_snake_case() {
        let content = serde_json::to_string(&Settings::default()).unwrap();
        assert!(content.contains("asr_engine"));
        assert!(content.contains("models_path"));
        assert!(content.contains("parakeet_models_path"));
        assert!(!content.contains("asrEngine"));
    }

    #[test]
    fn cloud_asr_settings_are_validated_before_persistence() {
        let (_tmp, dir) = test_config_dir();
        let mut settings = Settings {
            cloud_asr_endpoint: "file:///tmp/transcriptions".into(),
            ..Settings::default()
        };
        assert!(save_settings(&dir, &settings).is_err());

        settings.cloud_asr_endpoint = "https://api.example.com/v1".into();
        settings.cloud_asr_timeout_seconds = 9;
        assert!(save_settings(&dir, &settings).is_err());

        settings.cloud_asr_timeout_seconds = 120;
        settings.cloud_asr_retry_times = 6;
        assert!(save_settings(&dir, &settings).is_err());

        settings.cloud_asr_retry_times = 1;
        settings.cloud_asr_request_concurrency = 0;
        assert!(save_settings(&dir, &settings).is_err());

        settings.cloud_asr_request_concurrency = 9;
        assert!(save_settings(&dir, &settings).is_err());

        settings.cloud_asr_request_concurrency = 1;
        settings.cloud_asr_model = "\u{0}".into();
        assert!(save_settings(&dir, &settings).is_err());

        settings.cloud_asr_model = "nova-3".into();
        settings.cloud_asr_protocol = "unknown-provider".into();
        assert!(save_settings(&dir, &settings).is_err());
    }

    #[test]
    fn cloud_asr_profiles_roundtrip_and_require_unique_active_id() {
        let (_tmp, dir) = test_config_dir();
        let profile = CloudAsrProfile {
            id: "openai-work".into(),
            name: "Work OpenAI".into(),
            protocol: "openai-compatible".into(),
            endpoint: "https://api.openai.com/v1".into(),
            model: "gpt-4o-transcribe".into(),
            upload_consent: true,
            timeout_seconds: 120,
            retry_times: 1,
            request_concurrency: 2,
            request_interval_ms: 0,
        };
        let mut settings = Settings {
            cloud_asr_active_profile_id: profile.id.clone(),
            cloud_asr_profiles: vec![profile.clone()],
            ..Settings::default()
        };
        save_settings(&dir, &settings).unwrap();
        let loaded = load_settings(&dir).unwrap();
        assert_eq!(loaded.cloud_asr_active_profile_id, "openai-work");
        assert_eq!(loaded.cloud_asr_profiles.len(), 1);
        assert_eq!(loaded.cloud_asr_profiles[0].name, "Work OpenAI");
        assert_eq!(loaded.cloud_asr_profiles[0].request_concurrency, 2);

        settings.cloud_asr_active_profile_id = "missing".into();
        assert!(save_settings(&dir, &settings).is_err());
        settings.cloud_asr_active_profile_id = "openai-work".into();
        settings.cloud_asr_profiles.push(profile);
        assert!(save_settings(&dir, &settings).is_err());
    }

    #[test]
    fn import_legacy_camel_case_settings() {
        let (_tmp, dir) = test_config_dir();
        let legacy = r#"{
          "version": 1,
          "exported_at": "2026-01-01T00:00:00Z",
          "settings": {
            "language": "zh",
            "asrEngine": "whisper-cpp",
            "modelsPath": "/tmp/models",
            "maxConcurrentTasks": 2,
            "subtitleOutputFormat": "srt",
            "sourceLanguage": "auto",
            "targetLanguage": "zh",
            "translateProvider": "ollama",
            "translateRetryTimes": 1,
            "useVad": true,
            "vadThreshold": 0.5,
            "vadMinSpeechDurationMs": 250,
            "vadMinSilenceDurationMs": 100,
            "vadMaxSpeechDurationS": 0,
            "vadSpeechPadMs": 30,
            "vadSamplesOverlap": 0.1,
            "checkUpdateOnStartup": false,
            "useCustomTempDir": false,
            "customTempDir": "",
            "whisperCommand": "",
            "maxContext": -1
          }
        }"#;
        let imported = import_config(&dir, legacy).unwrap();
        assert_eq!(imported.asr_engine, "whisper-cpp");
        assert_eq!(imported.models_path, "/tmp/models");
        assert!(imported.parakeet_models_path.ends_with("parakeet-models"));
        assert_eq!(imported.max_concurrent_tasks, 2);
        assert_eq!(imported.cloud_asr_request_concurrency, 1);
    }

    #[test]
    fn load_missing_returns_default() {
        let (_tmp, dir) = test_config_dir();
        let settings = load_settings(&dir).unwrap();
        assert!(matches!(settings.language.as_str(), "zh" | "en" | "ja"));
    }

    #[test]
    fn save_and_modify() {
        let (_tmp, dir) = test_config_dir();
        let settings = Settings {
            language: "en".into(),
            max_concurrent_tasks: 3,
            ..Default::default()
        };
        save_settings(&dir, &settings).unwrap();

        let loaded = load_settings(&dir).unwrap();
        assert_eq!(loaded.language, "en");
        assert_eq!(loaded.max_concurrent_tasks, 3);
    }

    #[test]
    fn export_import_roundtrip() {
        let (_tmp, dir) = test_config_dir();
        let settings = Settings {
            language: "en".into(),
            target_language: "ja".into(),
            ..Default::default()
        };
        save_settings(&dir, &settings).unwrap();

        let exported = export_config(&dir).unwrap();
        assert!(exported.contains("\"version\": 1"));

        let (_tmp2, dir2) = test_config_dir();
        let imported = import_config(&dir2, &exported).unwrap();
        assert_eq!(imported.language, "en");
        assert_eq!(imported.target_language, "ja");
    }

    #[test]
    fn import_invalid_version() {
        let (_tmp, dir) = test_config_dir();
        let bad = r#"{"version": 99, "exported_at": "2026-01-01", "settings": {}}"#;
        let result = import_config(&dir, bad);
        assert!(result.is_err());
    }

    #[test]
    fn import_invalid_json() {
        let (_tmp, dir) = test_config_dir();
        let result = import_config(&dir, "not json");
        assert!(result.is_err());
    }

    #[test]
    fn import_invalid_settings_range() {
        let (_tmp, dir) = test_config_dir();
        let bad = r#"{
          "version": 1,
          "exported_at": "2026-01-01",
          "settings": {
            "language": "zh",
            "models_path": "/tmp/models",
            "max_concurrent_tasks": 0,
            "subtitle_output_format": "srt",
            "vad_threshold": 0.5,
            "vad_samples_overlap": 0.1,
            "max_context": -1
          }
        }"#;
        let result = import_config(&dir, bad);
        assert!(result.is_err());
    }

    #[test]
    fn reset_settings_restores_defaults() {
        let (_tmp, dir) = test_config_dir();
        let settings = Settings {
            language: "en".into(),
            ..Default::default()
        };
        super::save_settings(&dir, &settings).unwrap();

        let reset = super::reset_settings(&dir).unwrap();
        assert_eq!(reset.language, "zh");

        let loaded = super::load_settings(&dir).unwrap();
        assert_eq!(loaded.language, "zh");
    }

    #[test]
    fn encrypted_config_roundtrip_hides_plaintext_and_restores_settings() {
        let (_tmp, dir) = test_config_dir();
        let settings = Settings {
            language: "en".into(),
            target_language: "ja".into(),
            ..Default::default()
        };
        save_settings(&dir, &settings).unwrap();

        let encrypted = export_encrypted_config(&dir, "correct horse battery staple").unwrap();
        assert!(!encrypted.contains("target_language"));
        assert!(!encrypted.contains("\"ja\""));

        let (_tmp2, dir2) = test_config_dir();
        let restored =
            import_encrypted_config(&dir2, &encrypted, "correct horse battery staple").unwrap();
        assert_eq!(restored.language, "en");
        assert_eq!(restored.target_language, "ja");
    }

    #[test]
    fn encrypted_config_rejects_wrong_passphrase_and_tampering() {
        let (_tmp, dir) = test_config_dir();
        let encrypted = export_encrypted_config(&dir, "correct horse battery staple").unwrap();
        assert!(import_encrypted_config(&dir, &encrypted, "incorrect passphrase").is_err());

        let mut envelope: serde_json::Value = serde_json::from_str(&encrypted).unwrap();
        envelope["ciphertext"] = serde_json::Value::String("AAAA".into());
        assert!(import_encrypted_config(
            &dir,
            &serde_json::to_string(&envelope).unwrap(),
            "correct horse battery staple",
        )
        .is_err());
    }
}
