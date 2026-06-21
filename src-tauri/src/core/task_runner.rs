use keyring::Entry;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::AppHandle;
#[cfg(not(debug_assertions))]
use tauri::Manager;
use tokio::sync::{watch, RwLock};

use crate::commands::{default_local_llm_dir, resolve_sidecar, whisper_models_dir};
use crate::core::asr::parakeet::ParakeetMlxEngine;
use crate::core::asr::whisper::WhisperCppEngine;
use crate::core::asr::{AsrEngine, AsrModelRef, ProgressUpdate, TranscribeJob};
use crate::core::subtitle::SubtitleTrack;
use crate::core::task_queue::{Task, TaskStatus, TaskType};
use crate::core::translation::{builtin_providers, translate_text, TranslateRequest};

const MAX_OUTPUT_FILE_NAME_BYTES: usize = 240;

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
                    Box::new(WhisperCppEngine::new(whisper_bin, models_dir))
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
        // TranslateOnly 模式直接读取原字幕文件
        update_task_progress(app, tasks.clone(), task_id, 0.0, "正在读取源字幕文件...").await;
        let srt_content = tokio::fs::read_to_string(&media_path)
            .await
            .map_err(|e| format!("读取源字幕文件失败：{}", e))?;
        let track =
            SubtitleTrack::from_srt(&srt_content).map_err(|e| format!("解析字幕失败：{}", e))?;
        current_track = Some(track);
    }

    let mut track = current_track.ok_or_else(|| "未生成或解析到有效字幕轨道".to_string())?;
    if track.is_empty() {
        return Err("未生成或解析到有效字幕轨道".into());
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
                    let temp_len = temp_track.len();
                    if temp_len > 0 && temp_len <= total_cues {
                        for idx in 0..temp_len {
                            track.cues[idx].text = temp_track.cues[idx].text.clone();
                        }
                        start_cue_index = temp_len;
                        let msg = format!(
                            "发现已保存的翻译进度，从第 {}/{} 行恢复...",
                            start_cue_index + 1,
                            total_cues
                        );
                        write_task_log(app, &app_config_dir, task_id, &msg).await;
                    }
                }
            }
        }

        for i in start_cue_index..total_cues {
            if check_cancelled(cancel_rx) {
                let current_status = {
                    let task_map = tasks.read().await;
                    task_map
                        .get(task_id)
                        .map(|t| t.status)
                        .unwrap_or(TaskStatus::Cancelled)
                };
                if current_status == TaskStatus::Paused {
                    let partial_track = SubtitleTrack {
                        cues: track.cues[0..i].to_vec(),
                    };
                    let _ = std::fs::write(&temp_translated_path, partial_track.to_srt());
                    write_task_log(app, &app_config_dir, task_id, "翻译已暂停，已保存当前进度")
                        .await;
                } else {
                    update_task_cancelled(app, tasks, task_id).await;
                }
                return Ok(());
            }

            let text_to_translate = track.cues[i].text.clone();
            let mut translated_text = String::new();
            let mut success = false;
            let mut last_err = String::new();

            for attempt in 0..=retry_times {
                if check_cancelled(cancel_rx) {
                    let current_status = {
                        let task_map = tasks.read().await;
                        task_map
                            .get(task_id)
                            .map(|t| t.status)
                            .unwrap_or(TaskStatus::Cancelled)
                    };
                    if current_status == TaskStatus::Paused {
                        let partial_track = SubtitleTrack {
                            cues: track.cues[0..i].to_vec(),
                        };
                        let _ = std::fs::write(&temp_translated_path, partial_track.to_srt());
                        write_task_log(app, &app_config_dir, task_id, "翻译已暂停，已保存当前进度")
                            .await;
                    } else {
                        update_task_cancelled(app, tasks, task_id).await;
                    }
                    return Ok(());
                }

                let req = TranslateRequest {
                    text: text_to_translate.clone(),
                    source_language: source_lang.clone(),
                    target_language: target_lang.clone(),
                    provider: provider.clone(),
                    api_key: api_key.clone(),
                    api_url: api_url.clone(),
                    model_name: model_name.clone(),
                    secret_fields: secret_fields_opt.clone(),
                };

                let translate_fut = translate_text(&req);
                tokio::pin!(translate_fut);
                let translate_result = loop {
                    tokio::select! {
                        res = &mut translate_fut => break res,
                        change_res = cancel_rx.changed() => {
                            if change_res.is_err() || *cancel_rx.borrow() {
                                let current_status = {
                                    let task_map = tasks.read().await;
                                    task_map.get(task_id).map(|t| t.status).unwrap_or(TaskStatus::Cancelled)
                                };
                                if current_status == TaskStatus::Paused {
                                    let partial_track = SubtitleTrack {
                                        cues: track.cues[0..i].to_vec(),
                                    };
                                    let _ = std::fs::write(&temp_translated_path, partial_track.to_srt());
                                    write_task_log(app, &app_config_dir, task_id, "翻译已暂停，已保存当前进度").await;
                                } else {
                                    update_task_cancelled(app, tasks, task_id).await;
                                }
                                return Ok(());
                            }
                        }
                    }
                };

                match translate_result {
                    Ok(resp) => {
                        if resp.success {
                            translated_text = resp.translated_text;
                            success = true;
                            break;
                        } else {
                            last_err = resp.error.unwrap_or_else(|| "未知翻译错误".into());
                        }
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
                                let current_status = {
                                    let task_map = tasks.read().await;
                                    task_map.get(task_id).map(|t| t.status).unwrap_or(TaskStatus::Cancelled)
                                };
                                if current_status == TaskStatus::Paused {
                                    let partial_track = SubtitleTrack {
                                        cues: track.cues[0..i].to_vec(),
                                    };
                                    let _ = std::fs::write(&temp_translated_path, partial_track.to_srt());
                                    write_task_log(app, &app_config_dir, task_id, "翻译已暂停，已保存当前进度").await;
                                } else {
                                    update_task_cancelled(app, tasks, task_id).await;
                                }
                                return Ok(());
                            }
                        }
                    }
                }
            }

            if !success {
                return Err(format!(
                    "翻译失败（尝试 {} 次）：{}",
                    retry_times + 1,
                    last_err
                ));
            }

            track.cues[i].text = translated_text;

            let partial_track = SubtitleTrack {
                cues: track.cues[0..=i].to_vec(),
            };
            let _ = std::fs::write(&temp_translated_path, partial_track.to_srt());

            let progress = 0.80 + (0.95 - 0.80) * ((i + 1) as f32 / total_cues as f32);
            let msg = format!("正在翻译字幕... ({}/{})", i + 1, total_cues);
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
    let srt_output = track.to_format(&format_str).map_err(|e| e.to_string())?;

    let suffix = match task.task_type {
        TaskType::GenerateOnly => ".finalsub".to_string(),
        TaskType::GenerateAndTranslate | TaskType::TranslateOnly => {
            let target_lang = task.target_language.clone().unwrap_or_else(|| "zh".into());
            format!(".finalsub.{}", target_lang)
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
        task.progress = progress.clamp(0.0, 1.0);
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
