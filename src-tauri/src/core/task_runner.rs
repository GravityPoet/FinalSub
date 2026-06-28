use keyring::Entry;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::AppHandle;
use tauri::Manager;
use tokio::sync::{watch, RwLock};

use crate::commands::{default_local_llm_dir, resolve_sidecar, whisper_models_dir};
use crate::core::asr::parakeet::ParakeetMlxEngine;
use crate::core::asr::whisper::WhisperCppEngine;
use crate::core::asr::{AsrEngine, AsrModelRef, ProgressUpdate, TranscribeJob};
use crate::core::subtitle::{Cue, SubtitleTrack};
use crate::core::task_queue::{Task, TaskStatus, TaskType, TranslationContentMode};
use crate::core::translation::{
    builtin_providers, translate_text, TranslateRequest, TranslationProvider,
};

const MAX_OUTPUT_FILE_NAME_BYTES: usize = 240;
const TRANSLATION_PROGRESS_START: f32 = 0.80;
const TRANSLATION_PROGRESS_END: f32 = 0.95;
const AI_TRANSLATION_MAX_BATCH_CUES: usize = 24;
const AI_TRANSLATION_MAX_BATCH_CHARS: usize = 4_000;

enum TranslationAttemptResult {
    Success(String),
    Cancelled,
    Failed(String),
}

pub fn start_task(
    app: AppHandle,
    tasks: Arc<RwLock<HashMap<String, Task>>>,
    task_controls: Arc<RwLock<HashMap<String, watch::Sender<bool>>>>,
    app_config_dir: PathBuf,
    task_id: String,
    mut cancel_rx: watch::Receiver<bool>,
) {
    tauri::async_runtime::spawn(async move {
        let run_result = run_task_impl(
            &app,
            tasks.clone(),
            app_config_dir,
            &task_id,
            &mut cancel_rx,
        )
        .await;

        // 无论成功、失败还是取消，任务结束时都从 task_controls 移除
        {
            let mut controls = task_controls.write().await;
            controls.remove(&task_id);
        }

        if let Err(e) = run_result {
            // 检查当前状态是否已经是已取消，如果是，则不做 error 更新
            let is_cancelled = {
                let task_map = tasks.read().await;
                task_map
                    .get(&task_id)
                    .map(|t| t.status == TaskStatus::Cancelled)
                    .unwrap_or(false)
            };

            if !is_cancelled {
                let error_msg = e.to_string();
                let mut task_map = tasks.write().await;
                if let Some(task) = task_map.get_mut(&task_id) {
                    task.status = TaskStatus::Error;
                    task.error = Some(error_msg.clone());
                    task.status_message = format!("失败：{}", error_msg);
                    task.updated_at = chrono::Utc::now().to_rfc3339();
                    let task_clone = task.clone();
                    drop(task_map);
                    emit_task_update_internal(&app, &task_clone);
                }
            }
        }
    });
}

