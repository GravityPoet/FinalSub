use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::error::Error as StdError;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use crate::error::{FinalSubError, Result};

const MAX_PROVIDER_ERROR_BYTES: usize = 16 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranslationProvider {
    pub id: String,
    pub name: String,
    pub provider_type: String,
    pub is_ai: bool,
    pub implemented: bool,
    pub requires_api_key: bool,
    pub requires_endpoint: bool,
    pub requires_model: bool,
    pub secret_fields: Vec<String>,
    pub default_endpoint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranslationConfig {
    pub enabled: bool,
    pub target_language: String,
    pub provider: String,
    pub api_key: Option<String>,
}

impl Default for TranslationConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            target_language: "zh".into(),
            provider: String::new(),
            api_key: None,
        }
    }
}

fn translation_http_client(req: &TranslateRequest) -> Result<reqwest::Client> {
    let mut builder = reqwest::Client::builder()
        .user_agent("FinalSub/1.0")
        .connect_timeout(Duration::from_secs(20))
        .timeout(Duration::from_secs(120));
    if let Some(proxy_url) = configured_str(req.proxy_url.as_deref()) {
        if !(proxy_url.starts_with("http://") || proxy_url.starts_with("https://")) {
            return Err(FinalSubError::Validation(
                "翻译代理仅支持 http:// 或 https:// 地址".into(),
            ));
        }
        let proxy = reqwest::Proxy::all(proxy_url)
            .map_err(|error| FinalSubError::Validation(format!("翻译代理地址无效：{error}")))?;
        builder = builder.proxy(proxy);
    }
    builder.build().map_err(|e| {
        FinalSubError::Validation(format!(
            "初始化 HTTP 客户端失败：{}",
            describe_reqwest_error(&e)
        ))
    })
}

fn describe_reqwest_error(err: &reqwest::Error) -> String {
    let mut parts = vec![err.to_string()];
    let mut flags = Vec::new();

    if err.is_timeout() {
        flags.push("timeout");
    }
    if err.is_connect() {
        flags.push("connect");
    }
    if err.is_request() {
        flags.push("request");
    }
    if err.is_body() {
        flags.push("body");
    }
    if err.is_decode() {
        flags.push("decode");
    }
    if let Some(status) = err.status() {
        flags.push(if status.is_client_error() {
            "http_4xx"
        } else if status.is_server_error() {
            "http_5xx"
        } else {
            "http_status"
        });
    }

    if !flags.is_empty() {
        parts.push(format!("分类：{}", flags.join(",")));
    }

    let mut source = err.source();
    let mut source_parts = Vec::new();
    while let Some(item) = source {
        let text = item.to_string();
        if !text.is_empty() && !source_parts.iter().any(|existing| existing == &text) {
            source_parts.push(text);
        }
        source = item.source();
        if source_parts.len() >= 4 {
            break;
        }
    }

    if !source_parts.is_empty() {
        parts.push(format!("底层原因：{}", source_parts.join("；")));
    }

    parts.join("；")
}

async fn limited_response_text(mut response: reqwest::Response) -> String {
    let mut body = Vec::new();
    while body.len() < MAX_PROVIDER_ERROR_BYTES {
        match response.chunk().await {
            Ok(Some(chunk)) => {
                let remaining = MAX_PROVIDER_ERROR_BYTES - body.len();
                body.extend_from_slice(&chunk[..chunk.len().min(remaining)]);
            }
            Ok(None) | Err(_) => break,
        }
    }
    String::from_utf8_lossy(&body)
        .chars()
        .filter(|character| !character.is_control() || matches!(character, '\n' | '\r' | '\t'))
        .collect()
}

fn validation_message(err: FinalSubError) -> String {
    match err {
        FinalSubError::Validation(msg) => msg,
        other => other.to_string(),
    }
}

