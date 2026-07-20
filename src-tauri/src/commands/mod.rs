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
use crate::core::logs::{self, LogEntry, LogQuery};
use crate::core::models::{self, AsrModelInfo, ModelStatus};
use crate::core::recipes::{self, SaveTaskRecipeRequest, TaskRecipe};
use crate::core::settings::{self, Settings};
use crate::core::style_presets::{self, SaveSubtitleStylePresetRequest, SubtitleStylePreset};
use crate::core::subtitle::SubtitleTrack;
use crate::core::task_queue::{
    self, CreateTaskParams, PipelineConfig, Task, TaskMap, TaskStatus, TaskType,
    TranslationContentMode,
};
use crate::core::translation::{self, TranslationProvider};
use crate::core::tts::{
    CloudTtsSynthesisRequest, CloudVoiceSummary, CreateCloudVoiceProfileRequest,
    CreateVoiceProfileRequest, DubbingEngineSelection, DubbingRecheckDecision, DubbingSession,
    DubbingSubtitleWriteResult, DubbingSynthesizeCueRequest, LinkCloudVoiceProfileRequest,
    LocalTtsSynthesisRequest, PrepareVoiceSampleRequest, PreparedDubbingCue, PreparedVoiceSample,
    RetrainCloudVoiceProfileRequest, SaveTtsProviderRequest, TtsModelInfo, TtsProviderProfile,
    TtsSynthesisResult, UpdateDubbingCueRequest, VoiceProfile, VoiceSourceInfo, VoiceSubtitleCue,
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
    if let Some(pipeline) = task.pipeline.as_mut() {
        pipeline.prepare_current_stage_for_resume("准备从当前阶段重试");
    }
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
    let _ = logs::clear_logs(app_config_dir, Some(task_id)).await;
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

#[tauri::command]
pub async fn list_voice_profiles(state: State<'_, AppState>) -> Result<Vec<VoiceProfile>, String> {
    let profiles = state.voice_profiles.read().await;
    let mut values = profiles.values().cloned().collect::<Vec<_>>();
    values.sort_by(|left, right| {
        right
            .created_at
            .cmp(&left.created_at)
            .then_with(|| left.name.cmp(&right.name))
    });
    Ok(values)
}

#[tauri::command]
pub async fn inspect_voice_source(
    app: AppHandle,
    source_path: String,
) -> Result<VoiceSourceInfo, String> {
    let ffmpeg_path = resolve_sidecar(&app, "ffmpeg")?;
    crate::core::tts::inspect_voice_source(&ffmpeg_path, &source_path)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn list_voice_subtitle_cues(source_path: String) -> Result<Vec<VoiceSubtitleCue>, String> {
    crate::core::tts::list_voice_subtitle_cues(&source_path).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn save_voice_recording(
    state: State<'_, AppState>,
    data_base64: String,
    mime_type: String,
) -> Result<String, String> {
    crate::core::tts::save_voice_recording(&state.app_config_dir, &data_base64, &mime_type)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn discard_voice_recording(state: State<'_, AppState>, path: String) -> Result<(), String> {
    crate::core::tts::discard_voice_recording(&state.app_config_dir, &path)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn prepare_voice_sample(
    app: AppHandle,
    state: State<'_, AppState>,
    request: PrepareVoiceSampleRequest,
) -> Result<PreparedVoiceSample, String> {
    let ffmpeg_path = resolve_sidecar(&app, "ffmpeg")?;
    let vad_model_path = crate::core::task_runner::sherpa_vad_model_path(&app)?;
    let _power_save_lease = state.power_save.acquire("voice-profile:prepare");
    crate::core::tts::prepare_voice_sample(
        &state.app_config_dir,
        &ffmpeg_path,
        &vad_model_path,
        request,
    )
    .await
    .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn discard_prepared_voice_sample(
    state: State<'_, AppState>,
    token: String,
) -> Result<(), String> {
    crate::core::tts::discard_prepared_voice_sample(&state.app_config_dir, &token)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn create_voice_profile(
    state: State<'_, AppState>,
    request: CreateVoiceProfileRequest,
) -> Result<VoiceProfile, String> {
    let mut profiles = state.voice_profiles.write().await;
    crate::core::tts::create_voice_profile(&state.app_config_dir, &mut profiles, request)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn create_cloud_voice_profile(
    state: State<'_, AppState>,
    request: CreateCloudVoiceProfileRequest,
) -> Result<VoiceProfile, String> {
    let _power_save_lease = state.power_save.acquire("voice-profile:cloud-create");
    let mut profiles = state.voice_profiles.write().await;
    crate::core::tts::create_cloud_voice_profile(&state.app_config_dir, &mut profiles, request)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn list_cloud_voices(
    state: State<'_, AppState>,
    provider_id: String,
) -> Result<Vec<CloudVoiceSummary>, String> {
    crate::core::tts::list_cloud_voices(&state.app_config_dir, &provider_id)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn link_cloud_voice_profile(
    state: State<'_, AppState>,
    request: LinkCloudVoiceProfileRequest,
) -> Result<VoiceProfile, String> {
    let mut profiles = state.voice_profiles.write().await;
    crate::core::tts::link_cloud_voice_profile(&state.app_config_dir, &mut profiles, request)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn delete_cloud_voice_remote(
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    let profile = state
        .voice_profiles
        .read()
        .await
        .get(&id)
        .cloned()
        .ok_or_else(|| "音色不存在".to_string())?;
    crate::core::tts::delete_cloud_voice_remote(&state.app_config_dir, &profile)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn refresh_cloud_voice_status(
    state: State<'_, AppState>,
    id: String,
) -> Result<VoiceProfile, String> {
    let mut profiles = state.voice_profiles.write().await;
    crate::core::tts::refresh_cloud_voice_status(&state.app_config_dir, &mut profiles, &id)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn retrain_cloud_voice_profile(
    state: State<'_, AppState>,
    request: RetrainCloudVoiceProfileRequest,
) -> Result<VoiceProfile, String> {
    let _power_save_lease = state.power_save.acquire("voice-profile:cloud-retrain");
    let mut profiles = state.voice_profiles.write().await;
    crate::core::tts::retrain_cloud_voice_profile(&state.app_config_dir, &mut profiles, request)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn rename_voice_profile(
    state: State<'_, AppState>,
    id: String,
    name: String,
) -> Result<VoiceProfile, String> {
    let mut profiles = state.voice_profiles.write().await;
    crate::core::tts::rename_voice_profile(&state.app_config_dir, &mut profiles, &id, &name)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn remove_voice_profile(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let mut profiles = state.voice_profiles.write().await;
    crate::core::tts::remove_voice_profile(&state.app_config_dir, &mut profiles, &id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn export_voice_profile(
    state: State<'_, AppState>,
    id: String,
    output_path: String,
) -> Result<String, String> {
    let profile = state
        .voice_profiles
        .read()
        .await
        .get(&id)
        .cloned()
        .ok_or_else(|| "音色不存在".to_string())?;
    crate::core::tts::export_voice_profile(&profile, &output_path)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn import_voice_profile(
    app: AppHandle,
    state: State<'_, AppState>,
    input_path: String,
) -> Result<VoiceProfile, String> {
    let ffmpeg_path = resolve_sidecar(&app, "ffmpeg")?;
    let vad_model_path = crate::core::task_runner::sherpa_vad_model_path(&app)?;
    let _power_save_lease = state.power_save.acquire("voice-profile:import");
    let mut profiles = state.voice_profiles.write().await;
    crate::core::tts::import_voice_profile(
        &state.app_config_dir,
        &ffmpeg_path,
        &vad_model_path,
        &mut profiles,
        &input_path,
    )
    .await
    .map_err(|error| error.to_string())
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
    // 独立 worker 可能仍持有模型文件句柄；正在运行的合成已由上面的闸门
    // 排除，因此先优雅停止 worker，再删除受管目录。下一次合成会自动重建。
    state.tts_worker.stop().await;
    crate::core::tts::delete_managed_model(&state.app_config_dir, &normalized)
        .map_err(|error| error.to_string())
}

/// 登记外部 TTS 模型目录。只保存绝对路径，不复制模型，也不会取得源目录删除权。
#[tauri::command]
pub async fn register_tts_model_path(
    state: State<'_, AppState>,
    model_id: String,
    source_path: String,
) -> Result<TtsModelInfo, String> {
    if !state.tts_controls.read().await.is_empty() {
        return Err("配音正在进行，请完成或取消后再更改模型目录".into());
    }
    let info =
        crate::core::tts::register_external_model(&state.app_config_dir, &model_id, &source_path)
            .map_err(|error| error.to_string())?;
    state.tts_worker.stop().await;
    Ok(info)
}

/// 仅移除外部路径登记；源模型文件永远保留。
#[tauri::command]
pub async fn forget_tts_model_path(
    state: State<'_, AppState>,
    model_id: String,
) -> Result<(), String> {
    if !state.tts_controls.read().await.is_empty() {
        return Err("配音正在进行，请完成或取消后再更改模型目录".into());
    }
    crate::core::tts::remove_external_registration(&state.app_config_dir, &model_id)
        .map_err(|error| error.to_string())?;
    state.tts_worker.stop().await;
    Ok(())
}

#[tauri::command]
pub async fn set_tts_models_root(
    state: State<'_, AppState>,
    models_root: String,
) -> Result<Vec<TtsModelInfo>, String> {
    if !state.tts_controls.read().await.is_empty() {
        return Err("配音正在进行，请完成或取消后再更改模型目录".into());
    }
    let models = crate::core::tts::set_models_root(&state.app_config_dir, &models_root)
        .map_err(|error| error.to_string())?;
    state.tts_worker.stop().await;
    Ok(models)
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
    let _power_save_lease = state.power_save.acquire(format!("tts:{generation_id}"));
    let result = state.tts_worker.synthesize(model, request, cancelled).await;
    state.tts_controls.write().await.remove(&generation_id);
    result.map_err(|error| error.to_string())
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
pub async fn delete_tts_provider(
    state: State<'_, AppState>,
    provider_id: String,
) -> Result<(), String> {
    if state
        .voice_profiles
        .read()
        .await
        .values()
        .any(|profile| profile.provider_id.as_deref() == Some(provider_id.as_str()))
    {
        return Err("该在线 TTS 实例仍被云端音色使用；请先在“我的音色”中解绑这些音色".into());
    }
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
    let _power_save_lease = state.power_save.acquire(format!("tts:{generation_id}"));
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

async fn synthesize_prepared_dubbing_once(
    state: &AppState,
    ffmpeg: &Path,
    prepared: &PreparedDubbingCue,
    speed: f32,
    cancelled: std::sync::Arc<std::sync::atomic::AtomicBool>,
) -> Result<TtsSynthesisResult, String> {
    match &prepared.config.engine {
        DubbingEngineSelection::Local { model_id } => {
            let model = crate::core::tts::resolve_ready_model(&state.app_config_dir, model_id)
                .map_err(|error| error.to_string())?;
            let request = LocalTtsSynthesisRequest {
                model_id: model_id.clone(),
                text: prepared.text.clone(),
                voice_id: (!prepared.config.voice.is_empty())
                    .then(|| prepared.config.voice.clone()),
                speed: Some(speed),
                output_path: prepared.output_path.clone(),
                reference_audio_path: prepared.config.reference_audio_path.clone(),
                reference_text: prepared.config.reference_text.clone(),
                num_steps: prepared.config.num_steps,
            };
            state
                .tts_worker
                .synthesize(model, request, cancelled)
                .await
                .map_err(|error| error.to_string())
        }
        DubbingEngineSelection::Cloud { provider_id } => crate::core::tts::synthesize_cloud(
            &state.app_config_dir,
            ffmpeg,
            CloudTtsSynthesisRequest {
                provider_id: provider_id.clone(),
                text: prepared.text.clone(),
                voice: (!prepared.config.voice.is_empty()).then(|| prepared.config.voice.clone()),
                speed: Some(speed),
                output_path: prepared.output_path.clone(),
            },
            cancelled,
        )
        .await
        .map_err(|error| error.to_string()),
    }
}

pub(crate) async fn synthesize_and_align_dubbing_cue(
    state: &AppState,
    ffmpeg: &Path,
    prepared: &PreparedDubbingCue,
    cancelled: std::sync::Arc<std::sync::atomic::AtomicBool>,
) -> Result<DubbingSession, String> {
    let first = synthesize_prepared_dubbing_once(
        state,
        ffmpeg,
        prepared,
        prepared.synthesis_speed,
        cancelled.clone(),
    )
    .await?;
    let calibration_source_ms = first.duration_ms;
    let mut synthesized_ms = first.duration_ms;
    let mut applied_alignment_speed = prepared.alignment_speed;
    let mut resynthesized = false;

    if let DubbingRecheckDecision::Resynthesize { alignment_speed } =
        crate::core::tts::recheck_dubbing_cue(
            prepared,
            synthesized_ms,
            applied_alignment_speed,
            false,
        )
    {
        let (speed, effective_alignment_speed) = crate::core::tts::synthesis_speed_for_alignment(
            prepared.config.global_speed,
            alignment_speed,
            prepared.synthesis_speed_min,
            prepared.synthesis_speed_max,
        );
        let second =
            synthesize_prepared_dubbing_once(state, ffmpeg, prepared, speed, cancelled.clone())
                .await?;
        synthesized_ms = second.duration_ms;
        applied_alignment_speed = effective_alignment_speed;
        resynthesized = true;
    }

    crate::core::tts::complete_dubbing_cue(
        &state.app_config_dir,
        ffmpeg,
        prepared,
        crate::core::tts::DubbingCueCompletion {
            synthesized_ms,
            applied_alignment_speed,
            resynthesized,
            calibration_source_ms,
        },
        cancelled,
    )
    .await
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
    let _power_save_lease = state.power_save.acquire(format!("tts:{generation_id}"));

    let prepared = match crate::core::tts::prepare_dubbing_cue(&state.app_config_dir, &request) {
        Ok(prepared) => prepared,
        Err(error) => {
            state.tts_controls.write().await.remove(&generation_id);
            return Err(error.to_string());
        }
    };
    let result =
        synthesize_and_align_dubbing_cue(state.inner(), &ffmpeg, &prepared, cancelled.clone())
            .await;
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
    let _power_save_lease = state.power_save.acquire(format!("tts:{generation_id}"));
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
    let _power_save_lease = state.power_save.acquire(format!("tts:{generation_id}"));
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
pub fn discover_mixed_batch_inputs(
    paths: Vec<String>,
    recursive: Option<bool>,
) -> Result<crate::core::batch::MixedBatchInputs, String> {
    crate::core::batch::discover_mixed_inputs(&paths, recursive.unwrap_or(true))
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
    #[serde(default)]
    pub provided_subtitle_path: Option<String>,
    pub engine_id: String,
    pub model_id: String,
    pub source_language: Option<String>,
    pub target_language: Option<String>,
    pub translation_content_mode: Option<String>,
    pub output_format: Option<String>,
    pub output_name: Option<String>,
    pub strip_chinese_punctuation: Option<bool>,
    pub review_required: Option<bool>,
    pub max_subtitle_chars: Option<i32>,
    #[serde(default)]
    pub pipeline: Option<PipelineConfig>,
}

fn normalize_pipeline_config(
    task_type: TaskType,
    requested: Option<PipelineConfig>,
) -> Result<Option<PipelineConfig>, String> {
    let Some(config) = requested else {
        return Ok(None);
    };
    if !config.enable_dubbing
        && !config.enable_compose
        && !config.subtitle_review
        && !config.dubbing_review
    {
        return Ok(None);
    }
    if (config.enable_dubbing || config.enable_compose) && task_type == TaskType::TranslateOnly {
        return Err("配音或成片目标需要音视频输入；仅翻译任务只能输出字幕".into());
    }
    if config.dubbing_review && !config.enable_dubbing {
        return Err("配音确认闸门需要先启用配音目标".into());
    }
    if config.enable_dubbing {
        let dubbing = config
            .dubbing
            .as_ref()
            .ok_or_else(|| "已选择配音目标，但没有配音引擎配置".to_string())?;
        let engine = dubbing.engine.trim().to_ascii_lowercase();
        if !matches!(engine.as_str(), "local" | "cloud") {
            return Err("配音引擎只支持 local 或 cloud".into());
        }
        let target_id = dubbing.model_or_provider_id.trim();
        if target_id.is_empty() || target_id.len() > 200 || target_id.chars().any(char::is_control)
        {
            return Err("配音模型或服务实例不能为空".into());
        }
        if engine == "cloud" && uuid::Uuid::parse_str(target_id).is_err() {
            return Err("在线 TTS 实例 ID 无效".into());
        }
        if dubbing.voice.chars().any(char::is_control) || dubbing.voice.chars().count() > 200 {
            return Err("配音音色 ID 无效".into());
        }
        if !dubbing.global_speed.is_finite() || !(0.5..=2.0).contains(&dubbing.global_speed) {
            return Err("整体语速必须在 0.5-2.0 之间".into());
        }
        if dubbing
            .num_steps
            .is_some_and(|steps| !(1..=20).contains(&steps))
        {
            return Err("配音推理步数必须在 1-20 之间".into());
        }
        if dubbing
            .reference_text
            .as_ref()
            .is_some_and(|text| text.len() > 20_000 || text.contains('\0'))
        {
            return Err("参考文本不能包含空字符，且不能超过 20 KB".into());
        }
        if let Some(reference_audio_path) = dubbing
            .reference_audio_path
            .as_deref()
            .map(str::trim)
            .filter(|path| !path.is_empty())
        {
            let reference_audio_path = Path::new(reference_audio_path);
            if !reference_audio_path.is_absolute() || !reference_audio_path.is_file() {
                return Err("参考音频必须是存在的绝对文件路径".into());
            }
        }
    }
    if config.enable_compose {
        let compose = config
            .compose
            .as_ref()
            .ok_or_else(|| "已选择成片目标，但没有视频合成配置".to_string())?;
        if !matches!(
            compose.audio_mode.trim(),
            "keep" | "replace" | "mix" | "add-track"
        ) {
            return Err("成片音频模式只支持 keep、replace、mix 或 add-track".into());
        }
        if compose.audio_mode.trim() != "keep" && !config.enable_dubbing {
            return Err("替换、混合或新增音轨前，需要先启用配音目标".into());
        }
        if !matches!(
            compose.encoder_mode.trim(),
            "auto" | "cpu" | "hardware" | "hw"
        ) {
            return Err("成片编码模式只支持 auto、cpu 或 hardware".into());
        }
        if let Some(style) = compose.style.as_ref() {
            style.validate()?;
        }
    }
    let normalized_dubbing = config.dubbing.map(|mut value| {
        value.engine = value.engine.trim().to_ascii_lowercase();
        value.model_or_provider_id = value.model_or_provider_id.trim().to_string();
        value.voice = value.voice.trim().to_string();
        value.reference_audio_path = value
            .reference_audio_path
            .map(|path| path.trim().to_string())
            .filter(|path| !path.is_empty());
        value.reference_text = value
            .reference_text
            .map(|text| text.trim().to_string())
            .filter(|text| !text.is_empty());
        value
    });
    let normalized_compose = config.compose.map(|mut value| {
        value.audio_mode = value.audio_mode.trim().to_ascii_lowercase();
        value.encoder_mode = value.encoder_mode.trim().to_ascii_lowercase();
        value.style = value.style.map(|style| style.normalized());
        value
    });
    Ok(Some(PipelineConfig::for_task(
        task_type,
        config.enable_dubbing,
        config.enable_compose,
        config.subtitle_review,
        config.dubbing_review && config.enable_dubbing,
        normalized_dubbing,
        normalized_compose,
    )))
}

fn validate_pipeline_input_contract(
    media_path: &Path,
    output_format: &str,
    pipeline: Option<&PipelineConfig>,
) -> Result<(), String> {
    let Some(pipeline) = pipeline else {
        return Ok(());
    };
    if pipeline.has_downstream() && output_format == "txt" {
        return Err("配音或成片需要带时间轴的字幕格式，请选择 SRT、VTT、ASS 或 LRC".into());
    }
    if pipeline.enable_compose {
        let extension = media_path
            .extension()
            .and_then(|value| value.to_str())
            .map(str::to_ascii_lowercase)
            .unwrap_or_default();
        if !matches!(
            extension.as_str(),
            "mp4" | "mkv" | "mov" | "avi" | "webm" | "m4v" | "mpeg" | "mpg" | "ts" | "m2ts"
        ) {
            return Err("成片目标需要包含画面的源视频；纯音频只能输出字幕或配音音轨".into());
        }
    }
    Ok(())
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
        validate_subtitle_input_file(&media_path)?;
    }

    let provided_subtitle_path = req
        .provided_subtitle_path
        .map(|path| path.trim().to_string())
        .filter(|path| !path.is_empty())
        .map(|path| validate_existing_file_path(&path, "Provided subtitle file"))
        .transpose()?;
    if task_type == TaskType::TranslateOnly && provided_subtitle_path.is_some() {
        return Err("仅翻译任务的输入本身就是字幕，不能再附加配对字幕".into());
    }
    if let Some(path) = provided_subtitle_path.as_deref() {
        validate_subtitle_input_file(path)?;
    }

    let output_format = validate_subtitle_output_format(req.output_format)?;
    let max_subtitle_chars = validate_max_subtitle_chars(req.max_subtitle_chars)?;
    let output_name = validate_output_name_template(req.output_name)?;
    let translation_content_mode = validate_translation_content_mode(req.translation_content_mode)?;
    let source_language = validate_source_language_for_engine(
        if provided_subtitle_path.is_some() {
            "provided-subtitle"
        } else {
            &engine_id
        },
        req.source_language,
    )?;
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

    let pipeline = normalize_pipeline_config(task_type, req.pipeline)?;
    validate_pipeline_input_contract(
        &media_path,
        output_format.as_deref().unwrap_or("srt"),
        pipeline.as_ref(),
    )?;
    Ok(task_queue::create_task(CreateTaskParams {
        task_type,
        media_path: media_path.to_string_lossy().to_string(),
        provided_subtitle_path: provided_subtitle_path
            .map(|path| path.to_string_lossy().to_string()),
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
        max_subtitle_chars,
        pipeline,
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

    if let Some(cloud_task) = new_tasks.iter().find(|task| {
        task.engine_id == crate::core::asr::cloud::CLOUD_ASR_ENGINE_ID
            && task.provided_subtitle_path.is_none()
    }) {
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
        provided_subtitle_path: None,
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
        max_subtitle_chars: 0,
        pipeline: None,
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

#[tauri::command]
pub fn list_subtitle_style_presets(
    state: State<'_, AppState>,
) -> Result<Vec<SubtitleStylePreset>, String> {
    style_presets::load_style_presets(&state.app_config_dir)
}

#[tauri::command]
pub fn save_subtitle_style_preset(
    state: State<'_, AppState>,
    request: SaveSubtitleStylePresetRequest,
) -> Result<SubtitleStylePreset, String> {
    style_presets::save_style_preset(&state.app_config_dir, request)
}

#[tauri::command]
pub fn delete_subtitle_style_preset(
    state: State<'_, AppState>,
    preset_id: String,
) -> Result<String, String> {
    style_presets::delete_style_preset(&state.app_config_dir, &preset_id)
}

#[tauri::command]
pub fn reorder_subtitle_style_presets(
    state: State<'_, AppState>,
    ordered_ids: Vec<String>,
) -> Result<Vec<SubtitleStylePreset>, String> {
    style_presets::reorder_style_presets(&state.app_config_dir, &ordered_ids)
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

    let mut starts = Vec::new();
    {
        let mut controls = state.task_controls.write().await;
        for task in &approved {
            if task.status != TaskStatus::Pending {
                continue;
            }
            let (cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);
            controls.insert(task.id.clone(), cancel_tx);
            starts.push((task.id.clone(), cancel_rx));
        }
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
        let mut resume_stage = None;
        let mut pipeline_finished = false;
        if let Some(pipeline) = task.pipeline.as_mut() {
            let review_stage = pipeline
                .current_stage
                .filter(|kind| {
                    pipeline.stage(*kind).is_some_and(|stage| {
                        stage.status == task_queue::PipelineStageStatus::Review
                    })
                })
                .or_else(|| {
                    pipeline
                        .stages
                        .iter()
                        .find(|stage| stage.status == task_queue::PipelineStageStatus::Review)
                        .map(|stage| stage.kind)
                });
            if let Some(review_stage) = review_stage {
                if review_stage == task_queue::PipelineStageKind::Dub {
                    // 严重超长会把“配音”本身停在审核态。用户在配音工作台
                    // 处理完对应句后，这里必须重跑当前节点完成导出，而不是
                    // 把未生成的音轨误标为已完成并直接进入合成。
                    if let Some(stage) = pipeline.stage_mut(review_stage) {
                        stage.status = task_queue::PipelineStageStatus::Pending;
                        stage.progress = 0.0;
                        stage.message = "继续检查配音对齐".into();
                        stage.error = None;
                        stage.completed_at = None;
                    }
                    resume_stage = Some(review_stage);
                } else {
                    if let Some(stage) = pipeline.stage_mut(review_stage) {
                        stage.status = task_queue::PipelineStageStatus::Done;
                        stage.progress = 1.0;
                        stage.message = "已确认".into();
                        stage.error = None;
                        stage.completed_at = Some(reviewed_at.to_string());
                    }
                    resume_stage = pipeline
                        .stages
                        .iter()
                        .find(|stage| stage.status == task_queue::PipelineStageStatus::Pending)
                        .map(|stage| stage.kind);
                    if resume_stage == Some(task_queue::PipelineStageKind::Done) {
                        if let Some(done) = pipeline.stage_mut(task_queue::PipelineStageKind::Done)
                        {
                            done.status = task_queue::PipelineStageStatus::Done;
                            done.progress = 1.0;
                            done.message = "流水线已完成".into();
                            done.started_at = Some(reviewed_at.to_string());
                            done.completed_at = Some(reviewed_at.to_string());
                        }
                        resume_stage = None;
                        pipeline_finished = true;
                    }
                }
                pipeline.current_stage = resume_stage;
            }
        }
        if pipeline_finished || task.pipeline.is_none() {
            task.status = TaskStatus::Done;
            task.progress = 1.0;
            task.status_message = "审核通过".into();
        } else if resume_stage.is_some() {
            task.status = TaskStatus::Pending;
            task.status_message = "审核通过，正在继续后续处理...".into();
        } else {
            task.status = TaskStatus::Done;
            task.progress = 1.0;
            task.status_message = "审核通过".into();
        }
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
    if let Some(pipeline) = task.pipeline.as_mut() {
        pipeline.prepare_current_stage_for_resume("已暂停，稍后从当前阶段继续");
    }
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
pub async fn get_logs(
    state: State<'_, AppState>,
    query: LogQuery,
) -> std::result::Result<Vec<LogEntry>, String> {
    logs::query_logs(&state.app_config_dir, query).await
}

#[tauri::command]
pub fn get_log_dates(state: State<'_, AppState>) -> std::result::Result<Vec<String>, String> {
    logs::available_dates(&state.app_config_dir)
}

#[tauri::command]
pub async fn clear_logs(
    state: State<'_, AppState>,
    project_id: Option<String>,
) -> std::result::Result<(), String> {
    logs::clear_logs(&state.app_config_dir, project_id.as_deref()).await
}

#[tauri::command]
pub async fn add_log(
    app: AppHandle,
    state: State<'_, AppState>,
    level: String,
    message: String,
    task_id: Option<String>,
    project_id: Option<String>,
) -> std::result::Result<LogEntry, String> {
    if message.trim().is_empty() {
        return Err("日志内容不能为空".into());
    }
    if let Some(task_id) = task_id.as_deref() {
        validate_task_id(task_id)?;
    }
    let entry = logs::manual_entry(&level, &message, task_id, project_id)?;
    let entry = logs::append_entry(&state.app_config_dir, entry).await?;
    app.emit(logs::LOG_EVENT, entry.clone()).ok();
    Ok(entry)
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
    /// hard 字幕的视频编码方式：auto / cpu / hardware。
    pub encoder_mode: Option<String>,
    pub soft_subtitle: Option<bool>,
    pub audio_path: Option<String>,
    pub audio_mode: Option<String>,
    pub subtitle_language: Option<String>,
    pub subtitle_title: Option<String>,
    pub audio_language: Option<String>,
    pub audio_title: Option<String>,
}

#[tauri::command]
pub async fn get_video_encoder_info(app: AppHandle) -> Result<audio::HardwareEncoderInfo, String> {
    let ffmpeg_path = resolve_sidecar(&app, "ffmpeg")?;
    Ok(audio::get_hardware_encoder_info(&ffmpeg_path).await)
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
    let soft_subtitle = req.soft_subtitle.unwrap_or(false);
    let encoder_mode = if soft_subtitle {
        audio::VideoEncoderMode::Cpu
    } else {
        audio::VideoEncoderMode::parse(req.encoder_mode.as_deref())?
    };
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
    let video_encoding =
        audio::resolve_video_encoding(&ffmpeg_path, encoder_mode, &style, &video_path).await;
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
        soft_subtitle,
        audio_mode,
        video_encoding: Some(video_encoding.clone()),
        audio_path: audio_path.map(|path| path.to_string_lossy().to_string()),
        subtitle_language,
        subtitle_title,
        audio_language,
        audio_title,
        original_audio_tracks,
    };
    let initial_args = audio::compose_args(
        &video_path.to_string_lossy(),
        &subtitle_path.to_string_lossy(),
        &output_path.to_string_lossy(),
        &style,
        &options,
    )?;

    let (cancel_tx, mut cancel_rx) = tokio::sync::oneshot::channel::<()>();
    {
        let mut controls = state.burn_controls.write().await;
        if controls.contains_key(&burn_id) {
            return Err("A burn task is already running for this output path".to_string());
        }
        controls.insert(burn_id.clone(), cancel_tx);
    }
    let _power_save_lease = state.power_save.acquire(format!("burn:{burn_id}"));
    let video_path_display = req.video_path.clone();
    let mut total_duration_ms: Option<u64> = None;

    let mut args = initial_args;
    let mut encoding = video_encoding;
    let result = loop {
        let child = match tokio::process::Command::new(&ffmpeg_path)
            .args(&args)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .spawn()
        {
            Ok(child) => child,
            Err(error) => {
                if encoding.hardware && encoder_mode != audio::VideoEncoderMode::Cpu {
                    emit_burn_fallback(&app, &burn_id, &encoding.encoder_id);
                    encoding = audio::cpu_video_encoding_for_style(&style);
                    match audio::compose_args(
                        &video_path.to_string_lossy(),
                        &subtitle_path.to_string_lossy(),
                        &output_path.to_string_lossy(),
                        &style,
                        &audio::ComposeOptions {
                            video_encoding: Some(encoding.clone()),
                            ..options.clone()
                        },
                    ) {
                        Ok(cpu_args) => args = cpu_args,
                        Err(error) => break Err(error),
                    }
                    continue;
                }
                break Err(format!("Failed to start FFmpeg: {error}"));
            }
        };

        match run_burn_attempt(
            child,
            &app,
            &burn_id,
            &video_path_display,
            &output_path,
            &mut cancel_rx,
            &mut total_duration_ms,
        )
        .await
        {
            Ok(path) => break Ok(path),
            Err(BurnAttemptError::Cancelled) => break Err("Subtitle burning cancelled".into()),
            Err(BurnAttemptError::Failed(_error))
                if encoding.hardware && encoder_mode != audio::VideoEncoderMode::Cpu =>
            {
                emit_burn_fallback(&app, &burn_id, &encoding.encoder_id);
                if output_path.exists() {
                    let _ = std::fs::remove_file(&output_path);
                }
                encoding = audio::cpu_video_encoding_for_style(&style);
                match audio::compose_args(
                    &video_path.to_string_lossy(),
                    &subtitle_path.to_string_lossy(),
                    &output_path.to_string_lossy(),
                    &style,
                    &audio::ComposeOptions {
                        video_encoding: Some(encoding.clone()),
                        ..options.clone()
                    },
                ) {
                    Ok(cpu_args) => args = cpu_args,
                    Err(error) => break Err(error),
                }
                let _ = app.emit(
                    "subtitle-burn-updated",
                    BurnProgress {
                        burn_id: burn_id.clone(),
                        video_path: video_path_display.clone(),
                        progress: 0.0,
                    },
                );
                continue;
            }
            Err(BurnAttemptError::Failed(error)) => break Err(error),
        }
    };

    state.burn_controls.write().await.remove(&burn_id);

    if result.is_err() && output_path.exists() {
        let _ = std::fs::remove_file(&output_path);
    }

    result
}

#[derive(serde::Serialize, Clone)]
struct BurnProgress {
    burn_id: String,
    video_path: String,
    progress: f64,
}

#[derive(serde::Serialize, Clone)]
struct BurnFallback {
    burn_id: String,
    encoder: String,
}

fn emit_burn_fallback(app: &AppHandle, burn_id: &str, encoder: &str) {
    let _ = app.emit(
        "subtitle-burn-fallback",
        BurnFallback {
            burn_id: burn_id.to_string(),
            encoder: encoder.to_string(),
        },
    );
}

enum BurnAttemptError {
    Cancelled,
    Failed(String),
}

async fn run_burn_attempt(
    mut child: tokio::process::Child,
    app: &AppHandle,
    burn_id: &str,
    video_path: &str,
    output_path: &Path,
    cancel_rx: &mut tokio::sync::oneshot::Receiver<()>,
    total_duration_ms: &mut Option<u64>,
) -> Result<String, BurnAttemptError> {
    use tokio::io::AsyncBufReadExt;

    let Some(stderr) = child.stderr.take() else {
        let _ = child.kill().await;
        return Err(BurnAttemptError::Failed(
            "Unable to get FFmpeg error stream".into(),
        ));
    };
    let reader = tokio::io::BufReader::new(stderr);
    let result = tokio::select! {
        _ = &mut *cancel_rx => {
            let _ = child.kill().await;
            Err(BurnAttemptError::Cancelled)
        }
        res = async {
            let mut stderr_tail = std::collections::VecDeque::with_capacity(40);
            let mut lines_stream = reader.lines();
            while let Ok(Some(line)) = lines_stream.next_line().await {
                if stderr_tail.len() == 40 {
                    stderr_tail.pop_front();
                }
                stderr_tail.push_back(line.clone());
                if let Some(duration_ms) = audio::parse_duration_ms(&line) {
                    *total_duration_ms = Some(duration_ms);
                }
                if let Some(time_ms) = audio::parse_current_time_ms(&line) {
                    if let Some(total_ms) = *total_duration_ms {
                        let progress = (time_ms as f64 / total_ms as f64 * 100.0).clamp(0.0, 100.0);
                        let _ = app.emit(
                            "subtitle-burn-updated",
                            BurnProgress {
                                burn_id: burn_id.to_string(),
                                video_path: video_path.to_string(),
                                progress,
                            },
                        );
                    }
                }
            }
            let status = child
                .wait()
                .await
                .map_err(|error| BurnAttemptError::Failed(format!("Failed to wait for FFmpeg: {error}")))?;
            if status.success() {
                let _ = app.emit(
                    "subtitle-burn-updated",
                    BurnProgress {
                        burn_id: burn_id.to_string(),
                        video_path: video_path.to_string(),
                        progress: 100.0,
                    },
                );
                Ok(output_path.to_string_lossy().to_string())
            } else {
                let details = stderr_tail.into_iter().collect::<Vec<_>>().join("\n");
                if details.trim().is_empty() {
                    Err(BurnAttemptError::Failed("FFmpeg execution failed without diagnostic output".into()))
                } else {
                    Err(BurnAttemptError::Failed(format!("FFmpeg execution failed:\n{details}")))
                }
            }
        } => res
    };
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
    state: State<'_, AppState>,
    req: BurnSubtitleRequest,
) -> Result<String, String> {
    use tauri_plugin_opener::OpenerExt;

    let video_path = validate_media_path(&req.video_path)?;
    let subtitle_path = validate_existing_file_path(&req.subtitle_path, "Subtitle file")?;

    let temp_dir = settings::resolved_temp_dir(&state.app_config_dir)
        .map_err(|error| format!("无法准备字幕预览临时目录：{error}"))?;
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
        max_subtitle_chars: 0,
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
        max_subtitle_chars: 0,
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
        enable_thinking: None,
        thinking_control_bypassed: false,
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
        if req.enable_thinking.is_none() {
            req.enable_thinking = settings
                .translate_enable_thinking
                .get(&req.provider)
                .copied();
        }
        if req.proxy_url.is_none() && settings.proxy_enabled {
            req.proxy_url = Some(settings.proxy_url);
        }
    }

    // A provider test is always a fresh probe: an older backend may have
    // started accepting a parameter that was rejected earlier in this session.
    translation::clear_thinking_param_rejection(&req);

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

#[derive(Debug, Clone, serde::Serialize)]
pub struct StorageLayout {
    pub storage_root: String,
    pub whisper_models: settings::ResolvedStoragePath,
    pub parakeet_models: settings::ResolvedStoragePath,
    pub tts_models: settings::ResolvedStoragePath,
    pub temp_files: settings::ResolvedStoragePath,
}

#[tauri::command]
pub fn get_storage_layout(state: State<'_, AppState>) -> Result<StorageLayout, String> {
    let current = settings::load_settings(&state.app_config_dir).map_err(|e| e.to_string())?;
    let temp_files = settings::resolve_temp_storage_path(&current, &std::env::temp_dir());
    let tts_models =
        crate::core::tts::resolved_models_root(&state.app_config_dir).map_err(|e| e.to_string())?;
    Ok(StorageLayout {
        storage_root: current.storage_root.clone(),
        whisper_models: settings::resolve_whisper_models_path(&current),
        parakeet_models: settings::resolve_parakeet_models_path(&current),
        tts_models,
        temp_files,
    })
}

#[tauri::command]
pub fn get_power_save_status(
    state: State<'_, AppState>,
) -> crate::core::power_save::PowerSaveStatus {
    state.power_save.status()
}

#[tauri::command]
pub fn save_settings_cmd(
    state: State<'_, AppState>,
    new_settings: Settings,
) -> Result<Settings, String> {
    settings::save_settings(&state.app_config_dir, &new_settings).map_err(|e| e.to_string())?;
    // 并发数变更对之后新建的任务生效：保存时重建信号量，在飞任务持旧 permit 不受影响
    update_state_semaphore(&state, new_settings.max_concurrent_tasks);
    state
        .power_save
        .set_enabled(new_settings.prevent_sleep_during_tasks);
    crate::set_telemetry_enabled(new_settings.enable_telemetry);
    Ok(new_settings)
}

#[tauri::command]
pub fn reset_settings(state: State<'_, AppState>) -> Result<Settings, String> {
    let new_settings =
        settings::reset_settings(&state.app_config_dir).map_err(|e| e.to_string())?;
    update_state_semaphore(&state, new_settings.max_concurrent_tasks);
    state
        .power_save
        .set_enabled(new_settings.prevent_sleep_during_tasks);
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
    state
        .power_save
        .set_enabled(new_settings.prevent_sleep_during_tasks);
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
    state
        .power_save
        .set_enabled(new_settings.prevent_sleep_during_tasks);
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
    state
        .power_save
        .set_enabled(new_settings.prevent_sleep_during_tasks);
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
    validated_model_root(
        &settings::resolve_whisper_models_path(&settings).path,
        "Model",
    )
}

pub(crate) fn parakeet_models_dir(app_config_dir: &Path) -> Result<PathBuf, String> {
    let settings = settings::load_settings(app_config_dir).map_err(|e| e.to_string())?;
    validated_model_root(
        &settings::resolve_parakeet_models_path(&settings).path,
        "Parakeet model",
    )
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

fn validate_max_subtitle_chars(raw: Option<i32>) -> Result<i32, String> {
    let value = raw.unwrap_or(0);
    crate::core::subtitle::parse_subtitle_length_mode(value)
        .map(|_| value)
        .map_err(|error| error.to_string())
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

fn validate_subtitle_input_file(path: &Path) -> Result<(), String> {
    validate_translate_only_subtitle_extension(path)?;
    let bytes = std::fs::metadata(path)
        .map_err(|error| format!("Cannot inspect subtitle file {}: {error}", path.display()))?
        .len();
    if bytes > crate::core::subtitle::MAX_SUBTITLE_FILE_BYTES {
        return Err(format!(
            "Subtitle file exceeds the 20 MB limit: {}",
            path.display()
        ));
    }
    Ok(())
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
    audio::VideoEncoderMode::parse(req.encoder_mode.as_deref())?;
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
    // Windows 无法替换仍在运行的可执行文件。安装前关闭复用当前应用二进制
    // 的本地 TTS 子进程；后续若安装中止，下一次合成会自动重建 worker。
    state.tts_worker.stop().await;
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
    fn validate_max_subtitle_chars_enforces_tri_state_range() {
        for accepted in [-1, 0, 8, 40, 120] {
            assert_eq!(
                validate_max_subtitle_chars(Some(accepted)).unwrap(),
                accepted
            );
        }
        for rejected in [-2, 1, 7, 121] {
            assert!(validate_max_subtitle_chars(Some(rejected)).is_err());
        }
        assert_eq!(validate_max_subtitle_chars(None).unwrap(), 0);
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
    fn subtitle_task_input_rejects_files_over_twenty_megabytes() {
        let temp = tempfile::tempdir().unwrap();
        let subtitle = temp.path().join("oversized.srt");
        let file = std::fs::File::create(&subtitle).unwrap();
        file.set_len(crate::core::subtitle::MAX_SUBTITLE_FILE_BYTES + 1)
            .unwrap();

        assert!(validate_subtitle_input_file(&subtitle)
            .unwrap_err()
            .contains("20 MB"));
    }

    #[test]
    fn task_request_persists_a_valid_paired_subtitle_and_rejects_ambiguous_input() {
        let temp = tempfile::tempdir().unwrap();
        let media = temp.path().join("episode.mp4");
        let subtitle = temp.path().join("episode.zh.srt");
        std::fs::write(&media, b"media").unwrap();
        std::fs::write(&subtitle, b"1\n00:00:01,000 --> 00:00:02,000\nHello\n\n").unwrap();

        let request =
            |task_type: &str, media_path: &Path, paired: Option<&Path>| CreateTaskRequest {
                task_type: task_type.into(),
                media_path: media_path.to_string_lossy().to_string(),
                provided_subtitle_path: paired.map(|path| path.to_string_lossy().to_string()),
                engine_id: "parakeet-mlx".into(),
                model_id: "parakeet-tdt-0.6b-v2".into(),
                source_language: Some("zh".into()),
                target_language: Some("zh".into()),
                translation_content_mode: None,
                output_format: Some("srt".into()),
                output_name: None,
                strip_chinese_punctuation: None,
                review_required: None,
                max_subtitle_chars: None,
                pipeline: None,
            };

        let task = prepare_task_request(request("generate-only", &media, Some(&subtitle)))
            .expect("paired subtitle should be accepted");
        assert_eq!(
            task.provided_subtitle_path.as_deref(),
            Some(subtitle.to_string_lossy().as_ref())
        );

        let error = prepare_task_request(request("translate-only", &subtitle, Some(&subtitle)))
            .unwrap_err();
        assert!(error.contains("不能再附加配对字幕"));
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
                provided_subtitle_path: None,
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
                max_subtitle_chars: 0,
                pipeline: None,
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
    fn pipeline_review_approval_advances_to_persisted_downstream_stage() {
        let mut task = task_queue::create_task(CreateTaskParams {
            task_type: TaskType::GenerateAndTranslate,
            media_path: "/tmp/pipeline.mp4".into(),
            provided_subtitle_path: None,
            media_name: "pipeline.mp4".into(),
            engine_id: "parakeet-mlx".into(),
            model_id: "parakeet-tdt-0.6b-v2".into(),
            source_language: Some("auto".into()),
            target_language: Some("zh".into()),
            translation_content_mode: TranslationContentMode::TargetOnly,
            output_format: Some("srt".into()),
            output_name: None,
            strip_chinese_punctuation: false,
            review_required: false,
            max_subtitle_chars: 0,
            pipeline: Some(task_queue::PipelineConfig::for_task(
                TaskType::GenerateAndTranslate,
                true,
                true,
                true,
                false,
                Some(task_queue::PipelineDubbingConfig {
                    engine: "local".into(),
                    model_or_provider_id: "kokoro-multi-lang-v1_1".into(),
                    voice: "10".into(),
                    global_speed: 1.0,
                    reference_audio_path: None,
                    reference_text: None,
                    num_steps: None,
                }),
                Some(task_queue::PipelineComposeConfig {
                    soft_subtitle: false,
                    audio_mode: "replace".into(),
                    encoder_mode: "auto".into(),
                    style: None,
                }),
            )),
        });
        task.status = TaskStatus::Review;
        task.output_path = Some("/tmp/pipeline.finalsub.zh.srt".into());
        let pipeline = task.pipeline.as_mut().unwrap();
        for kind in [
            task_queue::PipelineStageKind::Transcribe,
            task_queue::PipelineStageKind::Translate,
        ] {
            let stage = pipeline.stage_mut(kind).unwrap();
            stage.status = task_queue::PipelineStageStatus::Done;
            stage.progress = 1.0;
        }
        let review = pipeline
            .stage_mut(task_queue::PipelineStageKind::SubtitleReview)
            .unwrap();
        review.status = task_queue::PipelineStageStatus::Review;
        review.progress = 1.0;
        pipeline.current_stage = Some(task_queue::PipelineStageKind::SubtitleReview);

        let id = task.id.clone();
        let original = HashMap::from([(id.clone(), task)]);
        let (approved_map, approved) =
            approve_review_tasks(&original, std::slice::from_ref(&id), "2026-07-20T00:00:00Z")
                .unwrap();
        assert_eq!(approved.len(), 1);
        assert_eq!(approved[0].status, TaskStatus::Pending);
        let advanced = approved_map[&id].pipeline.as_ref().unwrap();
        assert_eq!(
            advanced.current_stage,
            Some(task_queue::PipelineStageKind::Dub)
        );
        assert_eq!(
            advanced
                .stage(task_queue::PipelineStageKind::SubtitleReview)
                .unwrap()
                .status,
            task_queue::PipelineStageStatus::Done
        );
    }

    #[test]
    fn overlong_dubbing_review_retries_dub_before_downstream_export() {
        let mut task = task_queue::create_task(CreateTaskParams {
            task_type: TaskType::GenerateOnly,
            media_path: "/tmp/pipeline.mp4".into(),
            provided_subtitle_path: None,
            media_name: "pipeline.mp4".into(),
            engine_id: "parakeet-mlx".into(),
            model_id: "parakeet-tdt-0.6b-v2".into(),
            source_language: Some("auto".into()),
            target_language: None,
            translation_content_mode: TranslationContentMode::TargetOnly,
            output_format: Some("srt".into()),
            output_name: None,
            strip_chinese_punctuation: false,
            review_required: false,
            max_subtitle_chars: 0,
            pipeline: Some(task_queue::PipelineConfig::for_task(
                TaskType::GenerateOnly,
                true,
                true,
                false,
                false,
                Some(task_queue::PipelineDubbingConfig {
                    engine: "local".into(),
                    model_or_provider_id: "kokoro-multi-lang-v1_1".into(),
                    voice: "10".into(),
                    global_speed: 1.0,
                    reference_audio_path: None,
                    reference_text: None,
                    num_steps: None,
                }),
                Some(task_queue::PipelineComposeConfig {
                    soft_subtitle: false,
                    audio_mode: "replace".into(),
                    encoder_mode: "auto".into(),
                    style: None,
                }),
            )),
        });
        task.status = TaskStatus::Review;
        let pipeline = task.pipeline.as_mut().unwrap();
        pipeline
            .stage_mut(task_queue::PipelineStageKind::Transcribe)
            .unwrap()
            .status = task_queue::PipelineStageStatus::Done;
        let dub = pipeline
            .stage_mut(task_queue::PipelineStageKind::Dub)
            .unwrap();
        dub.status = task_queue::PipelineStageStatus::Review;
        dub.progress = 1.0;
        pipeline.current_stage = Some(task_queue::PipelineStageKind::Dub);

        let id = task.id.clone();
        let original = HashMap::from([(id.clone(), task)]);
        let (approved_map, approved) =
            approve_review_tasks(&original, std::slice::from_ref(&id), "2026-07-21T00:00:00Z")
                .unwrap();

        assert_eq!(approved[0].status, TaskStatus::Pending);
        let resumed = approved_map[&id].pipeline.as_ref().unwrap();
        assert_eq!(
            resumed.current_stage,
            Some(task_queue::PipelineStageKind::Dub)
        );
        assert_eq!(
            resumed
                .stage(task_queue::PipelineStageKind::Dub)
                .unwrap()
                .status,
            task_queue::PipelineStageStatus::Pending
        );
        assert!(resumed.dubbed_audio_path.is_none());
    }

    #[test]
    fn pipeline_request_rejects_media_targets_for_translation_only() {
        let config = task_queue::PipelineConfig::for_task(
            TaskType::TranslateOnly,
            false,
            true,
            false,
            false,
            None,
            Some(task_queue::PipelineComposeConfig {
                soft_subtitle: false,
                audio_mode: "keep".into(),
                encoder_mode: "auto".into(),
                style: None,
            }),
        );
        assert!(normalize_pipeline_config(TaskType::TranslateOnly, Some(config)).is_err());
    }

    #[test]
    fn pipeline_request_rejects_unresolved_audio_dependencies_and_bad_cloud_ids() {
        let review_without_dubbing = task_queue::PipelineConfig::for_task(
            TaskType::GenerateOnly,
            false,
            false,
            false,
            true,
            None,
            None,
        );
        assert!(
            normalize_pipeline_config(TaskType::GenerateOnly, Some(review_without_dubbing))
                .is_err()
        );

        let compose_without_dubbing = task_queue::PipelineConfig::for_task(
            TaskType::GenerateOnly,
            false,
            true,
            false,
            false,
            None,
            Some(task_queue::PipelineComposeConfig {
                soft_subtitle: false,
                audio_mode: "replace".into(),
                encoder_mode: "auto".into(),
                style: None,
            }),
        );
        assert!(
            normalize_pipeline_config(TaskType::GenerateOnly, Some(compose_without_dubbing))
                .is_err()
        );

        let invalid_cloud_id = task_queue::PipelineConfig::for_task(
            TaskType::GenerateOnly,
            true,
            false,
            false,
            false,
            Some(task_queue::PipelineDubbingConfig {
                engine: "cloud".into(),
                model_or_provider_id: "not-a-provider-uuid".into(),
                voice: String::new(),
                global_speed: 1.0,
                reference_audio_path: None,
                reference_text: None,
                num_steps: None,
            }),
            None,
        );
        assert!(normalize_pipeline_config(TaskType::GenerateOnly, Some(invalid_cloud_id)).is_err());
    }

    #[test]
    fn pipeline_request_preserves_and_validates_subtitle_style_snapshot() {
        let style = crate::core::style_presets::SubtitleStyle {
            font_size: 37,
            font_color: "&H0000FFFF".into(),
            margin_v: 48,
            ..Default::default()
        };
        let config = task_queue::PipelineConfig::for_task(
            TaskType::GenerateOnly,
            false,
            true,
            false,
            false,
            None,
            Some(task_queue::PipelineComposeConfig {
                soft_subtitle: false,
                audio_mode: "keep".into(),
                encoder_mode: "auto".into(),
                style: Some(style.clone()),
            }),
        );
        let normalized = normalize_pipeline_config(TaskType::GenerateOnly, Some(config))
            .unwrap()
            .unwrap();
        assert_eq!(normalized.compose.unwrap().style, Some(style.normalized()));

        let invalid = task_queue::PipelineConfig::for_task(
            TaskType::GenerateOnly,
            false,
            true,
            false,
            false,
            None,
            Some(task_queue::PipelineComposeConfig {
                soft_subtitle: false,
                audio_mode: "keep".into(),
                encoder_mode: "auto".into(),
                style: Some(crate::core::style_presets::SubtitleStyle {
                    font_name: "Arial,PrimaryColour=&H00FFFFFF".into(),
                    ..Default::default()
                }),
            }),
        );
        assert!(normalize_pipeline_config(TaskType::GenerateOnly, Some(invalid)).is_err());
    }

    #[test]
    fn pipeline_input_contract_requires_video_and_timed_subtitles_for_downstream_work() {
        let compose = task_queue::PipelineConfig::for_task(
            TaskType::GenerateOnly,
            false,
            true,
            false,
            false,
            None,
            Some(task_queue::PipelineComposeConfig {
                soft_subtitle: false,
                audio_mode: "keep".into(),
                encoder_mode: "auto".into(),
                style: None,
            }),
        );
        assert!(validate_pipeline_input_contract(
            Path::new("/tmp/audio.wav"),
            "srt",
            Some(&compose)
        )
        .is_err());
        assert!(validate_pipeline_input_contract(
            Path::new("/tmp/video.mp4"),
            "txt",
            Some(&compose)
        )
        .is_err());
        assert!(validate_pipeline_input_contract(
            Path::new("/tmp/video.mp4"),
            "srt",
            Some(&compose)
        )
        .is_ok());
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
            provided_subtitle_path: None,
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
            max_subtitle_chars: 0,
            pipeline: None,
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
            provided_subtitle_path: None,
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
            max_subtitle_chars: 0,
            pipeline: None,
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
    fn unified_storage_root_reuses_existing_parakeet_without_download() {
        let config = tempfile::tempdir().unwrap();
        let unified = tempfile::tempdir().unwrap();
        let parakeet_root = unified.path().join("parakeet-models");
        let model_dir = parakeet_root.join(crate::core::asr::parakeet::PARAKEET_MODEL_ID);
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
            storage_root: unified.path().to_string_lossy().into_owned(),
            ..Settings::default()
        };
        settings::save_settings(config.path(), &configured).unwrap();

        assert_eq!(parakeet_models_dir(config.path()).unwrap(), parakeet_root);
        let mut catalog = models::builtin_model_catalog();
        models::scan_model_status(
            &mut catalog,
            &unified.path().join("whisper-models"),
            &parakeet_root,
        );
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
