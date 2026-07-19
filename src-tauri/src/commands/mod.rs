use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::time::Duration;

use tauri::{ipc::Channel, AppHandle, Emitter, Manager, State};
use tauri_plugin_updater::{Update, UpdaterExt};

use crate::core::asr::parakeet::ParakeetNativeEngine;
use crate::core::asr::whisper::WhisperCppEngine;
use crate::core::asr::{AsrEngine, AsrModelRef, TranscribeJob};
use crate::core::audio;
use crate::core::models::{self, AsrModelInfo, ModelStatus};
use crate::core::recipes::{self, SaveTaskRecipeRequest, TaskRecipe};
use crate::core::settings::{self, Settings};
use crate::core::subtitle::SubtitleTrack;
use crate::core::task_queue::{
    self, CreateTaskParams, Task, TaskMap, TaskStatus, TaskType, TranslationContentMode,
};
use crate::core::translation::{self, TranslationProvider};
use crate::core::tts::{
    CloudTtsSynthesisRequest, DubbingEngineSelection, DubbingSession, DubbingSubtitleWriteResult,
    DubbingSynthesizeCueRequest, LocalTtsSynthesisRequest, SaveTtsProviderRequest, TtsModelInfo,
    TtsProviderProfile, TtsSynthesisResult, UpdateDubbingCueRequest,
};
use crate::state::AppState;
use tauri_plugin_fs::FsExt;

const TASK_UPDATED_EVENT: &str = "task-updated";
const TASK_DELETED_EVENT: &str = "task-deleted";
const TRANSLATE_ONLY_SUBTITLE_EXTENSIONS: &[&str] = &["srt", "vtt", "ass", "lrc"];
const MAX_BATCH_TASKS: usize = 10_000;
const RELEASE_LATEST_URL: &str = "https://github.com/GravityPoet/FinalSub/releases/latest";
const RELEASE_LATEST_API_URL: &str =
    "https://api.github.com/repos/GravityPoet/FinalSub/releases/latest";
const UPDATER_MANIFEST_URL: &str =
    "https://github.com/GravityPoet/FinalSub/releases/latest/download/latest.json";
const UPDATER_ASSET_PATH_PREFIX: &str = "/repos/GravityPoet/FinalSub/releases/assets/";

#[derive(serde::Serialize)]
pub struct AppInfo {
    pub version: String,
    pub name: String,
}

#[derive(serde::Serialize, Clone)]
struct TaskDeletedPayload {
    task_id: String,
}

#[tauri::command]
pub fn get_app_info() -> AppInfo {
    AppInfo {
        version: env!("CARGO_PKG_VERSION").into(),
        name: "FinalSub".into(),
    }
}

fn validate_task_id(task_id: &str) -> Result<(), String> {
    uuid::Uuid::parse_str(task_id)
        .map(|_| ())
        .map_err(|_| "Invalid task ID format".to_string())
}

fn task_can_be_deleted(status: TaskStatus) -> bool {
    matches!(
        status,
        TaskStatus::Review
            | TaskStatus::Done
            | TaskStatus::Error
            | TaskStatus::Cancelled
            | TaskStatus::Paused
    )
}

fn task_status_label(status: TaskStatus) -> &'static str {
    match status {
        TaskStatus::Pending => "pending",
        TaskStatus::Running => "running",
        TaskStatus::Paused => "paused",
        TaskStatus::Cancelled => "cancelled",
        TaskStatus::Review => "review",
        TaskStatus::Done => "done",
        TaskStatus::Error => "error",
    }
}

fn prepare_task_for_retry(task: &mut Task) {
    task.status = TaskStatus::Pending;
    task.progress = task.progress.clamp(0.0, 1.0);
    task.error = None;
    task.reviewed_at = None;
    task.status_message = "准备从上次进度继续...".into();
    task.updated_at = chrono::Utc::now().to_rfc3339();
}

async fn persist_tasks_snapshot(app_config_dir: &Path, tasks: &TaskMap) -> Result<(), String> {
    let task_map = tasks.read().await;
    task_queue::save_tasks(app_config_dir, &task_map)
}

async fn cleanup_task_artifacts(app_config_dir: &Path, task_id: &str) {
    let work_dir = app_config_dir.join("tasks").join(task_id);
    if work_dir.exists() {
        let _ = tokio::fs::remove_dir_all(&work_dir).await;
    }

    let log_path = app_config_dir.join("tasks").join(format!("{task_id}.log"));
    if log_path.exists() {
        let _ = tokio::fs::remove_file(log_path).await;
    }
}

async fn delete_tasks_by_ids(
    app: &AppHandle,
    state: &AppState,
    task_ids: Vec<String>,
) -> Result<Vec<String>, String> {
    if task_ids.is_empty() {
        return Err("Please select tasks to delete".into());
    }

    let mut seen = HashSet::new();
    let mut unique_task_ids = Vec::new();
    for task_id in task_ids {
        validate_task_id(&task_id)?;
        if seen.insert(task_id.clone()) {
            unique_task_ids.push(task_id);
        }
    }

    let mut tasks = state.tasks.write().await;
    for task_id in &unique_task_ids {
        let task = tasks
            .get(task_id)
            .ok_or_else(|| format!("task not found: {task_id}"))?;
        if !task_can_be_deleted(task.status) {
            return Err(format!(
                "Task \"{}\" is still {}, please pause or cancel it before deleting",
                task.media_name,
                task_status_label(task.status)
            ));
        }
    }

    for task_id in &unique_task_ids {
        tasks.remove(task_id);
    }
    drop(tasks);

    {
        let mut controls = state.task_controls.write().await;
        for task_id in &unique_task_ids {
            controls.remove(task_id);
        }
    }

    persist_tasks_snapshot(&state.app_config_dir, &state.tasks).await?;

    for task_id in &unique_task_ids {
        cleanup_task_artifacts(&state.app_config_dir, task_id).await;
        emit_task_deleted(app, task_id);
    }

    Ok(unique_task_ids)
}

#[tauri::command]
pub fn list_asr_models(state: State<'_, AppState>) -> Result<Vec<AsrModelInfo>, String> {
    scan_models_for_state(&state)
}

#[tauri::command]
pub fn get_model_status(
    state: State<'_, AppState>,
    model_id: String,
) -> Result<Option<AsrModelInfo>, String> {
    Ok(scan_models_for_state(&state)?
        .into_iter()
        .find(|m| m.id == model_id))
}

#[tauri::command]
pub fn scan_models(state: State<'_, AppState>) -> Result<Vec<AsrModelInfo>, String> {
    scan_models_for_state(&state)
}

#[tauri::command]
pub fn list_tts_models(state: State<'_, AppState>) -> Result<Vec<TtsModelInfo>, String> {
    crate::core::tts::list_models(&state.app_config_dir).map_err(|error| error.to_string())
}

/// 将固定清单中的 TTS 工件下载到受管目录。下载只接受内置模型 ID，
/// 不把前端传入的 URL 当作下载目标；这样“应用内下载”与“选择已有目录”保持清晰边界。
#[tauri::command]
pub async fn download_tts_model(
    app: AppHandle,
    state: State<'_, AppState>,
    model_id: String,
) -> Result<(), String> {
    let normalized = model_id.trim().to_string();
    crate::core::tts::find_spec(&normalized).map_err(|error| error.to_string())?;

    let (cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);
    {
        // 检查与登记必须在同一个写锁内完成，避免两次并发 IPC 都通过
        // contains_key 后启动两个写入同一 `.part`/staging 目录的任务。
        let mut controls = state.tts_model_controls.write().await;
        if controls.contains_key(&normalized) {
            return Err("该 TTS 模型已经在下载队列中".into());
        }
        controls.insert(normalized.clone(), cancel_tx);
    }

    let controls = state.tts_model_controls.clone();
    let cleanup_id = normalized.clone();
    let config_dir = state.app_config_dir.clone();
    let app_for_task = app.clone();
    tauri::async_runtime::spawn(async move {
        let result = crate::core::tts::download_model_impl(
            app_for_task.clone(),
            config_dir,
            normalized.clone(),
            cancel_rx,
        )
        .await;
        controls.write().await.remove(&cleanup_id);
        if let Err(error) = result {
            let _ = app_for_task.emit(
                "model-download-updated",
                crate::core::models::download::ModelDownloadProgress {
                    model_id: normalized,
                    bytes_downloaded: 0,
                    total_bytes: 0,
                    progress: 0.0,
                    status: "error".into(),
                    phase: "error".into(),
                    bytes_per_second: None,
                    eta_seconds: None,
                    error: Some(error.to_string()),
                },
            );
        }
    });

    Ok(())
}

#[tauri::command]
pub async fn cancel_tts_model_download(
    state: State<'_, AppState>,
    model_id: String,
) -> Result<bool, String> {
    let normalized = model_id.trim().to_string();
    crate::core::tts::find_spec(&normalized).map_err(|error| error.to_string())?;
    let controls = state.tts_model_controls.read().await;
    if let Some(sender) = controls.get(&normalized) {
        sender
            .send(true)
            .map_err(|_| "TTS 下载任务已经结束".to_string())?;
        Ok(true)
    } else {
        Ok(false)
    }
}

/// 只删除 FinalSub 受管目录中的 TTS 模型；外部登记路径永远不会被此命令触碰。
#[tauri::command]
pub async fn delete_tts_model(state: State<'_, AppState>, model_id: String) -> Result<(), String> {
    let normalized = model_id.trim().to_string();
    crate::core::tts::find_spec(&normalized).map_err(|error| error.to_string())?;
    if state
        .tts_model_controls
        .read()
        .await
        .contains_key(&normalized)
    {
        return Err("模型正在下载，请先暂停下载再删除".into());
    }
    if !state.tts_controls.read().await.is_empty() {
        return Err("配音正在进行，请等待当前配音任务完成后再删除模型".into());
    }
    // 引擎对象可能仍持有已删除目录的文件句柄；先从缓存移除，避免删除后
    // 下一次请求继续复用旧实例。正在运行的合成已由上面的闸门排除。
    let cache_key_prefix = format!("{normalized}|");
    state
        .tts_engines
        .lock()
        .map_err(|_| "TTS 引擎缓存不可用".to_string())?
        .retain(|key, _| !key.starts_with(&cache_key_prefix));
    crate::core::tts::delete_managed_model(&state.app_config_dir, &normalized)
        .map_err(|error| error.to_string())
}

/// 登记外部 TTS 模型目录。只保存绝对路径，不复制模型，也不会取得源目录删除权。
#[tauri::command]
pub fn register_tts_model_path(
    state: State<'_, AppState>,
    model_id: String,
    source_path: String,
) -> Result<TtsModelInfo, String> {
    crate::core::tts::register_external_model(&state.app_config_dir, &model_id, &source_path)
        .map_err(|error| error.to_string())
}