pub fn builtin_providers() -> Vec<TranslationProvider> {
    vec![
        TranslationProvider {
            id: "baidu".into(),
            name: "百度翻译".into(),
            provider_type: "api".into(),
            is_ai: false,
            implemented: true,
            requires_api_key: true,
            requires_endpoint: false,
            requires_model: false,
            secret_fields: vec!["appId".into(), "secretKey".into()],
            default_endpoint: "".into(),
        },
        TranslationProvider {
            id: "google".into(),
            name: "谷歌翻译".into(),
            provider_type: "api".into(),
            is_ai: false,
            implemented: true,
            requires_api_key: true,
            requires_endpoint: false,
            requires_model: false,
            secret_fields: vec!["apiKey".into()],
            default_endpoint: "".into(),
        },
        TranslationProvider {
            id: "aliyun".into(),
            name: "阿里云翻译".into(),
            provider_type: "api".into(),
            is_ai: false,
            implemented: true,
            requires_api_key: true,
            requires_endpoint: false,
            requires_model: false,
            secret_fields: vec!["accessKeyId".into(), "accessKeySecret".into()],
            default_endpoint: "".into(),
        },
        TranslationProvider {
            id: "volc".into(),
            name: "火山翻译".into(),
            provider_type: "api".into(),
            is_ai: false,
            implemented: true,
            requires_api_key: true,
            requires_endpoint: false,
            requires_model: false,
            secret_fields: vec!["accessKeyId".into(), "accessKeySecret".into()],
            default_endpoint: "".into(),
        },
        TranslationProvider {
            id: "doubao".into(),
            name: "豆包翻译".into(),
            provider_type: "ai".into(),
            is_ai: true,
            implemented: true,
            requires_api_key: true,
            requires_endpoint: true,
            requires_model: true,
            secret_fields: vec!["apiKey".into()],
            default_endpoint: "https://ark.cn-beijing.volces.com/api/v3".into(),
        },
        TranslationProvider {
            id: "niutrans".into(),
            name: "小牛翻译".into(),
            provider_type: "api".into(),
            is_ai: false,
            implemented: true,
            requires_api_key: true,
            requires_endpoint: false,
            requires_model: false,
            secret_fields: vec!["apiKey".into()],
            default_endpoint: "".into(),
        },
        TranslationProvider {
            id: "tencent".into(),
            name: "腾讯翻译".into(),
            provider_type: "api".into(),
            is_ai: false,
            implemented: true,
            requires_api_key: true,
            requires_endpoint: false,
            requires_model: false,
            secret_fields: vec!["secretId".into(), "secretKey".into(), "region".into()],
            default_endpoint: "".into(),
        },
        TranslationProvider {
            id: "xunfei".into(),
            name: "讯飞翻译".into(),
            provider_type: "api".into(),
            is_ai: false,
            implemented: true,
            requires_api_key: true,
            requires_endpoint: false,
            requires_model: false,
            secret_fields: vec!["appId".into(), "apiKey".into(), "apiSecret".into()],
            default_endpoint: "".into(),
        },
        TranslationProvider {
            id: "deeplx".into(),
            name: "DeepLX".into(),
            provider_type: "api".into(),
            is_ai: false,
            implemented: true,
            requires_api_key: false,
            requires_endpoint: true,
            requires_model: false,
            secret_fields: vec![],
            default_endpoint: "http://localhost:1188/translate".into(),
        },
        TranslationProvider {
            id: "azure".into(),
            name: "微软翻译".into(),
            provider_type: "api".into(),
            is_ai: false,
            implemented: true,
            requires_api_key: true,
            requires_endpoint: true,
            requires_model: false,
            secret_fields: vec!["apiKey".into(), "region".into()],
            default_endpoint: "https://api.cognitive.microsofttranslator.com".into(),
        },
        TranslationProvider {
            id: "ollama".into(),
            name: "Ollama".into(),
            provider_type: "ai".into(),
            is_ai: true,
            implemented: true,
            requires_api_key: false,
            requires_endpoint: true,
            requires_model: true,
            secret_fields: vec![],
            default_endpoint: "http://localhost:11434/api/generate".into(),
        },
        TranslationProvider {
            id: "deepseek".into(),
            name: "深度求索".into(),
            provider_type: "ai".into(),
            is_ai: true,
            implemented: true,
            requires_api_key: true,
            requires_endpoint: true,
            requires_model: true,
            secret_fields: vec!["apiKey".into()],
            default_endpoint: "https://api.deepseek.com/v1".into(),
        },
        TranslationProvider {
            id: "azureopenai".into(),
            name: "Azure OpenAI".into(),
            provider_type: "ai".into(),
            is_ai: true,
            implemented: true,
            requires_api_key: true,
            requires_endpoint: true,
            requires_model: true,
            secret_fields: vec!["apiKey".into(), "apiVersion".into()],
            default_endpoint: "".into(),
        },
        TranslationProvider {
            id: "deerapi".into(),
            name: "DeerAPI".into(),
            provider_type: "ai".into(),
            is_ai: true,
            implemented: true,
            requires_api_key: true,
            requires_endpoint: true,
            requires_model: true,
            secret_fields: vec!["apiKey".into()],
            default_endpoint: "https://api.deerapi.com/v1".into(),
        },
        TranslationProvider {
            id: "gemini".into(),
            name: "Gemini".into(),
            provider_type: "ai".into(),
            is_ai: true,
            implemented: true,
            requires_api_key: true,
            requires_endpoint: true,
            requires_model: true,
            secret_fields: vec!["apiKey".into()],
            default_endpoint: "https://generativelanguage.googleapis.com".into(),
        },
        TranslationProvider {
            id: "siliconflow".into(),
            name: "硅基流动".into(),
            provider_type: "ai".into(),
            is_ai: true,
            implemented: true,
            requires_api_key: true,
            requires_endpoint: true,
            requires_model: true,
            secret_fields: vec!["apiKey".into()],
            default_endpoint: "https://api.siliconflow.cn/v1".into(),
        },
        TranslationProvider {
            id: "qwen".into(),
            name: "通义千问".into(),
            provider_type: "ai".into(),
            is_ai: true,
            implemented: true,
            requires_api_key: true,
            requires_endpoint: true,
            requires_model: true,
            secret_fields: vec!["apiKey".into()],
            default_endpoint: "https://dashscope.aliyuncs.com/compatible-mode/v1".into(),
        },
        TranslationProvider {
            id: "custom-openai".into(),
            name: "自定义 OpenAI 兼容".into(),
            provider_type: "ai".into(),
            is_ai: true,
            implemented: true,
            requires_api_key: true,
            requires_endpoint: true,
            requires_model: true,
            secret_fields: vec!["apiKey".into()],
            default_endpoint: "".into(),
        },
    ]
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TranslateRequest {
    pub text: String,
    pub source_language: String,
    pub target_language: String,
    pub provider: String,
    pub api_key: Option<String>,
    pub api_url: Option<String>,
    pub model_name: Option<String>,
    pub secret_fields: Option<std::collections::HashMap<String, String>>,
    #[serde(default)]
    pub system_prompt: Option<String>,
    #[serde(default)]
    pub user_prompt: Option<String>,
    #[serde(default)]
    pub proxy_url: Option<String>,
    #[serde(default)]
    pub custom_headers: Option<std::collections::HashMap<String, String>>,
    #[serde(default)]
    pub custom_body: Option<serde_json::Map<String, serde_json::Value>>,
    #[serde(default)]
    pub structured_output: Option<String>,
    #[serde(default)]
    pub response_json_schema: Option<serde_json::Value>,
    #[serde(default)]
    pub glossary_prompt: Option<String>,
    /// Positive semantic switch: true follows the model default; false/None
    /// lets FinalSub proactively disable reasoning where the provider allows it.
    #[serde(default)]
    pub enable_thinking: Option<bool>,
    /// Internal retry flag. It is never accepted from or returned to the UI.
    #[serde(skip)]
    pub thinking_control_bypassed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranslateResponse {
    pub translated_text: String,
    pub provider: String,
    pub success: bool,
    pub error: Option<String>,
    #[serde(default)]
    pub thinking_enabled: Option<bool>,
}

const THINKING_PARAM_KEYS: [&str; 4] = ["enable_thinking", "thinking", "think", "reasoning_effort"];

static THINKING_PARAM_REJECTION_CACHE: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

fn thinking_rejection_cache() -> &'static Mutex<HashSet<String>> {
    THINKING_PARAM_REJECTION_CACHE.get_or_init(|| Mutex::new(HashSet::new()))
}

fn thinking_cache_key(req: &TranslateRequest) -> String {
    format!(
        "{}:{}:{}",
        req.provider.trim().to_ascii_lowercase(),
        req.api_url
            .as_deref()
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase(),
        req.model_name
            .as_deref()
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase()
    )
}

fn has_thinking_param_rejection(req: &TranslateRequest) -> bool {
    thinking_rejection_cache()
        .lock()
        .map(|cache| cache.contains(&thinking_cache_key(req)))
        .unwrap_or(false)
}

fn mark_thinking_param_rejected(req: &TranslateRequest) {
    if let Ok(mut cache) = thinking_rejection_cache().lock() {
        cache.insert(thinking_cache_key(req));
    }
}

/// Clear the in-memory refusal cache before a user-triggered provider probe.
/// The cache intentionally does not persist across app launches.
pub fn clear_thinking_param_rejection(req: &TranslateRequest) {
    if let Ok(mut cache) = thinking_rejection_cache().lock() {
        cache.remove(&thinking_cache_key(req));
    }
}

fn is_thinking_only_model(model: Option<&str>) -> bool {
    let model = model.unwrap_or_default().trim().to_ascii_lowercase();
    !model.is_empty()
        && (model.contains("deepseek-reasoner")
            || model.contains("thinking-")
            || model.ends_with("-thinking")
            || model.contains("-reasoning")
            || model.ends_with("-reasoner"))
}

fn has_custom_thinking_override(req: &TranslateRequest) -> bool {
    req.custom_body
        .as_ref()
        .map(|body| {
            body.keys().any(|key| {
                THINKING_PARAM_KEYS
                    .iter()
                    .any(|reserved| key.trim().eq_ignore_ascii_case(reserved))
            })
        })
        .unwrap_or(false)
}

fn url_contains(req: &TranslateRequest, needle: &str) -> bool {
    req.api_url
        .as_deref()
        .unwrap_or_default()
        .to_ascii_lowercase()
        .contains(needle)
}

/// Resolve the provider-specific parameter used to proactively disable model
/// reasoning. Unknown services deliberately return None: sending an invented
/// field to a strict OpenAI-compatible gateway would be worse than no control.
fn resolve_thinking_params(req: &TranslateRequest) -> Option<serde_json::Value> {
    if req.enable_thinking == Some(true)
        || req.thinking_control_bypassed
        || is_thinking_only_model(req.model_name.as_deref())
        || has_thinking_param_rejection(req)
        || has_custom_thinking_override(req)
    {
        return None;
    }

    let provider = req.provider.trim().to_ascii_lowercase();
    let model = req
        .model_name
        .as_deref()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();

    if provider == "qwen" || url_contains(req, "dashscope.aliyuncs.com") {
        return Some(serde_json::json!({"enable_thinking": false}));
    }
    if provider == "siliconflow" || url_contains(req, "siliconflow") {
        return Some(serde_json::json!({"enable_thinking": false}));
    }
    if url_contains(req, "volces.com") || url_contains(req, "volcengine") {
        return Some(serde_json::json!({"thinking": {"type": "disabled"}}));
    }
    if provider == "ollama" {
        return Some(serde_json::json!({"think": false}));
    }
    if provider == "gemini" || url_contains(req, "generativelanguage.googleapis.com") {
        return Some(serde_json::json!({"reasoning_effort": "none"}));
    }
    // DeepSeek chooses reasoning by model name and exposes no portable switch.
    if provider == "deepseek" || url_contains(req, "api.deepseek.com") {
        return None;
    }
    if model.starts_with("gpt-5") {
        return Some(serde_json::json!({"reasoning_effort": "minimal"}));
    }
    let is_o_series = ["o1", "o3", "o4"].iter().any(|prefix| {
        model == *prefix
            || (model.starts_with(prefix)
                && model
                    .chars()
                    .nth(prefix.len())
                    .map(|character| matches!(character, '-' | ':' | '.'))
                    .unwrap_or(false))
    });
    if is_o_series {
        return Some(serde_json::json!({"reasoning_effort": "low"}));
    }
    None
}

fn is_thinking_param_rejected_error(error: &FinalSubError) -> bool {
    let message = error.to_string().to_ascii_lowercase();
    let mentions_param = message.contains("think")
        || message.contains("reasoning_effort")
        || message.contains("reasoning.effort")
        || message.contains("budget");
    mentions_param
        && [
            "unsupported",
            "not support",
            "unrecognized",
            "unknown",
            "invalid",
            "not allowed",
            "unexpected",
            "extra_forbidden",
            "must be set to true",
            "不支持",
            "无效",
            "未知",
            "不允许",
        ]
        .iter()
        .any(|keyword| message.contains(keyword))
}

fn append_no_think_soft_switch(mut prompt: String, req: &TranslateRequest) -> String {
    if req.enable_thinking == Some(true)
        || has_custom_thinking_override(req)
        || is_thinking_only_model(req.model_name.as_deref())
        || !req
            .model_name
            .as_deref()
            .unwrap_or_default()
            .to_ascii_lowercase()
            .contains("qwen3")
        || resolve_thinking_params(req).is_some()
        || prompt.contains("/no_think")
    {
        return prompt;
    }
    prompt.push_str("\n/no_think");
    prompt
}

fn apply_thinking_control(
    mut body: serde_json::Value,
    req: &TranslateRequest,
) -> serde_json::Value {
    let Some(params) = resolve_thinking_params(req) else {
        return body;
    };
    let Some(body_object) = body.as_object_mut() else {
        return body;
    };
    let Some(params) = params.as_object() else {
        return body;
    };
    for (key, value) in params {
        if !body_object.contains_key(key) {
            body_object.insert(key.clone(), value.clone());
        }
    }
    body
}

fn detect_openai_thinking(value: &serde_json::Value) -> bool {
    let message = &value["choices"][0]["message"];
    let reasoning_content = message["reasoning_content"].as_str().unwrap_or_default();
    let thinking = message["thinking"].as_str().unwrap_or_default();
    let reasoning_tokens = value["usage"]["completion_tokens_details"]["reasoning_tokens"]
        .as_u64()
        .unwrap_or(0);
    !reasoning_content.trim().is_empty() || !thinking.trim().is_empty() || reasoning_tokens > 0
}

fn detect_ollama_thinking(value: &serde_json::Value) -> bool {
    value["message"]["thinking"]
        .as_str()
        .map(|thinking| !thinking.trim().is_empty())
        .unwrap_or(false)
        || value["thinking"]
            .as_str()
            .map(|thinking| !thinking.trim().is_empty())
            .unwrap_or(false)
}

fn detect_gemini_thinking(value: &serde_json::Value) -> bool {
    value["candidates"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|candidate| candidate["content"]["parts"].as_array())
        .flatten()
        .any(|part| part["thought"].as_bool().unwrap_or(false) || part["thinking"].is_string())
}

fn can_retry_without_thinking_params(req: &TranslateRequest) -> bool {
    !req.thinking_control_bypassed
        && !has_custom_thinking_override(req)
        && resolve_thinking_params(req).is_some()
}

pub async fn translate_text(req: &TranslateRequest) -> Result<TranslateResponse> {
    let mut attempt = req.clone();
    loop {
        match translate_text_with_structured_fallback(&attempt).await {
            Ok(response) => return Ok(response),
            Err(error)
                if can_retry_without_thinking_params(&attempt)
                    && is_thinking_param_rejected_error(&error) =>
            {
                mark_thinking_param_rejected(&attempt);
                attempt.thinking_control_bypassed = true;
            }
            Err(error) => return Err(error),
        }
    }
}

async fn translate_text_with_structured_fallback(
    req: &TranslateRequest,
) -> Result<TranslateResponse> {
    let requested_mode = req
        .structured_output
        .as_deref()
        .map(str::trim)
        .filter(|mode| !mode.is_empty());
    if let Some(mode) = requested_mode {
        if !matches!(mode, "disabled" | "json_object" | "json_schema") {
            return Err(FinalSubError::Validation(format!(
                "不支持的结构化输出模式：{mode}"
            )));
        }
    }

    let chain: &[&str] = match requested_mode {
        Some("json_schema") => &["json_schema", "json_object", "disabled"],
        Some("json_object") => &["json_object", "disabled"],
        Some("disabled") | None => &["disabled"],
        Some(_) => unreachable!(),
    };
    for (index, mode) in chain.iter().enumerate() {
        let mut attempt = req.clone();
        attempt.structured_output = Some((*mode).to_string());
        match translate_text_once(&attempt).await {
            Ok(response) => return Ok(response),
            Err(error)
                if index + 1 < chain.len() && is_structured_output_unsupported_error(&error) =>
            {
                continue;
            }
            Err(error) => return Err(error),
        }
    }
    Err(FinalSubError::Validation(
        "结构化翻译请求没有可用的回退模式".into(),
    ))
}

async fn translate_text_once(req: &TranslateRequest) -> Result<TranslateResponse> {
    let provider_info = provider_info(&req.provider).ok_or_else(|| {
        FinalSubError::Validation(format!("翻译 provider '{}' 暂未接入", req.provider))
    })?;

    if !provider_info.implemented {
        return Err(FinalSubError::Validation(format!(
            "翻译 provider '{}' 暂未接入",
            req.provider
        )));
    }

    validate_provider_request(req, &provider_info)?;

    if provider_info.requires_api_key && !provider_credentials_configured(req, &provider_info) {
        let fields = required_secret_fields(&provider_info.id).join("、");
        let hint = if fields.is_empty() {
            "API Key".to_string()
        } else {
            fields
        };
        return Err(FinalSubError::Validation(format!(
            "{} 缺少必要凭据：{}",
            provider_info.name, hint
        )));
    }

    let res = match req.provider.as_str() {
        "baidu" => translate_baidu(req).await,
        "google" => translate_google(req).await,
        "aliyun" => translate_aliyun(req).await,
        "volc" => translate_volc(req).await,
        "deeplx" => translate_deeplx(req).await,
        "ollama" => translate_ollama(req).await,
        "doubao" => translate_openai_compatible(req, "豆包").await,
        "deepseek" => translate_openai_compatible(req, "DeepSeek").await,
        "deerapi" => translate_openai_compatible(req, "DeerAPI").await,
        "gemini" => translate_gemini(req).await,
        "siliconflow" => translate_openai_compatible(req, "SiliconFlow").await,
        "qwen" => translate_openai_compatible(req, "Qwen").await,
        "custom-openai" => translate_custom_openai_compatible(req).await,
        "azure" => translate_azure(req).await,
        "azureopenai" => translate_azureopenai(req).await,
        "niutrans" => translate_niutrans(req).await,
        "tencent" => translate_tencent(req).await,
        "xunfei" => translate_xunfei(req).await,
        _ => Err(FinalSubError::Validation(format!(
            "翻译 provider '{}' 不可用，请在翻译管理中选择列表内服务",
            req.provider
        ))),
    };

    match res {
        Ok(resp) => Ok(resp),
        Err(err) => {
            let original_msg = validation_message(err);
            let redacted_msg = redact_secrets(&original_msg, req);
            Err(FinalSubError::Validation(redacted_msg))
        }
    }
}

pub async fn list_provider_models(req: &TranslateRequest) -> Result<Vec<String>> {
    let endpoint = request_endpoint(req, &req.provider)
        .ok_or_else(|| FinalSubError::Validation("获取模型列表前需要配置端点 URL".into()))?;
    let client = translation_http_client(req)?;
    let builder = match req.provider.as_str() {
        "ollama" => {
            let base = endpoint
                .trim_end_matches('/')
                .trim_end_matches("/api/generate")
                .trim_end_matches("/api/chat");
            client.get(format!("{base}/api/tags"))
        }
        "gemini" => {
            let api_key = request_api_key(req)
                .ok_or_else(|| FinalSubError::Validation("Gemini 缺少 API Key".into()))?;
            let mut base = endpoint.trim_end_matches('/').to_string();
            if base.ends_with("generativelanguage.googleapis.com") {
                base.push_str("/v1beta");
            }
            client
                .get(format!("{base}/models"))
                .header("x-goog-api-key", api_key)
        }
        "azureopenai" => {
            return Err(FinalSubError::Validation(
                "Azure OpenAI 使用部署名称，无法通过数据面端点自动枚举；请填写 Azure 中已有的 deployment name。".into(),
            ));
        }
        _ => {
            let api_key = request_api_key(req).ok_or_else(|| {
                FinalSubError::Validation(format!("{} 缺少 API Key", req.provider))
            })?;
            let models_url = openai_models_url(&endpoint);
            client.get(models_url).bearer_auth(api_key)
        }
    };
    let response = apply_custom_headers(builder, req)?
        .send()
        .await
        .map_err(|error| {
            FinalSubError::Validation(format!(
                "获取模型列表失败：{}",
                describe_reqwest_error(&error)
            ))
        })?;

    if !response.status().is_success() {
        let status = response.status();
        let body = redact_secrets(&limited_response_text(response).await, req);
        return Err(FinalSubError::Validation(format!(
            "模型列表接口返回 {status}：{body}"
        )));
    }
    let value: serde_json::Value = response
        .json()
        .await
        .map_err(|error| FinalSubError::Validation(format!("模型列表响应解析失败：{error}")))?;
    let mut models = if req.provider == "ollama" {
        value["models"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|model| model["name"].as_str())
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>()
    } else if req.provider == "gemini" {
        value["models"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|model| model["name"].as_str())
            .map(|name| name.trim_start_matches("models/").to_string())
            .collect::<Vec<_>>()
    } else {
        value["data"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|model| model["id"].as_str())
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>()
    };
    models.sort();
    models.dedup();
    models.truncate(500);
    if models.is_empty() {
        return Err(FinalSubError::Validation(
            "模型列表接口没有返回可用模型".into(),
        ));
    }
    Ok(models)
}

pub async fn test_proxy_connection(proxy_url: &str, target_url: &str) -> Result<String> {
    let proxy_url = proxy_url.trim();
    let target_url = target_url.trim();
    let proxy = reqwest::Url::parse(proxy_url)
        .map_err(|error| FinalSubError::Validation(format!("代理地址无效：{error}")))?;
    let target = reqwest::Url::parse(target_url)
        .map_err(|error| FinalSubError::Validation(format!("测试目标地址无效：{error}")))?;
    if !matches!(proxy.scheme(), "http" | "https") {
        return Err(FinalSubError::Validation(
            "翻译代理仅支持 http:// 或 https:// 地址".into(),
        ));
    }
    if !matches!(target.scheme(), "http" | "https") {
        return Err(FinalSubError::Validation(
            "代理测试目标仅支持 http:// 或 https:// 地址".into(),
        ));
    }

    let proxy = reqwest::Proxy::all(proxy_url)
        .map_err(|error| FinalSubError::Validation(format!("代理地址无效：{error}")))?;
    let client = reqwest::Client::builder()
        .user_agent("FinalSub-ProxyProbe/1.0")
        .proxy(proxy)
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(20))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|error| FinalSubError::Validation(format!("初始化代理测试失败：{error}")))?;
    let response = client
        .get(target)
        .header(reqwest::header::RANGE, "bytes=0-0")
        .send()
        .await
        .map_err(|error| {
            FinalSubError::Validation(format!("代理连接失败：{}", describe_reqwest_error(&error)))
        })?;
    Ok(format!("HTTP {}", response.status().as_u16()))
}