async fn run_task_impl(
    app: &AppHandle,
    tasks: Arc<RwLock<HashMap<String, Task>>>,
    app_config_dir: PathBuf,
    task_id: &str,
    cancel_rx: &mut tokio::sync::watch::Receiver<bool>,
) -> Result<(), String> {
    // 1. 获取任务信息
    let task = {
        let task_map = tasks.read().await;
        task_map
            .get(task_id)
            .cloned()
            .ok_or_else(|| format!("任务未找到：{}", task_id))?
    };

    let media_path = PathBuf::from(&task.media_path);
    let work_dir = app_config_dir.join("tasks").join(task_id);
    tokio::fs::create_dir_all(&work_dir)
        .await
        .map_err(|e| format!("创建工作目录失败：{}", e))?;

    // 检查取消
    if check_cancelled(cancel_rx) {
        let current_status = {
            let task_map = tasks.read().await;
            task_map
                .get(task_id)
                .map(|t| t.status)
                .unwrap_or(TaskStatus::Cancelled)
        };
        if current_status == TaskStatus::Paused {
            write_task_log(app, &app_config_dir, task_id, "任务启动前已暂停").await;
            return Ok(());
        }
        update_task_cancelled(app, tasks, task_id).await;
        return Ok(());
    }

    // 2. 并发限流队列等待
    update_task_progress(app, tasks.clone(), task_id, 0.0, "排队中，等待空闲并发槽...").await;
    write_task_log(app, &app_config_dir, task_id, "任务进入队列，等待并发通道...").await;

    let state = app.state::<crate::state::AppState>();
    let sem = {
        let lock = state.task_semaphore.lock().unwrap();
        lock.clone()
    };

    let _permit = tokio::select! {
        res = sem.acquire_owned() => {
            match res {
                Ok(p) => p,
                Err(e) => return Err(format!("获取并发通道失败：{}", e)),
            }
        }
        _ = cancel_rx.changed() => {
            let current_status = {
                let task_map = tasks.read().await;
                task_map.get(task_id).map(|t| t.status).unwrap_or(TaskStatus::Cancelled)
            };
            if current_status == TaskStatus::Paused {
                write_task_log(app, &app_config_dir, task_id, "排队已暂停").await;
                return Ok(());
            }
            update_task_cancelled(app, tasks, task_id).await;
            return Ok(());
        }
    };

    // 任务正式转入 Running 状态并启动
    {
        let mut task_map = tasks.write().await;
        if let Some(t) = task_map.get_mut(task_id) {
            t.status = TaskStatus::Running;
            t.status_message = "正在运行...".into();
            t.updated_at = chrono::Utc::now().to_rfc3339();
            let t_clone = t.clone();
            drop(task_map);
            emit_task_update_internal(app, &t_clone);
        }
    }
    write_task_log(app, &app_config_dir, task_id, "已获得并发通道，任务开始运行").await;

    let mut current_track: Option<SubtitleTrack> = None;

    if task.task_type != TaskType::TranslateOnly {
        let audio_output_path = work_dir.join("audio.wav");
        let asr_output_path = work_dir.join("asr.srt");

        // Check if ASR is already completed
        let mut asr_completed = false;
        if asr_output_path.exists()
            && std::fs::metadata(&asr_output_path)
                .map(|m| m.len() > 0)
                .unwrap_or(false)
        {
            if let Ok(srt_content) = std::fs::read_to_string(&asr_output_path) {
                if let Ok(track) = SubtitleTrack::from_srt(&srt_content) {
                    if !track.is_empty() {
                        current_track = Some(track);
                        asr_completed = true;
                        write_task_log(
                            app,
                            &app_config_dir,
                            task_id,
                            "发现已转录的字幕文件，跳过 ASR 转录阶段",
                        )
                        .await;
                        update_task_progress(
                            app,
                            tasks.clone(),
                            task_id,
                            0.80,
                            "ASR 转录已跳过 (已加载历史转录)",
                        )
                        .await;
                    }
                }
            }
        }

        if !asr_completed {
            // 2. 音频提取阶段 (0.00 - 0.15)
            let mut audio_extracted = false;
            if audio_output_path.exists()
                && std::fs::metadata(&audio_output_path)
                    .map(|m| m.len() > 0)
                    .unwrap_or(false)
            {
                audio_extracted = true;
                write_task_log(
                    app,
                    &app_config_dir,
                    task_id,
                    "发现已提取的音频文件，跳过音频提取阶段",
                )
                .await;
                update_task_progress(app, tasks.clone(), task_id, 0.15, "音频提取已跳过").await;
            }

            if !audio_extracted {
                update_task_progress(app, tasks.clone(), task_id, 0.0, "正在提取音频...").await;

                let ffmpeg_path = resolve_sidecar(app, "ffmpeg")?;
                let extract_args = crate::core::audio::extract_audio_args(
                    &task.media_path,
                    &audio_output_path.to_string_lossy(),
                );

                let mut ffmpeg_cmd = tokio::process::Command::new(&ffmpeg_path);
                ffmpeg_cmd.args(&extract_args);
                ffmpeg_cmd.kill_on_drop(true);

                let ffmpeg_fut = ffmpeg_cmd.output();
                tokio::pin!(ffmpeg_fut);

                let ffmpeg_res = tokio::select! {
                    res = &mut ffmpeg_fut => {
                        res.map_err(|e| format!("运行 FFmpeg 提取音频失败：{}", e))?
                    }
                    _ = cancel_rx.changed() => {
                        if *cancel_rx.borrow() {
                            let current_status = {
                                let task_map = tasks.read().await;
                                task_map.get(task_id).map(|t| t.status).unwrap_or(TaskStatus::Cancelled)
                            };
                            if current_status == TaskStatus::Paused {
                                write_task_log(app, &app_config_dir, task_id, "音频提取已暂停").await;
                                return Ok(());
                            }
                            update_task_cancelled(app, tasks, task_id).await;
                            return Ok(());
                        }
                        loop {
                            tokio::select! {
                                res = &mut ffmpeg_fut => {
                                    break res.map_err(|e| format!("运行 FFmpeg 提取音频失败：{}", e))?;
                                }
                                change_res = cancel_rx.changed() => {
                                    if change_res.is_err() || *cancel_rx.borrow() {
                                        let current_status = {
                                            let task_map = tasks.read().await;
                                            task_map.get(task_id).map(|t| t.status).unwrap_or(TaskStatus::Cancelled)
                                        };
                                        if current_status == TaskStatus::Paused {
                                            write_task_log(app, &app_config_dir, task_id, "音频提取已暂停").await;
                                            return Ok(());
                                        }
                                        update_task_cancelled(app, tasks, task_id).await;
                                        return Ok(());
                                    }
                                }
                            }
                        }
                    }
                };

                if !ffmpeg_res.status.success() {
                    let stderr = String::from_utf8_lossy(&ffmpeg_res.stderr);
                    return Err(format!("FFmpeg 音频提取失败：{}", stderr));
                }

                update_task_progress(
                    app,
                    tasks.clone(),
                    task_id,
                    0.15,
                    "音频提取完成，准备 ASR 模型...",
                )
                .await;
            }

            if check_cancelled(cancel_rx) {
                let current_status = {
                    let task_map = tasks.read().await;
                    task_map
                        .get(task_id)
                        .map(|t| t.status)
                        .unwrap_or(TaskStatus::Cancelled)
                };
                if current_status == TaskStatus::Paused {
                    write_task_log(app, &app_config_dir, task_id, "音频提取已暂停").await;
                    return Ok(());
                }
                update_task_cancelled(app, tasks, task_id).await;
                return Ok(());
            }

            // 3. ASR 转录阶段 (0.15 - 0.80)
            let engine: Box<dyn AsrEngine> = match task.engine_id.as_str() {
                "whisper-cpp" => {
                    let whisper_bin = resolve_sidecar(app, "whisper-cli")?;
                    let models_dir = whisper_models_dir(&app_config_dir)?;
                    let settings = crate::core::settings::load_settings(&app_config_dir)
                        .map_err(|e| format!("加载设置失败：{}", e))?;
                    
                    #[cfg(debug_assertions)]
                    let vad_model_path = {
                        let current_dir = std::env::current_dir().map_err(|e| e.to_string())?;
                        let p1 = current_dir
                            .join("src-tauri")
                            .join("resources")
                            .join("vad")
                            .join("ggml-silero-v5.1.2.bin");
                        if p1.exists() {
                            Some(p1)
                        } else {
                            let p2 = current_dir
                                .join("resources")
                                .join("vad")
                                .join("ggml-silero-v5.1.2.bin");
                            if p2.exists() { Some(p2) } else { None }
                        }
                    };
                    #[cfg(not(debug_assertions))]
                    let vad_model_path = app
                        .path()
                        .resolve(
                            "resources/vad/ggml-silero-v5.1.2.bin",
                            tauri::path::BaseDirectory::Resource,
                        )
                        .ok();

                    let options = crate::core::asr::whisper::WhisperOptions {
                        use_vad: settings.use_vad,
                        vad_threshold: settings.vad_threshold,
                        vad_min_speech_duration_ms: settings.vad_min_speech_duration_ms,
                        vad_min_silence_duration_ms: settings.vad_min_silence_duration_ms,
                        vad_max_speech_duration_s: settings.vad_max_speech_duration_s,
                        vad_speech_pad_ms: settings.vad_speech_pad_ms,
                        vad_samples_overlap: settings.vad_samples_overlap,
                        whisper_command: settings.whisper_command.clone(),
                        max_context: settings.max_context,
                        vad_model_path,
                    };
                    
                    Box::new(WhisperCppEngine::new(whisper_bin, models_dir, options))
                }
                "parakeet-mlx" => {
                    let uv_bin = default_uv_bin_fallback();

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
                    let ffmpeg_path = Some(resolve_sidecar(app, "ffmpeg")?);
                    Box::new(ParakeetMlxEngine::new(
                        uv_bin,
                        transcribe_script,
                        cache_root,
                        ffmpeg_path,
                    ))
                }
                "sensevoice" => {
                    let models_dir = whisper_models_dir(&app_config_dir)?;
                    Box::new(crate::core::asr::sensevoice::SenseVoiceEngine::new(models_dir))
                }
                "custom-command" => {
                    let settings = crate::core::settings::load_settings(&app_config_dir)
                        .map_err(|e| format!("加载设置失败：{}", e))?;
                    let models_dir = whisper_models_dir(&app_config_dir)?;
                    Box::new(crate::core::asr::custom::CustomCommandEngine::new(
                        settings.whisper_command,
                        models_dir,
                    ))
                }
                other => return Err(format!("不支持的 ASR 引擎：{}", other)),
            };

            let model_ref = AsrModelRef {
                engine_id: task.engine_id.clone(),
                model_id: task.model_id.clone(),
                model_path: None,
            };

            engine
                .prepare(&model_ref)
                .await
                .map_err(|e| e.to_string())?;
            update_task_progress(
                app,
                tasks.clone(),
                task_id,
                0.25,
                "正在进行 ASR 语音识别...",
            )
            .await;

            if check_cancelled(cancel_rx) {
                let current_status = {
                    let task_map = tasks.read().await;
                    task_map
                        .get(task_id)
                        .map(|t| t.status)
                        .unwrap_or(TaskStatus::Cancelled)
                };
                if current_status == TaskStatus::Paused {
                    write_task_log(app, &app_config_dir, task_id, "转录已暂停").await;
                    return Ok(());
                }
                update_task_cancelled(app, tasks, task_id).await;
                return Ok(());
            }

            let job = TranscribeJob {
                audio_path: audio_output_path.to_string_lossy().to_string(),
                output_path: asr_output_path.to_string_lossy().to_string(),
                language: task.source_language.clone(),
                model: model_ref,
            };

            let (progress_tx, mut progress_rx) = tokio::sync::mpsc::channel::<ProgressUpdate>(32);
            let transcribe_fut = engine.transcribe(job, progress_tx, Some(cancel_rx.clone()));
            tokio::pin!(transcribe_fut);

            let transcribe_res = loop {
                tokio::select! {
                    update_opt = progress_rx.recv() => {
                        if let Some(update) = update_opt {
                            let mapped_progress = 0.25 + (0.80 - 0.25) * update.progress;
                            update_task_progress(app, tasks.clone(), task_id, mapped_progress, &update.message).await;
                        }
                    }
                    res = &mut transcribe_fut => {
                        match res {
                            Ok(t) => {
                                break Ok(t);
                            }
                            Err(e) => {
                                if e.to_string().contains("已取消") || check_cancelled(cancel_rx) {
                                    let current_status = {
                                        let task_map = tasks.read().await;
                                        task_map.get(task_id).map(|t| t.status).unwrap_or(TaskStatus::Cancelled)
                                    };
                                    if current_status == TaskStatus::Paused {
                                        write_task_log(app, &app_config_dir, task_id, "转录已暂停").await;
                                        return Ok(());
                                    }
                                    update_task_cancelled(app, tasks, task_id).await;
                                    return Ok(());
                                }
                                break Err(format!("语音转录失败：{}", e));
                            }
                        }
                    }
                }
            };

            current_track = Some(transcribe_res?);
        }
    } else {
        // TranslateOnly 模式直接读取原字幕文件，按扩展名解析 SRT/VTT/ASS/LRC。
        update_task_progress(app, tasks.clone(), task_id, 0.0, "正在读取源字幕文件...").await;
        let sub_content = tokio::fs::read_to_string(&media_path)
            .await
            .map_err(|e| format!("读取源字幕文件失败：{}", e))?;
        let ext = media_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("srt")
            .to_lowercase();
        let track = SubtitleTrack::from_format(&sub_content, &ext)
            .map_err(|e| format!("解析字幕失败：{}", e))?;
        current_track = Some(track);
    }

    let mut track = current_track.ok_or_else(|| "未生成或解析到有效字幕轨道".to_string())?;
    if track.is_empty() {
        return Err("未生成或解析到有效字幕轨道".into());
    }
    let source_track = track.clone();

    if check_cancelled(cancel_rx) {
        let current_status = {
            let task_map = tasks.read().await;
            task_map
                .get(task_id)
                .map(|t| t.status)
                .unwrap_or(TaskStatus::Cancelled)
        };
        if current_status == TaskStatus::Paused {
            write_task_log(app, &app_config_dir, task_id, "字幕翻译启动前已暂停").await;
            return Ok(());
        }
        update_task_cancelled(app, tasks, task_id).await;
        return Ok(());
    }

    // 4. 翻译阶段 (0.80 - 0.95)
    let should_translate = task.task_type == TaskType::GenerateAndTranslate
        || task.task_type == TaskType::TranslateOnly;
    if should_translate {
        let settings =
            crate::core::settings::load_settings(&app_config_dir).map_err(|e| e.to_string())?;
        let provider = settings.translate_provider.clone();
        if provider.is_empty() {
            return Err("请先在翻译管理中配置翻译服务商".into());
        }

        update_task_progress(
            app,
            tasks.clone(),
            task_id,
            0.80,
            &format!("准备通过 {} 翻译字幕...", provider),
        )
        .await;

        let service = "com.gravitypoet.finalsub";
        let account = format!("translate:{}:apiKey", provider);
        let mut api_key = None;
        if let Ok(entry) = Entry::new(service, &account) {
            if let Ok(pwd) = entry.get_password() {
                api_key = Some(pwd);
            }
        }

        let provider_info = builtin_providers()
            .into_iter()
            .find(|item| item.id == provider);
        let api_url = configured_value(settings.translate_endpoints.get(&provider)).or_else(|| {
            provider_info
                .as_ref()
                .and_then(|item| configured_value(Some(&item.default_endpoint)))
        });
        let model_name = configured_value(settings.translate_models.get(&provider));
        let retry_times = settings.translate_retry_times;

        let mut secret_fields = std::collections::HashMap::new();
        if let Some(p) = &provider_info {
            for field in &p.secret_fields {
                let service = "com.gravitypoet.finalsub";
                let account = format!("translate:{}:{}", provider, field);
                if let Ok(entry) = Entry::new(service, &account) {
                    if let Ok(pwd) = entry.get_password() {
                        secret_fields.insert(field.clone(), pwd);
                    }
                }
            }
        }
        let secret_fields_opt = if secret_fields.is_empty() {
            None
        } else {
            Some(secret_fields)
        };

        let total_cues = track.cues.len();
        let source_lang = task
            .source_language
            .clone()
            .unwrap_or_else(|| "auto".into());
        let target_lang = task
            .target_language
            .clone()
            .ok_or_else(|| "目标语言未指定".to_string())?;

        let temp_translated_path = work_dir.join("translated.srt.tmp");
        let mut start_cue_index = 0;

        if temp_translated_path.exists() {
            if let Ok(temp_content) = std::fs::read_to_string(&temp_translated_path) {
                if let Ok(temp_track) = SubtitleTrack::from_srt(&temp_content) {
                    start_cue_index = restore_translated_checkpoint(&mut track, &temp_track);
                    if start_cue_index > 0 {
                        let msg = format!(
                            "发现已保存的翻译进度，已恢复 {}/{} 行{}",
                            start_cue_index,
                            total_cues,
                            if start_cue_index == total_cues {
                                "，准备写出字幕..."
                            } else {
                                "，继续翻译..."
                            }
                        );
                        write_task_log(app, &app_config_dir, task_id, &msg).await;
                        update_task_progress(
                            app,
                            tasks.clone(),
                            task_id,
                            translation_progress(start_cue_index, total_cues),
                            &msg,
                        )
                        .await;
                    }
                }
            }
        }

        let mut batch_translation_enabled = translation_supports_batch(provider_info.as_ref());
        let mut next_cue_index = start_cue_index;
        while next_cue_index < total_cues {
            if check_cancelled(cancel_rx) {
                handle_translation_stop(
                    app,
                    tasks.clone(),
                    &app_config_dir,
                    task_id,
                    &temp_translated_path,
                    &track,
                    next_cue_index,
                )
                .await;
                return Ok(());
            }

            let batch_end = if batch_translation_enabled {
                translation_batch_end(&track.cues, next_cue_index)
            } else {
                (next_cue_index + 1).min(total_cues)
            };
            if batch_translation_enabled && batch_end - next_cue_index > 1 {
                let batch_cues = &track.cues[next_cue_index..batch_end];
                let (batch_prompt, batch_keys) =
                    build_batch_translation_prompt(&source_lang, &target_lang, batch_cues);
                let batch_req = TranslateRequest {
                    text: batch_prompt,
                    source_language: source_lang.clone(),
                    target_language: target_lang.clone(),
                    provider: provider.clone(),
                    api_key: api_key.clone(),
                    api_url: api_url.clone(),
                    model_name: model_name.clone(),
                    secret_fields: secret_fields_opt.clone(),
                };

                match translate_with_retries(&batch_req, retry_times, cancel_rx).await {
                    TranslationAttemptResult::Success(raw_text) => {
                        match parse_batch_translation_response(&raw_text, &batch_keys) {
                            Ok(translated_batch) => {
                                for (offset, translated_text) in
                                    translated_batch.into_iter().enumerate()
                                {
                                    track.cues[next_cue_index + offset].text = translated_text;
                                }

                                next_cue_index = batch_end;
                                save_translation_checkpoint(
                                    &temp_translated_path,
                                    &track,
                                    next_cue_index,
                                );
                                let progress = translation_progress(next_cue_index, total_cues);
                                let msg = format!(
                                    "正在批量翻译字幕... ({}/{})",
                                    next_cue_index, total_cues
                                );
                                update_task_progress(app, tasks.clone(), task_id, progress, &msg)
                                    .await;
                                continue;
                            }
                            Err(parse_err) => {
                                batch_translation_enabled = false;
                                write_task_log(
                                    app,
                                    &app_config_dir,
                                    task_id,
                                    &format!(
                                        "批量翻译响应无法对齐，降级逐条翻译当前批次：{}",
                                        parse_err
                                    ),
                                )
                                .await;
                            }
                        }
                    }
                    TranslationAttemptResult::Cancelled => {
                        handle_translation_stop(
                            app,
                            tasks.clone(),
                            &app_config_dir,
                            task_id,
                            &temp_translated_path,
                            &track,
                            next_cue_index,
                        )
                        .await;
                        return Ok(());
                    }
                    TranslationAttemptResult::Failed(batch_err) => {
                        batch_translation_enabled = false;
                        write_task_log(
                            app,
                            &app_config_dir,
                            task_id,
                            &format!("批量翻译失败，降级逐条翻译当前批次：{}", batch_err),
                        )
                        .await;
                    }
                }
            }

            let req = TranslateRequest {
                text: track.cues[next_cue_index].text.clone(),
                source_language: source_lang.clone(),
                target_language: target_lang.clone(),
                provider: provider.clone(),
                api_key: api_key.clone(),
                api_url: api_url.clone(),
                model_name: model_name.clone(),
                secret_fields: secret_fields_opt.clone(),
            };

            let translated_text = match translate_with_retries(&req, retry_times, cancel_rx).await
            {
                TranslationAttemptResult::Success(text) => text,
                TranslationAttemptResult::Cancelled => {
                    handle_translation_stop(
                        app,
                        tasks.clone(),
                        &app_config_dir,
                        task_id,
                        &temp_translated_path,
                        &track,
                        next_cue_index,
                    )
                    .await;
                    return Ok(());
                }
                TranslationAttemptResult::Failed(last_err) => {
                    return Err(format!(
                        "翻译失败（尝试 {} 次）：{}",
                        retry_times + 1,
                        last_err
                    ));
                }
            };

            track.cues[next_cue_index].text = translated_text;
            next_cue_index += 1;
            save_translation_checkpoint(&temp_translated_path, &track, next_cue_index);

            let progress = translation_progress(next_cue_index, total_cues);
            let msg = format!("正在翻译字幕... ({}/{})", next_cue_index, total_cues);
            update_task_progress(app, tasks.clone(), task_id, progress, &msg).await;
        }

        let _ = std::fs::remove_file(&temp_translated_path);
    }

    if check_cancelled(cancel_rx) {
        let current_status = {
            let task_map = tasks.read().await;
            task_map
                .get(task_id)
                .map(|t| t.status)
                .unwrap_or(TaskStatus::Cancelled)
        };
        if current_status == TaskStatus::Paused {
            write_task_log(app, &app_config_dir, task_id, "写出字幕前已暂停").await;
            return Ok(());
        }
        update_task_cancelled(app, tasks, task_id).await;
        return Ok(());
    }

    // 5. 字幕输出阶段 (0.95 - 1.00)
    update_task_progress(app, tasks.clone(), task_id, 0.95, "正在写出字幕文件...").await;

    let format_str = task.output_format.clone();
    let output_track = if should_translate {
        build_translation_output_track(&source_track, &track, task.translation_content_mode)
    } else {
        track
    };
    let srt_output = output_track
        .to_format(&format_str)
        .map_err(|e| e.to_string())?;

    let suffix = match task.task_type {
        TaskType::GenerateOnly => ".finalsub".to_string(),
        TaskType::GenerateAndTranslate | TaskType::TranslateOnly => {
            let target_lang = task.target_language.clone().unwrap_or_else(|| "zh".into());
            if task.translation_content_mode.is_bilingual() {
                format!(".finalsub.{}.bilingual", target_lang)
            } else {
                format!(".finalsub.{}", target_lang)
            }
        }
    };

    let final_output_path = reserve_unique_output_path(&media_path, &suffix, &format_str)?;

    if check_cancelled(cancel_rx) {
        let _ = tokio::fs::remove_file(&final_output_path).await;
        let current_status = {
            let task_map = tasks.read().await;
            task_map
                .get(task_id)
                .map(|t| t.status)
                .unwrap_or(TaskStatus::Cancelled)
        };
        if current_status == TaskStatus::Paused {
            write_task_log(app, &app_config_dir, task_id, "写出字幕前已暂停").await;
            return Ok(());
        }
        update_task_cancelled(app, tasks, task_id).await;
        return Ok(());
    }

    // 原子写入：先 create_new 预留最终路径，防止并发任务覆盖；再写入唯一 temp 并 rename。
    let tmp_path = temporary_subtitle_output_path(&final_output_path, task_id, &format_str)?;
    if let Err(e) = tokio::fs::write(&tmp_path, &srt_output).await {
        let _ = tokio::fs::remove_file(&final_output_path).await;
        return Err(format!("写入临时字幕文件失败：{}", e));
    }

    if let Err(e) = tokio::fs::rename(&tmp_path, &final_output_path).await {
        let _ = tokio::fs::remove_file(&tmp_path).await;
        let _ = tokio::fs::remove_file(&final_output_path).await;
        return Err(format!("重命名字幕文件失败：{}", e));
    }

    // 6. 任务完成更新
    let mut task_map = tasks.write().await;
    if let Some(t) = task_map.get_mut(task_id) {
        if t.status == TaskStatus::Cancelled {
            return Ok(());
        }
        t.status = TaskStatus::Done;
        t.progress = 1.0;
        t.status_message = "已完成".into();
        t.output_path = Some(final_output_path.to_string_lossy().to_string());
        t.updated_at = chrono::Utc::now().to_rfc3339();
        let task_clone = t.clone();
        drop(task_map);
        emit_task_update_internal(app, &task_clone);
    }

    Ok(())
}

