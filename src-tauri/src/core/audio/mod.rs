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
    pub font_size: Option<u32>,
    pub font_color: Option<String>,
    pub outline_color: Option<String>,
    pub margin_v: Option<u32>,
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
    let fs = style.font_size.unwrap_or(24);
    let fc = style.font_color.as_deref().unwrap_or("&H00FFFFFF");
    let oc = style.outline_color.as_deref().unwrap_or("&H00000000");
    let mv = style.margin_v.unwrap_or(30);

    let subtitles_filter = format!(
        "subtitles={}:force_style='FontSize={fs},PrimaryColour={fc},OutlineColour={oc},MarginV={mv}'",
        escape_ass_path(subtitle_path),
    );

    BurnInPlan {
        ffmpeg_bin: ffmpeg_bin.to_string(),
        args: vec![
            "-i".into(),
            video_path.to_string(),
            "-vf".into(),
            subtitles_filter,
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
    let fs = style.font_size.unwrap_or(24);
    let fc = style.font_color.as_deref().unwrap_or("&H00FFFFFF");
    let oc = style.outline_color.as_deref().unwrap_or("&H00000000");
    let mv = style.margin_v.unwrap_or(30);

    let subtitles_filter = format!(
        "subtitles={}:force_style='FontSize={fs},PrimaryColour={fc},OutlineColour={oc},MarginV={mv}'",
        escape_ass_path(subtitle_path),
    );

    vec![
        "-i".into(),
        video_path.into(),
        "-vf".into(),
        subtitles_filter,
        "-c:a".into(),
        "copy".into(),
        "-y".into(),
        output_path.into(),
    ]
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
