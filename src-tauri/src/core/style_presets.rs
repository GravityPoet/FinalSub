use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use uuid::Uuid;

use crate::core::audio::BurnInStyleOptions;

const MAX_STYLE_PRESETS: usize = 64;
const MAX_STYLE_PRESET_FILE_BYTES: u64 = 1024 * 1024;
static STYLE_PRESET_SAVE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct SubtitleStyle {
    pub font_name: String,
    pub font_size: u32,
    pub font_color: String,
    pub outline_color: String,
    pub outline_width: f32,
    pub shadow: f32,
    pub background_color: String,
    pub opaque_background: bool,
    pub alignment: u8,
    pub margin_v: u32,
}

impl Default for SubtitleStyle {
    fn default() -> Self {
        Self {
            font_name: "PingFang SC".into(),
            font_size: 24,
            font_color: "&H00FFFFFF".into(),
            outline_color: "&H00000000".into(),
            outline_width: 2.0,
            shadow: 0.0,
            background_color: "&H80000000".into(),
            opaque_background: false,
            alignment: 2,
            margin_v: 30,
        }
    }
}

impl SubtitleStyle {
    pub fn validate(&self) -> Result<(), String> {
        let font_name = self.font_name.trim();
        if font_name.is_empty()
            || font_name.chars().count() > 128
            || font_name
                .chars()
                .any(|character| character.is_control() || matches!(character, ',' | '\'' | '\\'))
        {
            return Err("Subtitle font name contains unsupported characters".into());
        }
        if !(10..=120).contains(&self.font_size) {
            return Err("Subtitle font size must be between 10 and 120".into());
        }
        validate_ass_color("Font color", &self.font_color)?;
        validate_ass_color("Outline color", &self.outline_color)?;
        validate_ass_color("Background color", &self.background_color)?;
        if !self.outline_width.is_finite() || !(0.0..=10.0).contains(&self.outline_width) {
            return Err("Subtitle outline width must be between 0 and 10".into());
        }
        if !self.shadow.is_finite() || !(0.0..=20.0).contains(&self.shadow) {
            return Err("Subtitle shadow must be between 0 and 20".into());
        }
        if !(1..=9).contains(&self.alignment) {
            return Err("Subtitle alignment must be between 1 and 9".into());
        }
        if self.margin_v > 1_000 {
            return Err("Subtitle vertical margin cannot exceed 1000".into());
        }
        Ok(())
    }

    pub fn normalized(mut self) -> Self {
        self.font_name = self.font_name.trim().to_string();
        self.font_color = self.font_color.trim().to_ascii_uppercase();
        self.outline_color = self.outline_color.trim().to_ascii_uppercase();
        self.background_color = self.background_color.trim().to_ascii_uppercase();
        self
    }

