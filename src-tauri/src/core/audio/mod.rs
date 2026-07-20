use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::Duration;

use tokio::process::Command;

/// 视频重编码方式。`Auto` 只在真实探测通过时使用硬件编码，否则回退到
/// 既有的 libx264；`Hardware` 与 Auto 共享同一套安全回退逻辑。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum VideoEncoderMode {
    Auto,
    #[default]
    Cpu,
    Hardware,
}

impl VideoEncoderMode {
    pub fn parse(value: Option<&str>) -> Result<Self, String> {
        match value.unwrap_or("cpu").trim().to_ascii_lowercase().as_str() {
            "auto" => Ok(Self::Auto),
            "cpu" | "software" | "libx264" => Ok(Self::Cpu),
            "hardware" | "hw" => Ok(Self::Hardware),
            _ => Err("Video encoder mode only supports auto, cpu, or hardware".into()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HardwareRateMode {
    Cq,
    Bitrate,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HardwareEncoderInfo {
    pub available: bool,
    pub encoder_id: Option<String>,
    pub encoder_label: Option<String>,
    pub rate_mode: Option<HardwareRateMode>,
    pub platform_supported: bool,
}

impl HardwareEncoderInfo {
    fn unavailable(platform_supported: bool) -> Self {
        Self {
            available: false,
            encoder_id: None,
            encoder_label: None,
            rate_mode: None,
            platform_supported,
        }
    }
}

/// 已解析的视频编码参数。只有硬件探测成功时 `hardware` 才为 true；调用方
/// 可以据此在真正执行失败后安全重跑 CPU 编码。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoEncoding {
    pub args: Vec<String>,
    pub needs_nv12: bool,
    pub hardware: bool,
    pub encoder_id: String,
}

static HARDWARE_ENCODER_CACHE: tokio::sync::OnceCell<HardwareEncoderInfo> =
    tokio::sync::OnceCell::const_new();

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioExtractPlan {
    pub ffmpeg_bin: String,
    pub args: Vec<String>,
    pub input: String,
    pub output: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BurnInPlan {
    pub ffmpeg_bin: String,
    pub args: Vec<String>,
    pub video_input: String,
    pub subtitle_input: String,
    pub output: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BurnInStyleOptions {
    pub font_name: Option<String>,
    pub font_size: Option<u32>,
    pub font_color: Option<String>,
    pub outline_color: Option<String>,
    pub outline_width: Option<f32>,
    pub shadow: Option<f32>,
    pub background_color: Option<String>,
    pub opaque_background: Option<bool>,
    pub alignment: Option<u8>,
    pub margin_v: Option<u32>,
    pub crf: Option<u8>,
    pub preset: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum ComposeAudioMode {
    #[default]
    Keep,
    Replace,
    Mix,
    AddTrack,
}

impl ComposeAudioMode {
    pub fn parse(value: Option<&str>) -> Result<Self, String> {
        match value.unwrap_or("keep").trim() {
            "keep" => Ok(Self::Keep),
            "replace" => Ok(Self::Replace),
            "mix" => Ok(Self::Mix),
            "add-track" | "addTrack" => Ok(Self::AddTrack),
            _ => Err("Audio mode only supports keep, replace, mix, or add-track".into()),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ComposeOptions {
    pub soft_subtitle: bool,
    pub audio_mode: ComposeAudioMode,
    /// 已解析的视频编码参数；`None` 保持历史 CPU 编码行为。
    pub video_encoding: Option<VideoEncoding>,
    pub audio_path: Option<String>,
    pub subtitle_language: Option<String>,
    pub subtitle_title: Option<String>,
    pub audio_language: Option<String>,
    pub audio_title: Option<String>,
    pub original_audio_tracks: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FfmpegProgress {
    pub phase: String,
    pub percent: Option<f32>,
    pub message: String,
}

fn hardware_encoder_candidates() -> &'static [&'static str] {
    if cfg!(target_os = "macos") {
        &["h264_videotoolbox"]
    } else if cfg!(target_os = "windows") {
        &["h264_nvenc", "h264_qsv"]
    } else {
        &[]
    }
}

fn hardware_encoder_label(encoder_id: &str) -> &'static str {
    match encoder_id {
        "h264_videotoolbox" => "VideoToolbox",
        "h264_nvenc" => "NVIDIA NVENC",
        "h264_qsv" => "Intel QSV",
        _ => "Hardware encoder",
    }
}

fn hardware_cq_args(encoder_id: &str, quality: u8) -> Option<Vec<String>> {
    let quality = quality.to_string();
    match encoder_id {
        "h264_videotoolbox" => Some(vec![
            "-c:v".into(),
            encoder_id.into(),
            "-q:v".into(),
            quality,
            "-realtime".into(),
            "0".into(),
        ]),
        "h264_nvenc" => Some(vec![
            "-c:v".into(),
            encoder_id.into(),
            "-rc".into(),
            "vbr".into(),
            "-cq".into(),
            quality,
            "-b:v".into(),
            "0".into(),
            "-preset".into(),
            "p5".into(),
        ]),
        "h264_qsv" => Some(vec![
            "-c:v".into(),
            encoder_id.into(),
            "-global_quality".into(),
            quality,
        ]),
        _ => None,
    }
}

fn videotoolbox_bitrate_args(target: u64, maxrate: u64, bufsize: u64) -> Vec<String> {
    vec![
        "-c:v".into(),
        "h264_videotoolbox".into(),
        "-b:v".into(),
        target.to_string(),
        "-maxrate".into(),
        maxrate.to_string(),
        "-bufsize".into(),
        bufsize.to_string(),
        "-realtime".into(),
        "0".into(),
    ]
}

async fn probe_encoder(ffmpeg_bin: &Path, args: &[String]) -> bool {
    let mut command_args = vec![
        "-hide_banner".into(),
        "-f".into(),
        "lavfi".into(),
        "-i".into(),
        "color=black:s=640x360:d=0.1".into(),
        "-vf".into(),
        "format=nv12".into(),
    ];
    command_args.extend(args.iter().cloned());
    command_args.extend(["-f".into(), "null".into(), "-".into()]);

    let mut command = Command::new(ffmpeg_bin);
    command.args(command_args).kill_on_drop(true);
    let result = tokio::time::timeout(Duration::from_secs(15), command.output()).await;
    matches!(result, Ok(Ok(output)) if output.status.success())
}

async fn detect_hardware_encoder(ffmpeg_bin: &Path) -> HardwareEncoderInfo {
    let candidates = hardware_encoder_candidates();
    if candidates.is_empty() {
        return HardwareEncoderInfo::unavailable(false);
    }

    for encoder_id in candidates {
        // 取中高质量档做真实试编码；只检查参数/驱动能否跑通，正式质量由
        // resolve_video_encoding 根据用户的 CRF 映射。
        let cq = if *encoder_id == "h264_videotoolbox" {
            58
        } else {
            21
        };
        if let Some(args) = hardware_cq_args(encoder_id, cq) {
            if probe_encoder(ffmpeg_bin, &args).await {
                return HardwareEncoderInfo {
                    available: true,
                    encoder_id: Some((*encoder_id).to_string()),
                    encoder_label: Some(hardware_encoder_label(encoder_id).to_string()),
                    rate_mode: Some(HardwareRateMode::Cq),
                    platform_supported: true,
                };
            }
        }

        // Intel Mac 的部分 VideoToolbox 驱动不接受 -q:v；码率模式仍可用。
        if *encoder_id == "h264_videotoolbox"
            && probe_encoder(
                ffmpeg_bin,
                &videotoolbox_bitrate_args(2_000_000, 3_000_000, 4_000_000),
            )
            .await
        {
            return HardwareEncoderInfo {
                available: true,
                encoder_id: Some((*encoder_id).to_string()),
                encoder_label: Some(hardware_encoder_label(encoder_id).to_string()),
                rate_mode: Some(HardwareRateMode::Bitrate),
                platform_supported: true,
            };
        }
    }

    HardwareEncoderInfo::unavailable(true)
}

/// 获取本次应用会话的硬件编码能力。探测只执行一次，避免每次合成或打开
/// 设置页都启动 FFmpeg；不把结果持久化，防止用户更换驱动后使用过期能力。
pub async fn get_hardware_encoder_info(ffmpeg_bin: &Path) -> HardwareEncoderInfo {
    let ffmpeg_bin = ffmpeg_bin.to_path_buf();
    HARDWARE_ENCODER_CACHE
        .get_or_init(|| async move { detect_hardware_encoder(&ffmpeg_bin).await })
        .await
        .clone()
}

fn cpu_video_encoding(style: &BurnInStyleOptions) -> VideoEncoding {
    VideoEncoding {
        args: vec![
            "-c:v".into(),
            "libx264".into(),
            "-crf".into(),
            style.crf.unwrap_or(20).to_string(),
            "-preset".into(),
            style.preset.as_deref().unwrap_or("medium").into(),
        ],
        needs_nv12: false,
        hardware: false,
        encoder_id: "libx264".into(),
    }
}

pub fn cpu_video_encoding_for_style(style: &BurnInStyleOptions) -> VideoEncoding {
    cpu_video_encoding(style)
}

fn parse_video_probe(stderr: &str) -> (u32, u32, f64) {
    let mut width = 0;
    let mut height = 0;
    for line in stderr.lines().filter(|line| line.contains("Video:")) {
        for token in line.split(',') {
            let token = token.trim();
            let Some((left, right)) = token.split_once('x') else {
                continue;
            };
            let right = right.split_whitespace().next().unwrap_or_default();
            if let (Ok(candidate_width), Ok(candidate_height)) =
                (left.parse::<u32>(), right.parse::<u32>())
            {
                if candidate_width > 0 && candidate_height > 0 {
                    width = candidate_width;
                    height = candidate_height;
                    break;
                }
            }
        }
        if width > 0 && height > 0 {
            break;
        }
    }
    let duration = parse_duration_ms(stderr)
        .map(|value| value as f64 / 1000.0)
        .unwrap_or(0.0);
    (width, height, duration)
}

async fn probe_video_dimensions(ffmpeg_bin: &Path, video_path: &Path) -> (u32, u32, f64) {
    let mut command = Command::new(ffmpeg_bin);
    command
        .args(["-hide_banner", "-i"])
        .arg(video_path)
        .kill_on_drop(true);
    let output = tokio::time::timeout(Duration::from_secs(10), command.output()).await;
    match output {
        Ok(Ok(output)) => parse_video_probe(&String::from_utf8_lossy(&output.stderr)),
        _ => (0, 0, 0.0),
    }
}

fn clamp_bitrate(value: f64, height: u32) -> u64 {
    let (min, max) = if height >= 1_800 {
        (4_000_000.0, 50_000_000.0)
    } else if height >= 900 {
        (1_500_000.0, 16_000_000.0)
    } else if height > 0 {
        (500_000.0, 8_000_000.0)
    } else {
        (800_000.0, 16_000_000.0)
    };
    value.round().clamp(min, max) as u64
}

fn videotoolbox_quality(crf: u8) -> u8 {
    // VideoToolbox 的 q:v 量纲与 CRF 相反：数值越大画质越好。把现有
    // 0–51 CRF 控件映射到稳定的 35–75 区间，保留用户对质量的直觉。
    (75_i32 - i32::from(crf)).clamp(35, 75) as u8
}

fn videotoolbox_bitrate_factor(crf: u8) -> f64 {
    if crf <= 18 {
        1.0
    } else if crf <= 23 {
        1.0 - (f64::from(crf - 18) * 0.07)
    } else {
        (0.65 - f64::from(crf - 23) * 0.012).clamp(0.35, 0.65)
    }
}

/// 按用户选项解析本次合成的视频编码器。硬件能力不可用时返回 CPU
/// 方案，不让“启用硬件”把任务变成不可用状态。
pub async fn resolve_video_encoding(
    ffmpeg_bin: &Path,
    mode: VideoEncoderMode,
    style: &BurnInStyleOptions,
    video_path: &Path,
) -> VideoEncoding {
    let cpu = || cpu_video_encoding(style);
    if mode == VideoEncoderMode::Cpu {
        return cpu();
    }

    let info = get_hardware_encoder_info(ffmpeg_bin).await;
    let Some(encoder_id) = info.encoder_id.as_deref() else {
        return cpu();
    };
    let crf = style.crf.unwrap_or(20);

    if info.rate_mode == Some(HardwareRateMode::Bitrate) {
        let (_, height, duration) = probe_video_dimensions(ffmpeg_bin, video_path).await;
        let size = std::fs::metadata(video_path)
            .map(|metadata| metadata.len())
            .unwrap_or(0);
        if size == 0 || duration <= 0.0 {
            return cpu();
        }
        let source_bitrate = size as f64 * 8.0 / duration;
        let target = clamp_bitrate(
            source_bitrate * 0.85 * videotoolbox_bitrate_factor(crf),
            height,
        );
        return VideoEncoding {
            args: videotoolbox_bitrate_args(target, target * 3 / 2, target * 2),
            needs_nv12: true,
            hardware: true,
            encoder_id: encoder_id.to_string(),
        };
    }

    let quality = if encoder_id == "h264_videotoolbox" {
        videotoolbox_quality(crf)
    } else {
        crf.clamp(1, 51)
    };
    let Some(args) = hardware_cq_args(encoder_id, quality) else {
        return cpu();
    };
    VideoEncoding {
        args,
        needs_nv12: true,
        hardware: true,
        encoder_id: encoder_id.to_string(),
    }
}

pub fn audio_extract_plan(
    ffmpeg_bin: &str,
    video_path: &str,
    output_path: &str,
) -> AudioExtractPlan {
    AudioExtractPlan {
        ffmpeg_bin: ffmpeg_bin.to_string(),
        args: vec![
            "-i".into(),
            video_path.to_string(),
            "-vn".into(),
            "-acodec".into(),
            "pcm_s16le".into(),
            "-ar".into(),
            "16000".into(),
            "-ac".into(),
            "1".into(),
            "-y".into(),
            output_path.to_string(),
        ],
        input: video_path.to_string(),
        output: output_path.to_string(),
    }
}

pub fn subtitle_burn_in_plan(
    ffmpeg_bin: &str,
    video_path: &str,
    subtitle_path: &str,
    output_path: &str,
    style: BurnInStyleOptions,
) -> BurnInPlan {
    let subtitles_filter = subtitles_filter(subtitle_path, &style);
    let crf = style.crf.unwrap_or(20).to_string();
    let preset = style.preset.as_deref().unwrap_or("medium");

    BurnInPlan {
        ffmpeg_bin: ffmpeg_bin.to_string(),
        args: vec![
            "-i".into(),
            video_path.to_string(),
            "-vf".into(),
            subtitles_filter,
            "-c:v".into(),
            "libx264".into(),
            "-crf".into(),
            crf,
            "-preset".into(),
            preset.into(),
            "-c:a".into(),
            "copy".into(),
            "-y".into(),
            output_path.to_string(),
        ],
        video_input: video_path.to_string(),
        subtitle_input: subtitle_path.to_string(),
        output: output_path.to_string(),
    }
}

pub fn extract_audio_args(video_path: &str, output_path: &str) -> Vec<String> {
    vec![
        "-i".into(),
        video_path.into(),
        "-vn".into(),
        "-acodec".into(),
        "pcm_s16le".into(),
        "-ar".into(),
        "16000".into(),
        "-ac".into(),
        "1".into(),
        "-y".into(),
        output_path.into(),
    ]
}

pub fn burn_in_args(
    video_path: &str,
    subtitle_path: &str,
    output_path: &str,
    style: &BurnInStyleOptions,
) -> Vec<String> {
    burn_in_args_with_encoding(video_path, subtitle_path, output_path, style, None)
}

pub fn burn_in_args_with_encoding(
    video_path: &str,
    subtitle_path: &str,
    output_path: &str,
    style: &BurnInStyleOptions,
    encoding: Option<&VideoEncoding>,
) -> Vec<String> {
    let subtitles_filter = subtitles_filter(subtitle_path, style);
    let subtitles_filter = encoded_subtitle_filter(subtitles_filter, encoding);
    let mut args = vec![
        "-i".into(),
        video_path.into(),
        "-vf".into(),
        subtitles_filter,
    ];
    push_h264_args(&mut args, style, encoding);
    args.extend([
        "-c:a".into(),
        "copy".into(),
        "-y".into(),
        output_path.into(),
    ]);
    args
}

pub fn compose_requires_mkv(soft_subtitle: bool, audio_mode: ComposeAudioMode) -> bool {
    soft_subtitle || audio_mode == ComposeAudioMode::AddTrack
}

pub fn compose_args(
    video_path: &str,
    subtitle_path: &str,
    output_path: &str,
    style: &BurnInStyleOptions,
    options: &ComposeOptions,
) -> Result<Vec<String>, String> {
    if options.audio_mode == ComposeAudioMode::Keep && !options.soft_subtitle {
        return Ok(burn_in_args_with_encoding(
            video_path,
            subtitle_path,
            output_path,
            style,
            options.video_encoding.as_ref(),
        ));
    }

    let output_extension = std::path::Path::new(output_path)
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if compose_requires_mkv(options.soft_subtitle, options.audio_mode) && output_extension != "mkv"
    {
        return Err("Soft subtitles and dual audio tracks require an MKV output".into());
    }

    let audio_path = match options.audio_mode {
        ComposeAudioMode::Keep => None,
        ComposeAudioMode::Replace | ComposeAudioMode::Mix | ComposeAudioMode::AddTrack => Some(
            options
                .audio_path
                .as_deref()
                .filter(|path| !path.trim().is_empty())
                .ok_or_else(|| "The selected audio mode requires an audio file".to_string())?,
        ),
    };
    if options.audio_mode == ComposeAudioMode::Mix && options.original_audio_tracks == 0 {
        return Err("The source video has no audio track to mix".into());
    }

    let mut args = vec!["-i".into(), video_path.into()];
    if let Some(path) = audio_path {
        args.extend(["-i".into(), path.into()]);
    }
    if options.soft_subtitle {
        args.extend(["-i".into(), subtitle_path.into()]);
    }

    if options.soft_subtitle {
        let subtitle_input = if audio_path.is_some() { 2 } else { 1 };
        match options.audio_mode {
            ComposeAudioMode::Keep => {
                args.extend([
                    "-map".into(),
                    "0:v:0".into(),
                    "-map".into(),
                    "0:a?".into(),
                    "-map".into(),
                    format!("{subtitle_input}:0"),
                    "-c:v".into(),
                    "copy".into(),
                    "-c:a".into(),
                    "copy".into(),
                ]);
            }
            ComposeAudioMode::Replace => {
                args.extend([
                    "-map".into(),
                    "0:v:0".into(),
                    "-map".into(),
                    "1:a:0".into(),
                    "-map".into(),
                    format!("{subtitle_input}:0"),
                    "-c:v".into(),
                    "copy".into(),
                ]);
                push_aac_args(&mut args, None);
                push_audio_metadata(&mut args, 0, options, true);
            }
            ComposeAudioMode::Mix => {
                args.extend([
                    "-filter_complex".into(),
                    duck_mix_filter(None),
                    "-map".into(),
                    "0:v:0".into(),
                    "-map".into(),
                    "[mixed]".into(),
                    "-map".into(),
                    format!("{subtitle_input}:0"),
                    "-c:v".into(),
                    "copy".into(),
                ]);
                push_aac_args(&mut args, None);
                push_audio_metadata(&mut args, 0, options, true);
            }
            ComposeAudioMode::AddTrack => {
                let audio_index = options.original_audio_tracks;
                args.extend([
                    "-map".into(),
                    "0:v:0".into(),
                    "-map".into(),
                    "0:a?".into(),
                    "-map".into(),
                    "1:a:0".into(),
                    "-map".into(),
                    format!("{subtitle_input}:0"),
                    "-c:v".into(),
                    "copy".into(),
                    "-c:a".into(),
                    "copy".into(),
                ]);
                push_aac_args(&mut args, Some(audio_index));
                push_audio_metadata(&mut args, audio_index, options, false);
            }
        }

        let subtitle_codec = match std::path::Path::new(subtitle_path)
            .extension()
            .and_then(|extension| extension.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            Some("ass" | "ssa") => "ass",
            _ => "srt",
        };
        args.extend(["-c:s".into(), subtitle_codec.into()]);
        push_subtitle_metadata(&mut args, options);
    } else {
        let subtitle_filter = encoded_subtitle_filter(
            subtitles_filter(subtitle_path, style),
            options.video_encoding.as_ref(),
        );
        match options.audio_mode {
            ComposeAudioMode::Keep => unreachable!("handled above"),
            ComposeAudioMode::Replace => {
                args.extend([
                    "-vf".into(),
                    subtitle_filter,
                    "-map".into(),
                    "0:v:0".into(),
                    "-map".into(),
                    "1:a:0".into(),
                ]);
                push_h264_args(&mut args, style, options.video_encoding.as_ref());
                push_aac_args(&mut args, None);
                push_audio_metadata(&mut args, 0, options, true);
            }
            ComposeAudioMode::Mix => {
                args.extend([
                    "-filter_complex".into(),
                    duck_mix_filter(Some(&subtitle_filter)),
                    "-map".into(),
                    "[video]".into(),
                    "-map".into(),
                    "[mixed]".into(),
                ]);
                push_h264_args(&mut args, style, options.video_encoding.as_ref());
                push_aac_args(&mut args, None);
                push_audio_metadata(&mut args, 0, options, true);
            }
            ComposeAudioMode::AddTrack => {
                let audio_index = options.original_audio_tracks;
                args.extend([
                    "-vf".into(),
                    subtitle_filter,
                    "-map".into(),
                    "0:v:0".into(),
                    "-map".into(),
                    "0:a?".into(),
                    "-map".into(),
                    "1:a:0".into(),
                ]);
                push_h264_args(&mut args, style, options.video_encoding.as_ref());
                args.extend(["-c:a".into(), "copy".into()]);
                push_aac_args(&mut args, Some(audio_index));
                push_audio_metadata(&mut args, audio_index, options, false);
            }
        }
    }

    args.extend(["-map_metadata".into(), "0".into()]);
    if !compose_requires_mkv(options.soft_subtitle, options.audio_mode)
        && matches!(output_extension.as_str(), "mp4" | "mov" | "m4v")
    {
        args.extend(["-movflags".into(), "+faststart".into()]);
    }
    args.extend(["-y".into(), output_path.into()]);
    Ok(args)
}

fn push_h264_args(
    args: &mut Vec<String>,
    style: &BurnInStyleOptions,
    encoding: Option<&VideoEncoding>,
) {
    if let Some(encoding) = encoding {
        args.extend(encoding.args.iter().cloned());
    } else {
        args.extend([
            "-c:v".into(),
            "libx264".into(),
            "-crf".into(),
            style.crf.unwrap_or(20).to_string(),
            "-preset".into(),
            style.preset.as_deref().unwrap_or("medium").into(),
        ]);
    }
}

fn encoded_subtitle_filter(filter: String, encoding: Option<&VideoEncoding>) -> String {
    if encoding.is_some_and(|encoding| encoding.needs_nv12) {
        format!("{filter},format=nv12")
    } else {
        filter
    }
}

fn push_aac_args(args: &mut Vec<String>, stream_index: Option<usize>) {
    let suffix = stream_index
        .map(|index| format!(":{index}"))
        .unwrap_or_default();
    args.extend([
        format!("-c:a{suffix}"),
        "aac".into(),
        format!("-b:a{suffix}"),
        "192k".into(),
    ]);
}

fn duck_mix_filter(subtitle_filter: Option<&str>) -> String {
    let mut filters = Vec::new();
    if let Some(filter) = subtitle_filter {
        filters.push(format!("[0:v:0]{filter}[video]"));
    }
    filters.extend([
        "[1:a:0]asplit=2[sidechain][dub]".into(),
        "[0:a:0][sidechain]sidechaincompress=threshold=0.03:ratio=8:attack=20:release=300[background]".into(),
        "[background][dub]amix=inputs=2:duration=first:normalize=0[mixed]".into(),
    ]);
    filters.join(";")
}

fn push_subtitle_metadata(args: &mut Vec<String>, options: &ComposeOptions) {
    if let Some(language) = options.subtitle_language.as_deref() {
        args.extend(["-metadata:s:s:0".into(), format!("language={language}")]);
    }
    if let Some(title) = options.subtitle_title.as_deref() {
        args.extend(["-metadata:s:s:0".into(), format!("title={title}")]);
    }
    args.extend(["-disposition:s:0".into(), "default".into()]);
}

fn push_audio_metadata(
    args: &mut Vec<String>,
    index: usize,
    options: &ComposeOptions,
    default: bool,
) {
    let stream = format!("-metadata:s:a:{index}");
    if let Some(language) = options.audio_language.as_deref() {
        args.extend([stream.clone(), format!("language={language}")]);
    }
    if let Some(title) = options.audio_title.as_deref() {
        args.extend([stream, format!("title={title}")]);
    }
    args.extend([
        format!("-disposition:a:{index}"),
        if default { "default" } else { "0" }.into(),
    ]);
}

fn subtitles_filter(subtitle_path: &str, style: &BurnInStyleOptions) -> String {
    let mut fields = vec![
        format!("FontSize={}", style.font_size.unwrap_or(24)),
        format!(
            "PrimaryColour={}",
            style.font_color.as_deref().unwrap_or("&H00FFFFFF")
        ),
        format!(
            "OutlineColour={}",
            style.outline_color.as_deref().unwrap_or("&H00000000")
        ),
        format!("Outline={}", style.outline_width.unwrap_or(2.0)),
        format!("Shadow={}", style.shadow.unwrap_or(0.0)),
        format!(
            "Alignment={}",
            ffmpeg_force_style_alignment(style.alignment.unwrap_or(2))
        ),
        format!("MarginV={}", style.margin_v.unwrap_or(30)),
    ];
    if let Some(font_name) = style
        .font_name
        .as_deref()
        .filter(|name| !name.trim().is_empty())
    {
        fields.push(format!("FontName={}", font_name.trim()));
    }
    if style.opaque_background.unwrap_or(false) {
        fields.push("BorderStyle=3".into());
        fields.push(format!(
            "BackColour={}",
            style.background_color.as_deref().unwrap_or("&H80000000")
        ));
    }
    format!(
        "subtitles={}:force_style='{}'",
        escape_ass_path(subtitle_path),
        fields.join(",")
    )
}

fn ffmpeg_force_style_alignment(numpad_alignment: u8) -> u8 {
    match numpad_alignment {
        1..=3 => numpad_alignment,
        4 => 9,
        5 => 10,
        6 => 11,
        7 => 5,
        8 => 6,
        9 => 7,
        _ => 2,
    }
}

fn escape_ass_path(path: &str) -> String {
    path.replace('\\', "\\\\")
        .replace(':', "\\:")
        .replace('\'', "\\'")
}

pub fn parse_duration_ms(stderr: &str) -> Option<u64> {
    for line in stderr.lines() {
        if line.contains("Duration:") {
            let part = line.split("Duration:").nth(1)?;
            let time_str = part.split(',').next()?.trim();
            return parse_ffmpeg_time(time_str);
        }
    }
    None
}

pub fn parse_current_time_ms(line: &str) -> Option<u64> {
    if let Some(time_part) = line.split("time=").nth(1) {
        let time_str = time_part.split_whitespace().next()?;
        return parse_ffmpeg_time(time_str);
    }
    None
}

pub fn parse_progress_time_ms(line: &str) -> Option<u64> {
    if let Some(raw) = line.strip_prefix("out_time_us=") {
        return raw.trim().parse::<u64>().ok().map(|us| us / 1000);
    }
    if let Some(raw) = line.strip_prefix("out_time_ms=") {
        return raw.trim().parse::<u64>().ok().map(|us| us / 1000);
    }
    if let Some(raw) = line.strip_prefix("out_time=") {
        return parse_ffmpeg_time(raw.trim());
    }
    parse_current_time_ms(line)
}

fn parse_ffmpeg_time(time: &str) -> Option<u64> {
    let parts: Vec<&str> = time.split(':').collect();
    if parts.len() != 3 {
        return None;
    }
    let hours: f64 = parts[0].parse().ok()?;
    let minutes: f64 = parts[1].parse().ok()?;
    let seconds: f64 = parts[2].parse().ok()?;
    Some(((hours * 3600.0 + minutes * 60.0 + seconds) * 1000.0) as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_extract_basic() {
        let plan = audio_extract_plan("ffmpeg", "/tmp/in.mp4", "/tmp/out.wav");
        assert_eq!(plan.ffmpeg_bin, "ffmpeg");
        assert_eq!(plan.input, "/tmp/in.mp4");
        assert_eq!(plan.output, "/tmp/out.wav");
        assert!(plan.args.contains(&"-vn".to_string()));
        assert!(plan.args.contains(&"16000".to_string()));
        assert!(plan.args.contains(&"pcm_s16le".to_string()));
    }

    #[test]
    fn burn_in_default_style() {
        let plan = subtitle_burn_in_plan(
            "ffmpeg",
            "/tmp/video.mp4",
            "/tmp/subs.ass",
            "/tmp/out.mp4",
            BurnInStyleOptions::default(),
        );
        assert!(plan
            .args
            .windows(2)
            .any(|w| w[0] == "-vf" && w[1].contains("subtitles=")));
        assert!(plan
            .args
            .windows(2)
            .any(|w| w[0] == "-c:a" && w[1] == "copy"));
    }

    #[test]
    fn burn_in_custom_style() {
        let plan = subtitle_burn_in_plan(
            "ffmpeg",
            "/tmp/video.mp4",
            "/tmp/subs.ass",
            "/tmp/out.mp4",
            BurnInStyleOptions {
                font_size: Some(48),
                font_color: Some("&H0000FFFF".into()),
                outline_color: Some("&H00FF0000".into()),
                margin_v: Some(50),
                ..Default::default()
            },
        );
        let vf = plan
            .args
            .iter()
            .find(|a| a.contains("FontSize=48"))
            .unwrap();
        assert!(vf.contains("PrimaryColour=&H0000FFFF"));
        assert!(vf.contains("OutlineColour=&H00FF0000"));
        assert!(vf.contains("MarginV=50"));
    }

    #[test]
    fn numpad_alignment_maps_to_ffmpeg_ssa_force_style_values() {
        assert_eq!(ffmpeg_force_style_alignment(1), 1);
        assert_eq!(ffmpeg_force_style_alignment(2), 2);
        assert_eq!(ffmpeg_force_style_alignment(3), 3);
        assert_eq!(ffmpeg_force_style_alignment(4), 9);
        assert_eq!(ffmpeg_force_style_alignment(5), 10);
        assert_eq!(ffmpeg_force_style_alignment(6), 11);
        assert_eq!(ffmpeg_force_style_alignment(7), 5);
        assert_eq!(ffmpeg_force_style_alignment(8), 6);
        assert_eq!(ffmpeg_force_style_alignment(9), 7);
        assert_eq!(ffmpeg_force_style_alignment(0), 2);
    }

    #[test]
    fn path_escaping() {
        assert_eq!(escape_ass_path("/tmp/my:file.ass"), "/tmp/my\\:file.ass");
        assert_eq!(
            escape_ass_path("C:\\Users\\test.ass"),
            "C\\:\\\\Users\\\\test.ass"
        );
    }

    #[test]
    fn parse_duration_standard() {
        let stderr = "  Duration: 01:23:45.67, start: 0.000000, bitrate: 1234 kb/s";
        assert_eq!(parse_duration_ms(stderr), Some(5025670));
    }

    #[test]
    fn parse_duration_short() {
        let stderr = "  Duration: 00:00:30.50, start: 0.000000";
        assert_eq!(parse_duration_ms(stderr), Some(30500));
    }

    #[test]
    fn parse_current_time() {
        assert_eq!(
            parse_current_time_ms("frame= 100 fps=30 time=00:01:05.20 bitrate=1000kbits/s"),
            Some(65200)
        );
    }

    #[test]
    fn parse_ffmpeg_progress_time() {
        assert_eq!(
            parse_progress_time_ms("out_time_us=219939438"),
            Some(219939)
        );
        assert_eq!(
            parse_progress_time_ms("out_time_ms=219939438"),
            Some(219939)
        );
        assert_eq!(
            parse_progress_time_ms("out_time=00:03:39.939438"),
            Some(219939)
        );
    }

    #[test]
    fn parse_current_time_none() {
        assert_eq!(parse_current_time_ms("some random line"), None);
    }

    #[test]
    fn extract_audio_args_structure() {
        let args = extract_audio_args("/tmp/in.mp4", "/tmp/out.wav");
        assert_eq!(args[0], "-i");
        assert_eq!(args[1], "/tmp/in.mp4");
        assert!(args.contains(&"pcm_s16le".to_string()));
        assert!(args.contains(&"16000".to_string()));
    }

    #[test]
    fn burn_in_args_structure() {
        let args = burn_in_args(
            "/tmp/v.mp4",
            "/tmp/s.ass",
            "/tmp/o.mp4",
            &BurnInStyleOptions::default(),
        );
        assert!(args
            .windows(2)
            .any(|w| w[0] == "-vf" && w[1].contains("subtitles=")));
    }

    #[test]
    fn video_encoder_mode_accepts_only_supported_values() {
        assert_eq!(
            VideoEncoderMode::parse(Some("auto")).unwrap(),
            VideoEncoderMode::Auto
        );
        assert_eq!(
            VideoEncoderMode::parse(Some("hardware")).unwrap(),
            VideoEncoderMode::Hardware
        );
        assert_eq!(
            VideoEncoderMode::parse(Some("cpu")).unwrap(),
            VideoEncoderMode::Cpu
        );
        assert_eq!(
            VideoEncoderMode::parse(None).unwrap(),
            VideoEncoderMode::Cpu
        );
        assert!(VideoEncoderMode::parse(Some("shell-command")).is_err());
    }

    #[test]
    fn hardware_burn_adds_nv12_and_uses_resolved_encoder_args() {
        let encoding = VideoEncoding {
            args: hardware_cq_args("h264_videotoolbox", 55).unwrap(),
            needs_nv12: true,
            hardware: true,
            encoder_id: "h264_videotoolbox".into(),
        };
        let args = burn_in_args_with_encoding(
            "/tmp/v.mp4",
            "/tmp/s.ass",
            "/tmp/o.mp4",
            &BurnInStyleOptions::default(),
            Some(&encoding),
        );
        assert!(args.windows(2).any(|pair| {
            pair[0] == "-vf" && pair[1].contains("subtitles=") && pair[1].ends_with("format=nv12")
        }));
        assert!(args
            .windows(2)
            .any(|pair| pair == ["-c:v", "h264_videotoolbox"]));
        assert!(args.windows(2).any(|pair| pair == ["-q:v", "55"]));
        assert!(!args.iter().any(|arg| arg == "libx264"));
    }

    #[test]
    fn hardware_compose_mix_keeps_video_filter_inside_complex_graph() {
        let encoding = VideoEncoding {
            args: hardware_cq_args("h264_videotoolbox", 55).unwrap(),
            needs_nv12: true,
            hardware: true,
            encoder_id: "h264_videotoolbox".into(),
        };
        let args = compose_args(
            "/tmp/v.mp4",
            "/tmp/s.ass",
            "/tmp/o.mp4",
            &BurnInStyleOptions::default(),
            &ComposeOptions {
                audio_mode: ComposeAudioMode::Mix,
                video_encoding: Some(encoding),
                audio_path: Some("/tmp/dub.wav".into()),
                original_audio_tracks: 1,
                ..ComposeOptions::default()
            },
        )
        .unwrap();
        let graph = args
            .windows(2)
            .find(|pair| pair[0] == "-filter_complex")
            .map(|pair| pair[1].as_str())
            .unwrap();
        assert!(graph.contains("subtitles="));
        assert!(graph.contains("format=nv12[video]"));
        assert!(args
            .windows(2)
            .any(|pair| pair == ["-c:v", "h264_videotoolbox"]));
    }

    #[test]
    fn video_probe_parses_resolution_and_duration() {
        let stderr = "Duration: 00:01:02.50, start: 0.000000\n  Stream #0:0: Video: h264, yuv420p, 1920x1080, 30 fps";
        assert_eq!(parse_video_probe(stderr), (1920, 1080, 62.5));
    }

    #[test]
    fn compose_hard_keep_is_exactly_the_legacy_burn_command() {
        let style = BurnInStyleOptions::default();
        let legacy = burn_in_args("/tmp/v.mp4", "/tmp/s.ass", "/tmp/o.mp4", &style);
        let composed = compose_args(
            "/tmp/v.mp4",
            "/tmp/s.ass",
            "/tmp/o.mp4",
            &style,
            &ComposeOptions::default(),
        )
        .unwrap();
        assert_eq!(composed, legacy);
    }

    #[test]
    fn compose_soft_mkv_copies_video_audio_and_sets_subtitle_metadata() {
        let args = compose_args(
            "/tmp/v.mp4",
            "/tmp/s.ass",
            "/tmp/o.mkv",
            &BurnInStyleOptions::default(),
            &ComposeOptions {
                soft_subtitle: true,
                subtitle_language: Some("zho".into()),
                subtitle_title: Some("简体中文".into()),
                ..ComposeOptions::default()
            },
        )
        .unwrap();
        assert!(args.windows(2).any(|pair| pair == ["-c:v", "copy"]));
        assert!(args.windows(2).any(|pair| pair == ["-c:a", "copy"]));
        assert!(args.windows(2).any(|pair| pair == ["-c:s", "ass"]));
        assert!(args
            .windows(2)
            .any(|pair| pair == ["-metadata:s:s:0", "language=zho"]));
        assert!(args
            .windows(2)
            .any(|pair| pair == ["-metadata:s:s:0", "title=简体中文"]));
    }

    #[test]
    fn compose_soft_requires_mkv() {
        let error = compose_args(
            "/tmp/v.mp4",
            "/tmp/s.srt",
            "/tmp/o.mp4",
            &BurnInStyleOptions::default(),
            &ComposeOptions {
                soft_subtitle: true,
                ..ComposeOptions::default()
            },
        )
        .unwrap_err();
        assert!(error.contains("MKV"));
    }

    #[test]
    fn compose_hard_mix_builds_one_complex_filter_graph() {
        let args = compose_args(
            "/tmp/v.mp4",
            "/tmp/dub.wav",
            "/tmp/o.mp4",
            &BurnInStyleOptions::default(),
            &ComposeOptions {
                audio_mode: ComposeAudioMode::Mix,
                audio_path: Some("/tmp/dub.wav".into()),
                original_audio_tracks: 1,
                ..ComposeOptions::default()
            },
        )
        .unwrap();
        let graph = args
            .windows(2)
            .find(|pair| pair[0] == "-filter_complex")
            .map(|pair| pair[1].as_str())
            .unwrap();
        assert!(graph.contains("subtitles="));
        assert!(graph.contains("sidechaincompress"));
        assert!(graph.contains("[mixed]"));
        assert!(!args.iter().any(|arg| arg == "-vf"));
    }

    #[test]
    fn compose_dual_track_only_encodes_the_appended_audio_stream() {
        let args = compose_args(
            "/tmp/v.mp4",
            "/tmp/s.srt",
            "/tmp/o.mkv",
            &BurnInStyleOptions::default(),
            &ComposeOptions {
                audio_mode: ComposeAudioMode::AddTrack,
                audio_path: Some("/tmp/dub.wav".into()),
                audio_language: Some("zho".into()),
                audio_title: Some("中文配音".into()),
                original_audio_tracks: 2,
                ..ComposeOptions::default()
            },
        )
        .unwrap();
        assert!(args.windows(2).any(|pair| pair == ["-c:a", "copy"]));
        assert!(args.windows(2).any(|pair| pair == ["-c:a:2", "aac"]));
        assert!(args
            .windows(2)
            .any(|pair| pair == ["-metadata:s:a:2", "language=zho"]));
        assert!(args
            .windows(2)
            .any(|pair| pair == ["-metadata:s:a:2", "title=中文配音"]));
    }

    #[test]
    fn compose_mix_rejects_a_video_without_original_audio() {
        let error = compose_args(
            "/tmp/v.mp4",
            "/tmp/s.srt",
            "/tmp/o.mp4",
            &BurnInStyleOptions::default(),
            &ComposeOptions {
                audio_mode: ComposeAudioMode::Mix,
                audio_path: Some("/tmp/dub.wav".into()),
                ..ComposeOptions::default()
            },
        )
        .unwrap_err();
        assert!(error.contains("no audio track"));
    }

    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn detects_bundled_videotoolbox_when_enabled() {
        if std::env::var("FINALSUB_HW_ENCODER_E2E").as_deref() != Ok("1") {
            return;
        }
        let architecture = if cfg!(target_arch = "aarch64") {
            "aarch64"
        } else {
            "x86_64"
        };
        let ffmpeg = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("binaries")
            .join(format!("ffmpeg-{architecture}-apple-darwin"));
        let info = detect_hardware_encoder(&ffmpeg).await;
        assert!(info.available, "bundled FFmpeg should pass a real VT probe");
        assert_eq!(info.encoder_id.as_deref(), Some("h264_videotoolbox"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn compose_real_media_fixture_when_enabled() {
        if std::env::var("FINALSUB_COMPOSE_MEDIA_E2E").as_deref() != Ok("1") {
            return;
        }

        let architecture = if cfg!(target_arch = "aarch64") {
            "aarch64"
        } else {
            "x86_64"
        };
        let ffmpeg = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("binaries")
            .join(format!("ffmpeg-{architecture}-apple-darwin"));
        assert!(
            ffmpeg.is_file(),
            "missing FFmpeg sidecar: {}",
            ffmpeg.display()
        );

        let fixture = tempfile::tempdir().unwrap();
        let video = fixture.path().join("source.mp4");
        let dub = fixture.path().join("dub.wav");
        let subtitle = fixture.path().join("captions.srt");
        let soft_output = fixture.path().join("soft-dual.mkv");
        let mixed_output = fixture.path().join("hard-mixed.mp4");
        let hardware_output = fixture.path().join("hard-videotoolbox.mp4");

        let run = |arguments: &[String]| {
            let output = std::process::Command::new(&ffmpeg)
                .args(arguments)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "FFmpeg failed:\n{}",
                String::from_utf8_lossy(&output.stderr)
            );
        };

        run(&[
            "-f".into(),
            "lavfi".into(),
            "-i".into(),
            "color=c=blue:s=320x180:r=24:d=2".into(),
            "-f".into(),
            "lavfi".into(),
            "-i".into(),
            "sine=frequency=440:duration=2".into(),
            "-shortest".into(),
            "-c:v".into(),
            "libx264".into(),
            "-pix_fmt".into(),
            "yuv420p".into(),
            "-c:a".into(),
            "aac".into(),
            "-y".into(),
            video.to_string_lossy().into_owned(),
        ]);
        run(&[
            "-f".into(),
            "lavfi".into(),
            "-i".into(),
            "sine=frequency=880:duration=2".into(),
            "-c:a".into(),
            "pcm_s16le".into(),
            "-y".into(),
            dub.to_string_lossy().into_owned(),
        ]);
        std::fs::write(
            &subtitle,
            "1\n00:00:00,000 --> 00:00:01,800\nFinalSub compose fixture\n",
        )
        .unwrap();

        let style = BurnInStyleOptions {
            preset: Some("ultrafast".into()),
            ..BurnInStyleOptions::default()
        };
        let soft_args = compose_args(
            &video.to_string_lossy(),
            &subtitle.to_string_lossy(),
            &soft_output.to_string_lossy(),
            &style,
            &ComposeOptions {
                soft_subtitle: true,
                audio_mode: ComposeAudioMode::AddTrack,
                video_encoding: None,
                audio_path: Some(dub.to_string_lossy().into_owned()),
                subtitle_language: Some("zho".into()),
                subtitle_title: Some("FinalSub Subtitles".into()),
                audio_language: Some("zho".into()),
                audio_title: Some("FinalSub Dub".into()),
                original_audio_tracks: 1,
            },
        )
        .unwrap();
        run(&soft_args);

        let probe = std::process::Command::new(&ffmpeg)
            .arg("-i")
            .arg(&soft_output)
            .output()
            .unwrap();
        let probe = String::from_utf8_lossy(&probe.stderr);
        assert_eq!(probe.matches("Audio:").count(), 2, "{probe}");
        assert_eq!(probe.matches("Subtitle:").count(), 1, "{probe}");
        assert!(probe.contains("FinalSub Dub"), "{probe}");
        assert!(probe.contains("FinalSub Subtitles"), "{probe}");
        assert!(probe.contains("(zho)"), "{probe}");

        let mixed_args = compose_args(
            &video.to_string_lossy(),
            &subtitle.to_string_lossy(),
            &mixed_output.to_string_lossy(),
            &style,
            &ComposeOptions {
                audio_mode: ComposeAudioMode::Mix,
                audio_path: Some(dub.to_string_lossy().into_owned()),
                audio_language: Some("zho".into()),
                audio_title: Some("FinalSub Mixed Dub".into()),
                original_audio_tracks: 1,
                ..ComposeOptions::default()
            },
        )
        .unwrap();
        run(&mixed_args);
        assert!(mixed_output.is_file());

        if std::env::var("FINALSUB_HW_ENCODER_E2E").as_deref() == Ok("1") {
            let encoding = VideoEncoding {
                args: hardware_cq_args("h264_videotoolbox", 55).unwrap(),
                needs_nv12: true,
                hardware: true,
                encoder_id: "h264_videotoolbox".into(),
            };
            let hardware_args = burn_in_args_with_encoding(
                &video.to_string_lossy(),
                &subtitle.to_string_lossy(),
                &hardware_output.to_string_lossy(),
                &style,
                Some(&encoding),
            );
            run(&hardware_args);
            assert!(hardware_output.is_file());
            let probe = std::process::Command::new(&ffmpeg)
                .arg("-i")
                .arg(&hardware_output)
                .output()
                .unwrap();
            assert!(
                String::from_utf8_lossy(&probe.stderr).contains("Video: h264"),
                "hardware output must contain an H.264 video stream"
            );
        }
    }
}
