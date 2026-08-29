use async_trait::async_trait;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::SystemTime;

use super::{AsrCapabilities, AsrEngine, AsrModelRef, ProgressSink, ProgressUpdate, TranscribeJob};
use crate::core::subtitle::{Cue, SubtitleTrack};
use crate::error::{FinalSubError, Result};

pub const PARAKEET_MODEL_ID: &str = "parakeet-tdt-0.6b-v2";
pub const PARAKEET_MLX_MODEL_ID: &str = "mlx-community/parakeet-tdt-0.6b-v2";
pub const PARAKEET_ARCHIVE_DIR: &str = "sherpa-onnx-nemo-parakeet-tdt-0.6b-v2-int8";
pub(super) const REQUIRED_MODEL_FILES: &[&str] = &[
    "encoder.int8.onnx",
    "decoder.int8.onnx",
    "joiner.int8.onnx",
    "tokens.txt",
];

const PARAKEET_MLX_REPO_DIR: &str = "models--mlx-community--parakeet-tdt-0.6b-v2";
const PARAKEET_MLX_MIN_WEIGHT_BYTES: u64 = 1_000_000_000;
const PARAKEET_MLX_PYTHON: &str = "3.11";
const PARAKEET_MLX_PACKAGE: &str = "parakeet-mlx==0.5.2";

/// MLX 仅在 Apple Silicon 上可用；Native sherpa-onnx 仍作为跨平台兜底。
pub fn mlx_runtime_supported() -> bool {
    cfg!(all(target_os = "macos", target_arch = "aarch64"))
}

fn mlx_huggingface_cache(cache_root: &Path) -> PathBuf {
    if cache_root
        .file_name()
        .is_some_and(|name| name == "huggingface")
    {
        cache_root.to_path_buf()
    } else {
        cache_root.join("huggingface")
    }
}

fn is_complete_mlx_snapshot(snapshot: &Path) -> bool {
    let config = snapshot.join("config.json");
    let weights = snapshot.join("model.safetensors");
    config.is_file()
        && std::fs::metadata(&config).is_ok_and(|metadata| metadata.len() > 0)
        && weights.is_file()
        && std::fs::metadata(&weights)
            .is_ok_and(|metadata| metadata.len() >= PARAKEET_MLX_MIN_WEIGHT_BYTES)
}

fn valid_revision(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.chars().all(|character| character.is_ascii_hexdigit())
}

/// Find a complete Parakeet MLX snapshot in the standard Hugging Face cache.
///
/// The returned directory is always beneath `cache_root/huggingface`; no
/// network lookup or cache mutation is performed here.
pub fn find_mlx_model_snapshot(cache_root: &Path) -> Option<PathBuf> {
    let repository = mlx_huggingface_cache(cache_root).join(PARAKEET_MLX_REPO_DIR);
    let snapshots = repository.join("snapshots");

    if let Ok(revision) = std::fs::read_to_string(repository.join("refs/main")) {
        let revision = revision.trim();
        if valid_revision(revision) {
            let candidate = snapshots.join(revision);
            if is_complete_mlx_snapshot(&candidate) {
                return Some(candidate);
            }
        }
    }

    let mut candidates = std::fs::read_dir(&snapshots)
        .ok()?
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let file_type = entry.file_type().ok()?;
            if !file_type.is_dir() {
                return None;
            }
            let path = entry.path();
            is_complete_mlx_snapshot(&path).then_some(path)
        })
        .collect::<Vec<_>>();
    candidates.sort_by_key(|path| {
        std::fs::metadata(path)
            .and_then(|metadata| metadata.modified())
            .unwrap_or(SystemTime::UNIX_EPOCH)
    });
    candidates.pop()
}

pub fn is_mlx_model_installed_at(cache_root: &Path) -> bool {
    find_mlx_model_snapshot(cache_root).is_some()
}

fn command_available(command: &Path) -> bool {
    if command.components().count() > 1 || command.is_absolute() {
        return command.is_file();
    }
    std::env::var_os("PATH")
        .into_iter()
        .flat_map(|path| std::env::split_paths(&path).collect::<Vec<_>>())
        .map(|directory| directory.join(command))
        .any(|candidate| candidate.is_file())
}

pub fn default_uv_bin() -> PathBuf {
    let mut candidates = vec![PathBuf::from("/opt/homebrew/bin/uv")];
    if let Some(home) = std::env::var_os("HOME") {
        candidates.push(PathBuf::from(home).join(".local/bin/uv"));
    }
    candidates.push(PathBuf::from("/usr/local/bin/uv"));
    candidates.extend(
        std::env::var_os("PATH")
            .into_iter()
            .flat_map(|path| std::env::split_paths(&path).collect::<Vec<_>>())
            .map(|directory| directory.join("uv")),
    );
    candidates
        .into_iter()
        .find(|candidate| candidate.is_file())
        .unwrap_or_else(|| PathBuf::from("uv"))
}

