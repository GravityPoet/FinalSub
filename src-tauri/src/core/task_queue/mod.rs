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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TranslationContentMode {
    TargetOnly,
    SourceAndTarget,
    TargetAndSource,
}

impl TranslationContentMode {
    pub fn is_bilingual(self) -> bool {
        !matches!(self, TranslationContentMode::TargetOnly)
    }
}

impl Default for TranslationContentMode {
    fn default() -> Self {
        Self::TargetOnly
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
