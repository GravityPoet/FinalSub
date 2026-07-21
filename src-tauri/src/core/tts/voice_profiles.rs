use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use serde::{Deserialize, Serialize};
use sherpa_onnx::Wave;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::process::Command;
use uuid::Uuid;

use crate::core::asr::vad::{detect_speech, SAMPLE_RATE as VAD_SAMPLE_RATE};
use crate::core::audio;
use crate::core::secrets;
use crate::error::{FinalSubError, Result};

const MAX_PROFILES: usize = 100;
const MAX_SOURCE_BYTES: u64 = 8 * 1024 * 1024 * 1024;
const MAX_REFERENCE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_PACKAGE_BYTES: u64 = 96 * 1024 * 1024;
const MAX_RECORDING_BYTES: usize = 16 * 1024 * 1024;
const MAX_NAME_CHARS: usize = 60;
const MAX_REFERENCE_TEXT_BYTES: usize = 4_000;
const ZIPVOICE_MIN_SELECTION_MS: u64 = 3_000;
const ZIPVOICE_IDEAL_SELECTION_MIN_MS: u64 = 5_000;
const ZIPVOICE_DEFAULT_SELECTION_MS: u64 = 8_000;
const ZIPVOICE_MAX_SELECTION_MS: u64 = 10_000;
const ELEVENLABS_MIN_SELECTION_MS: u64 = 5_000;
const ELEVENLABS_IDEAL_SELECTION_MIN_MS: u64 = 30_000;
const ELEVENLABS_MAX_SELECTION_MS: u64 = 180_000;
const PROFILE_FILE: &str = "profile.json";
const PREPARED_FILE: &str = "prepared.json";
const REFERENCE_FILE: &str = "ref.wav";
const SVOICE_FORMAT: &str = "smartsub-voice";
const SVOICE_VERSION: u32 = 1;

