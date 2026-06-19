use std::path::PathBuf;
use std::time::Duration;

use tauri::{AppHandle, Emitter, State};
use tauri_plugin_shell::ShellExt;

use crate::core::audio;
use crate::core::asr::parakeet::ParakeetMlxEngine;
use crate::core::asr::whisper::WhisperCppEngine;
use crate::core::asr::{AsrEngine, AsrModelRef, TranscribeJob};
use crate::core::models::{self, AsrModelInfo};
use crate::core::settings::{self, Settings};
use crate::core::subtitle::SubtitleTrack;
use crate::core::task_queue::{self, CreateTaskParams, Task, TaskMap, TaskStatus, TaskType};
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
pub fn scan_models(_state: State<'_, AppState>) -> Vec<AsrModelInfo> {
    let whisper_dir = std::path::PathBuf::from("/Users/moonlitpoet/Tools/Local-LLM/whisper-models");
    let parakeet_dir = std::path::PathBuf::from("/Users/moonlitpoet/Tools/Local-LLM/parakeet-models");
    let mut catalog = models::builtin_model_catalog();
    models::scan_model_status(&mut catalog, &whisper_dir, &parakeet_dir);
    catalog
}

#[tauri::command]
pub fn delete_model(model_id: String) -> Result<(), String> {
    let models_dir = std::path::PathBuf::from("/Users/moonlitpoet/Tools/Local-LLM/whisper-models");
    models::delete_whisper_model(&models_dir, &model_id).map_err(|e| e.to_string())
}

#[derive(serde::Deserialize)]
pub struct CreateTaskRequest {
    pub task_type: String,
    pub media_path: String,
    pub engine_id: String,
    pub model_id: String,
    pub source_language: Option<String>,
    pub target_language: Option<String>,
    pub output_format: Option<String>,
}

