//! 火山引擎「声音复刻 2.0」的训练与状态协议。
//!
//! 合成接口使用 provider 的 API Key；声音复刻训练沿用火山当前兼容的
//! APP ID + Access Token 双凭据，并且只把凭据放在本地凭据存储。这里不
//! 保存参考音频，也不把服务端响应原文无限制地带回前端。

use base64::{engine::general_purpose::STANDARD, Engine as _};
use reqwest::{Client, Response};
use serde_json::{json, Value};
use std::path::Path;
use std::time::Duration;

use crate::error::{FinalSubError, Result};

pub const TRAIN_URL: &str = "https://openspeech.bytedance.com/api/v3/tts/voice_clone";
pub const STATUS_URL: &str = "https://openspeech.bytedance.com/api/v3/tts/get_voice";
pub const STATUS_V1_URL: &str = "https://openspeech.bytedance.com/api/v1/mega_tts/status";
pub const MAX_AUDIO_BYTES: u64 = 10 * 1024 * 1024;
const MAX_RESPONSE_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloneState {
    Training,
    Ready,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CloneStatus {
    pub state: CloneState,
    pub raw_status: Option<i64>,
    pub training_times_left: Option<u32>,
}

pub fn validate_speaker_id(raw: &str) -> Result<String> {
    let value = raw.trim();
    if value.is_empty()
        || value.len() > 200
        || value.chars().any(char::is_control)
        || !value.starts_with("S_")
    {
        return Err(FinalSubError::Validation(
            "豆包声音复刻音色 ID 必须以 S_ 开头且不能超过 200 字符".into(),
        ));
    }
    Ok(value.to_string())
}

pub fn build_train_body(
    speaker_id: &str,
    audio_base64: &str,
    language: &str,
    remove_background_noise: bool,
    enable_mss: bool,
) -> Value {
    let mut extra = serde_json::Map::new();
    if remove_background_noise {
        extra.insert("enable_audio_denoise".into(), Value::Bool(true));
    }
    if enable_mss {
        extra.insert("voice_clone_enable_mss".into(), Value::Bool(true));
    }
    let mut body = json!({
        "speaker_id": speaker_id,
        "audio": { "data": audio_base64, "format": "wav" },
        "language": if language.eq_ignore_ascii_case("en") { 1 } else { 0 },
        "model_types": [4],
    });
    if !extra.is_empty() {
        body["extra_params"] = Value::Object(extra);
    }
    body
}

fn status_number(value: Option<&Value>) -> Option<i64> {
    let value = value?;
    match value {
        Value::Number(number) => number
            .as_i64()
            .or_else(|| number.as_f64().map(|n| n as i64)),
        Value::String(text) => text.trim().parse::<i64>().ok().or_else(|| {
            match text.trim().to_ascii_lowercase().as_str() {
                "training" => Some(1),
                "success" | "ready" => Some(2),
                "failed" | "fail" => Some(3),
                "active" => Some(4),
                "notfound" | "unknown" => Some(0),
                _ => None,
            }
        }),
        _ => None,
    }
}

pub fn parse_status(payload: &Value) -> Option<CloneStatus> {
    let mut candidates = Vec::new();
    candidates.push(payload.get("status"));
    candidates.push(payload.get("state"));
    if let Some(data) = payload.get("data") {
        candidates.push(data.get("status"));
        candidates.push(data.get("state"));
    }
    if let Some(items) = payload.get("speaker_status").and_then(Value::as_array) {
        for item in items {
            candidates.push(item.get("status"));
            candidates.push(item.get("state"));
        }
    }
    let raw = candidates
        .into_iter()
        .flatten()
        .find_map(|value| status_number(Some(value)));
    let mut training_time_values = vec![payload.get("available_training_times")];
    if let Some(data) = payload.get("data") {
        training_time_values.push(data.get("available_training_times"));
    }
    if let Some(items) = payload.get("speaker_status").and_then(Value::as_array) {
        for item in items {
            training_time_values.push(item.get("available_training_times"));
        }
    }
    let training_times_left = training_time_values
        .into_iter()
        .flatten()
        .find_map(|value| match value {
            Value::Number(number) => number.as_u64(),
            Value::String(text) => text.trim().parse::<u64>().ok(),
            _ => None,
        })
        .and_then(|value| u32::try_from(value).ok());
    let state = match raw? {
        2 | 4 => CloneState::Ready,
        0 | 3 => CloneState::Failed,
        _ => CloneState::Training,
    };
    Some(CloneStatus {
        state,
        raw_status: raw,
        training_times_left,
    })
}

fn response_detail(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes)
        .chars()
        .filter(|character| !character.is_control() || matches!(*character, '\n' | '\t'))
        .take(1_000)
        .collect::<String>()
        .trim()
        .to_string()
}

