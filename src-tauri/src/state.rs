use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};

use crate::core::models::AsrModelInfo;
use crate::core::task_queue::Task;
use crate::core::tts::VoiceProfile;

/// 单个任务 runner 的控制句柄。
/// `stale` 在该 runner 被取消/暂停移除或被 retry/resume 的新 runner 取代时置位；
/// 旧 runner 在检查点看到取消信号后必须先查此标志，过时则静默退出，
/// 不得再写任务状态、断点或控制表，否则会把新一代任务标成已取消并删掉其控制通道。
pub struct TaskControl {
    pub cancel_tx: tokio::sync::watch::Sender<bool>,
    pub stale: Arc<AtomicBool>,
}

pub struct AppState {
    pub tasks: Arc<RwLock<std::collections::HashMap<String, Task>>>,
    pub task_controls: Arc<RwLock<std::collections::HashMap<String, TaskControl>>>,
    pub model_controls:
        Arc<RwLock<std::collections::HashMap<String, tokio::sync::watch::Sender<bool>>>>,
    pub tts_model_controls:
        Arc<RwLock<std::collections::HashMap<String, tokio::sync::watch::Sender<bool>>>>,
    pub burn_controls:
        Arc<RwLock<std::collections::HashMap<String, tokio::sync::oneshot::Sender<()>>>>,
    pub tts_controls: Arc<RwLock<std::collections::HashMap<String, Arc<AtomicBool>>>>,
    pub(crate) tts_worker: crate::core::tts::TtsWorkerManager,
    pub(crate) dubbing_session_io: Arc<Mutex<()>>,
    pub voice_profiles: Arc<RwLock<std::collections::HashMap<String, VoiceProfile>>>,
    pub models: Vec<AsrModelInfo>,
    pub app_config_dir: PathBuf,
    pub task_semaphore: Arc<std::sync::Mutex<Arc<tokio::sync::Semaphore>>>,
    pub power_save: crate::core::power_save::PowerSaveManager,
    pub update_in_progress: AtomicBool,
}

impl AppState {
    pub fn new(app_config_dir: PathBuf) -> Self {
        let mut loaded_tasks = match crate::core::task_queue::load_tasks(&app_config_dir) {
            Ok(tasks) => tasks,
            Err(error) => {
                // tasks.json 损坏时先把原文件改名备份再从空队列启动，
                // 否则下一次保存会用空表直接覆盖用户的全部任务历史且无法找回。
                let tasks_file = crate::core::task_queue::tasks_path(&app_config_dir);
                if tasks_file.exists() {
                    let backup = tasks_file.with_extension(format!(
                        "json.corrupt-{}",
                        chrono::Utc::now().format("%Y%m%d%H%M%S")
                    ));
                    let rename_result = std::fs::rename(&tasks_file, &backup);
                    eprintln!(
                        "[FinalSub] tasks.json 无法解析（{error}），已备份到 {}（rename: {:?}）",
                        backup.display(),
                        rename_result
                    );
                }
                Default::default()
            }
        };
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
            tts_worker: crate::core::tts::TtsWorkerManager::default(),
            dubbing_session_io: Arc::new(Mutex::new(())),
            voice_profiles: Arc::new(RwLock::new(voice_profiles)),
            models: crate::core::models::builtin_model_catalog(),
            app_config_dir,
            task_semaphore,
            power_save,
            update_in_progress: AtomicBool::new(false),
        }
    }
}