fn check_cancelled(cancel_rx: &mut tokio::sync::watch::Receiver<bool>) -> bool {
    *cancel_rx.borrow()
}

async fn update_task_progress(
    app: &AppHandle,
    tasks: Arc<RwLock<HashMap<String, Task>>>,
    task_id: &str,
    progress: f32,
    message: &str,
) {
    let mut task_map = tasks.write().await;
    if let Some(task) = task_map.get_mut(task_id) {
        // 如果状态已取消，则不更新
        if task.status == TaskStatus::Cancelled {
            return;
        }
        task.status = TaskStatus::Running;
        task.progress = task
            .progress
            .clamp(0.0, 1.0)
            .max(progress.clamp(0.0, 1.0));
        task.status_message = message.into();
        task.updated_at = chrono::Utc::now().to_rfc3339();
        let task_clone = task.clone();
        drop(task_map);
        emit_task_update_internal(app, &task_clone);
    }
}

async fn update_task_cancelled(
    app: &AppHandle,
    tasks: Arc<RwLock<HashMap<String, Task>>>,
    task_id: &str,
) {
    let mut task_map = tasks.write().await;
    if let Some(task) = task_map.get_mut(task_id) {
        task.status = TaskStatus::Cancelled;
        task.progress = task.progress.clamp(0.0, 1.0);
        task.status_message = "已取消".into();
        task.updated_at = chrono::Utc::now().to_rfc3339();
        let task_clone = task.clone();
        drop(task_map);
        emit_task_update_internal(app, &task_clone);
    }
}

