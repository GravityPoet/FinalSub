use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sherpa_onnx::Wave;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crate::core::subtitle::{Cue, SubtitleTrack};
use crate::error::{FinalSubError, Result};

const SESSION_VERSION: u32 = 1;
const MAX_SUBTITLE_BYTES: u64 = 20 * 1024 * 1024;
const MAX_CUES: usize = 2_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DubbingCueStatus {
    Pending,
    Synthesizing,
    Ready,
    Overlong,
    Accepted,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum DubbingEngineSelection {
    Local { model_id: String },
    Cloud { provider_id: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DubbingRunConfig {
    pub engine: DubbingEngineSelection,
    pub voice: String,
    pub global_speed: f32,
    pub reference_audio_path: Option<String>,
    pub reference_text: Option<String>,
    pub num_steps: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DubbingCue {
    pub index: u32,
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
    pub status: DubbingCueStatus,
    pub overlap: bool,
    pub voice_id: Option<String>,
    pub synthesized_ms: Option<u64>,
    pub applied_speed: Option<f32>,
    pub slot_ms: u64,
    pub ratio: Option<f32>,
    pub wav_path: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DubbingSession {
    pub version: u32,
    pub id: String,
    pub subtitle_path: String,
    pub subtitle_hash: String,
    pub video_path: Option<String>,
    pub cues: Vec<DubbingCue>,
    pub last_config: Option<DubbingRunConfig>,
    pub output_path: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default)]
    pub source_changed: bool,
    #[serde(default)]
    pub subtitle_dirty: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct DubbingSubtitleWriteResult {
    pub session: DubbingSession,
    pub backup_path: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DubbingSynthesizeCueRequest {
    pub session_id: String,
    pub cue_index: u32,
    pub engine: DubbingEngineSelection,
    pub voice: String,
    pub global_speed: f32,
    pub reference_audio_path: Option<String>,
    pub reference_text: Option<String>,
    pub num_steps: Option<i32>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateDubbingCueRequest {
    pub session_id: String,
    pub cue_index: u32,
    pub text: Option<String>,
    pub voice_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PreparedDubbingCue {
    pub session_id: String,
    pub cue_index: u32,
    pub text: String,
    pub output_path: String,
    pub config: DubbingRunConfig,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct AlignmentDecision {
    slot_ms: u64,
    ratio: f32,
    atempo: Option<f32>,
    overlong: bool,
    overlap: bool,
    pad_ms: u64,
}

fn sessions_root(app_config_dir: &Path) -> PathBuf {
    app_config_dir.join("tts").join("dubbing-sessions")
}

fn validate_session_id(session_id: &str) -> Result<()> {
    uuid::Uuid::parse_str(session_id)
        .map(|_| ())
        .map_err(|_| FinalSubError::Validation("配音会话 ID 无效".into()))
}

fn session_dir(app_config_dir: &Path, session_id: &str) -> Result<PathBuf> {
    validate_session_id(session_id)?;
    Ok(sessions_root(app_config_dir).join(session_id))
}

fn session_path(app_config_dir: &Path, session_id: &str) -> Result<PathBuf> {
    Ok(session_dir(app_config_dir, session_id)?.join("session.json"))
}

fn save_session(app_config_dir: &Path, session: &DubbingSession) -> Result<()> {
    let path = session_path(app_config_dir, &session.id)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension("json.tmp");
    std::fs::write(&temporary, serde_json::to_string_pretty(session)?)?;
    std::fs::rename(&temporary, path)?;
    Ok(())
}

fn hash_bytes(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn subtitle_format(path: &Path) -> Result<String> {
    let format = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    match format.as_str() {
        "srt" | "vtt" | "ass" | "lrc" => Ok(format),
        "ssa" => Ok("ass".into()),
        _ => Err(FinalSubError::Validation(format!(
            "不支持写出的字幕格式：{}",
            path.display()
        ))),
    }
}

fn track_differs_from_session(track: &SubtitleTrack, session: &DubbingSession) -> bool {
    track.cues.len() != session.cues.len()
        || track
            .cues
            .iter()
            .zip(&session.cues)
            .any(|(source, edited)| {
                source.start_ms != edited.start_ms
                    || source.end_ms != edited.end_ms
                    || source.text != edited.text
            })
}

fn refresh_source_state(session: &mut DubbingSession) {
    match read_subtitle(Path::new(&session.subtitle_path)) {
        Ok((bytes, track)) => {
            session.source_changed = hash_bytes(&bytes) != session.subtitle_hash;
            if !session.source_changed {
                session.subtitle_dirty = track_differs_from_session(&track, session);
            }
        }
        Err(_) => {
            session.source_changed = true;
        }
    }
}

fn session_subtitle_track(session: &DubbingSession) -> SubtitleTrack {
    SubtitleTrack {
        cues: session
            .cues
            .iter()
            .enumerate()
            .map(|(position, cue)| Cue {
                index: (position + 1) as u32,
                start_ms: cue.start_ms,
                end_ms: cue.end_ms,
                text: cue.text.clone(),
            })
            .collect(),
    }
}

fn rendered_track_matches_session(
    track: &SubtitleTrack,
    session: &DubbingSession,
    format: &str,
) -> bool {
    if format == "lrc" {
        return track.cues.len() == session.cues.len()
            && track
                .cues
                .iter()
                .zip(&session.cues)
                .all(|(rendered, edited)| {
                    rendered.text == edited.text
                        && rendered.start_ms.abs_diff(edited.start_ms) <= 10
                        && rendered.end_ms.abs_diff(edited.end_ms) <= 10
                });
    }
    !track_differs_from_session(track, session)
}

fn render_session_subtitle(session: &DubbingSession, output_path: &Path) -> Result<Vec<u8>> {
    let format = subtitle_format(output_path)?;
    if format == "lrc" && session.cues.iter().any(|cue| cue.text.contains('\n')) {
        return Err(FinalSubError::Validation(
            "LRC 字幕不支持多行文本，请先把本行改成单行后再保存".into(),
        ));
    }
    let rendered = session_subtitle_track(session).to_format(&format)?;
    if rendered.len() as u64 > MAX_SUBTITLE_BYTES {
        return Err(FinalSubError::Validation(
            "编辑后的字幕超过 20 MB 限制".into(),
        ));
    }
    let reparsed = SubtitleTrack::from_format(&rendered, &format)?;
    if !rendered_track_matches_session(&reparsed, session, &format) {
        return Err(FinalSubError::Validation(
            "字幕写出校验失败，已停止保存且未改动源文件".into(),
        ));
    }
    Ok(rendered.into_bytes())
}

fn write_new_synced_file(
    path: &Path,
    bytes: &[u8],
    permissions: Option<std::fs::Permissions>,
) -> Result<()> {
    let mut file = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)?;
    if let Err(error) = file.write_all(bytes).and_then(|_| file.sync_all()) {
        drop(file);
        let _ = std::fs::remove_file(path);
        return Err(error.into());
    }
    drop(file);
    if let Some(permissions) = permissions {
        if let Err(error) = std::fs::set_permissions(path, permissions) {
            let _ = std::fs::remove_file(path);
            return Err(error.into());
        }
    }
    Ok(())
}

fn temporary_sibling(path: &Path, purpose: &str) -> Result<PathBuf> {
    let parent = path
        .parent()
        .filter(|parent| parent.is_dir())
        .ok_or_else(|| FinalSubError::Validation("字幕文件所在目录不存在".into()))?;
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| FinalSubError::Validation("字幕文件名无效".into()))?;
    Ok(parent.join(format!(
        ".{file_name}.finalsub-{purpose}-{}",
        uuid::Uuid::new_v4()
    )))
}

fn replace_existing_file(
    path: &Path,
    bytes: &[u8],
    permissions: std::fs::Permissions,
) -> Result<()> {
    let temporary = temporary_sibling(path, "writing")?;
    write_new_synced_file(&temporary, bytes, Some(permissions))?;

    #[cfg(not(windows))]
    {
        if let Err(error) = std::fs::rename(&temporary, path) {
            let _ = std::fs::remove_file(&temporary);
            return Err(error.into());
        }
    }

    #[cfg(windows)]
    {
        let displaced = temporary_sibling(path, "rollback")?;
        if let Err(error) = std::fs::rename(path, &displaced) {
            let _ = std::fs::remove_file(&temporary);
            return Err(error.into());
        }
        if let Err(error) = std::fs::rename(&temporary, path) {
            let restore_error = std::fs::rename(&displaced, path).err();
            let _ = std::fs::remove_file(&temporary);
            return Err(FinalSubError::Validation(match restore_error {
                Some(restore_error) => {
                    format!("替换字幕失败：{error}；自动恢复也失败：{restore_error}")
                }
                None => format!("替换字幕失败，原文件已自动恢复：{error}"),
            }));
        }
        let _ = std::fs::remove_file(displaced);
    }

    Ok(())
}

fn backup_path_for(source: &Path) -> Result<PathBuf> {
    let parent = source
        .parent()
        .filter(|parent| parent.is_dir())
        .ok_or_else(|| FinalSubError::Validation("字幕文件所在目录不存在".into()))?;
    let stem = source
        .file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or("subtitle");
    let extension = source
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("srt");
    let timestamp = chrono::Utc::now().format("%Y%m%d-%H%M%S");
    let unique = uuid::Uuid::new_v4().simple().to_string();
    Ok(parent.join(format!(
        "{stem}.finalsub-backup-{timestamp}-{}.{}",
        &unique[..8],
        extension
    )))
}

fn read_subtitle(path: &Path) -> Result<(Vec<u8>, SubtitleTrack)> {
    if !path.is_absolute() || !path.is_file() {
        return Err(FinalSubError::Validation(format!(
            "字幕文件不存在或不是绝对路径：{}",
            path.display()
        )));
    }
    let metadata = std::fs::metadata(path)?;
    if metadata.len() > MAX_SUBTITLE_BYTES {
        return Err(FinalSubError::Validation("字幕文件超过 20 MB 限制".into()));
    }
    let bytes = std::fs::read(path)?;
    let content = String::from_utf8(bytes.clone())
        .map_err(|_| FinalSubError::Validation("字幕文件必须是 UTF-8 编码".into()))?;
    let format = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("");
    let track = SubtitleTrack::from_format(&content, format)?;
    if track.cues.len() > MAX_CUES {
        return Err(FinalSubError::Validation(format!(
            "单个配音会话不能超过 {MAX_CUES} 条字幕"
        )));
    }
    Ok((bytes, track))
}

fn build_cues(track: SubtitleTrack) -> Vec<DubbingCue> {
    let source = track.cues;
    source
        .iter()
        .enumerate()
        .map(|(position, cue)| {
            let next_start = source.get(position + 1).map(|next| next.start_ms);
            let overlap = next_start.is_some_and(|start| start < cue.end_ms);
            let slot_ms = cue_slot_ms(cue.start_ms, cue.end_ms, next_start);
            DubbingCue {
                index: position as u32,
                start_ms: cue.start_ms,
                end_ms: cue.end_ms,
                text: cue.text.clone(),
                status: DubbingCueStatus::Pending,
                overlap,
                voice_id: None,
                synthesized_ms: None,
                applied_speed: None,
                slot_ms,
                ratio: None,
                wav_path: None,
                error: None,
            }
        })
        .collect()
}

pub fn create_dubbing_session(
    app_config_dir: &Path,
    subtitle_path: &str,
    video_path: Option<&str>,
) -> Result<DubbingSession> {
    let subtitle = PathBuf::from(subtitle_path.trim());
    let (bytes, track) = read_subtitle(&subtitle)?;
    let video = video_path
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from);
    if let Some(video) = &video {
        if !video.is_absolute() || !video.is_file() {
            return Err(FinalSubError::Validation(format!(
                "视频文件不存在或不是绝对路径：{}",
                video.display()
            )));
        }
    }
    let now = chrono::Utc::now().to_rfc3339();
    let session = DubbingSession {
        version: SESSION_VERSION,
        id: uuid::Uuid::new_v4().to_string(),
        subtitle_path: subtitle.to_string_lossy().to_string(),
        subtitle_hash: hash_bytes(&bytes),
        video_path: video.map(|path| path.to_string_lossy().to_string()),
        cues: build_cues(track),
        last_config: None,
        output_path: None,
        created_at: now.clone(),
        updated_at: now,
        source_changed: false,
        subtitle_dirty: false,
    };
    let cues_dir = session_dir(app_config_dir, &session.id)?.join("cues");
    std::fs::create_dir_all(cues_dir)?;
    save_session(app_config_dir, &session)?;
    Ok(session)
}

fn load_session_raw_with_recovery(
    app_config_dir: &Path,
    session_id: &str,
    recover_incomplete: bool,
) -> Result<DubbingSession> {
    let path = session_path(app_config_dir, session_id)?;
    if !path.is_file() {
        return Err(FinalSubError::Validation("配音会话不存在".into()));
    }
    let metadata = std::fs::metadata(&path)?;
    if metadata.len() > MAX_SUBTITLE_BYTES {
        return Err(FinalSubError::Validation("配音会话文件异常过大".into()));
    }
    let mut session: DubbingSession = serde_json::from_str(&std::fs::read_to_string(path)?)?;
    if session.version != SESSION_VERSION || session.id != session_id {
        return Err(FinalSubError::Validation("配音会话版本或标识无效".into()));
    }
    if recover_incomplete {
        for cue in &mut session.cues {
            if cue.status == DubbingCueStatus::Synthesizing {
                cue.status = DubbingCueStatus::Pending;
                cue.error = None;
            }
            if cue
                .wav_path
                .as_deref()
                .is_some_and(|path| !Path::new(path).is_file())
            {
                cue.status = DubbingCueStatus::Pending;
                cue.wav_path = None;
                cue.synthesized_ms = None;
                cue.applied_speed = None;
                cue.ratio = None;
            }
        }
    }
    Ok(session)
}

fn load_session_raw(app_config_dir: &Path, session_id: &str) -> Result<DubbingSession> {
    load_session_raw_with_recovery(app_config_dir, session_id, true)
}

fn load_session_for_subtitle_write(
    app_config_dir: &Path,
    session_id: &str,
) -> Result<DubbingSession> {
    load_session_raw_with_recovery(app_config_dir, session_id, false)
}

pub fn get_dubbing_session(app_config_dir: &Path, session_id: &str) -> Result<DubbingSession> {
    let mut session = load_session_raw(app_config_dir, session_id)?;
    refresh_source_state(&mut session);
    Ok(session)
}

fn validate_run_config(request: &DubbingSynthesizeCueRequest) -> Result<DubbingRunConfig> {
    if request.voice.trim().len() > 200 || request.voice.chars().any(char::is_control) {
        return Err(FinalSubError::Validation("配音音色 ID 无效".into()));
    }
    if !request.global_speed.is_finite() || !(0.5..=2.0).contains(&request.global_speed) {
        return Err(FinalSubError::Validation(
            "工作台整体语速必须在 0.5-2.0 之间".into(),
        ));
    }
    match &request.engine {
        DubbingEngineSelection::Local { model_id } => {
            if model_id.trim().is_empty() || model_id.chars().any(char::is_control) {
                return Err(FinalSubError::Validation("本地 TTS 模型 ID 无效".into()));
            }
        }
        DubbingEngineSelection::Cloud { provider_id } => {
            uuid::Uuid::parse_str(provider_id)
                .map_err(|_| FinalSubError::Validation("在线 TTS 实例 ID 无效".into()))?;
        }
    }
    Ok(DubbingRunConfig {
        engine: request.engine.clone(),
        voice: request.voice.trim().to_string(),
        global_speed: request.global_speed,
        reference_audio_path: request.reference_audio_path.clone(),
        reference_text: request.reference_text.clone(),
        num_steps: request.num_steps,
    })
}

fn validate_cue_text(value: &str) -> Result<String> {
    let text = value.trim();
    if text.is_empty() || text.len() > MAX_SUBTITLE_BYTES as usize || text.contains('\0') {
        return Err(FinalSubError::Validation(
            "字幕行文本不能为空、不能包含空字符，且不能超过 20 MB".into(),
        ));
    }
    Ok(text.to_string())
}

fn validate_cue_voice(value: &str) -> Result<Option<String>> {
    let voice = value.trim();
    if voice.is_empty() {
        return Ok(None);
    }
    if voice.len() > 200 || voice.chars().any(char::is_control) {
        return Err(FinalSubError::Validation("字幕行音色 ID 无效".into()));
    }
    Ok(Some(voice.to_string()))
}

pub fn update_dubbing_cue(
    app_config_dir: &Path,
    request: UpdateDubbingCueRequest,
) -> Result<DubbingSession> {
    let mut session = load_session_raw(app_config_dir, &request.session_id)?;
    let cue_position = request.cue_index as usize;
    let next_text = request.text.as_deref().map(validate_cue_text).transpose()?;
    let next_voice = request
        .voice_id
        .as_deref()
        .map(validate_cue_voice)
        .transpose()?
        .flatten();
    let cue_file_number = session
        .cues
        .get(cue_position)
        .ok_or_else(|| FinalSubError::Validation("配音字幕行不存在".into()))?
        .index
        .checked_add(1)
        .ok_or_else(|| FinalSubError::Validation("配音字幕行索引无效".into()))?;
    let cues_dir = session_dir(app_config_dir, &session.id)?.join("cues");
    let expected_wav = cues_dir.join(format!("{cue_file_number:05}.wav"));
    let cue = session
        .cues
        .get_mut(cue_position)
        .ok_or_else(|| FinalSubError::Validation("配音字幕行不存在".into()))?;
    if cue.status == DubbingCueStatus::Synthesizing {
        return Err(FinalSubError::Validation(
            "当前字幕行正在合成，请稍后再编辑".into(),
        ));
    }
    let text_changed = next_text.as_ref().is_some_and(|text| text != &cue.text);
    let changed = text_changed || request.voice_id.is_some() && next_voice != cue.voice_id;
    if !changed {
        return Ok(session);
    }
    if expected_wav.is_file() {
        std::fs::remove_file(&expected_wav)?;
    }
    if let Some(text) = next_text {
        cue.text = text;
    }
    if request.voice_id.is_some() {
        cue.voice_id = next_voice;
    }
    cue.status = DubbingCueStatus::Pending;
    cue.synthesized_ms = None;
    cue.applied_speed = None;
    cue.ratio = None;
    cue.wav_path = None;
    cue.error = None;
    session.output_path = None;
    if text_changed {
        session.subtitle_dirty = true;
    }
    refresh_source_state(&mut session);
    session.updated_at = chrono::Utc::now().to_rfc3339();
    save_session(app_config_dir, &session)?;
    Ok(session)
}

pub fn export_dubbing_subtitle(
    app_config_dir: &Path,
    session_id: &str,
    output_path: &str,
) -> Result<String> {
    let session = load_session_for_subtitle_write(app_config_dir, session_id)?;
    if session
        .cues
        .iter()
        .any(|cue| cue.status == DubbingCueStatus::Synthesizing)
    {
        return Err(FinalSubError::Validation(
            "字幕行正在合成，请完成或取消后再导出字幕".into(),
        ));
    }
    let output = PathBuf::from(output_path.trim());
    if !output.is_absolute() {
        return Err(FinalSubError::Validation(
            "字幕副本必须保存到绝对路径".into(),
        ));
    }
    let source_identity = std::fs::canonicalize(&session.subtitle_path)?;
    let output_parent = output
        .parent()
        .filter(|parent| parent.is_dir())
        .ok_or_else(|| FinalSubError::Validation("字幕副本所在目录不存在".into()))?;
    let output_name = output
        .file_name()
        .ok_or_else(|| FinalSubError::Validation("字幕副本文件名无效".into()))?;
    let output_identity = std::fs::canonicalize(output_parent)?.join(output_name);
    if source_identity == output_identity {
        return Err(FinalSubError::Validation(
            "不能用“导出副本”覆盖源字幕，请使用安全写回".into(),
        ));
    }
    if output.exists() {
        return Err(FinalSubError::Validation(
            "字幕副本已存在，请选择新的文件名".into(),
        ));
    }
    let rendered = render_session_subtitle(&session, &output)?;
    write_new_synced_file(&output, &rendered, None)?;
    Ok(output.to_string_lossy().to_string())
}

pub fn write_back_dubbing_subtitle(
    app_config_dir: &Path,
    session_id: &str,
) -> Result<DubbingSubtitleWriteResult> {
    let mut session = load_session_for_subtitle_write(app_config_dir, session_id)?;
    if session
        .cues
        .iter()
        .any(|cue| cue.status == DubbingCueStatus::Synthesizing)
    {
        return Err(FinalSubError::Validation(
            "字幕行正在合成，请完成或取消后再写回字幕".into(),
        ));
    }
    let source = std::fs::canonicalize(&session.subtitle_path)?;
    let metadata = std::fs::metadata(&source)?;
    let permissions = metadata.permissions();
    let (source_bytes, source_track) = read_subtitle(&source)?;
    if hash_bytes(&source_bytes) != session.subtitle_hash {
        return Err(FinalSubError::Validation(
            "源字幕已在会话外变化，已停止写回且没有覆盖任何文件".into(),
        ));
    }
    if source_track.cues.len() != session.cues.len()
        || source_track
            .cues
            .iter()
            .zip(&session.cues)
            .any(|(original, edited)| {
                original.start_ms != edited.start_ms || original.end_ms != edited.end_ms
            })
    {
        return Err(FinalSubError::Validation(
            "源字幕时间轴与当前会话不一致，已停止写回".into(),
        ));
    }
    if !source_track
        .cues
        .iter()
        .zip(&session.cues)
        .any(|(original, edited)| original.text != edited.text)
    {
        session.subtitle_dirty = false;
        save_session(app_config_dir, &session)?;
        return Err(FinalSubError::Validation(
            "字幕文本没有变化，无需写回源文件".into(),
        ));
    }

    let rendered = render_session_subtitle(&session, &source)?;
    let backup = backup_path_for(&source)?;
    write_new_synced_file(&backup, &source_bytes, Some(permissions.clone()))?;

    let source_still_matches = std::fs::read(&source)
        .map(|bytes| bytes == source_bytes)
        .unwrap_or(false);
    if !source_still_matches {
        let _ = std::fs::remove_file(&backup);
        return Err(FinalSubError::Validation(
            "源字幕刚刚被其它程序修改，已停止写回且没有覆盖任何文件".into(),
        ));
    }

    replace_existing_file(&source, &rendered, permissions.clone())?;
    session.subtitle_hash = hash_bytes(&rendered);
    session.subtitle_dirty = false;
    session.source_changed = false;
    session.updated_at = chrono::Utc::now().to_rfc3339();
    if let Err(save_error) = save_session(app_config_dir, &session) {
        return match replace_existing_file(&source, &source_bytes, permissions) {
            Ok(()) => Err(FinalSubError::Validation(format!(
                "会话状态保存失败，源字幕已自动恢复：{save_error}"
            ))),
            Err(restore_error) => Err(FinalSubError::Validation(format!(
                "会话状态保存失败且源字幕自动恢复失败：{save_error}；{restore_error}。原始备份位于 {}",
                backup.display()
            ))),
        };
    }

    Ok(DubbingSubtitleWriteResult {
        session,
        backup_path: backup.to_string_lossy().to_string(),
    })
}

pub fn prepare_dubbing_cue(
    app_config_dir: &Path,
    request: &DubbingSynthesizeCueRequest,
) -> Result<PreparedDubbingCue> {
    let config = validate_run_config(request)?;
    let mut session = load_session_raw(app_config_dir, &request.session_id)?;
    let cue = session
        .cues
        .get_mut(request.cue_index as usize)
        .ok_or_else(|| FinalSubError::Validation("配音字幕行不存在".into()))?;
    if cue.text.trim().is_empty() {
        return Err(FinalSubError::Validation("配音字幕行文本为空".into()));
    }
    cue.status = DubbingCueStatus::Synthesizing;
    cue.error = None;
    cue.voice_id = (!config.voice.is_empty()).then(|| config.voice.clone());
    let output = session_dir(app_config_dir, &session.id)?
        .join("cues")
        .join(format!("{:05}.wav", cue.index + 1));
    session.last_config = Some(config.clone());
    session.updated_at = chrono::Utc::now().to_rfc3339();
    let text = cue.text.clone();
    save_session(app_config_dir, &session)?;
    Ok(PreparedDubbingCue {
        session_id: request.session_id.clone(),
        cue_index: request.cue_index,
        text,
        output_path: output.to_string_lossy().to_string(),
        config,
    })
}

fn alignment_decision(
    cue: &DubbingCue,
    next_start: Option<u64>,
    duration_ms: u64,
) -> AlignmentDecision {
    let slot_ms = cue_slot_ms(cue.start_ms, cue.end_ms, next_start);
    let overlap = next_start.is_some_and(|start| start < cue.end_ms);
    let ratio = duration_ms as f32 / slot_ms.max(1) as f32;
    let overlong = ratio > 1.5;
    let atempo = (!overlong && ratio > 1.005).then_some(ratio);
    let adjusted = atempo
        .map(|factor| (duration_ms as f32 / factor).round() as u64)
        .unwrap_or(duration_ms);
    AlignmentDecision {
        slot_ms,
        ratio,
        atempo,
        overlong,
        overlap,
        pad_ms: slot_ms.saturating_sub(adjusted),
    }
}

fn cue_slot_ms(start_ms: u64, end_ms: u64, next_start: Option<u64>) -> u64 {
    // 非重叠字幕可借用到下一句开始前的静音；原字幕本身重叠时保留各自的时间窗。
    next_start
        .filter(|next| *next >= end_ms)
        .map(|next| next.saturating_sub(start_ms).max(1))
        .unwrap_or_else(|| end_ms.saturating_sub(start_ms).max(1))
}

fn atempo_filter(mut factor: f32) -> Result<String> {
    if !factor.is_finite() || factor <= 0.0 {
        return Err(FinalSubError::Validation("配音变速倍率无效".into()));
    }
    let mut filters = Vec::new();
    while factor > 2.0 {
        filters.push("atempo=2.0".to_string());
        factor /= 2.0;
    }
    while factor < 0.5 {
        filters.push("atempo=0.5".to_string());
        factor /= 0.5;
    }
    filters.push(format!("atempo={factor:.6}"));
    Ok(filters.join(","))
}

async fn wait_cancelled(cancelled: Arc<AtomicBool>) {
    while !cancelled.load(Ordering::Relaxed) {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

async fn apply_atempo(
    ffmpeg_path: &Path,
    wav_path: &Path,
    factor: f32,
    cancelled: Arc<AtomicBool>,
) -> Result<u64> {
    let temporary = wav_path.with_extension("aligned.wav");
    let mut child = tokio::process::Command::new(ffmpeg_path)
        .args(["-hide_banner", "-loglevel", "error", "-y", "-i"])
        .arg(wav_path)
        .args([
            "-filter:a",
            &atempo_filter(factor)?,
            "-ac",
            "1",
            "-c:a",
            "pcm_s16le",
            "-f",
            "wav",
        ])
        .arg(&temporary)
        .kill_on_drop(true)
        .spawn()
        .map_err(|error| FinalSubError::Validation(format!("无法启动 FFmpeg 配音对齐：{error}")))?;
    let status = tokio::select! {
        result = child.wait() => result?,
        _ = wait_cancelled(cancelled) => {
            let _ = child.kill().await;
            let _ = tokio::fs::remove_file(&temporary).await;
            return Err(FinalSubError::Validation("配音已取消".into()));
        }
    };
    if !status.success() {
        let _ = tokio::fs::remove_file(&temporary).await;
        return Err(FinalSubError::Validation("配音时间轴变速失败".into()));
    }
    // 同目录 rename 在 macOS 上原子替换旧 WAV，失败时不会先丢失原产物。
    tokio::fs::rename(&temporary, wav_path).await?;
    wav_duration(wav_path)
}

fn wav_duration(path: &Path) -> Result<u64> {
    let wave = Wave::read(&path.to_string_lossy())
        .ok_or_else(|| FinalSubError::Validation("无法读取配音 WAV".into()))?;
    if wave.sample_rate() <= 0 || wave.samples().is_empty() {
        return Err(FinalSubError::Validation("配音 WAV 为空".into()));
    }
    Ok((wave.samples().len() as u128 * 1000 / wave.sample_rate() as u128) as u64)
}

pub async fn complete_dubbing_cue(
    app_config_dir: &Path,
    ffmpeg_path: &Path,
    prepared: &PreparedDubbingCue,
    synthesized_ms: u64,
    cancelled: Arc<AtomicBool>,
) -> Result<DubbingSession> {
    let mut session = load_session_raw(app_config_dir, &prepared.session_id)?;
    let position = prepared.cue_index as usize;
    let next_start = session.cues.get(position + 1).map(|cue| cue.start_ms);
    let decision = {
        let cue = session
            .cues
            .get(position)
            .ok_or_else(|| FinalSubError::Validation("配音字幕行不存在".into()))?;
        alignment_decision(cue, next_start, synthesized_ms)
    };
    let wav = PathBuf::from(&prepared.output_path);
    let final_duration = if let Some(factor) = decision.atempo {
        apply_atempo(ffmpeg_path, &wav, factor, cancelled).await?
    } else {
        synthesized_ms
    };
    let cue = session
        .cues
        .get_mut(position)
        .ok_or_else(|| FinalSubError::Validation("配音字幕行不存在".into()))?;
    cue.status = if decision.overlong {
        DubbingCueStatus::Overlong
    } else {
        DubbingCueStatus::Ready
    };
    cue.overlap = decision.overlap;
    cue.slot_ms = decision.slot_ms;
    cue.ratio = Some(decision.ratio);
    cue.synthesized_ms = Some(final_duration);
    cue.applied_speed = Some(prepared.config.global_speed * decision.atempo.unwrap_or(1.0));
    cue.wav_path = Some(prepared.output_path.clone());
    cue.error = None;
    session.updated_at = chrono::Utc::now().to_rfc3339();
    save_session(app_config_dir, &session)?;
    Ok(session)
}

pub fn fail_dubbing_cue(
    app_config_dir: &Path,
    session_id: &str,
    cue_index: u32,
    error: &str,
    cancelled: bool,
) -> Result<DubbingSession> {
    let mut session = load_session_raw(app_config_dir, session_id)?;
    let cue = session
        .cues
        .get_mut(cue_index as usize)
        .ok_or_else(|| FinalSubError::Validation("配音字幕行不存在".into()))?;
    cue.status = if cancelled {
        DubbingCueStatus::Pending
    } else {
        DubbingCueStatus::Failed
    };
    cue.error = (!cancelled).then(|| {
        error
            .chars()
            .filter(|ch| !ch.is_control() || matches!(*ch, '\n' | '\t'))
            .take(1_000)
            .collect()
    });
    session.updated_at = chrono::Utc::now().to_rfc3339();
    save_session(app_config_dir, &session)?;
    Ok(session)
}

pub async fn accept_dubbing_overflow(
    app_config_dir: &Path,
    ffmpeg_path: &Path,
    session_id: &str,
    cue_index: u32,
    cancelled: Arc<AtomicBool>,
) -> Result<DubbingSession> {
    let mut session = load_session_raw(app_config_dir, session_id)?;
    let cue = session
        .cues
        .get_mut(cue_index as usize)
        .ok_or_else(|| FinalSubError::Validation("配音字幕行不存在".into()))?;
    if cue.status != DubbingCueStatus::Overlong {
        return Err(FinalSubError::Validation("该字幕行不需要接受超速".into()));
    }
    let factor = cue.ratio.unwrap_or(1.0);
    let wav_path = cue
        .wav_path
        .as_deref()
        .map(PathBuf::from)
        .ok_or_else(|| FinalSubError::Validation("该字幕行没有可处理的音频".into()))?;
    let duration = apply_atempo(ffmpeg_path, &wav_path, factor, cancelled).await?;
    cue.status = DubbingCueStatus::Accepted;
    cue.synthesized_ms = Some(duration);
    cue.applied_speed = Some(cue.applied_speed.unwrap_or(1.0) * factor);
    cue.error = None;
    session.updated_at = chrono::Utc::now().to_rfc3339();
    save_session(app_config_dir, &session)?;
    Ok(session)
}

fn validate_export_path(
    output_path: &str,
) -> Result<(PathBuf, PathBuf, &'static str, &'static str)> {
    let output = PathBuf::from(output_path.trim());
    if !output.is_absolute() {
        return Err(FinalSubError::Validation(
            "配音导出路径必须是绝对路径".into(),
        ));
    }
    let extension = output
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    let (codec, format) = match extension.as_str() {
        "wav" => ("pcm_s16le", "wav"),
        "mp3" => ("libmp3lame", "mp3"),
        _ => {
            return Err(FinalSubError::Validation(
                "配音导出只支持 WAV 或 MP3".into(),
            ))
        }
    };
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temporary = output.with_extension(format!("{extension}.exporting"));
    Ok((output, temporary, codec, format))
}

pub async fn export_dubbing_audio(
    app_config_dir: &Path,
    ffmpeg_path: &Path,
    session_id: &str,
    output_path: &str,
    cancelled: Arc<AtomicBool>,
) -> Result<DubbingSession> {
    let mut session = load_session_raw(app_config_dir, session_id)?;
    if session.cues.is_empty() {
        return Err(FinalSubError::Validation("配音会话没有字幕行".into()));
    }
    if session.cues.len() > MAX_CUES {
        return Err(FinalSubError::Validation(format!(
            "单次导出最多支持 {MAX_CUES} 条配音"
        )));
    }
    let unavailable = session
        .cues
        .iter()
        .filter(|cue| {
            !matches!(
                cue.status,
                DubbingCueStatus::Ready | DubbingCueStatus::Accepted
            )
        })
        .map(|cue| cue.index + 1)
        .take(20)
        .collect::<Vec<_>>();
    if !unavailable.is_empty() {
        return Err(FinalSubError::Validation(format!(
            "仍有未完成或未确认的配音行：{}",
            unavailable
                .iter()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join("、")
        )));
    }
    let (output, temporary, codec, format) = validate_export_path(output_path)?;
    let directory = session_dir(app_config_dir, session_id)?;
    let filter_path = directory.join("export-filter.txt");
    let mut filters = Vec::new();
    let mut mix_inputs = String::new();
    let mut command = tokio::process::Command::new(ffmpeg_path);
    command.args(["-hide_banner", "-loglevel", "error", "-y"]);
    for (position, cue) in session.cues.iter().enumerate() {
        let wav = cue
            .wav_path
            .as_deref()
            .ok_or_else(|| FinalSubError::Validation("配音行缺少 WAV 产物".into()))?;
        if !Path::new(wav).is_file() {
            return Err(FinalSubError::Validation(format!(
                "第 {} 行配音 WAV 已丢失",
                cue.index + 1
            )));
        }
        command.arg("-i").arg(wav);
        filters.push(format!(
            "[{position}:a]aformat=sample_rates=48000:channel_layouts=stereo,adelay={}:all=1[a{position}]",
            cue.start_ms
        ));
        mix_inputs.push_str(&format!("[a{position}]"));
    }
    filters.push(format!(
        "{mix_inputs}amix=inputs={}:duration=longest:normalize=0,alimiter=limit=0.98[out]",
        session.cues.len()
    ));
    std::fs::write(&filter_path, filters.join(";\n"))?;
    command
        .args(["-filter_complex_script"])
        .arg(&filter_path)
        .args([
            "-map", "[out]", "-ac", "2", "-ar", "48000", "-c:a", codec, "-f", format,
        ])
        .arg(&temporary)
        .kill_on_drop(true);
    let mut child = command
        .spawn()
        .map_err(|error| FinalSubError::Validation(format!("无法启动 FFmpeg 配音导出：{error}")))?;
    let status = tokio::select! {
        result = child.wait() => result?,
        _ = wait_cancelled(cancelled) => {
            let _ = child.kill().await;
            let _ = tokio::fs::remove_file(&temporary).await;
            let _ = tokio::fs::remove_file(&filter_path).await;
            return Err(FinalSubError::Validation("配音导出已取消".into()));
        }
    };
    let _ = tokio::fs::remove_file(&filter_path).await;
    if !status.success() {
        let _ = tokio::fs::remove_file(&temporary).await;
        return Err(FinalSubError::Validation("配音时间轴导出失败".into()));
    }
    // 同目录 rename 在 macOS 上原子替换旧导出，失败时不会先丢失原产物。
    tokio::fs::rename(&temporary, &output).await?;
    session.output_path = Some(output.to_string_lossy().to_string());
    session.updated_at = chrono::Utc::now().to_rfc3339();
    save_session(app_config_dir, &session)?;
    Ok(session)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn cue(start_ms: u64, end_ms: u64) -> DubbingCue {
        DubbingCue {
            index: 0,
            start_ms,
            end_ms,
            text: "hello".into(),
            status: DubbingCueStatus::Pending,
            overlap: false,
            voice_id: None,
            synthesized_ms: None,
            applied_speed: None,
            slot_ms: end_ms - start_ms,
            ratio: None,
            wav_path: None,
            error: None,
        }
    }

    #[test]
    fn alignment_borrows_gap_until_next_cue() {
        let decision = alignment_decision(&cue(1_000, 2_000), Some(3_000), 1_800);
        assert_eq!(decision.slot_ms, 2_000);
        assert!(!decision.overlong);
        assert!(decision.atempo.is_none());
        assert_eq!(decision.pad_ms, 200);
    }

    #[test]
    fn alignment_marks_redline_and_overlap() {
        let decision = alignment_decision(&cue(1_000, 3_000), Some(2_000), 3_200);
        assert!(decision.overlap);
        assert!(decision.overlong);
        assert_eq!(decision.slot_ms, 2_000);
        assert!(decision.atempo.is_none());
    }

    #[test]
    fn atempo_filter_splits_large_factors() {
        assert_eq!(atempo_filter(1.25).unwrap(), "atempo=1.250000");
        assert_eq!(
            atempo_filter(5.0).unwrap(),
            "atempo=2.0,atempo=2.0,atempo=1.250000"
        );
    }

    #[test]
    fn session_persists_and_detects_changed_subtitle() {
        let config = TempDir::new().unwrap();
        let subtitle = config.path().join("demo.srt");
        std::fs::write(
            &subtitle,
            "1\n00:00:01,000 --> 00:00:02,000\nHello\n\n2\n00:00:03,000 --> 00:00:04,000\nWorld\n",
        )
        .unwrap();
        let created =
            create_dubbing_session(config.path(), subtitle.to_str().unwrap(), None).unwrap();
        assert_eq!(created.cues.len(), 2);
        assert_eq!(created.cues[0].slot_ms, 2_000);
        assert!(
            !get_dubbing_session(config.path(), &created.id)
                .unwrap()
                .source_changed
        );
        std::fs::write(&subtitle, "1\n00:00:01,000 --> 00:00:02,000\nChanged\n").unwrap();
        assert!(
            get_dubbing_session(config.path(), &created.id)
                .unwrap()
                .source_changed
        );
        std::fs::remove_file(&subtitle).unwrap();
        assert!(
            get_dubbing_session(config.path(), &created.id)
                .unwrap()
                .source_changed
        );
    }

    #[test]
    fn cue_edit_persists_text_and_voice_and_invalidates_audio() {
        let config = TempDir::new().unwrap();
        let subtitle = config.path().join("demo.srt");
        std::fs::write(&subtitle, "1\n00:00:01,000 --> 00:00:02,000\nHello\n").unwrap();
        let created =
            create_dubbing_session(config.path(), subtitle.to_str().unwrap(), None).unwrap();
        let wav = config
            .path()
            .join("tts/dubbing-sessions")
            .join(&created.id)
            .join("cues/00001.wav");
        std::fs::write(&wav, b"old audio").unwrap();
        let mut saved = load_session_raw(config.path(), &created.id).unwrap();
        saved.cues[0].status = DubbingCueStatus::Ready;
        saved.cues[0].voice_id = Some("alloy".into());
        saved.cues[0].wav_path = Some(wav.to_string_lossy().to_string());
        saved.output_path = Some(
            config
                .path()
                .join("export.wav")
                .to_string_lossy()
                .to_string(),
        );
        save_session(config.path(), &saved).unwrap();

        let updated = update_dubbing_cue(
            config.path(),
            UpdateDubbingCueRequest {
                session_id: created.id.clone(),
                cue_index: 0,
                text: Some("你好".into()),
                voice_id: Some("zh-CN-XiaoxiaoNeural".into()),
            },
        )
        .unwrap();
        assert_eq!(updated.cues[0].text, "你好");
        assert_eq!(
            updated.cues[0].voice_id.as_deref(),
            Some("zh-CN-XiaoxiaoNeural")
        );
        assert_eq!(updated.cues[0].status, DubbingCueStatus::Pending);
        assert!(updated.cues[0].wav_path.is_none());
        assert!(updated.output_path.is_none());
        assert!(!wav.exists());
    }

    #[test]
    fn cue_edit_rejects_empty_text_and_control_voice() {
        let config = TempDir::new().unwrap();
        let subtitle = config.path().join("demo.srt");
        std::fs::write(&subtitle, "1\n00:00:01,000 --> 00:00:02,000\nHello\n").unwrap();
        let created =
            create_dubbing_session(config.path(), subtitle.to_str().unwrap(), None).unwrap();
        assert!(update_dubbing_cue(
            config.path(),
            UpdateDubbingCueRequest {
                session_id: created.id.clone(),
                cue_index: 0,
                text: Some("  ".into()),
                voice_id: None,
            },
        )
        .is_err());
        assert!(update_dubbing_cue(
            config.path(),
            UpdateDubbingCueRequest {
                session_id: created.id,
                cue_index: 0,
                text: None,
                voice_id: Some("bad\nvoice".into()),
            },
        )
        .is_err());
    }

    #[test]
    fn cue_edit_rejects_out_of_range_index_without_overflow() {
        let config = TempDir::new().unwrap();
        let subtitle = config.path().join("demo.srt");
        std::fs::write(&subtitle, "1\n00:00:01,000 --> 00:00:02,000\nHello\n").unwrap();
        let created =
            create_dubbing_session(config.path(), subtitle.to_str().unwrap(), None).unwrap();

        assert!(update_dubbing_cue(
            config.path(),
            UpdateDubbingCueRequest {
                session_id: created.id,
                cue_index: u32::MAX,
                text: Some("你好".into()),
                voice_id: None,
            },
        )
        .is_err());
    }

    #[test]
    fn subtitle_copy_exports_edited_text_without_overwriting() {
        let config = TempDir::new().unwrap();
        let subtitle = config.path().join("demo.srt");
        std::fs::write(&subtitle, "1\n00:00:01,000 --> 00:00:02,000\nHello\n").unwrap();
        let created =
            create_dubbing_session(config.path(), subtitle.to_str().unwrap(), None).unwrap();
        let updated = update_dubbing_cue(
            config.path(),
            UpdateDubbingCueRequest {
                session_id: created.id.clone(),
                cue_index: 0,
                text: Some("你好，世界".into()),
                voice_id: None,
            },
        )
        .unwrap();
        assert!(updated.subtitle_dirty);

        let output = config.path().join("demo.finalsub-edited.srt");
        let exported =
            export_dubbing_subtitle(config.path(), &created.id, output.to_str().unwrap()).unwrap();
        assert_eq!(PathBuf::from(exported), output);
        assert!(std::fs::read_to_string(&output)
            .unwrap()
            .contains("你好，世界"));
        assert!(
            export_dubbing_subtitle(config.path(), &created.id, output.to_str().unwrap())
                .unwrap_err()
                .to_string()
                .contains("已存在")
        );
    }

    #[test]
    fn subtitle_writeback_creates_exact_backup_and_refreshes_hash() {
        let config = TempDir::new().unwrap();
        let subtitle = config.path().join("demo.srt");
        let original = b"1\r\n00:00:01,000 --> 00:00:02,000\r\nHello\r\n";
        std::fs::write(&subtitle, original).unwrap();
        let created =
            create_dubbing_session(config.path(), subtitle.to_str().unwrap(), None).unwrap();
        update_dubbing_cue(
            config.path(),
            UpdateDubbingCueRequest {
                session_id: created.id.clone(),
                cue_index: 0,
                text: Some("已安全写回".into()),
                voice_id: None,
            },
        )
        .unwrap();

        let result = write_back_dubbing_subtitle(config.path(), &created.id).unwrap();
        assert_eq!(std::fs::read(&result.backup_path).unwrap(), original);
        let written = std::fs::read(&subtitle).unwrap();
        assert!(String::from_utf8(written.clone())
            .unwrap()
            .contains("已安全写回"));
        assert_eq!(result.session.subtitle_hash, hash_bytes(&written));
        assert!(!result.session.subtitle_dirty);
        assert!(!result.session.source_changed);
        let reloaded = get_dubbing_session(config.path(), &created.id).unwrap();
        assert!(!reloaded.subtitle_dirty);
        assert!(!reloaded.source_changed);
    }

    #[test]
    fn subtitle_writeback_refuses_external_source_change() {
        let config = TempDir::new().unwrap();
        let subtitle = config.path().join("demo.srt");
        std::fs::write(&subtitle, "1\n00:00:01,000 --> 00:00:02,000\nHello\n").unwrap();
        let created =
            create_dubbing_session(config.path(), subtitle.to_str().unwrap(), None).unwrap();
        update_dubbing_cue(
            config.path(),
            UpdateDubbingCueRequest {
                session_id: created.id.clone(),
                cue_index: 0,
                text: Some("Session edit".into()),
                voice_id: None,
            },
        )
        .unwrap();
        let external = "1\n00:00:01,000 --> 00:00:02,000\nExternal edit\n";
        std::fs::write(&subtitle, external).unwrap();

        let error = write_back_dubbing_subtitle(config.path(), &created.id)
            .unwrap_err()
            .to_string();
        assert!(error.contains("会话外变化"));
        assert_eq!(std::fs::read_to_string(&subtitle).unwrap(), external);
        assert!(std::fs::read_dir(config.path())
            .unwrap()
            .filter_map(|entry| entry.ok())
            .all(|entry| !entry
                .file_name()
                .to_string_lossy()
                .contains("finalsub-backup")));
    }

    #[test]
    fn subtitle_export_accepts_lrc_centisecond_rounding() {
        let config = TempDir::new().unwrap();
        let subtitle = config.path().join("demo.lrc");
        std::fs::write(&subtitle, "[00:01.005]Hello\n[00:03.015]World\n").unwrap();
        let created =
            create_dubbing_session(config.path(), subtitle.to_str().unwrap(), None).unwrap();
        let output = config.path().join("demo-edited.lrc");
        export_dubbing_subtitle(config.path(), &created.id, output.to_str().unwrap()).unwrap();
        assert!(std::fs::read_to_string(output).unwrap().contains("Hello"));
    }

    #[test]
    fn subtitle_writeback_rejects_active_synthesis_without_recovery() {
        let config = TempDir::new().unwrap();
        let subtitle = config.path().join("demo.srt");
        std::fs::write(&subtitle, "1\n00:00:01,000 --> 00:00:02,000\nHello\n").unwrap();
        let created =
            create_dubbing_session(config.path(), subtitle.to_str().unwrap(), None).unwrap();
        let mut persisted = load_session_raw(config.path(), &created.id).unwrap();
        persisted.cues[0].status = DubbingCueStatus::Synthesizing;
        save_session(config.path(), &persisted).unwrap();
        assert!(write_back_dubbing_subtitle(config.path(), &created.id)
            .unwrap_err()
            .to_string()
            .contains("正在合成"));
    }
}
