//! 火山引擎豆包 TTS 的纯协议工具。
//!
//! 该模块不发网络请求、不读写文件，负责把 V3 单向流式 HTTP 的请求和
//! chunked JSON 响应收敛成可单测的纯函数。凭据只由调用层从本地凭据存储
//! 读取，绝不进入这里的持久化结构或日志。

use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde_json::{json, Map, Value};

pub const VOLC_TTS_URL: &str = "https://openspeech.bytedance.com/api/v3/tts/unidirectional";
pub const VOLC_TTS_SAMPLE_RATE: u32 = 24_000;
pub const DEFAULT_RESOURCE_ID: &str = "seed-tts-2.0";
pub const ICL_RESOURCE_ID: &str = "seed-icl-2.0";
/// 保守的同步短文本上限，避免长文本在 60 秒默认超时内占用连接却无法
/// 稳定完成。字幕行超过时应先拆分。
pub const MAX_TEXT_CHARS: usize = 1_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VolcTtsStreamResult {
    pub pcm: Vec<u8>,
    pub error_code: Option<i64>,
    pub end_code: Option<i64>,
    pub message: String,
}

pub fn build_headers(
    api_key: &str,
    resource_id: &str,
    request_id: &str,
) -> [(&'static str, String); 4] {
    [
        ("X-Api-Key", api_key.trim().to_string()),
        ("X-Api-Resource-Id", resource_id.trim().to_string()),
        ("X-Api-Request-Id", request_id.trim().to_string()),
        ("Content-Type", "application/json".to_string()),
    ]
}

pub fn resource_id_for_voice(voice: &str, configured: &str) -> String {
    if voice.trim().starts_with("S_") {
        return ICL_RESOURCE_ID.to_string();
    }
    let configured = configured.trim();
    if configured.is_empty() {
        DEFAULT_RESOURCE_ID.to_string()
    } else {
        configured.to_string()
    }
}

pub fn is_valid_resource_id(value: &str) -> bool {
    matches!(
        value.trim(),
        "seed-tts-2.0" | "seed-tts-1.0" | "seed-tts-1.0-concurr"
    )
}

pub fn text_char_count(value: &str) -> usize {
    value.chars().count()
}

pub fn text_is_within_limit(value: &str) -> bool {
    text_char_count(value) <= MAX_TEXT_CHARS
}

pub fn speed_to_speech_rate(speed: Option<f32>) -> Option<i32> {
    let speed = speed?;
    if !speed.is_finite() || speed <= 0.0 {
        return None;
    }
    let rate = ((speed - 1.0) * 100.0).round() as i32;
    if rate == 0 {
        None
    } else {
        Some(rate.clamp(-50, 100))
    }
}

pub fn build_request_body(text: &str, speaker: &str, speed: Option<f32>) -> Value {
    let mut audio_params = Map::new();
    audio_params.insert("format".into(), Value::String("pcm".into()));
    audio_params.insert(
        "sample_rate".into(),
        Value::Number(serde_json::Number::from(VOLC_TTS_SAMPLE_RATE)),
    );
    if let Some(rate) = speed_to_speech_rate(speed) {
        audio_params.insert("speech_rate".into(), Value::Number(rate.into()));
    }
    json!({
        "user": { "uid": "finalsub" },
        "req_params": {
            "text": text,
            "speaker": speaker,
            "audio_params": Value::Object(audio_params),
        }
    })
}

fn extract_json_chunks(text: &str) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut depth = 0_u32;
    let mut start = None;
    let mut in_string = false;
    let mut escaped = false;

    for (index, character) in text.char_indices() {
        if in_string {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                in_string = false;
            }
            continue;
        }
        match character {
            '"' if depth > 0 => in_string = true,
            '{' => {
                if depth == 0 {
                    start = Some(index);
                }
                depth = depth.saturating_add(1);
            }
            '}' if depth > 0 => {
                depth -= 1;
                if depth == 0 {
                    if let Some(begin) = start.take() {
                        let end = index + character.len_utf8();
                        chunks.push(text[begin..end].to_string());
                    }
                }
            }
            _ => {}
        }
    }
    chunks
}

