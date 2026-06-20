use std::path::{Path, PathBuf};
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager, State};

use crate::core::asr::parakeet::ParakeetMlxEngine;
use crate::core::asr::whisper::WhisperCppEngine;
use crate::core::asr::{AsrEngine, AsrModelRef, TranscribeJob};
use crate::core::audio;
use crate::core::models::{self, AsrModelInfo};
use crate::core::settings::{self, Settings};
use crate::core::subtitle::SubtitleTrack;
use crate::core::task_queue::{self, CreateTaskParams, Task, TaskMap, TaskStatus, TaskType};
use crate::core::translation::{self, TranslationProvider};
use crate::state::AppState;
use keyring::Entry;
use tauri_plugin_fs::FsExt;

const TASK_UPDATED_EVENT: &str = "task-updated";

#[derive(serde::Serialize)]
pub struct AppInfo {
    pub version: String,
    pub name: String,
}

#[tauri::command]
pub fn get_app_info() -> AppInfo {
    AppInfo {
        version: env!("CARGO_PKG_VERSION").into(),
        name: "FinalSub".into(),
    }
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
pub fn delete_model(state: State<'_, AppState>, model_id: String) -> Result<(), String> {
    let models_dir = whisper_models_dir(&state.app_config_dir)?;
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
    let task_type = match req.task_type.as_str() {
        "generate-and-translate" => TaskType::GenerateAndTranslate,
        "generate-only" => TaskType::GenerateOnly,
        "translate-only" => TaskType::TranslateOnly,
        _ => return Err(format!("未知任务类型：{}", req.task_type)),
    };

    let media_path = if task_type == TaskType::TranslateOnly {
        validate_existing_file_path(&req.media_path, "字幕文件")?
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
        let ext = media_path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_lowercase();
        if ext != "srt" {
            return Err("仅翻译模式只支持 .srt 字幕文件输入".into());
        }
    }

    let output_format = validate_subtitle_output_format(req.output_format)?;
    let source_language = req
        .source_language
        .map(|lang| lang.trim().to_string())
        .filter(|lang| !lang.is_empty());
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
        .unwrap_or("未命名媒体")
        .to_string();

    let task = task_queue::create_task(CreateTaskParams {
        task_type,
        media_path: media_path.to_string_lossy().to_string(),
        media_name,
        engine_id,
        model_id,
        source_language,
        target_language,
        output_format,
    });

    let task_clone = task.clone();
    let (cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);
    let tasks = state.tasks.clone();
    let task_controls = state.task_controls.clone();
    let app_config_dir = state.app_config_dir.clone();

    tasks.write().await.insert(task.id.clone(), task);
    task_controls
        .write()
        .await
        .insert(task_clone.id.clone(), cancel_tx);
    emit_task_update(&app, &task_clone);

    // 启动后台 worker
    crate::core::task_runner::start_task(
        app,
        tasks,
        task_controls,
        app_config_dir,
        task_clone.id.clone(),
        cancel_rx,
    );

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
    task.updated_at = chrono::Utc::now().to_rfc3339();
    let task_clone = task.clone();
    drop(tasks);

    // 向 cancel_sender 发送取消信号并移除
    if let Some(sender) = state.task_controls.write().await.remove(&task_id) {
        sender.send(true).ok();
    }

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
    let output_path = validate_new_output_path(&output_path, "音频输出路径")?;
    let args = audio::extract_audio_args(
        &video_path.to_string_lossy(),
        &output_path.to_string_lossy(),
    );

    let ffmpeg_path = resolve_sidecar(&app, "ffmpeg")?;
    let output = tokio::process::Command::new(ffmpeg_path)
        .args(&args)
        .output()
        .await
        .map_err(|e| format!("运行 FFmpeg 失败：{e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("FFmpeg 音频提取失败：{stderr}"));
    }

    Ok(output_path.to_string_lossy().to_string())
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
pub async fn burn_subtitle(app: AppHandle, req: BurnSubtitleRequest) -> Result<String, String> {
    let video_path = validate_media_path(&req.video_path)?;
    let subtitle_path = validate_existing_file_path(&req.subtitle_path, "字幕文件")?;
    let output_path = validate_new_output_path(&req.output_path, "视频输出路径")?;
    validate_burn_style(&req)?;
    let style = audio::BurnInStyleOptions {
        font_size: req.font_size,
        font_color: req.font_color,
        outline_color: req.outline_color,
        margin_v: req.margin_v,
    };
    let args = audio::burn_in_args(
        &video_path.to_string_lossy(),
        &subtitle_path.to_string_lossy(),
        &output_path.to_string_lossy(),
        &style,
    );

    let ffmpeg_path = resolve_sidecar(&app, "ffmpeg")?;
    let output = tokio::process::Command::new(ffmpeg_path)
        .args(&args)
        .output()
        .await
        .map_err(|e| format!("运行 FFmpeg 失败：{e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("FFmpeg 字幕烧录失败：{stderr}"));
    }

    Ok(output_path.to_string_lossy().to_string())
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
    let audio_path = validate_existing_file_path(&req.audio_path, "音频文件")?;
    let output_path = validate_new_output_path(&req.output_path, "字幕输出路径")?;

    let engine = WhisperCppEngine::new(whisper_bin, models_dir);
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
        .map_err(|e: std::io::Error| format!("写出 SRT 失败：{e}"))?;

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
    app: AppHandle,
    _state: State<'_, AppState>,
    req: TranscribeParakeetRequest,
) -> Result<String, String> {
    let audio_path = validate_existing_file_path(&req.audio_path, "音频文件")?;
    let output_path = validate_new_output_path(&req.output_path, "字幕输出路径")?;
    let uv_bin = crate::core::asr::parakeet::default_uv_bin();

    #[cfg(debug_assertions)]
    let transcribe_script = {
        let current_dir = std::env::current_dir().map_err(|e| e.to_string())?;
        let p1 = current_dir
            .join("src-tauri")
            .join("resources")
            .join("parakeet")
            .join("parakeet_transcribe.py");
        if p1.exists() {
            p1
        } else {
            current_dir
                .join("resources")
                .join("parakeet")
                .join("parakeet_transcribe.py")
        }
    };
    #[cfg(not(debug_assertions))]
    let transcribe_script = app
        .path()
        .resolve(
            "resources/parakeet/parakeet_transcribe.py",
            tauri::path::BaseDirectory::Resource,
        )
        .map_err(|e| format!("解析 Parakeet 脚本路径失败：{e}"))?;

    let cache_root = default_local_llm_dir();
    let ffmpeg_path = Some(resolve_sidecar(&app, "ffmpeg")?);

    let engine = ParakeetMlxEngine::new(uv_bin, transcribe_script, cache_root, ffmpeg_path);
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
        .map_err(|e: std::io::Error| format!("写出 SRT 失败：{e}"))?;

    Ok(output_path.to_string_lossy().to_string())
}

#[tauri::command]
pub fn list_translation_providers() -> Vec<TranslationProvider> {
    translation::builtin_providers()
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
    }

    if req.api_key.is_none() || req.api_key.as_deref().unwrap_or("").is_empty() {
        let service = "com.gravitypoet.finalsub";
        let account = format!("translate:{}:apiKey", req.provider);
        if let Ok(entry) = Entry::new(service, &account) {
            if let Ok(pwd) = entry.get_password() {
                req.api_key = Some(pwd);
            }
        }
    }

    translation::translate_text(&req)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_provider_secret(
    provider_id: String,
    field: String,
    value: String,
) -> std::result::Result<(), String> {
    let service = "com.gravitypoet.finalsub";
    let account = format!("translate:{provider_id}:{field}");
    let entry = Entry::new(service, &account).map_err(|e| e.to_string())?;
    entry.set_password(&value).map_err(|e| e.to_string())?;
    Ok(())
}

/// 仅返回「该 provider 字段是否已配置密钥」，绝不把明文密钥经 IPC 回传渲染层。
/// 翻译时由后端 test_translation 直接从 Keychain 取用，前端无需接触明文。
#[tauri::command]
pub fn has_provider_secret(
    provider_id: String,
    field: String,
) -> std::result::Result<bool, String> {
    let service = "com.gravitypoet.finalsub";
    let account = format!("translate:{provider_id}:{field}");
    let entry = Entry::new(service, &account).map_err(|e| e.to_string())?;
    match entry.get_password() {
        Ok(password) => Ok(!password.is_empty()),
        Err(keyring::Error::NoEntry) => Ok(false),
        Err(e) => Err(e.to_string()),
    }
}

#[tauri::command]
pub fn delete_provider_secret(
    provider_id: String,
    field: String,
) -> std::result::Result<(), String> {
    let service = "com.gravitypoet.finalsub";
    let account = format!("translate:{provider_id}:{field}");
    let entry = Entry::new(service, &account).map_err(|e| e.to_string())?;
    match entry.delete_credential() {
        Ok(_) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

#[tauri::command]
pub async fn get_ffmpeg_version(app: AppHandle) -> Result<String, String> {
    let ffmpeg_path = resolve_sidecar(&app, "ffmpeg")?;
    let output = tokio::process::Command::new(ffmpeg_path)
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

#[tauri::command]
pub fn export_config_to_path(
    state: State<'_, AppState>,
    output_path: String,
) -> Result<String, String> {
    let path = validate_json_output_path(&output_path)?;
    let json = settings::export_config(&state.app_config_dir).map_err(|e| e.to_string())?;
    let tmp_path = path.with_extension("json.tmp");
    std::fs::write(&tmp_path, json).map_err(|e| format!("写出配置失败：{e}"))?;
    std::fs::rename(&tmp_path, &path).map_err(|e| format!("保存配置失败：{e}"))?;
    Ok(path.to_string_lossy().to_string())
}

#[tauri::command]
pub fn import_config_from_path(
    state: State<'_, AppState>,
    input_path: String,
) -> Result<Settings, String> {
    let path = validate_json_input_path(&input_path)?;
    let json = std::fs::read_to_string(&path).map_err(|e| format!("读取配置失败：{e}"))?;
    settings::import_config(&state.app_config_dir, &json).map_err(|e| e.to_string())
}

fn scan_models_for_state(state: &AppState) -> Result<Vec<AsrModelInfo>, String> {
    let whisper_dir = whisper_models_dir(&state.app_config_dir)?;
    let parakeet_dir = default_local_llm_dir().join("parakeet-models");
    let mut catalog = state.models.clone();
    models::scan_model_status(&mut catalog, &whisper_dir, &parakeet_dir);
    Ok(catalog)
}
pub(crate) fn whisper_models_dir(app_config_dir: &Path) -> Result<PathBuf, String> {
    let settings = settings::load_settings(app_config_dir).map_err(|e| e.to_string())?;
    let path = expand_home_path(&settings.models_path);
    if !path.is_absolute() {
        return Err("模型路径必须是绝对路径".into());
    }
    Ok(path)
}

pub(crate) fn default_local_llm_dir() -> PathBuf {
    std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/tmp"))
        .join("Tools/Local-LLM")
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
        return Err(format!("{label}不能为空"));
    }
    if !path.is_absolute() {
        return Err(format!("{label}必须是绝对路径"));
    }
    if !path.is_file() {
        return Err(format!("{label}不存在：{}", path.display()));
    }
    Ok(path)
}

fn validate_new_output_path(raw: &str, label: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(raw.trim());
    if path.as_os_str().is_empty() {
        return Err(format!("{label}不能为空"));
    }
    if !path.is_absolute() {
        return Err(format!("{label}必须是绝对路径"));
    }
    let parent = path.parent().ok_or_else(|| format!("{label}缺少父目录"))?;
    if !parent.is_dir() {
        return Err(format!("{label}父目录不存在：{}", parent.display()));
    }
    if path.exists() {
        return Err(format!(
            "{label}已存在，为避免覆盖请重新选择：{}",
            path.display()
        ));
    }
    Ok(path)
}

fn validate_json_output_path(raw: &str) -> Result<PathBuf, String> {
    let path = validate_new_output_path(raw, "配置导出路径")?;
    validate_json_extension(&path)?;
    Ok(path)
}

fn validate_json_input_path(raw: &str) -> Result<PathBuf, String> {
    let path = validate_existing_file_path(raw, "配置文件")?;
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
        return Err("配置文件必须是 .json 文件".into());
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
        _ => Err("输出格式仅支持 srt、vtt、txt、lrc、ass".into()),
    }
}

fn validate_burn_style(req: &BurnSubtitleRequest) -> Result<(), String> {
    if let Some(font_size) = req.font_size {
        if !(10..=120).contains(&font_size) {
            return Err("字幕字号必须在 10-120 之间".into());
        }
    }
    if let Some(margin_v) = req.margin_v {
        if margin_v > 1_000 {
            return Err("字幕垂直边距不能超过 1000".into());
        }
    }
    if let Some(ref color) = req.font_color {
        validate_ass_color("字体颜色", color)?;
    }
    if let Some(ref color) = req.outline_color {
        validate_ass_color("描边颜色", color)?;
    }
    Ok(())
}

fn validate_ass_color(label: &str, value: &str) -> Result<(), String> {
    let valid = value.len() == 10
        && value.starts_with("&H")
        && value[2..].chars().all(|c| c.is_ascii_hexdigit());
    if valid {
        Ok(())
    } else {
        Err(format!("{label}必须使用 ASS 颜色格式，例如 &H00FFFFFF"))
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

        Err(format!("开发环境下找不到 sidecar 二进制：{}", name))
    }

    #[cfg(not(debug_assertions))]
    {
        let exe_path =
            std::env::current_exe().map_err(|e| format!("获取当前可执行文件路径失败：{e}"))?;
        let exe_dir = exe_path
            .parent()
            .ok_or_else(|| "无法获取可执行文件所在目录".to_string())?;

        let base_name = PathBuf::from(name)
            .file_name()
            .ok_or_else(|| format!("无效的 sidecar 名字：{name}"))?
            .to_os_string();

        let target_path = exe_dir.join(&base_name);
        if target_path.exists() {
            Ok(target_path)
        } else {
            Err(format!(
                "生产环境下找不到 sidecar 二进制：{}",
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
            .ok_or_else(|| "无法解析所在目录".to_string())?
    };
    if is_sensitive_dir(&dir) {
        return Err("拒绝授权敏感目录".to_string());
    }
    app.fs_scope()
        .allow_directory(&dir, false)
        .map_err(|e| e.to_string())?;
    Ok(())
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
    fn validate_new_output_path_rejects_existing_file() {
        let tmp = std::env::temp_dir().join("finalsub_existing_output.srt");
        std::fs::write(&tmp, b"exists").unwrap();

        let result = validate_new_output_path(tmp.to_str().unwrap(), "输出路径");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("已存在"));

        std::fs::remove_file(&tmp).unwrap();
    }

    #[test]
    fn validate_new_output_path_accepts_new_file_in_existing_parent() {
        let tmp = std::env::temp_dir().join("finalsub_new_output.srt");
        let _ = std::fs::remove_file(&tmp);

        let result = validate_new_output_path(tmp.to_str().unwrap(), "输出路径");
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
        let result = validate_ass_color("字体颜色", "white");
        assert!(result.is_err());
    }

    #[test]
    fn validate_ass_color_accepts_ass_hex() {
        assert!(validate_ass_color("字体颜色", "&H00FFFFFF").is_ok());
    }

    #[test]
    fn expand_home_path_expands_tilde_prefix() {
        let expanded = expand_home_path("~/Tools/Local-LLM");
        assert!(expanded.is_absolute());
        assert!(expanded.ends_with("Tools/Local-LLM"));
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
