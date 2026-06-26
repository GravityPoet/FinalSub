use serde::{Deserialize, Serialize};
use std::error::Error as StdError;

use crate::error::{FinalSubError, Result};

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

fn translation_http_client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent("FinalSub/1.0")
        .build()
        .map_err(|e| {
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
            provider_type: "api".into(),
            is_ai: false,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranslateRequest {
    pub text: String,
    pub source_language: String,
    pub target_language: String,
    pub provider: String,
    pub api_key: Option<String>,
    pub api_url: Option<String>,
    pub model_name: Option<String>,
    pub secret_fields: Option<std::collections::HashMap<String, String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranslateResponse {
    pub translated_text: String,
    pub provider: String,
    pub success: bool,
    pub error: Option<String>,
}

pub async fn translate_text(req: &TranslateRequest) -> Result<TranslateResponse> {
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

    let client = translation_http_client()?;
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
    })
}

async fn translate_google(req: &TranslateRequest) -> Result<TranslateResponse> {
    let api_key = request_api_key(req).unwrap_or("");
    if api_key.is_empty() {
        return Err(FinalSubError::Validation("谷歌翻译缺少 API Key".into()));
    }
    let client = translation_http_client()?;
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
        let err_body = resp.text().await.unwrap_or_default();
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
    })
}

async fn translate_deeplx(req: &TranslateRequest) -> Result<TranslateResponse> {
    let api_url =
        request_endpoint(req, "deeplx").unwrap_or_else(|| "http://localhost:1188/translate".into());
    let client = translation_http_client()?;
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
    })
}

async fn translate_ollama(req: &TranslateRequest) -> Result<TranslateResponse> {
    let api_url = request_endpoint(req, "ollama")
        .unwrap_or_else(|| "http://localhost:11434/api/generate".into());
    let model = request_model(req).unwrap_or("qwen2.5:7b");

    let prompt = format!(
        "Translate the following text from {} to {}. Only output the translation, nothing else.\n\n{}",
        req.source_language, req.target_language, req.text
    );

    let client = translation_http_client()?;
    let body = serde_json::json!({
        "model": model,
        "prompt": prompt,
        "stream": false,
    });

    let resp = client
        .post(api_url)
        .json(&body)
        .timeout(std::time::Duration::from_secs(120))
        .send()
        .await
        .map_err(|e| {
            FinalSubError::Validation(format!("Ollama 请求失败：{}", describe_reqwest_error(&e)))
        })?;

    if !resp.status().is_success() {
        return Err(FinalSubError::Validation(format!(
            "Ollama 返回错误：{}",
            resp.status()
        )));
    }

    let data: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| FinalSubError::Validation(format!("Ollama 响应解析失败：{e}")))?;

    let translated = data["response"].as_str().unwrap_or("").trim().to_string();

    Ok(TranslateResponse {
        translated_text: translated,
        provider: "ollama".into(),
        success: true,
        error: None,
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

    let system_prompt = format!(
        "You are a professional translator. Translate subtitles from {} to {}. \
         Only output the translated text, preserving line breaks and timing. \
         Do not add explanations.",
        req.source_language, req.target_language
    );

    let client = translation_http_client()?;
    let body = serde_json::json!({
        "model": model,
        "messages": [
            {"role": "system", "content": system_prompt},
            {"role": "user", "content": &req.text}
        ],
        "temperature": 0.3,
    });

    let resp = client
        .post(&api_url)
        .header("Authorization", format!("Bearer {api_key}"))
        .header("Content-Type", "application/json")
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
        let body_text = resp.text().await.unwrap_or_default();
        return Err(FinalSubError::Validation(format!(
            "{provider_name} 返回错误 {status}：{body_text}"
        )));
    }

    let data: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| FinalSubError::Validation(format!("{provider_name} 响应解析失败：{e}")))?;

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

    let system_prompt = format!(
        "You are a professional translator. Translate subtitles from {} to {}. \
         Only output the translated text. Preserve line breaks. Do not add explanations.",
        req.source_language, req.target_language
    );
    let user_prompt = format!(
        "Translate this subtitle text from {} to {}:\n\n{}",
        req.source_language, req.target_language, req.text
    );

    let client = translation_http_client()?;
    let body = serde_json::json!({
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
    });

    let resp = client
        .post(&api_url)
        .header("x-goog-api-key", api_key)
        .header("Content-Type", "application/json")
        .json(&body)
        .timeout(std::time::Duration::from_secs(120))
        .send()
        .await
        .map_err(|e| {
            FinalSubError::Validation(format!("Gemini 请求失败：{}", describe_reqwest_error(&e)))
        })?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body_text = resp.text().await.unwrap_or_default();
        return Err(FinalSubError::Validation(format!(
            "Gemini 返回错误 {status}：{body_text}"
        )));
    }

    let data: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| FinalSubError::Validation(format!("Gemini 响应解析失败：{e}")))?;

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

    let client = translation_http_client()?;
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
        let err_body = resp.text().await.unwrap_or_default();
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

    let system_prompt = format!(
        "You are a professional translator. Translate subtitles from {} to {}. \
         Only output the translated text, preserving line breaks and timing. \
         Do not add explanations.",
        req.source_language, req.target_language
    );

    let client = translation_http_client()?;
    let body = serde_json::json!({
        "messages": [
            {"role": "system", "content": system_prompt},
            {"role": "user", "content": &req.text}
        ],
        "temperature": 0.3,
    });

    let resp = client
        .post(&url)
        .header("api-key", api_key)
        .header("Content-Type", "application/json")
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
        let err_body = resp.text().await.unwrap_or_default();
        return Err(FinalSubError::Validation(format!(
            "Azure OpenAI 返回错误 {status}: {err_body}"
        )));
    }

    let res_json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| FinalSubError::Validation(format!("Azure OpenAI 响应解析失败: {e}")))?;

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
    })
}

async fn translate_niutrans(req: &TranslateRequest) -> Result<TranslateResponse> {
    let api_key = request_api_key(req).unwrap_or("");
    if api_key.is_empty() {
        return Err(FinalSubError::Validation("小牛翻译缺少 API Key".into()));
    }
    let client = translation_http_client()?;
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

    let client = translation_http_client()?;
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

    let client = translation_http_client()?;
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

    let client = translation_http_client()?;
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

    let client = translation_http_client()?;
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
