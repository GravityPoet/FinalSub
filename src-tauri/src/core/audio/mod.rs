use serde::{Deserialize, Serialize};

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FfmpegProgress {
    pub phase: String,
    pub percent: Option<f32>,
    pub message: String,
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
    let subtitles_filter = subtitles_filter(subtitle_path, style);
    let crf = style.crf.unwrap_or(20).to_string();
    let preset = style.preset.as_deref().unwrap_or("medium");

    vec![
        "-i".into(),
        video_path.into(),
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
        output_path.into(),
    ]
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
}
