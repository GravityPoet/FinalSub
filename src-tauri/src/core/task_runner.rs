use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::AppHandle;
use tauri::Manager;
use tokio::io::{AsyncBufReadExt, AsyncReadExt};
use tokio::sync::{watch, RwLock};

use crate::commands::{
    parakeet_models_dir, resolve_sidecar, synthesize_and_align_dubbing_cue, whisper_models_dir,
};
use crate::core::asr::cloud::{parse_protocol, CloudAsrConfig, CloudAsrEngine};
use crate::core::asr::parakeet::ParakeetEngine;
use crate::core::asr::sherpa_native::{SherpaNativeEngine, SherpaNativeKind};
use crate::core::asr::whisper::WhisperCppEngine;
use crate::core::asr::{AsrEngine, AsrModelRef, ProgressUpdate, TranscribeJob};
use crate::core::glossary::{
    build_glossary_prompt_block, match_glossary_entries, resolve_enabled_glossaries,
};
use crate::core::subtitle::{Cue, SubtitleTrack};
use crate::core::task_queue::{
    PipelineConfig, PipelineDubbingConfig, PipelineStageKind, PipelineStageStatus, Task,
    TaskStatus, TaskType, TranslationContentMode,
};
use crate::core::translation::{
    builtin_providers, translate_text, TranslateRequest, TranslationProvider,
};
use crate::core::tts::{
    create_dubbing_session, export_dubbing_audio, fail_dubbing_cue, get_dubbing_session,
    prepare_dubbing_cue, DubbingEngineSelection, DubbingSynthesizeCueRequest,
};

const MAX_OUTPUT_FILE_NAME_BYTES: usize = 240;
/// 批量对齐校验与定点补翻均失败时写入 cue 的占位文本。
/// 出现该占位符的任务不得按成功完成收尾，断点恢复也必须把这些行视为未翻译。
const TRANSLATION_FAILED_PLACEHOLDER: &str = "[翻译失败：对齐校验与定点补翻均未成功]";
const AUDIO_PROGRESS_END: f32 = 0.15;
const ASR_PROGRESS_START: f32 = AUDIO_PROGRESS_END;
const ASR_PROGRESS_END_WITH_TRANSLATION: f32 = 0.80;
const ASR_PROGRESS_END_GENERATE_ONLY: f32 = 0.95;
const TRANSLATION_PROGRESS_START: f32 = 0.80;
const TRANSLATION_ONLY_PROGRESS_START: f32 = 0.05;
const TRANSLATION_PROGRESS_END: f32 = 0.95;
const AI_TRANSLATION_MAX_BATCH_CHARS: usize = 4_000;
const ECHO_SIMILARITY_THRESHOLD: f64 = 0.75;
const ALIGNMENT_REPAIR_MAX_ATTEMPTS: usize = 3;

enum TranslationAttemptResult {
    Success(String),
    Cancelled,
    Failed(String),
}

#[derive(Debug, Clone)]
struct ParsedBatchTranslation {
    echoed_source: Option<String>,
    translation: String,
}

#[derive(Debug, Default)]
struct BatchAlignmentValidation {
    accepted: HashMap<String, String>,
    flagged: Vec<String>,
    echo_checked: usize,
}

#[derive(Debug)]
struct AlignedBatchReport {
    translations: Vec<String>,
    echo_checked: usize,
    flagged: usize,
    repaired: usize,
    unresolved: usize,
    alignment_retry_used: bool,
}

enum AlignedBatchResult {
    Success(AlignedBatchReport),
    Cancelled,
    Failed(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PipelineRunOutcome {
    Complete,
    Review,
    Paused,
}

struct PipelineRunContext<'a> {
    app: &'a AppHandle,
    tasks: Arc<RwLock<HashMap<String, Task>>>,
    app_config_dir: &'a Path,
    task_id: &'a str,
    task: &'a Task,
    media_path: &'a Path,
    subtitle_path: &'a Path,
    stale: &'a AtomicBool,
}

fn pipeline_stage_label(kind: PipelineStageKind) -> &'static str {
    match kind {
        PipelineStageKind::Transcribe => "转录",
        PipelineStageKind::Translate => "翻译",
        PipelineStageKind::SubtitleReview => "字幕校对",
        PipelineStageKind::Dub => "配音",
        PipelineStageKind::DubbingReview => "配音确认",
        PipelineStageKind::Compose => "生成成片",
        PipelineStageKind::Done => "完成",
    }
}

