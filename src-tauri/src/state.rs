use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::core::models::AsrModelInfo;
use crate::core::task_queue::Task;

pub struct AppState {
    pub tasks: Arc<RwLock<std::collections::HashMap<String, Task>>>,
    pub task_controls:
        Arc<RwLock<std::collections::HashMap<String, tokio::sync::watch::Sender<bool>>>>,
    pub model_controls:
        Arc<RwLock<std::collections::HashMap<String, tokio::sync::watch::Sender<bool>>>>,
    pub burn_controls:
        Arc<RwLock<std::collections::HashMap<String, tokio::sync::oneshot::Sender<()>>>>,
    pub models: Vec<AsrModelInfo>,
    pub app_config_dir: PathBuf,
}

impl AppState {
    pub fn new(app_config_dir: PathBuf) -> Self {
        let mut loaded_tasks =
            crate::core::task_queue::load_tasks(&app_config_dir).unwrap_or_default();
        let mut dirty = false;
        for task in loaded_tasks.values_mut() {
            if task.status == crate::core::task_queue::TaskStatus::Pending
                || task.status == crate::core::task_queue::TaskStatus::Running
            {
                task.status = crate::core::task_queue::TaskStatus::Paused;
                task.status_message = "应用上次关闭时未完成，已暂停，可点击继续".into();
                dirty = true;
            }
        }
        if dirty {
            let _ = crate::core::task_queue::save_tasks(&app_config_dir, &loaded_tasks);
        }
        Self {
            tasks: Arc::new(RwLock::new(loaded_tasks)),
            task_controls: Arc::new(RwLock::new(std::collections::HashMap::new())),
            model_controls: Arc::new(RwLock::new(std::collections::HashMap::new())),
            burn_controls: Arc::new(RwLock::new(std::collections::HashMap::new())),
            models: crate::core::models::builtin_model_catalog(),
            app_config_dir,
        }
    }
}
