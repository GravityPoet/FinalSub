use tauri::State;

use crate::core::audio;
use crate::core::models::AsrModelInfo;
use crate::core::subtitle::SubtitleTrack;
use crate::core::task_queue::{self, Task, TaskType};
use crate::state::AppState;

#[derive(serde::Serialize)]
pub struct AppInfo {
    pub version: String,
    pub name: String,
}

#[tauri::command]
pub fn get_app_info() -> AppInfo {
    AppInfo {
        version: "0.1.0".into(),
        name: "FinalSub Tauri Preview".into(),
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
    state: State<'_, AppState>,
    media_path: String,
    media_name: String,
    engine_id: String,
    model_id: String,
    language: Option<String>,
) -> Result<Task, String> {
    let task = task_queue::create_task(
        TaskType::GenerateOnly,
        media_path,
        media_name,
        engine_id,
        model_id,
        language,
    );
    let task_clone = task.clone();
    state.tasks.write().await.insert(task.id.clone(), task);
    Ok(task_clone)
}

#[tauri::command]
pub async fn list_tasks(state: State<'_, AppState>) -> Result<Vec<Task>, String> {
    Ok(state.tasks.read().await.values().cloned().collect())
}

#[tauri::command]
pub async fn cancel_task(state: State<'_, AppState>, task_id: String) -> Result<Task, String> {
    let mut tasks = state.tasks.write().await;
    let task = tasks
        .get_mut(&task_id)
        .ok_or_else(|| format!("task not found: {task_id}"))?;
    task.status = task_queue::TaskStatus::Cancelled;
    task.status_message = "Cancelled".into();
    Ok(task.clone())
}

#[tauri::command]
pub fn normalize_srt(srt_content: String) -> Result<String, String> {
    let track = SubtitleTrack::from_srt(&srt_content).map_err(|e| e.to_string())?;
    Ok(track.to_srt())
}

#[tauri::command]
pub fn extract_audio_plan(
    ffmpeg_bin: String,
    video_path: String,
    output_path: String,
) -> audio::AudioExtractPlan {
    audio::audio_extract_plan(&ffmpeg_bin, &video_path, &output_path)
}

#[tauri::command]
pub fn get_ffmpeg_version() -> Result<String, String> {
    let output = std::process::Command::new("ffmpeg")
        .arg("-version")
        .output()
        .map_err(|e| format!("failed to run ffmpeg: {e}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let first_line = stdout.lines().next().unwrap_or("unknown");
    Ok(first_line.to_string())
}
