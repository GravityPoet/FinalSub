use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum FinalSubError {
    #[error("parse error: {0}")]
    Parse(String),

    #[error("invalid path: {0}")]
    InvalidPath(String),

    #[error("invalid argument: {0}")]
    InvalidArgument(String),

    #[error("task not found: {0}")]
    TaskNotFound(String),

    #[error("model not found: {0}")]
    ModelNotFound(String),

    #[error("engine not ready: {0}")]
    EngineNotReady(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("tauri error: {0}")]
    Tauri(#[from] tauri::Error),
}

pub type Result<T> = std::result::Result<T, FinalSubError>;

impl Serialize for FinalSubError {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}