fn redact_secrets(err_msg: &str, req: &TranslateRequest) -> String {
    let mut redacted = err_msg.to_string();

    if let Some(ref api_key) = req.api_key {
        let trimmed = api_key.trim();
        if !trimmed.is_empty() && trimmed.len() > 3 {
            redacted = redacted.replace(trimmed, "[REDACTED_API_KEY]");
        }
    }

    if let Some(ref secrets) = req.secret_fields {
        for (field_name, val) in secrets {
            let trimmed = val.trim();
            if !trimmed.is_empty() && trimmed.len() > 3 {
                redacted = redacted.replace(
                    trimmed,
                    &format!("[REDACTED_{}]", field_name.to_uppercase()),
                );
            }
        }
    }

    redacted
}

fn provider_info(provider: &str) -> Option<TranslationProvider> {
    builtin_providers()
        .into_iter()
        .find(|candidate| candidate.id == provider)
}

fn validate_provider_request(req: &TranslateRequest, provider: &TranslationProvider) -> Result<()> {
    if provider.requires_endpoint && request_endpoint(req, &provider.id).is_none() {
        return Err(FinalSubError::Validation(format!(
            "{} 需要填写端点 URL",
            provider.name
        )));
    }

    if provider.requires_model && request_model(req).is_none() {
        return Err(FinalSubError::Validation(format!(
            "{} 需要填写模型名称",
            provider.name
        )));
    }

    Ok(())
}

fn request_endpoint(req: &TranslateRequest, provider: &str) -> Option<String> {
    configured_str(req.api_url.as_deref())
        .map(ToOwned::to_owned)
        .or_else(|| {
            provider_info(provider).and_then(|info| {
                configured_str(Some(&info.default_endpoint)).map(ToOwned::to_owned)
            })
        })
}

fn request_model(req: &TranslateRequest) -> Option<&str> {
    configured_str(req.model_name.as_deref())
}

fn request_api_key(req: &TranslateRequest) -> Option<&str> {
    configured_str(req.api_key.as_deref()).or_else(|| request_secret(req, "apiKey"))
}

fn request_secret<'a>(req: &'a TranslateRequest, field: &str) -> Option<&'a str> {
    req.secret_fields
        .as_ref()
        .and_then(|secrets| secrets.get(field))
        .and_then(|value| configured_str(Some(value.as_str())))
}

fn configured_str(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

fn render_prompt_template(template: &str, req: &TranslateRequest) -> String {
    template
        .replace("{source}", &req.source_language)
        .replace("{target}", &req.target_language)
        .replace("{text}", &req.text)
}

fn translation_system_prompt(req: &TranslateRequest) -> String {
    let mut prompt = configured_str(req.system_prompt.as_deref())
        .map(|template| render_prompt_template(template, req))
        .unwrap_or_else(|| {
            format!(
                "You are a professional subtitle translator. Translate from {} to {}. Only output the translation, preserve line breaks and structured batch keys, and do not add explanations.",
                req.source_language, req.target_language
            )
        });
    if let Some(glossary) = configured_str(req.glossary_prompt.as_deref()) {
        prompt.push_str("\n\n");
        prompt.push_str(glossary);
    }
    append_no_think_soft_switch(prompt, req)
}

fn is_structured_output_unsupported_error(error: &FinalSubError) -> bool {
    let message = error.to_string().to_ascii_lowercase();
    let mentions_format = message.contains("response_format")
        || message.contains("responseformat")
        || message.contains("responsejsonschema")
        || message.contains("json_schema")
        || message.contains("json_object")
        || message.contains("structured output")
        || message.contains("结构化输出");
    mentions_format
        && [
            "unsupported",
            "not support",
            "invalid",
            "unrecognized",
            "unknown",
            "not allowed",
            "extra_forbidden",
            "不支持",
            "无效",
            "未知",
            "不允许",
        ]
        .iter()
        .any(|keyword| message.contains(keyword))
}

fn apply_openai_structured_output(
    mut body: serde_json::Value,
    req: &TranslateRequest,
) -> Result<serde_json::Value> {
    let Some(mode) = configured_str(req.structured_output.as_deref()) else {
        return Ok(body);
    };
    let response_format = match mode {
        "disabled" => return Ok(body),
        "json_object" => serde_json::json!({"type": "json_object"}),
        "json_schema" => {
            let schema = req.response_json_schema.clone().ok_or_else(|| {
                FinalSubError::Validation("json_schema 结构化输出缺少 response JSON Schema".into())
            })?;
            serde_json::json!({
                "type": "json_schema",
                "json_schema": {
                    "name": "subtitle_translation_batch",
                    "strict": true,
                    "schema": schema
                }
            })
        }
        other => {
            return Err(FinalSubError::Validation(format!(
                "不支持的结构化输出模式：{other}"
            )))
        }
    };
    body.as_object_mut()
        .ok_or_else(|| FinalSubError::Validation("翻译请求体必须是 JSON 对象".into()))?
        .insert("response_format".into(), response_format);
    Ok(body)
}

fn apply_ollama_structured_output(
    mut body: serde_json::Value,
    req: &TranslateRequest,
) -> Result<serde_json::Value> {
    let Some(mode) = configured_str(req.structured_output.as_deref()) else {
        return Ok(body);
    };
    let format = match mode {
        "disabled" => return Ok(body),
        "json_object" => serde_json::Value::String("json".into()),
        "json_schema" => req.response_json_schema.clone().ok_or_else(|| {
            FinalSubError::Validation("Ollama json_schema 请求缺少 response JSON Schema".into())
        })?,
        other => {
            return Err(FinalSubError::Validation(format!(
                "不支持的结构化输出模式：{other}"
            )))
        }
    };
    body.as_object_mut()
        .ok_or_else(|| FinalSubError::Validation("Ollama 请求体必须是 JSON 对象".into()))?
        .insert("format".into(), format);
    Ok(body)
}

fn apply_gemini_structured_output(
    mut body: serde_json::Value,
    req: &TranslateRequest,
) -> Result<serde_json::Value> {
    let Some(mode) = configured_str(req.structured_output.as_deref()) else {
        return Ok(body);
    };
    if mode == "disabled" {
        return Ok(body);
    }
    let generation_config = body
        .as_object_mut()
        .ok_or_else(|| FinalSubError::Validation("Gemini 请求体必须是 JSON 对象".into()))?
        .entry("generationConfig")
        .or_insert_with(|| serde_json::json!({}));
    let config = generation_config.as_object_mut().ok_or_else(|| {
        FinalSubError::Validation("Gemini generationConfig 必须是 JSON 对象".into())
    })?;
    config.insert(
        "responseMimeType".into(),
        serde_json::Value::String("application/json".into()),
    );
    match mode {
        "json_object" => {
            config.remove("responseJsonSchema");
        }
        "json_schema" => {
            let schema = req.response_json_schema.clone().ok_or_else(|| {
                FinalSubError::Validation("Gemini json_schema 请求缺少 response JSON Schema".into())
            })?;
            config.insert("responseJsonSchema".into(), schema);
        }
        other => {
            return Err(FinalSubError::Validation(format!(
                "不支持的结构化输出模式：{other}"
            )))
        }
    }
    Ok(body)
}

fn translation_user_prompt(req: &TranslateRequest) -> String {
    match configured_str(req.user_prompt.as_deref()) {
        Some(template) if template.contains("{text}") => render_prompt_template(template, req),
        Some(template) => format!("{}\n\n{}", render_prompt_template(template, req), req.text),
        None => req.text.clone(),
    }
}

fn render_secret_template(template: &str, req: &TranslateRequest) -> String {
    let api_key = request_api_key(req).unwrap_or("");
    let mut rendered = template
        .replace("${API_KEY}", api_key)
        .replace("{apiKey}", api_key);
    if let Some(secrets) = &req.secret_fields {
        for (field, value) in secrets {
            rendered = rendered.replace(&format!("{{secret:{field}}}"), value);
        }
    }
    rendered
}

fn render_custom_json(value: serde_json::Value, req: &TranslateRequest) -> serde_json::Value {
    match value {
        serde_json::Value::String(value) => {
            serde_json::Value::String(render_secret_template(&value, req))
        }
        serde_json::Value::Array(values) => serde_json::Value::Array(
            values
                .into_iter()
                .map(|value| render_custom_json(value, req))
                .collect(),
        ),
        serde_json::Value::Object(values) => serde_json::Value::Object(
            values
                .into_iter()
                .map(|(key, value)| (key, render_custom_json(value, req)))
                .collect(),
        ),
        value => value,
    }
}

fn merge_json_value(target: &mut serde_json::Value, update: serde_json::Value) {
    match (target, update) {
        (serde_json::Value::Object(target), serde_json::Value::Object(update)) => {
            for (key, value) in update {
                match target.get_mut(&key) {
                    Some(existing) => merge_json_value(existing, value),
                    None => {
                        target.insert(key, value);
                    }
                }
            }
        }
        (target, update) => *target = update,
    }
}

fn merge_custom_body(
    mut base: serde_json::Value,
    req: &TranslateRequest,
    reserved_keys: &[&str],
) -> Result<serde_json::Value> {
    let Some(custom_body) = &req.custom_body else {
        return Ok(base);
    };
    if custom_body.len() > 64 {
        return Err(FinalSubError::Validation(
            "单个翻译服务最多配置 64 个自定义请求体参数".into(),
        ));
    }
    let encoded = serde_json::to_vec(custom_body)?;
    if encoded.len() > 64 * 1024 {
        return Err(FinalSubError::Validation(
            "单个翻译服务的自定义请求体不能超过 64 KiB".into(),
        ));
    }
    for key in custom_body.keys() {
        if reserved_keys.contains(&key.as_str()) {
            return Err(FinalSubError::Validation(format!(
                "自定义请求体不能覆盖核心字段：{key}"
            )));
        }
    }
    merge_json_value(
        &mut base,
        render_custom_json(serde_json::Value::Object(custom_body.clone()), req),
    );
    Ok(base)
}

fn apply_custom_headers(
    mut builder: reqwest::RequestBuilder,
    req: &TranslateRequest,
) -> Result<reqwest::RequestBuilder> {
    let Some(headers) = &req.custom_headers else {
        return Ok(builder);
    };
    if headers.len() > 64 {
        return Err(FinalSubError::Validation(
            "单个翻译服务最多配置 64 个自定义请求头".into(),
        ));
    }
    for (name, value) in headers {
        let normalized = name.trim().to_ascii_lowercase();
        if matches!(
            normalized.as_str(),
            "host" | "content-length" | "transfer-encoding" | "connection"
        ) {
            return Err(FinalSubError::Validation(format!(
                "不允许覆盖受保护的请求头：{name}"
            )));
        }
        let name = reqwest::header::HeaderName::from_bytes(name.trim().as_bytes())
            .map_err(|error| FinalSubError::Validation(format!("自定义请求头名称无效：{error}")))?;
        let rendered = render_secret_template(value, req);
        let value = reqwest::header::HeaderValue::from_str(&rendered)
            .map_err(|error| FinalSubError::Validation(format!("自定义请求头值无效：{error}")))?;
        builder = builder.header(name, value);
    }
    Ok(builder)
}

fn has_any_secret_field(req: &TranslateRequest) -> bool {
    req.secret_fields
        .as_ref()
        .map(|secrets| {
            secrets
                .values()
                .any(|value| configured_str(Some(value.as_str())).is_some())
        })
        .unwrap_or(false)
}

fn provider_credentials_configured(req: &TranslateRequest, provider: &TranslationProvider) -> bool {
    let required_fields = required_secret_fields(&provider.id);
    if required_fields.is_empty() {
        return request_api_key(req).is_some() || has_any_secret_field(req);
    }

    required_fields
        .iter()
        .all(|field| request_secret(req, field).is_some())
}

fn required_secret_fields(provider: &str) -> Vec<&'static str> {
    match provider {
        "baidu" => vec!["appId", "secretKey"],
        "google" | "doubao" | "deepseek" | "deerapi" | "gemini" | "siliconflow" | "qwen"
        | "custom-openai" | "azure" | "azureopenai" | "niutrans" => vec!["apiKey"],
        "aliyun" | "volc" => vec!["accessKeyId", "accessKeySecret"],
        "tencent" => vec!["secretId", "secretKey"],
        "xunfei" => vec!["appId", "apiKey", "apiSecret"],
        _ => vec![],
    }
}

