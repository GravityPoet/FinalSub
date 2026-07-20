use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::core::models::AsrModelInfo;
use crate::core::task_queue::Task;
use crate::core::tts::VoiceProfile;

pub struct AppState {
    pub tasks: Arc<RwLock<std::collections::HashMap<String, Task>>>,
    pub task_controls:
        Arc<RwLock<std::collections::HashMap<String, tokio::sync::watch::Sender<bool>>>>,
    pub model_controls:
        Arc<RwLock<std::collections::HashMap<String, tokio::sync::watch::Sender<bool>>>>,
    pub tts_model_controls:
        Arc<RwLock<std::collections::HashMap<String, tokio::sync::watch::Sender<bool>>>>,
    pub burn_controls:
        Arc<RwLock<std::collections::HashMap<String, tokio::sync::oneshot::Sender<()>>>>,
    pub tts_controls: Arc<RwLock<std::collections::HashMap<String, Arc<AtomicBool>>>>,
    pub tts_engines: crate::core::tts::TtsEngineCache,
    pub voice_profiles: Arc<RwLock<std::collections::HashMap<String, VoiceProfile>>>,
    pub models: Vec<AsrModelInfo>,
    pub app_config_dir: PathBuf,
    pub task_semaphore: Arc<std::sync::Mutex<Arc<tokio::sync::Semaphore>>>,
    pub power_save: crate::core::power_save::PowerSaveManager,
    pub update_in_progress: AtomicBool,
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
                if let Some(pipeline) = task.pipeline.as_mut() {
                    pipeline.prepare_current_stage_for_resume("应用关闭时中断，等待继续");
                }
                dirty = true;
            }
        }
        if dirty {
            let _ = crate::core::task_queue::save_tasks(&app_config_dir, &loaded_tasks);
        }
        let settings = crate::core::settings::load_settings(&app_config_dir)
            .unwrap_or_else(|_| crate::core::settings::system_default_settings());
        let initial_limit = settings.max_concurrent_tasks.max(1) as usize;
        let task_semaphore = Arc::new(std::sync::Mutex::new(Arc::new(
            tokio::sync::Semaphore::new(initial_limit),
        )));
        let power_save =
            crate::core::power_save::PowerSaveManager::new(settings.prevent_sleep_during_tasks);
        crate::core::tts::cleanup_voice_profile_transients(&app_config_dir);
        let voice_profiles =
            crate::core::tts::load_voice_profiles(&app_config_dir).unwrap_or_default();

        Self {
            tasks: Arc::new(RwLock::new(loaded_tasks)),
            task_controls: Arc::new(RwLock::new(std::collections::HashMap::new())),
            model_controls: Arc::new(RwLock::new(std::collections::HashMap::new())),
            tts_model_controls: Arc::new(RwLock::new(std::collections::HashMap::new())),
            burn_controls: Arc::new(RwLock::new(std::collections::HashMap::new())),
            tts_controls: Arc::new(RwLock::new(std::collections::HashMap::new())),
            tts_engines: Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
            voice_profiles: Arc::new(RwLock::new(voice_profiles)),
            models: crate::core::models::builtin_model_catalog(),
            app_config_dir,
            task_semaphore,
            power_save,
            update_in_progress: AtomicBool::new(false),
        }
    }
}