async fn response_bytes(response: Response) -> Result<Vec<u8>> {
    if response
        .content_length()
        .is_some_and(|length| length > MAX_RESPONSE_BYTES as u64)
    {
        return Err(FinalSubError::Validation(
            "豆包声音复刻响应超过 64 KB 限制".into(),
        ));
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|error| FinalSubError::Validation(format!("读取豆包声音复刻响应失败：{error}")))?;
    if bytes.len() > MAX_RESPONSE_BYTES {
        return Err(FinalSubError::Validation(
            "豆包声音复刻响应超过 64 KB 限制".into(),
        ));
    }
    Ok(bytes.to_vec())
}

fn response_error(status: u16, payload: Option<&Value>, raw: &[u8]) -> FinalSubError {
    let code = payload
        .and_then(|value| {
            value
                .pointer("/BaseResp/StatusCode")
                .or_else(|| value.get("code"))
                .or_else(|| value.pointer("/header/code"))
        })
        .and_then(|value| status_number(Some(value)));
    let message = payload
        .and_then(|value| {
            value
                .pointer("/BaseResp/StatusMessage")
                .or_else(|| value.get("message"))
                .or_else(|| value.pointer("/header/message"))
        })
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| response_detail(raw));
    let hint = if status == 401 || status == 403 {
        "请检查 APP ID、Access Token 与声音复刻权限"
    } else if code == Some(1109) || message.to_ascii_lowercase().contains("not found") {
        "请确认已购买 S_ 音色槽位且音色 ID 未被删除"
    } else if message.to_ascii_lowercase().contains("limit")
        || message.to_ascii_lowercase().contains("quota")
    {
        "该音色槽位训练次数可能已用尽，请更换仍有次数的 S_ 音色"
    } else if message.to_ascii_lowercase().contains("quality")
        || message.to_ascii_lowercase().contains("snr")
    {
        "参考音频未通过服务端质检，请换用清晰单人录音或开启降噪"
    } else {
        "请检查火山引擎声音复刻配置与服务状态"
    };
    FinalSubError::Validation(format!(
        "豆包声音复刻请求失败（HTTP {status}{}{}）。{hint}",
        code.map(|value| format!(", code {value}"))
            .unwrap_or_default(),
        if message.is_empty() {
            String::new()
        } else {
            format!(": {message}")
        }
    ))
}

fn clone_headers(app_id: &str, access_token: &str) -> [(&'static str, String); 4] {
    [
        ("X-Api-App-Key", app_id.trim().to_string()),
        ("X-Api-Access-Key", access_token.trim().to_string()),
        ("X-Api-Request-Id", uuid::Uuid::new_v4().to_string()),
        ("Content-Type", "application/json".into()),
    ]
}

fn validate_credentials(app_id: &str, access_token: &str) -> Result<()> {
    if app_id.trim().is_empty() || access_token.trim().is_empty() {
        return Err(FinalSubError::Validation(
            "豆包声音复刻需要 APP ID 与 Access Token，请在在线 TTS 实例中保存训练凭据".into(),
        ));
    }
    if app_id.chars().any(char::is_control) || access_token.chars().any(char::is_control) {
        return Err(FinalSubError::Validation(
            "豆包训练凭据不能包含控制字符".into(),
        ));
    }
    Ok(())
}

pub struct CloneTrainRequest<'a> {
    pub speaker_id: &'a str,
    pub audio_path: &'a Path,
    pub language: &'a str,
    pub remove_background_noise: bool,
    pub enable_mss: bool,
    pub timeout_seconds: u32,
}

