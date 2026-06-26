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

    pub fn from_vtt(vtt: &str) -> crate::error::Result<Self> {
        Ok(Self {
            cues: parse_vtt(vtt)?,
        })
    }

    pub fn from_ass(ass: &str) -> crate::error::Result<Self> {
        Ok(Self {
            cues: parse_ass(ass)?,
        })
    }

    pub fn from_lrc(lrc: &str) -> crate::error::Result<Self> {
        Ok(Self {
            cues: parse_lrc(lrc)?,
        })
    }

    /// 按显式格式或文件扩展名解析字幕文本。
    pub fn from_format(content: &str, format: &str) -> crate::error::Result<Self> {
        match format.trim().to_lowercase().as_str() {
            "srt" => Self::from_srt(content),
            "vtt" | "webvtt" => Self::from_vtt(content),
            "ass" | "ssa" => Self::from_ass(content),
            "lrc" => Self::from_lrc(content),
            other => Err(crate::error::FinalSubError::Validation(format!(
                "不支持解析的字幕格式：{other}"
            ))),
        }
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

    pub fn to_format(&self, format: &str) -> crate::error::Result<String> {
        match format.to_lowercase().as_str() {
            "srt" => Ok(self.to_srt()),
            "vtt" => Ok(serialize_vtt(&self.cues)),
            "txt" => Ok(serialize_txt(&self.cues)),
            "lrc" => Ok(serialize_lrc(&self.cues)),
            "ass" => Ok(serialize_ass(&self.cues)),
            _ => Err(crate::error::FinalSubError::Validation(format!(
                "不支持的字幕格式：{}",
                format
            ))),
        }
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
            parse_time_component("hours", parts[0])?,
            parse_time_component("minutes", parts[1])?,
            parts[2],
        ),
        2 => (0, parse_time_component("minutes", parts[0])?, parts[1]),
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
    if !sec.is_finite() || sec < 0.0 {
        return Err(crate::error::FinalSubError::Parse(format!(
            "bad seconds: {s_str}"
        )));
    }
    if parts.len() > 1 && sec >= 60.0 {
        return Err(crate::error::FinalSubError::Parse(format!(
            "seconds out of range: {s_str}"
        )));
    }
    if parts.len() > 1 && m >= 60 {
        return Err(crate::error::FinalSubError::Parse(format!(
            "minutes out of range: {m}"
        )));
    }
    let total_ms = h * 3_600_000 + m * 60_000 + (sec * 1000.0).round() as u64;
    Ok(total_ms)
}

