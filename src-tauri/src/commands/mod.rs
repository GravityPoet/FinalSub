use std::path::{Path, PathBuf};
use std::time::Duration;

use tauri::{AppHandle, Emitter, State};
use tauri_plugin_shell::ShellExt;

use crate::core::audio;
use crate::core::models::AsrModelInfo;
use crate::core::subtitle::SubtitleTrack;
use crate::core::task_queue::{self, Task, TaskMap, TaskStatus, TaskType};
use crate::state::AppState;

const TASK_UPDATED_EVENT: &str = "task-updated";

#[derive(serde::Serialize)]
pub struct AppInfo {
    pub version: String,
    pub name: String,
}

#[tauri::command]
pub fn get_app_info() -> AppInfo {
    AppInfo {
        version: "0.1.0".into(),
        name: "FinalSub Tauri 预览版".into(),
    }
}

#[tauri::command]
pub fn list_asr_models(state: State<'_, AppState>) -> Vec<AsrModelInfo> {
    state.models.clone()
}

#[tauri::command]
pub fn get_model_status(state: State<'_, AppState>, model_id: String) -> Option<AsrModelInfo> {
    state.models.iter().find(|m| m.id == model_id).cloned()
}

#[tauri::command]
pub async fn create_task(
    app: AppHandle,
    state: State<'_, AppState>,
    media_path: String,
    media_name: String,
    engine_id: String,
    model_id: String,
    language: Option<String>,
) -> Result<Task, String> {
    let media_path = validate_media_path(&media_path)?;
    let media_name = normalize_media_name(&media_path, &media_name);
    let engine_id = validate_non_empty("engine_id", engine_id)?;
    let model_id = validate_non_empty("model_id", model_id)?;

    let task = task_queue::create_task(
        TaskType::GenerateOnly,
        media_path.to_string_lossy().to_string(),
        media_name,
        engine_id,
        model_id,
        language,
    );
    let task_clone = task.clone();
    state.tasks.write().await.insert(task.id.clone(), task);
    emit_task_update(&app, &task_clone);
    start_preview_worker(app, state.tasks.clone(), task_clone.id.clone());
    Ok(task_clone)
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
        .unwrap_or("未命名媒体")
        .to_string();

    let task = task_queue::create_task(
        TaskType::GenerateOnly,
        media_path.to_string_lossy().to_string(),
        media_name,
        "preview-pipeline".into(),
        "ffmpeg-sidecar-probe".into(),
        None,
    );
    let task_clone = task.clone();
    state.tasks.write().await.insert(task.id.clone(), task);
    emit_task_update(&app, &task_clone);
    start_preview_worker(app, state.tasks.clone(), task_clone.id.clone());
    Ok(task_clone)
}

#[tauri::command]
pub async fn list_tasks(state: State<'_, AppState>) -> Result<Vec<Task>, String> {
    Ok(state.tasks.read().await.values().cloned().collect())
}

#[tauri::command]
pub async fn cancel_task(
    app: AppHandle,
    state: State<'_, AppState>,
    task_id: String,
) -> Result<Task, String> {
    let mut tasks = state.tasks.write().await;
    let task = tasks
        .get_mut(&task_id)
        .ok_or_else(|| format!("task not found: {task_id}"))?;
    if matches!(task.status, TaskStatus::Done | TaskStatus::Error) {
        return Ok(task.clone());
    }
    task.status = task_queue::TaskStatus::Cancelled;
    task.progress = task.progress.clamp(0.0, 1.0);
    task.status_message = "已取消".into();
    let task_clone = task.clone();
    drop(tasks);
    emit_task_update(&app, &task_clone);
    Ok(task_clone)
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
pub async fn get_ffmpeg_version(app: AppHandle) -> Result<String, String> {
    let output = app
        .shell()
        .sidecar("binaries/ffmpeg")
        .map_err(|e| format!("准备 FFmpeg sidecar 失败：{e}"))?
        .args(["-version"])
        .output()
        .await
        .map_err(|e| format!("运行 FFmpeg sidecar 失败：{e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("FFmpeg sidecar 返回错误：{stderr}"));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let first_line = stdout.lines().next().unwrap_or("unknown");
    Ok(first_line.to_string())
}

fn validate_media_path(raw: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(raw.trim());
    if path.as_os_str().is_empty() {
        return Err("请选择音视频文件".into());
    }
    if !path.is_absolute() {
        return Err("音视频路径必须是绝对路径".into());
    }
    if !path.is_file() {
        return Err(format!("音视频文件不存在：{}", path.display()));
    }
    Ok(path)
}

fn normalize_media_name(path: &Path, media_name: &str) -> String {
    let trimmed = media_name.trim();
    if !trimmed.is_empty() {
        return trimmed.to_string();
    }

    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("未命名媒体")
        .to_string()
}

fn validate_non_empty(name: &str, value: String) -> Result<String, String> {
    let value = value.trim().to_string();
    if value.is_empty() {
        Err(format!("{name} 不能为空"))
    } else {
        Ok(value)
    }
}

fn start_preview_worker(app: AppHandle, tasks: TaskMap, task_id: String) {
    tauri::async_runtime::spawn(async move {
        let steps = [
            (0.12, "已加入任务队列"),
            (0.28, "已检测 FFmpeg sidecar"),
            (0.45, "已准备音频提取计划"),
            (0.64, "已预留识别引擎"),
            (0.82, "已准备字幕写入器"),
            (1.0, "预览任务完成"),
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

fn emit_task_update(app: &AppHandle, task: &Task) {
    let _ = app.emit(TASK_UPDATED_EVENT, task.clone());
}