pub async fn train(app_id: &str, access_token: &str, request: CloneTrainRequest<'_>) -> Result<()> {
    validate_credentials(app_id, access_token)?;
    let speaker_id = validate_speaker_id(request.speaker_id)?;
    let metadata = std::fs::metadata(request.audio_path)?;
    if !request.audio_path.is_file() || metadata.len() == 0 || metadata.len() > MAX_AUDIO_BYTES {
        return Err(FinalSubError::Validation(
            "豆包声音复刻参考音频为空或超过 10 MB".into(),
        ));
    }
    let audio = std::fs::read(request.audio_path)?;
    let body = build_train_body(
        &speaker_id,
        &STANDARD.encode(audio),
        request.language,
        request.remove_background_noise,
        request.enable_mss,
    );
    let client = Client::builder()
        .timeout(Duration::from_secs(u64::from(
            request.timeout_seconds.max(120),
        )))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|error| FinalSubError::Validation(format!("创建豆包训练客户端失败：{error}")))?;
    let mut request = client.post(TRAIN_URL).json(&body);
    for (name, value) in clone_headers(app_id, access_token) {
        request = request.header(name, value);
    }
    let response = request
        .send()
        .await
        .map_err(|error| FinalSubError::Validation(format!("豆包声音复刻请求失败：{error}")))?;
    let status = response.status().as_u16();
    let bytes = response_bytes(response).await?;
    let payload = serde_json::from_slice::<Value>(&bytes).ok();
    let code = payload.as_ref().and_then(|value| {
        value
            .pointer("/BaseResp/StatusCode")
            .or_else(|| value.get("code"))
            .or_else(|| value.pointer("/header/code"))
            .and_then(|item| status_number(Some(item)))
    });
    if !(200..300).contains(&status) || code.is_some_and(|value| value != 0) {
        return Err(response_error(status, payload.as_ref(), &bytes));
    }
    Ok(())
}

async fn query_v3(
    client: &Client,
    app_id: &str,
    access_token: &str,
    speaker_id: &str,
) -> Result<Option<CloneStatus>> {
    let mut request = client
        .post(STATUS_URL)
        .json(&json!({ "speaker_id": speaker_id }));
    for (name, value) in clone_headers(app_id, access_token) {
        request = request.header(name, value);
    }
    let response = request
        .send()
        .await
        .map_err(|error| FinalSubError::Validation(format!("豆包声音复刻状态请求失败：{error}")))?;
    let status = response.status().as_u16();
    let bytes = response_bytes(response).await?;
    let payload = serde_json::from_slice::<Value>(&bytes).ok();
    if !(200..300).contains(&status) {
        return Ok(None);
    }
    Ok(payload.as_ref().and_then(parse_status))
}

pub async fn query_status(
    app_id: &str,
    access_token: &str,
    speaker_id: &str,
    timeout_seconds: u32,
) -> Result<CloneStatus> {
    validate_credentials(app_id, access_token)?;
    let speaker_id = validate_speaker_id(speaker_id)?;
    let client = Client::builder()
        .timeout(Duration::from_secs(u64::from(timeout_seconds.clamp(5, 60))))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|error| FinalSubError::Validation(format!("创建豆包状态客户端失败：{error}")))?;
    if let Ok(Some(status)) = query_v3(&client, app_id, access_token, &speaker_id).await {
        return Ok(status);
    }

    let response = client
        .post(STATUS_V1_URL)
        .header("Authorization", format!("Bearer;{}", access_token.trim()))
        .header("Resource-Id", "volc.megatts.voiceclone")
        .header("Content-Type", "application/json")
        .json(&json!({ "appid": app_id.trim(), "speaker_id": speaker_id }))
        .send()
        .await
        .map_err(|error| {
            FinalSubError::Validation(format!("豆包声音复刻状态回退请求失败：{error}"))
        })?;
    let status = response.status().as_u16();
    let bytes = response_bytes(response).await?;
    let payload = serde_json::from_slice::<Value>(&bytes).ok();
    if !(200..300).contains(&status) {
        return Err(response_error(status, payload.as_ref(), &bytes));
    }
    payload
        .as_ref()
        .and_then(parse_status)
        .ok_or_else(|| FinalSubError::Validation("豆包声音复刻状态响应无法识别".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn train_body_uses_icl_model_and_optional_flags() {
        let body = build_train_body("S_demo", "abc", "en", true, true);
        assert_eq!(body["model_types"][0], 4);
        assert_eq!(body["language"], 1);
        assert_eq!(body["audio"]["format"], "wav");
        assert_eq!(body["extra_params"]["enable_audio_denoise"], true);
        assert_eq!(body["extra_params"]["voice_clone_enable_mss"], true);
    }

    #[test]
    fn status_parser_handles_v3_values() {
        let ready = parse_status(&json!({
            "status": 2,
            "available_training_times": 3
        }))
        .unwrap();
        assert_eq!(ready.state, CloneState::Ready);
        assert_eq!(ready.training_times_left, Some(3));
        let nested = parse_status(&json!({
            "data": { "status": "training", "available_training_times": "2" }
        }))
        .unwrap();
        assert_eq!(nested.state, CloneState::Training);
        assert_eq!(nested.training_times_left, Some(2));
        let failed = parse_status(&json!({ "speaker_status": [{ "state": "failed" }] })).unwrap();
        assert_eq!(failed.state, CloneState::Failed);
        assert!(parse_status(&json!({ "message": "no status" })).is_none());
    }
}