fn parse_time_component(name: &str, raw: &str) -> crate::error::Result<u64> {
    raw.parse::<u64>()
        .map_err(|_| crate::error::FinalSubError::Parse(format!("bad {name}: {raw}")))
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

pub fn format_vtt_time(ms: u64) -> String {
    let h = ms / 3_600_000;
    let m = (ms % 3_600_000) / 60_000;
    let s = (ms % 60_000) / 1_000;
    let millis = ms % 1_000;
    format!("{h:02}:{m:02}:{s:02}.{millis:03}")
}

pub fn serialize_vtt(cues: &[Cue]) -> String {
    let mut out = String::from("WEBVTT\n\n");
    for (i, cue) in cues.iter().enumerate() {
        let idx = i + 1;
        out.push_str(&format!(
            "{idx}\n{} --> {}\n{}\n\n",
            format_vtt_time(cue.start_ms),
            format_vtt_time(cue.end_ms),
            cue.text
        ));
    }
    out
}

pub fn serialize_txt(cues: &[Cue]) -> String {
    cues.iter()
        .map(|c| c.text.as_str())
        .collect::<Vec<&str>>()
        .join("\n")
}

pub fn serialize_lrc(cues: &[Cue]) -> String {
    let mut out = String::new();
    for cue in cues {
        let min = cue.start_ms / 60_000;
        let sec = (cue.start_ms % 60_000) / 1000;
        let centis = (cue.start_ms % 1000) / 10;
        out.push_str(&format!("[{min:02}:{sec:02}.{centis:02}]{}\n", cue.text));
    }
    out
}

pub fn format_ass_time(ms: u64) -> String {
    let h = ms / 3_600_000;
    let m = (ms % 3_600_000) / 60_000;
    let s = (ms % 60_000) / 1_000;
    let centis = (ms % 1_000) / 10;
    format!("{h}:{m:02}:{s:02}.{centis:02}")
}

pub fn serialize_ass(cues: &[Cue]) -> String {
    let mut out = String::from(
        "[Script Info]\n\
         ScriptType: v4.00+\n\
         PlayResX: 384\n\
         PlayResY: 288\n\n\
         [V4+ Styles]\n\
         Format: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\n\
         Style: Default,Arial,16,&H00FFFFFF,&H000000FF,&H00000000,&H00000000,-1,0,0,0,100,100,0,0,1,2,2,2,10,10,10,1\n\n\
         [Events]\n\
         Format: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n"
    );
    for cue in cues {
        out.push_str(&format!(
            "Dialogue: 0,{},{},Default,,0,0,0,,{}\n",
            format_ass_time(cue.start_ms),
            format_ass_time(cue.end_ms),
            cue.text.replace('\n', "\\N")
        ));
    }
    out
}

/// end 必须晚于 start；外部格式可能给出非法或缺失的 end，统一兜底为 start + 2s。
fn ensure_end_after_start(start_ms: u64, end_ms: u64) -> u64 {
    if end_ms > start_ms {
        end_ms
    } else {
        start_ms + 2000
    }
}

pub fn parse_vtt(vtt: &str) -> crate::error::Result<Vec<Cue>> {
    let normalized = vtt.replace("\r\n", "\n").replace('\r', "\n");
    let mut cues = Vec::new();
    for block in normalized.split("\n\n") {
        let block = block.trim();
        if block.is_empty() || block.starts_with("WEBVTT") {
            continue;
        }
        // NOTE / STYLE / REGION 等元数据块跳过。
        let first = block.lines().next().unwrap_or("");
        if first.starts_with("NOTE") || first.starts_with("STYLE") || first.starts_with("REGION") {
            continue;
        }
        let Some(timing_line) = block.lines().find(|l| l.contains("-->")) else {
            continue;
        };
        let arrow = "-->";
        let arrow_pos = timing_line.find(arrow).unwrap();
        let start_str = timing_line[..arrow_pos].trim();
        // 右侧可能带 cue settings（align/position 等），取第一段。
        let end_str = timing_line[arrow_pos + arrow.len()..]
            .split_whitespace()
            .next()
            .unwrap_or("");
        let start_ms = parse_srt_time(start_str)?;
        let end_ms = ensure_end_after_start(start_ms, parse_srt_time(end_str)?);
        let text = block
            .lines()
            .skip_while(|l| !l.contains("-->"))
            .skip(1)
            .collect::<Vec<&str>>()
            .join("\n")
            .trim()
            .to_string();
        if text.is_empty() {
            continue;
        }
        cues.push(Cue {
            index: (cues.len() + 1) as u32,
            start_ms,
            end_ms,
            text,
        });
    }
    if cues.is_empty() {
        return Err(crate::error::FinalSubError::Parse(
            "no valid VTT cues found".into(),
        ));
    }
    Ok(cues)
}

pub fn parse_ass(ass: &str) -> crate::error::Result<Vec<Cue>> {
    let normalized = ass.replace("\r\n", "\n").replace('\r', "\n");
    let mut cues = Vec::new();
    for line in normalized.lines() {
        let line = line.trim();
        if !line.starts_with("Dialogue:") {
            continue;
        }
        let payload = line.trim_start_matches("Dialogue:").trim_start();
        // 字段：Layer,Start,End,Style,Name,MarginL,MarginR,MarginV,Effect,Text
        // Text 自身可含逗号，故仅按前 9 个逗号切分。
        let parts: Vec<&str> = payload.splitn(10, ',').collect();
        if parts.len() < 10 {
            continue;
        }
        let start_ms = parse_srt_time(parts[1].trim())?;
        let end_ms = ensure_end_after_start(start_ms, parse_srt_time(parts[2].trim())?);
        let text = strip_ass_text(parts[9]);
        if text.is_empty() {
            continue;
        }
        cues.push(Cue {
            index: (cues.len() + 1) as u32,
            start_ms,
            end_ms,
            text,
        });
    }
    if cues.is_empty() {
        return Err(crate::error::FinalSubError::Parse(
            "no valid ASS dialogue lines found".into(),
        ));
    }
    Ok(cues)
}

/// 去除 ASS 行内覆盖标签 `{...}`，并把换行符 `\N` / `\n` 还原为真实换行。
fn strip_ass_text(raw: &str) -> String {
    let mut out = String::new();
    let mut in_brace = false;
    let mut chars = raw.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '{' => in_brace = true,
            '}' => in_brace = false,
            '\\' if !in_brace => {
                if let Some(&next) = chars.peek() {
                    if next == 'N' || next == 'n' {
                        chars.next();
                        out.push('\n');
                        continue;
                    }
                }
                out.push('\\');
            }
            _ if !in_brace => out.push(c),
            _ => {}
        }
    }
    out.trim().to_string()
}

