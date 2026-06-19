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
    pub source_language: Option<String>,
    pub target_language: Option<String>,
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
        output_format: params.output_format.unwrap_or_else(|| "srt".into()),
        progress: 0.0,
        status_message: "待处理".into(),
        output_path: None,
        error: None,
        created_at: now.clone(),
        updated_at: now,
    }
}