fn prepend_path(directory: &Path, inherited_path: Option<&OsStr>) -> Option<OsString> {
    let mut paths = vec![directory.to_path_buf()];
    if let Some(inherited_path) = inherited_path {
        paths.extend(std::env::split_paths(inherited_path));
    }
    std::env::join_paths(paths).ok()
}

fn path_with_ffmpeg(ffmpeg_path: Option<&Path>) -> Option<OsString> {
    let inherited_path = std::env::var_os("PATH");
    let Some(ffmpeg_path) = ffmpeg_path else {
        return inherited_path;
    };
    let Some(directory) = ffmpeg_path
        .parent()
        .filter(|directory| !directory.as_os_str().is_empty())
    else {
        return inherited_path;
    };
    prepend_path(directory, inherited_path.as_deref())
}

/// MLX backend that invokes the maintained `parakeet-mlx` package through uv.
/// The model itself is always passed as a local snapshot, so a complete cache
/// never triggers a Hugging Face model download.
pub struct ParakeetMlxEngine {
    uv_bin: PathBuf,
    transcribe_script: PathBuf,
    cache_root: PathBuf,
    ffmpeg_path: Option<PathBuf>,
}

impl ParakeetMlxEngine {
    pub fn new(
        uv_bin: PathBuf,
        transcribe_script: PathBuf,
        cache_root: PathBuf,
        ffmpeg_path: Option<PathBuf>,
    ) -> Self {
        Self {
            uv_bin,
            transcribe_script,
            cache_root,
            ffmpeg_path,
        }
    }

    pub fn model_path(&self, model: &AsrModelRef) -> Option<PathBuf> {
        model
            .model_path
            .as_deref()
            .filter(|path| !path.trim().is_empty())
            .map(PathBuf::from)
            .filter(|path| is_complete_mlx_snapshot(path))
            .or_else(|| find_mlx_model_snapshot(&self.cache_root))
    }

    pub fn is_model_installed_at(cache_root: &Path) -> bool {
        is_mlx_model_installed_at(cache_root)
    }

    fn missing_model_error(&self) -> FinalSubError {
        FinalSubError::Validation(format!(
            "Parakeet MLX V2 缓存不完整。请确认模型缓存位于 {}/huggingface/models--mlx-community--parakeet-tdt-0.6b-v2/snapshots/<revision>/，并包含完整的 config.json 与 model.safetensors",
            self.cache_root.display()
        ))
    }
}