/// 仅移除外部路径登记；源模型文件永远保留。
#[tauri::command]
pub fn forget_tts_model_path(state: State<'_, AppState>, model_id: String) -> Result<(), String> {
    crate::core::tts::remove_external_registration(&state.app_config_dir, &model_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn set_tts_models_root(
    state: State<'_, AppState>,
    models_root: String,
) -> Result<Vec<TtsModelInfo>, String> {
    crate::core::tts::set_models_root(&state.app_config_dir, &models_root)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn synthesize_local_tts(
    state: State<'_, AppState>,
    generation_id: String,
    request: LocalTtsSynthesisRequest,
) -> Result<TtsSynthesisResult, String> {
    uuid::Uuid::parse_str(&generation_id).map_err(|_| "配音请求 ID 格式无效".to_string())?;
    let model =
        crate::core::tts::resolve_ready_model(&state.app_config_dir, request.model_id.trim())
            .map_err(|error| error.to_string())?;
    let cancelled = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    {
        let mut controls = state.tts_controls.write().await;
        if controls.contains_key(&generation_id) {
            return Err("同一配音请求正在运行".into());
        }
        controls.insert(generation_id.clone(), cancelled.clone());
    }
    let cache = state.tts_engines.clone();
    let joined = tokio::task::spawn_blocking(move || {
        crate::core::tts::synthesize_local(&cache, model, request, cancelled)
    })
    .await;
    state.tts_controls.write().await.remove(&generation_id);
    joined
        .map_err(|error| format!("本地 TTS 工作线程异常：{error}"))?
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn cancel_local_tts(
    state: State<'_, AppState>,
    generation_id: String,
) -> Result<bool, String> {
    uuid::Uuid::parse_str(&generation_id).map_err(|_| "配音请求 ID 格式无效".to_string())?;
    let controls = state.tts_controls.read().await;
    if let Some(cancelled) = controls.get(&generation_id) {
        cancelled.store(true, std::sync::atomic::Ordering::Relaxed);
        Ok(true)
    } else {
        Ok(false)
    }
}

#[tauri::command]
pub fn list_tts_providers(state: State<'_, AppState>) -> Result<Vec<TtsProviderProfile>, String> {
    crate::core::tts::list_providers(&state.app_config_dir).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn save_tts_provider(
    state: State<'_, AppState>,
    request: SaveTtsProviderRequest,
) -> Result<TtsProviderProfile, String> {
    crate::core::tts::save_provider(&state.app_config_dir, request)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn delete_tts_provider(state: State<'_, AppState>, provider_id: String) -> Result<(), String> {
    crate::core::tts::delete_provider(&state.app_config_dir, &provider_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn synthesize_cloud_tts(
    app: AppHandle,
    state: State<'_, AppState>,
    generation_id: String,
    request: CloudTtsSynthesisRequest,
) -> Result<TtsSynthesisResult, String> {
    uuid::Uuid::parse_str(&generation_id).map_err(|_| "配音请求 ID 格式无效".to_string())?;
    let ffmpeg = resolve_sidecar(&app, "ffmpeg")?;
    let cancelled = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    {
        let mut controls = state.tts_controls.write().await;
        if controls.contains_key(&generation_id) {
            return Err("同一配音请求正在运行".into());
        }
        controls.insert(generation_id.clone(), cancelled.clone());
    }
    let result =
        crate::core::tts::synthesize_cloud(&state.app_config_dir, &ffmpeg, request, cancelled)
            .await
            .map_err(|error| error.to_string());
    state.tts_controls.write().await.remove(&generation_id);
    result
}

#[tauri::command]
pub async fn test_tts_provider(
    app: AppHandle,
    state: State<'_, AppState>,
    provider_id: String,
) -> Result<TtsSynthesisResult, String> {
    uuid::Uuid::parse_str(&provider_id).map_err(|_| "TTS 服务实例 ID 无效".to_string())?;
    let generation_id = uuid::Uuid::new_v4().to_string();
    let output_dir = state.app_config_dir.join("tts").join("previews");
    std::fs::create_dir_all(&output_dir).map_err(|error| error.to_string())?;
    let output = output_dir.join(format!("provider-{provider_id}.wav"));
    let ffmpeg = resolve_sidecar(&app, "ffmpeg")?;
    let cancelled = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    state
        .tts_controls
        .write()
        .await
        .insert(generation_id.clone(), cancelled.clone());
    let result = crate::core::tts::synthesize_cloud(
        &state.app_config_dir,
        &ffmpeg,
        CloudTtsSynthesisRequest {
            provider_id,
            text: "Hello，欢迎使用 FinalSub。".into(),
            voice: None,
            speed: Some(1.0),
            output_path: output.to_string_lossy().to_string(),
        },
        cancelled,
    )
    .await
    .map_err(|error| error.to_string());
    state.tts_controls.write().await.remove(&generation_id);
    result
}

#[tauri::command]
pub fn create_dubbing_session(
    state: State<'_, AppState>,
    subtitle_path: String,
    video_path: Option<String>,
) -> Result<DubbingSession, String> {
    crate::core::tts::create_dubbing_session(
        &state.app_config_dir,
        &subtitle_path,
        video_path.as_deref(),
    )
    .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn get_dubbing_session(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<DubbingSession, String> {
    crate::core::tts::get_dubbing_session(&state.app_config_dir, &session_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn update_dubbing_cue(
    state: State<'_, AppState>,
    request: UpdateDubbingCueRequest,
) -> Result<DubbingSession, String> {
    crate::core::tts::update_dubbing_cue(&state.app_config_dir, request)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn export_dubbing_subtitle(
    state: State<'_, AppState>,
    session_id: String,
    output_path: String,
) -> Result<String, String> {
    if !state.tts_controls.read().await.is_empty() {
        return Err("配音合成正在运行，请完成或取消后再导出字幕".into());
    }
    crate::core::tts::export_dubbing_subtitle(&state.app_config_dir, &session_id, &output_path)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn write_back_dubbing_subtitle(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<DubbingSubtitleWriteResult, String> {
    if !state.tts_controls.read().await.is_empty() {
        return Err("配音合成正在运行，请完成或取消后再写回字幕".into());
    }
    crate::core::tts::write_back_dubbing_subtitle(&state.app_config_dir, &session_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn synthesize_dubbing_cue(
    app: AppHandle,
    state: State<'_, AppState>,
    generation_id: String,
    request: DubbingSynthesizeCueRequest,
) -> Result<DubbingSession, String> {
    uuid::Uuid::parse_str(&generation_id).map_err(|_| "配音请求 ID 格式无效".to_string())?;
    let ffmpeg = resolve_sidecar(&app, "ffmpeg")?;
    let cancelled = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    {
        let mut controls = state.tts_controls.write().await;
        if controls.contains_key(&generation_id) {
            return Err("同一配音请求正在运行".into());
        }
        controls.insert(generation_id.clone(), cancelled.clone());
    }

    let prepared = match crate::core::tts::prepare_dubbing_cue(&state.app_config_dir, &request) {
        Ok(prepared) => prepared,
        Err(error) => {
            state.tts_controls.write().await.remove(&generation_id);
            return Err(error.to_string());
        }
    };
    let synthesis_result: Result<TtsSynthesisResult, String> = match &prepared.config.engine {
        DubbingEngineSelection::Local { model_id } => {
            let model = crate::core::tts::resolve_ready_model(&state.app_config_dir, model_id)
                .map_err(|error| error.to_string());
            match model {
                Ok(model) => {
                    let cache = state.tts_engines.clone();
                    let local_request = LocalTtsSynthesisRequest {
                        model_id: model_id.clone(),
                        text: prepared.text.clone(),
                        voice_id: (!prepared.config.voice.is_empty())
                            .then(|| prepared.config.voice.clone()),
                        speed: Some(prepared.config.global_speed),
                        output_path: prepared.output_path.clone(),
                        reference_audio_path: prepared.config.reference_audio_path.clone(),
                        reference_text: prepared.config.reference_text.clone(),
                        num_steps: prepared.config.num_steps,
                    };
                    let local_cancelled = cancelled.clone();
                    let joined = tokio::task::spawn_blocking(move || {
                        crate::core::tts::synthesize_local(
                            &cache,
                            model,
                            local_request,
                            local_cancelled,
                        )
                    })
                    .await;
                    match joined {
                        Ok(result) => result.map_err(|error| error.to_string()),
                        Err(error) => Err(format!("本地 TTS 工作线程异常：{error}")),
                    }
                }
                Err(error) => Err(error),
            }
        }
        DubbingEngineSelection::Cloud { provider_id } => crate::core::tts::synthesize_cloud(
            &state.app_config_dir,
            &ffmpeg,
            CloudTtsSynthesisRequest {
                provider_id: provider_id.clone(),
                text: prepared.text.clone(),
                voice: (!prepared.config.voice.is_empty()).then(|| prepared.config.voice.clone()),
                speed: Some(prepared.config.global_speed),
                output_path: prepared.output_path.clone(),
            },
            cancelled.clone(),
        )
        .await
        .map_err(|error| error.to_string()),
    };

    let result = match synthesis_result {
        Ok(audio) => crate::core::tts::complete_dubbing_cue(
            &state.app_config_dir,
            &ffmpeg,
            &prepared,
            audio.duration_ms,
            cancelled.clone(),
        )
        .await
        .map_err(|error| error.to_string()),
        Err(error) => Err(error),
    };
    if let Err(error) = &result {
        let was_cancelled = cancelled.load(std::sync::atomic::Ordering::Relaxed)
            || error.contains("取消")
            || error.to_ascii_lowercase().contains("cancel");
        let _ = crate::core::tts::fail_dubbing_cue(
            &state.app_config_dir,
            &prepared.session_id,
            prepared.cue_index,
            error,
            was_cancelled,
        );
    }
    state.tts_controls.write().await.remove(&generation_id);
    result
}

#[tauri::command]
pub async fn accept_dubbing_overflow(
    app: AppHandle,
    state: State<'_, AppState>,
    generation_id: String,
    session_id: String,
    cue_index: u32,
) -> Result<DubbingSession, String> {
    uuid::Uuid::parse_str(&generation_id).map_err(|_| "配音请求 ID 格式无效".to_string())?;
    let ffmpeg = resolve_sidecar(&app, "ffmpeg")?;
    let cancelled = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    {
        let mut controls = state.tts_controls.write().await;
        if controls.contains_key(&generation_id) {
            return Err("同一配音请求正在运行".into());
        }
        controls.insert(generation_id.clone(), cancelled.clone());
    }
    let result = crate::core::tts::accept_dubbing_overflow(
        &state.app_config_dir,
        &ffmpeg,
        &session_id,
        cue_index,
        cancelled,
    )
    .await
    .map_err(|error| error.to_string());
    state.tts_controls.write().await.remove(&generation_id);
    result
}

#[tauri::command]
pub async fn export_dubbing_audio(
    app: AppHandle,
    state: State<'_, AppState>,
    generation_id: String,
    session_id: String,
    output_path: String,
) -> Result<DubbingSession, String> {
    uuid::Uuid::parse_str(&generation_id).map_err(|_| "配音请求 ID 格式无效".to_string())?;
    let ffmpeg = resolve_sidecar(&app, "ffmpeg")?;
    let cancelled = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    {
        let mut controls = state.tts_controls.write().await;
        if controls.contains_key(&generation_id) {
            return Err("同一配音请求正在运行".into());
        }
        controls.insert(generation_id.clone(), cancelled.clone());
    }
    let result = crate::core::tts::export_dubbing_audio(
        &state.app_config_dir,
        &ffmpeg,
        &session_id,
        &output_path,
        cancelled,
    )
    .await
    .map_err(|error| error.to_string());
    state.tts_controls.write().await.remove(&generation_id);
    result
}

#[tauri::command]
pub fn discover_batch_inputs(
    paths: Vec<String>,
    task_type: String,
    recursive: Option<bool>,
) -> Result<Vec<String>, String> {
    let kind = if task_type.trim() == "translate-only" {
        crate::core::batch::BatchInputKind::Subtitle
    } else {
        crate::core::batch::BatchInputKind::Media
    };
    crate::core::batch::discover_inputs(&paths, kind, recursive.unwrap_or(true))
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn delete_model(state: State<'_, AppState>, model_id: String) -> Result<(), String> {
    let models_dir = model_storage_dir(&state.app_config_dir, &model_id)?;
    models::delete_managed_model(&models_dir, &model_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn import_local_model(
    state: State<'_, AppState>,
    model_id: String,
    source_path: String,
    expected_sha256: Option<String>,
) -> Result<(), String> {
    let models_dir = whisper_models_dir(&state.app_config_dir)?;
    let source = validate_existing_file_path(&source_path, "Source model file")?;
    let normalized = models::validate_whisper_model_id(&model_id).map_err(|e| e.to_string())?;
    let dest_path = models::whisper_model_path(&models_dir, &normalized);

    if let Some(parent) = dest_path.parent() {
        tokio::fs::create_dir_all(parent).await.ok();
    }

    // 流式计算源文件 SHA256。
    let mut file = tokio::fs::File::open(&source)
        .await
        .map_err(|e| format!("无法打开源文件: {e}"))?;

    use sha2::{Digest, Sha256};
    use tokio::io::AsyncReadExt;

    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 1024 * 1024];

    loop {
        let n = file
            .read(&mut buffer)
            .await
            .map_err(|e| format!("读取源文件失败: {e}"))?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }

    let hash_str = hex::encode(hasher.finalize());

    // 调用方提供期望值时强制校验：不匹配直接拒绝，绝不落盘。
    if let Some(expected) = expected_sha256 {
        let expected = expected.trim().to_lowercase();
        if !expected.is_empty() && expected != hash_str {
            return Err(format!(
                "SHA256 校验失败：期望 {expected}，实际 {hash_str}，已拒绝导入"
            ));
        }
    }

    // 原子落盘：先拷到同目录临时文件，再 rename 覆盖目标，避免写一半留下半个模型。
    let tmp_name = format!(
        "{}.importing",
        dest_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("model.bin")
    );
    let tmp_path = dest_path.with_file_name(tmp_name);
    tokio::fs::copy(&source, &tmp_path)
        .await
        .map_err(|e| format!("拷贝文件失败: {e}"))?;
    if let Err(e) = tokio::fs::rename(&tmp_path, &dest_path).await {
        let _ = tokio::fs::remove_file(&tmp_path).await;
        return Err(format!("原子替换模型文件失败: {e}"));
    }

    println!("导入模型 {normalized} 完成，SHA256: {hash_str}");
    Ok(())
}

/// 原子拷贝：先写同目录临时文件再 rename，避免写一半留下损坏文件。
async fn copy_atomic(src: &Path, dest: &Path) -> Result<(), String> {
    let tmp_name = format!(
        "{}.importing",
        dest.file_name().and_then(|n| n.to_str()).unwrap_or("tmp")
    );
    let tmp = dest.with_file_name(tmp_name);
    tokio::fs::copy(src, &tmp)
        .await
        .map_err(|e| format!("拷贝文件失败: {e}"))?;
    if let Err(e) = tokio::fs::rename(&tmp, dest).await {
        let _ = tokio::fs::remove_file(&tmp).await;
        return Err(format!("原子替换失败: {e}"));
    }
    Ok(())
}

/// 导入 SenseVoice 模型：model.onnx + tokens.txt 落到 models/sensevoice-small/。
#[tauri::command]
pub async fn import_sensevoice_model(
    state: State<'_, AppState>,
    model_onnx_path: String,
    tokens_path: String,
) -> Result<(), String> {
    let models_dir = whisper_models_dir(&state.app_config_dir)?;
    let onnx_src = validate_existing_file_path(&model_onnx_path, "SenseVoice model.onnx")?;
    let tokens_src = validate_existing_file_path(&tokens_path, "SenseVoice tokens.txt")?;
    let target_dir = models_dir.join("sensevoice-small");
    tokio::fs::create_dir_all(&target_dir)
        .await
        .map_err(|e| format!("创建 SenseVoice 模型目录失败: {e}"))?;
    copy_atomic(&onnx_src, &target_dir.join("model.onnx")).await?;
    copy_atomic(&tokens_src, &target_dir.join("tokens.txt")).await?;
    Ok(())
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct EmbeddedSubtitleStream {
    pub sub_index: u32,
    pub codec: String,
    pub language: Option<String>,
}

/// 列出视频内嵌字幕轨（解析 `ffmpeg -i` 的 stderr）。
#[tauri::command]
pub async fn list_embedded_subtitles(
    app: AppHandle,
    video_path: String,
) -> Result<Vec<EmbeddedSubtitleStream>, String> {
    let video_path = validate_media_path(&video_path)?;
    let ffmpeg_path = resolve_sidecar(&app, "ffmpeg")?;
    let output = tokio::process::Command::new(ffmpeg_path)
        .arg("-i")
        .arg(&video_path)
        .output()
        .await
        .map_err(|e| format!("运行 FFmpeg 探测字幕流失败: {e}"))?;
    let stderr = String::from_utf8_lossy(&output.stderr);

    let mut streams = Vec::new();
    let mut sub_index = 0u32;
    for line in stderr.lines() {
        let line = line.trim();
        if !(line.contains("Stream #") && line.contains("Subtitle:")) {
            continue;
        }
        // 语言标签在 `Subtitle:` 之前的括号里，如 `Stream #0:2(eng): Subtitle: ...`；
        // `Subtitle:` 之后的括号（如 `subrip (default)`）不是语言。
        let prefix = line.split("Subtitle:").next().unwrap_or("");
        let language = prefix.find('(').and_then(|o| {
            prefix[o + 1..].find(')').and_then(|c| {
                let lang = prefix[o + 1..o + 1 + c].trim();
                (!lang.is_empty() && lang.len() <= 8).then(|| lang.to_string())
            })
        });
        let codec = line
            .split("Subtitle:")
            .nth(1)
            .and_then(|rest| rest.trim().split([' ', ',']).next())
            .filter(|s| !s.is_empty())
            .unwrap_or("unknown")
            .to_string();
        streams.push(EmbeddedSubtitleStream {
            sub_index,
            codec,
            language,
        });
        sub_index += 1;
    }
    Ok(streams)
}

/// 提取指定内嵌字幕轨为独立字幕文件（输出格式由扩展名决定）。
#[tauri::command]
pub async fn extract_embedded_subtitle(
    app: AppHandle,
    video_path: String,
    sub_index: u32,
    output_path: String,
) -> Result<String, String> {
    let video_path = validate_media_path(&video_path)?;
    let output_path = validate_new_output_path(&output_path, "Subtitle output path")?;
    let ffmpeg_path = resolve_sidecar(&app, "ffmpeg")?;
    let output = tokio::process::Command::new(ffmpeg_path)
        .arg("-y")
        .arg("-i")
        .arg(&video_path)
        .arg("-map")
        .arg(format!("0:s:{sub_index}"))
        .arg(&output_path)
        .output()
        .await
        .map_err(|e| format!("运行 FFmpeg 提取字幕失败: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("提取内嵌字幕失败（流 {sub_index}）: {stderr}"));
    }
    Ok(output_path.to_string_lossy().to_string())
}

#[tauri::command]
pub async fn download_model(
    app: AppHandle,
    state: State<'_, AppState>,
    model_id: String,
) -> Result<(), String> {
    let normalized = match models::validate_whisper_model_id(&model_id) {
        Ok(id) => id,
        Err(e) => return Err(e.to_string()),
    };
    let models_dir = model_storage_dir(&state.app_config_dir, &normalized)?;

    // 检查是否已经在下载中
    {
        let controls = state.model_controls.read().await;
        if controls.contains_key(&normalized) {
            return Err("This model is already in the download queue".to_string());
        }
    }

    let (tx, rx) = tokio::sync::watch::channel(false);
    {
        let mut controls = state.model_controls.write().await;
        controls.insert(normalized.clone(), tx);
    }

    let model_controls = state.model_controls.clone();
    let cleanup_controls = model_controls.clone();
    let cleanup_model_id = normalized.clone();
    let app_clone = app.clone();
    tauri::async_runtime::spawn(async move {
        let result =
            models::download::download_model_impl(app_clone, models_dir, normalized.clone(), rx)
                .await;
        cleanup_controls.write().await.remove(&cleanup_model_id);
        if let Err(error) = result {
            let _ = app.emit(
                "model-download-updated",
                models::download::ModelDownloadProgress {
                    model_id: normalized,
                    bytes_downloaded: 0,
                    total_bytes: 0,
                    progress: 0.0,
                    status: "error".into(),
                    phase: "error".into(),
                    bytes_per_second: None,
                    eta_seconds: None,
                    error: Some(error.to_string()),
                },
            );
        }
    });

    Ok(())
}

#[tauri::command]
pub async fn cancel_model_download(
    state: State<'_, AppState>,
    model_id: String,
) -> Result<(), String> {
    let normalized = models::normalize_whisper_model_id(&model_id);
    let mut controls = state.model_controls.write().await;
    if let Some(sender) = controls.remove(&normalized) {
        let _ = sender.send(true);
    }
    Ok(())
}

#[derive(serde::Deserialize)]
pub struct CreateTaskRequest {
    pub task_type: String,
    pub media_path: String,
    pub engine_id: String,
    pub model_id: String,
    pub source_language: Option<String>,
    pub target_language: Option<String>,
    pub translation_content_mode: Option<String>,
    pub output_format: Option<String>,
    pub output_name: Option<String>,
    pub strip_chinese_punctuation: Option<bool>,
    pub review_required: Option<bool>,
}

fn prepare_task_request(req: CreateTaskRequest) -> Result<Task, String> {
    let task_type = match req.task_type.as_str() {
        "generate-and-translate" => TaskType::GenerateAndTranslate,
        "generate-only" => TaskType::GenerateOnly,
        "translate-only" => TaskType::TranslateOnly,
        _ => return Err(format!("Unknown task type: {}", req.task_type)),
    };

    let media_path = if task_type == TaskType::TranslateOnly {
        validate_existing_file_path(&req.media_path, "Subtitle file")?
    } else {
        validate_media_path(&req.media_path)?
    };

    let (engine_id, model_id) = if task_type == TaskType::TranslateOnly {
        ("subtitle-translation".to_string(), "srt-input".to_string())
    } else {
        (
            validate_non_empty("engine_id", req.engine_id)?,
            validate_non_empty("model_id", req.model_id)?,
        )
    };

    // 如果是 translate-only，校验字幕格式
    if task_type == TaskType::TranslateOnly {
        validate_translate_only_subtitle_extension(&media_path)?;
    }

    let output_format = validate_subtitle_output_format(req.output_format)?;
    let output_name = validate_output_name_template(req.output_name)?;
    let translation_content_mode = validate_translation_content_mode(req.translation_content_mode)?;
    let source_language = validate_source_language_for_engine(&engine_id, req.source_language)?;
    let target_language = match task_type {
        TaskType::GenerateAndTranslate | TaskType::TranslateOnly => Some(validate_non_empty(
            "target_language",
            req.target_language.unwrap_or_default(),
        )?),
        TaskType::GenerateOnly => req
            .target_language
            .map(|lang| lang.trim().to_string())
            .filter(|lang| !lang.is_empty()),
    };

    let media_name = media_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("Unnamed media")
        .to_string();

    Ok(task_queue::create_task(CreateTaskParams {
        task_type,
        media_path: media_path.to_string_lossy().to_string(),
        media_name,
        engine_id,
        model_id,
        source_language,
        target_language,
        translation_content_mode,
        output_format,
        output_name,
        strip_chinese_punctuation: req.strip_chinese_punctuation.unwrap_or(false),
        review_required: req.review_required.unwrap_or(false),
    }))
}

async fn create_tasks_inner(
    app: AppHandle,
    state: &AppState,
    requests: Vec<CreateTaskRequest>,
) -> Result<Vec<Task>, String> {
    if requests.is_empty() {
        return Err("Please provide at least one task".into());
    }
    if requests.len() > MAX_BATCH_TASKS {
        return Err(format!(
            "A single batch can contain at most {MAX_BATCH_TASKS} tasks"
        ));
    }

    // Validate every request before changing memory, disk, controls, or worker state.
    let new_tasks = requests
        .into_iter()
        .map(prepare_task_request)
        .collect::<Result<Vec<_>, _>>()?;

    if let Some(cloud_task) = new_tasks
        .iter()
        .find(|task| task.engine_id == crate::core::asr::cloud::CLOUD_ASR_ENGINE_ID)
    {
        if cloud_task.model_id != crate::core::asr::cloud::CLOUD_ASR_MODEL_ID {
            return Err(format!(
                "Unsupported Cloud ASR model reference: {}",
                cloud_task.model_id
            ));
        }
        let settings = settings::load_settings(&state.app_config_dir)
            .map_err(|error| format!("Failed to load Cloud ASR settings: {error}"))?;
        validate_cloud_asr_readiness(&settings)?;
    }

    {
        let mut tasks = state.tasks.write().await;
        task_queue::insert_tasks_atomically(&state.app_config_dir, &mut tasks, &new_tasks)?;
    }

    let mut starts = Vec::with_capacity(new_tasks.len());
    {
        let mut controls = state.task_controls.write().await;
        for task in &new_tasks {
            let (cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);
            controls.insert(task.id.clone(), cancel_tx);
            starts.push((task.id.clone(), cancel_rx));
        }
    }

    for task in &new_tasks {
        emit_task_update(&app, task);
    }
    for (task_id, cancel_rx) in starts {
        crate::core::task_runner::start_task(
            app.clone(),
            state.tasks.clone(),
            state.task_controls.clone(),
            state.app_config_dir.clone(),
            task_id,
            cancel_rx,
        );
    }

    Ok(new_tasks)
}

#[tauri::command]
pub async fn create_task(
    app: AppHandle,
    state: State<'_, AppState>,
    req: CreateTaskRequest,
) -> Result<Task, String> {
    create_tasks_inner(app, &state, vec![req])
        .await?
        .into_iter()
        .next()
        .ok_or_else(|| "Task creation returned no task".to_string())
}

#[tauri::command]
pub async fn create_tasks(
    app: AppHandle,
    state: State<'_, AppState>,
    requests: Vec<CreateTaskRequest>,
) -> Result<Vec<Task>, String> {
    create_tasks_inner(app, &state, requests).await
}

#[tauri::command]
pub async fn create_preview_task(
    app: AppHandle,
    state: State<'_, AppState>,
    media_path: String,
) -> Result<Task, String> {
    let media_path = validate_media_path(&media_path)?;
    let media_name = media_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("Unnamed media")
        .to_string();

    let task = task_queue::create_task(CreateTaskParams {
        task_type: TaskType::GenerateOnly,
        media_path: media_path.to_string_lossy().to_string(),
        media_name,
        engine_id: "preview-pipeline".into(),
        model_id: "ffmpeg-sidecar-probe".into(),
        source_language: None,
        target_language: None,
        translation_content_mode: TranslationContentMode::TargetOnly,
        output_format: None,
        output_name: None,
        strip_chinese_punctuation: false,
        review_required: false,
    });
    let task_clone = task.clone();
    state.tasks.write().await.insert(task.id.clone(), task);
    if let Err(error) = persist_tasks_snapshot(&state.app_config_dir, &state.tasks).await {
        state.tasks.write().await.remove(&task_clone.id);
        return Err(error);
    }
    emit_task_update(&app, &task_clone);
    start_preview_worker(app, state.tasks.clone(), task_clone.id.clone());
    Ok(task_clone)
}

#[tauri::command]
pub async fn list_tasks(state: State<'_, AppState>) -> Result<Vec<Task>, String> {
    Ok(state.tasks.read().await.values().cloned().collect())
}

#[tauri::command]
pub fn list_task_recipes(state: State<'_, AppState>) -> Result<Vec<TaskRecipe>, String> {
    recipes::load_recipes(&state.app_config_dir)
}

#[tauri::command]
pub fn save_task_recipe(
    state: State<'_, AppState>,
    request: SaveTaskRecipeRequest,
) -> Result<TaskRecipe, String> {
    recipes::save_recipe(&state.app_config_dir, request)
}

#[tauri::command]
pub fn delete_task_recipe(state: State<'_, AppState>, recipe_id: String) -> Result<String, String> {
    recipes::delete_recipe(&state.app_config_dir, &recipe_id)
}

async fn approve_tasks_by_ids(
    app: &AppHandle,
    state: &AppState,
    task_ids: Vec<String>,
) -> Result<Vec<Task>, String> {
    if task_ids.is_empty() {
        return Err("Please select tasks to approve".into());
    }

    let mut seen = HashSet::new();
    let mut unique_task_ids = Vec::new();
    for task_id in task_ids {
        validate_task_id(&task_id)?;
        if seen.insert(task_id.clone()) {
            unique_task_ids.push(task_id);
        }
    }

    let reviewed_at = chrono::Utc::now().to_rfc3339();
    let mut tasks = state.tasks.write().await;
    let (next, approved) = approve_review_tasks(&tasks, &unique_task_ids, &reviewed_at)?;

    task_queue::save_tasks(&state.app_config_dir, &next)?;
    *tasks = next;
    drop(tasks);

    for task in &approved {
        emit_task_update(app, task);
    }
    Ok(approved)
}

fn approve_review_tasks(
    tasks: &HashMap<String, Task>,
    task_ids: &[String],
    reviewed_at: &str,
) -> Result<(HashMap<String, Task>, Vec<Task>), String> {
    let mut next = tasks.clone();
    for task_id in task_ids {
        let task = next
            .get(task_id)
            .ok_or_else(|| format!("task not found: {task_id}"))?;
        if task.status != TaskStatus::Review {
            return Err(format!(
                "Task \"{}\" is {}, only review tasks can be approved",
                task.media_name,
                task_status_label(task.status)
            ));
        }
    }

    let mut approved = Vec::with_capacity(task_ids.len());
    for task_id in task_ids {
        let task = next
            .get_mut(task_id)
            .ok_or_else(|| format!("task not found: {task_id}"))?;
        task.status = TaskStatus::Done;
        task.status_message = "审核通过".into();
        task.reviewed_at = Some(reviewed_at.to_string());
        task.updated_at = reviewed_at.to_string();
        approved.push(task.clone());
    }
    Ok((next, approved))
}

#[tauri::command]
pub async fn approve_task(
    app: AppHandle,
    state: State<'_, AppState>,
    task_id: String,
) -> Result<Task, String> {
    approve_tasks_by_ids(&app, &state, vec![task_id])
        .await?
        .into_iter()
        .next()
        .ok_or_else(|| "Task approval returned no task".to_string())
}

#[tauri::command]
pub async fn approve_tasks(
    app: AppHandle,
    state: State<'_, AppState>,
    task_ids: Vec<String>,
) -> Result<Vec<Task>, String> {
    approve_tasks_by_ids(&app, &state, task_ids).await
}

#[tauri::command]
pub async fn delete_task(
    app: AppHandle,
    state: State<'_, AppState>,
    task_id: String,
) -> Result<String, String> {
    let mut deleted = delete_tasks_by_ids(&app, &state, vec![task_id]).await?;
    Ok(deleted.remove(0))
}

#[tauri::command]
pub async fn delete_tasks(
    app: AppHandle,
    state: State<'_, AppState>,
    task_ids: Vec<String>,
) -> Result<Vec<String>, String> {
    delete_tasks_by_ids(&app, &state, task_ids).await
}

#[tauri::command]
pub async fn cancel_task(
    app: AppHandle,
    state: State<'_, AppState>,
    task_id: String,
) -> Result<Task, String> {
    validate_task_id(&task_id)?;
    let mut tasks = state.tasks.write().await;
    let task = tasks
        .get_mut(&task_id)
        .ok_or_else(|| format!("task not found: {task_id}"))?;
    if matches!(
        task.status,
        TaskStatus::Review | TaskStatus::Done | TaskStatus::Error
    ) {
        return Ok(task.clone());
    }
    task.status = task_queue::TaskStatus::Cancelled;
    task.progress = task.progress.clamp(0.0, 1.0);
    task.status_message = "Cancelled".into();
    task.updated_at = chrono::Utc::now().to_rfc3339();
    let task_clone = task.clone();
    drop(tasks);

    // 向 cancel_sender 发送取消信号并移除
    if let Some(sender) = state.task_controls.write().await.remove(&task_id) {
        sender.send(true).ok();
    }

    persist_tasks_snapshot(&state.app_config_dir, &state.tasks).await?;
    emit_task_update(&app, &task_clone);
    Ok(task_clone)
}

#[tauri::command]
pub async fn pause_task(
    app: AppHandle,
    state: State<'_, AppState>,
    task_id: String,
) -> std::result::Result<Task, String> {
    validate_task_id(&task_id)?;
    let mut tasks = state.tasks.write().await;
    let task = tasks
        .get_mut(&task_id)
        .ok_or_else(|| format!("task not found: {task_id}"))?;
    if task.status != TaskStatus::Running && task.status != TaskStatus::Pending {
        return Err("Only running or pending tasks can be paused".to_string());
    }
    task.status = TaskStatus::Paused;
    task.status_message = "Paused".into();
    task.updated_at = chrono::Utc::now().to_rfc3339();
    let task_clone = task.clone();
    drop(tasks);

    // Send signal to runner watch channel and remove control handle
    if let Some(sender) = state.task_controls.write().await.remove(&task_id) {
        let _ = sender.send(true);
    }

    persist_tasks_snapshot(&state.app_config_dir, &state.tasks).await?;
    emit_task_update(&app, &task_clone);

    let app_config_dir = state.app_config_dir.clone();
    let app_clone = app.clone();
    let task_id_clone = task_id.clone();
    tauri::async_runtime::spawn(async move {
        crate::core::task_runner::write_task_log(
            &app_clone,
            &app_config_dir,
            &task_id_clone,
            "User paused the task",
        )
        .await;
    });

    Ok(task_clone)
}

#[tauri::command]
pub async fn resume_task(
    app: AppHandle,
    state: State<'_, AppState>,
    task_id: String,
) -> std::result::Result<Task, String> {
    validate_task_id(&task_id)?;
    let mut tasks = state.tasks.write().await;
    let task = tasks
        .get_mut(&task_id)
        .ok_or_else(|| format!("task not found: {task_id}"))?;
    if task.status != TaskStatus::Paused {
        return Err("Only paused tasks can be resumed".to_string());
    }
    task.status = TaskStatus::Pending;
    task.status_message = "Preparing to resume...".into();
    task.updated_at = chrono::Utc::now().to_rfc3339();
    let task_clone = task.clone();
    drop(tasks);

    persist_tasks_snapshot(&state.app_config_dir, &state.tasks).await?;
    let (cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);
    state
        .task_controls
        .write()
        .await
        .insert(task_id.clone(), cancel_tx);
    emit_task_update(&app, &task_clone);

    let app_config_dir = state.app_config_dir.clone();
    let app_clone = app.clone();
    let task_id_clone = task_id.clone();
    tauri::async_runtime::spawn(async move {
        crate::core::task_runner::write_task_log(
            &app_clone,
            &app_config_dir,
            &task_id_clone,
            "User resumed the task",
        )
        .await;
    });

    let tasks_clone = state.tasks.clone();
    let task_controls_clone = state.task_controls.clone();
    let app_config_dir_clone = state.app_config_dir.clone();
    crate::core::task_runner::start_task(
        app,
        tasks_clone,
        task_controls_clone,
        app_config_dir_clone,
        task_clone.id.clone(),
        cancel_rx,
    );

    Ok(task_clone)
}

#[tauri::command]
pub async fn retry_task(
    app: AppHandle,
    state: State<'_, AppState>,
    task_id: String,
) -> std::result::Result<Task, String> {
    validate_task_id(&task_id)?;

    let mut tasks = state.tasks.write().await;
    let task = tasks
        .get_mut(&task_id)
        .ok_or_else(|| format!("task not found: {task_id}"))?;
    if !matches!(task.status, TaskStatus::Error | TaskStatus::Cancelled) {
        return Err("Only failed or cancelled tasks can be retried".to_string());
    }
    prepare_task_for_retry(task);
    let task_clone = task.clone();
    drop(tasks);

    persist_tasks_snapshot(&state.app_config_dir, &state.tasks).await?;
    let (cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);
    state
        .task_controls
        .write()
        .await
        .insert(task_id.clone(), cancel_tx);
    emit_task_update(&app, &task_clone);

    let app_config_dir = state.app_config_dir.clone();
    let app_clone = app.clone();
    let task_id_clone = task_id.clone();
    tauri::async_runtime::spawn(async move {
        crate::core::task_runner::write_task_log(
            &app_clone,
            &app_config_dir,
            &task_id_clone,
            "User retried the task",
        )
        .await;
    });

    let tasks_clone = state.tasks.clone();
    let task_controls_clone = state.task_controls.clone();
    let app_config_dir_clone = state.app_config_dir.clone();
    crate::core::task_runner::start_task(
        app,
        tasks_clone,
        task_controls_clone,
        app_config_dir_clone,
        task_clone.id.clone(),
        cancel_rx,
    );

    Ok(task_clone)
}

#[tauri::command]
pub async fn get_task_logs(
    state: State<'_, AppState>,
    task_id: String,
) -> std::result::Result<String, String> {
    validate_task_id(&task_id)?;
    let log_path = state
        .app_config_dir
        .join("tasks")
        .join(format!("{}.log", task_id));
    if !log_path.exists() {
        return Ok(String::new());
    }
    tokio::fs::read_to_string(&log_path)
        .await
        .map_err(|e| format!("Failed to read log file: {}", e))
}

#[tauri::command]
pub fn normalize_srt(srt_content: String) -> Result<String, String> {
    let track = SubtitleTrack::from_srt(&srt_content).map_err(|e| e.to_string())?;
    Ok(track.to_srt())
}

#[tauri::command]
pub fn extract_audio_plan(video_path: String, output_path: String) -> audio::AudioExtractPlan {
    audio::audio_extract_plan("ffmpeg-sidecar", &video_path, &output_path)
}

#[tauri::command]
pub async fn extract_audio(
    app: AppHandle,
    video_path: String,
    output_path: String,
) -> Result<String, String> {
    let video_path = validate_media_path(&video_path)?;
    let output_path = validate_new_output_path(&output_path, "Audio output path")?;
    let args = audio::extract_audio_args(
        &video_path.to_string_lossy(),
        &output_path.to_string_lossy(),
    );

    let ffmpeg_path = resolve_sidecar(&app, "ffmpeg")?;
    let output = tokio::process::Command::new(ffmpeg_path)
        .args(&args)
        .output()
        .await
        .map_err(|e| format!("Failed to run FFmpeg: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("FFmpeg audio extraction failed: {stderr}"));
    }

    Ok(output_path.to_string_lossy().to_string())
}

#[derive(serde::Deserialize)]
pub struct BurnSubtitleRequest {
    pub video_path: String,
    pub subtitle_path: String,
    pub output_path: String,
    pub font_name: Option<String>,
    pub font_size: Option<u32>,
    pub font_color: Option<String>,
    pub outline_color: Option<String>,
    pub outline_width: Option<f32>,
    pub shadow: Option<f32>,
    pub background_color: Option<String>,
    pub opaque_background: Option<bool>,
    pub alignment: Option<u8>,
    pub margin_v: Option<u32>,
    pub crf: Option<u8>,
    pub preset: Option<String>,
    pub soft_subtitle: Option<bool>,
    pub audio_path: Option<String>,
    pub audio_mode: Option<String>,
    pub subtitle_language: Option<String>,
    pub subtitle_title: Option<String>,
    pub audio_language: Option<String>,
    pub audio_title: Option<String>,
}

#[tauri::command]
pub async fn burn_subtitle(
    app: AppHandle,
    state: State<'_, AppState>,
    req: BurnSubtitleRequest,
) -> Result<String, String> {
    let video_path = validate_media_path(&req.video_path)?;
    let subtitle_path = validate_existing_file_path(&req.subtitle_path, "Subtitle file")?;
    let output_path = validate_new_output_path(&req.output_path, "Video output path")?;
    let burn_id = output_path.to_string_lossy().to_string();
    let audio_mode = audio::ComposeAudioMode::parse(req.audio_mode.as_deref())?;
    let audio_path = match audio_mode {
        audio::ComposeAudioMode::Keep => None,
        audio::ComposeAudioMode::Replace
        | audio::ComposeAudioMode::Mix
        | audio::ComposeAudioMode::AddTrack => {
            let path = validate_existing_file_path(
                req.audio_path.as_deref().unwrap_or_default(),
                "Audio file",
            )?;
            validate_audio_input_extension(&path)?;
            Some(path)
        }
    };
    validate_subtitle_input_extension(&subtitle_path)?;
    validate_compose_output_extension(&output_path)?;
    validate_burn_style(&req)?;
    let style = audio::BurnInStyleOptions {
        font_name: req.font_name,
        font_size: req.font_size,
        font_color: req.font_color,
        outline_color: req.outline_color,
        outline_width: req.outline_width,
        shadow: req.shadow,
        background_color: req.background_color,
        opaque_background: req.opaque_background,
        alignment: req.alignment,
        margin_v: req.margin_v,
        crf: req.crf,
        preset: req.preset,
    };
    let ffmpeg_path = resolve_sidecar(&app, "ffmpeg")?;
    let original_audio_tracks = if matches!(
        audio_mode,
        audio::ComposeAudioMode::Mix | audio::ComposeAudioMode::AddTrack
    ) {
        probe_audio_track_count(&ffmpeg_path, &video_path).await?
    } else {
        0
    };
    let subtitle_language =
        validate_track_language("Subtitle language", req.subtitle_language.as_deref())?
            .or_else(|| Some("und".into()));
    let subtitle_title = validate_track_title("Subtitle title", req.subtitle_title.as_deref())?
        .or_else(|| Some("FinalSub Subtitles".into()));
    let audio_language = validate_track_language("Audio language", req.audio_language.as_deref())?
        .or_else(|| Some("und".into()));
    let audio_title = validate_track_title("Audio title", req.audio_title.as_deref())?
        .or_else(|| Some("FinalSub Dub".into()));
    let options = audio::ComposeOptions {
        soft_subtitle: req.soft_subtitle.unwrap_or(false),
        audio_mode,
        audio_path: audio_path.map(|path| path.to_string_lossy().to_string()),
        subtitle_language,
        subtitle_title,
        audio_language,
        audio_title,
        original_audio_tracks,
    };
    let args = audio::compose_args(
        &video_path.to_string_lossy(),
        &subtitle_path.to_string_lossy(),
        &output_path.to_string_lossy(),
        &style,
        &options,
    )?;

    let mut child = tokio::process::Command::new(ffmpeg_path)
        .args(&args)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to start FFmpeg: {e}"))?;

    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "Unable to get FFmpeg error stream".to_string())?;
    let reader = tokio::io::BufReader::new(stderr);

    let (cancel_tx, mut cancel_rx) = tokio::sync::oneshot::channel::<()>();
    {
        let mut controls = state.burn_controls.write().await;
        if controls.contains_key(&burn_id) {
            let _ = child.kill().await;
            return Err("A burn task is already running for this output path".to_string());
        }
        controls.insert(burn_id.clone(), cancel_tx);
    }

    let app_handle_clone = app.clone();
    let video_path_clone = req.video_path.clone();
    let burn_id_clone = burn_id.clone();
    let output_path_clone = output_path.clone();

    let mut total_duration_ms: Option<u64> = None;

    let result = tokio::select! {
        _ = &mut cancel_rx => {
            let _ = child.kill().await;
            Err("Subtitle burning cancelled".to_string())
        }
        res = async {
            use tokio::io::AsyncBufReadExt;
            let mut stderr_tail = std::collections::VecDeque::with_capacity(40);
            let mut lines_stream = reader.lines();
            while let Ok(Some(line)) = lines_stream.next_line().await {
                if stderr_tail.len() == 40 {
                    stderr_tail.pop_front();
                }
                stderr_tail.push_back(line.clone());
                if let Some(duration_ms) = audio::parse_duration_ms(&line) {
                    total_duration_ms = Some(duration_ms);
                }

                if let Some(time_ms) = audio::parse_current_time_ms(&line) {
                    if let Some(total_ms) = total_duration_ms {
                        let progress =
                            (time_ms as f64 / total_ms as f64 * 100.0).clamp(0.0, 100.0);
                        #[derive(serde::Serialize, Clone)]
                        struct BurnProgress {
                            burn_id: String,
                            video_path: String,
                            progress: f64,
                        }
                        let _ = app_handle_clone.emit(
                            "subtitle-burn-updated",
                            BurnProgress {
                                burn_id: burn_id_clone.clone(),
                                video_path: video_path_clone.clone(),
                                progress,
                            }
                        );
                    }
                }
            }

            let status = child.wait().await.map_err(|e| format!("Failed to wait for FFmpeg: {e}"))?;
            if status.success() {
                #[derive(serde::Serialize, Clone)]
                struct BurnProgress {
                    burn_id: String,
                    video_path: String,
                    progress: f64,
                }
                let _ = app_handle_clone.emit(
                    "subtitle-burn-updated",
                    BurnProgress {
                        burn_id: burn_id_clone.clone(),
                        video_path: video_path_clone.clone(),
                        progress: 100.0,
                    }
                );
                Ok(output_path_clone.to_string_lossy().to_string())
            } else {
                let details = stderr_tail.into_iter().collect::<Vec<_>>().join("\n");
                if details.trim().is_empty() {
                    Err("FFmpeg execution failed without diagnostic output".to_string())
                } else {
                    Err(format!("FFmpeg execution failed:\n{details}"))
                }
            }
        } => res
    };

    state.burn_controls.write().await.remove(&burn_id);

    if result.is_err() && output_path.exists() {
        let _ = std::fs::remove_file(&output_path);
    }

    result
}

#[tauri::command]
pub async fn cancel_burn_subtitle(
    state: State<'_, AppState>,
    burn_id: String,
) -> Result<(), String> {
    if let Some(cancel_tx) = state.burn_controls.write().await.remove(&burn_id) {
        let _ = cancel_tx.send(());
    }
    Ok(())
}

async fn probe_audio_track_count(
    ffmpeg_path: &std::path::Path,
    video_path: &std::path::Path,
) -> Result<usize, String> {
    let output = tokio::process::Command::new(ffmpeg_path)
        .arg("-i")
        .arg(video_path)
        .output()
        .await
        .map_err(|error| format!("Failed to inspect source audio tracks: {error}"))?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    Ok(stderr
        .lines()
        .filter(|line| line.contains("Stream #") && line.contains("Audio:"))
        .count())
}

#[derive(serde::Serialize)]
pub struct VideoMetadata {
    pub duration_seconds: f64,
    pub duration_string: String,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub codec: String,
    pub audio_codec: Option<String>,
    pub audio_sample_rate: Option<u32>,
    pub audio_channels: Option<u32>,
    pub audio_tracks: u32,
}

#[tauri::command]
pub async fn get_video_metadata(
    app: AppHandle,
    video_path: String,
) -> Result<VideoMetadata, String> {
    let video_path = validate_media_path(&video_path)?;
    let ffmpeg_path = resolve_sidecar(&app, "ffmpeg")?;

    let output = tokio::process::Command::new(ffmpeg_path)
        .arg("-i")
        .arg(&video_path)
        .output()
        .await
        .map_err(|e| format!("Failed to run FFmpeg for metadata: {e}"))?;

    let stderr = String::from_utf8_lossy(&output.stderr);

    let mut duration_seconds = 0.0;
    let mut duration_string = "00:00".to_string();
    let mut width = 0;
    let mut height = 0;
    let mut fps = 0.0;
    let mut codec = "unknown".to_string();
    let mut audio_codec: Option<String> = None;
    let mut audio_sample_rate: Option<u32> = None;
    let mut audio_channels: Option<u32> = None;
    let mut audio_tracks = 0;

    for line in stderr.lines() {
        if let Some(pos) = line.find("Duration: ") {
            let dur_part = &line[pos + 10..];
            if let Some(comma_pos) = dur_part.find(',') {
                let dur_str = dur_part[..comma_pos].trim();
                duration_string = dur_str.split('.').next().unwrap_or("00:00").to_string();
                if let Some(duration_ms) = audio::parse_duration_ms(line) {
                    duration_seconds = duration_ms as f64 / 1000.0;
                }
            }
        }

        if line.contains("Stream #") && line.contains("Video:") {
            if let Some(video_pos) = line.find("Video: ") {
                let codec_part = &line[video_pos + 7..];
                if let Some(space_pos) = codec_part.find(' ') {
                    codec = codec_part[..space_pos].trim_end_matches(',').to_string();
                }
            }

            for token in line.split(',') {
                let token = token.trim();
                if let Some(x_pos) = token.find('x') {
                    let left = &token[..x_pos];
                    let right = &token[x_pos + 1..];
                    let right_clean = right.split_whitespace().next().unwrap_or("");
                    if let (Ok(w), Ok(h)) = (left.parse::<u32>(), right_clean.parse::<u32>()) {
                        if w > 0 && h > 0 {
                            width = w;
                            height = h;
                        }
                    }
                }
                if token.ends_with("fps") || token.contains(" fps") {
                    let fps_part = token.split_whitespace().next().unwrap_or("");
                    if let Ok(f) = fps_part.parse::<f64>() {
                        fps = f;
                    }
                }
            }
        }

        if line.contains("Stream #") && line.contains("Audio:") {
            audio_tracks += 1;
            if let Some(audio_pos) = line.find("Audio: ") {
                let audio_part = &line[audio_pos + 7..];
                if let Some(comma_pos) = audio_part.find(',') {
                    let codec_name = audio_part[..comma_pos].trim().to_string();
                    if audio_codec.is_none() {
                        audio_codec = Some(codec_name);
                    }
                }
            }

            for token in line.split(',') {
                let token = token.trim();
                if token.contains("Hz") {
                    let hz_part = token.split_whitespace().next().unwrap_or("");
                    if let Ok(hz) = hz_part.parse::<u32>() {
                        if audio_sample_rate.is_none() {
                            audio_sample_rate = Some(hz);
                        }
                    }
                }
                if token.contains("stereo") {
                    if audio_channels.is_none() {
                        audio_channels = Some(2);
                    }
                } else if token.contains("mono") {
                    if audio_channels.is_none() {
                        audio_channels = Some(1);
                    }
                } else if token.contains("channels") || token.contains("channel") {
                    let ch_part = token.split_whitespace().next().unwrap_or("");
                    if let Ok(ch) = ch_part.parse::<u32>() {
                        if audio_channels.is_none() {
                            audio_channels = Some(ch);
                        }
                    }
                }
            }
        }
    }

    Ok(VideoMetadata {
        duration_seconds,
        duration_string,
        width,
        height,
        fps,
        codec,
        audio_codec,
        audio_sample_rate,
        audio_channels,
        audio_tracks,
    })
}

#[tauri::command]
pub async fn generate_subtitle_preview(
    app: AppHandle,
    req: BurnSubtitleRequest,
) -> Result<String, String> {
    use tauri_plugin_opener::OpenerExt;

    let video_path = validate_media_path(&req.video_path)?;
    let subtitle_path = validate_existing_file_path(&req.subtitle_path, "Subtitle file")?;

    // Generate preview output path in system temp directory
    let temp_dir = std::env::temp_dir();
    let preview_filename = format!("finalsub-preview-{}.mp4", uuid::Uuid::new_v4());
    let preview_path = temp_dir.join(preview_filename);

    validate_burn_style(&req)?;
    let style = audio::BurnInStyleOptions {
        font_name: req.font_name,
        font_size: req.font_size,
        font_color: req.font_color,
        outline_color: req.outline_color,
        outline_width: req.outline_width,
        shadow: req.shadow,
        background_color: req.background_color,
        opaque_background: req.opaque_background,
        alignment: req.alignment,
        margin_v: req.margin_v,
        crf: req.crf,
        preset: req.preset,
    };

    let mut args = audio::burn_in_args(
        &video_path.to_string_lossy(),
        &subtitle_path.to_string_lossy(),
        &preview_path.to_string_lossy(),
        &style,
    );

    // Insert "-t" "10" before the last argument to limit to 10 seconds
    let len = args.len();
    if len >= 1 {
        args.insert(len - 1, "-t".to_string());
        args.insert(len - 1, "10".to_string());
    }

    let ffmpeg_path = resolve_sidecar(&app, "ffmpeg")?;
    let output = tokio::process::Command::new(ffmpeg_path)
        .args(&args)
        .output()
        .await
        .map_err(|e| format!("Failed to generate preview video: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "Failed to generate preview video, FFmpeg error: {stderr}"
        ));
    }

    // Open preview video
    let preview_path_str = preview_path.to_string_lossy().to_string();
    app.opener()
        .open_path(preview_path_str.clone(), None::<String>)
        .map_err(|e| format!("Failed to open preview video: {e}"))?;

    Ok(preview_path_str)
}

#[derive(serde::Deserialize)]
pub struct TranscribeRequest {
    pub audio_path: String,
    pub output_path: String,
    pub model_id: String,
    pub language: Option<String>,
}

#[tauri::command]
pub async fn transcribe_audio(
    app: AppHandle,
    state: State<'_, AppState>,
    req: TranscribeRequest,
) -> Result<String, String> {
    let whisper_bin = resolve_sidecar(&app, "whisper-cli")?;
    let models_dir = whisper_models_dir(&state.app_config_dir)?;
    let audio_path = validate_existing_file_path(&req.audio_path, "Audio file")?;
    let output_path = validate_new_output_path(&req.output_path, "Subtitle output path")?;

    let engine = WhisperCppEngine::new(whisper_bin, models_dir, Default::default());
    let model_ref = AsrModelRef {
        engine_id: "whisper-cpp".into(),
        model_id: req.model_id.clone(),
        model_path: None,
    };

    engine
        .prepare(&model_ref)
        .await
        .map_err(|e: crate::error::FinalSubError| e.to_string())?;

    let (tx, _rx) = tokio::sync::mpsc::channel(32);
    let job = TranscribeJob {
        audio_path: audio_path.to_string_lossy().to_string(),
        output_path: output_path.to_string_lossy().to_string(),
        language: req.language,
        model: model_ref,
    };

    let track = engine
        .transcribe(job, tx, None)
        .await
        .map_err(|e: crate::error::FinalSubError| e.to_string())?;

    let srt = track.to_srt();
    tokio::fs::write(&output_path, &srt)
        .await
        .map_err(|e: std::io::Error| format!("Failed to write SRT: {e}"))?;

    Ok(output_path.to_string_lossy().to_string())
}

#[derive(serde::Deserialize)]
pub struct TranscribeParakeetRequest {
    pub audio_path: String,
    pub output_path: String,
    pub language: Option<String>,
}

#[tauri::command]
pub async fn transcribe_parakeet(
    _app: AppHandle,
    state: State<'_, AppState>,
    req: TranscribeParakeetRequest,
) -> Result<String, String> {
    let audio_path = validate_existing_file_path(&req.audio_path, "Audio file")?;
    let output_path = validate_new_output_path(&req.output_path, "Subtitle output path")?;
    let models_dir = parakeet_models_dir(&state.app_config_dir)?;
    let engine = ParakeetNativeEngine::new(models_dir);
    let model_ref = AsrModelRef {
        engine_id: "parakeet-mlx".into(),
        model_id: "parakeet-tdt-0.6b-v2".into(),
        model_path: None,
    };

    engine
        .prepare(&model_ref)
        .await
        .map_err(|e: crate::error::FinalSubError| e.to_string())?;

    let (tx, _rx) = tokio::sync::mpsc::channel(32);
    let job = TranscribeJob {
        audio_path: audio_path.to_string_lossy().to_string(),
        output_path: output_path.to_string_lossy().to_string(),
        language: req.language.or_else(|| Some("en".into())),
        model: model_ref,
    };

    let track = engine
        .transcribe(job, tx, None)
        .await
        .map_err(|e: crate::error::FinalSubError| e.to_string())?;

    let srt = track.to_srt();
    tokio::fs::write(&output_path, &srt)
        .await
        .map_err(|e: std::io::Error| format!("Failed to write SRT: {e}"))?;

    Ok(output_path.to_string_lossy().to_string())
}

#[tauri::command]
pub fn list_translation_providers() -> Vec<TranslationProvider> {
    translation::builtin_providers()
}

#[tauri::command]
pub async fn list_translation_models(
    app: AppHandle,
    provider_id: String,
    endpoint: String,
    custom_headers: Option<std::collections::HashMap<String, String>>,
) -> Result<Vec<String>, String> {
    let provider = translation::builtin_providers()
        .into_iter()
        .find(|provider| provider.id == provider_id)
        .ok_or_else(|| format!("Unknown translation provider: {provider_id}"))?;
    if !provider.requires_model {
        return Err(format!("{} does not use selectable models", provider.name));
    }
    let state = app.state::<AppState>();
    let settings =
        settings::load_settings(&state.app_config_dir).map_err(|error| error.to_string())?;
    let resolved_endpoint = if endpoint.trim().is_empty() {
        settings
            .translate_endpoints
            .get(&provider_id)
            .cloned()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| provider.default_endpoint.clone())
    } else {
        endpoint.trim().to_string()
    };
    let mut secret_fields = std::collections::HashMap::new();
    for field in &provider.secret_fields {
        if let Some(secret) =
            crate::core::secrets::get_provider_secret(&provider_id, &resolved_endpoint, field)?
        {
            secret_fields.insert(field.clone(), secret);
        }
    }
    let request = translation::TranslateRequest {
        text: String::new(),
        source_language: "auto".into(),
        target_language: "en".into(),
        provider: provider_id,
        api_key: secret_fields.get("apiKey").cloned(),
        api_url: Some(resolved_endpoint),
        model_name: None,
        secret_fields: (!secret_fields.is_empty()).then_some(secret_fields),
        system_prompt: None,
        user_prompt: None,
        proxy_url: settings.proxy_enabled.then_some(settings.proxy_url),
        custom_headers: custom_headers
            .or_else(|| settings.translate_custom_headers.get(&provider.id).cloned()),
        custom_body: settings.translate_custom_body.get(&provider.id).cloned(),
        structured_output: None,
        response_json_schema: None,
        glossary_prompt: None,
    };
    translation::list_provider_models(&request)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn test_translation(
    app: AppHandle,
    mut req: translation::TranslateRequest,
) -> Result<translation::TranslateResponse, String> {
    let state = app.state::<AppState>();
    if let Ok(settings) = crate::core::settings::load_settings(&state.app_config_dir) {
        if req.api_url.is_none() || req.api_url.as_deref().unwrap_or("").is_empty() {
            req.api_url = settings.translate_endpoints.get(&req.provider).cloned();
        }
        if req.model_name.is_none() || req.model_name.as_deref().unwrap_or("").is_empty() {
            req.model_name = settings.translate_models.get(&req.provider).cloned();
        }
        if req.system_prompt.is_none() {
            req.system_prompt = settings
                .translate_system_prompts
                .get(&req.provider)
                .cloned();
        }
        if req.user_prompt.is_none() {
            req.user_prompt = settings.translate_user_prompts.get(&req.provider).cloned();
        }
        if req.custom_headers.is_none() {
            req.custom_headers = settings
                .translate_custom_headers
                .get(&req.provider)
                .cloned();
        }
        if req.custom_body.is_none() {
            req.custom_body = settings.translate_custom_body.get(&req.provider).cloned();
        }
        if req.proxy_url.is_none() && settings.proxy_enabled {
            req.proxy_url = Some(settings.proxy_url);
        }
    }

    let provider_info = translation::builtin_providers()
        .into_iter()
        .find(|p| p.id == req.provider);
    if let Some(p) = provider_info {
        if (req.api_url.is_none() || req.api_url.as_deref().unwrap_or("").trim().is_empty())
            && !p.default_endpoint.trim().is_empty()
        {
            req.api_url = Some(p.default_endpoint.clone());
        }

        let mut secret_map = req.secret_fields.take().unwrap_or_default();
        for field in &p.secret_fields {
            if !secret_map.contains_key(field) {
                if let Some(secret) = crate::core::secrets::get_provider_secret(
                    &req.provider,
                    req.api_url.as_deref().unwrap_or_default(),
                    field,
                )? {
                    secret_map.insert(field.clone(), secret);
                }
            }
        }
        if req.api_key.is_none() {
            req.api_key = secret_map.get("apiKey").cloned();
        }
        if !secret_map.is_empty() {
            req.secret_fields = Some(secret_map);
        }
    }

    translation::translate_text(&req)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn test_translation_proxy(
    proxy_url: String,
    target_url: String,
) -> Result<String, String> {
    translation::test_proxy_connection(&proxy_url, &target_url)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn set_provider_secret(
    provider_id: String,
    endpoint: String,
    field: String,
    value: String,
) -> std::result::Result<(), String> {
    crate::core::secrets::set_provider_secret(&provider_id, &endpoint, &field, &value)
}

/// 仅返回「该 provider 字段是否已配置密钥」，绝不把明文密钥经 IPC 回传渲染层。
/// 翻译时由后端 test_translation 直接从 Keychain 取用，前端无需接触明文。
#[tauri::command]
pub fn has_provider_secret(
    provider_id: String,
    endpoint: String,
    field: String,
) -> std::result::Result<bool, String> {
    crate::core::secrets::has_provider_secret(&provider_id, &endpoint, &field)
}

#[tauri::command]
pub fn delete_provider_secret(
    provider_id: String,
    endpoint: String,
    field: String,
) -> std::result::Result<(), String> {
    crate::core::secrets::delete_provider_secret(&provider_id, &endpoint, &field)
}

#[tauri::command]
pub async fn get_ffmpeg_version(app: AppHandle) -> Result<String, String> {
    let ffmpeg_path = resolve_sidecar(&app, "ffmpeg")?;
    let output = tokio::process::Command::new(ffmpeg_path)
        .args(["-version"])
        .output()
        .await
        .map_err(|e| format!("Failed to run FFmpeg sidecar: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("FFmpeg sidecar returned error: {stderr}"));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let first_line = stdout.lines().next().unwrap_or("unknown");
    Ok(first_line.to_string())
}

fn validate_media_path(raw: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(raw.trim());
    if path.as_os_str().is_empty() {
        return Err("Please select a media file".into());
    }
    if !path.is_absolute() {
        return Err("Media file path must be an absolute path".into());
    }
    if !path.is_file() {
        return Err(format!("Media file does not exist: {}", path.display()));
    }
    Ok(path)
}

fn validate_non_empty(name: &str, value: String) -> Result<String, String> {
    let value = value.trim().to_string();
    if value.is_empty() {
        Err(format!("{name} cannot be empty"))
    } else {
        Ok(value)
    }
}

fn start_preview_worker(app: AppHandle, tasks: TaskMap, task_id: String) {
    tauri::async_runtime::spawn(async move {
        let steps = [
            (0.12, "Added to task queue"),
            (0.28, "FFmpeg sidecar checked"),
            (0.45, "Audio extraction plan prepared"),
            (0.64, "Recognition engine reserved"),
            (0.82, "Subtitle writer prepared"),
            (1.0, "Preview task completed"),
        ];

        for (progress, message) in steps {
            tokio::time::sleep(Duration::from_millis(450)).await;

            let task = {
                let mut task_map = tasks.write().await;
                let Some(task) = task_map.get_mut(&task_id) else {
                    return;
                };

                if task.status == TaskStatus::Cancelled {
                    task.clone()
                } else {
                    task.status = if progress >= 1.0 {
                        TaskStatus::Done
                    } else {
                        TaskStatus::Running
                    };
                    task.progress = progress;
                    task.status_message = message.into();
                    task.updated_at = chrono::Utc::now().to_rfc3339();
                    task.clone()
                }
            };

            emit_task_update(&app, &task);
            if task.status == TaskStatus::Cancelled {
                return;
            }
        }
    });
}

fn update_state_semaphore(state: &AppState, limit: u32) {
    let new_limit = limit.max(1) as usize;
    let mut lock = state.task_semaphore.lock().unwrap();
    *lock = std::sync::Arc::new(tokio::sync::Semaphore::new(new_limit));
}

fn emit_task_update(app: &AppHandle, task: &Task) {
    let _ = app.emit(TASK_UPDATED_EVENT, task.clone());
    if let Some(state) = app.try_state::<AppState>() {
        let app_config_dir = state.app_config_dir.clone();
        let tasks = state.tasks.clone();
        tauri::async_runtime::spawn(async move {
            let task_map = tasks.read().await;
            let _ = crate::core::task_queue::save_tasks(&app_config_dir, &task_map);
        });
    }
}

fn emit_task_deleted(app: &AppHandle, task_id: &str) {
    let _ = app.emit(
        TASK_DELETED_EVENT,
        TaskDeletedPayload {
            task_id: task_id.to_string(),
        },
    );
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Result<Settings, String> {
    settings::load_settings(&state.app_config_dir).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn save_settings_cmd(
    state: State<'_, AppState>,
    new_settings: Settings,
) -> Result<Settings, String> {
    settings::save_settings(&state.app_config_dir, &new_settings).map_err(|e| e.to_string())?;
    // 并发数变更对之后新建的任务生效：保存时重建信号量，在飞任务持旧 permit 不受影响
    update_state_semaphore(&state, new_settings.max_concurrent_tasks);
    crate::set_telemetry_enabled(new_settings.enable_telemetry);
    Ok(new_settings)
}

#[tauri::command]
pub fn reset_settings(state: State<'_, AppState>) -> Result<Settings, String> {
    let new_settings =
        settings::reset_settings(&state.app_config_dir).map_err(|e| e.to_string())?;
    update_state_semaphore(&state, new_settings.max_concurrent_tasks);
    crate::set_telemetry_enabled(new_settings.enable_telemetry);
    Ok(new_settings)
}

#[tauri::command]
pub fn export_config(state: State<'_, AppState>) -> Result<String, String> {
    settings::export_config(&state.app_config_dir).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn import_config(state: State<'_, AppState>, json: String) -> Result<Settings, String> {
    let new_settings =
        settings::import_config(&state.app_config_dir, &json).map_err(|e| e.to_string())?;
    update_state_semaphore(&state, new_settings.max_concurrent_tasks);
    crate::set_telemetry_enabled(new_settings.enable_telemetry);
    Ok(new_settings)
}

#[tauri::command]
pub fn export_config_to_path(
    state: State<'_, AppState>,
    output_path: String,
) -> Result<String, String> {
    let path = validate_json_output_path(&output_path)?;
    let json = settings::export_config(&state.app_config_dir).map_err(|e| e.to_string())?;
    let tmp_path = path.with_extension("json.tmp");
    std::fs::write(&tmp_path, json).map_err(|e| format!("Failed to write config: {e}"))?;
    std::fs::rename(&tmp_path, &path).map_err(|e| format!("Failed to save config: {e}"))?;
    Ok(path.to_string_lossy().to_string())
}

#[tauri::command]
pub fn import_config_from_path(
    state: State<'_, AppState>,
    input_path: String,
) -> Result<Settings, String> {
    let path = validate_json_input_path(&input_path)?;
    let json = std::fs::read_to_string(&path).map_err(|e| format!("Failed to read config: {e}"))?;
    let new_settings =
        settings::import_config(&state.app_config_dir, &json).map_err(|e| e.to_string())?;
    update_state_semaphore(&state, new_settings.max_concurrent_tasks);
    crate::set_telemetry_enabled(new_settings.enable_telemetry);
    Ok(new_settings)
}

#[tauri::command]
pub fn export_encrypted_config_to_path(
    state: State<'_, AppState>,
    output_path: String,
    passphrase: String,
) -> Result<String, String> {
    let path = validate_json_output_path(&output_path)?;
    let encrypted = settings::export_encrypted_config(&state.app_config_dir, &passphrase)
        .map_err(|error| error.to_string())?;
    let tmp_path = path.with_extension("json.tmp");
    std::fs::write(&tmp_path, encrypted)
        .map_err(|error| format!("Failed to write encrypted config: {error}"))?;
    if let Err(error) = std::fs::rename(&tmp_path, &path) {
        let _ = std::fs::remove_file(&tmp_path);
        return Err(format!("Failed to save encrypted config: {error}"));
    }
    Ok(path.to_string_lossy().to_string())
}

#[tauri::command]
pub fn import_encrypted_config_from_path(
    state: State<'_, AppState>,
    input_path: String,
    passphrase: String,
) -> Result<Settings, String> {
    let path = validate_json_input_path(&input_path)?;
    let encrypted = std::fs::read_to_string(&path)
        .map_err(|error| format!("Failed to read encrypted config: {error}"))?;
    let new_settings =
        settings::import_encrypted_config(&state.app_config_dir, &encrypted, &passphrase)
            .map_err(|error| error.to_string())?;
    update_state_semaphore(&state, new_settings.max_concurrent_tasks);
    crate::set_telemetry_enabled(new_settings.enable_telemetry);
    Ok(new_settings)
}

fn scan_models_for_state(state: &AppState) -> Result<Vec<AsrModelInfo>, String> {
    let whisper_dir = whisper_models_dir(&state.app_config_dir)?;
    let parakeet_dir = parakeet_models_dir(&state.app_config_dir)?;
    let mut catalog = state.models.clone();
    models::scan_model_status(&mut catalog, &whisper_dir, &parakeet_dir);
    let settings = settings::load_settings(&state.app_config_dir).map_err(|e| e.to_string())?;
    if let Some(model) = catalog
        .iter_mut()
        .find(|model| model.engine_id == crate::core::asr::cloud::CLOUD_ASR_ENGINE_ID)
    {
        let protocol = crate::core::asr::cloud::parse_protocol(&settings.cloud_asr_protocol)
            .map_err(|error| error.to_string())?;
        model.name = format!(
            "Cloud ASR · {} · {}",
            protocol.display_name(),
            settings.cloud_asr_model.trim()
        );
        model.status = if !settings.cloud_asr_upload_consent {
            ModelStatus::NotReady
        } else {
            let mut ready = true;
            let mut keychain_error = false;
            for field in protocol.required_secret_fields() {
                match crate::core::secrets::has_provider_secret(
                    protocol.secret_provider(),
                    &settings.cloud_asr_endpoint,
                    field,
                ) {
                    Ok(configured) => ready &= configured,
                    Err(_) => keychain_error = true,
                }
            }
            if keychain_error {
                ModelStatus::Error("Unable to read the system Keychain".into())
            } else if ready {
                ModelStatus::Downloaded
            } else {
                ModelStatus::NotReady
            }
        };
    }
    Ok(catalog)
}

fn validate_cloud_asr_readiness(settings: &Settings) -> Result<(), String> {
    if !settings.cloud_asr_upload_consent {
        return Err(
            "Cloud ASR is not ready: enable explicit audio upload consent in Models first".into(),
        );
    }
    crate::core::asr::cloud::validate_service_settings(
        &settings.cloud_asr_protocol,
        &settings.cloud_asr_endpoint,
        &settings.cloud_asr_model,
        settings.cloud_asr_timeout_seconds,
        settings.cloud_asr_retry_times,
        settings.cloud_asr_request_concurrency,
        settings.cloud_asr_request_interval_ms,
    )
    .map_err(|error| error.to_string())?;
    let secret_provider =
        crate::core::asr::cloud::secret_provider_for_protocol(&settings.cloud_asr_protocol)
            .map_err(|error| error.to_string())?;
    let protocol = crate::core::asr::cloud::parse_protocol(&settings.cloud_asr_protocol)
        .map_err(|error| error.to_string())?;
    for field in protocol.required_secret_fields() {
        if !crate::core::secrets::has_provider_secret(
            secret_provider,
            &settings.cloud_asr_endpoint,
            field,
        )? {
            return Err(format!(
                "Cloud ASR is not ready: save {field} for the current protocol and endpoint in Models"
            ));
        }
    }
    Ok(())
}
pub(crate) fn whisper_models_dir(app_config_dir: &Path) -> Result<PathBuf, String> {
    let settings = settings::load_settings(app_config_dir).map_err(|e| e.to_string())?;
    validated_model_root(&settings.models_path, "Model")
}

pub(crate) fn parakeet_models_dir(app_config_dir: &Path) -> Result<PathBuf, String> {
    let settings = settings::load_settings(app_config_dir).map_err(|e| e.to_string())?;
    validated_model_root(&settings.parakeet_models_path, "Parakeet model")
}

fn model_storage_dir(app_config_dir: &Path, model_id: &str) -> Result<PathBuf, String> {
    let normalized = models::validate_whisper_model_id(model_id).map_err(|e| e.to_string())?;
    let model = models::builtin_model_catalog()
        .into_iter()
        .find(|model| model.id == normalized)
        .ok_or_else(|| format!("Unknown model ID: {normalized}"))?;
    if model.engine_id == "parakeet-mlx" {
        parakeet_models_dir(app_config_dir)
    } else {
        whisper_models_dir(app_config_dir)
    }
}

fn validated_model_root(raw: &str, label: &str) -> Result<PathBuf, String> {
    let path = expand_home_path(raw);
    if !path.is_absolute() {
        return Err(format!("{label} path must be an absolute path"));
    }
    Ok(path)
}

fn expand_home_path(raw: &str) -> PathBuf {
    let trimmed = raw.trim();
    if let Some(rest) = trimmed.strip_prefix("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home).join(rest);
        }
    }
    PathBuf::from(trimmed)
}

fn validate_existing_file_path(raw: &str, label: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(raw.trim());
    if path.as_os_str().is_empty() {
        return Err(format!("{label} cannot be empty"));
    }
    if !path.is_absolute() {
        return Err(format!("{label} must be an absolute path"));
    }
    if !path.is_file() {
        return Err(format!("{label} does not exist: {}", path.display()));
    }
    Ok(path)
}

fn validate_new_output_path(raw: &str, label: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(raw.trim());
    if path.as_os_str().is_empty() {
        return Err(format!("{label} cannot be empty"));
    }
    if !path.is_absolute() {
        return Err(format!("{label} must be an absolute path"));
    }
    let parent = path
        .parent()
        .ok_or_else(|| format!("{label} lacks a parent directory"))?;
    if !parent.is_dir() {
        return Err(format!(
            "{label} parent directory does not exist: {}",
            parent.display()
        ));
    }
    if path.exists() {
        return Err(format!(
            "{label} already exists. To avoid overwriting, please choose another path: {}",
            path.display()
        ));
    }
    Ok(path)
}

fn validate_json_output_path(raw: &str) -> Result<PathBuf, String> {
    let path = validate_new_output_path(raw, "Config export path")?;
    validate_json_extension(&path)?;
    Ok(path)
}

fn validate_json_input_path(raw: &str) -> Result<PathBuf, String> {
    let path = validate_existing_file_path(raw, "Config file")?;
    validate_json_extension(&path)?;
    Ok(path)
}

fn validate_json_extension(path: &Path) -> Result<(), String> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if ext != "json" {
        return Err("Config file must be a .json file".into());
    }
    Ok(())
}

fn validate_subtitle_output_format(raw: Option<String>) -> Result<Option<String>, String> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    let format = raw.trim().to_ascii_lowercase();
    if format.is_empty() {
        return Ok(None);
    }
    match format.as_str() {
        "srt" | "vtt" | "txt" | "lrc" | "ass" => Ok(Some(format)),
        _ => Err("Output format only supports srt, vtt, txt, lrc, ass".into()),
    }
}

fn validate_output_name_template(raw: Option<String>) -> Result<Option<String>, String> {
    let Some(value) = raw.map(|value| value.trim().to_string()) else {
        return Ok(None);
    };
    if value.is_empty() {
        return Ok(None);
    }
    if value.len() > 180 {
        return Err("Output name template is too long (maximum 180 bytes)".into());
    }
    if value
        .chars()
        .any(|character| character.is_control() || matches!(character, '/' | '\\' | ':'))
    {
        return Err(
            "Output name must not contain path separators, colons, or control characters".into(),
        );
    }
    if matches!(value.as_str(), "." | "..") {
        return Err("Output name is invalid".into());
    }
    Ok(Some(value))
}

fn validate_translate_only_subtitle_extension(path: &Path) -> Result<(), String> {
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if TRANSLATE_ONLY_SUBTITLE_EXTENSIONS.contains(&ext.as_str()) {
        return Ok(());
    }
    Err(format!(
        "Translate-only mode only supports subtitle inputs: {}",
        TRANSLATE_ONLY_SUBTITLE_EXTENSIONS
            .iter()
            .map(|ext| format!(".{ext}"))
            .collect::<Vec<_>>()
            .join(", ")
    ))
}

fn validate_source_language_for_engine(
    engine_id: &str,
    raw: Option<String>,
) -> Result<Option<String>, String> {
    let language = raw
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    if engine_id == "parakeet-mlx"
        && language
            .as_deref()
            .is_some_and(|value| !matches!(value, "auto" | "en" | "english"))
    {
        return Err("Parakeet v2 only supports English transcription (auto or en)".into());
    }
    if engine_id == "paraformer"
        && language
            .as_deref()
            .is_some_and(|value| !matches!(value, "auto" | "zh" | "chinese"))
    {
        return Err("Paraformer Zh only supports Chinese transcription (auto or zh)".into());
    }
    if engine_id == "firered-asr"
        && language.as_deref().is_some_and(|value| {
            !matches!(value, "auto" | "zh" | "chinese" | "en" | "english" | "yue")
        })
    {
        return Err("FireRedASR2 CTC supports Chinese, English, and Chinese dialects".into());
    }

    Ok(language)
}

fn validate_translation_content_mode(
    raw: Option<String>,
) -> Result<TranslationContentMode, String> {
    let Some(raw) = raw else {
        return Ok(TranslationContentMode::TargetOnly);
    };
    let mode = raw.trim();
    if mode.is_empty() {
        return Ok(TranslationContentMode::TargetOnly);
    }
    match mode {
        "target-only" | "onlyTranslate" => Ok(TranslationContentMode::TargetOnly),
        "source-and-target" | "sourceAndTranslate" => {
            Ok(TranslationContentMode::SourceAndTarget)
        }
        "target-and-source" | "translateAndSource" => {
            Ok(TranslationContentMode::TargetAndSource)
        }
        _ => Err(
            "Translation content mode only supports target-only, source-and-target, target-and-source"
                .into(),
        ),
    }
}

fn validate_burn_style(req: &BurnSubtitleRequest) -> Result<(), String> {
    if let Some(font_name) = req.font_name.as_deref() {
        let valid = !font_name.trim().is_empty()
            && font_name.len() <= 128
            && font_name.chars().all(|character| {
                !character.is_control() && !matches!(character, ',' | '\'' | '\\')
            });
        if !valid {
            return Err("Subtitle font name contains unsupported characters".into());
        }
    }
    if let Some(font_size) = req.font_size {
        if !(10..=120).contains(&font_size) {
            return Err("Subtitle font size must be between 10 and 120".into());
        }
    }
    if let Some(margin_v) = req.margin_v {
        if margin_v > 1_000 {
            return Err("Subtitle vertical margin cannot exceed 1000".into());
        }
    }
    if let Some(ref color) = req.font_color {
        validate_ass_color("Font color", color)?;
    }
    if let Some(ref color) = req.outline_color {
        validate_ass_color("Outline color", color)?;
    }
    if let Some(ref color) = req.background_color {
        validate_ass_color("Background color", color)?;
    }
    if req
        .outline_width
        .is_some_and(|value| !value.is_finite() || !(0.0..=10.0).contains(&value))
    {
        return Err("Subtitle outline width must be between 0 and 10".into());
    }
    if req
        .shadow
        .is_some_and(|value| !value.is_finite() || !(0.0..=20.0).contains(&value))
    {
        return Err("Subtitle shadow must be between 0 and 20".into());
    }
    if req.alignment.is_some_and(|value| !(1..=9).contains(&value)) {
        return Err("Subtitle alignment must be between 1 and 9".into());
    }
    if req.crf.is_some_and(|value| value > 51) {
        return Err("Video CRF must be between 0 and 51".into());
    }
    if let Some(preset) = req.preset.as_deref() {
        if !matches!(
            preset,
            "ultrafast"
                | "superfast"
                | "veryfast"
                | "faster"
                | "fast"
                | "medium"
                | "slow"
                | "slower"
                | "veryslow"
        ) {
            return Err("Unsupported video encoding preset".into());
        }
    }
    Ok(())
}

fn validate_subtitle_input_extension(path: &std::path::Path) -> Result<(), String> {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if matches!(extension.as_str(), "srt" | "ass" | "ssa" | "vtt") {
        Ok(())
    } else {
        Err("Subtitle file must use .srt, .ass, .ssa, or .vtt".into())
    }
}

fn validate_audio_input_extension(path: &std::path::Path) -> Result<(), String> {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if matches!(
        extension.as_str(),
        "wav" | "mp3" | "m4a" | "aac" | "flac" | "ogg" | "opus"
    ) {
        Ok(())
    } else {
        Err("Audio file must use WAV, MP3, M4A, AAC, FLAC, OGG, or Opus".into())
    }
}

fn validate_compose_output_extension(path: &std::path::Path) -> Result<(), String> {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if matches!(extension.as_str(), "mp4" | "m4v" | "mov" | "mkv") {
        Ok(())
    } else {
        Err("Video output must use MP4, M4V, MOV, or MKV".into())
    }
}

fn validate_track_language(label: &str, value: Option<&str>) -> Result<Option<String>, String> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    if value.len() > 16
        || !value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '-')
    {
        return Err(format!(
            "{label} must be an ISO language tag containing only letters, numbers, or hyphens"
        ));
    }
    Ok(Some(value.to_ascii_lowercase()))
}

fn validate_track_title(label: &str, value: Option<&str>) -> Result<Option<String>, String> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    if value.len() > 128 || value.chars().any(char::is_control) {
        return Err(format!("{label} must be at most 128 visible characters"));
    }
    Ok(Some(value.to_string()))
}

fn validate_ass_color(label: &str, value: &str) -> Result<(), String> {
    let valid = value.len() == 10
        && value.starts_with("&H")
        && value[2..].chars().all(|c| c.is_ascii_hexdigit());
    if valid {
        Ok(())
    } else {
        Err(format!(
            "{label} must use ASS color format, e.g. &H00FFFFFF"
        ))
    }
}

pub(crate) fn resolve_sidecar(_app: &tauri::AppHandle, name: &str) -> Result<PathBuf, String> {
    #[cfg(debug_assertions)]
    {
        if let Ok(current_dir) = std::env::current_dir() {
            let target_triple = if cfg!(target_arch = "aarch64") {
                "aarch64-apple-darwin"
            } else {
                "x86_64-apple-darwin"
            };
            let file_name = format!("{name}-{target_triple}");

            let path1 = current_dir
                .join("src-tauri")
                .join("binaries")
                .join(&file_name);
            if path1.exists() {
                return Ok(path1);
            }
            let path2 = current_dir.join("binaries").join(&file_name);
            if path2.exists() {
                return Ok(path2);
            }
        }

        if let Ok(exe_path) = std::env::current_exe() {
            let target_triple = if cfg!(target_arch = "aarch64") {
                "aarch64-apple-darwin"
            } else {
                "x86_64-apple-darwin"
            };
            let file_name = format!("{name}-{target_triple}");

            let mut current = exe_path.as_path();
            for _ in 0..10 {
                if let Some(parent) = current.parent() {
                    let path1 = parent.join("src-tauri").join("binaries").join(&file_name);
                    if path1.exists() {
                        return Ok(path1);
                    }
                    let path2 = parent.join("binaries").join(&file_name);
                    if path2.exists() {
                        return Ok(path2);
                    }
                    current = parent;
                } else {
                    break;
                }
            }
        }

        Err(format!(
            "Could not find sidecar binary in development environment: {}",
            name
        ))
    }

    #[cfg(not(debug_assertions))]
    {
        let exe_path = std::env::current_exe()
            .map_err(|e| format!("Failed to get current executable path: {e}"))?;
        let exe_dir = exe_path
            .parent()
            .ok_or_else(|| "Unable to get directory containing executable".to_string())?;

        let base_name = PathBuf::from(name)
            .file_name()
            .ok_or_else(|| format!("Invalid sidecar name: {name}"))?
            .to_os_string();

        let target_path = exe_dir.join(&base_name);
        if target_path.exists() {
            Ok(target_path)
        } else {
            Err(format!(
                "Could not find sidecar binary in production environment: {}",
                target_path.display()
            ))
        }
    }
}

#[tauri::command]
pub fn load_proofread_tasks(app: AppHandle) -> std::result::Result<String, String> {
    let state = app.state::<AppState>();
    let path = state.app_config_dir.join("proofread_tasks.json");
    if !path.exists() {
        return Ok("[]".to_string());
    }
    std::fs::read_to_string(&path).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn save_proofread_tasks(app: AppHandle, data: String) -> std::result::Result<(), String> {
    let state = app.state::<AppState>();
    let path = state.app_config_dir.join("proofread_tasks.json");
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let tmp_path = path.with_extension("json.tmp");
    std::fs::write(&tmp_path, &data).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp_path, &path).map_err(|e| e.to_string())?;
    Ok(())
}

/// 判断 canonicalize 之后的路径是否落在敏感目录内（纵深防御黑名单）。
/// canonicalize 已解析符号链接，可挡住软链逃逸。
fn is_sensitive_dir(path: &Path) -> bool {
    let p = path.to_string_lossy();
    // 系统级目录：一律拒绝授权
    const SYSTEM_PREFIXES: [&str; 8] = [
        "/etc", "/var", "/usr", "/bin", "/sbin", "/System", "/private", "/Library",
    ];
    for sys in SYSTEM_PREFIXES {
        if p == sys || p.starts_with(&format!("{sys}/")) {
            return true;
        }
    }
    // 用户 home 下的敏感子目录（密钥、凭据、应用私有配置）
    if let Ok(home) = std::env::var("HOME") {
        if !home.is_empty() {
            const HOME_SENSITIVE: [&str; 7] = [
                ".ssh", ".aws", ".gnupg", ".config", ".docker", ".kube", "Library",
            ];
            for sub in HOME_SENSITIVE {
                let banned = format!("{home}/{sub}");
                if p == banned || p.starts_with(&format!("{banned}/")) {
                    return true;
                }
            }
        }
    }
    false
}

/// 受控的运行时 scope 授权命令：把「用户主动导入的字幕/视频所在目录」加入
/// tauri-plugin-fs 的允许范围，使前端 plugin-fs 能读取该文件并扫描同目录字幕。
/// 与已删除的裸 fs_* 命令本质不同——本命令不直接读写任何文件，只做最小授权：
/// 传文件则授权其父目录、传目录则授权自身，均非递归，并用 is_sensitive_dir
/// 黑名单挡住敏感路径。dialog 选中的文件/文件夹已由 tauri-plugin-dialog 自动授权。
#[tauri::command]
pub fn authorize_subtitle_directory(
    app: AppHandle,
    dir_path: String,
) -> std::result::Result<(), String> {
    let canonical = std::fs::canonicalize(&dir_path).map_err(|e| e.to_string())?;
    // 传入文件则授权其所在目录，传入目录则授权目录本身
    let dir = if canonical.is_dir() {
        canonical.clone()
    } else {
        canonical
            .parent()
            .map(|p| p.to_path_buf())
            .ok_or_else(|| "Unable to resolve directory".to_string())?
    };
    if is_sensitive_dir(&dir) {
        return Err("Permission denied for sensitive directory".to_string());
    }
    app.fs_scope()
        .allow_directory(&dir, false)
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UpdateInfo {
    pub latest_version: String,
    pub url: String,
    pub body: Option<String>,
    pub install_supported: bool,
}

fn updater_public_key() -> Option<&'static str> {
    option_env!("FINALSUB_UPDATER_PUBLIC_KEY").filter(|key| !key.trim().is_empty())
}

fn signed_updater(app: &AppHandle) -> std::result::Result<tauri_plugin_updater::Updater, String> {
    let public_key = updater_public_key()
        .ok_or_else(|| "当前构建未配置更新签名公钥，请前往发布页手动下载。".to_string())?;
    let endpoint =
        tauri::Url::parse(UPDATER_MANIFEST_URL).map_err(|e| format!("更新清单地址无效：{e}"))?;
    let builder = app
        .updater_builder()
        .pubkey(public_key)
        .timeout(Duration::from_secs(30))
        .endpoints(vec![endpoint])
        .map_err(|e| format!("更新器配置失败：{e}"))?;
    builder
        .build()
        .map_err(|e| format!("更新器初始化失败：{e}"))
}

async fn check_signed_update(app: &AppHandle) -> std::result::Result<Option<UpdateInfo>, String> {
    let update = signed_updater(app)?
        .check()
        .await
        .map_err(|e| format!("签名更新检查失败：{e}"))?;
    let Some(update) = update else {
        return Ok(None);
    };
    validate_update_download_url(&update.download_url)?;
    Ok(Some(UpdateInfo {
        latest_version: update.version,
        url: RELEASE_LATEST_URL.into(),
        body: update.body,
        install_supported: true,
    }))
}

async fn check_manual_update(app: &AppHandle) -> std::result::Result<Option<UpdateInfo>, String> {
    let current_version = app.package_info().version.to_string();
    let client = reqwest::Client::builder()
        .user_agent("FinalSub-Updater")
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| format!("无法初始化更新检查：{e}"))?;

    let resp = client
        .get(RELEASE_LATEST_API_URL)
        .send()
        .await
        .map_err(|e| format!("无法连接更新服务：{e}"))?;

    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }

    if !resp.status().is_success() {
        return Err(format!("更新服务返回异常状态：{}", resp.status()));
    }

    #[derive(serde::Deserialize)]
    struct GithubRelease {
        tag_name: String,
        body: Option<String>,
    }

    let release: GithubRelease = resp
        .json()
        .await
        .map_err(|e| format!("更新信息解析失败：{e}"))?;

    let latest_tag = release.tag_name;
    let latest_ver = latest_tag.trim_start_matches('v').to_string();

    if is_newer_version(&current_version, &latest_ver) {
        Ok(Some(UpdateInfo {
            latest_version: latest_ver,
            url: RELEASE_LATEST_URL.into(),
            body: release.body,
            install_supported: false,
        }))
    } else {
        Ok(None)
    }
}

#[tauri::command]
pub async fn check_for_update(
    app: tauri::AppHandle,
) -> std::result::Result<Option<UpdateInfo>, String> {
    if updater_public_key().is_some() {
        check_signed_update(&app).await
    } else {
        check_manual_update(&app).await
    }
}

fn is_newer_version(current_ver: &str, new_ver: &str) -> bool {
    match (
        semver::Version::parse(current_ver),
        semver::Version::parse(new_ver),
    ) {
        (Ok(current), Ok(new)) => new > current,
        _ => false,
    }
}

fn validate_update_download_url(url: &tauri::Url) -> std::result::Result<(), String> {
    let trusted = url.scheme() == "https"
        && url.host_str() == Some("api.github.com")
        && url.path().starts_with(UPDATER_ASSET_PATH_PREFIX)
        && url.path()[UPDATER_ASSET_PATH_PREFIX.len()..]
            .chars()
            .all(|character| character.is_ascii_digit());
    if trusted {
        Ok(())
    } else {
        Err("更新清单包含非 FinalSub 官方 Release 资产地址，已拒绝下载。".into())
    }
}

#[derive(Debug, Clone, Copy, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum AppUpdatePhase {
    Downloading,
    Verifying,
    Installing,
    Restarting,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct AppUpdateEvent {
    pub phase: AppUpdatePhase,
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
}

fn send_update_event(
    channel: &Channel<AppUpdateEvent>,
    phase: AppUpdatePhase,
    downloaded_bytes: u64,
    total_bytes: Option<u64>,
) {
    let _ = channel.send(AppUpdateEvent {
        phase,
        downloaded_bytes,
        total_bytes,
    });
}

fn update_blocker<'a>(
    mut tasks: impl Iterator<Item = &'a Task>,
    task_operation_active: bool,
    model_operation_active: bool,
    tts_operation_active: bool,
    burn_operation_active: bool,
) -> Option<&'static str> {
    if task_operation_active
        || tasks.any(|task| matches!(task.status, TaskStatus::Pending | TaskStatus::Running))
    {
        Some("有字幕任务正在处理或等待，请先暂停或完成任务后再安装更新。")
    } else if model_operation_active {
        Some("有模型正在下载或安装，请完成或取消模型操作后再安装更新。")
    } else if tts_operation_active {
        Some("有配音正在生成、对齐或导出，请完成或取消配音操作后再安装更新。")
    } else if burn_operation_active {
        Some("有视频正在合成字幕，请完成或取消合成后再安装更新。")
    } else {
        None
    }
}

async fn ensure_update_can_start(state: &AppState) -> std::result::Result<(), String> {
    let tasks = state.tasks.read().await;
    let task_controls = state.task_controls.read().await;
    let model_controls = state.model_controls.read().await;
    let tts_controls = state.tts_controls.read().await;
    let burn_controls = state.burn_controls.read().await;
    if let Some(reason) = update_blocker(
        tasks.values(),
        !task_controls.is_empty(),
        !model_controls.is_empty(),
        !tts_controls.is_empty(),
        !burn_controls.is_empty(),
    ) {
        Err(reason.into())
    } else {
        Ok(())
    }
}

struct UpdateInProgressGuard<'a>(&'a std::sync::atomic::AtomicBool);

impl Drop for UpdateInProgressGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

#[tauri::command]
pub async fn download_and_install_update(
    app: AppHandle,
    state: State<'_, AppState>,
    expected_version: String,
    on_progress: Channel<AppUpdateEvent>,
) -> std::result::Result<(), String> {
    let expected = semver::Version::parse(&expected_version)
        .map_err(|_| "待安装版本号无效，请重新检查更新。".to_string())?;
    if updater_public_key().is_none() {
        return Err("当前构建不支持应用内安装，请前往发布页手动下载。".into());
    }
    state
        .update_in_progress
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .map_err(|_| "另一个更新安装正在进行中。".to_string())?;
    let _update_guard = UpdateInProgressGuard(&state.update_in_progress);

    ensure_update_can_start(&state).await?;

    let update: Update = signed_updater(&app)?
        .check()
        .await
        .map_err(|e| format!("签名更新检查失败：{e}"))?
        .ok_or_else(|| "没有可安装的新版本，请重新检查更新。".to_string())?;
    validate_update_download_url(&update.download_url)?;
    let manifest_version = semver::Version::parse(&update.version)
        .map_err(|_| "更新清单中的版本号无效。".to_string())?;
    if manifest_version != expected {
        return Err(format!(
            "更新版本已从 {expected_version} 变为 {}，请重新确认后安装。",
            update.version
        ));
    }

    send_update_event(&on_progress, AppUpdatePhase::Downloading, 0, None);
    let progress_channel = on_progress.clone();
    let verify_channel = on_progress.clone();
    let mut downloaded_bytes = 0_u64;
    let mut total_bytes = None;
    let bytes = update
        .download(
            move |chunk_length, content_length| {
                downloaded_bytes = downloaded_bytes.saturating_add(chunk_length as u64);
                total_bytes = content_length.or(total_bytes);
                send_update_event(
                    &progress_channel,
                    AppUpdatePhase::Downloading,
                    downloaded_bytes,
                    total_bytes,
                );
            },
            move || {
                send_update_event(&verify_channel, AppUpdatePhase::Verifying, 0, None);
            },
        )
        .await
        .map_err(|e| format!("更新包下载或签名验证失败：{e}"))?;

    // 下载期间用户仍可操作应用。安装前持有这些读锁并再次检查，确保不会在
    // 新任务、模型安装、配音或视频合成启动的竞态窗口内替换应用并重启。
    let tasks = state.tasks.read().await;
    let task_controls = state.task_controls.read().await;
    let model_controls = state.model_controls.read().await;
    let tts_controls = state.tts_controls.read().await;
    let burn_controls = state.burn_controls.read().await;
    if let Some(reason) = update_blocker(
        tasks.values(),
        !task_controls.is_empty(),
        !model_controls.is_empty(),
        !tts_controls.is_empty(),
        !burn_controls.is_empty(),
    ) {
        return Err(reason.into());
    }

    send_update_event(&on_progress, AppUpdatePhase::Installing, 0, None);
    update
        .install(&bytes)
        .map_err(|e| format!("更新包安装失败：{e}"))?;
    drop(burn_controls);
    drop(tts_controls);
    drop(model_controls);
    drop(task_controls);
    drop(tasks);

    send_update_event(&on_progress, AppUpdatePhase::Restarting, 0, None);
    tokio::time::sleep(Duration::from_millis(300)).await;
    app.restart()
}

#[tauri::command]
pub fn convert_subtitle_opencc(srt_content: String, config: String) -> Result<String, String> {
    crate::core::opencc::convert_subtitle(&srt_content, &config)
        .map_err(|e| format!("简繁转换失败：{}", e))
}

#[tauri::command]
pub fn convert_strings_opencc(texts: Vec<String>, config: String) -> Result<Vec<String>, String> {
    let converter = opencc_fmmseg::OpenCC::new();
    Ok(texts
        .into_iter()
        .map(|t| converter.convert(&t, &config, false))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "writes to the user's OS keyring; run manually to validate native keyring backend"]
    fn keyring_native_backend_manages_provider_secret() {
        let provider_id = format!("codex-keyring-roundtrip-{}", uuid::Uuid::new_v4());
        let endpoint = "https://api.example.test/v1".to_string();
        let field = "apiKey".to_string();
        let value = format!("secret-{}", uuid::Uuid::new_v4());

        let _ = delete_provider_secret(provider_id.clone(), endpoint.clone(), field.clone());
        set_provider_secret(
            provider_id.clone(),
            endpoint.clone(),
            field.clone(),
            value.clone(),
        )
        .unwrap();

        assert!(has_provider_secret(provider_id.clone(), endpoint.clone(), field.clone()).unwrap());

        delete_provider_secret(provider_id.clone(), endpoint.clone(), field.clone()).unwrap();
        assert!(!has_provider_secret(provider_id, endpoint, field).unwrap());
    }

    #[test]
    fn validate_media_path_empty() {
        let result = validate_media_path("");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Please select"));
    }

    #[test]
    fn validate_media_path_whitespace() {
        let result = validate_media_path("   ");
        assert!(result.is_err());
    }

    #[test]
    fn validate_media_path_relative() {
        let result = validate_media_path("relative/path.mp4");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("absolute path"));
    }

    #[test]
    fn validate_media_path_nonexistent() {
        let result = validate_media_path("/nonexistent/file.mp4");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("does not exist"));
    }

    #[test]
    fn validate_media_path_directory() {
        let result = validate_media_path("/tmp");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("does not exist"));
    }

    #[test]
    fn validate_media_path_valid() {
        let tmp = std::env::temp_dir().join("finalsub_test_media.mp4");
        std::fs::write(&tmp, b"fake").unwrap();
        let result = validate_media_path(tmp.to_str().unwrap());
        assert!(result.is_ok());
        std::fs::remove_file(&tmp).unwrap();
    }

    #[test]
    fn validate_non_empty_ok() {
        assert_eq!(validate_non_empty("x", "hello".into()).unwrap(), "hello");
    }

    #[test]
    fn validate_non_empty_trimmed() {
        assert_eq!(
            validate_non_empty("x", "  hello  ".into()).unwrap(),
            "hello"
        );
    }

    #[test]
    fn validate_non_empty_fail() {
        let result = validate_non_empty("engine_id", "".into());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("engine_id"));
    }

    #[test]
    fn validate_non_empty_whitespace_fail() {
        let result = validate_non_empty("model_id", "   ".into());
        assert!(result.is_err());
    }

    #[test]
    fn validate_subtitle_output_format_normalizes_supported_values() {
        assert_eq!(validate_subtitle_output_format(None).unwrap(), None);
        assert_eq!(
            validate_subtitle_output_format(Some(" VTT ".into())).unwrap(),
            Some("vtt".into())
        );
        assert!(validate_subtitle_output_format(Some("srt/evil".into())).is_err());
    }

    #[test]
    fn validate_translate_only_subtitle_extension_matches_picker_formats() {
        for ext in ["srt", "vtt", "ass", "lrc"] {
            let path = PathBuf::from(format!("/tmp/subtitle.{ext}"));
            assert!(
                validate_translate_only_subtitle_extension(&path).is_ok(),
                "{ext} should be accepted for translate-only tasks"
            );
        }

        let path = PathBuf::from("/tmp/subtitle.txt");
        assert!(validate_translate_only_subtitle_extension(&path).is_err());
    }

    #[test]
    fn validate_parakeet_source_language_rejects_non_english_before_task_creation() {
        assert_eq!(
            validate_source_language_for_engine("parakeet-mlx", Some(" en ".into())).unwrap(),
            Some("en".into())
        );
        assert!(validate_source_language_for_engine("parakeet-mlx", Some("zh".into())).is_err());
        assert_eq!(
            validate_source_language_for_engine("whisper-cpp", Some("zh".into())).unwrap(),
            Some("zh".into())
        );
    }

    #[test]
    fn validate_native_sherpa_engine_language_boundaries() {
        assert!(validate_source_language_for_engine("paraformer", Some("ja".into())).is_err());
        assert_eq!(
            validate_source_language_for_engine("paraformer", Some(" zh ".into())).unwrap(),
            Some("zh".into())
        );
        assert!(validate_source_language_for_engine("firered-asr", Some("ja".into())).is_err());
        assert_eq!(
            validate_source_language_for_engine("firered-asr", Some("yue".into())).unwrap(),
            Some("yue".into())
        );
        assert_eq!(
            validate_source_language_for_engine("qwen3-asr", Some("ja".into())).unwrap(),
            Some("ja".into())
        );
    }

    #[test]
    fn cloud_asr_readiness_requires_explicit_upload_consent() {
        let settings = Settings {
            cloud_asr_upload_consent: false,
            ..Settings::default()
        };
        let error = validate_cloud_asr_readiness(&settings).unwrap_err();
        assert!(error.contains("explicit audio upload consent"));
    }

    #[test]
    fn validate_translation_content_mode_accepts_supported_values() {
        assert_eq!(
            validate_translation_content_mode(None).unwrap(),
            TranslationContentMode::TargetOnly
        );
        assert_eq!(
            validate_translation_content_mode(Some("source-and-target".into())).unwrap(),
            TranslationContentMode::SourceAndTarget
        );
        assert_eq!(
            validate_translation_content_mode(Some("translateAndSource".into())).unwrap(),
            TranslationContentMode::TargetAndSource
        );
        assert!(validate_translation_content_mode(Some("source/evil".into())).is_err());
    }

    #[test]
    fn validate_new_output_path_rejects_existing_file() {
        let tmp = std::env::temp_dir().join("finalsub_existing_output.srt");
        std::fs::write(&tmp, b"exists").unwrap();

        let result = validate_new_output_path(tmp.to_str().unwrap(), "Output path");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("already exists"));

        std::fs::remove_file(&tmp).unwrap();
    }

    #[test]
    fn validate_new_output_path_accepts_new_file_in_existing_parent() {
        let tmp = std::env::temp_dir().join("finalsub_new_output.srt");
        let _ = std::fs::remove_file(&tmp);

        let result = validate_new_output_path(tmp.to_str().unwrap(), "Output path");
        assert!(result.is_ok());
    }

    #[test]
    fn validate_json_extension_rejects_non_json() {
        let path = std::path::PathBuf::from("/tmp/config.txt");
        let result = validate_json_extension(&path);
        assert!(result.is_err());
    }

    #[test]
    fn validate_ass_color_rejects_bad_value() {
        let result = validate_ass_color("Font color", "white");
        assert!(result.is_err());
    }

    #[test]
    fn validate_compose_extensions_match_the_ui_contract() {
        for path in [
            "/tmp/out.mp4",
            "/tmp/out.m4v",
            "/tmp/out.mov",
            "/tmp/out.mkv",
        ] {
            assert!(validate_compose_output_extension(std::path::Path::new(path)).is_ok());
        }
        assert!(validate_compose_output_extension(std::path::Path::new("/tmp/out.webm")).is_err());

        for path in [
            "/tmp/dub.wav",
            "/tmp/dub.mp3",
            "/tmp/dub.m4a",
            "/tmp/dub.flac",
        ] {
            assert!(validate_audio_input_extension(std::path::Path::new(path)).is_ok());
        }
        assert!(validate_audio_input_extension(std::path::Path::new("/tmp/dub.exe")).is_err());
    }

    #[test]
    fn validate_track_metadata_rejects_control_characters_and_invalid_language_tags() {
        assert_eq!(
            validate_track_language("Language", Some(" ZHO ")).unwrap(),
            Some("zho".into())
        );
        assert!(validate_track_language("Language", Some("zh;rm -rf")).is_err());
        assert_eq!(
            validate_track_title("Title", Some("  中文配音  ")).unwrap(),
            Some("中文配音".into())
        );
        assert!(validate_track_title("Title", Some("bad\ntitle")).is_err());
    }

    #[test]
    fn validate_ass_color_accepts_ass_hex() {
        assert!(validate_ass_color("Font color", "&H00FFFFFF").is_ok());
    }

    #[test]
    fn validate_task_id_accepts_uuid() {
        assert!(validate_task_id("019ecae8-d5eb-7720-9c25-37bfa115fa48").is_ok());
    }

    #[test]
    fn validate_task_id_rejects_path_escape() {
        let result = validate_task_id("../../Library/Secrets");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid"));
    }

    #[test]
    fn task_can_be_deleted_rejects_active_tasks() {
        assert!(!task_can_be_deleted(TaskStatus::Pending));
        assert!(!task_can_be_deleted(TaskStatus::Running));
        assert!(task_can_be_deleted(TaskStatus::Paused));
        assert!(task_can_be_deleted(TaskStatus::Cancelled));
        assert!(task_can_be_deleted(TaskStatus::Review));
        assert!(task_can_be_deleted(TaskStatus::Done));
        assert!(task_can_be_deleted(TaskStatus::Error));
    }

    #[test]
    fn review_batch_approval_is_all_or_nothing() {
        let make_review_task = |name: &str| {
            let mut task = task_queue::create_task(CreateTaskParams {
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
                review_required: true,
            });
            task.status = TaskStatus::Review;
            task.output_path = Some(format!("/tmp/{name}.srt"));
            task
        };
        let first = make_review_task("first");
        let second = make_review_task("second");
        let original = HashMap::from([
            (first.id.clone(), first.clone()),
            (second.id.clone(), second.clone()),
        ]);
        let ids = vec![first.id.clone(), second.id.clone()];

        let (approved_map, approved) =
            approve_review_tasks(&original, &ids, "2026-07-19T00:00:00Z").unwrap();

        assert_eq!(approved.len(), 2);
        assert!(approved.iter().all(|task| task.status == TaskStatus::Done));
        assert!(approved
            .iter()
            .all(|task| task.reviewed_at.as_deref() == Some("2026-07-19T00:00:00Z")));
        assert!(ids
            .iter()
            .all(|id| approved_map[id].status == TaskStatus::Done));
        assert!(ids
            .iter()
            .all(|id| original[id].status == TaskStatus::Review));

        let mut invalid = original.clone();
        invalid.get_mut(&second.id).unwrap().status = TaskStatus::Done;
        assert!(approve_review_tasks(&invalid, &ids, "2026-07-19T00:00:00Z").is_err());
        assert_eq!(invalid[&first.id].status, TaskStatus::Review);
    }

    #[test]
    fn updater_version_comparison_uses_semver_precedence() {
        assert!(is_newer_version("1.0.9", "1.0.10"));
        assert!(is_newer_version("1.0.10-beta.1", "1.0.10"));
        assert!(!is_newer_version("1.0.10", "1.0.10-beta.1"));
        assert!(!is_newer_version("1.0.10", "1.0.10"));
        assert!(!is_newer_version("1.0.10", "not-a-version"));
    }

    #[test]
    fn updater_accepts_only_finalsub_github_release_assets() {
        let valid = tauri::Url::parse(
            "https://api.github.com/repos/GravityPoet/FinalSub/releases/assets/123456",
        )
        .unwrap();
        assert!(validate_update_download_url(&valid).is_ok());

        for invalid in [
            "http://api.github.com/repos/GravityPoet/FinalSub/releases/assets/123456",
            "https://example.com/repos/GravityPoet/FinalSub/releases/assets/123456",
            "https://api.github.com/repos/other/FinalSub/releases/assets/123456",
            "https://api.github.com/repos/GravityPoet/FinalSub/releases/assets/not-a-number",
            "https://api.github.com/repos/GravityPoet/FinalSub/releases/assets/123456/extra",
        ] {
            let url = tauri::Url::parse(invalid).unwrap();
            assert!(validate_update_download_url(&url).is_err(), "{invalid}");
        }
    }

    #[test]
    fn updater_blocks_only_work_that_cannot_survive_restart() {
        let mut task = task_queue::create_task(CreateTaskParams {
            task_type: TaskType::GenerateOnly,
            media_path: "/tmp/video.mp4".into(),
            media_name: "video.mp4".into(),
            engine_id: "whisper-cpp".into(),
            model_id: "small".into(),
            source_language: Some("auto".into()),
            target_language: None,
            translation_content_mode: TranslationContentMode::TargetOnly,
            output_format: Some("srt".into()),
            output_name: None,
            strip_chinese_punctuation: false,
            review_required: false,
        });

        task.status = TaskStatus::Paused;
        assert_eq!(
            update_blocker([&task].into_iter(), false, false, false, false),
            None
        );

        assert!(
            update_blocker([&task].into_iter(), true, false, false, false)
                .unwrap()
                .contains("字幕任务")
        );

        task.status = TaskStatus::Running;
        assert!(
            update_blocker([&task].into_iter(), false, false, false, false)
                .unwrap()
                .contains("字幕任务")
        );

        task.status = TaskStatus::Done;
        assert!(
            update_blocker([&task].into_iter(), false, true, false, false)
                .unwrap()
                .contains("模型")
        );
        assert!(
            update_blocker([&task].into_iter(), false, false, true, false)
                .unwrap()
                .contains("配音")
        );
        assert!(
            update_blocker([&task].into_iter(), false, false, false, true)
                .unwrap()
                .contains("合成")
        );
    }

    #[test]
    fn prepare_task_for_retry_preserves_checkpoint_progress() {
        let mut task = task_queue::create_task(CreateTaskParams {
            task_type: TaskType::GenerateAndTranslate,
            media_path: "/tmp/video.mp4".into(),
            media_name: "video.mp4".into(),
            engine_id: "parakeet-mlx".into(),
            model_id: "parakeet-tdt-0.6b-v2".into(),
            source_language: Some("auto".into()),
            target_language: Some("zh".into()),
            translation_content_mode: TranslationContentMode::TargetOnly,
            output_format: Some("srt".into()),
            output_name: None,
            strip_chinese_punctuation: false,
            review_required: false,
        });
        task.status = TaskStatus::Error;
        task.progress = 0.87;
        task.error = Some("network error".into());

        prepare_task_for_retry(&mut task);

        assert_eq!(task.status, TaskStatus::Pending);
        assert_eq!(task.progress, 0.87);
        assert!(task.error.is_none());
        assert!(task.status_message.contains("上次进度"));
    }

    #[test]
    fn expand_home_path_expands_tilde_prefix() {
        let expanded = expand_home_path("~/Tools/Local-LLM");
        assert!(expanded.is_absolute());
        assert!(expanded.ends_with("Tools/Local-LLM"));
    }

    #[test]
    fn parakeet_model_storage_uses_its_dedicated_root() {
        let config = tempfile::tempdir().unwrap();
        let whisper = tempfile::tempdir().unwrap();
        let parakeet = tempfile::tempdir().unwrap();
        let model_dir = parakeet
            .path()
            .join(crate::core::asr::parakeet::PARAKEET_MODEL_ID);
        std::fs::create_dir_all(&model_dir).unwrap();
        for name in [
            "encoder.int8.onnx",
            "decoder.int8.onnx",
            "joiner.int8.onnx",
            "tokens.txt",
        ] {
            std::fs::write(model_dir.join(name), b"fixture").unwrap();
        }

        let configured = Settings {
            models_path: whisper.path().to_string_lossy().into_owned(),
            parakeet_models_path: parakeet.path().to_string_lossy().into_owned(),
            ..Settings::default()
        };
        settings::save_settings(config.path(), &configured).unwrap();

        assert_eq!(parakeet_models_dir(config.path()).unwrap(), parakeet.path());
        assert_eq!(
            model_storage_dir(config.path(), crate::core::asr::parakeet::PARAKEET_MODEL_ID)
                .unwrap(),
            parakeet.path()
        );

        let mut catalog = models::builtin_model_catalog();
        models::scan_model_status(&mut catalog, whisper.path(), parakeet.path());
        let model = catalog
            .iter()
            .find(|model| model.id == crate::core::asr::parakeet::PARAKEET_MODEL_ID)
            .unwrap();
        assert!(matches!(model.status, ModelStatus::Downloaded));
    }

    #[test]
    fn test_resolve_sidecar_whisper_logic() {
        let name = "whisper-cli";
        let target_triple = if cfg!(target_arch = "aarch64") {
            "aarch64-apple-darwin"
        } else {
            "x86_64-apple-darwin"
        };
        let file_name = format!("{name}-{target_triple}");
        let current_dir = std::env::current_dir().unwrap();
        let path1 = current_dir.join("binaries").join(&file_name);
        let path2 = current_dir
            .join("src-tauri")
            .join("binaries")
            .join(&file_name);
        assert!(
            path1.exists() || path2.exists(),
            "开发环境缺少 whisper-cli thin sidecar：{file_name}"
        );
    }

    #[test]
    fn test_resolve_sidecar_ffmpeg_logic() {
        let name = "ffmpeg";
        let target_triple = if cfg!(target_arch = "aarch64") {
            "aarch64-apple-darwin"
        } else {
            "x86_64-apple-darwin"
        };
        let file_name = format!("{name}-{target_triple}");
        let current_dir = std::env::current_dir().unwrap();
        let path1 = current_dir.join("binaries").join(&file_name);
        let path2 = current_dir
            .join("src-tauri")
            .join("binaries")
            .join(&file_name);
        assert!(
            path1.exists() || path2.exists(),
            "开发环境缺少 ffmpeg thin sidecar：{file_name}"
        );
    }
}
