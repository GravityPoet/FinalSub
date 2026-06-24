use crate::core::subtitle::SubtitleTrack;
use crate::error::Result;

pub fn convert_subtitle(srt_content: &str, config: &str) -> Result<String> {
    let mut track = SubtitleTrack::from_srt(srt_content)?;
    let converter = opencc_fmmseg::OpenCC::new();
    for cue in &mut track.cues {
        cue.text = converter.convert(&cue.text, config, false);
    }
    Ok(track.to_srt())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_opencc_conversion() {
        let srt = "1\n00:00:01,000 --> 00:00:04,000\n汉字转换测试\n\n2\n00:00:04,500 --> 00:00:08,000\n软件和软体";
        let s2t_converted = convert_subtitle(srt, "s2t").unwrap();
        assert!(s2t_converted.contains("漢字轉換測試"));
        
        let s2twp_converted = convert_subtitle(srt, "s2twp").unwrap();
        assert!(s2twp_converted.contains("軟體")); // 软件 -> 软体 (Taiwan phrase)
    }
}