async fn handle_translation_stop(
    app: &AppHandle,
    tasks: Arc<RwLock<HashMap<String, Task>>>,
    app_config_dir: &Path,
    task_id: &str,
    checkpoint_path: &Path,
    track: &SubtitleTrack,
    completed_cues: usize,
) {
    let current_status = {
        let task_map = tasks.read().await;
        task_map
            .get(task_id)
            .map(|t| t.status)
            .unwrap_or(TaskStatus::Cancelled)
    };

    if current_status == TaskStatus::Paused {
        save_translation_checkpoint(checkpoint_path, track, completed_cues);
        write_task_log(app, app_config_dir, task_id, "翻译已暂停，已保存当前进度").await;
    } else {
        update_task_cancelled(app, tasks, task_id).await;
    }
}

fn emit_task_update_internal(app: &AppHandle, task: &Task) {
    use tauri::{Emitter, Manager};
    app.emit("task-updated", task).ok();
    if let Some(state) = app.try_state::<crate::state::AppState>() {
        let app_config_dir = state.app_config_dir.clone();
        let tasks = state.tasks.clone();
        tauri::async_runtime::spawn(async move {
            let task_map = tasks.read().await;
            let _ = crate::core::task_queue::save_tasks(&app_config_dir, &task_map);
        });
    }
}