fn value_i64(value: Option<&Value>) -> Option<i64> {
    match value? {
        Value::Number(number) => number
            .as_i64()
            .or_else(|| number.as_f64().map(|n| n as i64)),
        Value::String(text) => text.trim().parse::<i64>().ok(),
        _ => None,
    }
}

fn value_message(value: Option<&Value>) -> String {
    value
        .and_then(Value::as_str)
        .unwrap_or_default()
        .chars()
        .filter(|character| !character.is_control() || matches!(*character, '\n' | '\t'))
        .take(2_000)
        .collect::<String>()
        .trim()
        .to_string()
}

pub fn parse_stream(text: &str) -> VolcTtsStreamResult {
    let mut result = VolcTtsStreamResult {
        pcm: Vec::new(),
        error_code: None,
        end_code: None,
        message: String::new(),
    };

    for raw in extract_json_chunks(text) {
        let Ok(value) = serde_json::from_str::<Value>(&raw) else {
            continue;
        };
        let header = value.get("header");
        let code = value_i64(
            value
                .get("code")
                .or_else(|| header.and_then(|item| item.get("code"))),
        )
        .unwrap_or(0);
        let message = value_message(
            value
                .get("message")
                .or_else(|| header.and_then(|item| item.get("message"))),
        );

        if code == 0 {
            if let Some(data) = value
                .get("data")
                .and_then(Value::as_str)
                .filter(|data| !data.is_empty())
            {
                match STANDARD.decode(data) {
                    Ok(bytes) => result.pcm.extend_from_slice(&bytes),
                    Err(_) => {
                        result.error_code = Some(-1);
                        result.message = "豆包 TTS 音频分片不是有效的 base64".into();
                        break;
                    }
                }
            }
            continue;
        }
        if code == 20_000_000 {
            result.end_code = Some(code);
            if result.message.is_empty() {
                result.message = message;
            }
            continue;
        }
        result.error_code = Some(code);
        result.message = message;
        break;
    }
    result
}