async fn translate_baidu(req: &TranslateRequest) -> Result<TranslateResponse> {
    let app_id = request_secret(req, "appId").unwrap_or("");
    let secret_key = request_secret(req, "secretKey").unwrap_or("");
    if app_id.is_empty() || secret_key.is_empty() {
        return Err(FinalSubError::Validation(
            "百度翻译缺少 AppID 或 SecretKey".into(),
        ));
    }
    let salt = uuid::Uuid::new_v4().to_string();
    let sign_str = format!("{}{}{}{}", app_id, req.text, salt, secret_key);
    let sign = format!("{:x}", md5::compute(sign_str));

    let client = translation_http_client(req)?;
    let url = "https://fanyi-api.baidu.com/api/trans/vip/translate";
    let params = [
        ("q", req.text.as_str()),
        ("from", &map_lang_baidu(&req.source_language)),
        ("to", &map_lang_baidu(&req.target_language)),
        ("appid", app_id),
        ("salt", &salt),
        ("sign", &sign),
    ];

    let resp = client.post(url).form(&params).send().await.map_err(|e| {
        FinalSubError::Validation(format!("百度翻译请求失败: {}", describe_reqwest_error(&e)))
    })?;
    if !resp.status().is_success() {
        return Err(FinalSubError::Validation(format!(
            "百度翻译返回错误: {}",
            resp.status()
        )));
    }
    let res_json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| FinalSubError::Validation(format!("百度翻译解析 JSON 失败: {e}")))?;

    if let Some(err_code) = res_json["error_code"].as_str() {
        let err_msg = res_json["error_msg"].as_str().unwrap_or("未知百度翻译错误");
        return Err(FinalSubError::Validation(format!(
            "百度翻译 API 报错 [{err_code}]: {err_msg}"
        )));
    }

    let translated = res_json["trans_result"][0]["dst"]
        .as_str()
        .ok_or_else(|| FinalSubError::Validation("百度翻译返回格式异常，找不到 dst 字段".into()))?
        .to_string();

    Ok(TranslateResponse {
        translated_text: translated,
        provider: "baidu".into(),
        success: true,
        error: None,
        thinking_enabled: None,
    })
}

async fn translate_google(req: &TranslateRequest) -> Result<TranslateResponse> {
    let api_key = request_api_key(req).unwrap_or("");
    if api_key.is_empty() {
        return Err(FinalSubError::Validation("谷歌翻译缺少 API Key".into()));
    }
    let client = translation_http_client(req)?;
    let url = "https://translation.googleapis.com/language/translate/v2";

    let source_lang = if req.source_language == "auto" {
        ""
    } else {
        &req.source_language
    };

    let mut query = vec![
        ("key", api_key.to_string()),
        ("q", req.text.clone()),
        ("target", req.target_language.clone()),
        ("format", "text".to_string()),
    ];
    if !source_lang.is_empty() {
        query.push(("source", source_lang.to_string()));
    }

    let resp = client.post(url).query(&query).send().await.map_err(|e| {
        FinalSubError::Validation(format!("谷歌翻译请求失败: {}", describe_reqwest_error(&e)))
    })?;

    if !resp.status().is_success() {
        let status = resp.status();
        let err_body = limited_response_text(resp).await;
        return Err(FinalSubError::Validation(format!(
            "谷歌翻译返回错误 {status}: {err_body}"
        )));
    }

    let res_json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| FinalSubError::Validation(format!("谷歌翻译解析 JSON 失败: {e}")))?;

    let translated = res_json["data"]["translations"][0]["translatedText"]
        .as_str()
        .ok_or_else(|| FinalSubError::Validation("谷歌翻译响应格式异常".into()))?
        .to_string();

    Ok(TranslateResponse {
        translated_text: decode_simple_html(&translated),
        provider: "google".into(),
        success: true,
        error: None,
        thinking_enabled: None,
    })
}

async fn translate_deeplx(req: &TranslateRequest) -> Result<TranslateResponse> {
    let api_url =
        request_endpoint(req, "deeplx").unwrap_or_else(|| "http://localhost:1188/translate".into());
    let client = translation_http_client(req)?;
    let body = serde_json::json!({
        "text": req.text,
        "source_lang": req.source_language.to_uppercase(),
        "target_lang": req.target_language.to_uppercase(),
    });

    let resp = client
        .post(api_url)
        .json(&body)
        .timeout(std::time::Duration::from_secs(30))
        .send()
        .await
        .map_err(|e| {
            FinalSubError::Validation(format!("DeepLX 请求失败：{}", describe_reqwest_error(&e)))
        })?;

    if !resp.status().is_success() {
        return Err(FinalSubError::Validation(format!(
            "DeepLX 返回错误：{}",
            resp.status()
        )));
    }

    let data: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| FinalSubError::Validation(format!("DeepLX 响应解析失败：{e}")))?;

    let translated = data["data"]
        .as_str()
        .or_else(|| data["translated_text"].as_str())
        .unwrap_or("")
        .to_string();

    Ok(TranslateResponse {
        translated_text: translated,
        provider: "deeplx".into(),
        success: true,
        error: None,
        thinking_enabled: None,
    })
}

async fn translate_ollama(req: &TranslateRequest) -> Result<TranslateResponse> {
    let api_url = request_endpoint(req, "ollama")
        .unwrap_or_else(|| "http://localhost:11434/api/generate".into());
    let model = request_model(req).unwrap_or("qwen2.5:7b");

    let prompt = format!(
        "{}\n\n{}",
        translation_system_prompt(req),
        translation_user_prompt(req)
    );

    let client = translation_http_client(req)?;
    let body = merge_custom_body(
        serde_json::json!({
            "model": model,
            "prompt": prompt,
            "stream": false,
            "options": {
                "temperature": 0.2,
                "num_ctx": 8192
            }
        }),
        req,
        &["model", "prompt", "stream"],
    )?;
    let body = apply_thinking_control(body, req);
    let body = apply_ollama_structured_output(body, req)?;

    let builder = apply_custom_headers(client.post(api_url), req)?;
    let resp = builder
        .json(&body)
        .timeout(std::time::Duration::from_secs(120))
        .send()
        .await
        .map_err(|e| {
            FinalSubError::Validation(format!("Ollama 请求失败：{}", describe_reqwest_error(&e)))
        })?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = limited_response_text(resp).await;
        return Err(FinalSubError::Validation(format!(
            "Ollama 返回错误 {status}：{body}"
        )));
    }

    let data: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| FinalSubError::Validation(format!("Ollama 响应解析失败：{e}")))?;

    let translated = data["response"].as_str().unwrap_or("").trim().to_string();
    let thinking_enabled = detect_ollama_thinking(&data);

    Ok(TranslateResponse {
        translated_text: translated,
        provider: "ollama".into(),
        success: true,
        error: None,
        thinking_enabled: Some(thinking_enabled),
    })
}

async fn translate_openai_compatible(
    req: &TranslateRequest,
    provider_name: &str,
) -> Result<TranslateResponse> {
    let api_url = openai_chat_completions_url(
        &request_endpoint(req, &req.provider).unwrap_or_else(|| "https://api.openai.com/v1".into()),
    );
    let api_key = request_api_key(req).unwrap_or("");
    if api_key.is_empty() {
        return Err(FinalSubError::Validation(format!(
            "{provider_name} 缺少 API Key"
        )));
    }
    let model = request_model(req).unwrap_or("gpt-4o-mini");

    let system_prompt = translation_system_prompt(req);
    let user_prompt = translation_user_prompt(req);

    let client = translation_http_client(req)?;
    let body = merge_custom_body(
        serde_json::json!({
            "model": model,
            "messages": [
                {"role": "system", "content": system_prompt},
                {"role": "user", "content": user_prompt}
            ],
            "temperature": 0.3,
        }),
        req,
        &["model", "messages"],
    )?;
    let body = apply_thinking_control(body, req);
    let body = apply_openai_structured_output(body, req)?;

    let builder = client
        .post(&api_url)
        .header("Authorization", format!("Bearer {api_key}"))
        .header("Content-Type", "application/json");
    let resp = apply_custom_headers(builder, req)?
        .json(&body)
        .timeout(std::time::Duration::from_secs(120))
        .send()
        .await
        .map_err(|e| {
            FinalSubError::Validation(format!(
                "{provider_name} 请求失败：{}",
                describe_reqwest_error(&e)
            ))
        })?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body_text = limited_response_text(resp).await;
        return Err(FinalSubError::Validation(format!(
            "{provider_name} 返回错误 {status}：{body_text}"
        )));
    }

    let data: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| FinalSubError::Validation(format!("{provider_name} 响应解析失败：{e}")))?;
    let thinking_enabled = detect_openai_thinking(&data);

    let translated = data["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("")
        .trim()
        .to_string();

    Ok(TranslateResponse {
        translated_text: translated,
        provider: req.provider.clone(),
        success: true,
        error: None,
        thinking_enabled: Some(thinking_enabled),
    })
}