fn configured_value(value: Option<&String>) -> Option<String> {
    value
        .map(|item| item.trim())
        .filter(|item| !item.is_empty())
        .map(ToOwned::to_owned)
}

fn restore_translated_checkpoint(
    track: &mut SubtitleTrack,
    checkpoint: &SubtitleTrack,
) -> usize {
    let checkpoint_len = checkpoint.len();
    if checkpoint_len == 0 || checkpoint_len > track.len() {
        return 0;
    }

    for idx in 0..checkpoint_len {
        track.cues[idx].text = checkpoint.cues[idx].text.clone();
    }
    checkpoint_len
}

fn translation_progress(completed_cues: usize, total_cues: usize) -> f32 {
    if total_cues == 0 {
        return TRANSLATION_PROGRESS_START;
    }

    let ratio = (completed_cues as f32 / total_cues as f32).clamp(0.0, 1.0);
    TRANSLATION_PROGRESS_START + (TRANSLATION_PROGRESS_END - TRANSLATION_PROGRESS_START) * ratio
}

fn translation_supports_batch(provider_info: Option<&TranslationProvider>) -> bool {
    provider_info.map(|item| item.is_ai).unwrap_or(false)
}

fn translation_batch_end(cues: &[Cue], start_index: usize) -> usize {
    if start_index >= cues.len() {
        return start_index;
    }

    let mut end_index = start_index;
    let mut char_count = 0usize;
    while end_index < cues.len() && end_index - start_index < AI_TRANSLATION_MAX_BATCH_CUES {
        let cue_chars = cues[end_index].text.chars().count();
        if end_index > start_index && char_count + cue_chars > AI_TRANSLATION_MAX_BATCH_CHARS {
            break;
        }
        char_count += cue_chars;
        end_index += 1;
        if char_count >= AI_TRANSLATION_MAX_BATCH_CHARS {
            break;
        }
    }

    end_index
}

