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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub task_type: TaskType,
    pub status: TaskStatus,
    pub media_path: String,
    pub media_name: String,
    pub engine_id: String,
    pub model_id: String,
    pub language: Option<String>,
    pub progress: f32,
    pub status_message: String,
    pub output_path: Option<String>,
    pub error: Option<String>,
    pub created_at: String,
}

pub type TaskMap = Arc<RwLock<HashMap<String, Task>>>;

pub fn create_task(
    task_type: TaskType,
    media_path: String,
    media_name: String,
    engine_id: String,
    model_id: String,
    language: Option<String>,
) -> Task {
    Task {
        id: Uuid::new_v4().to_string(),
        task_type,
        status: TaskStatus::Pending,
        media_path,
        media_name,
        engine_id,
        model_id,
        language,
        progress: 0.0,
        status_message: "待处理".into(),
        output_path: None,
        error: None,
        created_at: chrono::Utc::now().to_rfc3339(),
    }
}
