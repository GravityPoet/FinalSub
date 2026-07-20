use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TaskStatus {
    Pending,
    Running,
    Paused,
    Cancelled,
    Review,
    Done,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TaskType {
    GenerateAndTranslate,
    GenerateOnly,
    TranslateOnly,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TranslationContentMode {
    #[default]
    TargetOnly,
    SourceAndTarget,
    TargetAndSource,
}

impl TranslationContentMode {
    pub fn is_bilingual(self) -> bool {
        !matches!(self, TranslationContentMode::TargetOnly)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub task_type: TaskType,
    pub status: TaskStatus,
    pub media_path: String,
    pub media_name: String,
    pub engine_id: String,
    pub model_id: String,
    pub source_language: Option<String>,
    pub target_language: Option<String>,
    #[serde(default)]
    pub translation_content_mode: TranslationContentMode,
    pub output_format: String,
    #[serde(default)]
    pub output_name: Option<String>,
    #[serde(default)]
    pub strip_chinese_punctuation: bool,
    #[serde(default)]
    pub review_required: bool,
    #[serde(default)]
    pub max_subtitle_chars: i32,
    #[serde(default)]
    pub reviewed_at: Option<String>,
    pub progress: f32,
    pub status_message: String,
    pub output_path: Option<String>,
    pub error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

pub type TaskMap = Arc<RwLock<HashMap<String, Task>>>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateTaskParams {
    pub task_type: TaskType,
    pub media_path: String,
    pub media_name: String,
    pub engine_id: String,
    pub model_id: String,
    pub source_language: Option<String>,
    pub target_language: Option<String>,
    pub translation_content_mode: TranslationContentMode,
    pub output_format: Option<String>,
    pub output_name: Option<String>,
    pub strip_chinese_punctuation: bool,
    pub review_required: bool,
    pub max_subtitle_chars: i32,
}

pub fn create_task(params: CreateTaskParams) -> Task {
    let now = chrono::Utc::now().to_rfc3339();
    Task {
        id: Uuid::new_v4().to_string(),
        task_type: params.task_type,
        status: TaskStatus::Pending,
        media_path: params.media_path,
        media_name: params.media_name,
        engine_id: params.engine_id,
        model_id: params.model_id,
        source_language: params.source_language,
        target_language: params.target_language,
        translation_content_mode: params.translation_content_mode,
        output_format: params.output_format.unwrap_or_else(|| "srt".into()),
        output_name: params.output_name,
        strip_chinese_punctuation: params.strip_chinese_punctuation,
        review_required: params.review_required,
        max_subtitle_chars: params.max_subtitle_chars,
        reviewed_at: None,
        progress: 0.0,
        status_message: "待处理".into(),
        output_path: None,
        error: None,
        created_at: now.clone(),
        updated_at: now,
    }
}

use std::path::{Path, PathBuf};

pub fn tasks_path(app_config_dir: &Path) -> PathBuf {
    app_config_dir.join("tasks").join("tasks.json")
}

pub fn load_tasks(app_config_dir: &Path) -> Result<HashMap<String, Task>, String> {
    let path = tasks_path(app_config_dir);
    if !path.exists() {
        return Ok(HashMap::new());
    }
    let content = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let tasks: Vec<Task> = serde_json::from_str(&content).map_err(|e| e.to_string())?;
    let mut map = HashMap::new();
    for task in tasks {
        map.insert(task.id.clone(), task);
    }
    Ok(map)
}

pub fn save_tasks(app_config_dir: &Path, tasks: &HashMap<String, Task>) -> Result<(), String> {
    let path = tasks_path(app_config_dir);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let tasks_vec: Vec<&Task> = tasks.values().collect();
    let content = serde_json::to_string_pretty(&tasks_vec).map_err(|e| e.to_string())?;
    let tmp_path = path.with_extension("json.tmp");
    std::fs::write(&tmp_path, content).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp_path, &path).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn insert_tasks_atomically(
    app_config_dir: &Path,
    tasks: &mut HashMap<String, Task>,
    new_tasks: &[Task],
) -> Result<(), String> {
    let mut next = tasks.clone();
    for task in new_tasks {
        next.insert(task.id.clone(), task.clone());
    }
    save_tasks(app_config_dir, &next)?;
    *tasks = next;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_task(name: &str) -> Task {
        create_task(CreateTaskParams {
            task_type: TaskType::GenerateOnly,
            media_path: format!("/tmp/{name}.wav"),
            media_name: format!("{name}.wav"),
            engine_id: "whisper-cpp".into(),
            model_id: "small".into(),
            source_language: Some("auto".into()),
            target_language: None,
            translation_content_mode: TranslationContentMode::TargetOnly,
            output_format: Some("srt".into()),
            output_name: None,
            strip_chinese_punctuation: false,
            review_required: false,
            max_subtitle_chars: 0,
        })
    }

    #[test]
    fn batch_insert_persists_all_tasks_together() {
        let temp = tempfile::tempdir().unwrap();
        let first = sample_task("first");
        let second = sample_task("second");
        let mut tasks = HashMap::new();

        insert_tasks_atomically(temp.path(), &mut tasks, &[first.clone(), second.clone()]).unwrap();

        assert_eq!(tasks.len(), 2);
        let loaded = load_tasks(temp.path()).unwrap();
        assert_eq!(loaded.len(), 2);
        assert!(loaded.contains_key(&first.id));
        assert!(loaded.contains_key(&second.id));
    }

    #[test]
    fn batch_insert_keeps_memory_unchanged_when_persistence_fails() {
        let temp = tempfile::tempdir().unwrap();
        let blocked_config_dir = temp.path().join("not-a-directory");
        std::fs::write(&blocked_config_dir, b"blocked").unwrap();
        let existing = sample_task("existing");
        let incoming = sample_task("incoming");
        let mut tasks = HashMap::from([(existing.id.clone(), existing.clone())]);

        let error = insert_tasks_atomically(
            &blocked_config_dir,
            &mut tasks,
            std::slice::from_ref(&incoming),
        )
        .unwrap_err();

        assert!(!error.is_empty());
        assert_eq!(tasks.len(), 1);
        assert!(tasks.contains_key(&existing.id));
        assert!(!tasks.contains_key(&incoming.id));
    }

    #[test]
    fn legacy_task_without_review_fields_remains_compatible() {
        let task = sample_task("legacy");
        let mut value = serde_json::to_value(task).unwrap();
        let object = value.as_object_mut().unwrap();
        object.remove("review_required");
        object.remove("reviewed_at");
        object.remove("max_subtitle_chars");

        let restored: Task = serde_json::from_value(value).unwrap();

        assert!(!restored.review_required);
        assert!(restored.reviewed_at.is_none());
        assert_eq!(restored.max_subtitle_chars, 0);
    }
}
