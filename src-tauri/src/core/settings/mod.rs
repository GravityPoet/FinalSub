use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::error::Result;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub language: String,
    pub asr_engine: String,
    pub models_path: String,
    pub max_concurrent_tasks: u32,
    pub subtitle_output_format: String,
    pub source_language: String,
    pub target_language: String,
    pub translate_provider: String,
    pub translate_retry_times: u32,
    pub use_vad: bool,
    pub vad_threshold: f64,
    pub vad_min_speech_duration_ms: u32,
    pub vad_min_silence_duration_ms: u32,
    pub vad_max_speech_duration_s: u32,
    pub vad_speech_pad_ms: u32,
    pub vad_samples_overlap: f64,
    pub check_update_on_startup: bool,
    pub use_custom_temp_dir: bool,
    pub custom_temp_dir: String,
    pub whisper_command: String,
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
        .join("Tools/Local-LLM/FinalSub/whisper-models")
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
    Ok(settings)
}

pub fn save_settings(app_config_dir: &Path, settings: &Settings) -> Result<()> {
    let path = settings_path(app_config_dir);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let content = serde_json::to_string_pretty(settings)?;
    std::fs::write(&path, content)?;
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
    save_settings(app_config_dir, &export.settings)?;
    Ok(export.settings)
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
    fn load_missing_returns_default() {
        let (_tmp, dir) = test_config_dir();
        let settings = load_settings(&dir).unwrap();
        assert_eq!(settings.language, "zh");
    }

    #[test]
    fn save_and_modify() {
        let (_tmp, dir) = test_config_dir();
        let mut settings = Settings::default();
        settings.language = "en".into();
        settings.max_concurrent_tasks = 3;
        save_settings(&dir, &settings).unwrap();

        let loaded = load_settings(&dir).unwrap();
        assert_eq!(loaded.language, "en");
        assert_eq!(loaded.max_concurrent_tasks, 3);
    }

    #[test]
    fn export_import_roundtrip() {
        let (_tmp, dir) = test_config_dir();
        let mut settings = Settings::default();
        settings.language = "en".into();
        settings.target_language = "ja".into();
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
    fn reset_settings_restores_defaults() {
        let (_tmp, dir) = test_config_dir();
        let mut settings = Settings::default();
        settings.language = "en".into();
        super::save_settings(&dir, &settings).unwrap();

        let reset = super::reset_settings(&dir).unwrap();
        assert_eq!(reset.language, "zh");

        let loaded = super::load_settings(&dir).unwrap();
        assert_eq!(loaded.language, "zh");
    }
}