async fn translate_gemini(req: &TranslateRequest) -> Result<TranslateResponse> {
    let api_url = gemini_generate_content_url(
        &request_endpoint(req, "gemini")
            .unwrap_or_else(|| "https://generativelanguage.googleapis.com/v1beta".into()),
        request_model(req).unwrap_or("gemini-2.5-flash"),
    );
    let api_key = request_api_key(req).unwrap_or("");
    if api_key.is_empty() {
        return Err(FinalSubError::Validation("Gemini 缺少 API Key".into()));
    }

    let system_prompt = translation_system_prompt(req);
    let user_prompt = translation_user_prompt(req);

    let client = translation_http_client(req)?;
    let body = merge_custom_body(
        serde_json::json!({
            "systemInstruction": {
                "parts": [{"text": system_prompt}]
            },
            "contents": [{
                "role": "user",
                "parts": [{"text": user_prompt}]
            }],
            "generationConfig": {
                "temperature": 0.2
            }
        }),
        req,
        &["systemInstruction", "contents"],
    )?;
    let body = apply_thinking_control(body, req);
    let body = apply_gemini_structured_output(body, req)?;

    let builder = client
        .post(&api_url)
        .header("x-goog-api-key", api_key)
        .header("Content-Type", "application/json");
    let resp = apply_custom_headers(builder, req)?
        .json(&body)
        .timeout(std::time::Duration::from_secs(120))
        .send()
        .await
        .map_err(|e| {
            FinalSubError::Validation(format!("Gemini 请求失败：{}", describe_reqwest_error(&e)))
        })?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body_text = limited_response_text(resp).await;
        return Err(FinalSubError::Validation(format!(
            "Gemini 返回错误 {status}：{body_text}"
        )));
    }

    let data: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| FinalSubError::Validation(format!("Gemini 响应解析失败：{e}")))?;
    let thinking_enabled = detect_gemini_thinking(&data);

    let translated = data["candidates"][0]["content"]["parts"]
        .as_array()
        .map(|parts| {
            parts
                .iter()
                .filter_map(|part| part["text"].as_str())
                .collect::<Vec<_>>()
                .join("")
        })
        .unwrap_or_default()
        .trim()
        .to_string();

    if translated.is_empty() {
        return Err(FinalSubError::Validation(
            "Gemini 响应中没有可用译文".into(),
        ));
    }

    Ok(TranslateResponse {
        translated_text: translated,
        provider: req.provider.clone(),
        success: true,
        error: None,
        thinking_enabled: Some(thinking_enabled),
    })
}

async fn translate_custom_openai_compatible(req: &TranslateRequest) -> Result<TranslateResponse> {
    if request_endpoint(req, "custom-openai").is_none() {
        return Err(FinalSubError::Validation(
            "自定义 OpenAI 兼容服务需要填写端点 URL".into(),
        ));
    }
    if request_model(req).is_none() {
        return Err(FinalSubError::Validation(
            "自定义 OpenAI 兼容服务需要填写模型名称".into(),
        ));
    }
    translate_openai_compatible(req, "自定义 OpenAI 兼容").await
}

fn openai_chat_completions_url(raw: &str) -> String {
    let trimmed = raw.trim().trim_end_matches('/');
    if trimmed.ends_with("/chat/completions") {
        trimmed.to_string()
    } else {
        format!("{trimmed}/chat/completions")
    }
}

fn openai_models_url(raw: &str) -> String {
    let trimmed = raw.trim().trim_end_matches('/');
    if let Some(base) = trimmed.strip_suffix("/chat/completions") {
        format!("{base}/models")
    } else if trimmed.ends_with("/models") {
        trimmed.to_string()
    } else {
        format!("{trimmed}/models")
    }
}

fn gemini_generate_content_url(raw: &str, model: &str) -> String {
    let mut base = raw.trim().trim_end_matches('/').to_string();
    if base.contains(":generateContent") {
        return base;
    }
    if base.ends_with("generativelanguage.googleapis.com") {
        base.push_str("/v1beta");
    }
    let model = model.trim().trim_start_matches("models/");
    format!("{base}/models/{model}:generateContent")
}

// ======================== Provider Implementations ========================

fn map_lang_baidu(lang: &str) -> String {
    match lang.to_lowercase().as_str() {
        "zh" | "zh-cn" | "zh-hans" => "zh".into(),
        "zh-hant" | "zh-tw" | "zh-hk" => "cht".into(),
        "en" => "en".into(),
        "ja" | "jp" => "jp".into(),
        "ko" | "kor" => "kor".into(),
        "fr" => "fra".into(),
        "es" => "spa".into(),
        "ru" => "ru".into(),
        "auto" => "auto".into(),
        other => other.to_string(),
    }
}

fn decode_simple_html(s: &str) -> String {
    s.replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
}

fn hmac_sha256(key: &[u8], msg: &[u8]) -> Vec<u8> {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    type HmacSha256 = Hmac<Sha256>;
    let mut mac = HmacSha256::new_from_slice(key).unwrap();
    mac.update(msg);
    mac.finalize().into_bytes().to_vec()
}

fn hmac_sha1(key: &[u8], msg: &[u8]) -> Vec<u8> {
    use hmac::{Hmac, Mac};
    use sha1::Sha1;
    type HmacSha1 = Hmac<Sha1>;
    let mut mac = HmacSha1::new_from_slice(key).unwrap();
    mac.update(msg);
    mac.finalize().into_bytes().to_vec()
}

fn sha256_hex(data: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(data);
    hex::encode(hasher.finalize())
}

fn base64_encode_bytes(b: &[u8]) -> String {
    use base64::Engine;
    base64::prelude::BASE64_STANDARD.encode(b)
}

fn base64_decode(s: &str) -> std::result::Result<Vec<u8>, base64::DecodeError> {
    use base64::Engine;
    base64::prelude::BASE64_STANDARD.decode(s)
}

fn sha256_base64(data: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(data);
    base64_encode_bytes(&hasher.finalize())
}

async fn translate_azure(req: &TranslateRequest) -> Result<TranslateResponse> {
    let api_key = request_api_key(req).unwrap_or("");
    if api_key.is_empty() {
        return Err(FinalSubError::Validation("微软翻译缺少 API Key".into()));
    }
    let region = request_secret(req, "region").unwrap_or("");

    let base_url = request_endpoint(req, "azure")
        .unwrap_or_else(|| "https://api.cognitive.microsofttranslator.com".into());
    let trimmed = base_url.trim().trim_end_matches('/');

    let source_lang = if req.source_language == "auto" {
        ""
    } else {
        &req.source_language
    };
    let mut url = format!(
        "{trimmed}/translate?api-version=3.0&to={}",
        req.target_language
    );
    if !source_lang.is_empty() {
        url.push_str(&format!("&from={source_lang}"));
    }

    let client = translation_http_client(req)?;
    let mut builder = client
        .post(&url)
        .header("Ocp-Apim-Subscription-Key", api_key)
        .header("Content-Type", "application/json");

    if !region.is_empty() {
        builder = builder.header("Ocp-Apim-Subscription-Region", region);
    }

    let body = serde_json::json!([{"Text": req.text}]);
    let resp = builder.json(&body).send().await.map_err(|e| {
        FinalSubError::Validation(format!("微软翻译请求失败: {}", describe_reqwest_error(&e)))
    })?;

    if !resp.status().is_success() {
        let status = resp.status();
        let err_body = limited_response_text(resp).await;
        return Err(FinalSubError::Validation(format!(
            "微软翻译返回错误 {status}: {err_body}"
        )));
    }

    let res_json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| FinalSubError::Validation(format!("微软翻译解析 JSON 失败: {e}")))?;

    let translated = res_json[0]["translations"][0]["text"]
        .as_str()
        .ok_or_else(|| FinalSubError::Validation("微软翻译返回数据格式不正确".into()))?
        .to_string();

    Ok(TranslateResponse {
        translated_text: translated,
        provider: "azure".into(),
        success: true,
        error: None,
        thinking_enabled: None,
    })
}

async fn translate_azureopenai(req: &TranslateRequest) -> Result<TranslateResponse> {
    let api_key = request_api_key(req).unwrap_or("");
    if api_key.is_empty() {
        return Err(FinalSubError::Validation(
            "Azure OpenAI 缺少 API Key".into(),
        ));
    }
    let api_url = request_endpoint(req, "azureopenai").unwrap_or_default();
    if api_url.is_empty() {
        return Err(FinalSubError::Validation(
            "Azure OpenAI 缺少端点 URL".into(),
        ));
    }
    let deployment = request_model(req).unwrap_or("");
    if deployment.is_empty() {
        return Err(FinalSubError::Validation(
            "Azure OpenAI 缺少部署模型名称".into(),
        ));
    }
    let api_version = request_secret(req, "apiVersion").unwrap_or("2024-02-15-preview");

    let trimmed_url = api_url.trim().trim_end_matches('/');
    let url = format!(
        "{}/openai/deployments/{}/chat/completions?api-version={}",
        trimmed_url, deployment, api_version
    );

    let system_prompt = translation_system_prompt(req);
    let user_prompt = translation_user_prompt(req);

    let client = translation_http_client(req)?;
    let body = merge_custom_body(
        serde_json::json!({
            "messages": [
                {"role": "system", "content": system_prompt},
                {"role": "user", "content": user_prompt}
            ],
            "temperature": 0.3,
        }),
        req,
        &["messages"],
    )?;
    let body = apply_thinking_control(body, req);
    let body = apply_openai_structured_output(body, req)?;

    let builder = client
        .post(&url)
        .header("api-key", api_key)
        .header("Content-Type", "application/json");
    let resp = apply_custom_headers(builder, req)?
        .json(&body)
        .send()
        .await
        .map_err(|e| {
            FinalSubError::Validation(format!(
                "Azure OpenAI 请求失败: {}",
                describe_reqwest_error(&e)
            ))
        })?;

    if !resp.status().is_success() {
        let status = resp.status();
        let err_body = limited_response_text(resp).await;
        return Err(FinalSubError::Validation(format!(
            "Azure OpenAI 返回错误 {status}: {err_body}"
        )));
    }

    let res_json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| FinalSubError::Validation(format!("Azure OpenAI 响应解析失败: {e}")))?;
    let thinking_enabled = detect_openai_thinking(&res_json);

    let translated = res_json["choices"][0]["message"]["content"]
        .as_str()
        .ok_or_else(|| FinalSubError::Validation("Azure OpenAI 响应中缺少 content".into()))?
        .trim()
        .to_string();

    Ok(TranslateResponse {
        translated_text: translated,
        provider: "azureopenai".into(),
        success: true,
        error: None,
        thinking_enabled: Some(thinking_enabled),
    })
}

async fn translate_niutrans(req: &TranslateRequest) -> Result<TranslateResponse> {
    let api_key = request_api_key(req).unwrap_or("");
    if api_key.is_empty() {
        return Err(FinalSubError::Validation("小牛翻译缺少 API Key".into()));
    }
    let client = translation_http_client(req)?;
    let url = "https://api.niutrans.com/NiuTransServer/translation";

    let params = [
        ("from", req.source_language.as_str()),
        ("to", req.target_language.as_str()),
        ("apikey", api_key),
        ("src_text", req.text.as_str()),
    ];

    let resp = client.post(url).form(&params).send().await.map_err(|e| {
        FinalSubError::Validation(format!("小牛翻译请求失败: {}", describe_reqwest_error(&e)))
    })?;

    if !resp.status().is_success() {
        return Err(FinalSubError::Validation(format!(
            "小牛翻译返回错误: {}",
            resp.status()
        )));
    }

    let res_json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| FinalSubError::Validation(format!("小牛翻译解析 JSON 失败: {e}")))?;

    if let Some(err_code) = res_json["error_code"].as_str() {
        if err_code != "0" {
            return Err(FinalSubError::Validation(format!(
                "小牛翻译 API 报错 [{err_code}]"
            )));
        }
    }

    let translated = res_json["tgt_text"]
        .as_str()
        .ok_or_else(|| FinalSubError::Validation("小牛翻译返回数据格式不正确".into()))?
        .to_string();

    Ok(TranslateResponse {
        translated_text: translated,
        provider: "niutrans".into(),
        success: true,
        error: None,
        thinking_enabled: None,
    })
}