#[async_trait]
impl AsrEngine for ParakeetMlxEngine {
    fn id(&self) -> &'static str {
        "parakeet-mlx"
    }

    fn capabilities(&self) -> AsrCapabilities {
        AsrCapabilities {
            supports_streaming: false,
            supported_languages: vec!["en".into(), "auto".into()],
            requires_model_download: false,
        }
    }

    async fn prepare(&self, model: &AsrModelRef) -> Result<()> {
        if !mlx_runtime_supported() {
            return Err(FinalSubError::Validation(
                "Parakeet MLX 仅支持 Apple Silicon；当前平台将使用 Native 兜底".into(),
            ));
        }
        if !command_available(&self.uv_bin) {
            return Err(FinalSubError::Validation(
                "未找到 uv，无法启动 Parakeet MLX。请安装 uv：https://docs.astral.sh/uv/".into(),
            ));
        }
        if !self.transcribe_script.is_file() {
            return Err(FinalSubError::Validation(format!(
                "Parakeet MLX 转录脚本未找到：{}",
                self.transcribe_script.display()
            )));
        }
        if self
            .ffmpeg_path
            .as_deref()
            .is_some_and(|path| !path.is_file())
            || (self.ffmpeg_path.is_none() && !command_available(Path::new("ffmpeg")))
        {
            return Err(FinalSubError::Validation(
                "未找到 FFmpeg，Parakeet MLX 无法读取音频；请重新安装 FinalSub 以恢复内置 FFmpeg"
                    .into(),
            ));
        }
        if self
            .model_path(model)
            .is_none_or(|path| !is_complete_mlx_snapshot(&path))
        {
            return Err(self.missing_model_error());
        }

        // Resolve the executable once during preparation so a Finder-launched
        // app reports a useful error before creating a task subprocess.
        let version = tokio::process::Command::new(&self.uv_bin)
            .arg("--version")
            .output()
            .await
            .map_err(|error| FinalSubError::Validation(format!("启动 uv 失败：{error}")))?;
        if !version.status.success() {
            return Err(FinalSubError::Validation(
                "uv 无法运行，请检查本机 uv 安装".into(),
            ));
        }
        Ok(())
    }

    async fn transcribe(
        &self,
        job: TranscribeJob,
        progress: ProgressSink,
        cancel_rx: Option<tokio::sync::watch::Receiver<bool>>,
    ) -> Result<SubtitleTrack> {
        let language = job
            .language
            .as_deref()
            .unwrap_or("auto")
            .to_ascii_lowercase();
        if !matches!(language.as_str(), "auto" | "en" | "english") {
            return Err(FinalSubError::Validation(format!(
                "Parakeet v2 仅支持英文转录，当前语言：{language}"
            )));
        }
        if cancel_rx.as_ref().is_some_and(|rx| *rx.borrow()) {
            return Err(FinalSubError::Validation("任务已取消".into()));
        }

        let model_path = self
            .model_path(&job.model)
            .filter(|path| is_complete_mlx_snapshot(path))
            .ok_or_else(|| self.missing_model_error())?;
        let max_block_chars = match job.max_subtitle_chars {
            -1 => 140,
            value if (8..=120).contains(&value) => value,
            _ => 84,
        };

        progress
            .send(ProgressUpdate {
                progress: 0.05,
                message: "正在加载 Parakeet MLX V2（复用本地缓存）...".into(),
            })
            .await
            .ok();

        let mut command = tokio::process::Command::new(&self.uv_bin);
        command
            .args([
                "run",
                "--python",
                PARAKEET_MLX_PYTHON,
                "--with",
                PARAKEET_MLX_PACKAGE,
                "python",
            ])
            .arg(&self.transcribe_script)
            .args(["--audio", &job.audio_path, "--output", &job.output_path])
            .args(["--local-model-path", &model_path.to_string_lossy()])
            .args(["--source-language", &language])
            .args(["--max-block-chars", &max_block_chars.to_string()]);

        let hf_home = mlx_huggingface_cache(&self.cache_root);
        command
            .env("HF_HOME", &hf_home)
            .env("HF_HUB_CACHE", &hf_home)
            .env("HF_HUB_OFFLINE", "1")
            .env("TRANSFORMERS_OFFLINE", "1")
            .env("PYTHONUNBUFFERED", "1")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        if let Some(path) = path_with_ffmpeg(self.ffmpeg_path.as_deref()) {
            // Finder-launched apps receive a minimal PATH. Keep the bundled
            // sidecar ahead of it so parakeet_mlx.audio can resolve `ffmpeg`
            // through shutil.which without requiring a Homebrew installation.
            command.env("PATH", path);
        }

        progress
            .send(ProgressUpdate {
                progress: 0.12,
                message: "正在执行 Parakeet MLX V2...".into(),
            })
            .await
            .ok();

        let child = command.spawn().map_err(|error| {
            FinalSubError::Validation(format!("启动 Parakeet MLX 失败：{error}"))
        })?;
        let output = wait_for_process(child, cancel_rx).await?;
        if !output.status.success() {
            return Err(FinalSubError::Validation(format!(
                "Parakeet MLX 转录失败：{}",
                summarize_process_output(&output.stdout, &output.stderr)
            )));
        }

        progress
            .send(ProgressUpdate {
                progress: 0.9,
                message: "正在解析 Parakeet 字幕...".into(),
            })
            .await
            .ok();
        let srt = tokio::fs::read_to_string(&job.output_path)
            .await
            .map_err(|error| {
                FinalSubError::Validation(format!("读取 Parakeet SRT 失败：{error}"))
            })?;
        let track = SubtitleTrack::from_srt(&srt)?;
        if track.cues.is_empty() {
            return Err(FinalSubError::Validation(
                "Parakeet 未识别到字幕内容。该模型仅适用于英文语音；其他语言请切换 Whisper.cpp 或 SenseVoice。".into(),
            ));
        }

        progress
            .send(ProgressUpdate {
                progress: 1.0,
                message: format!("Parakeet MLX 转录完成，共 {} 条字幕", track.cues.len()),
            })
            .await
            .ok();
        Ok(track)
    }
}

async fn wait_for_process(
    child: tokio::process::Child,
    cancel_rx: Option<tokio::sync::watch::Receiver<bool>>,
) -> Result<std::process::Output> {
    let wait = child.wait_with_output();
    tokio::pin!(wait);

    if let Some(mut cancel_rx) = cancel_rx {
        loop {
            tokio::select! {
                output = &mut wait => {
                    return output.map_err(|error| FinalSubError::Validation(format!("读取 Parakeet MLX 进程输出失败：{error}")));
                }
                changed = cancel_rx.changed() => {
                    if changed.is_ok() && *cancel_rx.borrow() {
                        return Err(FinalSubError::Validation("任务已取消".into()));
                    }
                    if changed.is_err() {
                        return wait.await.map_err(|error| FinalSubError::Validation(format!("读取 Parakeet MLX 进程输出失败：{error}")));
                    }
                }
            }
        }
    }

    wait.await.map_err(|error| {
        FinalSubError::Validation(format!("读取 Parakeet MLX 进程输出失败：{error}"))
    })
}