fn save_translation_checkpoint(path: &Path, track: &SubtitleTrack, completed_cues: usize) {
    if completed_cues == 0 {
        return;
    }

    let checkpoint = SubtitleTrack {
        cues: track.cues[0..completed_cues.min(track.len())].to_vec(),
    };
    let _ = std::fs::write(path, checkpoint.to_srt());
}

fn build_batch_translation_prompt(
    source_language: &str,
    target_language: &str,
    cues: &[Cue],
) -> (String, Vec<String>) {
    let mut input = serde_json::Map::new();
    let mut keys = Vec::with_capacity(cues.len());
    for (offset, cue) in cues.iter().enumerate() {
        let key = (offset + 1).to_string();
        keys.push(key.clone());
        input.insert(key, serde_json::Value::String(cue.text.clone()));
    }

    let input_json = serde_json::to_string(&serde_json::Value::Object(input))
        .unwrap_or_else(|_| "{}".to_string());
    let prompt = format!(
        "Translate each JSON value from {source_language} to {target_language}. \
Return only a valid JSON object with exactly the same keys. \
Do not add Markdown fences, comments, explanations, or extra keys. \
Preserve line breaks inside each value.\n\nInput JSON:\n{input_json}"
    );

    (prompt, keys)
}

fn parse_batch_translation_response(
    raw_text: &str,
    expected_keys: &[String],
) -> Result<Vec<String>, String> {
    let cleaned = extract_json_object(raw_text)
        .ok_or_else(|| "响应中没有 JSON 对象".to_string())?;
    let value: serde_json::Value = serde_json::from_str(cleaned)
        .map_err(|e| format!("JSON 解析失败：{e}"))?;
    let object = value
        .as_object()
        .ok_or_else(|| "响应不是 JSON 对象".to_string())?;

    let mut translated = Vec::with_capacity(expected_keys.len());
    for key in expected_keys {
        let text = object
            .get(key)
            .and_then(|item| item.as_str())
            .ok_or_else(|| format!("缺少键 {key} 的字符串译文"))?;
        translated.push(text.trim().to_string());
    }

    Ok(translated)
}

fn extract_json_object(raw_text: &str) -> Option<&str> {
    let trimmed = raw_text.trim();
    let without_fence = if trimmed.starts_with("```") {
        let after_first_line = trimmed.find('\n').map(|idx| &trimmed[idx + 1..])?;
        after_first_line
            .strip_suffix("```")
            .unwrap_or(after_first_line)
            .trim()
    } else {
        trimmed
    };

    let start = without_fence.find('{')?;
    let end = without_fence.rfind('}')?;
    (start <= end).then_some(&without_fence[start..=end])
}

async fn translate_with_retries(
    req: &TranslateRequest,
    retry_times: u32,
    cancel_rx: &mut tokio::sync::watch::Receiver<bool>,
) -> TranslationAttemptResult {
    let mut last_err = String::new();

    for attempt in 0..=retry_times {
        if check_cancelled(cancel_rx) {
            return TranslationAttemptResult::Cancelled;
        }

        let translate_fut = translate_text(req);
        tokio::pin!(translate_fut);
        let translate_result = loop {
            tokio::select! {
                res = &mut translate_fut => break res,
                change_res = cancel_rx.changed() => {
                    if change_res.is_err() || *cancel_rx.borrow() {
                        return TranslationAttemptResult::Cancelled;
                    }
                }
            }
        };

        match translate_result {
            Ok(resp) => {
                if resp.success {
                    return TranslationAttemptResult::Success(resp.translated_text);
                }
                last_err = resp.error.unwrap_or_else(|| "未知翻译错误".into());
            }
            Err(e) => {
                last_err = e.to_string();
            }
        }

        if attempt < retry_times {
            let retry_delay = tokio::time::sleep(std::time::Duration::from_millis(500));
            tokio::pin!(retry_delay);
            tokio::select! {
                _ = &mut retry_delay => {}
                change_res = cancel_rx.changed() => {
                    if change_res.is_err() || *cancel_rx.borrow() {
                        return TranslationAttemptResult::Cancelled;
                    }
                }
            }
        }
    }

    TranslationAttemptResult::Failed(last_err)
}

fn build_translation_output_track(
    source_track: &SubtitleTrack,
    translated_track: &SubtitleTrack,
    mode: TranslationContentMode,
) -> SubtitleTrack {
    if mode == TranslationContentMode::TargetOnly {
        return translated_track.clone();
    }

    let cues = translated_track
        .cues
        .iter()
        .enumerate()
        .map(|(index, translated_cue)| {
            let source_text = source_track
                .cues
                .get(index)
                .map(|cue| cue.text.as_str())
                .unwrap_or("");
            let translated_text = translated_cue.text.as_str();
            let (top, bottom) = match mode {
                TranslationContentMode::SourceAndTarget => (source_text, translated_text),
                TranslationContentMode::TargetAndSource => (translated_text, source_text),
                TranslationContentMode::TargetOnly => (translated_text, ""),
            };
            let mut cue = translated_cue.clone();
            cue.text = merge_bilingual_text(top, bottom);
            cue
        })
        .collect();

    SubtitleTrack { cues }
}

