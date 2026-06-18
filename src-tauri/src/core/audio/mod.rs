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

fn escape_ass_path(path: &str) -> String {
    path.replace('\\', "\\\\")
        .replace(':', "\\:")
        .replace('\'', "\\'")
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
}