pub(crate) fn sherpa_vad_model_path(app: &AppHandle) -> Result<PathBuf, String> {
    #[cfg(debug_assertions)]
    {
        let _ = app;
        let current_dir = std::env::current_dir().map_err(|error| error.to_string())?;
        for candidate in [
            current_dir
                .join("src-tauri")
                .join("resources")
                .join("vad")
                .join("silero_vad.onnx"),
            current_dir
                .join("resources")
                .join("vad")
                .join("silero_vad.onnx"),
        ] {
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
        Err("开发环境缺少 src-tauri/resources/vad/silero_vad.onnx".into())
    }

    #[cfg(not(debug_assertions))]
    {
        let path = app
            .path()
            .resolve(
                "resources/vad/silero_vad.onnx",
                tauri::path::BaseDirectory::Resource,
            )
            .map_err(|error| format!("解析内置 Silero VAD 路径失败：{error}"))?;
        if !path.is_file() {
            return Err(format!("内置 Silero VAD 资源缺失：{}", path.display()));
        }
        Ok(path)
    }
}

pub(crate) fn parakeet_mlx_script_path(app: &AppHandle) -> Option<PathBuf> {
    #[cfg(debug_assertions)]
    {
        let _ = app;
        let current_dir = std::env::current_dir().ok()?;
        [
            current_dir
                .join("src-tauri")
                .join("resources")
                .join("parakeet")
                .join("parakeet_transcribe.py"),
            current_dir
                .join("resources")
                .join("parakeet")
                .join("parakeet_transcribe.py"),
        ]
        .into_iter()
        .find(|path| path.is_file())
    }

    #[cfg(not(debug_assertions))]
    {
        app.path()
            .resolve(
                "resources/parakeet/parakeet_transcribe.py",
                tauri::path::BaseDirectory::Resource,
            )
            .ok()
            .filter(|path| path.is_file())
    }
}

fn parse_subtitle_file(path: &Path, content: &str, label: &str) -> Result<SubtitleTrack, String> {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("srt")
        .to_ascii_lowercase();
    let track = SubtitleTrack::from_format(content, &extension)
        .map_err(|error| format!("解析{label}失败：{error}"))?;
    if track.is_empty() {
        return Err(format!("{label}中没有有效字幕条目"));
    }
    Ok(track)
}

async fn load_subtitle_file(path: &Path, label: &str) -> Result<SubtitleTrack, String> {
    let file = tokio::fs::File::open(path)
        .await
        .map_err(|error| format!("读取{label}失败：{error}"))?;
    let metadata = file
        .metadata()
        .await
        .map_err(|error| format!("读取{label}属性失败：{error}"))?;
    if metadata.len() > crate::core::subtitle::MAX_SUBTITLE_FILE_BYTES {
        return Err(format!("{label}超过 20 MB 限制"));
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(crate::core::subtitle::MAX_SUBTITLE_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .await
        .map_err(|error| format!("读取{label}失败：{error}"))?;
    if bytes.len() as u64 > crate::core::subtitle::MAX_SUBTITLE_FILE_BYTES {
        return Err(format!("{label}超过 20 MB 限制"));
    }
    let content = String::from_utf8(bytes).map_err(|_| format!("{label}不是有效的 UTF-8 文本"))?;
    parse_subtitle_file(path, &content, label)
}

pub fn start_task(
    app: AppHandle,
    tasks: Arc<RwLock<HashMap<String, Task>>>,
    task_controls: Arc<RwLock<HashMap<String, crate::state::TaskControl>>>,
    app_config_dir: PathBuf,
    task_id: String,
    mut cancel_rx: watch::Receiver<bool>,
    stale: Arc<AtomicBool>,
) {
    tauri::async_runtime::spawn(async move {
        let run_result = run_task_impl(
            &app,
            tasks.clone(),
            app_config_dir,
            &task_id,
            &mut cancel_rx,
            &stale,
        )
        .await;

        // 任务结束时移除自己的控制句柄；过时 runner 不得移除，否则会删掉新一代 runner 的通道。
        if !stale.load(Ordering::Relaxed) {
            let mut controls = task_controls.write().await;
            controls.remove(&task_id);
        }

        if stale.load(Ordering::Relaxed) {
            return;
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
                    if let Some(pipeline) = task.pipeline.as_mut() {
                        if let Some(kind) = pipeline.current_stage {
                            if let Some(stage) = pipeline.stage_mut(kind) {
                                stage.status = PipelineStageStatus::Error;
                                stage.error = Some(error_msg.clone());
                                stage.message =
                                    format!("{}失败，可从当前阶段重试", pipeline_stage_label(kind));
                            }
                        }
                    }
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
    stale: &AtomicBool,
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
        handle_cancel_signal(
            app,
            tasks,
            &app_config_dir,
            task_id,
            stale,
            "任务启动前已暂停",
        )
        .await;
        return Ok(());
    }

    // 2. 并发限流队列等待
    update_task_progress(
        app,
        tasks.clone(),
        task_id,
        0.0,
        "排队中，等待空闲并发槽...",
    )
    .await;
    write_task_log(
        app,
        &app_config_dir,
        task_id,
        "任务进入队列，等待并发通道...",
    )
    .await;

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
            handle_cancel_signal(app, tasks, &app_config_dir, task_id, stale, "排队已暂停").await;
            return Ok(());
        }
    };

    // 持有并发 permit 的整个生命周期都算作“正在处理”；PowerSaveLease 在任务
    // 成功、失败、暂停或取消返回时自动释放，Windows 的线程绑定也由独立 worker 保证。
    let _power_save_lease = state.power_save.acquire(format!("task:{task_id}"));

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
    write_task_log(
        app,
        &app_config_dir,
        task_id,
        "已获得并发通道，任务开始运行",
    )
    .await;

    let mut current_track: Option<SubtitleTrack> = None;

    // 审核闸门放行后只从持久化的下游节点继续。这样不会重新读取/上传
    // 原始音视频，也不会把已经校对过的字幕覆盖掉。
    if let Some(pipeline) = task.pipeline.as_ref() {
        if matches!(
            pipeline.current_stage,
            Some(PipelineStageKind::Dub)
                | Some(PipelineStageKind::DubbingReview)
                | Some(PipelineStageKind::Compose)
        ) {
            let subtitle_path = pipeline
                .subtitle_output_path
                .as_deref()
                .or(task.output_path.as_deref())
                .map(PathBuf::from)
                .ok_or_else(|| "流水线缺少可继续使用的字幕产物".to_string())?;
            let pipeline_context = PipelineRunContext {
                app,
                tasks: tasks.clone(),
                app_config_dir: &app_config_dir,
                task_id,
                task: &task,
                media_path: &media_path,
                subtitle_path: &subtitle_path,
                stale,
            };
            let outcome = continue_pipeline_downstream(&pipeline_context, cancel_rx).await?;
            if outcome != PipelineRunOutcome::Complete {
                return Ok(());
            }
            return Ok(());
        }
        if pipeline.current_stage == Some(PipelineStageKind::Transcribe) {
            update_pipeline_stage(
                app,
                tasks.clone(),
                task_id,
                PipelineStageKind::Transcribe,
                PipelineStageStatus::Running,
                0.0,
                if task.provided_subtitle_path.is_some() {
                    "正在读取已配对字幕..."
                } else {
                    "正在准备音频转录..."
                },
            )
            .await;
        }
    }

    if let Some(provided_subtitle_path) = task.provided_subtitle_path.as_deref() {
        let provided_subtitle_path = PathBuf::from(provided_subtitle_path);
        update_task_progress(
            app,
            tasks.clone(),
            task_id,
            0.02,
            "正在读取已配对字幕，跳过音频提取与 ASR...",
        )
        .await;
        current_track = Some(load_subtitle_file(&provided_subtitle_path, "已配对字幕").await?);
        write_task_log(
            app,
            &app_config_dir,
            task_id,
            &format!(
                "已加载配对字幕 {}，跳过音频提取与 ASR",
                provided_subtitle_path.display()
            ),
        )
        .await;
        update_task_progress(
            app,
            tasks.clone(),
            task_id,
            map_asr_progress(task.task_type, 1.0),
            "已加载配对字幕，ASR 已跳过",
        )
        .await;
        if task.pipeline.is_some() {
            update_pipeline_stage(
                app,
                tasks.clone(),
                task_id,
                PipelineStageKind::Transcribe,
                PipelineStageStatus::Skipped,
                1.0,
                "已使用配对字幕，跳过转录",
            )
            .await;
        }
    } else if task.task_type != TaskType::TranslateOnly {
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
                update_task_progress(
                    app,
                    tasks.clone(),
                    task_id,
                    AUDIO_PROGRESS_END,
                    "音频提取已跳过",
                )
                .await;
            }

            if !audio_extracted {
                update_task_progress(app, tasks.clone(), task_id, 0.0, "正在提取音频...").await;

                let ffmpeg_path = resolve_sidecar(app, "ffmpeg")?;
                let mut extract_args = crate::core::audio::extract_audio_args(
                    &task.media_path,
                    &audio_output_path.to_string_lossy(),
                );
                extract_args.splice(
                    0..0,
                    [
                        "-nostats".to_string(),
                        "-progress".to_string(),
                        "pipe:2".to_string(),
                    ],
                );

                let mut ffmpeg_cmd = tokio::process::Command::new(&ffmpeg_path);
                ffmpeg_cmd.args(&extract_args);
                ffmpeg_cmd.stdout(Stdio::null());
                ffmpeg_cmd.stderr(Stdio::piped());
                ffmpeg_cmd.kill_on_drop(true);

                let mut ffmpeg_child = ffmpeg_cmd
                    .spawn()
                    .map_err(|e| format!("运行 FFmpeg 提取音频失败：{}", e))?;
                let stderr = ffmpeg_child
                    .stderr
                    .take()
                    .ok_or_else(|| "无法读取 FFmpeg 进度输出".to_string())?;
                let mut stderr_lines = tokio::io::BufReader::new(stderr).lines();
                let mut stderr_buf = String::new();
                let mut stderr_done = false;
                let mut total_duration_ms: Option<u64> = None;
                let wait_fut = ffmpeg_child.wait();
                tokio::pin!(wait_fut);

                let ffmpeg_status = loop {
                    tokio::select! {
                        line_res = stderr_lines.next_line(), if !stderr_done => {
                            match line_res {
                                Ok(Some(line)) => {
                                    if let Some(duration_ms) = crate::core::audio::parse_duration_ms(&line) {
                                        total_duration_ms = Some(duration_ms);
                                    }
                                    if let (Some(time_ms), Some(total_ms)) = (
                                        crate::core::audio::parse_progress_time_ms(&line),
                                        total_duration_ms,
                                    ) {
                                        if total_ms > 0 {
                                            let ratio = (time_ms as f32 / total_ms as f32).clamp(0.0, 1.0);
                                            update_task_progress(
                                                app,
                                                tasks.clone(),
                                                task_id,
                                                AUDIO_PROGRESS_END * ratio,
                                                &format!("正在提取音频... {:.0}%", ratio * 100.0),
                                            )
                                            .await;
                                        }
                                    }
                                    stderr_buf.push_str(&line);
                                    stderr_buf.push('\n');
                                }
                                Ok(None) => {
                                    stderr_done = true;
                                }
                                Err(e) => return Err(format!("读取 FFmpeg 进度输出失败：{}", e)),
                            }
                        }
                        status_res = &mut wait_fut => {
                            break status_res.map_err(|e| format!("等待 FFmpeg 提取音频结束失败：{}", e))?;
                        }
                        change_res = cancel_rx.changed() => {
                            if change_res.is_err() || *cancel_rx.borrow() {
                                handle_cancel_signal(app, tasks, &app_config_dir, task_id, stale, "音频提取已暂停").await;
                                return Ok(());
                            }
                        }
                    }
                };

                if !ffmpeg_status.success() {
                    return Err(format!("FFmpeg 音频提取失败：{}", stderr_buf));
                }

                update_task_progress(
                    app,
                    tasks.clone(),
                    task_id,
                    AUDIO_PROGRESS_END,
                    "音频提取完成，准备 ASR 模型...",
                )
                .await;
            }

            if check_cancelled(cancel_rx) {
                handle_cancel_signal(
                    app,
                    tasks,
                    &app_config_dir,
                    task_id,
                    stale,
                    "音频提取已暂停",
                )
                .await;
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
                            if p2.exists() {
                                Some(p2)
                            } else {
                                None
                            }
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
                    let models_dir = parakeet_models_dir(&app_config_dir)?;
                    Box::new(ParakeetEngine::preferred(
                        models_dir,
                        parakeet_mlx_script_path(app),
                        sherpa_vad_model_path(app)?,
                    ))
                }
                "sensevoice" => {
                    let models_dir = whisper_models_dir(&app_config_dir)?;
                    Box::new(crate::core::asr::sensevoice::SenseVoiceEngine::new(
                        models_dir,
                        sherpa_vad_model_path(app)?,
                    ))
                }
                "paraformer" => {
                    let models_dir = whisper_models_dir(&app_config_dir)?;
                    Box::new(SherpaNativeEngine::new(
                        SherpaNativeKind::Paraformer,
                        models_dir,
                        sherpa_vad_model_path(app)?,
                    ))
                }
                "qwen3-asr" => {
                    let models_dir = whisper_models_dir(&app_config_dir)?;
                    Box::new(SherpaNativeEngine::new(
                        SherpaNativeKind::Qwen3,
                        models_dir,
                        sherpa_vad_model_path(app)?,
                    ))
                }
                "firered-asr" => {
                    let models_dir = whisper_models_dir(&app_config_dir)?;
                    Box::new(SherpaNativeEngine::new(
                        SherpaNativeKind::FireRedCtc,
                        models_dir,
                        sherpa_vad_model_path(app)?,
                    ))
                }
                "cloud-asr" => {
                    let settings = crate::core::settings::load_settings(&app_config_dir)
                        .map_err(|e| format!("加载云端 ASR 设置失败：{e}"))?;
                    if !settings.cloud_asr_upload_consent {
                        return Err("云端 ASR 未获得音频上传授权，请先在模型管理中明确启用".into());
                    }
                    let protocol = parse_protocol(&settings.cloud_asr_protocol)
                        .map_err(|error| error.to_string())?;
                    let provider_key = crate::core::secrets::get_provider_secret(
                        protocol.secret_provider(),
                        &settings.cloud_asr_endpoint,
                        "apiKey",
                    )?
                    .filter(|secret| !secret.trim().is_empty())
                    .ok_or_else(|| {
                        "当前云端 ASR endpoint 未保存 API Key，请先在模型管理中配置".to_string()
                    })?;
                    let api_secret = crate::core::secrets::get_provider_secret(
                        protocol.secret_provider(),
                        &settings.cloud_asr_endpoint,
                        "apiSecret",
                    )?;
                    let account_id = crate::core::secrets::get_provider_secret(
                        protocol.secret_provider(),
                        &settings.cloud_asr_endpoint,
                        "accountId",
                    )?;
                    let proxy_url = settings
                        .proxy_enabled
                        .then(|| settings.proxy_url.clone())
                        .filter(|value| !value.trim().is_empty());
                    Box::new(
                        CloudAsrEngine::new(
                            CloudAsrConfig {
                                protocol,
                                endpoint: settings.cloud_asr_endpoint,
                                model: settings.cloud_asr_model,
                                api_key: provider_key,
                                api_secret,
                                account_id,
                                timeout_seconds: settings.cloud_asr_timeout_seconds,
                                retry_times: settings.cloud_asr_retry_times,
                                request_concurrency: settings.cloud_asr_request_concurrency,
                                request_interval_ms: settings.cloud_asr_request_interval_ms,
                                proxy_url,
                                state_dir: Some(work_dir.join("cloud-asr-state")),
                            },
                            sherpa_vad_model_path(app)?,
                        )
                        .map_err(|error| error.to_string())?,
                    )
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
                ASR_PROGRESS_START,
                "正在进行 ASR 语音识别...",
            )
            .await;

            if check_cancelled(cancel_rx) {
                handle_cancel_signal(app, tasks, &app_config_dir, task_id, stale, "转录已暂停")
                    .await;
                return Ok(());
            }

            let job = TranscribeJob {
                audio_path: audio_output_path.to_string_lossy().to_string(),
                output_path: asr_output_path.to_string_lossy().to_string(),
                language: task.source_language.clone(),
                model: model_ref,
                max_subtitle_chars: task.max_subtitle_chars,
            };

            let (progress_tx, mut progress_rx) = tokio::sync::mpsc::channel::<ProgressUpdate>(32);
            let transcribe_fut = engine.transcribe(job, progress_tx, Some(cancel_rx.clone()));
            tokio::pin!(transcribe_fut);

            let transcribe_res = loop {
                tokio::select! {
                    update_opt = progress_rx.recv() => {
                        if let Some(update) = update_opt {
                            let mapped_progress =
                                map_asr_progress(task.task_type, update.progress);
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
                                    handle_cancel_signal(app, tasks, &app_config_dir, task_id, stale, "转录已暂停").await;
                                    return Ok(());
                                }
                                break Err(format!("语音转录失败：{}", e));
                            }
                        }
                    }
                }
            };

            let transcribed = transcribe_res?;
            // 统一固化转录结果：cloud/sherpa/parakeet 系引擎只返回内存轨道，不落盘的话
            // 翻译阶段暂停/重试会整段重跑 ASR（云端引擎重复上传计费），且重转录分段
            // 漂移会让按下标恢复的旧译文贴错行。写失败不阻塞本次任务，仅影响断点恢复。
            if let Err(error) = std::fs::write(&asr_output_path, transcribed.to_srt()) {
                write_task_log(
                    app,
                    &app_config_dir,
                    task_id,
                    &format!("固化转录结果失败（仅影响断点恢复）：{error}"),
                )
                .await;
            }
            current_track = Some(transcribed);
        }
    } else {
        // TranslateOnly 模式直接读取原字幕文件，按扩展名解析 SRT/VTT/ASS/LRC。
        update_task_progress(app, tasks.clone(), task_id, 0.02, "正在读取源字幕文件...").await;
        current_track = Some(load_subtitle_file(&media_path, "源字幕文件").await?);
    }

    let mut track = current_track.ok_or_else(|| "未生成或解析到有效字幕轨道".to_string())?;
    if track.is_empty() {
        return Err("未生成或解析到有效字幕轨道".into());
    }
    let resplit_stats =
        crate::core::subtitle::resplit_track_for_width(&mut track, task.max_subtitle_chars)
            .map_err(|error| format!("字幕断句配置无效：{error}"))?;
    if resplit_stats.split_cues > 0 {
        write_task_log(
            app,
            &app_config_dir,
            task_id,
            &format!(
                "已按任务宽度拆分 {} 条字幕，{} → {} 条",
                resplit_stats.split_cues, resplit_stats.original_cues, resplit_stats.final_cues
            ),
        )
        .await;
    }
    let source_track = track.clone();

    if check_cancelled(cancel_rx) {
        handle_cancel_signal(
            app,
            tasks,
            &app_config_dir,
            task_id,
            stale,
            "字幕翻译启动前已暂停",
        )
        .await;
        return Ok(());
    }

    // 4. 翻译阶段 (0.80 - 0.95)
    let should_translate = task.task_type == TaskType::GenerateAndTranslate
        || task.task_type == TaskType::TranslateOnly;
    if should_translate {
        if task.pipeline.is_some() {
            if task.task_type != TaskType::TranslateOnly && task.provided_subtitle_path.is_none() {
                update_pipeline_stage(
                    app,
                    tasks.clone(),
                    task_id,
                    PipelineStageKind::Transcribe,
                    PipelineStageStatus::Done,
                    1.0,
                    "转录完成",
                )
                .await;
            }
            update_pipeline_stage(
                app,
                tasks.clone(),
                task_id,
                PipelineStageKind::Translate,
                PipelineStageStatus::Running,
                0.0,
                "正在准备字幕翻译...",
            )
            .await;
        }
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
            translation_progress_start(task.task_type),
            &format!("准备通过 {} 翻译字幕...", provider),
        )
        .await;

        let provider_info = builtin_providers()
            .into_iter()
            .find(|item| item.id == provider);
        let api_url = if provider == "auto-free" {
            configured_value(settings.translate_endpoints.get("deeplx"))
        } else {
            configured_value(settings.translate_endpoints.get(&provider)).or_else(|| {
                provider_info
                    .as_ref()
                    .and_then(|item| configured_value(Some(&item.default_endpoint)))
            })
        };
        let model_name = configured_value(settings.translate_models.get(&provider));
        let retry_times = settings.translate_retry_times;
        let system_prompt = configured_value(settings.translate_system_prompts.get(&provider));
        let user_prompt = configured_value(settings.translate_user_prompts.get(&provider));
        let custom_headers = settings.translate_custom_headers.get(&provider).cloned();
        let custom_body = settings.translate_custom_body.get(&provider).cloned();
        let proxy_url = settings
            .proxy_enabled
            .then(|| settings.proxy_url.trim().to_string())
            .filter(|value| !value.is_empty());
        let batch_size = settings.translate_batch_size as usize;
        let translation_concurrency = settings.translate_concurrency as usize;
        let request_interval_ms = settings.translate_request_interval_ms;
        let structured_output = settings
            .translate_structured_output
            .get(&provider)
            .map(String::as_str)
            .filter(|mode| matches!(*mode, "disabled" | "json_object" | "json_schema"))
            .unwrap_or("json_schema")
            .to_string();
        let echo_anchoring = settings
            .translate_echo_anchoring
            .get(&provider)
            .copied()
            .unwrap_or(true);
        let enable_thinking = settings
            .translate_enable_thinking
            .get(&provider)
            .copied()
            .unwrap_or(false);
        let glossary_resolution = if provider_info
            .as_ref()
            .map(|info| info.is_ai)
            .unwrap_or(false)
        {
            resolve_enabled_glossaries(&settings.translation_glossaries)
        } else {
            Default::default()
        };
        if !glossary_resolution.conflicts.is_empty() {
            write_task_log(
                app,
                &app_config_dir,
                task_id,
                &format!(
                    "启用的术语表存在 {} 组重复原文，已按术语表优先级使用第一条",
                    glossary_resolution.conflicts.len()
                ),
            )
            .await;
        }
        let glossary_entries = glossary_resolution.entries;

        let mut secret_fields = std::collections::HashMap::new();
        if let Some(p) = &provider_info {
            for field in &p.secret_fields {
                if let Some(secret) = crate::core::secrets::get_provider_secret(
                    &provider,
                    api_url.as_deref().unwrap_or_default(),
                    field,
                )? {
                    secret_fields.insert(field.clone(), secret);
                }
            }
        }
        let api_key = secret_fields.get("apiKey").cloned();
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
                            translation_progress_for(task.task_type, start_cue_index, total_cues),
                            &msg,
                        )
                        .await;
                    }
                }
            }
        }

        let batch_translation_enabled = translation_supports_batch(provider_info.as_ref());
        let mut next_cue_index = start_cue_index;
        let mut unresolved_total = 0usize;
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
                    stale,
                )
                .await;
                return Ok(());
            }

            if !batch_translation_enabled && translation_concurrency > 1 {
                let concurrent_end = (next_cue_index + translation_concurrency).min(total_cues);
                let mut requests = tokio::task::JoinSet::new();
                for cue_index in next_cue_index..concurrent_end {
                    let request = TranslateRequest {
                        text: track.cues[cue_index].text.clone(),
                        source_language: source_lang.clone(),
                        target_language: target_lang.clone(),
                        provider: provider.clone(),
                        api_key: api_key.clone(),
                        api_url: api_url.clone(),
                        model_name: model_name.clone(),
                        secret_fields: secret_fields_opt.clone(),
                        system_prompt: system_prompt.clone(),
                        user_prompt: user_prompt.clone(),
                        proxy_url: proxy_url.clone(),
                        custom_headers: custom_headers.clone(),
                        custom_body: custom_body.clone(),
                        structured_output: None,
                        response_json_schema: None,
                        glossary_prompt: None,
                        enable_thinking: Some(enable_thinking),
                        thinking_control_bypassed: false,
                    };
                    let mut child_cancel = cancel_rx.clone();
                    requests.spawn(async move {
                        (
                            cue_index,
                            translate_with_retries(&request, retry_times, &mut child_cancel).await,
                        )
                    });
                }

                let mut translated = Vec::with_capacity(concurrent_end - next_cue_index);
                while let Some(joined) = requests.join_next().await {
                    let (cue_index, result) =
                        joined.map_err(|error| format!("并发翻译任务异常：{error}"))?;
                    match result {
                        TranslationAttemptResult::Success(text) => {
                            translated.push((cue_index, text));
                        }
                        TranslationAttemptResult::Cancelled => {
                            requests.abort_all();
                            handle_translation_stop(
                                app,
                                tasks.clone(),
                                &app_config_dir,
                                task_id,
                                &temp_translated_path,
                                &track,
                                next_cue_index,
                                stale,
                            )
                            .await;
                            return Ok(());
                        }
                        TranslationAttemptResult::Failed(error) => {
                            requests.abort_all();
                            return Err(format!(
                                "并发翻译失败（尝试 {} 次）：{}",
                                retry_times + 1,
                                error
                            ));
                        }
                    }
                }
                translated.sort_by_key(|(cue_index, _)| *cue_index);
                for (cue_index, text) in translated {
                    track.cues[cue_index].text = text;
                }
                next_cue_index = concurrent_end;
                save_translation_checkpoint(&temp_translated_path, &track, next_cue_index);
                let progress = translation_progress_for(task.task_type, next_cue_index, total_cues);
                let msg = format!("正在并发翻译字幕... ({}/{})", next_cue_index, total_cues);
                update_task_progress(app, tasks.clone(), task_id, progress, &msg).await;
                if !wait_translation_interval(request_interval_ms, cancel_rx).await {
                    handle_translation_stop(
                        app,
                        tasks.clone(),
                        &app_config_dir,
                        task_id,
                        &temp_translated_path,
                        &track,
                        next_cue_index,
                        stale,
                    )
                    .await;
                    return Ok(());
                }
                continue;
            }

            let batch_end = if batch_translation_enabled {
                translation_batch_end(&track.cues, next_cue_index, batch_size)
            } else {
                (next_cue_index + 1).min(total_cues)
            };
            if batch_translation_enabled && batch_end - next_cue_index > 1 {
                let source_texts = track.cues[next_cue_index..batch_end]
                    .iter()
                    .map(|cue| cue.text.clone())
                    .collect::<Vec<_>>();
                let glossary_matches = match_glossary_entries(&glossary_entries, &source_texts);
                let glossary_prompt = build_glossary_prompt_block(&glossary_matches);
                let batch_req = TranslateRequest {
                    text: String::new(),
                    source_language: source_lang.clone(),
                    target_language: target_lang.clone(),
                    provider: provider.clone(),
                    api_key: api_key.clone(),
                    api_url: api_url.clone(),
                    model_name: model_name.clone(),
                    secret_fields: secret_fields_opt.clone(),
                    system_prompt: system_prompt.clone(),
                    user_prompt: user_prompt.clone(),
                    proxy_url: proxy_url.clone(),
                    custom_headers: custom_headers.clone(),
                    custom_body: custom_body.clone(),
                    structured_output: Some(structured_output.clone()),
                    response_json_schema: None,
                    glossary_prompt: (!glossary_prompt.is_empty()).then_some(glossary_prompt),
                    enable_thinking: Some(enable_thinking),
                    thinking_control_bypassed: false,
                };

                match translate_aligned_batch(
                    &batch_req,
                    &source_texts,
                    &structured_output,
                    echo_anchoring,
                    retry_times,
                    cancel_rx,
                )
                .await
                {
                    AlignedBatchResult::Success(report) => {
                        unresolved_total += report.unresolved;
                        for (offset, translated_text) in report.translations.into_iter().enumerate()
                        {
                            track.cues[next_cue_index + offset].text = translated_text;
                        }

                        next_cue_index = batch_end;
                        save_translation_checkpoint(&temp_translated_path, &track, next_cue_index);
                        let progress =
                            translation_progress_for(task.task_type, next_cue_index, total_cues);
                        let msg =
                            format!("正在校验并翻译字幕... ({}/{})", next_cue_index, total_cues);
                        update_task_progress(app, tasks.clone(), task_id, progress, &msg).await;
                        write_task_log(
                            app,
                            &app_config_dir,
                            task_id,
                            &format!(
                                "批量翻译对齐完成：回显校验 {} 条，检出 {} 条，定点修复 {} 条，未解决 {} 条{}",
                                report.echo_checked,
                                report.flagged,
                                report.repaired,
                                report.unresolved,
                                if report.alignment_retry_used {
                                    "，已执行一次整批对齐重试"
                                } else {
                                    ""
                                }
                            ),
                        )
                        .await;
                        if !wait_translation_interval(request_interval_ms, cancel_rx).await {
                            handle_translation_stop(
                                app,
                                tasks.clone(),
                                &app_config_dir,
                                task_id,
                                &temp_translated_path,
                                &track,
                                next_cue_index,
                                stale,
                            )
                            .await;
                            return Ok(());
                        }
                        continue;
                    }
                    AlignedBatchResult::Cancelled => {
                        handle_translation_stop(
                            app,
                            tasks.clone(),
                            &app_config_dir,
                            task_id,
                            &temp_translated_path,
                            &track,
                            next_cue_index,
                            stale,
                        )
                        .await;
                        return Ok(());
                    }
                    AlignedBatchResult::Failed(batch_err) => {
                        write_task_log(
                            app,
                            &app_config_dir,
                            task_id,
                            &format!("批量翻译请求失败，当前条目降级逐条翻译：{}", batch_err),
                        )
                        .await;
                    }
                }
            }

            let single_source = track.cues[next_cue_index].text.clone();
            let single_glossary_prompt = if provider_info
                .as_ref()
                .map(|info| info.is_ai)
                .unwrap_or(false)
            {
                let matches =
                    match_glossary_entries(&glossary_entries, std::slice::from_ref(&single_source));
                let block = build_glossary_prompt_block(&matches);
                (!block.is_empty()).then_some(block)
            } else {
                None
            };
            let req = TranslateRequest {
                text: single_source,
                source_language: source_lang.clone(),
                target_language: target_lang.clone(),
                provider: provider.clone(),
                api_key: api_key.clone(),
                api_url: api_url.clone(),
                model_name: model_name.clone(),
                secret_fields: secret_fields_opt.clone(),
                system_prompt: system_prompt.clone(),
                user_prompt: user_prompt.clone(),
                proxy_url: proxy_url.clone(),
                custom_headers: custom_headers.clone(),
                custom_body: custom_body.clone(),
                structured_output: None,
                response_json_schema: None,
                glossary_prompt: single_glossary_prompt,
                enable_thinking: Some(enable_thinking),
                thinking_control_bypassed: false,
            };

            let translated_text = match translate_with_retries(&req, retry_times, cancel_rx).await {
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
                        stale,
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

            let progress = translation_progress_for(task.task_type, next_cue_index, total_cues);
            let msg = format!("正在翻译字幕... ({}/{})", next_cue_index, total_cues);
            update_task_progress(app, tasks.clone(), task_id, progress, &msg).await;
            if !wait_translation_interval(request_interval_ms, cancel_rx).await {
                handle_translation_stop(
                    app,
                    tasks.clone(),
                    &app_config_dir,
                    task_id,
                    &temp_translated_path,
                    &track,
                    next_cue_index,
                    stale,
                )
                .await;
                return Ok(());
            }
        }

        if unresolved_total > 0 {
            // 保留断点：重试时 restore_translated_checkpoint 会把占位符行视为未翻译并重新翻译。
            return Err(format!(
                "有 {} 行字幕在对齐校验与定点补翻后仍未成功，任务未写出成品；重试任务将从失败行继续翻译",
                unresolved_total
            ));
        }
        let _ = std::fs::remove_file(&temp_translated_path);
    }

    if check_cancelled(cancel_rx) {
        handle_cancel_signal(
            app,
            tasks,
            &app_config_dir,
            task_id,
            stale,
            "写出字幕前已暂停",
        )
        .await;
        return Ok(());
    }

    // 5. 字幕输出阶段 (0.95 - 1.00)
    update_task_progress(app, tasks.clone(), task_id, 0.95, "正在写出字幕文件...").await;

    // 双语字幕 + 配音：固化一份纯译文轨道供配音阶段使用，
    // 避免 TTS 把每句的原文和译文各念一遍；合成阶段仍用双语成品。
    if should_translate
        && task.translation_content_mode.is_bilingual()
        && task
            .pipeline
            .as_ref()
            .is_some_and(|pipeline| pipeline.dubbing.is_some())
    {
        let mut dubbing_track = track.clone();
        if task.strip_chinese_punctuation {
            for cue in &mut dubbing_track.cues {
                cue.text = strip_chinese_punctuation(&cue.text);
            }
        }
        let dubbing_source_path = work_dir.join("dubbing-source.srt");
        if let Err(error) = std::fs::write(&dubbing_source_path, dubbing_track.to_srt()) {
            write_task_log(
                app,
                &app_config_dir,
                task_id,
                &format!("固化配音用译文轨道失败，配音将退回使用双语字幕：{error}"),
            )
            .await;
        }
    }

    let format_str = task.output_format.clone();
    let mut output_track = if should_translate {
        build_translation_output_track(&source_track, &track, task.translation_content_mode)
    } else {
        track
    };
    if task.strip_chinese_punctuation {
        for cue in &mut output_track.cues {
            cue.text = strip_chinese_punctuation(&cue.text);
        }
    }
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

    let output_stem = resolve_output_stem(&task, &media_path)?;
    let final_output_path =
        reserve_unique_output_path(&media_path, output_stem.as_deref(), &suffix, &format_str)?;

    if check_cancelled(cancel_rx) {
        let _ = tokio::fs::remove_file(&final_output_path).await;
        handle_cancel_signal(
            app,
            tasks,
            &app_config_dir,
            task_id,
            stale,
            "写出字幕前已暂停",
        )
        .await;
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

    // 6. 写入统一流水线快照；没有 pipeline 的旧任务继续沿用原有
    // review → done 语义，保证升级后历史任务不发生状态漂移。
    if task.pipeline.is_some() {
        let subtitle_output = final_output_path.to_string_lossy().to_string();
        if task.task_type != TaskType::TranslateOnly && task.provided_subtitle_path.is_none() {
            update_pipeline_stage(
                app,
                tasks.clone(),
                task_id,
                PipelineStageKind::Transcribe,
                PipelineStageStatus::Done,
                1.0,
                "转录完成",
            )
            .await;
        }
        if should_translate {
            update_pipeline_stage(
                app,
                tasks.clone(),
                task_id,
                PipelineStageKind::Translate,
                PipelineStageStatus::Done,
                1.0,
                "翻译完成",
            )
            .await;
        }
        set_pipeline_subtitle_output(app, tasks.clone(), task_id, &subtitle_output).await;

        let subtitle_review = task
            .pipeline
            .as_ref()
            .map(|pipeline| pipeline.subtitle_review)
            .unwrap_or(false);
        if subtitle_review {
            update_pipeline_stage(
                app,
                tasks.clone(),
                task_id,
                PipelineStageKind::SubtitleReview,
                PipelineStageStatus::Review,
                1.0,
                "等待字幕校对",
            )
            .await;
            set_task_review_state(
                app,
                tasks.clone(),
                task_id,
                "字幕已写出，等待校对后继续",
                stale,
            )
            .await;
            return Ok(());
        }

        let pipeline_context = PipelineRunContext {
            app,
            tasks: tasks.clone(),
            app_config_dir: &app_config_dir,
            task_id,
            task: &task,
            media_path: &media_path,
            subtitle_path: &final_output_path,
            stale,
        };
        let outcome = continue_pipeline_downstream(&pipeline_context, cancel_rx).await?;
        if outcome != PipelineRunOutcome::Complete {
            return Ok(());
        }
        return Ok(());
    }

    let mut task_map = tasks.write().await;
    if let Some(t) = task_map.get_mut(task_id) {
        if t.status == TaskStatus::Cancelled {
            return Ok(());
        }
        t.status = if t.review_required {
            TaskStatus::Review
        } else {
            TaskStatus::Done
        };
        t.progress = 1.0;
        t.status_message = if t.review_required {
            "等待人工审核".into()
        } else {
            "已完成".into()
        };
        t.reviewed_at = None;
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

async fn update_pipeline_stage(
    app: &AppHandle,
    tasks: Arc<RwLock<HashMap<String, Task>>>,
    task_id: &str,
    kind: PipelineStageKind,
    status: PipelineStageStatus,
    progress: f32,
    message: &str,
) {
    let mut task_map = tasks.write().await;
    let Some(task) = task_map.get_mut(task_id) else {
        return;
    };
    if task.status == TaskStatus::Paused && status == PipelineStageStatus::Running {
        return;
    }
    let now = chrono::Utc::now().to_rfc3339();
    if let Some(pipeline) = task.pipeline.as_mut() {
        if let Some(stage) = pipeline.stage_mut(kind) {
            stage.status = status;
            stage.progress = progress.clamp(0.0, 1.0);
            stage.message = message.to_string();
            stage.error = None;
            match status {
                PipelineStageStatus::Running => {
                    if stage.started_at.is_none() {
                        stage.started_at = Some(now.clone());
                    }
                    stage.completed_at = None;
                }
                PipelineStageStatus::Done | PipelineStageStatus::Skipped => {
                    if stage.started_at.is_none() {
                        stage.started_at = Some(now.clone());
                    }
                    stage.progress = 1.0;
                    stage.completed_at = Some(now.clone());
                }
                PipelineStageStatus::Review | PipelineStageStatus::Error => {
                    if stage.started_at.is_none() {
                        stage.started_at = Some(now.clone());
                    }
                    stage.completed_at = None;
                }
                PipelineStageStatus::Pending => {}
            }
        }
        pipeline.current_stage = Some(kind);
        if matches!(
            status,
            PipelineStageStatus::Done | PipelineStageStatus::Skipped
        ) {
            let next_stage = pipeline
                .stages
                .iter()
                .find(|stage| {
                    matches!(
                        stage.status,
                        PipelineStageStatus::Pending
                            | PipelineStageStatus::Running
                            | PipelineStageStatus::Review
                            | PipelineStageStatus::Error
                    )
                })
                .map(|stage| stage.kind);
            pipeline.current_stage = next_stage;
        }
    }
    if !matches!(task.status, TaskStatus::Cancelled | TaskStatus::Paused)
        && status == PipelineStageStatus::Running
    {
        task.status = TaskStatus::Running;
    }
    task.progress = task.progress.max(
        match kind {
            PipelineStageKind::Transcribe => 0.78,
            PipelineStageKind::Translate => 0.94,
            PipelineStageKind::SubtitleReview => 0.95,
            PipelineStageKind::Dub => 0.985,
            PipelineStageKind::DubbingReview => 0.99,
            PipelineStageKind::Compose => 0.998,
            PipelineStageKind::Done => 1.0,
        } * progress.clamp(0.0, 1.0),
    );
    task.status_message = message.to_string();
    task.error = None;
    task.updated_at = now;
    let task_clone = task.clone();
    drop(task_map);
    emit_task_update_internal(app, &task_clone);
}

async fn set_pipeline_subtitle_output(
    app: &AppHandle,
    tasks: Arc<RwLock<HashMap<String, Task>>>,
    task_id: &str,
    output_path: &str,
) {
    let mut task_map = tasks.write().await;
    if let Some(task) = task_map.get_mut(task_id) {
        if let Some(pipeline) = task.pipeline.as_mut() {
            pipeline.subtitle_output_path = Some(output_path.to_string());
            pipeline.current_stage = pipeline
                .stages
                .iter()
                .find(|stage| stage.status == PipelineStageStatus::Pending)
                .map(|stage| stage.kind);
        }
        task.output_path = Some(output_path.to_string());
        task.updated_at = chrono::Utc::now().to_rfc3339();
        let task_clone = task.clone();
        drop(task_map);
        emit_task_update_internal(app, &task_clone);
    }
}

async fn set_pipeline_session_id(
    app: &AppHandle,
    tasks: Arc<RwLock<HashMap<String, Task>>>,
    task_id: &str,
    session_id: &str,
) {
    let mut task_map = tasks.write().await;
    if let Some(task) = task_map.get_mut(task_id) {
        if let Some(pipeline) = task.pipeline.as_mut() {
            pipeline.dubbing_session_id = Some(session_id.to_string());
        }
        task.updated_at = chrono::Utc::now().to_rfc3339();
        let task_clone = task.clone();
        drop(task_map);
        emit_task_update_internal(app, &task_clone);
    }
}

async fn set_pipeline_audio_path(
    app: &AppHandle,
    tasks: Arc<RwLock<HashMap<String, Task>>>,
    task_id: &str,
    audio_path: &str,
) {
    let mut task_map = tasks.write().await;
    if let Some(task) = task_map.get_mut(task_id) {
        if let Some(pipeline) = task.pipeline.as_mut() {
            pipeline.dubbed_audio_path = Some(audio_path.to_string());
        }
        task.updated_at = chrono::Utc::now().to_rfc3339();
        let task_clone = task.clone();
        drop(task_map);
        emit_task_update_internal(app, &task_clone);
    }
}

async fn set_pipeline_final_path(
    app: &AppHandle,
    tasks: Arc<RwLock<HashMap<String, Task>>>,
    task_id: &str,
    final_path: &str,
) {
    let mut task_map = tasks.write().await;
    if let Some(task) = task_map.get_mut(task_id) {
        if let Some(pipeline) = task.pipeline.as_mut() {
            pipeline.final_video_path = Some(final_path.to_string());
        }
        task.updated_at = chrono::Utc::now().to_rfc3339();
        let task_clone = task.clone();
        drop(task_map);
        emit_task_update_internal(app, &task_clone);
    }
}

async fn set_task_review_state(
    app: &AppHandle,
    tasks: Arc<RwLock<HashMap<String, Task>>>,
    task_id: &str,
    message: &str,
    stale: &AtomicBool,
) {
    if stale.load(Ordering::Relaxed) {
        return;
    }
    let mut task_map = tasks.write().await;
    if let Some(task) = task_map.get_mut(task_id) {
        if matches!(task.status, TaskStatus::Cancelled | TaskStatus::Paused) {
            return;
        }
        task.status = TaskStatus::Review;
        task.status_message = message.to_string();
        task.reviewed_at = None;
        task.updated_at = chrono::Utc::now().to_rfc3339();
        let task_clone = task.clone();
        drop(task_map);
        emit_task_update_internal(app, &task_clone);
    }
}

async fn set_task_done(
    app: &AppHandle,
    tasks: Arc<RwLock<HashMap<String, Task>>>,
    task_id: &str,
    message: &str,
    stale: &AtomicBool,
) {
    if stale.load(Ordering::Relaxed) {
        return;
    }
    let mut task_map = tasks.write().await;
    if let Some(task) = task_map.get_mut(task_id) {
        if matches!(task.status, TaskStatus::Cancelled | TaskStatus::Paused) {
            return;
        }
        task.status = TaskStatus::Done;
        task.progress = 1.0;
        task.status_message = message.to_string();
        task.updated_at = chrono::Utc::now().to_rfc3339();
        let task_clone = task.clone();
        drop(task_map);
        emit_task_update_internal(app, &task_clone);
    }
}

fn pipeline_dubbing_engine(
    config: &PipelineDubbingConfig,
) -> Result<DubbingEngineSelection, String> {
    match config.engine.as_str() {
        "local" => Ok(DubbingEngineSelection::Local {
            model_id: config.model_or_provider_id.clone(),
        }),
        "cloud" => Ok(DubbingEngineSelection::Cloud {
            provider_id: config.model_or_provider_id.clone(),
        }),
        other => Err(format!("不支持的配音引擎：{other}")),
    }
}

fn pipeline_cancel_bridge(
    cancel_rx: &watch::Receiver<bool>,
    cancelled: Arc<AtomicBool>,
) -> tokio::task::JoinHandle<()> {
    let mut receiver = cancel_rx.clone();
    tokio::spawn(async move {
        while receiver.changed().await.is_ok() {
            if *receiver.borrow() {
                cancelled.store(true, Ordering::Relaxed);
                break;
            }
        }
    })
}

async fn handle_pipeline_stop(
    app: &AppHandle,
    tasks: Arc<RwLock<HashMap<String, Task>>>,
    app_config_dir: &Path,
    task_id: &str,
    stage: PipelineStageKind,
    cancel_rx: &mut watch::Receiver<bool>,
    stale: &AtomicBool,
) -> PipelineRunOutcome {
    if stale.load(Ordering::Relaxed) {
        // 过时 runner：任务已由新一代 runner 接管，静默退出，不写状态与日志。
        return PipelineRunOutcome::Paused;
    }
    let current_status = {
        let task_map = tasks.read().await;
        task_map
            .get(task_id)
            .map(|task| task.status)
            .unwrap_or(TaskStatus::Cancelled)
    };
    if current_status == TaskStatus::Paused {
        update_pipeline_stage(
            app,
            tasks,
            task_id,
            stage,
            PipelineStageStatus::Pending,
            0.0,
            &format!("{}已暂停，稍后从当前进度继续", pipeline_stage_label(stage)),
        )
        .await;
        write_task_log(
            app,
            app_config_dir,
            task_id,
            &format!(
                "{}阶段已暂停，保留全部中间产物",
                pipeline_stage_label(stage)
            ),
        )
        .await;
        PipelineRunOutcome::Paused
    } else {
        update_task_cancelled(app, tasks, task_id).await;
        let _ = cancel_rx;
        PipelineRunOutcome::Paused
    }
}

async fn run_dubbing_stage(
    context: &PipelineRunContext<'_>,
    pipeline: &PipelineConfig,
    cancel_rx: &mut watch::Receiver<bool>,
) -> Result<PipelineRunOutcome, String> {
    let app = context.app;
    let tasks = context.tasks.clone();
    let app_config_dir = context.app_config_dir;
    let task_id = context.task_id;
    let task = context.task;
    let media_path = context.media_path;
    // 双语字幕任务会额外固化一份纯译文轨道：配音必须用它，
    // 否则 TTS 会把每句的原文和译文各念一遍；合成阶段仍使用双语字幕。
    let dubbing_source_override = app_config_dir
        .join("tasks")
        .join(task_id)
        .join("dubbing-source.srt");
    let subtitle_path: &Path = if dubbing_source_override.is_file() {
        &dubbing_source_override
    } else {
        context.subtitle_path
    };
    let stale = context.stale;
    let dubbing = pipeline
        .dubbing
        .as_ref()
        .ok_or_else(|| "流水线缺少配音配置".to_string())?;
    update_pipeline_stage(
        app,
        tasks.clone(),
        task_id,
        PipelineStageKind::Dub,
        PipelineStageStatus::Running,
        0.0,
        "准备配音会话...",
    )
    .await;

    let session_id = pipeline
        .dubbing_session_id
        .as_deref()
        .filter(|id| get_dubbing_session(app_config_dir, id).is_ok())
        .map(ToOwned::to_owned);
    let session_id = match session_id {
        Some(id) => id,
        None => {
            let session = create_dubbing_session(
                app_config_dir,
                &subtitle_path.to_string_lossy(),
                Some(&media_path.to_string_lossy()),
            )
            .map_err(|error| error.to_string())?;
            set_pipeline_session_id(app, tasks.clone(), task_id, &session.id).await;
            session.id
        }
    };
    let engine = pipeline_dubbing_engine(dubbing)?;
    let ffmpeg = resolve_sidecar(app, "ffmpeg")?;
    let session = get_dubbing_session(app_config_dir, &session_id).map_err(|e| e.to_string())?;
    let total = session.cues.len().max(1);
    let pending_cues = session
        .cues
        .into_iter()
        .filter(|cue| {
            !(matches!(
                cue.status,
                crate::core::tts::DubbingCueStatus::Ready
                    | crate::core::tts::DubbingCueStatus::Accepted
            ) && cue
                .wav_path
                .as_deref()
                .is_some_and(|path| Path::new(path).is_file()))
        })
        .collect::<Vec<_>>();
    let concurrency = if matches!(engine, DubbingEngineSelection::Local { .. }) {
        usize::from(dubbing.local_concurrency.clamp(1, 3))
    } else {
        1
    };
    let mut completed = total.saturating_sub(pending_cues.len());

    for chunk in pending_cues.chunks(concurrency) {
        if check_cancelled(cancel_rx) {
            app.state::<crate::state::AppState>()
                .tts_worker
                .trim_idle_workers()
                .await;
            return Ok(handle_pipeline_stop(
                app,
                tasks,
                app_config_dir,
                task_id,
                PipelineStageKind::Dub,
                cancel_rx,
                stale,
            )
            .await);
        }
        let mut prepared_cues = Vec::with_capacity(chunk.len());
        for cue in chunk {
            let request = DubbingSynthesizeCueRequest {
                session_id: session_id.clone(),
                cue_index: cue.index,
                engine: engine.clone(),
                voice: cue
                    .voice_id
                    .clone()
                    .filter(|voice| !voice.trim().is_empty())
                    .unwrap_or_else(|| dubbing.voice.clone()),
                global_speed: dubbing.global_speed,
                local_concurrency: dubbing.local_concurrency,
                reference_audio_path: dubbing.reference_audio_path.clone(),
                reference_text: dubbing.reference_text.clone(),
                num_steps: dubbing.num_steps,
            };
            let prepared = {
                let state = app.state::<crate::state::AppState>();
                let _session_io = state.dubbing_session_io.lock().await;
                prepare_dubbing_cue(app_config_dir, &request).map_err(|error| error.to_string())?
            };
            prepared_cues.push(prepared);
        }

        let mut jobs = tokio::task::JoinSet::new();
        for prepared in prepared_cues {
            let cancelled = Arc::new(AtomicBool::new(check_cancelled(cancel_rx)));
            let child_cancel = cancel_rx.clone();
            let child_app = app.clone();
            let child_ffmpeg = ffmpeg.clone();
            jobs.spawn(async move {
                let bridge = pipeline_cancel_bridge(&child_cancel, cancelled.clone());
                let state = child_app.state::<crate::state::AppState>();
                let synthesis = synthesize_and_align_dubbing_cue(
                    state.inner(),
                    &child_ffmpeg,
                    &prepared,
                    cancelled.clone(),
                )
                .await;
                bridge.abort();
                (prepared.cue_index, cancelled, synthesis)
            });
        }

        let mut first_error = None;
        let mut batch_cancelled = false;
        while let Some(joined) = jobs.join_next().await {
            let (cue_index, cancelled, synthesis) =
                joined.map_err(|error| format!("并发配音任务异常：{error}"))?;
            if let Err(error) = synthesis {
                let was_cancelled = cancelled.load(Ordering::Relaxed)
                    || error.contains("取消")
                    || error.to_ascii_lowercase().contains("cancel");
                let state = app.state::<crate::state::AppState>();
                let _session_io = state.dubbing_session_io.lock().await;
                let _ = fail_dubbing_cue(
                    app_config_dir,
                    &session_id,
                    cue_index,
                    &error,
                    was_cancelled,
                );
                batch_cancelled |= was_cancelled;
                if !was_cancelled && first_error.is_none() {
                    first_error = Some(error);
                }
            }
            completed += 1;
            update_pipeline_stage(
                app,
                tasks.clone(),
                task_id,
                PipelineStageKind::Dub,
                PipelineStageStatus::Running,
                (completed as f32 / total as f32).clamp(0.0, 1.0),
                &format!("正在配音... ({completed}/{total})"),
            )
            .await;
        }
        if batch_cancelled || check_cancelled(cancel_rx) {
            app.state::<crate::state::AppState>()
                .tts_worker
                .trim_idle_workers()
                .await;
            return Ok(handle_pipeline_stop(
                app,
                tasks,
                app_config_dir,
                task_id,
                PipelineStageKind::Dub,
                cancel_rx,
                stale,
            )
            .await);
        }
        if let Some(error) = first_error {
            app.state::<crate::state::AppState>()
                .tts_worker
                .trim_idle_workers()
                .await;
            return Err(error);
        }
    }

    app.state::<crate::state::AppState>()
        .tts_worker
        .trim_idle_workers()
        .await;

    let session = get_dubbing_session(app_config_dir, &session_id).map_err(|e| e.to_string())?;
    let overlong_count = session
        .cues
        .iter()
        .filter(|cue| cue.status == crate::core::tts::DubbingCueStatus::Overlong)
        .count();
    if overlong_count > 0 {
        let message =
            format!("有 {overlong_count} 句需要人工处理：编辑文本、重新生成或明确接受超速");
        update_pipeline_stage(
            app,
            tasks.clone(),
            task_id,
            PipelineStageKind::Dub,
            PipelineStageStatus::Review,
            1.0,
            &message,
        )
        .await;
        set_task_review_state(app, tasks, task_id, &message, stale).await;
        write_task_log(app, app_config_dir, task_id, &message).await;
        return Ok(PipelineRunOutcome::Review);
    }
    let output_path = match pipeline.dubbed_audio_path.clone() {
        Some(path) if !path.trim().is_empty() => path,
        _ => {
            let output_stem = resolve_output_stem(task, media_path)?;
            reserve_unique_output_path(media_path, output_stem.as_deref(), ".finalsub.dub", "wav")?
                .to_string_lossy()
                .to_string()
        }
    };
    if !std::fs::metadata(&output_path)
        .map(|metadata| metadata.is_file() && metadata.len() > 0)
        .unwrap_or(false)
    {
        let cancelled = Arc::new(AtomicBool::new(check_cancelled(cancel_rx)));
        if check_cancelled(cancel_rx) {
            return Ok(handle_pipeline_stop(
                app,
                tasks,
                app_config_dir,
                task_id,
                PipelineStageKind::Dub,
                cancel_rx,
                stale,
            )
            .await);
        }
        let bridge = pipeline_cancel_bridge(cancel_rx, cancelled.clone());
        let export_result = export_dubbing_audio(
            app_config_dir,
            &ffmpeg,
            &session_id,
            &output_path,
            cancelled.clone(),
        )
        .await;
        bridge.abort();
        if let Err(error) = export_result {
            let _ = std::fs::remove_file(&output_path);
            if cancelled.load(Ordering::Relaxed) || check_cancelled(cancel_rx) {
                return Ok(handle_pipeline_stop(
                    app,
                    tasks,
                    app_config_dir,
                    task_id,
                    PipelineStageKind::Dub,
                    cancel_rx,
                    stale,
                )
                .await);
            }
            return Err(error.to_string());
        }
    }
    set_pipeline_audio_path(app, tasks.clone(), task_id, &output_path).await;
    update_pipeline_stage(
        app,
        tasks,
        task_id,
        PipelineStageKind::Dub,
        PipelineStageStatus::Done,
        1.0,
        "配音完成",
    )
    .await;
    Ok(PipelineRunOutcome::Complete)
}

async fn probe_pipeline_audio_tracks(ffmpeg_path: &Path, video_path: &Path) -> usize {
    let output = tokio::process::Command::new(ffmpeg_path)
        .arg("-i")
        .arg(video_path)
        .output()
        .await;
    output
        .ok()
        .map(|output| {
            String::from_utf8_lossy(&output.stderr)
                .lines()
                .filter(|line| line.contains("Stream #") && line.contains("Audio:"))
                .count()
        })
        .unwrap_or(0)
}

async fn run_pipeline_ffmpeg(
    ffmpeg_path: &Path,
    args: &[String],
    cancel_rx: &mut watch::Receiver<bool>,
) -> Result<bool, String> {
    let mut child = tokio::process::Command::new(ffmpeg_path)
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|error| format!("无法启动成片 FFmpeg：{error}"))?;
    let mut cancel_wait = cancel_rx.clone();
    let cancelled = async move {
        loop {
            if *cancel_wait.borrow() {
                return true;
            }
            if cancel_wait.changed().await.is_err() {
                return true;
            }
        }
    };
    tokio::pin!(cancelled);
    let status = tokio::select! {
        result = child.wait() => result.map_err(|error| format!("等待成片 FFmpeg 结束失败：{error}"))?,
        _ = &mut cancelled => {
            let _ = child.kill().await;
            return Ok(false);
        }
    };
    if !status.success() {
        return Err("视频合成失败，请检查输入媒体与字幕格式".into());
    }
    Ok(true)
}

async fn run_compose_stage(
    context: &PipelineRunContext<'_>,
    dubbed_audio_path: Option<&str>,
    pipeline: &PipelineConfig,
    cancel_rx: &mut watch::Receiver<bool>,
) -> Result<PipelineRunOutcome, String> {
    let app = context.app;
    let tasks = context.tasks.clone();
    let app_config_dir = context.app_config_dir;
    let task_id = context.task_id;
    let task = context.task;
    let media_path = context.media_path;
    let subtitle_path = context.subtitle_path;
    let stale = context.stale;
    let compose = pipeline
        .compose
        .as_ref()
        .ok_or_else(|| "流水线缺少视频合成配置".to_string())?;
    if !media_path.is_file() || !subtitle_path.is_file() {
        return Err("视频或字幕产物已不存在，无法生成成片".into());
    }
    let ffmpeg = resolve_sidecar(app, "ffmpeg")?;
    update_pipeline_stage(
        app,
        tasks.clone(),
        task_id,
        PipelineStageKind::Compose,
        PipelineStageStatus::Running,
        0.0,
        "准备视频合成...",
    )
    .await;

    let mut audio_mode = crate::core::audio::ComposeAudioMode::parse(Some(&compose.audio_mode))
        .map_err(|error| error.to_string())?;
    let audio_path = dubbed_audio_path.filter(|path| Path::new(path).is_file());
    if matches!(
        audio_mode,
        crate::core::audio::ComposeAudioMode::Replace
            | crate::core::audio::ComposeAudioMode::Mix
            | crate::core::audio::ComposeAudioMode::AddTrack
    ) && audio_path.is_none()
    {
        return Err("成片配置需要配音音轨，但配音产物不存在".into());
    }
    if audio_path.is_none() {
        audio_mode = crate::core::audio::ComposeAudioMode::Keep;
    }
    let soft_subtitle = compose.soft_subtitle;
    let encoder_mode = if soft_subtitle {
        crate::core::audio::VideoEncoderMode::Cpu
    } else {
        crate::core::audio::VideoEncoderMode::parse(Some(&compose.encoder_mode))
            .map_err(|error| error.to_string())?
    };
    let style_snapshot = compose.style.clone().unwrap_or_default();
    style_snapshot.validate()?;
    let style = style_snapshot.to_burn_in_options();
    let encoding =
        crate::core::audio::resolve_video_encoding(&ffmpeg, encoder_mode, &style, media_path).await;
    let original_audio_tracks = if matches!(
        audio_mode,
        crate::core::audio::ComposeAudioMode::Mix | crate::core::audio::ComposeAudioMode::AddTrack
    ) {
        probe_pipeline_audio_tracks(&ffmpeg, media_path).await
    } else {
        0
    };
    let extension = if crate::core::audio::compose_requires_mkv(soft_subtitle, audio_mode) {
        "mkv"
    } else {
        "mp4"
    };
    let output_path = match pipeline.final_video_path.clone() {
        Some(path) if !path.trim().is_empty() => path,
        _ => {
            let output_stem = resolve_output_stem(task, media_path)?;
            reserve_unique_output_path(
                media_path,
                output_stem.as_deref(),
                ".finalsub.final",
                extension,
            )?
            .to_string_lossy()
            .to_string()
        }
    };
    let options = crate::core::audio::ComposeOptions {
        soft_subtitle,
        audio_mode,
        video_encoding: Some(encoding.clone()),
        audio_path: audio_path.map(ToOwned::to_owned),
        subtitle_language: task
            .target_language
            .clone()
            .or_else(|| task.source_language.clone()),
        subtitle_title: Some("FinalSub Subtitles".into()),
        audio_language: task.target_language.clone(),
        audio_title: Some("FinalSub Dub".into()),
        original_audio_tracks,
    };
    let mut active_encoding = encoding;
    let mut args = crate::core::audio::compose_args(
        &media_path.to_string_lossy(),
        &subtitle_path.to_string_lossy(),
        &output_path,
        &style,
        &options,
    )?;
    if check_cancelled(cancel_rx) {
        return Ok(handle_pipeline_stop(
            app,
            tasks,
            app_config_dir,
            task_id,
            PipelineStageKind::Compose,
            cancel_rx,
            stale,
        )
        .await);
    }
    loop {
        match run_pipeline_ffmpeg(&ffmpeg, &args, cancel_rx).await {
            Ok(true) => break,
            Err(_error)
                if active_encoding.hardware
                    && encoder_mode != crate::core::audio::VideoEncoderMode::Cpu =>
            {
                let _ = std::fs::remove_file(&output_path);
                update_pipeline_stage(
                    app,
                    tasks.clone(),
                    task_id,
                    PipelineStageKind::Compose,
                    PipelineStageStatus::Running,
                    0.05,
                    "硬件编码不可用，已自动切换 CPU 继续",
                )
                .await;
                active_encoding = crate::core::audio::cpu_video_encoding_for_style(&style);
                args = crate::core::audio::compose_args(
                    &media_path.to_string_lossy(),
                    &subtitle_path.to_string_lossy(),
                    &output_path,
                    &style,
                    &crate::core::audio::ComposeOptions {
                        video_encoding: Some(active_encoding.clone()),
                        ..options.clone()
                    },
                )?;
            }
            Err(error) => {
                let _ = std::fs::remove_file(&output_path);
                return Err(error);
            }
            Ok(false) => {
                let _ = std::fs::remove_file(&output_path);
                return Ok(handle_pipeline_stop(
                    app,
                    tasks,
                    app_config_dir,
                    task_id,
                    PipelineStageKind::Compose,
                    cancel_rx,
                    stale,
                )
                .await);
            }
        }
    }
    set_pipeline_final_path(app, tasks.clone(), task_id, &output_path).await;
    update_pipeline_stage(
        app,
        tasks,
        task_id,
        PipelineStageKind::Compose,
        PipelineStageStatus::Done,
        1.0,
        "成片已生成",
    )
    .await;
    Ok(PipelineRunOutcome::Complete)
}

async fn continue_pipeline_downstream(
    context: &PipelineRunContext<'_>,
    cancel_rx: &mut watch::Receiver<bool>,
) -> Result<PipelineRunOutcome, String> {
    let app = context.app;
    let tasks = context.tasks.clone();
    let task_id = context.task_id;
    let task = context.task;
    let stale = context.stale;
    let pipeline = {
        let task_map = tasks.read().await;
        task_map
            .get(task_id)
            .and_then(|current| current.pipeline.clone())
            .or_else(|| task.pipeline.clone())
            .ok_or_else(|| "流水线配置不存在".to_string())?
    };
    let current_stage = pipeline.current_stage;
    let skip_dub = matches!(
        current_stage,
        Some(PipelineStageKind::DubbingReview) | Some(PipelineStageKind::Compose)
    );
    let mut dubbed_audio = pipeline.dubbed_audio_path.clone();
    if pipeline.enable_dubbing && !skip_dub {
        match run_dubbing_stage(context, &pipeline, cancel_rx).await? {
            PipelineRunOutcome::Paused => return Ok(PipelineRunOutcome::Paused),
            PipelineRunOutcome::Review => return Ok(PipelineRunOutcome::Review),
            PipelineRunOutcome::Complete => {}
        }
        dubbed_audio = {
            let refreshed = tasks.read().await;
            refreshed
                .get(task_id)
                .and_then(|current| current.pipeline.as_ref())
                .and_then(|current| current.dubbed_audio_path.clone())
                .or(dubbed_audio)
        };

        if pipeline.dubbing_review {
            update_pipeline_stage(
                app,
                tasks.clone(),
                task_id,
                PipelineStageKind::DubbingReview,
                PipelineStageStatus::Review,
                1.0,
                "等待配音确认",
            )
            .await;
            set_task_review_state(app, tasks, task_id, "配音已生成，等待确认后继续", stale).await;
            return Ok(PipelineRunOutcome::Review);
        }
    }

    if pipeline.enable_compose {
        match run_compose_stage(context, dubbed_audio.as_deref(), &pipeline, cancel_rx).await? {
            PipelineRunOutcome::Paused => return Ok(PipelineRunOutcome::Paused),
            PipelineRunOutcome::Review => return Ok(PipelineRunOutcome::Review),
            PipelineRunOutcome::Complete => {}
        }
    }

    update_pipeline_stage(
        app,
        tasks.clone(),
        task_id,
        PipelineStageKind::Done,
        PipelineStageStatus::Done,
        1.0,
        "流水线已完成",
    )
    .await;
    let done_message = match (pipeline.enable_dubbing, pipeline.enable_compose) {
        (true, true) => "已完成：字幕、配音与成片均已就绪",
        (true, false) => "已完成：字幕与配音音轨均已就绪",
        (false, true) => "已完成：字幕与成片均已就绪",
        (false, false) => "已完成：字幕已就绪",
    };
    set_task_done(app, tasks, task_id, done_message, stale).await;
    Ok(PipelineRunOutcome::Complete)
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
        // 暂停或取消后的迟到进度不能把任务重新推回 Running。
        if matches!(task.status, TaskStatus::Cancelled | TaskStatus::Paused) {
            return;
        }
        task.status = TaskStatus::Running;
        task.progress = task.progress.clamp(0.0, 1.0).max(progress.clamp(0.0, 1.0));
        task.status_message = message.into();
        task.updated_at = chrono::Utc::now().to_rfc3339();
        let task_clone = task.clone();
        drop(task_map);
        emit_task_update_internal(app, &task_clone);
    }
}

