use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::core::models::AsrModelInfo;
use crate::core::task_queue::Task;

pub struct AppState {
    pub tasks: Arc<RwLock<std::collections::HashMap<String, Task>>>,
    pub task_controls:
        Arc<RwLock<std::collections::HashMap<String, tokio::sync::watch::Sender<bool>>>>,
    pub models: Vec<AsrModelInfo>,
    pub app_config_dir: PathBuf,
}

impl AppState {
    pub fn new(app_config_dir: PathBuf) -> Self {
        Self {
            tasks: Arc::new(RwLock::new(std::collections::HashMap::new())),
            task_controls: Arc::new(RwLock::new(std::collections::HashMap::new())),
            models: crate::core::models::builtin_model_catalog(),
            app_config_dir,
        }
    }
}