    pub fn to_burn_in_options(&self) -> BurnInStyleOptions {
        BurnInStyleOptions {
            font_name: Some(self.font_name.clone()),
            font_size: Some(self.font_size),
            font_color: Some(self.font_color.clone()),
            outline_color: Some(self.outline_color.clone()),
            outline_width: Some(self.outline_width),
            shadow: Some(self.shadow),
            background_color: Some(self.background_color.clone()),
            opaque_background: Some(self.opaque_background),
            alignment: Some(self.alignment),
            margin_v: Some(self.margin_v),
            crf: None,
            preset: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SubtitleStylePreset {
    pub id: String,
    pub name: String,
    pub style: SubtitleStyle,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SaveSubtitleStylePresetRequest {
    pub id: Option<String>,
    pub name: String,
    pub style: SubtitleStyle,
}

pub fn style_presets_path(app_config_dir: &Path) -> PathBuf {
    app_config_dir.join("subtitle").join("style-presets.json")
}

pub fn load_style_presets(app_config_dir: &Path) -> Result<Vec<SubtitleStylePreset>, String> {
    let path = style_presets_path(app_config_dir);
    if !path.exists() {
        return Ok(Vec::new());
    }
    if std::fs::metadata(&path)
        .map_err(|error| format!("Failed to inspect subtitle style presets: {error}"))?
        .len()
        > MAX_STYLE_PRESET_FILE_BYTES
    {
        return Err("Subtitle style preset file exceeds 1 MB".into());
    }
    let content = std::fs::read_to_string(&path)
        .map_err(|error| format!("Failed to read subtitle style presets: {error}"))?;
    let presets: Vec<SubtitleStylePreset> = serde_json::from_str(&content)
        .map_err(|error| format!("Failed to parse subtitle style presets: {error}"))?;
    validate_style_presets(&presets)?;
    Ok(presets)
}

pub fn save_style_preset(
    app_config_dir: &Path,
    request: SaveSubtitleStylePresetRequest,
) -> Result<SubtitleStylePreset, String> {
    let _save_guard = lock_style_presets()?;
    validate_preset_name(&request.name)?;
    request.style.validate()?;
    let mut presets = load_style_presets(app_config_dir)?;
    let name = request.name.trim().to_string();
    let canonical_name = canonical_preset_name(&name);
    let requested_id = request.id.as_deref();
    if presets.iter().any(|preset| {
        Some(preset.id.as_str()) != requested_id
            && canonical_preset_name(&preset.name) == canonical_name
    }) {
        return Err("Subtitle style preset names must be unique".into());
    }
    let now = chrono::Utc::now().to_rfc3339();
    let style = request.style.normalized();

    let saved = if let Some(id) = requested_id {
        validate_preset_id(id)?;
        let preset = presets
            .iter_mut()
            .find(|preset| preset.id == id)
            .ok_or_else(|| "Subtitle style preset not found".to_string())?;
        preset.name = name;
        preset.style = style;
        preset.updated_at = now;
        preset.clone()
    } else {
        if presets.len() >= MAX_STYLE_PRESETS {
            return Err(format!(
                "At most {MAX_STYLE_PRESETS} subtitle style presets can be saved"
            ));
        }
        let preset = SubtitleStylePreset {
            id: Uuid::new_v4().to_string(),
            name,
            style,
            created_at: now.clone(),
            updated_at: now,
        };
        presets.push(preset.clone());
        preset
    };

    persist_style_presets(app_config_dir, &presets)?;
    Ok(saved)
}

pub fn delete_style_preset(app_config_dir: &Path, preset_id: &str) -> Result<String, String> {
    let _save_guard = lock_style_presets()?;
    validate_preset_id(preset_id)?;
    let mut presets = load_style_presets(app_config_dir)?;
    let original_len = presets.len();
    presets.retain(|preset| preset.id != preset_id);
    if presets.len() == original_len {
        return Err("Subtitle style preset not found".into());
    }
    persist_style_presets(app_config_dir, &presets)?;
    Ok(preset_id.to_string())
}

pub fn reorder_style_presets(
    app_config_dir: &Path,
    ordered_ids: &[String],
) -> Result<Vec<SubtitleStylePreset>, String> {
    let _save_guard = lock_style_presets()?;
    let presets = load_style_presets(app_config_dir)?;
    if ordered_ids.len() != presets.len() {
        return Err("Subtitle style preset order must include every preset exactly once".into());
    }
    let expected: HashSet<&str> = presets.iter().map(|preset| preset.id.as_str()).collect();
    let received: HashSet<&str> = ordered_ids.iter().map(String::as_str).collect();
    if received.len() != ordered_ids.len() || received != expected {
        return Err("Subtitle style preset order must include every preset exactly once".into());
    }
    let mut by_id = presets
        .into_iter()
        .map(|preset| (preset.id.clone(), preset))
        .collect::<std::collections::HashMap<_, _>>();
    let reordered = ordered_ids
        .iter()
        .map(|id| {
            by_id
                .remove(id)
                .ok_or_else(|| "Subtitle style preset order is invalid".to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    persist_style_presets(app_config_dir, &reordered)?;
    Ok(reordered)
}

fn lock_style_presets() -> Result<std::sync::MutexGuard<'static, ()>, String> {
    STYLE_PRESET_SAVE_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|_| "Subtitle style preset storage lock is unavailable".to_string())
}

fn persist_style_presets(
    app_config_dir: &Path,
    presets: &[SubtitleStylePreset],
) -> Result<(), String> {
    validate_style_presets(presets)?;
    let path = style_presets_path(app_config_dir);
    let parent = path
        .parent()
        .ok_or_else(|| "Subtitle style preset path has no parent directory".to_string())?;
    std::fs::create_dir_all(parent)
        .map_err(|error| format!("Failed to create subtitle preset directory: {error}"))?;
    let content = serde_json::to_string_pretty(presets)
        .map_err(|error| format!("Failed to serialize subtitle style presets: {error}"))?;
    let temporary_path = path.with_extension("json.tmp");
    std::fs::write(&temporary_path, content)
        .map_err(|error| format!("Failed to write subtitle style presets: {error}"))?;
    if let Err(error) = replace_preset_file(&temporary_path, &path) {
        let _ = std::fs::remove_file(&temporary_path);
        return Err(format!("Failed to save subtitle style presets: {error}"));
    }
    Ok(())
}

#[cfg(not(windows))]
fn replace_preset_file(temporary_path: &Path, path: &Path) -> std::io::Result<()> {
    std::fs::rename(temporary_path, path)
}

#[cfg(windows)]
fn replace_preset_file(temporary_path: &Path, path: &Path) -> std::io::Result<()> {
    if !path.exists() {
        return std::fs::rename(temporary_path, path);
    }
    let displaced = path.with_extension(format!("json.rollback-{}", Uuid::new_v4()));
    std::fs::rename(path, &displaced)?;
    if let Err(error) = std::fs::rename(temporary_path, path) {
        let _ = std::fs::rename(&displaced, path);
        return Err(error);
    }
    let _ = std::fs::remove_file(displaced);
    Ok(())
}

fn validate_style_presets(presets: &[SubtitleStylePreset]) -> Result<(), String> {
    if presets.len() > MAX_STYLE_PRESETS {
        return Err(format!(
            "At most {MAX_STYLE_PRESETS} subtitle style presets can be saved"
        ));
    }
    let mut ids = HashSet::new();
    let mut names = HashSet::new();
    for preset in presets {
        validate_preset_id(&preset.id)?;
        if !ids.insert(preset.id.as_str()) {
            return Err("Subtitle style preset IDs must be unique".into());
        }
        validate_preset_name(&preset.name)?;
        if !names.insert(canonical_preset_name(&preset.name)) {
            return Err("Subtitle style preset names must be unique".into());
        }
        preset.style.validate()?;
        if chrono::DateTime::parse_from_rfc3339(&preset.created_at).is_err()
            || chrono::DateTime::parse_from_rfc3339(&preset.updated_at).is_err()
        {
            return Err("Subtitle style preset timestamps are invalid".into());
        }
    }
    Ok(())
}

fn validate_preset_id(id: &str) -> Result<(), String> {
    Uuid::parse_str(id)
        .map(|_| ())
        .map_err(|_| "Subtitle style preset ID is invalid".to_string())
}

fn validate_preset_name(name: &str) -> Result<(), String> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 64 || name.chars().any(char::is_control) {
        return Err("Subtitle style preset name must contain 1-64 visible characters".into());
    }
    Ok(())
}

fn canonical_preset_name(name: &str) -> String {
    name.trim().to_lowercase()
}

fn validate_ass_color(label: &str, color: &str) -> Result<(), String> {
    let normalized = color.trim();
    let digits = normalized
        .strip_prefix("&H")
        .or_else(|| normalized.strip_prefix("&h"))
        .ok_or_else(|| format!("{label} must use ASS hexadecimal format"))?;
    if !matches!(digits.len(), 6 | 8)
        || !digits
            .chars()
            .all(|character| character.is_ascii_hexdigit())
    {
        return Err(format!("{label} must use ASS hexadecimal format"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preset_crud_and_reorder_are_atomic() {
        let directory = tempfile::tempdir().unwrap();
        let first = save_style_preset(
            directory.path(),
            SaveSubtitleStylePresetRequest {
                id: None,
                name: "  课程字幕  ".into(),
                style: SubtitleStyle::default(),
            },
        )
        .unwrap();
        let second = save_style_preset(
            directory.path(),
            SaveSubtitleStylePresetRequest {
                id: None,
                name: "夜间大字".into(),
                style: SubtitleStyle {
                    font_size: 36,
                    ..SubtitleStyle::default()
                },
            },
        )
        .unwrap();
        assert_eq!(first.name, "课程字幕");
        assert_eq!(load_style_presets(directory.path()).unwrap().len(), 2);

        let reordered =
            reorder_style_presets(directory.path(), &[second.id.clone(), first.id.clone()])
                .unwrap();
        assert_eq!(reordered[0].id, second.id);

        let updated = save_style_preset(
            directory.path(),
            SaveSubtitleStylePresetRequest {
                id: Some(first.id.clone()),
                name: "课程双语".into(),
                style: SubtitleStyle {
                    margin_v: 48,
                    ..SubtitleStyle::default()
                },
            },
        )
        .unwrap();
        assert_eq!(updated.created_at, first.created_at);
        assert_eq!(updated.style.margin_v, 48);

        assert_eq!(
            delete_style_preset(directory.path(), &second.id).unwrap(),
            second.id
        );
        assert_eq!(load_style_presets(directory.path()).unwrap(), vec![updated]);
        assert!(!style_presets_path(directory.path())
            .with_extension("json.tmp")
            .exists());
    }

    #[test]
    fn validation_rejects_duplicate_names_injection_and_invalid_order() {
        let directory = tempfile::tempdir().unwrap();
        let saved = save_style_preset(
            directory.path(),
            SaveSubtitleStylePresetRequest {
                id: None,
                name: "Clean".into(),
                style: SubtitleStyle::default(),
            },
        )
        .unwrap();
        assert!(save_style_preset(
            directory.path(),
            SaveSubtitleStylePresetRequest {
                id: None,
                name: " clean ".into(),
                style: SubtitleStyle::default(),
            },
        )
        .is_err());
        assert!(SubtitleStyle {
            font_name: "Arial,PrimaryColour=&H00FFFFFF".into(),
            ..SubtitleStyle::default()
        }
        .validate()
        .is_err());
        assert!(reorder_style_presets(directory.path(), &[saved.id.clone(), saved.id]).is_err());
    }

    #[test]
    fn legacy_style_defaults_missing_fields() {
        let style: SubtitleStyle = serde_json::from_value(serde_json::json!({
            "font_size": 32,
            "font_color": "&H00FFFFFF"
        }))
        .unwrap();
        assert_eq!(style.font_size, 32);
        assert_eq!(style.outline_width, 2.0);
        assert_eq!(style.alignment, 2);
        style.validate().unwrap();
        let burn_style = style.to_burn_in_options();
        assert_eq!(burn_style.font_size, Some(32));
        assert_eq!(burn_style.outline_width, Some(2.0));
    }

    #[test]
    fn oversized_preset_file_is_rejected_before_parsing() {
        let directory = tempfile::tempdir().unwrap();
        let path = style_presets_path(directory.path());
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, vec![b' '; MAX_STYLE_PRESET_FILE_BYTES as usize + 1]).unwrap();
        assert!(load_style_presets(directory.path()).is_err());
    }
}
