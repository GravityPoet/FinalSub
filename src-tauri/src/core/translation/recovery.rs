use super::{builtin_providers, TranslateRequest};
use crate::core::settings::Settings;
use std::collections::HashMap;

/// Build each request from that provider's configuration; credentials and custom
/// headers from the primary service must never be inherited by a backup.
pub fn configured_request(settings: &Settings, id: &str) -> Result<TranslateRequest, String> {
    let info = builtin_providers()
        .into_iter()
        .find(|provider| provider.id == id)
        .ok_or("未知翻译服务")?;
    let endpoint = settings
        .translate_endpoints
        .get(if id == "auto-free" { "deeplx" } else { id })
        .filter(|value| !value.trim().is_empty())
        .cloned()
        .or_else(|| (!info.default_endpoint.is_empty()).then_some(info.default_endpoint));
    let mut secrets = HashMap::new();
    for field in &info.secret_fields {
        if let Some(value) = crate::core::secrets::get_provider_secret(
            id,
            endpoint.as_deref().unwrap_or_default(),
            field,
        )? {
            secrets.insert(field.clone(), value);
        }
    }
    if info.requires_api_key
        && info
            .secret_fields
            .iter()
            .any(|field| !secrets.contains_key(field))
    {
        return Err(format!("{id} 尚未配置必要凭据"));
    }
    let model = settings
        .translate_models
        .get(id)
        .filter(|model| !model.trim().is_empty())
        .cloned();
    if info.requires_model && model.is_none() {
        return Err(format!("{id} 尚未选择模型"));
    }
    if info.requires_endpoint && endpoint.is_none() {
        return Err(format!("{id} 尚未配置地址"));
    }
    Ok(TranslateRequest {
        provider: id.into(),
        api_key: secrets.get("apiKey").cloned(),
        api_url: endpoint,
        model_name: model,
        secret_fields: Some(secrets),
        system_prompt: settings.translate_system_prompts.get(id).cloned(),
        user_prompt: settings.translate_user_prompts.get(id).cloned(),
        proxy_url: settings.proxy_enabled.then(|| settings.proxy_url.clone()),
        custom_headers: settings.translate_custom_headers.get(id).cloned(),
        custom_body: settings.translate_custom_body.get(id).cloned(),
        enable_thinking: Some(
            settings
                .translate_enable_thinking
                .get(id)
                .copied()
                .unwrap_or(false),
        ),
        ..Default::default()
    })
}

pub fn backup_requests(settings: &Settings, primary: &str) -> Vec<TranslateRequest> {
    let catalog = builtin_providers();
    let is_ai = catalog
        .iter()
        .find(|provider| provider.id == primary)
        .map(|provider| provider.is_ai);
    let mut seen = std::collections::HashSet::from([primary.to_string()]);
    settings
        .translate_fallback_providers
        .iter()
        .take(3)
        .filter(|id| seen.insert((*id).clone()))
        .filter(|id| {
            catalog
                .iter()
                .any(|provider| provider.id == **id && Some(provider.is_ai) == is_ai)
        })
        .filter_map(|id| configured_request(settings, id).ok())
        .collect()
}

pub fn inherit_content(
    template: &TranslateRequest,
    original: &TranslateRequest,
) -> TranslateRequest {
    TranslateRequest {
        text: original.text.clone(),
        source_language: original.source_language.clone(),
        target_language: original.target_language.clone(),
        structured_output: original.structured_output.clone(),
        response_json_schema: original.response_json_schema.clone(),
        glossary_prompt: original.glossary_prompt.clone(),
        ..template.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn backup_never_inherits_primary_identity_or_headers() {
        let primary = TranslateRequest {
            text: "Hello".into(),
            api_key: Some("primary-fixture".into()),
            api_url: Some("https://primary.invalid".into()),
            custom_headers: Some(HashMap::from([("X-Primary".into(), "fixture".into())])),
            ..Default::default()
        };
        let backup = TranslateRequest {
            provider: "backup".into(),
            api_url: Some("https://backup.invalid".into()),
            ..Default::default()
        };
        let request = inherit_content(&backup, &primary);
        assert_eq!(request.text, "Hello");
        assert_eq!(request.api_url, backup.api_url);
        assert!(request.api_key.is_none());
        assert!(request.custom_headers.is_none());
    }
}