#[tauri::command]
pub async fn create_task(
    app: AppHandle,
    state: State<'_, AppState>,
    req: CreateTaskRequest,
) -> Result<Task, String> {
    let media_path = validate_media_path(&req.media_path)?;
    let media_name = media_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("未命名媒体")
        .to_string();
    let engine_id = validate_non_empty("engine_id", req.engine_id)?;
    let model_id = validate_non_empty("model_id", req.model_id)?;

    let task_type = match req.task_type.as_str() {
        "generate-and-translate" => TaskType::GenerateAndTranslate,
        "generate-only" => TaskType::GenerateOnly,
        "translate-only" => TaskType::TranslateOnly,
        _ => return Err(format!("未知任务类型：{}", req.task_type)),
    };

    let task = task_queue::create_task(CreateTaskParams {
        task_type,
        media_path: media_path.to_string_lossy().to_string(),
        media_name,
        engine_id,
        model_id,
        source_language: req.source_language,
        target_language: req.target_language,
        output_format: req.output_format,
    });
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

    let task = task_queue::create_task(CreateTaskParams {
        task_type: TaskType::GenerateOnly,
        media_path: media_path.to_string_lossy().to_string(),
        media_name,
        engine_id: "preview-pipeline".into(),
        model_id: "ffmpeg-sidecar-probe".into(),
        source_language: None,
        target_language: None,
        output_format: None,
    });
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
pub async fn extract_audio(
    app: AppHandle,
    video_path: String,
    output_path: String,
) -> Result<String, String> {
    let video_path = validate_media_path(&video_path)?;
    let args = audio::extract_audio_args(
        &video_path.to_string_lossy(),
        &output_path,
    );

    let output = app
        .shell()
        .sidecar("binaries/ffmpeg")
        .map_err(|e| format!("准备 FFmpeg sidecar 失败：{e}"))?
        .args(&args)
        .output()
        .await
        .map_err(|e| format!("运行 FFmpeg 失败：{e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("FFmpeg 音频提取失败：{stderr}"));
    }

    Ok(output_path)
}

#[derive(serde::Deserialize)]
pub struct BurnSubtitleRequest {
    pub video_path: String,
    pub subtitle_path: String,
    pub output_path: String,
    pub font_size: Option<u32>,
    pub font_color: Option<String>,
    pub outline_color: Option<String>,
    pub margin_v: Option<u32>,
}

#[tauri::command]
pub async fn burn_subtitle(
    app: AppHandle,
    req: BurnSubtitleRequest,
) -> Result<String, String> {
    let video_path = validate_media_path(&req.video_path)?;
    let style = audio::BurnInStyleOptions {
        font_size: req.font_size,
        font_color: req.font_color,
        outline_color: req.outline_color,
        margin_v: req.margin_v,
    };
    let args = audio::burn_in_args(
        &video_path.to_string_lossy(),
        &req.subtitle_path,
        &req.output_path,
        &style,
    );

    let output = app
        .shell()
        .sidecar("binaries/ffmpeg")
        .map_err(|e| format!("准备 FFmpeg sidecar 失败：{e}"))?
        .args(&args)
        .output()
        .await
        .map_err(|e| format!("运行 FFmpeg 失败：{e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("FFmpeg 字幕烧录失败：{stderr}"));
    }

    Ok(req.output_path)
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
    state: State<'_, AppState>,
    req: TranscribeRequest,
) -> Result<String, String> {
    let whisper_bin = std::path::PathBuf::from("/opt/homebrew/bin/whisper-cli");
    let models_dir = std::path::PathBuf::from(&state.app_config_dir)
        .parent()
        .unwrap_or(&std::path::PathBuf::from("/tmp"))
        .join("whisper-models");

    let engine = WhisperCppEngine::new(whisper_bin, models_dir);
    let model_ref = AsrModelRef {
        engine_id: "whisper-cpp".into(),
        model_id: req.model_id.clone(),
        model_path: None,
    };

    engine.prepare(&model_ref).await.map_err(|e: crate::error::FinalSubError| e.to_string())?;

    let (tx, _rx) = tokio::sync::mpsc::channel(32);
    let job = TranscribeJob {
        audio_path: req.audio_path,
        output_path: req.output_path.clone(),
        language: req.language,
        model: model_ref,
    };

    let track = engine.transcribe(job, tx).await.map_err(|e: crate::error::FinalSubError| e.to_string())?;

    let srt = track.to_srt();
    tokio::fs::write(&req.output_path, &srt)
        .await
        .map_err(|e: std::io::Error| format!("写出 SRT 失败：{e}"))?;

    Ok(req.output_path)
}

#[derive(serde::Deserialize)]
pub struct TranscribeParakeetRequest {
    pub audio_path: String,
    pub output_path: String,
    pub language: Option<String>,
}

#[tauri::command]
pub async fn transcribe_parakeet(
    _state: State<'_, AppState>,
    req: TranscribeParakeetRequest,
) -> Result<String, String> {
    let uv_bin = crate::core::asr::parakeet::default_uv_bin();
    let transcribe_script = std::path::PathBuf::from(
        "/Users/moonlitpoet/Tools/AI-tools/FinalSub/extraResources/parakeet/parakeet_transcribe.py",
    );
    let cache_root = std::path::PathBuf::from("/Users/moonlitpoet/Tools/Local-LLM");
    let ffmpeg_path = Some(std::path::PathBuf::from(
        "/opt/homebrew/Cellar/ffmpeg/8.1.1/bin/ffmpeg",
    ));

    let engine = ParakeetMlxEngine::new(uv_bin, transcribe_script, cache_root, ffmpeg_path);
    let model_ref = AsrModelRef {
        engine_id: "parakeet-mlx".into(),
        model_id: "parakeet-tdt-0.6b-v2".into(),
        model_path: None,
    };

    engine.prepare(&model_ref).await.map_err(|e: crate::error::FinalSubError| e.to_string())?;

    let (tx, _rx) = tokio::sync::mpsc::channel(32);
    let job = TranscribeJob {
        audio_path: req.audio_path,
        output_path: req.output_path.clone(),
        language: req.language.or_else(|| Some("en".into())),
        model: model_ref,
    };

    let track = engine.transcribe(job, tx).await.map_err(|e: crate::error::FinalSubError| e.to_string())?;

    let srt = track.to_srt();
    tokio::fs::write(&req.output_path, &srt)
        .await
        .map_err(|e: std::io::Error| format!("写出 SRT 失败：{e}"))?;

    Ok(req.output_path)
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
    Ok(new_settings)
}

#[tauri::command]
pub fn reset_settings(state: State<'_, AppState>) -> Result<Settings, String> {
    settings::reset_settings(&state.app_config_dir).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn export_config(state: State<'_, AppState>) -> Result<String, String> {
    settings::export_config(&state.app_config_dir).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn import_config(state: State<'_, AppState>, json: String) -> Result<Settings, String> {
    settings::import_config(&state.app_config_dir, &json).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_media_path_empty() {
        let result = validate_media_path("");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("请选择"));
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
        assert!(result.unwrap_err().contains("绝对路径"));
    }

    #[test]
    fn validate_media_path_nonexistent() {
        let result = validate_media_path("/nonexistent/file.mp4");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("不存在"));
    }

    #[test]
    fn validate_media_path_directory() {
        let result = validate_media_path("/tmp");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("不存在"));
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
        assert_eq!(validate_non_empty("x", "  hello  ".into()).unwrap(), "hello");
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
}