const MEDIA_EXTENSIONS: &[&str] = &[
    "wav", "mp3", "m4a", "aac", "flac", "ogg", "opus", "webm", "mp4", "mov", "mkv", "avi", "m4v",
    "ts",
];

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum VoiceProfileLanguage {
    Zh,
    En,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum VoiceCloneEngine {
    #[default]
    Zipvoice,
    Elevenlabs,
    Volcengine,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CloudVoiceStatus {
    Training,
    Ready,
    Failed,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum VoiceQualityVerdict {
    Good,
    Fair,
    Poor,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum VoiceQualityIssueCode {
    NoSpeech,
    TooShort,
    ShortForEngine,
    LowSnr,
    Clipping,
    LowVolume,
    LowSpeechRatio,
    LongSilence,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum VoiceQualityIssueSeverity {
    Error,
    Warning,
    Info,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VoiceQualityIssue {
    pub code: VoiceQualityIssueCode,
    pub severity: VoiceQualityIssueSeverity,
    pub value: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VoiceQualityReport {
    pub duration_ms: u64,
    pub speech_ms: u64,
    pub speech_ratio: f64,
    pub longest_silence_ms: u64,
    pub rms_db: f64,
    pub peak_db: f64,
    pub clipping_ratio: f64,
    pub snr_db: f64,
    pub verdict: VoiceQualityVerdict,
    pub issues: Vec<VoiceQualityIssue>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VoiceProfile {
    pub id: String,
    pub name: String,
    pub engine: String,
    pub language: VoiceProfileLanguage,
    pub reference_audio_path: String,
    pub reference_text: String,
    pub source_name: Option<String>,
    pub quality: VoiceQualityReport,
    #[serde(default)]
    pub provider_id: Option<String>,
    #[serde(default)]
    pub cloud_voice_id: Option<String>,
    #[serde(default)]
    pub cloud_status: Option<CloudVoiceStatus>,
    #[serde(default)]
    pub volc_training_times_left: Option<u32>,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VoiceSourceInfo {
    pub path: String,
    pub file_name: String,
    pub duration_ms: u64,
    pub default_selection_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VoiceSubtitleCue {
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PreparedVoiceSample {
    pub token: String,
    pub audio_path: String,
    pub source_name: String,
    pub start_ms: u64,
    pub duration_ms: u64,
    pub quality: VoiceQualityReport,
    pub can_create: bool,
    pub engine: VoiceCloneEngine,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PrepareVoiceSampleRequest {
    pub source_path: String,
    pub start_ms: u64,
    pub duration_ms: u64,
    #[serde(default)]
    pub engine: VoiceCloneEngine,
    #[serde(default)]
    pub local_denoise: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateVoiceProfileRequest {
    pub token: String,
    pub name: String,
    pub language: VoiceProfileLanguage,
    pub reference_text: String,
    pub consent: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateCloudVoiceProfileRequest {
    pub token: String,
    pub name: String,
    pub language: VoiceProfileLanguage,
    pub provider_id: String,
    pub consent: bool,
    pub upload_consent: bool,
    #[serde(default)]
    pub voice_id: String,
    #[serde(default)]
    pub remove_background_noise: bool,
    #[serde(default)]
    pub enable_mss: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RetrainCloudVoiceProfileRequest {
    pub id: String,
    #[serde(default)]
    pub remove_background_noise: bool,
    #[serde(default)]
    pub enable_mss: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LinkCloudVoiceProfileRequest {
    pub name: String,
    pub language: VoiceProfileLanguage,
    pub provider_id: String,
    pub voice_id: String,
    pub consent: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PreparedVoiceRecord {
    token: String,
    source_name: String,
    start_ms: u64,
    duration_ms: u64,
    quality: VoiceQualityReport,
    created_at: i64,
    #[serde(default)]
    engine: VoiceCloneEngine,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SvoiceQualityReport<'a> {
    duration_ms: u64,
    speech_ms: u64,
    speech_ratio: f64,
    longest_silence_ms: u64,
    rms_db: f64,
    peak_db: f64,
    clipping_ratio: f64,
    snr_db: f64,
    verdict: VoiceQualityVerdict,
    issues: &'a [VoiceQualityIssue],
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SvoiceExportVoice<'a> {
    name: &'a str,
    engine: &'a str,
    language: VoiceProfileLanguage,
    ref_text: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    speaker_id: Option<&'a str>,
    quality: SvoiceQualityReport<'a>,
    created_at: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SvoiceExportPackage<'a> {
    format: &'static str,
    version: u32,
    voice: SvoiceExportVoice<'a>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ref_wav_base64: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SvoiceImportVoice {
    name: String,
    engine: String,
    language: VoiceProfileLanguage,
    ref_text: Option<String>,
    #[serde(default)]
    speaker_id: Option<String>,
    #[serde(default)]
    quality: Option<SvoiceImportQualityReport>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SvoiceImportQualityReport {
    duration_ms: u64,
    speech_ms: u64,
    speech_ratio: f64,
    longest_silence_ms: u64,
    rms_db: f64,
    peak_db: f64,
    clipping_ratio: f64,
    snr_db: f64,
    verdict: VoiceQualityVerdict,
    issues: Vec<VoiceQualityIssue>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SvoiceImportPackage {
    format: String,
    version: u32,
    voice: SvoiceImportVoice,
    ref_wav_base64: Option<String>,
}

fn voice_root(app_config_dir: &Path) -> PathBuf {
    app_config_dir.join("tts").join("voice-profiles")
}

fn staging_root(app_config_dir: &Path) -> PathBuf {
    voice_root(app_config_dir).join(".staging")
}

fn recordings_root(app_config_dir: &Path) -> PathBuf {
    voice_root(app_config_dir).join(".recordings")
}

fn trash_root(app_config_dir: &Path) -> PathBuf {
    voice_root(app_config_dir).join(".trash")
}

fn path_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

#[derive(Debug, Clone, Copy)]
struct VoiceSelectionLimits {
    min_ms: u64,
    ideal_min_ms: u64,
    max_ms: u64,
}

fn selection_limits(engine: VoiceCloneEngine) -> VoiceSelectionLimits {
    match engine {
        VoiceCloneEngine::Zipvoice => VoiceSelectionLimits {
            min_ms: ZIPVOICE_MIN_SELECTION_MS,
            ideal_min_ms: ZIPVOICE_IDEAL_SELECTION_MIN_MS,
            max_ms: ZIPVOICE_MAX_SELECTION_MS,
        },
        VoiceCloneEngine::Elevenlabs => VoiceSelectionLimits {
            min_ms: ELEVENLABS_MIN_SELECTION_MS,
            ideal_min_ms: ELEVENLABS_IDEAL_SELECTION_MIN_MS,
            max_ms: ELEVENLABS_MAX_SELECTION_MS,
        },
        VoiceCloneEngine::Volcengine => VoiceSelectionLimits {
            min_ms: 5_000,
            ideal_min_ms: 14_000,
            max_ms: 30_000,
        },
    }
}

fn cloud_quality_placeholder() -> VoiceQualityReport {
    VoiceQualityReport {
        duration_ms: 0,
        speech_ms: 0,
        speech_ratio: 0.0,
        longest_silence_ms: 0,
        rms_db: 0.0,
        peak_db: 0.0,
        clipping_ratio: 0.0,
        snr_db: 0.0,
        verdict: VoiceQualityVerdict::Fair,
        issues: Vec::new(),
    }
}

fn validate_uuid(raw: &str, label: &str) -> Result<Uuid> {
    Uuid::parse_str(raw.trim()).map_err(|_| FinalSubError::Validation(format!("{label} 无效")))
}

fn profile_dir(app_config_dir: &Path, id: &str) -> Result<PathBuf> {
    let id = validate_uuid(id, "音色 ID")?;
    Ok(voice_root(app_config_dir).join(id.to_string()))
}

fn prepared_dir(app_config_dir: &Path, token: &str) -> Result<PathBuf> {
    let token = validate_uuid(token, "音色准备令牌")?;
    Ok(staging_root(app_config_dir).join(token.to_string()))
}

fn validate_name(raw: &str) -> Result<String> {
    let name = raw.trim();
    if name.is_empty() || name.chars().count() > MAX_NAME_CHARS {
        return Err(FinalSubError::Validation(format!(
            "音色名称必须为 1-{MAX_NAME_CHARS} 个字符"
        )));
    }
    if name.chars().any(char::is_control) {
        return Err(FinalSubError::Validation("音色名称不能包含控制字符".into()));
    }
    Ok(name.to_string())
}

fn validate_reference_text(raw: &str) -> Result<String> {
    let text = raw.trim();
    if text.is_empty() {
        return Err(FinalSubError::Validation(
            "请填写与参考音频逐字一致的文本".into(),
        ));
    }
    if text.len() > MAX_REFERENCE_TEXT_BYTES || text.contains('\0') {
        return Err(FinalSubError::Validation(format!(
            "参考文本不能包含空字符，且不能超过 {MAX_REFERENCE_TEXT_BYTES} 字节"
        )));
    }
    Ok(text.to_string())
}

fn validate_source_path(raw: &str) -> Result<PathBuf> {
    let path = PathBuf::from(raw.trim());
    if !path.is_absolute() || !path.is_file() {
        return Err(FinalSubError::Validation(
            "音色素材必须是存在的绝对文件路径".into(),
        ));
    }
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if !MEDIA_EXTENSIONS.contains(&extension.as_str()) {
        return Err(FinalSubError::Validation(format!(
            "不支持的音色素材格式：.{extension}"
        )));
    }
    if std::fs::metadata(&path)?.len() > MAX_SOURCE_BYTES {
        return Err(FinalSubError::Validation("音色素材不能超过 8 GB".into()));
    }
    Ok(std::fs::canonicalize(path)?)
}

fn save_json_atomic<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension("json.tmp");
    std::fs::write(&temporary, serde_json::to_vec_pretty(value)?)?;
    std::fs::rename(&temporary, path)?;
    Ok(())
}

fn read_json_bounded<T: for<'de> Deserialize<'de>>(path: &Path, max_bytes: u64) -> Result<T> {
    let metadata = std::fs::metadata(path)?;
    if metadata.len() > max_bytes {
        return Err(FinalSubError::Validation(format!(
            "文件超过允许大小：{} MB",
            max_bytes / 1024 / 1024
        )));
    }
    Ok(serde_json::from_slice(&std::fs::read(path)?)?)
}

fn load_prepared(app_config_dir: &Path, token: &str) -> Result<PreparedVoiceRecord> {
    let dir = prepared_dir(app_config_dir, token)?;
    let record: PreparedVoiceRecord = read_json_bounded(&dir.join(PREPARED_FILE), 256 * 1024)?;
    if record.token != token.trim() {
        return Err(FinalSubError::Validation("音色准备记录与令牌不匹配".into()));
    }
    let reference = dir.join(REFERENCE_FILE);
    if !reference.is_file() || std::fs::metadata(reference)?.len() > MAX_REFERENCE_BYTES {
        return Err(FinalSubError::Validation(
            "准备好的参考音频缺失或超过 64 MB".into(),
        ));
    }
    Ok(record)
}

fn save_profile_in_dir(directory: &Path, profile: &VoiceProfile) -> Result<()> {
    save_json_atomic(&directory.join(PROFILE_FILE), profile)
}

pub fn cleanup_transient_files(app_config_dir: &Path) {
    for root in [
        staging_root(app_config_dir),
        recordings_root(app_config_dir),
    ] {
        if root.exists() {
            let _ = std::fs::remove_dir_all(&root);
        }
        let _ = std::fs::create_dir_all(root);
    }
}

pub fn load_profiles(app_config_dir: &Path) -> Result<HashMap<String, VoiceProfile>> {
    let root = voice_root(app_config_dir);
    std::fs::create_dir_all(&root)?;
    let mut profiles = HashMap::new();
    for entry in std::fs::read_dir(&root)? {
        let entry = match entry {
            Ok(value) => value,
            Err(_) => continue,
        };
        let file_name = entry.file_name().to_string_lossy().into_owned();
        if file_name.starts_with('.') || !entry.path().is_dir() {
            continue;
        }
        let id = match Uuid::parse_str(&file_name) {
            Ok(value) => value.to_string(),
            Err(_) => continue,
        };
        let mut profile: VoiceProfile =
            match read_json_bounded(&entry.path().join(PROFILE_FILE), 256 * 1024) {
                Ok(value) => value,
                Err(_) => continue,
            };
        if profile.id != id {
            continue;
        }
        match profile.engine.as_str() {
            "zipvoice" => {
                let reference = entry.path().join(REFERENCE_FILE);
                if !reference.is_file() {
                    continue;
                }
                profile.reference_audio_path = path_string(&reference);
                profile.provider_id = None;
                profile.cloud_voice_id = None;
                profile.cloud_status = None;
            }
            "elevenlabs" | "volcengine" => {
                if profile
                    .provider_id
                    .as_deref()
                    .is_none_or(|value| Uuid::parse_str(value).is_err())
                    || profile.cloud_voice_id.as_deref().is_none_or(|value| {
                        value.trim().is_empty()
                            || value.len() > 200
                            || value.chars().any(char::is_control)
                            || (profile.engine == "volcengine" && !value.starts_with("S_"))
                    })
                {
                    continue;
                }
                let reference = entry.path().join(REFERENCE_FILE);
                profile.reference_audio_path = if reference.is_file()
                    && std::fs::metadata(&reference)
                        .map(|metadata| metadata.len() <= MAX_REFERENCE_BYTES)
                        .unwrap_or(false)
                {
                    path_string(&reference)
                } else {
                    String::new()
                };
                profile.reference_text.clear();
                profile.cloud_status.get_or_insert(CloudVoiceStatus::Ready);
            }
            _ => continue,
        }
        profiles.insert(id, profile);
        if profiles.len() >= MAX_PROFILES {
            break;
        }
    }
    Ok(profiles)
}

pub async fn inspect_voice_source(
    ffmpeg_path: &Path,
    source_path: &str,
) -> Result<VoiceSourceInfo> {
    let source = validate_source_path(source_path)?;
    let output = Command::new(ffmpeg_path)
        .arg("-nostdin")
        .arg("-hide_banner")
        .arg("-i")
        .arg(&source)
        .output()
        .await
        .map_err(|error| FinalSubError::Validation(format!("无法检查音色素材：{error}")))?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !stderr.lines().any(|line| line.contains("Audio:")) {
        return Err(FinalSubError::Validation("素材中没有可用音轨".into()));
    }
    let duration_ms = audio::parse_duration_ms(&stderr)
        .filter(|duration| *duration > 0)
        .ok_or_else(|| FinalSubError::Validation("无法读取音色素材时长".into()))?;
    let file_name = source
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("voice-sample")
        .to_string();
    Ok(VoiceSourceInfo {
        path: path_string(&source),
        file_name,
        duration_ms,
        default_selection_ms: duration_ms.min(ZIPVOICE_DEFAULT_SELECTION_MS),
    })
}

pub fn list_voice_subtitle_cues(source_path: &str) -> Result<Vec<VoiceSubtitleCue>> {
    let path = PathBuf::from(source_path.trim());
    if !path.is_absolute() || !path.is_file() {
        return Err(FinalSubError::Validation(
            "字幕文件必须是存在的绝对路径".into(),
        ));
    }
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if !matches!(extension.as_str(), "srt" | "vtt" | "ass" | "ssa" | "lrc") {
        return Err(FinalSubError::Validation(
            "只支持 SRT、VTT、ASS、SSA 或 LRC 字幕文件".into(),
        ));
    }
    let metadata = std::fs::metadata(&path)?;
    if metadata.len() > 20 * 1024 * 1024 {
        return Err(FinalSubError::Validation("字幕文件不能超过 20 MB".into()));
    }
    let canonical = std::fs::canonicalize(path)?;
    let content = std::fs::read_to_string(&canonical)
        .map_err(|error| FinalSubError::Validation(format!("读取字幕文件失败：{error}")))?;
    let track = crate::core::subtitle::SubtitleTrack::from_format(&content, &extension)?;
    let cues = track
        .cues
        .into_iter()
        .filter(|cue| cue.end_ms > cue.start_ms && !cue.text.trim().is_empty())
        .take(2_000)
        .map(|cue| VoiceSubtitleCue {
            start_ms: cue.start_ms,
            end_ms: cue.end_ms,
            text: cue.text.trim().replace('\n', " "),
        })
        .collect::<Vec<_>>();
    if cues.is_empty() {
        return Err(FinalSubError::Validation(
            "字幕文件中没有可用的带时间轴文本".into(),
        ));
    }
    Ok(cues)
}

async fn run_ffmpeg(ffmpeg_path: &Path, args: &[String], label: &str) -> Result<()> {
    let output = Command::new(ffmpeg_path)
        .args(args)
        .output()
        .await
        .map_err(|error| FinalSubError::Validation(format!("{label}：{error}")))?;
    if !output.status.success() {
        let details = String::from_utf8_lossy(&output.stderr);
        let details = details
            .lines()
            .rev()
            .take(8)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join("\n");
        return Err(FinalSubError::Validation(format!("{label}：{details}")));
    }
    Ok(())
}

fn seconds_argument(milliseconds: u64) -> String {
    format!("{:.3}", milliseconds as f64 / 1_000.0)
}

fn db_from_energy(energy: f64, count: usize) -> f64 {
    if count == 0 || energy <= 1e-16 {
        -160.0
    } else {
        10.0 * (energy / count as f64).log10()
    }
}

fn analyze_reference(
    path: &Path,
    vad_model_path: &Path,
    limits: VoiceSelectionLimits,
) -> Result<VoiceQualityReport> {
    let wave = Wave::read(&path_string(path))
        .ok_or_else(|| FinalSubError::Validation("无法读取准备好的参考音频".into()))?;
    if wave.sample_rate() != VAD_SAMPLE_RATE || wave.samples().is_empty() {
        return Err(FinalSubError::Validation(
            "参考音频分析副本必须是 16 kHz 单声道 WAV".into(),
        ));
    }
    let samples = wave.samples();
    let duration_ms = (samples.len() as u128 * 1_000 / VAD_SAMPLE_RATE as u128) as u64;
    let segments = detect_speech(samples, vad_model_path, 60).map_err(FinalSubError::Validation)?;
    let mut speech_mask = vec![false; samples.len()];
    for segment in &segments {
        let end = segment
            .start_sample
            .saturating_add(segment.samples.len())
            .min(speech_mask.len());
        if segment.start_sample < end {
            speech_mask[segment.start_sample..end].fill(true);
        }
    }

    let mut speech_energy = 0.0;
    let mut noise_energy = 0.0;
    let mut speech_count = 0usize;
    let mut noise_count = 0usize;
    let mut peak = 0.0f64;
    let mut clipped = 0usize;
    for (index, sample) in samples.iter().enumerate() {
        let value = *sample as f64;
        let absolute = value.abs();
        peak = peak.max(absolute);
        if absolute >= 0.999 {
            clipped += 1;
        }
        if speech_mask[index] {
            speech_energy += value * value;
            speech_count += 1;
        } else {
            noise_energy += value * value;
            noise_count += 1;
        }
    }
    let speech_ms = (speech_count as u128 * 1_000 / VAD_SAMPLE_RATE as u128) as u64;
    let speech_ratio = if samples.is_empty() {
        0.0
    } else {
        speech_count as f64 / samples.len() as f64
    };
    let rms_db = db_from_energy(speech_energy, speech_count);
    let noise_db = db_from_energy(noise_energy, noise_count);
    let snr_db = if speech_count == 0 {
        0.0
    } else if noise_count == 0 || noise_db <= -150.0 {
        40.0
    } else {
        (rms_db - noise_db).clamp(0.0, 40.0)
    };
    let peak_db = if peak <= 1e-8 {
        -160.0
    } else {
        20.0 * peak.log10()
    };
    let clipping_ratio = clipped as f64 / samples.len() as f64;

    let mut longest_silence_samples = 0usize;
    let mut previous_end = 0usize;
    for segment in &segments {
        longest_silence_samples =
            longest_silence_samples.max(segment.start_sample.saturating_sub(previous_end));
        previous_end = segment
            .start_sample
            .saturating_add(segment.samples.len())
            .min(samples.len());
    }
    longest_silence_samples =
        longest_silence_samples.max(samples.len().saturating_sub(previous_end));
    let longest_silence_ms =
        (longest_silence_samples as u128 * 1_000 / VAD_SAMPLE_RATE as u128) as u64;

    let mut issues = Vec::new();
    let mut issue = |code, severity, value| {
        issues.push(VoiceQualityIssue {
            code,
            severity,
            value,
        });
    };
    if speech_ms == 0 {
        issue(
            VoiceQualityIssueCode::NoSpeech,
            VoiceQualityIssueSeverity::Error,
            None,
        );
    } else if speech_ms < limits.min_ms {
        issue(
            VoiceQualityIssueCode::TooShort,
            VoiceQualityIssueSeverity::Error,
            Some(speech_ms as f64),
        );
    } else if speech_ms < limits.ideal_min_ms {
        issue(
            VoiceQualityIssueCode::ShortForEngine,
            VoiceQualityIssueSeverity::Warning,
            Some(speech_ms as f64),
        );
    }
    if speech_ms > 0 && snr_db < 15.0 {
        issue(
            VoiceQualityIssueCode::LowSnr,
            VoiceQualityIssueSeverity::Warning,
            Some(snr_db),
        );
    }
    if clipping_ratio > 0.001 || peak_db > -0.5 {
        issue(
            VoiceQualityIssueCode::Clipping,
            VoiceQualityIssueSeverity::Warning,
            Some(clipping_ratio),
        );
    }
    if speech_ms > 0 && rms_db < -35.0 {
        issue(
            VoiceQualityIssueCode::LowVolume,
            VoiceQualityIssueSeverity::Info,
            Some(rms_db),
        );
    }
    if speech_ms > 0 && speech_ratio < 0.55 {
        issue(
            VoiceQualityIssueCode::LowSpeechRatio,
            VoiceQualityIssueSeverity::Warning,
            Some(speech_ratio),
        );
    }
    if longest_silence_ms > 1_500 {
        issue(
            VoiceQualityIssueCode::LongSilence,
            VoiceQualityIssueSeverity::Warning,
            Some(longest_silence_ms as f64),
        );
    }
    let verdict = if issues
        .iter()
        .any(|item| item.severity == VoiceQualityIssueSeverity::Error)
    {
        VoiceQualityVerdict::Poor
    } else if issues
        .iter()
        .any(|item| item.severity == VoiceQualityIssueSeverity::Warning)
    {
        VoiceQualityVerdict::Fair
    } else {
        VoiceQualityVerdict::Good
    };
    Ok(VoiceQualityReport {
        duration_ms,
        speech_ms,
        speech_ratio,
        longest_silence_ms,
        rms_db,
        peak_db,
        clipping_ratio,
        snr_db,
        verdict,
        issues,
    })
}

pub async fn prepare_voice_sample(
    app_config_dir: &Path,
    ffmpeg_path: &Path,
    vad_model_path: &Path,
    request: PrepareVoiceSampleRequest,
) -> Result<PreparedVoiceSample> {
    let limits = selection_limits(request.engine);
    if !(limits.min_ms..=limits.max_ms).contains(&request.duration_ms) {
        return Err(FinalSubError::Validation(format!(
            "声音克隆片段必须在 {}-{} 秒之间",
            limits.min_ms / 1_000,
            limits.max_ms / 1_000
        )));
    }
    let source = validate_source_path(&request.source_path)?;
    let staging_id = Uuid::new_v4().to_string();
    let directory = staging_root(app_config_dir).join(&staging_id);
    std::fs::create_dir_all(&directory)?;
    let analysis_path = directory.join("analysis.wav");
    let reference_path = directory.join(REFERENCE_FILE);
    let start = seconds_argument(request.start_ms);
    let duration = seconds_argument(request.duration_ms);
    let common = vec![
        "-nostdin".into(),
        "-hide_banner".into(),
        "-loglevel".into(),
        "error".into(),
        "-ss".into(),
        start,
        "-t".into(),
        duration,
        "-i".into(),
        path_string(&source),
        "-vn".into(),
        "-ac".into(),
        "1".into(),
    ];
    let result = async {
        let mut analysis_args = common.clone();
        analysis_args.extend([
            "-af".into(),
            if request.local_denoise {
                "afftdn=nr=12:nf=-40".into()
            } else {
                "anull".into()
            },
            "-ar".into(),
            VAD_SAMPLE_RATE.to_string(),
            "-c:a".into(),
            "pcm_s16le".into(),
            "-y".into(),
            path_string(&analysis_path),
        ]);
        run_ffmpeg(ffmpeg_path, &analysis_args, "无法分析音色素材").await?;
        let quality_path = analysis_path.clone();
        let vad_path = vad_model_path.to_path_buf();
        let quality = tokio::task::spawn_blocking(move || {
            analyze_reference(&quality_path, &vad_path, limits)
        })
        .await
        .map_err(|error| FinalSubError::Validation(format!("音色质检线程失败：{error}")))??;

        let mut reference_args = common;
        reference_args.extend([
            "-af".into(),
            if request.local_denoise {
                "afftdn=nr=12:nf=-40,loudnorm=I=-20:LRA=7:TP=-3".into()
            } else {
                "loudnorm=I=-20:LRA=7:TP=-3".into()
            },
            "-ar".into(),
            "24000".into(),
            "-c:a".into(),
            "pcm_s16le".into(),
            "-y".into(),
            path_string(&reference_path),
        ]);
        run_ffmpeg(ffmpeg_path, &reference_args, "无法准备音色参考音频").await?;
        let reference_wave = Wave::read(&path_string(&reference_path))
            .ok_or_else(|| FinalSubError::Validation("无法读取归一化后的参考音频".into()))?;
        if reference_wave.samples().is_empty()
            || reference_wave.sample_rate() != 24_000
            || std::fs::metadata(&reference_path)?.len() > MAX_REFERENCE_BYTES
        {
            return Err(FinalSubError::Validation(
                "归一化后的参考音频为空、采样率不正确或超过 64 MB".into(),
            ));
        }
        let actual_duration_ms = (reference_wave.samples().len() as u128 * 1_000
            / reference_wave.sample_rate() as u128) as u64;
        let source_name = source
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("voice-sample")
            .to_string();
        let record = PreparedVoiceRecord {
            token: staging_id.clone(),
            source_name: source_name.clone(),
            start_ms: request.start_ms,
            duration_ms: actual_duration_ms,
            quality: quality.clone(),
            created_at: chrono::Utc::now().timestamp_millis(),
            engine: request.engine,
        };
        save_json_atomic(&directory.join(PREPARED_FILE), &record)?;
        let _ = std::fs::remove_file(&analysis_path);
        let can_create = !quality
            .issues
            .iter()
            .any(|item| item.severity == VoiceQualityIssueSeverity::Error);
        Ok(PreparedVoiceSample {
            token: staging_id.clone(),
            audio_path: path_string(&reference_path),
            source_name,
            start_ms: request.start_ms,
            duration_ms: actual_duration_ms,
            quality,
            can_create,
            engine: request.engine,
        })
    }
    .await;
    if result.is_err() {
        let _ = std::fs::remove_dir_all(&directory);
    }
    result
}

pub fn discard_prepared_voice_sample(app_config_dir: &Path, token: &str) -> Result<()> {
    let directory = prepared_dir(app_config_dir, token)?;
    if directory.exists() {
        std::fs::remove_dir_all(directory)?;
    }
    Ok(())
}

pub fn create_voice_profile(
    app_config_dir: &Path,
    profiles: &mut HashMap<String, VoiceProfile>,
    request: CreateVoiceProfileRequest,
) -> Result<VoiceProfile> {
    if !request.consent {
        return Err(FinalSubError::Validation(
            "请确认你拥有该声音的使用与克隆授权".into(),
        ));
    }
    if profiles.len() >= MAX_PROFILES {
        return Err(FinalSubError::Validation(format!(
            "我的音色最多保存 {MAX_PROFILES} 个"
        )));
    }
    let name = validate_name(&request.name)?;
    let reference_text = validate_reference_text(&request.reference_text)?;
    let prepared = load_prepared(app_config_dir, &request.token)?;
    if prepared.engine != VoiceCloneEngine::Zipvoice {
        return Err(FinalSubError::Validation(
            "该参考音频不是为本地 ZipVoice 准备的，请重新分析".into(),
        ));
    }
    if prepared
        .quality
        .issues
        .iter()
        .any(|item| item.severity == VoiceQualityIssueSeverity::Error)
    {
        return Err(FinalSubError::Validation(
            "参考片段没有足够的有效语音，请重新选择片段".into(),
        ));
    }

    let id = Uuid::new_v4().to_string();
    let source_directory = prepared_dir(app_config_dir, &request.token)?;
    let target_directory = profile_dir(app_config_dir, &id)?;
    if target_directory.exists() {
        return Err(FinalSubError::Validation("音色目录冲突，请重试".into()));
    }
    let rollback_record = prepared.clone();
    let profile = VoiceProfile {
        id: id.clone(),
        name,
        engine: "zipvoice".into(),
        language: request.language,
        reference_audio_path: path_string(&target_directory.join(REFERENCE_FILE)),
        reference_text,
        source_name: Some(prepared.source_name),
        quality: prepared.quality,
        provider_id: None,
        cloud_voice_id: None,
        cloud_status: None,
        volc_training_times_left: None,
        created_at: chrono::Utc::now().timestamp_millis(),
    };
    save_profile_in_dir(&source_directory, &profile)?;
    let _ = std::fs::remove_file(source_directory.join(PREPARED_FILE));
    if let Err(error) = std::fs::rename(&source_directory, &target_directory) {
        let _ = std::fs::remove_file(source_directory.join(PROFILE_FILE));
        let _ = save_json_atomic(&source_directory.join(PREPARED_FILE), &rollback_record);
        return Err(error.into());
    }
    profiles.insert(id, profile.clone());
    Ok(profile)
}

fn validate_cloud_voice_id(engine: &str, raw: &str) -> Result<String> {
    let voice_id = raw.trim();
    if voice_id.is_empty()
        || voice_id.len() > 200
        || voice_id.chars().any(char::is_control)
        || (engine == "volcengine" && !voice_id.starts_with("S_"))
    {
        return Err(FinalSubError::Validation(if engine == "volcengine" {
            "豆包声音复刻音色 ID 必须以 S_ 开头".into()
        } else {
            "云端音色 ID 无效".into()
        }));
    }
    Ok(voice_id.to_string())
}

fn ensure_cloud_voice_not_linked(
    profiles: &HashMap<String, VoiceProfile>,
    provider_id: &str,
    voice_id: &str,
) -> Result<()> {
    if profiles.values().any(|profile| {
        profile.provider_id.as_deref() == Some(provider_id)
            && profile.cloud_voice_id.as_deref() == Some(voice_id)
    }) {
        return Err(FinalSubError::Validation(
            "这个云端音色已经在“我的音色”中，无需重复找回".into(),
        ));
    }
    Ok(())
}

fn cloud_engine_for_provider(
    provider: &super::providers::TtsProviderProfile,
) -> Result<VoiceCloneEngine> {
    match provider.protocol {
        super::providers::TtsProviderProtocol::Elevenlabs => Ok(VoiceCloneEngine::Elevenlabs),
        super::providers::TtsProviderProtocol::Volcengine => Ok(VoiceCloneEngine::Volcengine),
        _ => Err(FinalSubError::Validation(
            "只有 ElevenLabs 或豆包 TTS 实例可以创建云端克隆音色".into(),
        )),
    }
}

fn volc_clone_credentials(
    app_config_dir: &Path,
    provider_id: &str,
) -> Result<(String, String, u32)> {
    let provider = super::providers::find_provider(app_config_dir, provider_id)?;
    if provider.protocol != super::providers::TtsProviderProtocol::Volcengine {
        return Err(FinalSubError::Validation(
            "训练凭据只适用于豆包 TTS 实例".into(),
        ));
    }
    let endpoint = super::providers::resolved_provider_endpoint(&provider)?;
    let secret_id = super::providers::provider_secret_id(&provider.id);
    let read_secret = |field: &str| {
        secrets::get_provider_secret(&secret_id, &endpoint, field)
            .map_err(FinalSubError::Validation)?
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| {
                FinalSubError::Validation(
                    "豆包声音复刻需要 APP ID 与 Access Token，请先在在线 TTS 实例中保存训练凭据"
                        .into(),
                )
            })
    };
    Ok((
        read_secret("cloneAppId")?,
        read_secret("cloneAccessToken")?,
        provider.timeout_seconds,
    ))
}

fn cloud_status_from_clone_state(state: super::volcengine_clone::CloneState) -> CloudVoiceStatus {
    match state {
        super::volcengine_clone::CloneState::Training => CloudVoiceStatus::Training,
        super::volcengine_clone::CloneState::Ready => CloudVoiceStatus::Ready,
        super::volcengine_clone::CloneState::Failed => CloudVoiceStatus::Failed,
    }
}

fn persist_cloud_profile(
    app_config_dir: &Path,
    profiles: &mut HashMap<String, VoiceProfile>,
    mut profile: VoiceProfile,
    reference_source: Option<&Path>,
) -> Result<VoiceProfile> {
    if profiles.len() >= MAX_PROFILES {
        return Err(FinalSubError::Validation(format!(
            "我的音色最多保存 {MAX_PROFILES} 个"
        )));
    }
    let target = profile_dir(app_config_dir, &profile.id)?;
    if target.exists() {
        return Err(FinalSubError::Validation("音色目录冲突，请重试".into()));
    }
    let staging = staging_root(app_config_dir).join(format!("cloud-{}", profile.id));
    if staging.exists() {
        std::fs::remove_dir_all(&staging)?;
    }
    std::fs::create_dir_all(&staging)?;
    let result = (|| -> Result<()> {
        if let Some(source) = reference_source {
            let metadata = std::fs::metadata(source)?;
            if !source.is_file() || metadata.len() == 0 || metadata.len() > MAX_REFERENCE_BYTES {
                return Err(FinalSubError::Validation(
                    "云端音色参考音频为空或超过 64 MB".into(),
                ));
            }
            std::fs::copy(source, staging.join(REFERENCE_FILE))?;
            profile.reference_audio_path = path_string(&target.join(REFERENCE_FILE));
        } else {
            profile.reference_audio_path.clear();
        }
        save_profile_in_dir(&staging, &profile)?;
        std::fs::rename(&staging, &target)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_dir_all(&staging);
    }
    result?;
    profiles.insert(profile.id.clone(), profile.clone());
    Ok(profile)
}

pub async fn create_cloud_voice_profile(
    app_config_dir: &Path,
    profiles: &mut HashMap<String, VoiceProfile>,
    request: CreateCloudVoiceProfileRequest,
) -> Result<VoiceProfile> {
    if !request.consent {
        return Err(FinalSubError::Validation(
            "请确认你拥有该声音的使用与克隆授权".into(),
        ));
    }
    if !request.upload_consent {
        return Err(FinalSubError::Validation(
            "请明确授权把参考音频上传到当前云端服务".into(),
        ));
    }
    if profiles.len() >= MAX_PROFILES {
        return Err(FinalSubError::Validation(format!(
            "我的音色最多保存 {MAX_PROFILES} 个"
        )));
    }
    let name = validate_name(&request.name)?;
    validate_uuid(&request.provider_id, "TTS 服务实例 ID")?;
    let provider = super::providers::find_provider(app_config_dir, &request.provider_id)?;
    let engine = cloud_engine_for_provider(&provider)?;
    let prepared = load_prepared(app_config_dir, &request.token)?;
    if prepared.engine != engine {
        return Err(FinalSubError::Validation(
            "该参考音频不是为当前云端引擎准备的，请重新分析".into(),
        ));
    }
    if prepared
        .quality
        .issues
        .iter()
        .any(|item| item.severity == VoiceQualityIssueSeverity::Error)
    {
        return Err(FinalSubError::Validation(
            "参考片段没有足够的有效语音，请重新选择片段".into(),
        ));
    }
    let source_directory = prepared_dir(app_config_dir, &request.token)?;
    let reference_path = source_directory.join(REFERENCE_FILE);
    let (cloud_voice_id, cloud_status, cloud_training_times_left) = match engine {
        VoiceCloneEngine::Elevenlabs => {
            let cloud_voice_id = super::providers::create_elevenlabs_voice(
                app_config_dir,
                &request.provider_id,
                &name,
                &reference_path,
                request.remove_background_noise,
            )
            .await?;
            if let Err(error) =
                ensure_cloud_voice_not_linked(profiles, &request.provider_id, &cloud_voice_id)
            {
                let _ = super::providers::delete_elevenlabs_voice(
                    app_config_dir,
                    &request.provider_id,
                    &cloud_voice_id,
                )
                .await;
                return Err(error);
            }
            (cloud_voice_id, CloudVoiceStatus::Ready, None)
        }
        VoiceCloneEngine::Volcengine => {
            let cloud_voice_id = validate_cloud_voice_id("volcengine", &request.voice_id)?;
            ensure_cloud_voice_not_linked(profiles, &request.provider_id, &cloud_voice_id)?;
            let (app_id, access_token, timeout_seconds) =
                volc_clone_credentials(app_config_dir, &request.provider_id)?;
            super::volcengine_clone::train(
                &app_id,
                &access_token,
                super::volcengine_clone::CloneTrainRequest {
                    speaker_id: &cloud_voice_id,
                    audio_path: &reference_path,
                    language: match request.language {
                        VoiceProfileLanguage::Zh => "zh",
                        VoiceProfileLanguage::En => "en",
                    },
                    remove_background_noise: request.remove_background_noise,
                    enable_mss: request.enable_mss,
                    timeout_seconds,
                },
            )
            .await?;
            let status = super::volcengine_clone::query_status(
                &app_id,
                &access_token,
                &cloud_voice_id,
                timeout_seconds,
            )
            .await
            .map(|value| {
                (
                    cloud_status_from_clone_state(value.state),
                    value.training_times_left,
                )
            })
            .unwrap_or((CloudVoiceStatus::Training, None));
            (cloud_voice_id, status.0, status.1)
        }
        VoiceCloneEngine::Zipvoice => {
            return Err(FinalSubError::Validation(
                "本地 ZipVoice 不能通过云端克隆接口创建音色".into(),
            ));
        }
    };
    let profile = VoiceProfile {
        id: Uuid::new_v4().to_string(),
        name,
        engine: match engine {
            VoiceCloneEngine::Elevenlabs => "elevenlabs",
            VoiceCloneEngine::Volcengine => "volcengine",
            VoiceCloneEngine::Zipvoice => "zipvoice",
        }
        .into(),
        language: request.language,
        reference_audio_path: String::new(),
        reference_text: String::new(),
        source_name: Some(prepared.source_name),
        quality: prepared.quality,
        provider_id: Some(request.provider_id),
        cloud_voice_id: Some(cloud_voice_id.clone()),
        cloud_status: Some(cloud_status),
        volc_training_times_left: cloud_training_times_left,
        created_at: chrono::Utc::now().timestamp_millis(),
    };
    let saved = persist_cloud_profile(
        app_config_dir,
        profiles,
        profile,
        Some(&reference_path),
    )
    .map_err(|error| {
        FinalSubError::Validation(format!(
            "云端音色 {cloud_voice_id} 已创建，但本机索引保存失败：{error}。请使用“找回云端音色”重新关联"
        ))
    })?;
    let _ = std::fs::remove_dir_all(source_directory);
    Ok(saved)
}

async fn link_cloud_voice_profile_with_volc_status<F, Fut>(
    app_config_dir: &Path,
    profiles: &mut HashMap<String, VoiceProfile>,
    request: LinkCloudVoiceProfileRequest,
    resolve_volc_status: F,
) -> Result<VoiceProfile>
where
    F: FnOnce(PathBuf, String, String) -> Fut,
    Fut: std::future::Future<Output = Result<super::volcengine_clone::CloneStatus>>,
{
    if !request.consent {
        return Err(FinalSubError::Validation(
            "请确认你拥有该声音的使用授权".into(),
        ));
    }
    let name = validate_name(&request.name)?;
    let provider = super::providers::find_provider(app_config_dir, &request.provider_id)?;
    let engine = match provider.protocol {
        super::providers::TtsProviderProtocol::Elevenlabs => "elevenlabs",
        super::providers::TtsProviderProtocol::Volcengine => "volcengine",
        _ => {
            return Err(FinalSubError::Validation(
                "只有 ElevenLabs 或豆包 TTS 实例可以关联克隆音色".into(),
            ))
        }
    };
    let cloud_voice_id = validate_cloud_voice_id(engine, &request.voice_id)?;
    ensure_cloud_voice_not_linked(profiles, &request.provider_id, &cloud_voice_id)?;
    let (cloud_status, volc_training_times_left) = if engine == "volcengine" {
        let status = resolve_volc_status(
            app_config_dir.to_path_buf(),
            request.provider_id.clone(),
            cloud_voice_id.clone(),
        )
        .await?;
        (
            cloud_status_from_clone_state(status.state),
            status.training_times_left,
        )
    } else {
        (CloudVoiceStatus::Ready, None)
    };
    persist_cloud_profile(
        app_config_dir,
        profiles,
        VoiceProfile {
            id: Uuid::new_v4().to_string(),
            name,
            engine: engine.into(),
            language: request.language,
            reference_audio_path: String::new(),
            reference_text: String::new(),
            source_name: None,
            quality: cloud_quality_placeholder(),
            provider_id: Some(request.provider_id),
            cloud_voice_id: Some(cloud_voice_id),
            cloud_status: Some(cloud_status),
            volc_training_times_left,
            created_at: chrono::Utc::now().timestamp_millis(),
        },
        None,
    )
}

pub async fn link_cloud_voice_profile(
    app_config_dir: &Path,
    profiles: &mut HashMap<String, VoiceProfile>,
    request: LinkCloudVoiceProfileRequest,
) -> Result<VoiceProfile> {
    link_cloud_voice_profile_with_volc_status(
        app_config_dir,
        profiles,
        request,
        |app_config_dir, provider_id, cloud_voice_id| async move {
            let (app_id, access_token, timeout_seconds) =
                volc_clone_credentials(&app_config_dir, &provider_id)?;
            super::volcengine_clone::query_status(
                &app_id,
                &access_token,
                &cloud_voice_id,
                timeout_seconds,
            )
            .await
        },
    )
    .await
}

pub async fn refresh_cloud_voice_status(
    app_config_dir: &Path,
    profiles: &mut HashMap<String, VoiceProfile>,
    id: &str,
) -> Result<VoiceProfile> {
    let profile_id = validate_uuid(id, "音色 ID")?.to_string();
    let mut profile = profiles
        .get(&profile_id)
        .cloned()
        .ok_or_else(|| FinalSubError::Validation("音色不存在".into()))?;
    if profile.engine != "volcengine" {
        return Ok(profile);
    }
    let provider_id = profile
        .provider_id
        .as_deref()
        .ok_or_else(|| FinalSubError::Validation("豆包音色缺少服务实例关联".into()))?;
    let voice_id = profile
        .cloud_voice_id
        .as_deref()
        .ok_or_else(|| FinalSubError::Validation("豆包音色缺少云端 ID".into()))?;
    let (app_id, access_token, timeout_seconds) =
        volc_clone_credentials(app_config_dir, provider_id)?;
    let status =
        super::volcengine_clone::query_status(&app_id, &access_token, voice_id, timeout_seconds)
            .await?;
    profile.cloud_status = Some(cloud_status_from_clone_state(status.state));
    profile.volc_training_times_left = status.training_times_left;
    let directory = profile_dir(app_config_dir, &profile.id)?;
    save_profile_in_dir(&directory, &profile)?;
    profiles.insert(profile.id.clone(), profile.clone());
    Ok(profile)
}

pub async fn retrain_cloud_voice_profile(
    app_config_dir: &Path,
    profiles: &mut HashMap<String, VoiceProfile>,
    request: RetrainCloudVoiceProfileRequest,
) -> Result<VoiceProfile> {
    let profile_id = validate_uuid(&request.id, "音色 ID")?.to_string();
    let mut profile = profiles
        .get(&profile_id)
        .cloned()
        .ok_or_else(|| FinalSubError::Validation("音色不存在".into()))?;
    if profile.engine != "volcengine" {
        return Err(FinalSubError::Validation(
            "只有豆包云端音色支持在本机复用参考音频重训".into(),
        ));
    }
    let provider_id = profile
        .provider_id
        .as_deref()
        .ok_or_else(|| FinalSubError::Validation("豆包音色缺少服务实例关联".into()))?;
    let voice_id = profile
        .cloud_voice_id
        .as_deref()
        .ok_or_else(|| FinalSubError::Validation("豆包音色缺少云端 ID".into()))?;
    let reference_path = PathBuf::from(profile.reference_audio_path.trim());
    if !reference_path.is_absolute() || !reference_path.is_file() {
        return Err(FinalSubError::Validation(
            "该豆包音色没有保留参考音频，无法一键重训；请重新创建或找回后再试".into(),
        ));
    }
    let (app_id, access_token, timeout_seconds) =
        volc_clone_credentials(app_config_dir, provider_id)?;
    super::volcengine_clone::train(
        &app_id,
        &access_token,
        super::volcengine_clone::CloneTrainRequest {
            speaker_id: voice_id,
            audio_path: &reference_path,
            language: match profile.language {
                VoiceProfileLanguage::Zh => "zh",
                VoiceProfileLanguage::En => "en",
            },
            remove_background_noise: request.remove_background_noise,
            enable_mss: request.enable_mss,
            timeout_seconds,
        },
    )
    .await?;
    let status =
        super::volcengine_clone::query_status(&app_id, &access_token, voice_id, timeout_seconds)
            .await
            .unwrap_or(super::volcengine_clone::CloneStatus {
                state: super::volcengine_clone::CloneState::Training,
                raw_status: None,
                training_times_left: None,
            });
    profile.cloud_status = Some(cloud_status_from_clone_state(status.state));
    profile.volc_training_times_left = status.training_times_left;
    save_profile_in_dir(&profile_dir(app_config_dir, &profile.id)?, &profile)?;
    profiles.insert(profile.id.clone(), profile.clone());
    Ok(profile)
}

pub async fn delete_cloud_voice_remote(
    app_config_dir: &Path,
    profile: &VoiceProfile,
) -> Result<()> {
    if profile.engine != "elevenlabs" {
        return Err(FinalSubError::Validation(
            "当前只支持从 ElevenLabs 永久删除云端音色；豆包音色请在火山控制台管理".into(),
        ));
    }
    let provider_id = profile
        .provider_id
        .as_deref()
        .ok_or_else(|| FinalSubError::Validation("音色缺少服务实例关联".into()))?;
    let cloud_voice_id = profile
        .cloud_voice_id
        .as_deref()
        .ok_or_else(|| FinalSubError::Validation("音色缺少云端 ID".into()))?;
    super::providers::delete_elevenlabs_voice(app_config_dir, provider_id, cloud_voice_id).await
}

pub fn rename_voice_profile(
    app_config_dir: &Path,
    profiles: &mut HashMap<String, VoiceProfile>,
    id: &str,
    name: &str,
) -> Result<VoiceProfile> {
    let name = validate_name(name)?;
    let mut profile = profiles
        .get(id)
        .cloned()
        .ok_or_else(|| FinalSubError::Validation("音色不存在".into()))?;
    profile.name = name;
    save_profile_in_dir(&profile_dir(app_config_dir, id)?, &profile)?;
    profiles.insert(id.to_string(), profile.clone());
    Ok(profile)
}

pub fn remove_voice_profile(
    app_config_dir: &Path,
    profiles: &mut HashMap<String, VoiceProfile>,
    id: &str,
) -> Result<()> {
    validate_uuid(id, "音色 ID")?;
    if !profiles.contains_key(id) {
        return Err(FinalSubError::Validation("音色不存在".into()));
    }
    let source = profile_dir(app_config_dir, id)?;
    let trash = trash_root(app_config_dir);
    std::fs::create_dir_all(&trash)?;
    let destination = trash.join(format!(
        "{}-{}",
        id,
        chrono::Utc::now().format("%Y%m%d%H%M%S")
    ));
    std::fs::rename(source, destination)?;
    profiles.remove(id);
    Ok(())
}

pub fn save_voice_recording(
    app_config_dir: &Path,
    data_base64: &str,
    mime_type: &str,
) -> Result<String> {
    let approximate_bytes = data_base64.len().saturating_mul(3) / 4;
    if data_base64.is_empty() || approximate_bytes > MAX_RECORDING_BYTES {
        return Err(FinalSubError::Validation(
            "录音为空或超过 16 MB，请缩短后重试".into(),
        ));
    }
    let extension = if mime_type.starts_with("audio/webm") {
        "webm"
    } else if mime_type.starts_with("audio/mp4") {
        "m4a"
    } else if mime_type.starts_with("audio/ogg") {
        "ogg"
    } else {
        return Err(FinalSubError::Validation("当前录音格式不受支持".into()));
    };
    let bytes = BASE64
        .decode(data_base64)
        .map_err(|_| FinalSubError::Validation("录音数据不是有效 Base64".into()))?;
    if bytes.is_empty() || bytes.len() > MAX_RECORDING_BYTES {
        return Err(FinalSubError::Validation(
            "录音为空或超过 16 MB，请缩短后重试".into(),
        ));
    }
    let root = recordings_root(app_config_dir);
    std::fs::create_dir_all(&root)?;
    let path = root.join(format!("{}.{}", Uuid::new_v4(), extension));
    std::fs::write(&path, bytes)?;
    Ok(path_string(&path))
}

pub fn discard_voice_recording(app_config_dir: &Path, raw_path: &str) -> Result<()> {
    let path = PathBuf::from(raw_path.trim());
    if !path.is_absolute() || !path.is_file() {
        return Ok(());
    }
    let canonical = std::fs::canonicalize(&path)?;
    let root = recordings_root(app_config_dir);
    std::fs::create_dir_all(&root)?;
    let canonical_root = std::fs::canonicalize(root)?;
    if canonical.parent() != Some(canonical_root.as_path()) {
        return Err(FinalSubError::Validation(
            "只能清理 FinalSub 创建的临时录音".into(),
        ));
    }
    std::fs::remove_file(canonical)?;
    Ok(())
}

pub fn export_voice_profile(profile: &VoiceProfile, output_path: &str) -> Result<String> {
    if !matches!(
        profile.engine.as_str(),
        "zipvoice" | "elevenlabs" | "volcengine"
    ) {
        return Err(FinalSubError::Validation("未知音色引擎，无法导出".into()));
    }
    let output = PathBuf::from(output_path.trim());
    if !output.is_absolute()
        || !output
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|value| value.eq_ignore_ascii_case("svoice"))
    {
        return Err(FinalSubError::Validation(
            "音色包输出必须是绝对 .svoice 路径".into(),
        ));
    }
    let reference_path = PathBuf::from(&profile.reference_audio_path);
    let ref_wav_base64 = if profile.reference_audio_path.trim().is_empty() {
        if profile.engine == "zipvoice" {
            return Err(FinalSubError::Validation("本地音色参考音频缺失".into()));
        }
        None
    } else {
        if !reference_path.is_file()
            || std::fs::metadata(&reference_path)?.len() > MAX_REFERENCE_BYTES
        {
            return Err(FinalSubError::Validation(
                "音色参考音频缺失或超过 64 MB".into(),
            ));
        }
        Some(BASE64.encode(std::fs::read(reference_path)?))
    };
    let quality = &profile.quality;
    let package = SvoiceExportPackage {
        format: SVOICE_FORMAT,
        version: SVOICE_VERSION,
        voice: SvoiceExportVoice {
            name: &profile.name,
            engine: &profile.engine,
            language: profile.language,
            ref_text: &profile.reference_text,
            speaker_id: profile.cloud_voice_id.as_deref(),
            quality: SvoiceQualityReport {
                duration_ms: quality.duration_ms,
                speech_ms: quality.speech_ms,
                speech_ratio: quality.speech_ratio,
                longest_silence_ms: quality.longest_silence_ms,
                rms_db: quality.rms_db,
                peak_db: quality.peak_db,
                clipping_ratio: quality.clipping_ratio,
                snr_db: quality.snr_db,
                verdict: quality.verdict,
                issues: &quality.issues,
            },
            created_at: profile.created_at,
        },
        ref_wav_base64,
    };
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temporary = output.with_extension("svoice.tmp");
    let bytes = serde_json::to_vec(&package)?;
    if bytes.len() as u64 > MAX_PACKAGE_BYTES {
        return Err(FinalSubError::Validation("音色包超过 96 MB 限制".into()));
    }
    std::fs::write(&temporary, bytes)?;
    std::fs::rename(&temporary, &output)?;
    Ok(path_string(&output))
}

fn imported_quality_or_placeholder(
    quality: Option<SvoiceImportQualityReport>,
) -> VoiceQualityReport {
    quality
        .map(|value| VoiceQualityReport {
            duration_ms: value.duration_ms,
            speech_ms: value.speech_ms,
            speech_ratio: value.speech_ratio,
            longest_silence_ms: value.longest_silence_ms,
            rms_db: value.rms_db,
            peak_db: value.peak_db,
            clipping_ratio: value.clipping_ratio,
            snr_db: value.snr_db,
            verdict: value.verdict,
            issues: value.issues,
        })
        .unwrap_or_else(cloud_quality_placeholder)
}

fn provider_for_imported_cloud_voice(app_config_dir: &Path, engine: &str) -> Result<String> {
    let protocol = match engine {
        "elevenlabs" => super::providers::TtsProviderProtocol::Elevenlabs,
        "volcengine" => super::providers::TtsProviderProtocol::Volcengine,
        _ => {
            return Err(FinalSubError::Validation(
                "音色包中的云端引擎不受支持".into(),
            ))
        }
    };
    super::providers::list_providers(app_config_dir)?
        .into_iter()
        .find(|provider| provider.protocol == protocol)
        .map(|provider| provider.id)
        .ok_or_else(|| {
            FinalSubError::Validation(format!(
                "本机尚未配置 {engine} 在线 TTS 实例，请先配置后再导入音色包"
            ))
        })
}

fn decode_imported_reference(
    app_config_dir: &Path,
    encoded: Option<String>,
) -> Result<Option<PathBuf>> {
    let Some(encoded) = encoded else {
        return Ok(None);
    };
    if encoded.len().saturating_mul(3) / 4 > MAX_REFERENCE_BYTES as usize {
        return Err(FinalSubError::Validation("音色包参考音频超过 64 MB".into()));
    }
    let bytes = BASE64
        .decode(encoded)
        .map_err(|_| FinalSubError::Validation("音色包参考音频 Base64 无效".into()))?;
    if bytes.is_empty() || bytes.len() as u64 > MAX_REFERENCE_BYTES {
        return Err(FinalSubError::Validation(
            "音色包参考音频为空或超过 64 MB".into(),
        ));
    }
    let root = recordings_root(app_config_dir);
    std::fs::create_dir_all(&root)?;
    let path = root.join(format!("{}.wav", Uuid::new_v4()));
    std::fs::write(&path, bytes)?;
    if Wave::read(&path_string(&path)).is_none() {
        let _ = std::fs::remove_file(&path);
        return Err(FinalSubError::Validation(
            "音色包内不是有效 WAV 音频".into(),
        ));
    }
    Ok(Some(path))
}

pub async fn import_voice_profile(
    app_config_dir: &Path,
    ffmpeg_path: &Path,
    vad_model_path: &Path,
    profiles: &mut HashMap<String, VoiceProfile>,
    input_path: &str,
) -> Result<VoiceProfile> {
    let input = PathBuf::from(input_path.trim());
    if !input.is_absolute()
        || !input.is_file()
        || !input
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|value| value.eq_ignore_ascii_case("svoice"))
    {
        return Err(FinalSubError::Validation(
            "请选择存在的 .svoice 音色包".into(),
        ));
    }
    let package: SvoiceImportPackage = read_json_bounded(&input, MAX_PACKAGE_BYTES)?;
    if package.format != SVOICE_FORMAT || package.version != SVOICE_VERSION {
        return Err(FinalSubError::Validation("音色包格式或版本不受支持".into()));
    }
    if package.voice.engine != "zipvoice" {
        let engine = package.voice.engine.as_str();
        let cloud_voice_id = validate_cloud_voice_id(
            engine,
            package.voice.speaker_id.as_deref().unwrap_or_default(),
        )?;
        let provider_id = provider_for_imported_cloud_voice(app_config_dir, engine)?;
        ensure_cloud_voice_not_linked(profiles, &provider_id, &cloud_voice_id)?;
        let temporary_reference =
            decode_imported_reference(app_config_dir, package.ref_wav_base64)?;
        let source_name = input
            .file_stem()
            .and_then(|value| value.to_str())
            .map(str::to_string);
        let profile = VoiceProfile {
            id: Uuid::new_v4().to_string(),
            name: validate_name(&package.voice.name)?,
            engine: engine.to_string(),
            language: package.voice.language,
            reference_audio_path: String::new(),
            reference_text: String::new(),
            source_name,
            quality: imported_quality_or_placeholder(package.voice.quality),
            provider_id: Some(provider_id),
            cloud_voice_id: Some(cloud_voice_id),
            cloud_status: Some(CloudVoiceStatus::Ready),
            volc_training_times_left: None,
            created_at: chrono::Utc::now().timestamp_millis(),
        };
        let result = persist_cloud_profile(
            app_config_dir,
            profiles,
            profile,
            temporary_reference.as_deref(),
        );
        if let Some(path) = temporary_reference {
            let _ = std::fs::remove_file(path);
        }
        return result;
    }
    let name = validate_name(&package.voice.name)?;
    let reference_text =
        validate_reference_text(package.voice.ref_text.as_deref().unwrap_or_default())?;
    let encoded = package
        .ref_wav_base64
        .ok_or_else(|| FinalSubError::Validation("音色包缺少参考音频".into()))?;
    if encoded.len().saturating_mul(3) / 4 > MAX_REFERENCE_BYTES as usize {
        return Err(FinalSubError::Validation("音色包参考音频超过 64 MB".into()));
    }
    let bytes = BASE64
        .decode(encoded)
        .map_err(|_| FinalSubError::Validation("音色包参考音频 Base64 无效".into()))?;
    if bytes.is_empty() || bytes.len() as u64 > MAX_REFERENCE_BYTES {
        return Err(FinalSubError::Validation(
            "音色包参考音频为空或超过 64 MB".into(),
        ));
    }
    let root = recordings_root(app_config_dir);
    std::fs::create_dir_all(&root)?;
    let temporary_source = root.join(format!("{}.wav", Uuid::new_v4()));
    std::fs::write(&temporary_source, bytes)?;
    let source_duration_result = (|| -> Result<u64> {
        let wave = Wave::read(&path_string(&temporary_source))
            .ok_or_else(|| FinalSubError::Validation("音色包内不是有效 WAV 音频".into()))?;
        if wave.samples().is_empty() || wave.sample_rate() <= 0 {
            return Err(FinalSubError::Validation("音色包内 WAV 音频为空".into()));
        }
        Ok((wave.samples().len() as u128 * 1_000 / wave.sample_rate() as u128) as u64)
    })();
    let source_duration = match source_duration_result {
        Ok(value) => value,
        Err(error) => {
            let _ = std::fs::remove_file(&temporary_source);
            return Err(error);
        }
    };
    if source_duration > ZIPVOICE_MAX_SELECTION_MS + 100 {
        let _ = std::fs::remove_file(&temporary_source);
        return Err(FinalSubError::Validation(
            "ZipVoice 音色包的参考音频不能超过 10 秒".into(),
        ));
    }
    let prepared_result = prepare_voice_sample(
        app_config_dir,
        ffmpeg_path,
        vad_model_path,
        PrepareVoiceSampleRequest {
            source_path: path_string(&temporary_source),
            start_ms: 0,
            duration_ms: source_duration
                .clamp(ZIPVOICE_MIN_SELECTION_MS, ZIPVOICE_MAX_SELECTION_MS),
            engine: VoiceCloneEngine::Zipvoice,
            local_denoise: false,
        },
    )
    .await;
    let _ = std::fs::remove_file(&temporary_source);
    let prepared = prepared_result?;
    if !prepared.can_create {
        let _ = discard_prepared_voice_sample(app_config_dir, &prepared.token);
        return Err(FinalSubError::Validation(
            "音色包参考音频没有足够的有效语音".into(),
        ));
    }
    create_voice_profile(
        app_config_dir,
        profiles,
        CreateVoiceProfileRequest {
            token: prepared.token,
            name,
            language: package.voice.language,
            reference_text,
            consent: true,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report() -> VoiceQualityReport {
        VoiceQualityReport {
            duration_ms: 8_000,
            speech_ms: 7_000,
            speech_ratio: 0.875,
            longest_silence_ms: 400,
            rms_db: -20.0,
            peak_db: -3.0,
            clipping_ratio: 0.0,
            snr_db: 24.0,
            verdict: VoiceQualityVerdict::Good,
            issues: Vec::new(),
        }
    }

    #[test]
    fn profile_store_roundtrips_and_delete_is_recoverable() {
        let root = tempfile::tempdir().unwrap();
        let id = Uuid::new_v4().to_string();
        let directory = profile_dir(root.path(), &id).unwrap();
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join(REFERENCE_FILE), b"fixture").unwrap();
        let profile = VoiceProfile {
            id: id.clone(),
            name: "旁白".into(),
            engine: "zipvoice".into(),
            language: VoiceProfileLanguage::Zh,
            reference_audio_path: path_string(&directory.join(REFERENCE_FILE)),
            reference_text: "这是参考文本".into(),
            source_name: Some("voice.wav".into()),
            quality: report(),
            provider_id: None,
            cloud_voice_id: None,
            cloud_status: None,
            volc_training_times_left: None,
            created_at: 1,
        };
        save_profile_in_dir(&directory, &profile).unwrap();
        let mut loaded = load_profiles(root.path()).unwrap();
        assert_eq!(loaded.get(&id).unwrap().name, "旁白");
        let renamed = rename_voice_profile(root.path(), &mut loaded, &id, "主讲人").unwrap();
        assert_eq!(renamed.name, "主讲人");
        remove_voice_profile(root.path(), &mut loaded, &id).unwrap();
        assert!(!loaded.contains_key(&id));
        assert!(!directory.exists());
        assert_eq!(
            std::fs::read_dir(trash_root(root.path())).unwrap().count(),
            1
        );
    }

    #[test]
    fn rejects_untrusted_names_text_and_recording_formats() {
        assert!(validate_name("").is_err());
        assert!(validate_name("bad\nname").is_err());
        assert!(validate_reference_text("  ").is_err());
        let root = tempfile::tempdir().unwrap();
        assert!(save_voice_recording(root.path(), "AAAA", "application/octet-stream").is_err());
    }

    #[test]
    fn svoice_export_is_smartsub_v1_compatible() {
        let root = tempfile::tempdir().unwrap();
        let reference = root.path().join("ref.wav");
        std::fs::write(&reference, b"wav-fixture").unwrap();
        let profile = VoiceProfile {
            id: Uuid::new_v4().to_string(),
            name: "Narrator".into(),
            engine: "zipvoice".into(),
            language: VoiceProfileLanguage::En,
            reference_audio_path: path_string(&reference),
            reference_text: "The exact transcript.".into(),
            source_name: None,
            quality: report(),
            provider_id: None,
            cloud_voice_id: None,
            cloud_status: None,
            volc_training_times_left: None,
            created_at: 1,
        };
        let output = root.path().join("voice.svoice");
        export_voice_profile(&profile, &path_string(&output)).unwrap();
        let value: serde_json::Value =
            serde_json::from_slice(&std::fs::read(output).unwrap()).unwrap();
        assert_eq!(value["format"], SVOICE_FORMAT);
        assert_eq!(value["version"], 1);
        assert_eq!(value["voice"]["engine"], "zipvoice");
        assert_eq!(value["voice"]["refText"], "The exact transcript.");
        assert!(value["refWavBase64"].is_string());
    }

    #[test]
    fn cloud_profile_loads_without_reference_audio() {
        let root = tempfile::tempdir().unwrap();
        let id = Uuid::new_v4().to_string();
        let provider_id = Uuid::new_v4().to_string();
        let profile = VoiceProfile {
            id: id.clone(),
            name: "Cloud narrator".into(),
            engine: "elevenlabs".into(),
            language: VoiceProfileLanguage::En,
            reference_audio_path: "/untrusted/stale/path.wav".into(),
            reference_text: "must not persist".into(),
            source_name: None,
            quality: cloud_quality_placeholder(),
            provider_id: Some(provider_id.clone()),
            cloud_voice_id: Some("voice-123".into()),
            cloud_status: Some(CloudVoiceStatus::Ready),
            volc_training_times_left: None,
            created_at: 1,
        };
        let directory = profile_dir(root.path(), &id).unwrap();
        std::fs::create_dir_all(&directory).unwrap();
        save_profile_in_dir(&directory, &profile).unwrap();
        let loaded = load_profiles(root.path()).unwrap();
        let loaded = loaded.get(&id).unwrap();
        assert_eq!(loaded.provider_id.as_deref(), Some(provider_id.as_str()));
        assert_eq!(loaded.cloud_voice_id.as_deref(), Some("voice-123"));
        assert!(loaded.reference_audio_path.is_empty());
        assert!(loaded.reference_text.is_empty());
    }

    #[test]
    fn cloud_voice_ids_are_engine_scoped() {
        assert_eq!(
            validate_cloud_voice_id("elevenlabs", " voice-123 ").unwrap(),
            "voice-123"
        );
        assert_eq!(
            validate_cloud_voice_id("volcengine", "S_example").unwrap(),
            "S_example"
        );
        assert!(validate_cloud_voice_id("volcengine", "voice-123").is_err());
        assert!(validate_cloud_voice_id("elevenlabs", "bad\nvoice").is_err());
    }

    fn save_test_volc_provider(app_config_dir: &Path) -> String {
        crate::core::tts::providers::save_provider(
            app_config_dir,
            crate::core::tts::providers::SaveTtsProviderRequest {
                id: None,
                name: "Doubao clone test".into(),
                protocol: crate::core::tts::providers::TtsProviderProtocol::Volcengine,
                endpoint: String::new(),
                model: String::new(),
                voice: "BV001_streaming".into(),
                region: String::new(),
                resource_id: crate::core::tts::volcengine::DEFAULT_RESOURCE_ID.into(),
                text_upload_consent: true,
                timeout_seconds: 60,
                request_concurrency: 1,
            },
        )
        .unwrap()
        .id
    }

    fn link_request(provider_id: String) -> LinkCloudVoiceProfileRequest {
        LinkCloudVoiceProfileRequest {
            name: "Recovered narrator".into(),
            language: VoiceProfileLanguage::Zh,
            provider_id,
            voice_id: "S_recovery_test".into(),
            consent: true,
        }
    }

    #[tokio::test]
    async fn failed_volc_recovery_does_not_persist_a_local_profile() {
        let root = tempfile::tempdir().unwrap();
        let provider_id = save_test_volc_provider(root.path());
        let mut profiles = HashMap::new();
        let result = link_cloud_voice_profile_with_volc_status(
            root.path(),
            &mut profiles,
            link_request(provider_id),
            |_, _, _| async {
                Err(FinalSubError::Validation(
                    "remote slot is unavailable".into(),
                ))
            },
        )
        .await;

        assert!(result.is_err());
        assert!(profiles.is_empty());
        assert!(!voice_root(root.path()).exists());
    }

    #[tokio::test]
    async fn recovered_volc_profile_persists_verified_remote_status() {
        let root = tempfile::tempdir().unwrap();
        let provider_id = save_test_volc_provider(root.path());
        let mut profiles = HashMap::new();
        let saved = link_cloud_voice_profile_with_volc_status(
            root.path(),
            &mut profiles,
            link_request(provider_id),
            |_, _, _| async {
                Ok(super::super::volcengine_clone::CloneStatus {
                    state: super::super::volcengine_clone::CloneState::Training,
                    raw_status: Some(1),
                    training_times_left: Some(2),
                })
            },
        )
        .await
        .unwrap();

        assert_eq!(saved.cloud_status, Some(CloudVoiceStatus::Training));
        assert_eq!(saved.volc_training_times_left, Some(2));
        let loaded = load_profiles(root.path()).unwrap();
        assert_eq!(
            loaded.get(&saved.id).unwrap().cloud_status,
            saved.cloud_status
        );
    }

    #[test]
    fn subtitle_cues_are_loaded_for_reference_selection() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("reference.srt");
        std::fs::write(
            &path,
            "1\n00:00:01,000 --> 00:00:02,500\n第一句\n\n2\n00:00:02,700 --> 00:00:04,000\n第二句\n",
        )
        .unwrap();
        let cues = list_voice_subtitle_cues(&path_string(&path)).unwrap();
        assert_eq!(cues.len(), 2);
        assert_eq!(cues[0].start_ms, 1_000);
        assert_eq!(cues[1].text, "第二句");
    }

    #[test]
    fn cloud_svoice_export_keeps_engine_and_remote_id() {
        let root = tempfile::tempdir().unwrap();
        let profile = VoiceProfile {
            id: Uuid::new_v4().to_string(),
            name: "Doubao narrator".into(),
            engine: "volcengine".into(),
            language: VoiceProfileLanguage::Zh,
            reference_audio_path: String::new(),
            reference_text: String::new(),
            source_name: None,
            quality: cloud_quality_placeholder(),
            provider_id: Some(Uuid::new_v4().to_string()),
            cloud_voice_id: Some("S_demo_001".into()),
            cloud_status: Some(CloudVoiceStatus::Ready),
            volc_training_times_left: Some(2),
            created_at: 1,
        };
        let output = root.path().join("cloud.svoice");
        export_voice_profile(&profile, &path_string(&output)).unwrap();
        let value: serde_json::Value =
            serde_json::from_slice(&std::fs::read(output).unwrap()).unwrap();
        assert_eq!(value["voice"]["engine"], "volcengine");
        assert_eq!(value["voice"]["speakerId"], "S_demo_001");
        assert!(value.get("refWavBase64").is_none());
    }
}