fn summarize_process_output(stdout: &[u8], stderr: &[u8]) -> String {
    let stdout = String::from_utf8_lossy(stdout);
    let stderr = String::from_utf8_lossy(stderr);
    let combined = if stderr.trim().is_empty() {
        stdout.trim().to_string()
    } else if stdout.trim().is_empty() {
        stderr.trim().to_string()
    } else {
        format!("{}\n{}", stdout.trim(), stderr.trim())
    };
    let max_chars = 4_000;
    if combined.chars().count() <= max_chars {
        combined
    } else {
        let tail = combined
            .chars()
            .skip(combined.chars().count() - max_chars)
            .collect::<String>();
        format!("…{tail}")
    }
}

/// Select MLX on Apple Silicon when its local snapshot is complete; otherwise
/// retain the existing in-process Native sherpa-onnx implementation.
pub enum ParakeetEngine {
    Mlx(ParakeetMlxEngine),
    Native(ParakeetNativeEngine),
}

impl ParakeetEngine {
    pub fn preferred(
        models_dir: PathBuf,
        mlx_script: Option<PathBuf>,
        vad_model_path: PathBuf,
        ffmpeg_path: Option<PathBuf>,
    ) -> Self {
        let native_available =
            ParakeetNativeEngine::is_model_installed_at(&models_dir.join(PARAKEET_MODEL_ID));
        let mlx_available = mlx_runtime_supported() && is_mlx_model_installed_at(&models_dir);
        if mlx_available && (mlx_script.is_some() || !native_available) {
            return Self::Mlx(ParakeetMlxEngine::new(
                default_uv_bin(),
                mlx_script
                    .unwrap_or_else(|| PathBuf::from("resources/parakeet/parakeet_transcribe.py")),
                models_dir,
                ffmpeg_path,
            ));
        }
        Self::Native(ParakeetNativeEngine::new(models_dir, vad_model_path))
    }

    pub fn runtime_name(&self) -> &'static str {
        match self {
            Self::Mlx(_) => "mlx",
            Self::Native(_) => "native",
        }
    }
}

#[async_trait]
impl AsrEngine for ParakeetEngine {
    fn id(&self) -> &'static str {
        "parakeet-mlx"
    }

    fn capabilities(&self) -> AsrCapabilities {
        match self {
            Self::Mlx(engine) => engine.capabilities(),
            Self::Native(engine) => engine.capabilities(),
        }
    }

    async fn prepare(&self, model: &AsrModelRef) -> Result<()> {
        match self {
            Self::Mlx(engine) => engine.prepare(model).await,
            Self::Native(engine) => engine.prepare(model).await,
        }
    }

    async fn transcribe(
        &self,
        job: TranscribeJob,
        progress: ProgressSink,
        cancel_rx: Option<tokio::sync::watch::Receiver<bool>>,
    ) -> Result<SubtitleTrack> {
        match self {
            Self::Mlx(engine) => engine.transcribe(job, progress, cancel_rx).await,
            Self::Native(engine) => engine.transcribe(job, progress, cancel_rx).await,
        }
    }
}

/// In-process Parakeet fallback backed by the Rust sherpa-onnx binding.
pub struct ParakeetNativeEngine {
    models_dir: PathBuf,
    vad_model_path: PathBuf,
}

impl ParakeetNativeEngine {
    pub fn new(models_dir: PathBuf, vad_model_path: PathBuf) -> Self {
        Self {
            models_dir,
            vad_model_path,
        }
    }

    pub fn model_dir(&self, model: &AsrModelRef) -> PathBuf {
        model
            .model_path
            .as_deref()
            .filter(|path| !path.trim().is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| self.models_dir.join(PARAKEET_MODEL_ID))
    }

    pub fn is_model_installed_at(model_dir: &Path) -> bool {
        REQUIRED_MODEL_FILES
            .iter()
            .all(|name| model_dir.join(name).is_file())
    }
}