/// 检查点收到取消信号后的统一处理：
/// 过时 runner（已被取消移除或被新一代 retry/resume 取代）静默退出，不得写任何任务状态；
/// 当前 runner 按暂停/取消分别记录日志或置为已取消。
async fn handle_cancel_signal(
    app: &AppHandle,
    tasks: Arc<RwLock<HashMap<String, Task>>>,
    app_config_dir: &Path,
    task_id: &str,
    stale: &AtomicBool,
    paused_log: &str,
) {
    if stale.load(Ordering::Relaxed) {
        return;
    }
    let current_status = {
        let task_map = tasks.read().await;
        task_map
            .get(task_id)
            .map(|t| t.status)
            .unwrap_or(TaskStatus::Cancelled)
    };
    if current_status == TaskStatus::Paused {
        write_task_log(app, app_config_dir, task_id, paused_log).await;
        return;
    }
    update_task_cancelled(app, tasks, task_id).await;
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

#[allow(clippy::too_many_arguments)]
async fn handle_translation_stop(
    app: &AppHandle,
    tasks: Arc<RwLock<HashMap<String, Task>>>,
    app_config_dir: &Path,
    task_id: &str,
    checkpoint_path: &Path,
    track: &SubtitleTrack,
    completed_cues: usize,
    stale: &AtomicBool,
) {
    if stale.load(Ordering::Relaxed) {
        // 过时 runner：不得覆盖新一代 runner 的断点或改写任务状态。
        return;
    }
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

fn restore_translated_checkpoint(track: &mut SubtitleTrack, checkpoint: &SubtitleTrack) -> usize {
    let checkpoint_len = checkpoint.len();
    if checkpoint_len == 0 || checkpoint_len > track.len() {
        return 0;
    }

    for idx in 0..checkpoint_len {
        if checkpoint.cues[idx]
            .text
            .contains(TRANSLATION_FAILED_PLACEHOLDER)
        {
            // 占位符表示该行未翻译成功：从此行起重新翻译，不把失败文本当作已完成译文。
            return idx;
        }
        track.cues[idx].text = checkpoint.cues[idx].text.clone();
    }
    checkpoint_len
}

fn translation_progress_start(task_type: TaskType) -> f32 {
    match task_type {
        TaskType::TranslateOnly => TRANSLATION_ONLY_PROGRESS_START,
        TaskType::GenerateAndTranslate | TaskType::GenerateOnly => TRANSLATION_PROGRESS_START,
    }
}

fn translation_progress_for(task_type: TaskType, completed_cues: usize, total_cues: usize) -> f32 {
    if total_cues == 0 {
        return translation_progress_start(task_type);
    }

    let ratio = (completed_cues as f32 / total_cues as f32).clamp(0.0, 1.0);
    let start = translation_progress_start(task_type);
    start + (TRANSLATION_PROGRESS_END - start) * ratio
}

fn asr_progress_end(task_type: TaskType) -> f32 {
    match task_type {
        TaskType::GenerateAndTranslate => ASR_PROGRESS_END_WITH_TRANSLATION,
        TaskType::GenerateOnly => ASR_PROGRESS_END_GENERATE_ONLY,
        TaskType::TranslateOnly => ASR_PROGRESS_START,
    }
}

fn map_asr_progress(task_type: TaskType, engine_progress: f32) -> f32 {
    let end = asr_progress_end(task_type);
    let ratio = engine_progress.clamp(0.0, 1.0);
    ASR_PROGRESS_START + (end - ASR_PROGRESS_START) * ratio
}

fn translation_supports_batch(provider_info: Option<&TranslationProvider>) -> bool {
    provider_info.map(|item| item.is_ai).unwrap_or(false)
}

fn translation_batch_end(cues: &[Cue], start_index: usize, max_batch_cues: usize) -> usize {
    if start_index >= cues.len() {
        return start_index;
    }

    let mut end_index = start_index;
    let mut char_count = 0usize;
    let max_batch_cues = max_batch_cues.clamp(1, 50);
    while end_index < cues.len() && end_index - start_index < max_batch_cues {
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
    source_texts: &[String],
    echo_anchoring: bool,
    alignment_retry: bool,
) -> (String, Vec<String>) {
    let mut input = serde_json::Map::new();
    let mut keys = Vec::with_capacity(source_texts.len());
    for (offset, source_text) in source_texts.iter().enumerate() {
        let key = (offset + 1).to_string();
        keys.push(key.clone());
        input.insert(key, serde_json::Value::String(source_text.clone()));
    }

    let input_json = serde_json::to_string(&serde_json::Value::Object(input))
        .unwrap_or_else(|_| "{}".to_string());
    let output_contract = if echo_anchoring {
        "For every input key, return an object with exactly two string fields: \
{\"src\": the source text copied verbatim from that same key, \"tr\": the translation}. \
The src field must remain in the source language and must never contain the translation."
    } else {
        "For every input key, return its translated text as a JSON string value."
    };
    let retry_notice = if alignment_retry {
        "\nThe previous response was missing, merged, or shifted across subtitle IDs. \
This is a strict alignment retry: copy each src from its own key and never merge, split, or reorder entries."
    } else {
        ""
    };
    let prompt = format!(
        "Translate each JSON value from {source_language} to {target_language}. \
{output_contract} Return only one valid JSON object with exactly the same keys. \
Do not add Markdown fences, comments, explanations, thinking, or extra keys. \
Preserve line breaks inside each value and never merge, split, or reorder subtitle entries.\
{retry_notice}\n\nInput JSON:\n{input_json}"
    );

    (prompt, keys)
}

fn make_batch_translation_schema(
    expected_keys: &[String],
    echo_anchoring: bool,
) -> serde_json::Value {
    let value_schema = if echo_anchoring {
        serde_json::json!({
            "type": "object",
            "properties": {
                "src": {
                    "type": "string",
                    "description": "Copy the source subtitle for this exact ID verbatim; keep the source language and never put the translation here."
                },
                "tr": {
                    "type": "string",
                    "description": "Translation for this exact subtitle ID."
                }
            },
            "required": ["src", "tr"],
            "additionalProperties": false
        })
    } else {
        serde_json::json!({
            "type": "string",
            "description": "Translation for this exact subtitle ID."
        })
    };
    let properties = expected_keys
        .iter()
        .map(|key| (key.clone(), value_schema.clone()))
        .collect::<serde_json::Map<_, _>>();
    serde_json::json!({
        "type": "object",
        "properties": properties,
        "required": expected_keys,
        "additionalProperties": false
    })
}

fn parse_batch_translation_response(
    raw_text: &str,
    expected_keys: &[String],
) -> Result<HashMap<String, ParsedBatchTranslation>, String> {
    let cleaned =
        extract_json_object(raw_text).ok_or_else(|| "响应中没有 JSON 对象".to_string())?;
    let value: serde_json::Value =
        serde_json::from_str(cleaned).map_err(|e| format!("JSON 解析失败：{e}"))?;
    let object = value
        .as_object()
        .ok_or_else(|| "响应不是 JSON 对象".to_string())?;

    if let Some(extra_key) = object
        .keys()
        .find(|key| !expected_keys.iter().any(|expected| expected == *key))
    {
        return Err(format!("响应包含额外键 {extra_key}"));
    }

    let mut parsed = HashMap::with_capacity(expected_keys.len());
    for key in expected_keys {
        let Some(item) = object.get(key) else {
            continue;
        };
        let entry = match item {
            serde_json::Value::String(translation) => ParsedBatchTranslation {
                echoed_source: None,
                translation: translation.trim().to_string(),
            },
            serde_json::Value::Object(fields) => {
                if let Some(extra_field) = fields
                    .keys()
                    .find(|field| !matches!(field.as_str(), "src" | "tr"))
                {
                    return Err(format!("字幕 {key} 包含额外字段 {extra_field}"));
                }
                ParsedBatchTranslation {
                    echoed_source: fields
                        .get("src")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_string),
                    translation: fields
                        .get("tr")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or_default()
                        .trim()
                        .to_string(),
                }
            }
            _ => continue,
        };
        parsed.insert(key.clone(), entry);
    }

    Ok(parsed)
}

fn validate_batch_alignment(
    parsed: &HashMap<String, ParsedBatchTranslation>,
    expected_keys: &[String],
    source_texts: &[String],
    echo_anchoring: bool,
) -> BatchAlignmentValidation {
    let mut validation = BatchAlignmentValidation::default();
    for (index, key) in expected_keys.iter().enumerate() {
        let Some(entry) = parsed.get(key) else {
            validation.flagged.push(key.clone());
            continue;
        };
        if entry.translation.trim().is_empty() {
            validation.flagged.push(key.clone());
            continue;
        }
        if echo_anchoring {
            let Some(echoed_source) = entry.echoed_source.as_deref() else {
                validation.flagged.push(key.clone());
                continue;
            };
            validation.echo_checked += 1;
            if text_similarity(echoed_source, &source_texts[index]) < ECHO_SIMILARITY_THRESHOLD {
                validation.flagged.push(key.clone());
                continue;
            }
        }
        validation
            .accepted
            .insert(key.clone(), entry.translation.clone());
    }
    validation
}

fn normalize_for_alignment(text: &str) -> Vec<char> {
    crate::core::glossary::glossary_source_key(text)
        .chars()
        .filter(|ch| ch.is_alphanumeric())
        .collect()
}

fn text_similarity(left: &str, right: &str) -> f64 {
    if left.trim() == right.trim() {
        return 1.0;
    }
    let left = normalize_for_alignment(left);
    let right = normalize_for_alignment(right);
    if left.is_empty() || right.is_empty() {
        return 0.0;
    }
    if left == right {
        return 1.0;
    }

    let mut previous = (0..=right.len()).collect::<Vec<_>>();
    let mut current = vec![0usize; right.len() + 1];
    for (left_index, left_char) in left.iter().enumerate() {
        current[0] = left_index + 1;
        for (right_index, right_char) in right.iter().enumerate() {
            let substitution = previous[right_index] + usize::from(left_char != right_char);
            current[right_index + 1] = (previous[right_index + 1] + 1)
                .min(current[right_index] + 1)
                .min(substitution);
        }
        std::mem::swap(&mut previous, &mut current);
    }
    1.0 - previous[right.len()] as f64 / left.len().max(right.len()) as f64
}

fn build_repair_translation_prompt(
    target_key: &str,
    expected_keys: &[String],
    source_texts: &[String],
    accepted: &HashMap<String, String>,
    target_language: &str,
    echo_anchoring: bool,
) -> String {
    let target_index = expected_keys
        .iter()
        .position(|key| key == target_key)
        .unwrap_or(0);
    let start = target_index.saturating_sub(2);
    let end = (target_index + 3).min(expected_keys.len());
    let context = (start..end)
        .map(|index| {
            serde_json::json!({
                "id": expected_keys[index],
                "source": source_texts[index],
                "accepted_translation_for_context_only": accepted.get(&expected_keys[index])
            })
        })
        .collect::<Vec<_>>();
    let output_contract = if echo_anchoring {
        format!(
            "Return only {{\"{target_key}\":{{\"src\":<copy the target source verbatim>,\"tr\":<translation>}}}}."
        )
    } else {
        format!("Return only {{\"{target_key}\":<translation string>}}.")
    };
    format!(
        "Repair exactly one subtitle translation into {target_language}. \
The neighboring entries below are context only: do not translate or return them. \
Translate only ID {target_key}; do not merge it with neighboring text. {output_contract}\n\n\
Context JSON:\n{}",
        serde_json::to_string_pretty(&context).unwrap_or_else(|_| "[]".into())
    )
}

async fn translate_aligned_batch(
    base_request: &TranslateRequest,
    source_texts: &[String],
    structured_output: &str,
    echo_anchoring: bool,
    retry_times: u32,
    cancel_rx: &mut tokio::sync::watch::Receiver<bool>,
) -> AlignedBatchResult {
    let mut alignment_retry_used = false;
    let mut last_validation = BatchAlignmentValidation::default();
    let mut expected_keys = Vec::new();

    for alignment_attempt in 0..=1 {
        let (prompt, keys) = build_batch_translation_prompt(
            &base_request.source_language,
            &base_request.target_language,
            source_texts,
            echo_anchoring,
            alignment_attempt > 0,
        );
        expected_keys = keys;
        let mut request = base_request.clone();
        request.text = prompt;
        request.structured_output = Some(structured_output.to_string());
        request.response_json_schema = Some(make_batch_translation_schema(
            &expected_keys,
            echo_anchoring,
        ));

        let raw_text = match translate_with_retries(&request, retry_times, cancel_rx).await {
            TranslationAttemptResult::Success(text) => text,
            TranslationAttemptResult::Cancelled => return AlignedBatchResult::Cancelled,
            TranslationAttemptResult::Failed(error) => return AlignedBatchResult::Failed(error),
        };
        let parsed = parse_batch_translation_response(&raw_text, &expected_keys)
            .unwrap_or_else(|_| HashMap::new());
        last_validation =
            validate_batch_alignment(&parsed, &expected_keys, source_texts, echo_anchoring);
        let large_mismatch_threshold = expected_keys.len().div_ceil(3);
        if last_validation.flagged.len() > large_mismatch_threshold && alignment_attempt == 0 {
            alignment_retry_used = true;
            continue;
        }
        break;
    }

    let flagged_count = last_validation.flagged.len();
    let flagged_keys = last_validation.flagged.clone();
    let mut repaired = 0usize;
    for key in flagged_keys {
        let mut repaired_translation = None;
        for _ in 0..ALIGNMENT_REPAIR_MAX_ATTEMPTS {
            let mut request = base_request.clone();
            request.text = build_repair_translation_prompt(
                &key,
                &expected_keys,
                source_texts,
                &last_validation.accepted,
                &base_request.target_language,
                echo_anchoring,
            );
            request.structured_output = Some(structured_output.to_string());
            request.response_json_schema = Some(make_batch_translation_schema(
                std::slice::from_ref(&key),
                echo_anchoring,
            ));
            let raw_text =
                match translate_with_retries(&request, retry_times.min(1), cancel_rx).await {
                    TranslationAttemptResult::Success(text) => text,
                    TranslationAttemptResult::Cancelled => return AlignedBatchResult::Cancelled,
                    TranslationAttemptResult::Failed(_) => continue,
                };
            let Ok(parsed) =
                parse_batch_translation_response(&raw_text, std::slice::from_ref(&key))
            else {
                continue;
            };
            let source_index = expected_keys
                .iter()
                .position(|item| item == &key)
                .unwrap_or(0);
            let validation = validate_batch_alignment(
                &parsed,
                std::slice::from_ref(&key),
                std::slice::from_ref(&source_texts[source_index]),
                echo_anchoring,
            );
            if let Some(translation) = validation.accepted.get(&key) {
                repaired_translation = Some(translation.clone());
                break;
            }
        }
        if let Some(translation) = repaired_translation {
            last_validation.accepted.insert(key, translation);
            repaired += 1;
        }
    }

    let unresolved = expected_keys
        .iter()
        .filter(|key| !last_validation.accepted.contains_key(*key))
        .count();
    let translations = expected_keys
        .iter()
        .map(|key| {
            last_validation
                .accepted
                .get(key)
                .cloned()
                .unwrap_or_else(|| TRANSLATION_FAILED_PLACEHOLDER.into())
        })
        .collect();
    AlignedBatchResult::Success(AlignedBatchReport {
        translations,
        echo_checked: last_validation.echo_checked,
        flagged: flagged_count,
        repaired,
        unresolved,
        alignment_retry_used,
    })
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

/// 认证/权限类翻译错误重试不会成功；批量+逐句降级双路径下盲目重试会放大为
/// (retry_times+1)×2 次无效请求，直接判失败让用户先修配置。
fn is_non_retryable_translation_error(error: &str) -> bool {
    let lower = error.to_ascii_lowercase();
    lower.contains("401")
        || lower.contains("403")
        || lower.contains("unauthorized")
        || lower.contains("forbidden")
        || lower.contains("invalid api key")
        || lower.contains("invalid_api_key")
        || lower.contains("incorrect api key")
        || error.contains("缺少必要凭据")
        || error.contains("尚未保存")
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

        if is_non_retryable_translation_error(&last_err) {
            break;
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

async fn wait_translation_interval(
    interval_ms: u64,
    cancel_rx: &mut tokio::sync::watch::Receiver<bool>,
) -> bool {
    if interval_ms == 0 {
        return !check_cancelled(cancel_rx);
    }
    let delay = tokio::time::sleep(std::time::Duration::from_millis(interval_ms));
    tokio::pin!(delay);
    loop {
        tokio::select! {
            _ = &mut delay => return true,
            changed = cancel_rx.changed() => {
                if changed.is_err() || *cancel_rx.borrow() {
                    return false;
                }
            }
        }
    }
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
    stem_override: Option<&str>,
    suffix: &str,
    format: &str,
) -> Result<PathBuf, String> {
    let parent = media_path.parent().ok_or("媒体文件必须有父级目录")?;
    let stem = match stem_override {
        Some(stem) if !stem.trim().is_empty() => stem.trim(),
        _ => media_path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or("无法获取媒体文件名")?,
    };

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

fn resolve_output_stem(task: &Task, media_path: &Path) -> Result<Option<String>, String> {
    let Some(template) = task.output_name.as_deref() else {
        return Ok(None);
    };
    let source_stem = media_path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .ok_or("无法获取媒体文件名")?;
    let language = task
        .target_language
        .as_deref()
        .unwrap_or(task.source_language.as_deref().unwrap_or("auto"));
    let resolved = template
        .replace("{name}", source_stem)
        .replace("{lang}", language)
        .trim()
        .to_string();
    if resolved.is_empty() || matches!(resolved.as_str(), "." | "..") {
        return Err("输出名称模板解析后为空或无效".into());
    }
    Ok(Some(resolved))
}

fn strip_chinese_punctuation(text: &str) -> String {
    const PUNCTUATION: &[char] = &[
        '，', '。', '！', '？', '；', '：', '、', '“', '”', '‘', '’', '（', '）', '【', '】', '《',
        '》', '〈', '〉', '…', '—', '·', '～', '﹏', '「', '」', '『', '』',
    ];
    text.chars()
        .filter(|character| !PUNCTUATION.contains(character))
        .collect()
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
    if let Ok(entry) = crate::core::logs::append_task_log(app_config_dir, task_id, message).await {
        app.emit(crate::core::logs::LOG_EVENT, entry).ok();
    }
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
    fn provided_subtitle_parser_accepts_timed_formats_and_rejects_empty_tracks() {
        let parsed = parse_subtitle_file(
            Path::new("/tmp/episode.vtt"),
            "WEBVTT\n\n00:00:01.000 --> 00:00:02.000\nHello\n",
            "已配对字幕",
        )
        .unwrap();
        assert_eq!(parsed.len(), 1);
        assert!(parse_subtitle_file(Path::new("/tmp/empty.srt"), "", "已配对字幕").is_err());
    }

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
        assert!(
            (translation_progress_for(TaskType::GenerateAndTranslate, restored, track.len())
                - 0.90)
                .abs()
                < 0.0001
        );
    }

    #[test]
    fn non_retryable_translation_errors_are_classified() {
        assert!(is_non_retryable_translation_error("HTTP 401 Unauthorized"));
        assert!(is_non_retryable_translation_error("status 403 Forbidden"));
        assert!(is_non_retryable_translation_error(
            "OpenAI: invalid api key provided"
        ));
        assert!(is_non_retryable_translation_error(
            "百度翻译 缺少必要凭据：App ID"
        ));
        assert!(!is_non_retryable_translation_error(
            "HTTP 429 Too Many Requests"
        ));
        assert!(!is_non_retryable_translation_error("connection timed out"));
        assert!(!is_non_retryable_translation_error(
            "HTTP 500 Internal Server Error"
        ));
    }

    #[test]
    fn translated_checkpoint_treats_failure_placeholder_as_untranslated() {
        let mut track = SubtitleTrack::from_srt(
            "1\n00:00:01,000 --> 00:00:02,000\nHello\n\n\
             2\n00:00:02,000 --> 00:00:03,000\nWorld\n\n\
             3\n00:00:03,000 --> 00:00:04,000\nAgain\n\n",
        )
        .unwrap();
        let checkpoint = SubtitleTrack::from_srt(&format!(
            "1\n00:00:01,000 --> 00:00:02,000\n你好\n\n\
             2\n00:00:02,000 --> 00:00:03,000\n{TRANSLATION_FAILED_PLACEHOLDER}\n\n\
             3\n00:00:03,000 --> 00:00:04,000\n再次\n\n",
        ))
        .unwrap();

        let restored = restore_translated_checkpoint(&mut track, &checkpoint);

        // 首个占位符行截断恢复：该行及其后全部重新翻译。
        assert_eq!(restored, 1);
        assert_eq!(track.cues[0].text, "你好");
        assert_eq!(track.cues[1].text, "World");
        assert_eq!(track.cues[2].text, "Again");
    }

    #[test]
    fn asr_progress_uses_task_specific_endpoints() {
        assert!((map_asr_progress(TaskType::GenerateAndTranslate, 1.0) - 0.80).abs() < 0.0001);
        assert!((map_asr_progress(TaskType::GenerateOnly, 1.0) - 0.95).abs() < 0.0001);
        assert!(
            (map_asr_progress(TaskType::GenerateOnly, 0.0) - ASR_PROGRESS_START).abs() < 0.0001
        );
    }

    #[test]
    fn translate_only_progress_starts_near_beginning() {
        assert!((translation_progress_start(TaskType::TranslateOnly) - 0.05).abs() < 0.0001);
        assert!((translation_progress_for(TaskType::TranslateOnly, 1, 2) - 0.50).abs() < 0.0001);
    }

    #[test]
    fn batch_translation_prompt_and_response_preserve_key_alignment() {
        let sources = vec!["Hello".to_string(), "World".to_string()];
        let (prompt, keys) = build_batch_translation_prompt("en", "zh", &sources, true, false);
        let parsed = parse_batch_translation_response(
            "```json\n{\"1\":{\"src\":\"Hello\",\"tr\":\"你好\"},\"2\":{\"src\":\"World\",\"tr\":\"世界\"}}\n```",
            &keys,
        )
        .unwrap();
        let validated = validate_batch_alignment(&parsed, &keys, &sources, true);

        assert!(prompt.contains("same keys"));
        assert_eq!(keys, vec!["1".to_string(), "2".to_string()]);
        assert_eq!(validated.echo_checked, 2);
        assert!(validated.flagged.is_empty());
        assert_eq!(validated.accepted["1"], "你好");
        assert_eq!(validated.accepted["2"], "世界");
    }

    #[test]
    fn batch_translation_validation_flags_missing_and_shifted_entries() {
        let keys = vec!["1".to_string(), "2".to_string()];
        let sources = vec!["Hello".to_string(), "World".to_string()];
        let parsed =
            parse_batch_translation_response("{\"1\":{\"src\":\"World\",\"tr\":\"世界\"}}", &keys)
                .unwrap();
        let validated = validate_batch_alignment(&parsed, &keys, &sources, true);

        assert_eq!(validated.flagged, keys);
        assert!(validated.accepted.is_empty());
    }

    #[test]
    fn dynamic_batch_schema_locks_keys_and_echo_shape() {
        let keys = vec!["1".to_string(), "2".to_string()];
        let schema = make_batch_translation_schema(&keys, true);

        assert_eq!(schema["required"], serde_json::json!(["1", "2"]));
        assert_eq!(schema["additionalProperties"], false);
        assert_eq!(
            schema["properties"]["1"]["required"],
            serde_json::json!(["src", "tr"])
        );
        assert_eq!(schema["properties"]["1"]["additionalProperties"], false);
    }

    #[test]
    fn batch_translation_response_rejects_extra_keys() {
        let keys = vec!["1".to_string()];
        let error = parse_batch_translation_response("{\"1\":\"你好\",\"extra\":\"不要\"}", &keys)
            .unwrap_err();
        assert!(error.contains("额外键 extra"));

        let nested = parse_batch_translation_response(
            "{\"1\":{\"src\":\"Hello\",\"tr\":\"你好\",\"reason\":\"不要\"}}",
            &keys,
        )
        .unwrap_err();
        assert!(nested.contains("额外字段 reason"));
    }

    #[test]
    fn echo_similarity_ignores_formatting_but_detects_merged_text() {
        assert!(text_similarity("Hello, WORLD!", "hello world") > 0.99);
        assert!(text_similarity("Hello world and next subtitle", "Hello world") < 0.75);
    }

    #[test]
    fn repair_prompt_returns_only_target_with_neighbor_context() {
        let keys = (1..=6).map(|index| index.to_string()).collect::<Vec<_>>();
        let sources = (1..=6)
            .map(|index| format!("source {index}"))
            .collect::<Vec<_>>();
        let prompt = build_repair_translation_prompt(
            "4",
            &keys,
            &sources,
            &HashMap::from([("3".into(), "译文三".into())]),
            "zh",
            true,
        );

        assert!(prompt.contains("Translate only ID 4"));
        assert!(prompt.contains("source 2"));
        assert!(prompt.contains("source 6"));
        assert!(!prompt.contains("source 1"));
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
        assert_eq!(translation_batch_end(&short_cues, 0, 24), 24);

        let long_cues = (0..4)
            .map(|idx| Cue {
                index: idx,
                start_ms: idx as u64 * 1000,
                end_ms: idx as u64 * 1000 + 500,
                text: "长".repeat(2_500),
            })
            .collect::<Vec<_>>();
        assert_eq!(translation_batch_end(&long_cues, 0, 24), 1);
    }

    #[test]
    fn output_path_uses_counter_without_overwriting() {
        let tmp = TempDir::new().unwrap();
        let media = tmp.path().join("clip.mp4");
        std::fs::write(&media, b"media").unwrap();
        std::fs::write(tmp.path().join("clip.finalsub.srt"), b"existing").unwrap();

        let output = reserve_unique_output_path(&media, None, ".finalsub", "srt").unwrap();
        assert_eq!(output, tmp.path().join("clip.finalsub-1.srt"));
        assert!(output.exists());
    }

    #[test]
    fn output_path_reservation_is_atomic_for_repeated_tasks() {
        let tmp = TempDir::new().unwrap();
        let media = tmp.path().join("clip.mp4");
        std::fs::write(&media, b"media").unwrap();

        let first = reserve_unique_output_path(&media, None, ".finalsub", "srt").unwrap();
        let second = reserve_unique_output_path(&media, None, ".finalsub", "srt").unwrap();

        assert_eq!(first, tmp.path().join("clip.finalsub.srt"));
        assert_eq!(second, tmp.path().join("clip.finalsub-1.srt"));
        assert!(first.exists());
        assert!(second.exists());
    }

    #[test]
    fn output_path_truncates_long_media_stem() {
        let tmp = TempDir::new().unwrap();
        let media = tmp.path().join(format!("{}.mp4", "a".repeat(320)));

        let output = reserve_unique_output_path(&media, None, ".finalsub", "srt").unwrap();
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

        let output = reserve_unique_output_path(&media, None, ".finalsub.zh", "srt").unwrap();
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

    #[test]
    fn custom_output_stem_replaces_source_name_without_overwriting() {
        let temp = tempfile::tempdir().unwrap();
        let media = temp.path().join("episode-01.mp4");
        std::fs::write(&media, b"video").unwrap();
        let output =
            reserve_unique_output_path(&media, Some("season-one-episode-01"), ".finalsub", "srt")
                .unwrap();
        assert_eq!(
            output.file_name().and_then(|name| name.to_str()),
            Some("season-one-episode-01.finalsub.srt")
        );
    }

    #[test]
    fn chinese_punctuation_removal_preserves_latin_punctuation_and_line_breaks() {
        assert_eq!(
            strip_chinese_punctuation("你好，世界！\nHello, world!"),
            "你好世界\nHello, world!"
        );
    }
}
