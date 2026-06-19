use serde::{Deserialize, Serialize};

use crate::error::{FinalSubError, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranslationProvider {
    pub id: String,
    pub name: String,
    pub provider_type: String,
    pub is_ai: bool,
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

pub fn builtin_providers() -> Vec<TranslationProvider> {
    vec![
        TranslationProvider {
            id: "baidu".into(),
            name: "百度翻译".into(),
            provider_type: "api".into(),
            is_ai: false,
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
            requires_api_key: true,
            requires_endpoint: false,
            requires_model: false,
            secret_fields: vec!["secretId".into(), "secretKey".into()],
            default_endpoint: "".into(),
        },
        TranslationProvider {
            id: "xunfei".into(),
            name: "讯飞翻译".into(),
            provider_type: "api".into(),
            is_ai: false,
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
            requires_api_key: true,
            requires_endpoint: true,
            requires_model: false,
            secret_fields: vec!["apiKey".into()],
            default_endpoint: "https://api.cognitive.microsofttranslator.com".into(),
        },
        TranslationProvider {
            id: "ollama".into(),
            name: "Ollama".into(),
            provider_type: "ai".into(),
            is_ai: true,
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
            requires_api_key: true,
            requires_endpoint: true,
            requires_model: true,
            secret_fields: vec!["apiKey".into()],
            default_endpoint: "".into(),
        },
        TranslationProvider {
            id: "deerapi".into(),
            name: "DeerAPI".into(),
            provider_type: "ai".into(),
            is_ai: true,
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
            requires_api_key: true,
            requires_endpoint: true,
            requires_model: true,
            secret_fields: vec!["apiKey".into()],
            default_endpoint: "https://dashscope.aliyuncs.com/compatible-mode/v1".into(),
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranslateResponse {
    pub translated_text: String,
    pub provider: String,
    pub success: bool,
    pub error: Option<String>,
}

pub async fn translate_text(req: &TranslateRequest) -> Result<TranslateResponse> {
    if provider_requires_api_key(&req.provider) && req.api_key.as_deref().unwrap_or("").is_empty() {
        return Err(FinalSubError::Validation(format!(
            "翻译 provider '{}' 需要 API Key，请在设置中配置",
            req.provider
        )));
    }

    match req.provider.as_str() {
        "baidu" => translate_baidu(req).await,
        "google" => translate_google(req).await,
        "deeplx" => translate_deeplx(req).await,
        "ollama" => translate_ollama(req).await,
        "deepseek" => translate_openai_compatible(req, "DeepSeek").await,
        "gemini" => translate_openai_compatible(req, "Gemini").await,
        "siliconflow" => translate_openai_compatible(req, "SiliconFlow").await,
        "qwen" => translate_openai_compatible(req, "Qwen").await,
        _ => Err(FinalSubError::Validation(format!(
            "翻译 provider '{}' 暂未接入，敬请期待",
            req.provider
        ))),
    }
}

fn provider_requires_api_key(provider: &str) -> bool {
    !matches!(provider, "ollama" | "deeplx")
}

async fn translate_baidu(_req: &TranslateRequest) -> Result<TranslateResponse> {
    Err(FinalSubError::Validation(
        "百度翻译 API 暂未实现，请使用其他 provider".into(),
    ))
}

async fn translate_google(_req: &TranslateRequest) -> Result<TranslateResponse> {
    Err(FinalSubError::Validation(
        "谷歌翻译 API 暂未实现，请使用其他 provider".into(),
    ))
}

async fn translate_deeplx(req: &TranslateRequest) -> Result<TranslateResponse> {
    let api_url = req
        .api_url
        .as_deref()
        .unwrap_or("http://localhost:1188/translate");
    let client = reqwest::Client::new();
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
        .map_err(|e| FinalSubError::Validation(format!("DeepLX 请求失败：{e}")))?;

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
    let api_url = req
        .api_url
        .as_deref()
        .unwrap_or("http://localhost:11434/api/generate");
    let model = req.model_name.as_deref().unwrap_or("qwen2.5:7b");

    let prompt = format!(
        "Translate the following text from {} to {}. Only output the translation, nothing else.\n\n{}",
        req.source_language, req.target_language, req.text
    );

    let client = reqwest::Client::new();
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
        .map_err(|e| FinalSubError::Validation(format!("Ollama 请求失败：{e}")))?;

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
    let api_url = req
        .api_url
        .as_deref()
        .unwrap_or("https://api.openai.com/v1/chat/completions");
    let api_key = req.api_key.as_deref().unwrap_or("");
    let model = req.model_name.as_deref().unwrap_or("gpt-4o-mini");

    let system_prompt = format!(
        "You are a professional translator. Translate subtitles from {} to {}. \
         Only output the translated text, preserving line breaks and timing. \
         Do not add explanations.",
        req.source_language, req.target_language
    );

    let client = reqwest::Client::new();
    let body = serde_json::json!({
        "model": model,
        "messages": [
            {"role": "system", "content": system_prompt},
            {"role": "user", "content": &req.text}
        ],
        "temperature": 0.3,
    });

    let resp = client
        .post(api_url)
        .header("Authorization", format!("Bearer {api_key}"))
        .header("Content-Type", "application/json")
        .json(&body)
        .timeout(std::time::Duration::from_secs(120))
        .send()
        .await
        .map_err(|e| FinalSubError::Validation(format!("{provider_name} 请求失败：{e}")))?;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_providers_count() {
        assert_eq!(builtin_providers().len(), 17);
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
    fn local_providers_do_not_require_api_key() {
        assert!(!provider_requires_api_key("ollama"));
        assert!(!provider_requires_api_key("deeplx"));
        assert!(provider_requires_api_key("google"));
    }
}