fn merge_bilingual_text(top: &str, bottom: &str) -> String {
    let has_top = !top.trim().is_empty();
    let has_bottom = !bottom.trim().is_empty();
    match (has_top, has_bottom) {
        (true, true) => format!("{}\n{}", top.trim_end(), bottom.trim_start()),
        (true, false) => top.to_string(),
        (false, true) => bottom.to_string(),
        (false, false) => String::new(),
    }
}

fn reserve_unique_output_path(
    media_path: &Path,
    suffix: &str,
    format: &str,
) -> Result<PathBuf, String> {
    let parent = media_path.parent().ok_or("媒体文件必须有父级目录")?;
    let stem = media_path
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or("无法获取媒体文件名")?;

    for counter in 0..=1000 {
        let file_name = build_output_file_name(stem, suffix, format, counter)?;
        let target = parent.join(file_name);
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&target)
        {
            Ok(_) => return Ok(target),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => {
                return Err(format!(
                    "创建字幕输出占位文件失败：{}：{}",
                    target.display(),
                    e
                ))
            }
        }
    }

    Err("无法生成唯一的输出路径，尝试次数过多".into())
}

fn build_output_file_name(
    stem: &str,
    suffix: &str,
    format: &str,
    counter: usize,
) -> Result<String, String> {
    let counter_suffix = if counter == 0 {
        String::new()
    } else {
        format!("-{}", counter)
    };
    let tail = format!("{}{}.{}", suffix, counter_suffix, format);
    if tail.len() >= MAX_OUTPUT_FILE_NAME_BYTES {
        return Err("字幕输出文件后缀过长，无法生成安全文件名".into());
    }

    let stem_budget = MAX_OUTPUT_FILE_NAME_BYTES - tail.len();
    let safe_stem = shorten_utf8_with_hash(stem, stem_budget);
    Ok(format!("{}{}", safe_stem, tail))
}

fn temporary_subtitle_output_path(
    final_output_path: &Path,
    task_id: &str,
    format: &str,
) -> Result<PathBuf, String> {
    let parent = final_output_path.parent().ok_or("字幕输出路径缺少父目录")?;
    Ok(parent.join(format!(".finalsub-{}.{}.tmp", task_id, format)))
}

fn shorten_utf8_with_hash(input: &str, max_bytes: usize) -> String {
    if input.len() <= max_bytes {
        return input.to_string();
    }

    let hash = stable_hash_hex(input);
    let marker = format!("-{}", hash);
    if max_bytes <= marker.len() {
        return truncate_utf8_to_bytes(&hash, max_bytes);
    }

    let prefix_budget = max_bytes - marker.len();
    let mut shortened = truncate_utf8_to_bytes(input, prefix_budget);
    shortened.push_str(&marker);
    shortened
}

fn truncate_utf8_to_bytes(input: &str, max_bytes: usize) -> String {
    let mut output = String::new();
    for ch in input.chars() {
        if output.len() + ch.len_utf8() > max_bytes {
            break;
        }
        output.push(ch);
    }
    output
}

fn stable_hash_hex(input: &str) -> String {
    let mut hash = 0x811c9dc5_u32;
    for byte in input.as_bytes() {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(0x01000193);
    }
    format!("{:08x}", hash)
}

fn default_uv_bin_fallback() -> PathBuf {
    crate::core::asr::parakeet::default_uv_bin()
}