#[async_trait]
impl AsrEngine for ParakeetNativeEngine {
    fn id(&self) -> &'static str {
        "parakeet-mlx"
    }

    fn capabilities(&self) -> AsrCapabilities {
        AsrCapabilities {
            supports_streaming: false,
            supported_languages: vec!["en".into(), "auto".into()],
            requires_model_download: true,
        }
    }

    async fn prepare(&self, model: &AsrModelRef) -> Result<()> {
        let model_dir = self.model_dir(model);
        if !Self::is_model_installed_at(&model_dir) {
            return Err(FinalSubError::Validation(format!(
                "Parakeet 原生模型尚未安装。请先在模型管理下载 {}（目录应包含 encoder.int8.onnx、decoder.int8.onnx、joiner.int8.onnx 和 tokens.txt）：{}",
                PARAKEET_MODEL_ID,
                model_dir.display()
            )));
        }
        Ok(())
    }

    async fn transcribe(
        &self,
        job: TranscribeJob,
        progress: ProgressSink,
        cancel_rx: Option<tokio::sync::watch::Receiver<bool>>,
    ) -> Result<SubtitleTrack> {
        let language = job.language.as_deref().unwrap_or("auto");
        if !matches!(language, "auto" | "en" | "english") {
            return Err(FinalSubError::Validation(format!(
                "Parakeet v2 仅支持英文转录，当前语言：{language}"
            )));
        }
        if cancel_rx.as_ref().is_some_and(|rx| *rx.borrow()) {
            return Err(FinalSubError::Validation("任务已取消".into()));
        }

        let model_dir = self.model_dir(&job.model);
        if !Self::is_model_installed_at(&model_dir) {
            return Err(FinalSubError::Validation(format!(
                "Parakeet 模型文件不完整：{}",
                model_dir.display()
            )));
        }

        progress
            .send(ProgressUpdate {
                progress: 0.05,
                message: "正在加载 Parakeet 原生 ONNX 模型...".into(),
            })
            .await
            .ok();

        let audio_path = PathBuf::from(&job.audio_path);
        let track = super::parakeet_worker::transcribe_isolated(
            &model_dir,
            &self.vad_model_path,
            &audio_path,
            job.max_subtitle_chars,
            progress.clone(),
            cancel_rx,
        )
        .await?;

        progress
            .send(ProgressUpdate {
                progress: 0.98,
                message: "Parakeet 识别完成，正在生成时间轴...".into(),
            })
            .await
            .ok();

        if track.cues.is_empty() {
            return Err(FinalSubError::Validation(
                "Parakeet 未识别到字幕内容。该模型仅适用于英文语音；其他语言请切换 Whisper.cpp 或 SenseVoice。".into(),
            ));
        }

        progress
            .send(ProgressUpdate {
                progress: 1.0,
                message: format!("Parakeet 转录完成，共 {} 条字幕", track.cues.len()),
            })
            .await
            .ok();
        Ok(track)
    }
}

fn normalize_token(token: &str) -> String {
    token.replace('\u{2581}', " ")
}

fn is_content_token(token: &str) -> bool {
    let token = token.trim();
    !(token.is_empty() || token.starts_with('<') && token.ends_with('>'))
}

fn token_starts_word(token: &str) -> bool {
    token
        .chars()
        .next()
        .is_some_and(|character| character.is_whitespace() || character == '\u{2581}')
}

fn word_finishes_at(tokens: &[String], index: usize) -> bool {
    match tokens[index + 1..]
        .iter()
        .find(|token| is_content_token(token))
    {
        Some(next) => token_starts_word(next),
        None => true,
    }
}

fn should_break_for_pause(text: &str, gap_ms: u64) -> bool {
    const BRIEF_PAUSE_MS: u64 = 800;
    const LONG_PAUSE_MS: u64 = 1_500;
    const MIN_CUE_WIDTH: usize = 12;

    gap_ms >= LONG_PAUSE_MS
        || (gap_ms >= BRIEF_PAUSE_MS
            && crate::core::subtitle::subtitle_visual_width(&normalize_spaces(text))
                >= MIN_CUE_WIDTH)
}

