use std::sync::Arc;
use tokio::sync::RwLock;

use crate::core::models::AsrModelInfo;
use crate::core::task_queue::Task;

pub struct AppState {
    pub tasks: Arc<RwLock<std::collections::HashMap<String, Task>>>,
    pub models: Vec<AsrModelInfo>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            tasks: Arc::new(RwLock::new(std::collections::HashMap::new())),
            models: crate::core::models::builtin_model_catalog(),
        }
    }
}

impl AppState {
    pub fn new() -> Self {
        Self::default()
    }
}