async fn translate_tencent(req: &TranslateRequest) -> Result<TranslateResponse> {
    let secret_id = request_secret(req, "secretId").unwrap_or("");
    let secret_key = request_secret(req, "secretKey").unwrap_or("");
    let region = request_secret(req, "region").unwrap_or("ap-guangzhou");

    if secret_id.is_empty() || secret_key.is_empty() {
        return Err(FinalSubError::Validation(
            "腾讯翻译缺少 secretId 或 secretKey".into(),
        ));
    }

    let now = chrono::Utc::now();
    let timestamp = now.timestamp();
    let date = now.format("%Y-%m-%d").to_string();

    let payload = serde_json::json!({
        "SourceText": req.text,
        "Source": map_lang_tencent(&req.source_language),
        "Target": map_lang_tencent(&req.target_language),
        "ProjectId": 0
    });
    let payload_str = payload.to_string();
    let hashed_payload = sha256_hex(payload_str.as_bytes());

    let canonical_req = format!(
        "POST\n/\n\ncontent-type:application/json; charset=utf-8\nhost:tmt.tencentcloudapi.com\n\ncontent-type;host\n{}",
        hashed_payload
    );
    let hashed_canonical_req = sha256_hex(canonical_req.as_bytes());

    let credential_scope = format!("{}/tmt/tc3_request", date);
    let string_to_sign = format!(
        "TC3-HMAC-SHA256\n{}\n{}\n{}",
        timestamp, credential_scope, hashed_canonical_req
    );

    let k_date = hmac_sha256(format!("TC3{}", secret_key).as_bytes(), date.as_bytes());
    let k_service = hmac_sha256(&k_date, b"tmt");
    let k_signing = hmac_sha256(&k_service, b"tc3_request");
    let signature = hex::encode(hmac_sha256(&k_signing, string_to_sign.as_bytes()));

    let authorization = format!(
        "TC3-HMAC-SHA256 Credential={}/{}, SignedHeaders=content-type;host, Signature={}",
        secret_id, credential_scope, signature
    );

    let client = translation_http_client(req)?;
    let resp = client
        .post("https://tmt.tencentcloudapi.com")
        .header("Authorization", authorization)
        .header("Content-Type", "application/json; charset=utf-8")
        .header("Host", "tmt.tencentcloudapi.com")
        .header("X-TC-Action", "TextTranslate")
        .header("X-TC-Version", "2018-03-21")
        .header("X-TC-Timestamp", timestamp.to_string())
        .header("X-TC-Region", region)
        .body(payload_str)
        .send()
        .await
        .map_err(|e| {
            FinalSubError::Validation(format!("腾讯翻译请求失败: {}", describe_reqwest_error(&e)))
        })?;

    if !resp.status().is_success() {
        return Err(FinalSubError::Validation(format!(
            "腾讯翻译返回错误: {}",
            resp.status()
        )));
    }

    let res_json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| FinalSubError::Validation(format!("腾讯翻译解析 JSON 失败: {e}")))?;

    if let Some(err) = res_json["Response"]["Error"].as_object() {
        let code = err
            .get("Code")
            .and_then(|v| v.as_str())
            .unwrap_or("Unknown");
        let msg = err
            .get("Message")
            .and_then(|v| v.as_str())
            .unwrap_or("Unknown");
        return Err(FinalSubError::Validation(format!(
            "腾讯翻译 API 报错 [{code}]: {msg}"
        )));
    }

    let translated = res_json["Response"]["TargetText"]
        .as_str()
        .ok_or_else(|| FinalSubError::Validation("腾讯翻译返回格式异常".into()))?
        .to_string();

    Ok(TranslateResponse {
        translated_text: translated,
        provider: "tencent".into(),
        success: true,
        error: None,
        thinking_enabled: None,
    })
}

fn map_lang_tencent(lang: &str) -> String {
    match lang.to_lowercase().as_str() {
        "zh" | "zh-cn" | "zh-hans" => "zh".into(),
        "zh-hant" | "zh-tw" | "zh-hk" => "zh-TW".into(),
        "en" => "en".into(),
        "ja" | "jp" => "ja".into(),
        "ko" | "kor" => "ko".into(),
        "fr" => "fr".into(),
        "es" => "es".into(),
        "ru" => "ru".into(),
        "auto" => "auto".into(),
        other => other.to_string(),
    }
}

async fn translate_aliyun(req: &TranslateRequest) -> Result<TranslateResponse> {
    let access_key = request_secret(req, "accessKeyId").unwrap_or("");
    let secret_key = request_secret(req, "accessKeySecret").unwrap_or("");

    if access_key.is_empty() || secret_key.is_empty() {
        return Err(FinalSubError::Validation(
            "阿里云翻译缺少 accessKeyId 或 accessKeySecret".into(),
        ));
    }

    let now = chrono::Utc::now();
    let timestamp_iso = now.format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let nonce = uuid::Uuid::new_v4().to_string();

    let mut params = vec![
        ("Format", "JSON".to_string()),
        ("Version", "2018-10-12".to_string()),
        ("Action", "TranslateGeneral".to_string()),
        ("AccessKeyId", access_key.to_string()),
        ("SignatureMethod", "HMAC-SHA1".to_string()),
        ("SignatureVersion", "1.0".to_string()),
        ("SignatureNonce", nonce),
        ("Timestamp", timestamp_iso),
        ("SourceLanguage", map_lang_aliyun(&req.source_language)),
        ("TargetLanguage", map_lang_aliyun(&req.target_language)),
        ("SourceText", req.text.clone()),
        ("FormatType", "text".to_string()),
        ("Scene", "general".to_string()),
    ];

    params.sort_by(|a, b| a.0.cmp(b.0));

    let query_string: String = params
        .iter()
        .map(|(k, v)| format!("{}={}", aliyun_percent_encode(k), aliyun_percent_encode(v)))
        .collect::<Vec<_>>()
        .join("&");

    let string_to_sign = format!("POST&%2F&{}", aliyun_percent_encode(&query_string));
    let signing_key = format!("{}&", secret_key);
    let signature = base64_encode_bytes(&hmac_sha1(
        signing_key.as_bytes(),
        string_to_sign.as_bytes(),
    ));

    let client = translation_http_client(req)?;
    let url = "https://mt.aliyuncs.com";

    let mut body_params = params.clone();
    body_params.push(("Signature", signature));

    let resp = client
        .post(url)
        .form(&body_params)
        .send()
        .await
        .map_err(|e| {
            FinalSubError::Validation(format!(
                "阿里云翻译请求失败: {}",
                describe_reqwest_error(&e)
            ))
        })?;

    if !resp.status().is_success() {
        return Err(FinalSubError::Validation(format!(
            "阿里云翻译返回错误: {}",
            resp.status()
        )));
    }

    let res_json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| FinalSubError::Validation(format!("阿里云翻译解析 JSON 失败: {e}")))?;

    if let Some(code) = res_json["Code"].as_str() {
        if code != "200" {
            let msg = res_json["Message"].as_str().unwrap_or("未知阿里云错误");
            return Err(FinalSubError::Validation(format!(
                "阿里云翻译 API 报错 [{code}]: {msg}"
            )));
        }
    }

    let translated = res_json["Data"]["Translated"]
        .as_str()
        .ok_or_else(|| FinalSubError::Validation("阿里云翻译返回格式异常".into()))?
        .to_string();

    Ok(TranslateResponse {
        translated_text: translated,
        provider: "aliyun".into(),
        success: true,
        error: None,
        thinking_enabled: None,
    })
}

fn aliyun_percent_encode(s: &str) -> String {
    use percent_encoding::{utf8_percent_encode, AsciiSet, CONTROLS};
    const ALIYUN_SET: &AsciiSet = &CONTROLS
        .add(b' ')
        .add(b'/')
        .add(b':')
        .add(b';')
        .add(b'=')
        .add(b'?')
        .add(b'@')
        .add(b'&')
        .add(b'+')
        .add(b'$')
        .add(b',')
        .add(b'%')
        .add(b'#')
        .add(b'[')
        .add(b']')
        .add(b'!')
        .add(b'\'')
        .add(b'(')
        .add(b')')
        .add(b'*');
    utf8_percent_encode(s, ALIYUN_SET)
        .to_string()
        .replace("+", "%20")
        .replace("*", "%2A")
        .replace("%7E", "~")
}

fn map_lang_aliyun(lang: &str) -> String {
    match lang.to_lowercase().as_str() {
        "zh" | "zh-cn" | "zh-hans" => "zh".into(),
        "zh-hant" | "zh-tw" | "zh-hk" => "zh-tw".into(),
        "en" => "en".into(),
        "ja" | "jp" => "ja".into(),
        "ko" | "kor" => "ko".into(),
        "fr" => "fr".into(),
        "es" => "es".into(),
        "ru" => "ru".into(),
        "auto" => "auto".into(),
        other => other.to_string(),
    }
}

async fn translate_volc(req: &TranslateRequest) -> Result<TranslateResponse> {
    let access_key = request_secret(req, "accessKeyId").unwrap_or("");
    let secret_key = request_secret(req, "accessKeySecret").unwrap_or("");

    if access_key.is_empty() || secret_key.is_empty() {
        return Err(FinalSubError::Validation(
            "火山翻译缺少 accessKeyId 或 accessKeySecret".into(),
        ));
    }

    let now = chrono::Utc::now();
    let timestamp_iso = now.format("%Y%m%dT%H%M%SZ").to_string();
    let date = now.format("%Y%m%d").to_string();

    let payload = serde_json::json!({
        "TargetLanguage": map_lang_volc(&req.target_language),
        "SourceLanguage": map_lang_volc(&req.source_language),
        "TextList": [req.text]
    });
    let payload_str = payload.to_string();
    let hashed_payload = sha256_hex(payload_str.as_bytes());

    let canonical_req = format!(
        "POST\n/\nAction=TranslateText&Version=2020-06-01\ncontent-type:application/json\nhost:open.volcengineapi.com\nx-content-sha256:{}\nx-date:{}\n\ncontent-type;host;x-content-sha256;x-date\n{}",
        hashed_payload, timestamp_iso, hashed_payload
    );
    let hashed_canonical_req = sha256_hex(canonical_req.as_bytes());

    let credential_scope = format!("{}/cn-north-1/translate/request", date);
    let string_to_sign = format!(
        "HMAC-SHA256\n{}\n{}\n{}",
        timestamp_iso, credential_scope, hashed_canonical_req
    );

    let k_date = hmac_sha256(secret_key.as_bytes(), date.as_bytes());
    let k_region = hmac_sha256(&k_date, b"cn-north-1");
    let k_service = hmac_sha256(&k_region, b"translate");
    let k_signing = hmac_sha256(&k_service, b"request");
    let signature = hex::encode(hmac_sha256(&k_signing, string_to_sign.as_bytes()));

    let authorization = format!(
        "HMAC-SHA256 Credential={}/{}, SignedHeaders=content-type;host;x-content-sha256;x-date, Signature={}",
        access_key, credential_scope, signature
    );

    let client = translation_http_client(req)?;
    let url = "https://open.volcengineapi.com/?Action=TranslateText&Version=2020-06-01";
    let resp = client
        .post(url)
        .header("Authorization", authorization)
        .header("Content-Type", "application/json")
        .header("Host", "open.volcengineapi.com")
        .header("X-Content-Sha256", hashed_payload)
        .header("X-Date", timestamp_iso)
        .body(payload_str)
        .send()
        .await
        .map_err(|e| {
            FinalSubError::Validation(format!("火山翻译请求失败: {}", describe_reqwest_error(&e)))
        })?;

    if !resp.status().is_success() {
        return Err(FinalSubError::Validation(format!(
            "火山翻译返回错误: {}",
            resp.status()
        )));
    }

    let res_json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| FinalSubError::Validation(format!("火山翻译解析 JSON 失败: {e}")))?;

    if let Some(err) = res_json["ResponseMetadata"]["Error"].as_object() {
        let code = err
            .get("Code")
            .and_then(|v| v.as_str())
            .unwrap_or("Unknown");
        let msg = err
            .get("Message")
            .and_then(|v| v.as_str())
            .unwrap_or("Unknown");
        return Err(FinalSubError::Validation(format!(
            "火山翻译 API 报错 [{code}]: {msg}"
        )));
    }

    let translated = res_json["Response"]["TranslationList"][0]["Translation"]
        .as_str()
        .ok_or_else(|| FinalSubError::Validation("火山翻译返回格式异常".into()))?
        .to_string();

    Ok(TranslateResponse {
        translated_text: translated,
        provider: "volc".into(),
        success: true,
        error: None,
        thinking_enabled: None,
    })
}

fn map_lang_volc(lang: &str) -> String {
    match lang.to_lowercase().as_str() {
        "zh" | "zh-cn" | "zh-hans" => "zh".into(),
        "zh-hant" | "zh-tw" | "zh-hk" => "zh-Hant".into(),
        "en" => "en".into(),
        "ja" | "jp" => "ja".into(),
        "ko" | "kor" => "ko".into(),
        "fr" => "fr".into(),
        "es" => "es".into(),
        "ru" => "ru".into(),
        "auto" => "auto".into(),
        other => other.to_string(),
    }
}