pub fn parse_lrc(lrc: &str) -> crate::error::Result<Vec<Cue>> {
    let normalized = lrc.replace("\r\n", "\n").replace('\r', "\n");
    let mut raw: Vec<(u64, String)> = Vec::new();
    for line in normalized.lines() {
        let line = line.trim();
        // 一行可带多个时间标签：[00:01.00][00:05.00]文本
        let mut rest = line;
        let mut stamps = Vec::new();
        while rest.starts_with('[') {
            let Some(close) = rest.find(']') else { break };
            let tag = &rest[1..close];
            // 仅接受形如 mm:ss(.cc) 的时间标签，跳过 [ti:]/[ar:] 等元数据。
            if let Some(colon) = tag.find(':') {
                if tag[..colon].chars().all(|c| c.is_ascii_digit()) && colon > 0 {
                    if let Ok(ms) = parse_srt_time(tag) {
                        stamps.push(ms);
                    }
                }
            }
            rest = rest[close + 1..].trim_start();
        }
        let text = rest.trim().to_string();
        if stamps.is_empty() || text.is_empty() {
            continue;
        }
        for ms in stamps {
            raw.push((ms, text.clone()));
        }
    }
    raw.sort_by_key(|(ms, _)| *ms);
    let mut cues = Vec::new();
    for i in 0..raw.len() {
        let start_ms = raw[i].0;
        let end_ms = raw
            .get(i + 1)
            .map(|(next, _)| *next)
            .filter(|next| *next > start_ms)
            .unwrap_or(start_ms + 4000);
        cues.push(Cue {
            index: (i + 1) as u32,
            start_ms,
            end_ms,
            text: raw[i].1.clone(),
        });
    }
    if cues.is_empty() {
        return Err(crate::error::FinalSubError::Parse(
            "no valid LRC lines found".into(),
        ));
    }
    Ok(cues)
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
        assert_eq!(parse_srt_time("90").unwrap(), 90_000);
        assert_eq!(parse_srt_time("01:30.500").unwrap(), 90_500);
    }

    #[test]
    fn parse_srt_time_rejects_invalid_components() {
        assert!(parse_srt_time("xx:01:02,000")
            .unwrap_err()
            .to_string()
            .contains("bad hours"));
        assert!(parse_srt_time("00:yy:02,000")
            .unwrap_err()
            .to_string()
            .contains("bad minutes"));
        assert!(parse_srt_time("00:60:02,000")
            .unwrap_err()
            .to_string()
            .contains("minutes out of range"));
        assert!(parse_srt_time("00:01:60,000")
            .unwrap_err()
            .to_string()
            .contains("seconds out of range"));
        assert!(parse_srt_time("-1")
            .unwrap_err()
            .to_string()
            .contains("bad seconds"));
        assert!(parse_srt_time("NaN")
            .unwrap_err()
            .to_string()
            .contains("bad seconds"));
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

    #[test]
    fn to_format_conversions() {
        let input = "1\n00:00:01,000 --> 00:00:03,500\nHello world\n\n";
        let track = SubtitleTrack::from_srt(input).unwrap();

        let vtt = track.to_format("vtt").unwrap();
        assert!(vtt.contains("WEBVTT"));
        assert!(vtt.contains("00:00:01.000 --> 00:00:03.500"));

        let txt = track.to_format("txt").unwrap();
        assert_eq!(txt, "Hello world");

        let lrc = track.to_format("lrc").unwrap();
        assert!(lrc.contains("[00:01.00]Hello world"));

        let ass = track.to_format("ass").unwrap();
        assert!(ass.contains("[Events]"));
        assert!(ass.contains("Dialogue: 0,0:00:01.00,0:00:03.50,Default,,0,0,0,,Hello world"));
    }

    #[test]
    fn parse_vtt_basic() {
        let vtt = "WEBVTT\n\n1\n00:00:01.000 --> 00:00:03.500 align:start\nHello world\n\n00:00:04.000 --> 00:00:06.000\nSecond line";
        let track = SubtitleTrack::from_vtt(vtt).unwrap();
        assert_eq!(track.len(), 2);
        assert_eq!(track.cues[0].start_ms, 1000);
        assert_eq!(track.cues[0].end_ms, 3500);
        assert_eq!(track.cues[0].text, "Hello world");
        assert_eq!(track.cues[1].text, "Second line");
    }

    #[test]
    fn parse_vtt_skips_note_blocks() {
        let vtt = "WEBVTT\n\nNOTE this is a comment\n\n00:00:01.000 --> 00:00:02.000\nText";
        let track = SubtitleTrack::from_vtt(vtt).unwrap();
        assert_eq!(track.len(), 1);
        assert_eq!(track.cues[0].text, "Text");
    }

    #[test]
    fn parse_ass_dialogue() {
        let ass = "[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\nDialogue: 0,0:00:01.00,0:00:03.50,Default,,0,0,0,,{\\b1}Hello,\\Nworld";
        let track = SubtitleTrack::from_ass(ass).unwrap();
        assert_eq!(track.len(), 1);
        assert_eq!(track.cues[0].start_ms, 1000);
        assert_eq!(track.cues[0].end_ms, 3500);
        // 覆盖标签 {\b1} 去除，\N 转换行，文本内逗号保留。
        assert_eq!(track.cues[0].text, "Hello,\nworld");
    }

    #[test]
    fn parse_lrc_lines() {
        let lrc = "[ti:Song]\n[00:01.00]First line\n[00:05.00]Second line";
        let track = SubtitleTrack::from_lrc(lrc).unwrap();
        assert_eq!(track.len(), 2);
        assert_eq!(track.cues[0].start_ms, 1000);
        assert_eq!(track.cues[0].end_ms, 5000);
        assert_eq!(track.cues[1].start_ms, 5000);
        assert_eq!(track.cues[1].text, "Second line");
    }

    #[test]
    fn from_format_dispatches() {
        let vtt = "WEBVTT\n\n00:00:01.000 --> 00:00:02.000\nHi";
        assert_eq!(SubtitleTrack::from_format(vtt, "vtt").unwrap().len(), 1);
        let err = SubtitleTrack::from_format("x", "docx").unwrap_err();
        assert!(err.to_string().contains("不支持解析的字幕格式"));
    }
}
