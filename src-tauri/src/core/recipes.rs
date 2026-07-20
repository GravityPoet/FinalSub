use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use uuid::Uuid;

use crate::core::style_presets::SubtitleStyle;

const MAX_RECIPES: usize = 64;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TaskRecipePipelineSnapshot {
    pub enable_dubbing: bool,
    pub enable_compose: bool,
    pub subtitle_review: bool,
    pub dubbing_review: bool,
    pub dubbing_engine: String,
    pub dubbing_model_or_provider_id: String,
    pub dubbing_voice: String,
    pub dubbing_speed: f32,
    pub compose_soft_subtitle: bool,
    pub compose_audio_mode: String,
    pub compose_encoder_mode: String,
    #[serde(default)]
    pub compose_style: Option<SubtitleStyle>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TaskRecipeSnapshot {
    pub task_type: String,
    pub engine_id: String,
    pub model_id: String,
    pub source_language: String,
    pub target_language: String,
    pub translation_content_mode: String,
    pub output_format: String,
    pub output_name: String,
    pub strip_chinese_punctuation: bool,
    pub review_required: bool,
    #[serde(default)]
    pub max_subtitle_chars: i32,
    #[serde(default)]
    pub pipeline: Option<TaskRecipePipelineSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TaskRecipe {
    pub id: String,
    pub name: String,
    pub snapshot: TaskRecipeSnapshot,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SaveTaskRecipeRequest {
    pub id: Option<String>,
    pub name: String,
    pub snapshot: TaskRecipeSnapshot,
}

pub fn recipes_path(app_config_dir: &Path) -> PathBuf {
    app_config_dir.join("tasks").join("recipes.json")
}

pub fn load_recipes(app_config_dir: &Path) -> Result<Vec<TaskRecipe>, String> {
    let path = recipes_path(app_config_dir);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let content = std::fs::read_to_string(&path)
        .map_err(|error| format!("Failed to read task recipes: {error}"))?;
    let mut recipes: Vec<TaskRecipe> = serde_json::from_str(&content)
        .map_err(|error| format!("Failed to parse task recipes: {error}"))?;
    validate_recipes(&recipes)?;
    recipes.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
    Ok(recipes)
}

pub fn save_recipe(
    app_config_dir: &Path,
    request: SaveTaskRecipeRequest,
) -> Result<TaskRecipe, String> {
    validate_recipe_name(&request.name)?;
    validate_snapshot(&request.snapshot)?;
    let mut recipes = load_recipes(app_config_dir)?;
    let now = chrono::Utc::now().to_rfc3339();

    let recipe = if let Some(id) = request.id.as_deref() {
        validate_recipe_id(id)?;
        let existing = recipes
            .iter_mut()
            .find(|recipe| recipe.id == id)
            .ok_or_else(|| "Task recipe not found".to_string())?;
        existing.name = request.name.trim().to_string();
        existing.snapshot = request.snapshot;
        existing.updated_at = now;
        existing.clone()
    } else {
        if recipes.len() >= MAX_RECIPES {
            return Err(format!("At most {MAX_RECIPES} task recipes can be saved"));
        }
        let recipe = TaskRecipe {
            id: Uuid::new_v4().to_string(),
            name: request.name.trim().to_string(),
            snapshot: request.snapshot,
            created_at: now.clone(),
            updated_at: now,
        };
        recipes.push(recipe.clone());
        recipe
    };

    persist_recipes(app_config_dir, &recipes)?;
    Ok(recipe)
}

pub fn delete_recipe(app_config_dir: &Path, recipe_id: &str) -> Result<String, String> {
    validate_recipe_id(recipe_id)?;
    let mut recipes = load_recipes(app_config_dir)?;
    let original_len = recipes.len();
    recipes.retain(|recipe| recipe.id != recipe_id);
    if recipes.len() == original_len {
        return Err("Task recipe not found".into());
    }
    persist_recipes(app_config_dir, &recipes)?;
    Ok(recipe_id.to_string())
}

fn persist_recipes(app_config_dir: &Path, recipes: &[TaskRecipe]) -> Result<(), String> {
    validate_recipes(recipes)?;
    let path = recipes_path(app_config_dir);
    let parent = path
        .parent()
        .ok_or_else(|| "Task recipe path has no parent directory".to_string())?;
    std::fs::create_dir_all(parent)
        .map_err(|error| format!("Failed to create task recipe directory: {error}"))?;
    let content = serde_json::to_string_pretty(recipes)
        .map_err(|error| format!("Failed to serialize task recipes: {error}"))?;
    let temporary_path = path.with_extension("json.tmp");
    std::fs::write(&temporary_path, content)
        .map_err(|error| format!("Failed to write task recipes: {error}"))?;
    if let Err(error) = std::fs::rename(&temporary_path, &path) {
        let _ = std::fs::remove_file(&temporary_path);
        return Err(format!("Failed to save task recipes: {error}"));
    }
    Ok(())
}

fn validate_recipes(recipes: &[TaskRecipe]) -> Result<(), String> {
    if recipes.len() > MAX_RECIPES {
        return Err(format!("At most {MAX_RECIPES} task recipes can be saved"));
    }
    let mut ids = std::collections::HashSet::new();
    for recipe in recipes {
        validate_recipe_id(&recipe.id)?;
        if !ids.insert(recipe.id.as_str()) {
            return Err("Task recipe IDs must be unique".into());
        }
        validate_recipe_name(&recipe.name)?;
        validate_snapshot(&recipe.snapshot)?;
        if chrono::DateTime::parse_from_rfc3339(&recipe.created_at).is_err()
            || chrono::DateTime::parse_from_rfc3339(&recipe.updated_at).is_err()
        {
            return Err("Task recipe timestamps are invalid".into());
        }
    }
    Ok(())
}

fn validate_recipe_id(id: &str) -> Result<(), String> {
    Uuid::parse_str(id)
        .map(|_| ())
        .map_err(|_| "Task recipe ID is invalid".to_string())
}

fn validate_recipe_name(name: &str) -> Result<(), String> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 80 || name.chars().any(char::is_control) {
        return Err("Task recipe name must contain 1-80 visible characters".into());
    }
    Ok(())
}

fn validate_snapshot(snapshot: &TaskRecipeSnapshot) -> Result<(), String> {
    crate::core::subtitle::parse_subtitle_length_mode(snapshot.max_subtitle_chars)
        .map_err(|error| error.to_string())?;
    if !matches!(
        snapshot.task_type.as_str(),
        "generate-only" | "generate-and-translate" | "translate-only"
    ) {
        return Err("Task recipe uses an unsupported task type".into());
    }
    if !matches!(
        snapshot.translation_content_mode.as_str(),
        "target-only" | "source-and-target" | "target-and-source"
    ) {
        return Err("Task recipe uses an unsupported subtitle content mode".into());
    }
    if !matches!(
        snapshot.output_format.as_str(),
        "srt" | "vtt" | "ass" | "lrc" | "txt"
    ) {
        return Err("Task recipe uses an unsupported output format".into());
    }
    for (label, value, limit) in [
        ("engine", snapshot.engine_id.as_str(), 128),
        ("model", snapshot.model_id.as_str(), 256),
        ("source language", snapshot.source_language.as_str(), 32),
        ("target language", snapshot.target_language.as_str(), 32),
        ("output name", snapshot.output_name.as_str(), 180),
    ] {
        if value.chars().count() > limit || value.chars().any(char::is_control) {
            return Err(format!("Task recipe {label} value is invalid"));
        }
    }
    if let Some(pipeline) = snapshot.pipeline.as_ref() {
        if !pipeline.enable_dubbing
            && !pipeline.enable_compose
            && !pipeline.subtitle_review
            && !pipeline.dubbing_review
        {
            return Err(
                "Task recipe pipeline must enable at least one target or review step".into(),
            );
        }
        if (pipeline.enable_dubbing || pipeline.enable_compose)
            && snapshot.task_type == "translate-only"
        {
            return Err("Task recipe media targets require a media task type".into());
        }
        if pipeline.dubbing_review && !pipeline.enable_dubbing {
            return Err("Task recipe dubbing review requires dubbing".into());
        }
        if pipeline.enable_dubbing {
            if !matches!(pipeline.dubbing_engine.as_str(), "local" | "cloud") {
                return Err("Task recipe dubbing engine is invalid".into());
            }
            if pipeline.dubbing_model_or_provider_id.trim().is_empty() {
                return Err("Task recipe dubbing target is missing".into());
            }
            if pipeline.dubbing_engine == "cloud"
                && Uuid::parse_str(pipeline.dubbing_model_or_provider_id.trim()).is_err()
            {
                return Err("Task recipe cloud TTS target is invalid".into());
            }
            if !pipeline.dubbing_speed.is_finite() || !(0.5..=2.0).contains(&pipeline.dubbing_speed)
            {
                return Err("Task recipe dubbing speed is invalid".into());
            }
        }
        if !matches!(
            pipeline.compose_audio_mode.as_str(),
            "keep" | "replace" | "mix" | "add-track"
        ) {
            return Err("Task recipe compose audio mode is invalid".into());
        }
        if !matches!(
            pipeline.compose_encoder_mode.as_str(),
            "auto" | "cpu" | "hardware"
        ) {
            return Err("Task recipe compose encoder mode is invalid".into());
        }
        if let Some(style) = pipeline.compose_style.as_ref() {
            style.validate()?;
        }
        if pipeline.enable_compose
            && pipeline.compose_audio_mode != "keep"
            && !pipeline.enable_dubbing
        {
            return Err("Task recipe compose audio mode requires dubbing".into());
        }
        if (pipeline.enable_dubbing || pipeline.enable_compose) && snapshot.output_format == "txt" {
            return Err("Task recipe downstream targets require timed subtitles".into());
        }
        for (label, value) in [
            (
                "dubbing target",
                pipeline.dubbing_model_or_provider_id.as_str(),
            ),
            ("dubbing voice", pipeline.dubbing_voice.as_str()),
        ] {
            if value.chars().count() > 200 || value.chars().any(char::is_control) {
                return Err(format!("Task recipe {label} is invalid"));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot() -> TaskRecipeSnapshot {
        TaskRecipeSnapshot {
            task_type: "generate-and-translate".into(),
            engine_id: "parakeet-mlx".into(),
            model_id: "parakeet-tdt-0.6b-v2".into(),
            source_language: "auto".into(),
            target_language: "zh".into(),
            translation_content_mode: "source-and-target".into(),
            output_format: "srt".into(),
            output_name: String::new(),
            strip_chinese_punctuation: false,
            review_required: true,
            max_subtitle_chars: 0,
            pipeline: None,
        }
    }

    #[test]
    fn recipe_create_update_reload_and_delete_are_atomic() {
        let directory = tempfile::tempdir().unwrap();
        let created = save_recipe(
            directory.path(),
            SaveTaskRecipeRequest {
                id: None,
                name: "课程双语".into(),
                snapshot: snapshot(),
            },
        )
        .unwrap();
        assert_eq!(
            load_recipes(directory.path()).unwrap(),
            vec![created.clone()]
        );

        let updated = save_recipe(
            directory.path(),
            SaveTaskRecipeRequest {
                id: Some(created.id.clone()),
                name: "课程双语 · 已校对".into(),
                snapshot: TaskRecipeSnapshot {
                    output_format: "ass".into(),
                    ..snapshot()
                },
            },
        )
        .unwrap();
        assert_eq!(updated.id, created.id);
        assert_eq!(updated.created_at, created.created_at);
        assert_eq!(updated.snapshot.output_format, "ass");

        assert_eq!(
            delete_recipe(directory.path(), &created.id).unwrap(),
            created.id
        );
        assert!(load_recipes(directory.path()).unwrap().is_empty());
        assert!(!recipes_path(directory.path())
            .with_extension("json.tmp")
            .exists());
    }

    #[test]
    fn recipe_validation_rejects_invalid_enums_and_control_characters() {
        let mut invalid = snapshot();
        invalid.task_type = "delete-everything".into();
        assert!(validate_snapshot(&invalid).is_err());
        assert!(validate_recipe_name("bad\nname").is_err());
    }

    #[test]
    fn legacy_recipe_defaults_to_smart_line_breaking() {
        let mut value = serde_json::to_value(snapshot()).unwrap();
        value.as_object_mut().unwrap().remove("max_subtitle_chars");
        value.as_object_mut().unwrap().remove("pipeline");
        let restored: TaskRecipeSnapshot = serde_json::from_value(value).unwrap();
        assert_eq!(restored.max_subtitle_chars, 0);
        assert!(restored.pipeline.is_none());
    }

    #[test]
    fn target_driven_recipe_roundtrips_and_rejects_impossible_dependencies() {
        let mut target_driven = snapshot();
        target_driven.pipeline = Some(TaskRecipePipelineSnapshot {
            enable_dubbing: true,
            enable_compose: true,
            subtitle_review: true,
            dubbing_review: false,
            dubbing_engine: "local".into(),
            dubbing_model_or_provider_id: "kokoro-multi-lang-v1_1".into(),
            dubbing_voice: "10".into(),
            dubbing_speed: 1.0,
            compose_soft_subtitle: false,
            compose_audio_mode: "replace".into(),
            compose_encoder_mode: "auto".into(),
            compose_style: Some(SubtitleStyle::default()),
        });
        assert!(validate_snapshot(&target_driven).is_ok());
        let restored: TaskRecipeSnapshot =
            serde_json::from_value(serde_json::to_value(&target_driven).unwrap()).unwrap();
        assert_eq!(restored, target_driven);

        target_driven.pipeline.as_mut().unwrap().enable_dubbing = false;
        assert!(validate_snapshot(&target_driven).is_err());
    }
}