async fn translate_xunfei(req: &TranslateRequest) -> Result<TranslateResponse> {
    let app_id = request_secret(req, "appId").unwrap_or("");
    let api_key = request_secret(req, "apiKey").unwrap_or("");
    let api_secret = request_secret(req, "apiSecret").unwrap_or("");

    if app_id.is_empty() || api_key.is_empty() || api_secret.is_empty() {
        return Err(FinalSubError::Validation(
            "讯飞翻译缺少 appId, apiKey 或 apiSecret".into(),
        ));
    }

    let date = chrono::Utc::now()
        .format("%a, %d %b %Y %H:%M:%S GMT")
        .to_string();

    let from = map_lang_xunfei(&req.source_language);
    let to = map_lang_xunfei(&req.target_language);

    let payload = serde_json::json!({
        "common": {
            "app_id": app_id
        },
        "business": {
            "from": from,
            "to": to
        },
        "data": {
            "text": base64_encode(&req.text)
        }
    });

    let body_str = payload.to_string();
    let body_sha256 = sha256_base64(body_str.as_bytes());
    let digest = format!("SHA-256={}", body_sha256);

    let signature_origin = format!(
        "host: itrans.xfyun.cn\ndate: {}\nPOST /v2/its HTTP/1.1\ndigest: {}",
        date, digest
    );

    let signature_sha = hmac_sha256(api_secret.as_bytes(), signature_origin.as_bytes());
    let signature = base64_encode_bytes(&signature_sha);

    let authorization = format!(
        "api_key=\"{}\", algorithm=\"hmac-sha256\", headers=\"host date request-line digest\", signature=\"{}\"",
        api_key, signature
    );

    let client = translation_http_client(req)?;
    let resp = client
        .post("https://itrans.xfyun.cn/v2/its")
        .header("Content-Type", "application/json")
        .header("Accept", "application/json,version=1.0")
        .header("Host", "itrans.xfyun.cn")
        .header("Date", date)
        .header("Digest", digest)
        .header("Authorization", authorization)
        .body(body_str)
        .send()
        .await
        .map_err(|e| {
            FinalSubError::Validation(format!("讯飞翻译请求失败: {}", describe_reqwest_error(&e)))
        })?;

    if !resp.status().is_success() {
        return Err(FinalSubError::Validation(format!(
            "讯飞翻译返回错误: {}",
            resp.status()
        )));
    }

    let res_json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| FinalSubError::Validation(format!("讯飞翻译解析 JSON 失败: {e}")))?;

    let code = res_json["code"].as_i64().unwrap_or(-1);
    if code != 0 {
        let msg = res_json["message"].as_str().unwrap_or("未知讯飞翻译错误");
        return Err(FinalSubError::Validation(format!(
            "讯飞翻译 API 报错 [{code}]: {msg}"
        )));
    }

    let dst_base64 = res_json["data"]["result"]["trans_result"]["dst"]
        .as_str()
        .ok_or_else(|| FinalSubError::Validation("讯飞翻译返回数据格式不正确".into()))?;

    let dst_bytes = base64_decode(dst_base64)
        .map_err(|e| FinalSubError::Validation(format!("讯飞翻译 Base64 解码失败: {e}")))?;

    let translated = String::from_utf8(dst_bytes)
        .map_err(|e| FinalSubError::Validation(format!("讯飞翻译 UTF8 转换失败: {e}")))?;

    Ok(TranslateResponse {
        translated_text: translated,
        provider: "xunfei".into(),
        success: true,
        error: None,
        thinking_enabled: None,
    })
}

fn base64_encode(s: &str) -> String {
    base64_encode_bytes(s.as_bytes())
}