pub async fn write_task_log(app: &AppHandle, app_config_dir: &Path, task_id: &str, message: &str) {
    let log_dir = app_config_dir.join("tasks");
    let log_path = log_dir.join(format!("{}.log", task_id));
    if let Some(parent) = log_path.parent() {
        let _ = tokio::fs::create_dir_all(parent).await;
    }

    let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let log_line = format!("[{}] {}\n", now, message);

    use tokio::io::AsyncWriteExt;
    if let Ok(mut file) = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .await
    {
        let _ = file.write_all(log_line.as_bytes()).await;
    }

    use tauri::Emitter;
    #[derive(serde::Serialize, Clone)]
    struct LogPayload {
        task_id: String,
        message: String,
    }
    app.emit(
        "task-log",
        LogPayload {
            task_id: task_id.to_string(),
            message: log_line,
        },
    )
    .ok();
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn configured_value_ignores_empty_strings() {
        assert_eq!(configured_value(None), None);
        assert_eq!(configured_value(Some(&"   ".to_string())), None);
        assert_eq!(
            configured_value(Some(&"  value  ".to_string())),
            Some("value".into())
        );
    }

    #[test]
    fn translation_output_track_keeps_target_only_default() {
        let source =
            SubtitleTrack::from_srt("1\n00:00:01,000 --> 00:00:03,000\nHello world\n\n").unwrap();
        let translated =
            SubtitleTrack::from_srt("1\n00:00:01,000 --> 00:00:03,000\n你好世界\n\n").unwrap();

        let output = build_translation_output_track(
            &source,
            &translated,
            TranslationContentMode::TargetOnly,
        );

        assert_eq!(output.cues[0].text, "你好世界");
    }

    #[test]
    fn translation_output_track_respects_bilingual_order() {
        let source =
            SubtitleTrack::from_srt("1\n00:00:01,000 --> 00:00:03,000\nHello world\n\n").unwrap();
        let translated =
            SubtitleTrack::from_srt("1\n00:00:01,000 --> 00:00:03,000\n你好世界\n\n").unwrap();

        let source_first = build_translation_output_track(
            &source,
            &translated,
            TranslationContentMode::SourceAndTarget,
        );
        let target_first = build_translation_output_track(
            &source,
            &translated,
            TranslationContentMode::TargetAndSource,
        );

        assert_eq!(source_first.cues[0].text, "Hello world\n你好世界");
        assert_eq!(target_first.cues[0].text, "你好世界\nHello world");
    }

    #[test]
    fn translated_checkpoint_restores_completed_cues_and_progress() {
        let mut track = SubtitleTrack::from_srt(
            "1\n00:00:01,000 --> 00:00:02,000\nHello\n\n\
             2\n00:00:02,000 --> 00:00:03,000\nWorld\n\n\
             3\n00:00:03,000 --> 00:00:04,000\nAgain\n\n",
        )
        .unwrap();
        let checkpoint = SubtitleTrack::from_srt(
            "1\n00:00:01,000 --> 00:00:02,000\n你好\n\n\
             2\n00:00:02,000 --> 00:00:03,000\n世界\n\n",
        )
        .unwrap();

        let restored = restore_translated_checkpoint(&mut track, &checkpoint);

        assert_eq!(restored, 2);
        assert_eq!(track.cues[0].text, "你好");
        assert_eq!(track.cues[1].text, "世界");
        assert_eq!(track.cues[2].text, "Again");
        assert!((translation_progress(restored, track.len()) - 0.90).abs() < 0.0001);
    }

    #[test]
    fn batch_translation_prompt_and_response_preserve_key_alignment() {
        let cues = SubtitleTrack::from_srt(
            "1\n00:00:01,000 --> 00:00:02,000\nHello\n\n\
             2\n00:00:02,000 --> 00:00:03,000\nWorld\n\n",
        )
        .unwrap()
        .cues;

        let (prompt, keys) = build_batch_translation_prompt("en", "zh", &cues);
        let translated = parse_batch_translation_response(
            "```json\n{\"1\":\"你好\",\"2\":\"世界\"}\n```",
            &keys,
        )
        .unwrap();

        assert!(prompt.contains("same keys"));
        assert_eq!(keys, vec!["1".to_string(), "2".to_string()]);
        assert_eq!(translated, vec!["你好".to_string(), "世界".to_string()]);
    }

    #[test]
    fn batch_translation_response_rejects_missing_key() {
        let keys = vec!["1".to_string(), "2".to_string()];
        let err = parse_batch_translation_response("{\"1\":\"你好\"}", &keys).unwrap_err();
        assert!(err.contains("缺少键 2"));
    }

    #[test]
    fn translation_batching_only_enables_ai_providers() {
        let providers = builtin_providers();
        let ai_provider = providers.iter().find(|item| item.id == "deepseek");
        let api_provider = providers.iter().find(|item| item.id == "google");

        assert!(translation_supports_batch(ai_provider));
        assert!(!translation_supports_batch(api_provider));
    }

    #[test]
    fn translation_batch_end_uses_cue_and_character_limits() {
        let short_cues = (0..30)
            .map(|idx| Cue {
                index: idx,
                start_ms: idx as u64 * 1000,
                end_ms: idx as u64 * 1000 + 500,
                text: "短句".into(),
            })
            .collect::<Vec<_>>();
        assert_eq!(translation_batch_end(&short_cues, 0), AI_TRANSLATION_MAX_BATCH_CUES);

        let long_cues = (0..4)
            .map(|idx| Cue {
                index: idx,
                start_ms: idx as u64 * 1000,
                end_ms: idx as u64 * 1000 + 500,
                text: "长".repeat(2_500),
            })
            .collect::<Vec<_>>();
        assert_eq!(translation_batch_end(&long_cues, 0), 1);
    }

    #[test]
    fn output_path_uses_counter_without_overwriting() {
        let tmp = TempDir::new().unwrap();
        let media = tmp.path().join("clip.mp4");
        std::fs::write(&media, b"media").unwrap();
        std::fs::write(tmp.path().join("clip.finalsub.srt"), b"existing").unwrap();

        let output = reserve_unique_output_path(&media, ".finalsub", "srt").unwrap();
        assert_eq!(output, tmp.path().join("clip.finalsub-1.srt"));
        assert!(output.exists());
    }

    #[test]
    fn output_path_reservation_is_atomic_for_repeated_tasks() {
        let tmp = TempDir::new().unwrap();
        let media = tmp.path().join("clip.mp4");
        std::fs::write(&media, b"media").unwrap();

        let first = reserve_unique_output_path(&media, ".finalsub", "srt").unwrap();
        let second = reserve_unique_output_path(&media, ".finalsub", "srt").unwrap();

        assert_eq!(first, tmp.path().join("clip.finalsub.srt"));
        assert_eq!(second, tmp.path().join("clip.finalsub-1.srt"));
        assert!(first.exists());
        assert!(second.exists());
    }

    #[test]
    fn output_path_truncates_long_media_stem() {
        let tmp = TempDir::new().unwrap();
        let media = tmp.path().join(format!("{}.mp4", "a".repeat(320)));

        let output = reserve_unique_output_path(&media, ".finalsub", "srt").unwrap();
        let file_name = output.file_name().unwrap().to_str().unwrap();

        assert!(file_name.len() <= MAX_OUTPUT_FILE_NAME_BYTES);
        assert!(file_name.ends_with(".finalsub.srt"));
        assert!(file_name.contains('-'));
        assert!(output.exists());
    }

    #[test]
    fn output_path_truncates_long_unicode_stem_on_char_boundary() {
        let tmp = TempDir::new().unwrap();
        let media = tmp
            .path()
            .join(format!("{}.mp4", "很长的字幕视频标题".repeat(80)));

        let output = reserve_unique_output_path(&media, ".finalsub.zh", "srt").unwrap();
        let file_name = output.file_name().unwrap().to_str().unwrap();

        assert!(file_name.len() <= MAX_OUTPUT_FILE_NAME_BYTES);
        assert!(file_name.ends_with(".finalsub.zh.srt"));
        assert!(output.exists());
    }

    #[test]
    fn temporary_subtitle_output_path_does_not_extend_long_final_name() {
        let tmp = TempDir::new().unwrap();
        let final_output = tmp.path().join(format!("{}.finalsub.srt", "a".repeat(230)));

        let temp_output = temporary_subtitle_output_path(
            &final_output,
            "019edc9a-1111-2222-3333-444455556666",
            "srt",
        )
        .unwrap();
        let file_name = temp_output.file_name().unwrap().to_str().unwrap();

        assert_eq!(
            file_name,
            ".finalsub-019edc9a-1111-2222-3333-444455556666.srt.tmp"
        );
        assert!(file_name.len() <= MAX_OUTPUT_FILE_NAME_BYTES);
    }
}
