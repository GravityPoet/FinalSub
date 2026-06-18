use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cue {
    pub index: u32,
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubtitleTrack {
    pub cues: Vec<Cue>,
}

impl SubtitleTrack {
    pub fn new() -> Self {
        Self { cues: Vec::new() }
    }

    pub fn from_srt(srt: &str) -> crate::error::Result<Self> {
        let cues = parse_srt(srt)?;
        Ok(Self { cues })
    }

    pub fn to_srt(&self) -> String {
        serialize_srt(&self.cues)
    }

    pub fn len(&self) -> usize {
        self.cues.len()
    }

    pub fn is_empty(&self) -> bool {
        self.cues.is_empty()
    }
}

impl Default for SubtitleTrack {
    fn default() -> Self {
        Self::new()
    }
}

pub fn format_srt_time(ms: u64) -> String {
    let h = ms / 3_600_000;
    let m = (ms % 3_600_000) / 60_000;
    let s = (ms % 60_000) / 1_000;
    let millis = ms % 1_000;
    format!("{h:02}:{m:02}:{s:02},{millis:03}")
}

pub fn parse_srt_time(raw: &str) -> crate::error::Result<u64> {
    let raw = raw.trim().replace(',', ".");
    let parts: Vec<&str> = raw.split(':').collect();
    let (h, m, s_str) = match parts.len() {
        3 => (
            parts[0].parse::<u64>().unwrap_or(0),
            parts[1].parse::<u64>().unwrap_or(0),
            parts[2],
        ),
        2 => (0, parts[0].parse::<u64>().unwrap_or(0), parts[1]),
        1 => (0, 0, parts[0]),
        _ => {
            return Err(crate::error::FinalSubError::Parse(format!(
                "bad time: {raw}"
            )))
        }
    };
    let sec: f64 = s_str
        .parse()
        .map_err(|_| crate::error::FinalSubError::Parse(format!("bad seconds: {s_str}")))?;
    let total_ms = h * 3_600_000 + m * 60_000 + (sec * 1000.0).round() as u64;
    Ok(total_ms)
}

fn parse_srt_block(block: &str) -> crate::error::Result<Cue> {
    let lines: Vec<&str> = block.lines().collect();
    if lines.len() < 3 {
        return Err(crate::error::FinalSubError::Parse(
            "bad SRT block: expected index, timing, and text".into(),
        ));
    }

    let index_raw = lines[0].trim().trim_start_matches('\u{feff}');
    let index: u32 = index_raw
        .parse()
        .map_err(|_| crate::error::FinalSubError::Parse(format!("bad cue index: {index_raw}")))?;

    let timing = lines[1].trim();
    let arrow = "-->";
    let arrow_pos = timing
        .find(arrow)
        .ok_or_else(|| crate::error::FinalSubError::Parse(format!("bad cue timing: {timing}")))?;
    let start_str = &timing[..arrow_pos];
    let end_str = &timing[arrow_pos + arrow.len()..];

    let start_ms = parse_srt_time(start_str)?;
    let end_ms = parse_srt_time(end_str)?;
    if end_ms <= start_ms {
        return Err(crate::error::FinalSubError::Parse(format!(
            "cue end must be after start: {timing}"
        )));
    }

    let text = lines[2..].join("\n").trim().to_string();

    if text.is_empty() {
        return Err(crate::error::FinalSubError::Parse(format!(
            "empty cue text: {index}"
        )));
    }

    Ok(Cue {
        index,
        start_ms,
        end_ms,
        text,
    })
}

pub fn parse_srt(srt: &str) -> crate::error::Result<Vec<Cue>> {
    let normalized = srt.replace("\r\n", "\n").replace('\r', "\n");
    let mut cues = Vec::new();
    let mut saw_block = false;
    for block in normalized.split("\n\n") {
        let block = block.trim();
        if block.is_empty() {
            continue;
        }
        saw_block = true;
        cues.push(parse_srt_block(block)?);
    }
    if cues.is_empty() {
        return Err(crate::error::FinalSubError::Parse(if saw_block {
            "no valid SRT cues found".into()
        } else {
            "empty SRT".into()
        }));
    }
    Ok(cues)
}

pub fn serialize_srt(cues: &[Cue]) -> String {
    let mut out = String::new();
    for (i, cue) in cues.iter().enumerate() {
        let idx = i + 1;
        out.push_str(&format!(
            "{idx}\n{} --> {}\n{}\n\n",
            format_srt_time(cue.start_ms),
            format_srt_time(cue.end_ms),
            cue.text
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_srt() {
        let input = "1\n00:00:01,000 --> 00:00:03,500\nHello world\n\n2\n00:00:04,000 --> 00:00:06,000\nSecond line\n\n";
        let track = SubtitleTrack::from_srt(input).unwrap();
        assert_eq!(track.len(), 2);
        assert_eq!(track.cues[0].text, "Hello world");
        assert_eq!(track.cues[0].start_ms, 1000);
        assert_eq!(track.cues[0].end_ms, 3500);
        assert_eq!(track.cues[1].text, "Second line");

        let output = track.to_srt();
        assert!(output.contains("00:00:01,000 --> 00:00:03,500"));
        assert!(output.contains("Hello world"));
    }

    #[test]
    fn parse_srt_time_variants() {
        assert_eq!(parse_srt_time("00:00:01,000").unwrap(), 1000);
        assert_eq!(parse_srt_time("00:01:30.500").unwrap(), 90_500);
        assert_eq!(parse_srt_time("1:30:00").unwrap(), 5_400_000);
    }

    #[test]
    fn format_srt_time_values() {
        assert_eq!(format_srt_time(0), "00:00:00,000");
        assert_eq!(format_srt_time(1000), "00:00:01,000");
        assert_eq!(format_srt_time(90_500), "00:01:30,500");
    }

    #[test]
    fn empty_srt() {
        let err = SubtitleTrack::from_srt("").unwrap_err();
        assert!(err.to_string().contains("empty SRT"));
    }

    #[test]
    fn multiline_cue() {
        let input = "1\n00:00:00,000 --> 00:00:02,000\nLine one\nLine two\n\n";
        let track = SubtitleTrack::from_srt(input).unwrap();
        assert_eq!(track.cues[0].text, "Line one\nLine two");
    }

    #[test]
    fn crlf_srt_blocks() {
        let input = "1\r\n00:00:01,000 --> 00:00:02,000\r\nHello\r\n\r\n2\r\n00:00:03,000 --> 00:00:04,000\r\nWorld\r\n\r\n";
        let track = SubtitleTrack::from_srt(input).unwrap();
        assert_eq!(track.len(), 2);
        assert_eq!(track.cues[0].text, "Hello");
        assert_eq!(track.cues[1].text, "World");
    }

    #[test]
    fn malformed_srt_fails() {
        let err = SubtitleTrack::from_srt("not an srt block\nwithout timing\n\n").unwrap_err();
        assert!(err.to_string().contains("bad SRT block"));
    }

    #[test]
    fn zero_duration_srt_fails() {
        let input = "1\n00:00:01,000 --> 00:00:01,000\nHello\n\n";
        let err = SubtitleTrack::from_srt(input).unwrap_err();
        assert!(err.to_string().contains("cue end must be after start"));
    }
}