fn normalize_spaces(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn ends_sentence(text: &str) -> bool {
    text.chars()
        .last()
        .is_some_and(|character| matches!(character, '.' | '?' | '!' | ';' | ':'))
}

fn push_cue(cues: &mut Vec<Cue>, text: &mut String, start_ms: u64, end_ms: u64) {
    let normalized = normalize_spaces(text);
    text.clear();
    if normalized.is_empty() {
        return;
    }
    let previous_end = cues.last().map(|cue| cue.end_ms).unwrap_or(0);
    let start_ms = start_ms.max(previous_end);
    let end_ms = end_ms.max(start_ms + 300);
    cues.push(Cue {
        index: (cues.len() + 1) as u32,
        start_ms,
        end_ms,
        text: normalized,
    });
}

pub(super) fn build_cues(
    full_text: &str,
    tokens: &[String],
    timestamps: Option<&[f32]>,
    duration_ms: u64,
    max_subtitle_chars: i32,
) -> Vec<Cue> {
    if let Some(timestamps) = timestamps {
        if !timestamps.is_empty() && timestamps.len() == tokens.len() {
            let mut cues = Vec::new();
            let mut text = String::new();
            let mut cue_start = 0;
            let mut previous_token_ms = 0;

            for (index, token) in tokens.iter().enumerate() {
                let token_content = token.trim();
                if !is_content_token(token) {
                    continue;
                }
                let starts_word = token_starts_word(token);
                let token_ms = (timestamps[index].max(0.0) * 1_000.0) as u64;
                let gap_ms = token_ms.saturating_sub(previous_token_ms);
                if !text.is_empty() && starts_word && should_break_for_pause(&text, gap_ms) {
                    push_cue(&mut cues, &mut text, cue_start, token_ms);
                    cue_start = token_ms;
                } else if text.is_empty() {
                    cue_start = token_ms;
                }

                let normalized_token = normalize_token(token);
                if starts_word && !normalize_spaces(&text).is_empty() {
                    let mut candidate = text.clone();
                    candidate.push_str(&normalized_token);
                    if crate::core::subtitle::exceeds_custom_subtitle_width(
                        &normalize_spaces(&candidate),
                        max_subtitle_chars,
                    ) {
                        push_cue(&mut cues, &mut text, cue_start, token_ms);
                        cue_start = token_ms;
                    }
                }
                text.push_str(&normalized_token);
                previous_token_ms = token_ms;
                let next_ms = timestamps
                    .get(index + 1)
                    .map(|next| (next.max(0.0) * 1_000.0) as u64)
                    .unwrap_or(duration_ms);
                let normalized_text = normalize_spaces(&text);
                let too_long = token_ms.saturating_sub(cue_start) >= 6_000
                    || crate::core::subtitle::should_break_for_width(
                        &normalized_text,
                        max_subtitle_chars,
                        normalized_text.chars().count() >= 84,
                    );
                if ends_sentence(token_content) || (too_long && word_finishes_at(tokens, index)) {
                    push_cue(&mut cues, &mut text, cue_start, next_ms);
                }
            }
            if !text.trim().is_empty() {
                push_cue(&mut cues, &mut text, cue_start, duration_ms);
            }
            if !cues.is_empty() {
                return cues;
            }
        }
    }

    build_even_cues(full_text, duration_ms, max_subtitle_chars)
}

fn build_even_cues(full_text: &str, duration_ms: u64, max_subtitle_chars: i32) -> Vec<Cue> {
    let normalized = normalize_spaces(full_text);
    if normalized.is_empty() {
        return Vec::new();
    }
    let mut blocks = Vec::new();
    let mut current = String::new();
    for word in normalized.split_whitespace() {
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(word);
        if ends_sentence(word)
            || crate::core::subtitle::should_break_for_width(
                &current,
                max_subtitle_chars,
                current.chars().count() >= 84,
            )
        {
            blocks.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        blocks.push(current);
    }

    let total_chars = blocks
        .iter()
        .map(|block| block.chars().count())
        .sum::<usize>();
    let mut cursor = 0;
    blocks
        .into_iter()
        .enumerate()
        .map(|(index, text)| {
            let share = if total_chars == 0 {
                1_000
            } else {
                (duration_ms * text.chars().count() as u64 / total_chars as u64).max(500)
            };
            let start_ms = cursor;
            let end_ms = (cursor + share).min(duration_ms.max(cursor + 500));
            cursor = end_ms;
            Cue {
                index: (index + 1) as u32,
                start_ms,
                end_ms,
                text,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model_ref() -> AsrModelRef {
        AsrModelRef {
            engine_id: "parakeet-mlx".into(),
            model_id: PARAKEET_MODEL_ID.into(),
            model_path: None,
        }
    }

    #[test]
    fn finds_complete_mlx_snapshot_without_network_access() {
        let temp = tempfile::tempdir().unwrap();
        let snapshot = temp
            .path()
            .join("huggingface")
            .join(PARAKEET_MLX_REPO_DIR)
            .join("snapshots")
            .join("0123456789abcdef");
        std::fs::create_dir_all(&snapshot).unwrap();
        std::fs::write(snapshot.join("config.json"), b"{}").unwrap();
        let weights = std::fs::File::create(snapshot.join("model.safetensors")).unwrap();
        weights.set_len(PARAKEET_MLX_MIN_WEIGHT_BYTES).unwrap();

        assert_eq!(find_mlx_model_snapshot(temp.path()), Some(snapshot));
        assert!(is_mlx_model_installed_at(temp.path()));
    }

    #[test]
    fn ignores_partial_mlx_snapshot() {
        let temp = tempfile::tempdir().unwrap();
        let snapshot = temp
            .path()
            .join("huggingface")
            .join(PARAKEET_MLX_REPO_DIR)
            .join("snapshots")
            .join("0123456789abcdef");
        std::fs::create_dir_all(&snapshot).unwrap();
        std::fs::write(snapshot.join("config.json"), b"{}").unwrap();
        std::fs::write(snapshot.join("model.safetensors"), b"partial").unwrap();

        assert!(find_mlx_model_snapshot(temp.path()).is_none());
    }

    #[test]
    fn preferred_engine_selects_mlx_for_a_complete_local_cache() {
        if !mlx_runtime_supported() {
            return;
        }
        let temp = tempfile::tempdir().unwrap();
        let snapshot = temp
            .path()
            .join("huggingface")
            .join(PARAKEET_MLX_REPO_DIR)
            .join("snapshots")
            .join("0123456789abcdef");
        std::fs::create_dir_all(&snapshot).unwrap();
        std::fs::write(snapshot.join("config.json"), b"{}").unwrap();
        let weights = std::fs::File::create(snapshot.join("model.safetensors")).unwrap();
        weights.set_len(PARAKEET_MLX_MIN_WEIGHT_BYTES).unwrap();
        let script = temp.path().join("parakeet_transcribe.py");
        std::fs::write(&script, b"# test").unwrap();

        let engine = ParakeetEngine::preferred(
            temp.path().to_path_buf(),
            Some(script),
            temp.path().join("vad.onnx"),
            None,
        );
        assert_eq!(engine.runtime_name(), "mlx");
    }

    #[test]
    fn bundled_ffmpeg_directory_is_prepended_without_dropping_inherited_path() {
        let inherited =
            std::env::join_paths([PathBuf::from("/usr/bin"), PathBuf::from("/bin")]).unwrap();
        let augmented = prepend_path(
            Path::new("/Applications/FinalSub.app/Contents/MacOS"),
            Some(&inherited),
        )
        .unwrap();
        let paths = std::env::split_paths(&augmented).collect::<Vec<_>>();

        assert_eq!(
            paths,
            vec![
                PathBuf::from("/Applications/FinalSub.app/Contents/MacOS"),
                PathBuf::from("/usr/bin"),
                PathBuf::from("/bin"),
            ]
        );
    }

    #[test]
    fn requires_all_native_model_files() {
        let temp = tempfile::tempdir().unwrap();
        let model_dir = temp.path().join(PARAKEET_MODEL_ID);
        std::fs::create_dir_all(&model_dir).unwrap();
        for name in REQUIRED_MODEL_FILES {
            std::fs::write(model_dir.join(name), b"test").unwrap();
        }
        let engine = ParakeetNativeEngine::new(
            temp.path().to_path_buf(),
            PathBuf::from("/vad/silero_vad.onnx"),
        );
        assert!(ParakeetNativeEngine::is_model_installed_at(
            &engine.model_dir(&model_ref())
        ));
    }

    #[test]
    fn capabilities_are_native_downloaded_and_english_only() {
        let engine = ParakeetNativeEngine::new(
            PathBuf::from("/models"),
            PathBuf::from("/vad/silero_vad.onnx"),
        );
        let capabilities = engine.capabilities();
        assert!(capabilities.requires_model_download);
        assert_eq!(capabilities.supported_languages, vec!["en", "auto"]);
    }

    #[test]
    fn token_timestamps_create_non_overlapping_sentence_cues() {
        let tokens = vec![
            "\u{2581}Hello".into(),
            "\u{2581}world.".into(),
            "\u{2581}Next".into(),
            "\u{2581}line!".into(),
        ];
        let timestamps = [0.0, 0.4, 1.2, 1.6];
        let cues = build_cues(
            "Hello world. Next line!",
            &tokens,
            Some(&timestamps),
            2_200,
            0,
        );
        assert_eq!(cues.len(), 2);
        assert_eq!(cues[0].text, "Hello world.");
        assert_eq!(cues[1].text, "Next line!");
        assert!(cues[0].end_ms <= cues[1].start_ms);
    }

    #[test]
    fn sherpa_decoded_leading_spaces_preserve_english_word_boundaries() {
        let tokens = vec![
            " I".into(),
            " belie".into(),
            "ve".into(),
            " the".into(),
            " ro".into(),
            "le".into(),
            ".".into(),
        ];
        let timestamps = [0.0, 0.2, 0.4, 0.7, 0.9, 1.1, 1.3];
        let cues = build_cues("I believe the role.", &tokens, Some(&timestamps), 1_600, -1);

        assert_eq!(cues.len(), 1);
        assert_eq!(cues[0].text, "I believe the role.");
    }

    #[test]
    fn smart_width_never_splits_an_english_word_between_cues() {
        let tokens = [
            " I",
            " wanted",
            " to",
            " start",
            " out",
            " by",
            " asking",
            " you,",
            " you",
            " are",
            " in",
            " charge",
            " of",
            " a",
            " lot",
            " more",
            " than",
            " just",
            " this",
            " comp",
            "any,",
            " but",
            " how",
            " important",
            " is",
            " memory?",
        ]
        .into_iter()
        .map(String::from)
        .collect::<Vec<_>>();
        let timestamps = (0..tokens.len())
            .map(|index| index as f32 * 0.1)
            .collect::<Vec<_>>();

        let cues = build_cues(
            "I wanted to start out by asking you, you are in charge of a lot more than just this company, but how important is memory?",
            &tokens,
            Some(&timestamps),
            3_000,
            0,
        );
        let text = cues.iter().map(|cue| cue.text.as_str()).collect::<Vec<_>>();

        assert_eq!(
            text,
            vec![
                "I wanted to start out by asking you, you are in charge of a lot more than just this company,",
                "but how important is memory?",
            ]
        );
    }

    #[test]
    fn duration_limit_waits_for_the_end_of_a_word() {
        let tokens = [
            " We",
            " keep",
            " every",
            " word",
            " together",
            " while",
            " the",
            " subtitle",
            " reaches",
            " comp",
            "any",
            " here.",
        ]
        .into_iter()
        .map(String::from)
        .collect::<Vec<_>>();
        let timestamps = [0.0, 0.7, 1.4, 2.1, 2.8, 3.5, 4.2, 4.9, 5.6, 6.0, 6.2, 6.4];

        let cues = build_cues(
            "We keep every word together while the subtitle reaches company here.",
            &tokens,
            Some(&timestamps),
            7_000,
            -1,
        );
        let text = cues.iter().map(|cue| cue.text.as_str()).collect::<Vec<_>>();

        assert_eq!(
            text,
            vec![
                "We keep every word together while the subtitle reaches company",
                "here.",
            ]
        );
    }

    #[test]
    fn timestamp_gap_inside_a_word_does_not_split_it() {
        let tokens = [" The", " comp", "any", " grows."]
            .into_iter()
            .map(String::from)
            .collect::<Vec<_>>();
        let timestamps = [0.0, 0.1, 1.2, 1.3];

        let cues = build_cues("The company grows.", &tokens, Some(&timestamps), 2_000, -1);

        assert_eq!(cues.len(), 1);
        assert_eq!(cues[0].text, "The company grows.");
    }

    #[test]
    fn short_lead_in_is_kept_with_the_following_phrase_across_a_brief_pause() {
        let tokens = [" So", " we", " continue."]
            .into_iter()
            .map(String::from)
            .collect::<Vec<_>>();
        let timestamps = [0.0, 1.0, 1.2];

        let cues = build_cues("So we continue.", &tokens, Some(&timestamps), 2_000, 0);

        assert_eq!(cues.len(), 1);
        assert_eq!(cues[0].text, "So we continue.");
    }

    #[test]
    fn long_pause_still_separates_a_short_lead_in() {
        let tokens = [" So", " we", " continue."]
            .into_iter()
            .map(String::from)
            .collect::<Vec<_>>();
        let timestamps = [0.0, 2.0, 2.2];

        let cues = build_cues("So we continue.", &tokens, Some(&timestamps), 3_000, 0);
        let text = cues.iter().map(|cue| cue.text.as_str()).collect::<Vec<_>>();

        assert_eq!(text, vec!["So", "we continue."]);
    }

    #[test]
    fn text_fallback_still_produces_subtitles() {
        let cues = build_cues("One sentence. Another sentence!", &[], None, 4_000, 0);
        assert_eq!(cues.len(), 2);
        assert_eq!(cues[0].index, 1);
        assert!(cues.iter().all(|cue| cue.end_ms > cue.start_ms));
    }

    #[test]
    fn custom_width_uses_token_timestamps_and_unlimited_disables_width_cut() {
        let tokens = vec![
            "\u{2581}Hello".into(),
            "\u{2581}world".into(),
            "\u{2581}again".into(),
        ];
        let timestamps = [0.0, 0.4, 0.8];
        let custom = build_cues("Hello world again", &tokens, Some(&timestamps), 1_600, 8);
        assert_eq!(custom.len(), 3);
        assert_eq!(custom[0].text, "Hello");
        assert_eq!(custom[0].end_ms, 400);
        assert!(custom
            .iter()
            .all(|cue| crate::core::subtitle::subtitle_visual_width(&cue.text) <= 8));

        let unlimited = build_cues("Hello world again", &tokens, Some(&timestamps), 1_600, -1);
        assert_eq!(unlimited.len(), 1);
    }
}
