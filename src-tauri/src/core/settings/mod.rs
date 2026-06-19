use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::error::Result;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub language: String,
    #[serde(alias = "asrEngine")]
    pub asr_engine: String,
    #[serde(alias = "modelsPath")]
    pub models_path: String,
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
    #[serde(alias = "translateRetryTimes")]
    pub translate_retry_times: u32,
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
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            language: "zh".into(),
            asr_engine: "whisper-cpp".into(),
            models_path: default_models_path(),
            max_concurrent_tasks: 1,
            subtitle_output_format: "srt".into(),
            source_language: "auto".into(),
            target_language: "zh".into(),
            translate_provider: String::new(),
            translate_retry_times: 0,
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

fn home_dir() -> Option<PathBuf> {
    std::env::var("HOME")
        .ok()
        .map(PathBuf::from)
        .filter(|p| p.is_dir())
}

pub fn settings_path(app_config_dir: &Path) -> PathBuf {
    app_config_dir.join("settings.json")
}

pub fn load_settings(app_config_dir: &Path) -> Result<Settings> {
    let path = settings_path(app_config_dir);
    if !path.exists() {
        return Ok(Settings::default());
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

pub fn validate_settings(settings: &Settings) -> Result<()> {
    if !matches!(settings.language.as_str(), "zh" | "en") {
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
        assert_eq!(loaded.asr_engine, "whisper-cpp");
        assert_eq!(loaded.max_concurrent_tasks, 1);
        assert_eq!(loaded.subtitle_output_format, "srt");
        assert!(loaded.use_vad);
    }

    #[test]
    fn settings_serialize_as_snake_case() {
        let content = serde_json::to_string(&Settings::default()).unwrap();
        assert!(content.contains("asr_engine"));
        assert!(content.contains("models_path"));
        assert!(!content.contains("asrEngine"));
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
        assert_eq!(imported.max_concurrent_tasks, 2);
    }

    #[test]
    fn load_missing_returns_default() {
        let (_tmp, dir) = test_config_dir();
        let settings = load_settings(&dir).unwrap();
        assert_eq!(settings.language, "zh");
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
}