pub fn error_hint(http_status: u16, code: Option<i64>, message: &str) -> String {
    let message = message
        .chars()
        .filter(|character| !character.is_control() || matches!(*character, '\n' | '\t'))
        .take(200)
        .collect::<String>()
        .trim()
        .to_string();
    let detail = [
        code.map(|value| format!("code {value}")),
        (!message.is_empty()).then(|| message.clone()),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" ");
    let suffix = if detail.is_empty() {
        String::new()
    } else {
        format!(" - {detail}")
    };
    let lower = message.to_ascii_lowercase();

    if matches!(http_status, 401 | 403) {
        return format!(
            "豆包 TTS: HTTP {http_status}{suffix}。请检查 API Key：需为「豆包语音」控制台签发（火山方舟/大模型推理的 Key 不通用），并确认账号已开通语音合成大模型服务"
        );
    }
    if http_status == 429 || lower.contains("concurrency") || lower.contains("quota") {
        return format!(
            "豆包 TTS: 并发/配额受限{suffix}。请减少同时生成的字幕行或稍后重试；免费赠额与字符版的并发上限有限"
        );
    }
    if code == Some(45_000_000) && (lower.contains("speaker") || lower.contains("permission")) {
        return format!(
            "豆包 TTS: 音色不可用{suffix}。请检查音色 ID 拼写、账号是否已开通对应音色；S_ 开头的克隆音色还必须属于当前账号"
        );
    }
    if code == Some(55_000_000) && lower.contains("mismatch") {
        return format!(
            "豆包 TTS: 音色 ID 有误或与资源版本不匹配{suffix}。2.0 音色（*_uranus_bigtts 等）请选择 seed-tts-2.0；1.0 音色（*_mars/moon_bigtts 等）请选择 seed-tts-1.0"
        );
    }
    if code == Some(40_402_003) {
        return format!("豆包 TTS: 单请求文本超长{suffix}。请拆分该行字幕");
    }
    format!("豆包 TTS: 合成失败（HTTP {http_status}{suffix}）")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_headers_use_fixed_resource_fallback() {
        let headers = build_headers(" key ", "seed-tts-2.0", "req-1");
        assert_eq!(headers[0], ("X-Api-Key", "key".into()));
        assert_eq!(headers[1], ("X-Api-Resource-Id", "seed-tts-2.0".into()));
        assert_eq!(headers[2], ("X-Api-Request-Id", "req-1".into()));
        assert!(is_valid_resource_id("seed-tts-1.0-concurr"));
        assert!(!is_valid_resource_id("https://example.com"));
    }

    #[test]
    fn cloned_voice_routes_to_icl_resource() {
        assert_eq!(
            resource_id_for_voice("S_demo", "seed-tts-2.0"),
            ICL_RESOURCE_ID
        );
        assert_eq!(
            resource_id_for_voice("zh_female_x", ""),
            DEFAULT_RESOURCE_ID
        );
    }

    #[test]
    fn speed_maps_to_native_rate_and_omits_default() {
        assert_eq!(speed_to_speech_rate(Some(1.0)), None);
        assert_eq!(speed_to_speech_rate(Some(1.3)), Some(30));
        assert_eq!(speed_to_speech_rate(Some(0.25)), Some(-50));
        assert_eq!(speed_to_speech_rate(Some(2.5)), Some(100));
        assert_eq!(speed_to_speech_rate(Some(f32::NAN)), None);
        let body = build_request_body("hello", "zh_female_x", Some(1.3));
        assert_eq!(body["req_params"]["audio_params"]["format"], "pcm");
        assert_eq!(body["req_params"]["audio_params"]["speech_rate"], 30);
        assert!(
            build_request_body("hello", "zh_female_x", Some(1.0))["req_params"]["audio_params"]
                ["speech_rate"]
                .is_null()
        );
    }

    #[test]
    fn text_limit_counts_unicode_characters_instead_of_utf8_bytes() {
        assert_eq!(text_char_count("你a好"), 3);
        assert!(text_is_within_limit(&"你".repeat(MAX_TEXT_CHARS)));
        assert!(!text_is_within_limit(&"你".repeat(MAX_TEXT_CHARS + 1)));
    }

    #[test]
    fn parser_accepts_line_and_concatenated_chunks_and_header_errors() {
        let first = STANDARD.encode([0_u8, 1, 2, 3]);
        let second = STANDARD.encode([4_u8, 5]);
        let stream = format!(
            "{{\"code\":0,\"data\":\"{first}\"}}\n{{\"code\":0,\"data\":\"{second}\"}}{{\"code\":20000000,\"message\":\"ok\"}}"
        );
        let parsed = parse_stream(&stream);
        assert_eq!(parsed.pcm, vec![0, 1, 2, 3, 4, 5]);
        assert_eq!(parsed.end_code, Some(20_000_000));
        assert_eq!(parsed.error_code, None);

        let error = parse_stream(r#"{"header":{"code":45000010,"message":"Invalid X-Api-Key"}}"#);
        assert_eq!(error.error_code, Some(45_000_010));
        assert!(error.message.contains("Invalid X-Api-Key"));
    }

    #[test]
    fn parser_rejects_invalid_audio_chunk() {
        let parsed = parse_stream(r#"{"code":0,"data":"not-base64?"}"#);
        assert_eq!(parsed.error_code, Some(-1));
        assert!(parsed.pcm.is_empty());
    }

    #[test]
    fn error_hints_are_actionable_without_echoing_secrets() {
        let hint = error_hint(401, Some(45_000_010), "Invalid X-Api-Key");
        assert!(hint.contains("豆包语音"));
        assert!(!hint.contains("secret"));
        let mismatch = error_hint(200, Some(55_000_000), "resource mismatched");
        assert!(mismatch.contains("seed-tts-2.0"));
        let quota = error_hint(429, None, "concurrency quota");
        assert!(quota.contains("减少同时生成"));
    }
}
