use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use serde::{Deserialize, Serialize};
use sherpa_onnx::Wave;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::process::Command;
use uuid::Uuid;

use crate::core::asr::vad::{detect_speech, SAMPLE_RATE as VAD_SAMPLE_RATE};
use crate::core::audio;
use crate::error::{FinalSubError, Result};

const MAX_PROFILES: usize = 100;
const MAX_SOURCE_BYTES: u64 = 8 * 1024 * 1024 * 1024;
const MAX_REFERENCE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_PACKAGE_BYTES: u64 = 96 * 1024 * 1024;
const MAX_RECORDING_BYTES: usize = 16 * 1024 * 1024;
const MAX_NAME_CHARS: usize = 60;
const MAX_REFERENCE_TEXT_BYTES: usize = 4_000;
const MIN_SELECTION_MS: u64 = 3_000;
const IDEAL_SELECTION_MIN_MS: u64 = 5_000;
const DEFAULT_SELECTION_MS: u64 = 8_000;
const MAX_SELECTION_MS: u64 = 10_000;
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
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VoiceSourceInfo {
    pub path: String,
    pub file_name: String,
    pub duration_ms: u64,
    pub default_selection_ms: u64,
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
}

#[derive(Debug, Clone, Deserialize)]
pub struct PrepareVoiceSampleRequest {
    pub source_path: String,
    pub start_ms: u64,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateVoiceProfileRequest {
    pub token: String,
    pub name: String,
    pub language: VoiceProfileLanguage,
    pub reference_text: String,
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
    engine: &'static str,
    language: VoiceProfileLanguage,
    ref_text: &'a str,
    quality: SvoiceQualityReport<'a>,
    created_at: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SvoiceExportPackage<'a> {
    format: &'static str,
    version: u32,
    voice: SvoiceExportVoice<'a>,
    ref_wav_base64: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SvoiceImportVoice {
    name: String,
    engine: String,
    language: VoiceProfileLanguage,
    ref_text: Option<String>,
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
        let reference = entry.path().join(REFERENCE_FILE);
        if profile.id != id || profile.engine != "zipvoice" || !reference.is_file() {
            continue;
        }
        profile.reference_audio_path = path_string(&reference);
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
        default_selection_ms: duration_ms.min(DEFAULT_SELECTION_MS),
    })
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

fn analyze_reference(path: &Path, vad_model_path: &Path) -> Result<VoiceQualityReport> {
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
    } else if speech_ms < MIN_SELECTION_MS {
        issue(
            VoiceQualityIssueCode::TooShort,
            VoiceQualityIssueSeverity::Error,
            Some(speech_ms as f64),
        );
    } else if speech_ms < IDEAL_SELECTION_MIN_MS {
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
    if !(MIN_SELECTION_MS..=MAX_SELECTION_MS).contains(&request.duration_ms) {
        return Err(FinalSubError::Validation(format!(
            "声音克隆片段必须在 {}-{} 秒之间",
            MIN_SELECTION_MS / 1_000,
            MAX_SELECTION_MS / 1_000
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
        let quality =
            tokio::task::spawn_blocking(move || analyze_reference(&quality_path, &vad_path))
                .await
                .map_err(|error| {
                    FinalSubError::Validation(format!("音色质检线程失败：{error}"))
                })??;

        let mut reference_args = common;
        reference_args.extend([
            "-af".into(),
            "loudnorm=I=-20:LRA=7:TP=-3".into(),
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
    if !reference_path.is_file() || std::fs::metadata(&reference_path)?.len() > MAX_REFERENCE_BYTES
    {
        return Err(FinalSubError::Validation(
            "音色参考音频缺失或超过 64 MB".into(),
        ));
    }
    let quality = &profile.quality;
    let package = SvoiceExportPackage {
        format: SVOICE_FORMAT,
        version: SVOICE_VERSION,
        voice: SvoiceExportVoice {
            name: &profile.name,
            engine: "zipvoice",
            language: profile.language,
            ref_text: &profile.reference_text,
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
        ref_wav_base64: BASE64.encode(std::fs::read(reference_path)?),
    };
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temporary = output.with_extension("svoice.tmp");
    std::fs::write(&temporary, serde_json::to_vec(&package)?)?;
    std::fs::rename(&temporary, &output)?;
    Ok(path_string(&output))
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
        return Err(FinalSubError::Validation(
            "当前只支持导入可离线复用的 ZipVoice 音色包".into(),
        ));
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
    if source_duration > MAX_SELECTION_MS + 100 {
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
            duration_ms: source_duration.clamp(MIN_SELECTION_MS, MAX_SELECTION_MS),
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
}