fn map_lang_xunfei(lang: &str) -> String {
    match lang.to_lowercase().as_str() {
        "zh" | "zh-cn" | "zh-hans" => "cn".into(),
        "zh-hant" | "zh-tw" | "zh-hk" => "cn".into(),
        "en" => "en".into(),
        "ja" | "jp" => "ja".into(),
        "ko" | "kor" => "ko".into(),
        "fr" => "fr".into(),
        "es" => "es".into(),
        "ru" => "ru".into(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::thread;

    fn read_http_request(stream: &mut TcpStream) -> String {
        let mut bytes = Vec::new();
        let mut buffer = [0_u8; 4096];
        loop {
            let read = stream.read(&mut buffer).unwrap();
            if read == 0 {
                break;
            }
            bytes.extend_from_slice(&buffer[..read]);
            let Some(header_end) = bytes.windows(4).position(|window| window == b"\r\n\r\n") else {
                continue;
            };
            let headers = String::from_utf8_lossy(&bytes[..header_end]);
            let content_length = headers
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().ok())
                        .flatten()
                })
                .unwrap_or(0);
            if bytes.len() >= header_end + 4 + content_length {
                break;
            }
        }
        String::from_utf8(bytes).unwrap()
    }

    fn base_test_request() -> TranslateRequest {
        TranslateRequest {
            text: "Hello world".into(),
            source_language: "en".into(),
            target_language: "zh".into(),
            provider: "custom-openai".into(),
            api_key: Some("bound-secret".into()),
            api_url: Some("https://gateway.example.com/v1".into()),
            model_name: Some("model-a".into()),
            secret_fields: Some(std::collections::HashMap::from([
                ("apiKey".into(), "bound-secret".into()),
                ("tenantId".into(), "tenant-42".into()),
            ])),
            system_prompt: None,
            user_prompt: None,
            proxy_url: None,
            custom_headers: None,
            custom_body: None,
            structured_output: None,
            response_json_schema: None,
            glossary_prompt: None,
            enable_thinking: None,
            thinking_control_bypassed: false,
        }
    }

    #[test]
    fn builtin_providers_count() {
        assert_eq!(builtin_providers().len(), 18);
    }

    #[test]
    fn translation_config_default() {
        let config = TranslationConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.target_language, "zh");
    }

    #[test]
    fn providers_have_ids() {
        for p in builtin_providers() {
            assert!(!p.id.is_empty());
            assert!(!p.name.is_empty());
        }
    }

    #[test]
    fn implemented_providers_match_dispatch_table() {
        let source = include_str!("mod.rs");
        let dispatch_start = source
            .find("match req.provider.as_str() {")
            .expect("translate_text dispatch match should exist");
        let dispatch_section = &source[dispatch_start..];
        let fallback_start = dispatch_section
            .find("_ => Err")
            .expect("translate_text dispatch fallback should exist");
        let dispatch_section = &dispatch_section[..fallback_start];

        for provider in builtin_providers()
            .into_iter()
            .filter(|provider| provider.implemented)
        {
            let expected_arm = format!("\"{}\" =>", provider.id);
            assert!(
                dispatch_section.contains(&expected_arm),
                "implemented provider `{}` is missing a translate_text dispatch arm",
                provider.id
            );
        }
    }

    #[test]
    fn local_providers_do_not_require_api_key() {
        assert!(!provider_info("ollama").unwrap().requires_api_key);
        assert!(!provider_info("deeplx").unwrap().requires_api_key);
        assert!(provider_info("google").unwrap().requires_api_key);
    }

    #[test]
    fn request_api_key_uses_secret_fields_fallback() {
        let req = TranslateRequest {
            text: "Hello".into(),
            source_language: "en".into(),
            target_language: "zh".into(),
            provider: "custom-openai".into(),
            api_key: None,
            api_url: None,
            model_name: None,
            secret_fields: Some(std::collections::HashMap::from([(
                "apiKey".to_string(),
                "stored-key".to_string(),
            )])),
            system_prompt: None,
            user_prompt: None,
            proxy_url: None,
            custom_headers: None,
            custom_body: None,
            ..TranslateRequest::default()
        };

        assert_eq!(request_api_key(&req), Some("stored-key"));
        assert!(has_any_secret_field(&req));
    }

    #[test]
    fn validation_message_does_not_double_wrap_validation_errors() {
        let msg = validation_message(FinalSubError::Validation(
            "自定义 OpenAI 兼容 请求失败：error sending request".into(),
        ));

        assert_eq!(msg, "自定义 OpenAI 兼容 请求失败：error sending request");
    }

    #[tokio::test]
    async fn translate_text_rejects_missing_required_model_before_network() {
        let req = TranslateRequest {
            text: "Hello".into(),
            source_language: "en".into(),
            target_language: "zh".into(),
            provider: "deepseek".into(),
            api_key: Some("test-key".into()),
            api_url: Some("https://api.deepseek.com/v1".into()),
            model_name: None,
            secret_fields: Some(std::collections::HashMap::from([(
                "apiKey".to_string(),
                "test-key".to_string(),
            )])),
            system_prompt: None,
            user_prompt: None,
            proxy_url: None,
            custom_headers: None,
            custom_body: None,
            ..TranslateRequest::default()
        };

        let err = translate_text(&req).await.unwrap_err();
        assert!(err.to_string().contains("模型名称"));
    }

    #[test]
    fn aliyun_rpc_signature_uses_hmac_sha1_base64() {
        let signature = base64_encode_bytes(&hmac_sha1(b"Jefe", b"what do ya want for nothing?"));

        assert_eq!(signature, "7/zfauXrL6LSdBbV8YTfnCWafHk=");
    }

    #[test]
    fn openai_compatible_url_appends_chat_completions() {
        assert_eq!(
            openai_chat_completions_url("https://api.deepseek.com/v1"),
            "https://api.deepseek.com/v1/chat/completions"
        );
        assert_eq!(
            openai_chat_completions_url("https://api.deepseek.com/v1/chat/completions"),
            "https://api.deepseek.com/v1/chat/completions"
        );
    }

    #[test]
    fn prompt_templates_render_source_target_and_text_tokens() {
        let req = TranslateRequest {
            system_prompt: Some("Translate {source} -> {target}".into()),
            user_prompt: Some("Input:\n{text}".into()),
            ..base_test_request()
        };

        assert_eq!(translation_system_prompt(&req), "Translate en -> zh");
        assert_eq!(translation_user_prompt(&req), "Input:\nHello world");

        let req = TranslateRequest {
            user_prompt: Some("Keep subtitle timing.".into()),
            ..req
        };
        assert_eq!(
            translation_user_prompt(&req),
            "Keep subtitle timing.\n\nHello world"
        );
    }

    #[test]
    fn custom_headers_render_endpoint_bound_secret_placeholders() {
        let req = TranslateRequest {
            custom_headers: Some(std::collections::HashMap::from([
                ("Authorization".into(), "Bearer ${API_KEY}".into()),
                ("X-Tenant".into(), "{secret:tenantId}".into()),
            ])),
            ..base_test_request()
        };
        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        let request = apply_custom_headers(client.get("http://127.0.0.1"), &req)
            .unwrap()
            .build()
            .unwrap();

        assert_eq!(request.headers()["authorization"], "Bearer bound-secret");
        assert_eq!(request.headers()["x-tenant"], "tenant-42");

        let protected = TranslateRequest {
            custom_headers: Some(std::collections::HashMap::from([(
                "Content-Length".into(),
                "100".into(),
            )])),
            ..base_test_request()
        };
        assert!(apply_custom_headers(client.get("http://127.0.0.1"), &protected).is_err());
    }

    #[test]
    fn custom_body_deep_merges_typed_values_and_protects_core_fields() {
        let req = TranslateRequest {
            custom_body: Some(serde_json::Map::from_iter([
                ("temperature".into(), serde_json::json!(0.8)),
                (
                    "response_format".into(),
                    serde_json::json!({"type": "json_object"}),
                ),
                (
                    "metadata".into(),
                    serde_json::json!({"tenant": "{secret:tenantId}"}),
                ),
            ])),
            ..base_test_request()
        };
        let merged = merge_custom_body(
            serde_json::json!({
                "model": "model-a",
                "messages": [],
                "temperature": 0.3,
                "metadata": {"source": "FinalSub"}
            }),
            &req,
            &["model", "messages"],
        )
        .unwrap();

        assert_eq!(merged["temperature"], serde_json::json!(0.8));
        assert_eq!(merged["metadata"]["source"], "FinalSub");
        assert_eq!(merged["metadata"]["tenant"], "tenant-42");
        assert_eq!(merged["response_format"]["type"], "json_object");

        let protected = TranslateRequest {
            custom_body: Some(serde_json::Map::from_iter([(
                "messages".into(),
                serde_json::json!([]),
            )])),
            ..base_test_request()
        };
        assert!(merge_custom_body(
            serde_json::json!({"messages": []}),
            &protected,
            &["messages"]
        )
        .is_err());
    }

    #[test]
    fn provider_request_bodies_apply_dynamic_structured_output() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {"1": {"type": "string"}},
            "required": ["1"],
            "additionalProperties": false
        });
        let request = TranslateRequest {
            structured_output: Some("json_schema".into()),
            response_json_schema: Some(schema.clone()),
            ..base_test_request()
        };

        let openai = apply_openai_structured_output(serde_json::json!({}), &request).unwrap();
        assert_eq!(openai["response_format"]["type"], "json_schema");
        assert_eq!(openai["response_format"]["json_schema"]["schema"], schema);
        assert_eq!(openai["response_format"]["json_schema"]["strict"], true);

        let ollama = apply_ollama_structured_output(serde_json::json!({}), &request).unwrap();
        assert_eq!(ollama["format"]["required"], serde_json::json!(["1"]));

        let gemini = apply_gemini_structured_output(
            serde_json::json!({"generationConfig": {"temperature": 0.2}}),
            &request,
        )
        .unwrap();
        assert_eq!(
            gemini["generationConfig"]["responseMimeType"],
            "application/json"
        );
        assert_eq!(
            gemini["generationConfig"]["responseJsonSchema"]["required"],
            serde_json::json!(["1"])
        );
    }

    #[test]
    fn glossary_block_is_appended_after_rendering_custom_system_prompt() {
        let request = TranslateRequest {
            system_prompt: Some("Translate {source} to {target}.".into()),
            glossary_prompt: Some("# Terminology\n[]".into()),
            ..base_test_request()
        };

        assert_eq!(
            translation_system_prompt(&request),
            "Translate en to zh.\n\n# Terminology\n[]"
        );
    }

    #[test]
    fn thinking_control_maps_known_providers_and_stays_conservative() {
        let request = |provider: &str, endpoint: &str, model: &str| TranslateRequest {
            provider: provider.into(),
            api_url: Some(endpoint.into()),
            model_name: Some(model.into()),
            enable_thinking: Some(false),
            ..base_test_request()
        };

        assert_eq!(
            resolve_thinking_params(&request("qwen", "https://example.com/v1", "qwen-plus")),
            Some(serde_json::json!({"enable_thinking": false}))
        );
        assert_eq!(
            resolve_thinking_params(&request(
                "custom-openai",
                "https://api.siliconflow.cn/v1",
                "Qwen/Qwen3-8B"
            )),
            Some(serde_json::json!({"enable_thinking": false}))
        );
        assert_eq!(
            resolve_thinking_params(&request(
                "custom-openai",
                "https://ark.cn-beijing.volces.com/api/v3",
                "doubao-seed"
            )),
            Some(serde_json::json!({"thinking": {"type": "disabled"}}))
        );
        assert_eq!(
            resolve_thinking_params(&request("ollama", "http://localhost:11434", "qwen3:8b")),
            Some(serde_json::json!({"think": false}))
        );
        assert_eq!(
            resolve_thinking_params(&request(
                "gemini",
                "https://generativelanguage.googleapis.com",
                "gemini-2.5-flash"
            )),
            Some(serde_json::json!({"reasoning_effort": "none"}))
        );
        assert_eq!(
            resolve_thinking_params(&request(
                "custom-openai",
                "https://api.openai.com/v1",
                "gpt-5-mini"
            )),
            Some(serde_json::json!({"reasoning_effort": "minimal"}))
        );
        assert_eq!(
            resolve_thinking_params(&request(
                "azureopenai",
                "https://example.openai.azure.com",
                "o3-mini"
            )),
            Some(serde_json::json!({"reasoning_effort": "low"}))
        );
        assert!(resolve_thinking_params(&request(
            "custom-openai",
            "https://gateway.example.com/v1",
            "gpt-4o"
        ))
        .is_none());
        assert!(resolve_thinking_params(&request(
            "deepseek",
            "https://api.deepseek.com/v1",
            "deepseek-chat"
        ))
        .is_none());
        assert!(resolve_thinking_params(&request(
            "qwen",
            "https://example.com/v1",
            "qwen3-235b-thinking-2507"
        ))
        .is_none());

        let enabled = TranslateRequest {
            enable_thinking: Some(true),
            ..request("qwen", "https://example.com/v1", "qwen-plus")
        };
        assert!(resolve_thinking_params(&enabled).is_none());
    }

    #[test]
    fn custom_thinking_parameter_overrides_the_switch() {
        let request = TranslateRequest {
            provider: "qwen".into(),
            enable_thinking: Some(false),
            custom_body: Some(serde_json::Map::from_iter([(
                "enable_thinking".into(),
                serde_json::json!(true),
            )])),
            ..base_test_request()
        };
        let body =
            merge_custom_body(serde_json::json!({"model": "qwen-plus"}), &request, &[]).unwrap();
        let body = apply_thinking_control(body, &request);

        assert_eq!(body["enable_thinking"], true);
        assert!(!can_retry_without_thinking_params(&request));
    }

    #[test]
    fn qwen3_soft_switch_is_only_used_when_parameter_control_is_unavailable() {
        let unknown = TranslateRequest {
            provider: "custom-openai".into(),
            api_url: Some("https://gateway.example.com/v1".into()),
            model_name: Some("qwen3:8b".into()),
            enable_thinking: Some(false),
            ..base_test_request()
        };
        assert!(translation_system_prompt(&unknown).ends_with("/no_think"));

        let mapped = TranslateRequest {
            provider: "qwen".into(),
            ..unknown.clone()
        };
        assert!(!translation_system_prompt(&mapped).contains("/no_think"));

        mark_thinking_param_rejected(&mapped);
        assert!(translation_system_prompt(&mapped).ends_with("/no_think"));
        clear_thinking_param_rejection(&mapped);
    }

    #[test]
    fn thinking_response_metadata_is_detected_without_exposing_token_counts() {
        assert!(detect_openai_thinking(&serde_json::json!({
            "choices": [{"message": {"content": "译文", "reasoning_content": "推理"}}],
            "usage": {"completion_tokens_details": {"reasoning_tokens": 0}}
        })));
        assert!(detect_openai_thinking(&serde_json::json!({
            "choices": [{"message": {"content": "译文"}}],
            "usage": {"completion_tokens_details": {"reasoning_tokens": 12}}
        })));
        assert!(!detect_openai_thinking(&serde_json::json!({
            "choices": [{"message": {"content": "译文"}}]
        })));
        assert!(detect_ollama_thinking(
            &serde_json::json!({"thinking": "step"})
        ));
        assert!(detect_gemini_thinking(&serde_json::json!({
            "candidates": [{"content": {"parts": [{"thought": true, "text": "step"}]}}]
        })));
    }

    #[tokio::test]
    async fn rejected_thinking_parameter_is_removed_retried_and_cached() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            for attempt in 0..3 {
                let (mut stream, _) = listener.accept().unwrap();
                let request = read_http_request(&mut stream);
                let body = request.split_once("\r\n\r\n").unwrap().1;
                let json: serde_json::Value = serde_json::from_str(body).unwrap();
                if attempt == 0 {
                    assert_eq!(json["enable_thinking"], false);
                    let response = r#"{"error":{"message":"unknown field enable_thinking"}}"#;
                    write!(
                        stream,
                        "HTTP/1.1 400 Bad Request\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        response.len(),
                        response
                    )
                    .unwrap();
                } else {
                    assert!(json.get("enable_thinking").is_none());
                    let response = r#"{"choices":[{"message":{"content":"你好"}}]}"#;
                    write!(
                        stream,
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        response.len(),
                        response
                    )
                    .unwrap();
                }
            }
        });

        let request = TranslateRequest {
            provider: "qwen".into(),
            api_url: Some(format!("http://{endpoint}/v1")),
            model_name: Some("qwen-plus".into()),
            enable_thinking: Some(false),
            ..base_test_request()
        };
        clear_thinking_param_rejection(&request);

        let first = translate_text(&request).await.unwrap();
        let second = translate_text(&request).await.unwrap();
        assert_eq!(first.translated_text, "你好");
        assert_eq!(second.translated_text, "你好");
        assert!(has_thinking_param_rejection(&request));

        clear_thinking_param_rejection(&request);
        server.join().unwrap();
    }

    #[tokio::test]
    async fn unsupported_json_schema_falls_back_to_json_object() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            for attempt in 0..2 {
                let (mut stream, _) = listener.accept().unwrap();
                let request = read_http_request(&mut stream);
                let body = request.split_once("\r\n\r\n").unwrap().1;
                let json: serde_json::Value = serde_json::from_str(body).unwrap();
                if attempt == 0 {
                    assert_eq!(json["response_format"]["type"], "json_schema");
                    let response =
                        r#"{"error":{"message":"response_format json_schema unsupported"}}"#;
                    write!(
                        stream,
                        "HTTP/1.1 400 Bad Request\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        response.len(),
                        response
                    )
                    .unwrap();
                } else {
                    assert_eq!(json["response_format"]["type"], "json_object");
                    let response = r#"{"choices":[{"message":{"content":"{\"1\":\"你好\"}"}}]}"#;
                    write!(
                        stream,
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        response.len(),
                        response
                    )
                    .unwrap();
                }
            }
        });

        let request = TranslateRequest {
            api_url: Some(format!("http://{endpoint}/v1")),
            structured_output: Some("json_schema".into()),
            response_json_schema: Some(serde_json::json!({
                "type": "object",
                "properties": {"1": {"type": "string"}},
                "required": ["1"],
                "additionalProperties": false
            })),
            ..base_test_request()
        };
        let response = translate_text(&request).await.unwrap();

        server.join().unwrap();
        assert_eq!(response.translated_text, "{\"1\":\"你好\"}");
    }

    #[tokio::test]
    async fn unsupported_structured_modes_fall_back_to_plain_text() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            for attempt in 0..3 {
                let (mut stream, _) = listener.accept().unwrap();
                let request = read_http_request(&mut stream);
                let body = request.split_once("\r\n\r\n").unwrap().1;
                let json: serde_json::Value = serde_json::from_str(body).unwrap();
                if attempt < 2 {
                    assert_eq!(
                        json["response_format"]["type"],
                        if attempt == 0 {
                            "json_schema"
                        } else {
                            "json_object"
                        }
                    );
                    let response = r#"{"error":{"message":"response_format is not supported"}}"#;
                    write!(
                        stream,
                        "HTTP/1.1 400 Bad Request\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        response.len(),
                        response
                    )
                    .unwrap();
                } else {
                    assert!(json.get("response_format").is_none());
                    let response = r#"{"choices":[{"message":{"content":"{\"1\":\"你好\"}"}}]}"#;
                    write!(
                        stream,
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        response.len(),
                        response
                    )
                    .unwrap();
                }
            }
        });

        let request = TranslateRequest {
            api_url: Some(format!("http://{endpoint}/v1")),
            structured_output: Some("json_schema".into()),
            response_json_schema: Some(serde_json::json!({
                "type": "object",
                "properties": {"1": {"type": "string"}},
                "required": ["1"],
                "additionalProperties": false
            })),
            ..base_test_request()
        };
        let response = translate_text(&request).await.unwrap();

        server.join().unwrap();
        assert_eq!(response.translated_text, "{\"1\":\"你好\"}");
    }

    #[tokio::test]
    async fn proxy_probe_uses_the_configured_http_proxy() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let listener_endpoint = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0_u8; 4096];
            let read = stream.read(&mut request).unwrap();
            let request = String::from_utf8_lossy(&request[..read]);
            assert!(request.starts_with("GET http://example.invalid/probe HTTP/1.1"));
            stream
                .write_all(b"HTTP/1.1 204 No Content\r\nConnection: close\r\n\r\n")
                .unwrap();
        });

        let status = test_proxy_connection(
            &format!("http://{listener_endpoint}"),
            "http://example.invalid/probe",
        )
        .await
        .unwrap();

        server.join().unwrap();
        assert_eq!(status, "HTTP 204");
    }

    #[tokio::test]
    async fn custom_openai_requires_endpoint_and_model() {
        let req = TranslateRequest {
            text: "Hello".into(),
            source_language: "en".into(),
            target_language: "zh".into(),
            provider: "custom-openai".into(),
            api_key: Some("test-key".into()),
            api_url: None,
            model_name: None,
            secret_fields: None,
            system_prompt: None,
            user_prompt: None,
            proxy_url: None,
            custom_headers: None,
            custom_body: None,
            ..TranslateRequest::default()
        };

        let err = translate_custom_openai_compatible(&req).await.unwrap_err();
        assert!(err.to_string().contains("端点 URL"));

        let req = TranslateRequest {
            api_url: Some("https://gateway.example.com/v1".into()),
            ..req
        };
        let err = translate_custom_openai_compatible(&req).await.unwrap_err();
        assert!(err.to_string().contains("模型名称"));
    }

    #[test]
    fn gemini_url_builds_generate_content_endpoint() {
        assert_eq!(
            gemini_generate_content_url("https://generativelanguage.googleapis.com", "gemini-2.5-flash"),
            "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-flash:generateContent"
        );
        assert_eq!(
            gemini_generate_content_url("https://generativelanguage.googleapis.com/v1beta", "models/gemini-2.5-flash"),
            "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-flash:generateContent"
        );
    }
}
