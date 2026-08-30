use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;

use tokio::process::Command;
use unicode_segmentation::UnicodeSegmentation;

use crate::core::subtitle::{
    format_ass_time, looks_like_bilingual_track, split_bilingual_text, subtitle_visual_width, Cue,
    SubtitleTrack, MAX_SUBTITLE_FILE_BYTES,
};

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

// Netflix timed-text guidance is expressed relative to the video canvas
// (roughly 42 characters per line and two lines maximum), so the generated
// ASS uses a 1280x720 reference canvas instead of libass's 384x288 fallback.
const BILINGUAL_PLAY_RES_X: u32 = 1280;
const BILINGUAL_PLAY_RES_Y: u32 = 720;
const BILINGUAL_LATIN_MAX_LINE_CHARS: usize = 42;
const BILINGUAL_CJK_MAX_LINE_CHARS: usize = 16;
const NETFLIX_MAX_EVENT_LINES: usize = 2;
const BILINGUAL_MAX_LINES_PER_LANGUAGE: usize = 1;
const BILINGUAL_LANE_GAP: u32 = 10;
const BILINGUAL_MAX_FONT_SIZE: u32 = 32;
const BILINGUAL_MIN_EVENT_MS: u64 = 833;
const BILINGUAL_MAX_EVENT_MS: u64 = 7_000;

/// A subtitle path plus an optional generated ASS sidecar. The sidecar is
/// removed automatically after the FFmpeg operation that owns this value.
pub struct PreparedSubtitle {
    path: PathBuf,
    cleanup_path: Option<PathBuf>,
}

impl PreparedSubtitle {
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for PreparedSubtitle {
    fn drop(&mut self) {
        if let Some(path) = self.cleanup_path.take() {
            let _ = std::fs::remove_file(path);
        }
    }
}

fn ass_safe_text(text: &str) -> String {
    // ASS uses braces/backslashes for override tags. Replacing these literal
    // characters prevents subtitle text from injecting a tag.
    text.replace('\\', "／")
        .replace('{', "｛")
        .replace('}', "｝")
        .replace('\r', "")
}

fn ass_safe_font_name(value: Option<&str>) -> String {
    let candidate = value.unwrap_or("PingFang SC").trim();
    if candidate.is_empty()
        || candidate
            .chars()
            .any(|character| character.is_control() || matches!(character, ',' | '\\'))
    {
        "PingFang SC".into()
    } else {
        candidate.to_string()
    }
}

fn ass_safe_color(value: Option<&str>, fallback: &str) -> String {
    let candidate = value.unwrap_or(fallback).trim();
    let valid = candidate.len() == 10
        && candidate.starts_with("&H")
        && candidate[2..]
            .chars()
            .all(|character| character.is_ascii_hexdigit());
    if valid {
        candidate.to_ascii_uppercase()
    } else {
        fallback.into()
    }
}

fn contains_cjk(text: &str) -> bool {
    text.chars().any(|character| {
        let codepoint = character as u32;
        (0x2e80..=0x2fff).contains(&codepoint)
            || (0x3040..=0x30ff).contains(&codepoint)
            || (0x3400..=0x4dbf).contains(&codepoint)
            || (0x4e00..=0x9fff).contains(&codepoint)
            || (0xac00..=0xd7af).contains(&codepoint)
            || (0xf900..=0xfaff).contains(&codepoint)
    })
}

fn is_ass_preferred_break(grapheme: &str) -> bool {
    grapheme.chars().all(char::is_whitespace)
        || grapheme.chars().last().is_some_and(|character| {
            matches!(
                character,
                '。' | '．'
                    | '.'
                    | '！'
                    | '!'
                    | '？'
                    | '?'
                    | '…'
                    | '，'
                    | ','
                    | '；'
                    | ';'
                    | '、'
                    | '：'
                    | ':'
                    | '—'
                    | '-'
            )
        })
}

/// Apply the language-specific Netflix character cap. Prefer whitespace and
/// punctuation boundaries so product names and English words are not cut in
/// the middle just to hit the numeric cap.
fn split_ass_character_limit(text: &str, max_chars: usize) -> Vec<String> {
    if max_chars == 0 {
        return vec![text.trim().to_string()];
    }
    let graphemes: Vec<&str> = UnicodeSegmentation::graphemes(text, true).collect();
    if graphemes.len() <= max_chars {
        return vec![text.trim().to_string()];
    }
    let mut lines = Vec::new();
    let mut start = 0usize;
    while start < graphemes.len() {
        while start < graphemes.len() && graphemes[start].chars().all(char::is_whitespace) {
            start += 1;
        }
        if start >= graphemes.len() {
            break;
        }
        let mut end = start;
        let mut preferred_cut = None;
        while end < graphemes.len() && end - start < max_chars {
            end += 1;
            if is_ass_preferred_break(graphemes[end - 1]) {
                preferred_cut = Some(end);
            }
        }
        if end == graphemes.len() {
            let line = graphemes[start..end].concat().trim().to_string();
            if !line.is_empty() {
                lines.push(line);
            }
            break;
        }
        let cut = preferred_cut
            .filter(|candidate| *candidate > start)
            .unwrap_or(end);
        let line = graphemes[start..cut].concat().trim().to_string();
        if !line.is_empty() {
            lines.push(line);
        }
        start = cut;
    }
    if lines.is_empty() {
        vec![text.trim().to_string()]
    } else {
        lines
    }
}

fn wrapped_ass_lines(text: &str) -> Vec<String> {
    let is_cjk = contains_cjk(text);
    let max_characters = if is_cjk {
        BILINGUAL_CJK_MAX_LINE_CHARS
    } else {
        BILINGUAL_LATIN_MAX_LINE_CHARS
    };
    let mut lines = Vec::new();
    // Some subtitle exporters use the ASS line-break token inside an SRT/VTT
    // cue. Treat it as a real line break before applying the Netflix width
    // limit; literal braces/backslashes are escaped later when serialized.
    for line in text.replace("\\N", "\n").replace('\r', "").lines() {
        lines.extend(split_ass_character_limit(line.trim(), max_characters));
    }
    if lines.is_empty() {
        vec![String::new()]
    } else {
        lines
    }
}

fn wrapped_ass_blocks(text: &str, max_lines: usize) -> Vec<Vec<String>> {
    wrapped_ass_lines(text)
        .chunks(max_lines.max(1))
        .map(|lines| lines.to_vec())
        .collect()
}

fn render_block_width(lines: &[String]) -> u64 {
    lines
        .iter()
        .map(|line| subtitle_visual_width(line) as u64)
        .max()
        .unwrap_or(1)
        .max(1)
}

fn render_block_for_segment(
    blocks: &[Vec<String>],
    segment_index: usize,
    segment_count: usize,
) -> Vec<String> {
    if blocks.is_empty() {
        return Vec::new();
    }
    if blocks.len() == 1 {
        return blocks[0].clone();
    }
    // Advance at the start of a segment and hold the final block when the two
    // languages need different numbers of segments. This avoids showing the
    // first translated line twice before ever revealing the next one.
    let count = segment_count.max(1);
    let block_index = (((segment_index + 1) * blocks.len()).div_ceil(count))
        .saturating_sub(1)
        .min(blocks.len().saturating_sub(1));
    blocks[block_index].clone()
}

fn split_bilingual_text_for_render(text: &str, force_bilingual: bool) -> Option<(String, String)> {
    if let Some(pair) = split_bilingual_text(text) {
        return Some(pair);
    }
    if !force_bilingual {
        return None;
    }

    // A few ASR/export paths produce a bilingual cue whose two lines contain
    // only punctuation, numbers, or a short token (for example "." / "。").
    // Once the surrounding track is known to be bilingual, preserve those two
    // physical lines as separate lanes instead of collapsing them into one ASS
    // event. Do not guess for arbitrary multiline single-language subtitles.
    let lines: Vec<String> = text
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .collect();
    if lines.len() != 2 {
        return None;
    }
    let first_cjk = contains_cjk(&lines[0]);
    let second_cjk = contains_cjk(&lines[1]);
    let first_latin = lines[0]
        .chars()
        .any(|character| character.is_ascii_alphabetic());
    let second_latin = lines[1]
        .chars()
        .any(|character| character.is_ascii_alphabetic());
    let both_same_known_script = (first_cjk && second_cjk) || (first_latin && second_latin);
    if !both_same_known_script {
        Some((lines[0].clone(), lines[1].clone()))
    } else {
        None
    }
}

fn ascii_word_at_start(text: &str) -> Option<&str> {
    text.trim_start()
        .split(|character: char| !character.is_ascii_alphabetic())
        .next()
        .filter(|word| !word.is_empty())
}

fn ascii_word_at_end(text: &str) -> Option<&str> {
    text.trim_end()
        .rsplit(|character: char| !character.is_ascii_alphabetic())
        .next()
        .filter(|word| !word.is_empty())
}

fn is_common_short_english_word(word: &str) -> bool {
    matches!(
        word.to_ascii_lowercase().as_str(),
        "a" | "an"
            | "as"
            | "at"
            | "be"
            | "by"
            | "do"
            | "go"
            | "he"
            | "if"
            | "in"
            | "is"
            | "it"
            | "me"
            | "my"
            | "no"
            | "of"
            | "on"
            | "or"
            | "so"
            | "to"
            | "up"
            | "us"
            | "we"
            | "am"
    )
}

/// Detect the small class of ASR boundaries that cut one English word across
/// two cues. Common short words are excluded so normal phrase boundaries are
/// left intact.
fn looks_like_split_english_word(previous: &str, next: &str) -> bool {
    let Some(previous_word) = ascii_word_at_end(previous) else {
        return false;
    };
    let Some(next_word) = ascii_word_at_start(next) else {
        return false;
    };
    let Some(first) = next_word.chars().next() else {
        return false;
    };
    let previous_trimmed = previous.trim_end();
    let fragment_starts_after_apostrophe = previous_trimmed
        .strip_suffix(previous_word)
        .and_then(|prefix| prefix.chars().last())
        .is_some_and(|character| matches!(character, '\'' | '’'));
    previous_word.len() <= 2
        && previous_word
            .chars()
            .any(|character| character.is_ascii_lowercase())
        && previous_trimmed.ends_with(previous_word)
        && !fragment_starts_after_apostrophe
        && first.is_ascii_lowercase()
        && !is_common_short_english_word(previous_word)
}

fn join_render_language_parts(left: &str, right: &str, force_no_space: bool) -> String {
    let left = left.trim_end();
    let right = right.trim_start();
    if left.is_empty() {
        return right.to_string();
    }
    if right.is_empty() {
        return left.to_string();
    }
    let left_ascii = left
        .chars()
        .last()
        .is_some_and(|character| character.is_ascii_alphanumeric());
    let right_ascii = right
        .chars()
        .next()
        .is_some_and(|character| character.is_ascii_alphanumeric());
    let likely_fragment_boundary = ascii_word_at_end(left)
        .zip(ascii_word_at_start(right))
        .is_some_and(|(left_word, right_word)| {
            left_word.len() <= 2
                && right_word
                    .chars()
                    .next()
                    .is_some_and(|character| character.is_ascii_lowercase())
        });
    if left_ascii && right_ascii && !(force_no_space || likely_fragment_boundary) {
        format!("{left} {right}")
    } else {
        format!("{left}{right}")
    }
}

fn merge_render_bilingual_text(left: &(String, String), right: &(String, String)) -> String {
    format!(
        "{}\n{}",
        join_render_language_parts(&left.0, &right.0, true),
        join_render_language_parts(&left.1, &right.1, false)
    )
}

/// Repair only word-split boundaries for rendering. The user's exported SRT
/// and its original cue timing remain untouched; the generated ASS sidecar is
/// allowed to show the complete word before applying Netflix line limits.
fn coalesce_split_word_cues(track: &SubtitleTrack) -> SubtitleTrack {
    let mut cues: Vec<Cue> = Vec::with_capacity(track.cues.len());
    for cue in &track.cues {
        let should_merge = cues.last().is_some_and(|previous| {
            previous.end_ms <= cue.start_ms
                && cue.start_ms - previous.end_ms <= 120
                && split_bilingual_text_for_render(&previous.text, true)
                    .zip(split_bilingual_text_for_render(&cue.text, true))
                    .is_some_and(|(left, right)| looks_like_split_english_word(&left.0, &right.0))
        });
        if should_merge {
            if let Some(previous) = cues.last_mut() {
                let left = split_bilingual_text_for_render(&previous.text, true);
                let right = split_bilingual_text_for_render(&cue.text, true);
                if let (Some(left), Some(right)) = (left, right) {
                    previous.text = merge_render_bilingual_text(&left, &right);
                    previous.end_ms = previous.end_ms.max(cue.end_ms);
                    continue;
                }
            }
        }
        cues.push(cue.clone());
    }
    SubtitleTrack { cues }
}

#[derive(Debug, Clone)]
struct BilingualRenderSegment {
    start_ms: u64,
    end_ms: u64,
    first_lines: Vec<String>,
    second_lines: Vec<String>,
}

/// Split an overlong cue into Netflix-sized events without changing the
/// source SRT on disk. A bilingual event uses one line per language, keeping
/// the Netflix two-line maximum for the complete event. If one language needs
/// fewer segments, its nearest block remains visible while the other advances.
fn bilingual_render_segments(cue: &Cue, force_bilingual: bool) -> Vec<BilingualRenderSegment> {
    let duration = cue.end_ms.saturating_sub(cue.start_ms).max(1);
    let (first_blocks, second_blocks) =
        match split_bilingual_text_for_render(&cue.text, force_bilingual) {
            Some((first, second)) => (
                wrapped_ass_blocks(&first, BILINGUAL_MAX_LINES_PER_LANGUAGE),
                wrapped_ass_blocks(&second, BILINGUAL_MAX_LINES_PER_LANGUAGE),
            ),
            None => (
                wrapped_ass_blocks(&cue.text, NETFLIX_MAX_EVENT_LINES),
                Vec::new(),
            ),
        };
    let duration_segment_count = duration
        .saturating_add(BILINGUAL_MAX_EVENT_MS.saturating_sub(1))
        .checked_div(BILINGUAL_MAX_EVENT_MS)
        .unwrap_or(1) as usize;
    let desired_count = first_blocks
        .len()
        .max(second_blocks.len())
        .max(duration_segment_count)
        .max(1);
    // A one-millisecond cue cannot hold an arbitrary number of events. This
    // only affects malformed/very short external files; normal media cues are
    // long enough to retain every wrapped block.
    let segment_count = desired_count.min(duration as usize).max(1);
    let mut selected = Vec::with_capacity(segment_count);
    for index in 0..segment_count {
        let first_lines = render_block_for_segment(&first_blocks, index, segment_count);
        let second_lines = render_block_for_segment(&second_blocks, index, segment_count);
        let weight = render_block_width(&first_lines).max(render_block_width(&second_lines));
        selected.push((first_lines, second_lines, weight));
    }

    let total_weight = selected
        .iter()
        .map(|(_, _, weight)| *weight)
        .sum::<u64>()
        .max(1);
    let min_event = if duration >= BILINGUAL_MIN_EVENT_MS.saturating_mul(segment_count as u64) {
        BILINGUAL_MIN_EVENT_MS
    } else {
        1
    };
    let mut cursor = cue.start_ms;
    let mut accumulated_weight = 0u64;
    selected
        .into_iter()
        .enumerate()
        .map(|(index, (first_lines, second_lines, weight))| {
            accumulated_weight = accumulated_weight.saturating_add(weight);
            let is_last = index + 1 == segment_count;
            let end_ms = if is_last {
                cue.end_ms
            } else {
                let weighted_offset = (duration as u128)
                    .saturating_mul(accumulated_weight as u128)
                    .checked_div(total_weight as u128)
                    .unwrap_or(0) as u64;
                let remaining = (segment_count - index - 1) as u64;
                let latest_end = cue
                    .end_ms
                    .saturating_sub(min_event.saturating_mul(remaining));
                let earliest_end = cursor.saturating_add(min_event).min(cue.end_ms);
                let max_end = cursor
                    .saturating_add(BILINGUAL_MAX_EVENT_MS)
                    .min(cue.end_ms);
                let upper_bound = latest_end.min(max_end).max(earliest_end).min(cue.end_ms);
                cue.start_ms
                    .saturating_add(weighted_offset)
                    .max(earliest_end)
                    .min(upper_bound)
            };
            let segment = BilingualRenderSegment {
                start_ms: cursor,
                end_ms,
                first_lines,
                second_lines,
            };
            cursor = end_ms;
            segment
        })
        .collect()
}

fn bilingual_font_size(style: &BurnInStyleOptions) -> u32 {
    // The UI value is a logical pixel size at the 1280x720 reference canvas.
    // Do not multiply it by libass's legacy 384x288 fallback scale: a nominal
    // 24px SRT otherwise becomes roughly 60px on a 720p video.
    style
        .font_size
        .unwrap_or(24)
        .clamp(10, BILINGUAL_MAX_FONT_SIZE)
}

fn bilingual_alignment(style: &BurnInStyleOptions) -> (u8, u32) {
    let alignment = style.alignment.unwrap_or(2).clamp(1, 9);
    let column = (alignment - 1) % 3;
    let anchor = 7 + column; // top-left / top-center / top-right
    let x = match column {
        0 => 64,
        1 => BILINGUAL_PLAY_RES_X / 2,
        _ => BILINGUAL_PLAY_RES_X - 64,
    };
    (anchor, x)
}

fn bilingual_vertical_start(alignment: u8, margin: u32, total_height: u32) -> u32 {
    let max_start = BILINGUAL_PLAY_RES_Y
        .saturating_sub(margin)
        .saturating_sub(total_height);
    let proposed = if alignment >= 7 {
        margin
    } else if alignment >= 4 {
        BILINGUAL_PLAY_RES_Y.saturating_sub(total_height) / 2
    } else {
        BILINGUAL_PLAY_RES_Y
            .saturating_sub(margin)
            .saturating_sub(total_height)
    };
    proposed.min(max_start)
}

/// Serialize a bilingual track as an explicit two-lane ASS script.
///
/// Each language block becomes its own Dialogue event with an explicit
/// position. This avoids renderer-dependent collapsing of two SRT lines while
/// preserving the selected top/center/bottom and left/center/right alignment.
pub fn serialize_bilingual_ass(track: &SubtitleTrack, style: &BurnInStyleOptions) -> String {
    let font_size = bilingual_font_size(style);
    let font_name = ass_safe_font_name(style.font_name.as_deref());
    let primary = ass_safe_color(style.font_color.as_deref(), "&H00FFFFFF");
    let outline = ass_safe_color(style.outline_color.as_deref(), "&H00000000");
    let background = ass_safe_color(style.background_color.as_deref(), "&H80000000");
    let outline_width = style.outline_width.unwrap_or(2.0).clamp(0.0, 10.0);
    let shadow = style.shadow.unwrap_or(0.0).clamp(0.0, 20.0);
    let border_style = if style.opaque_background.unwrap_or(false) {
        3
    } else {
        1
    };
    let back_colour = if border_style == 3 {
        background
    } else {
        "&H00000000".into()
    };
    let margin = style.margin_v.unwrap_or(30).min(BILINGUAL_PLAY_RES_Y / 2);
    let line_height = ((font_size as f32 * 1.25).round() as u32).max(font_size.saturating_add(4));
    let (anchor, x) = bilingual_alignment(style);
    let alignment = style.alignment.unwrap_or(2).clamp(1, 9);

    let mut out = format!(
        "[Script Info]\nScriptType: v4.00+\nCollisions: Normal\nPlayResX: {BILINGUAL_PLAY_RES_X}\nPlayResY: {BILINGUAL_PLAY_RES_Y}\nWrapStyle: 2\nScaledBorderAndShadow: yes\n\n[V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\nStyle: Bilingual,{font_name},{font_size},{primary},&H000000FF,{outline},{back_colour},0,0,0,0,100,100,0,0,{border_style},{outline_width:.2},{shadow:.2},8,48,48,{margin},1\n\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n"
    );

    let track_is_bilingual = looks_like_bilingual_track(track, false);
    let render_track = coalesce_split_word_cues(track);
    for cue in &render_track.cues {
        for segment in bilingual_render_segments(cue, track_is_bilingual) {
            let mut rendered_blocks = Vec::new();
            if !segment.first_lines.is_empty() {
                rendered_blocks.push(segment.first_lines);
            }
            if !segment.second_lines.is_empty() {
                rendered_blocks.push(segment.second_lines);
            }
            let total_lines = rendered_blocks.iter().map(Vec::len).sum::<usize>().max(1) as u32;
            let total_height = total_lines.saturating_mul(line_height).saturating_add(
                BILINGUAL_LANE_GAP.saturating_mul(rendered_blocks.len().saturating_sub(1) as u32),
            );
            let mut y = bilingual_vertical_start(alignment, margin, total_height);
            for (block_index, lines) in rendered_blocks.iter().enumerate() {
                let text = lines
                    .iter()
                    .map(|line| ass_safe_text(line))
                    .collect::<Vec<_>>()
                    .join("\\N");
                out.push_str(&format!(
                    "Dialogue: 0,{},{},Bilingual,,0,0,0,,{{\\an{anchor}\\pos({x},{y})\\fs{font_size}}}{text}\n",
                    format_ass_time(segment.start_ms),
                    format_ass_time(segment.end_ms),
                ));
                y = y
                    .saturating_add((lines.len() as u32).saturating_mul(line_height))
                    .saturating_add(if block_index + 1 < rendered_blocks.len() {
                        BILINGUAL_LANE_GAP
                    } else {
                        0
                    });
            }
        }
    }
    out
}

fn subtitle_format_for_path(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("srt")
        .to_ascii_lowercase()
        .as_str()
    {
        "vtt" => "vtt",
        "ass" | "ssa" => "ass",
        "lrc" => "lrc",
        _ => "srt",
    }
}

/// Prepare a subtitle path for deterministic rendering. Plain/non-bilingual
/// files are returned untouched; bilingual SRT/VTT/LRC files receive a unique
/// temporary ASS sidecar that is deleted when the returned guard is dropped.
pub async fn prepare_subtitle_for_render(
    subtitle_path: &Path,
    temp_dir: &Path,
    style: &BurnInStyleOptions,
) -> Result<PreparedSubtitle, String> {
    let extension = subtitle_path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if matches!(extension.as_str(), "ass" | "ssa") {
        return Ok(PreparedSubtitle {
            path: subtitle_path.to_path_buf(),
            cleanup_path: None,
        });
    }

    let metadata = tokio::fs::metadata(subtitle_path)
        .await
        .map_err(|error| format!("无法读取字幕文件信息：{error}"))?;
    if metadata.len() > MAX_SUBTITLE_FILE_BYTES {
        return Err(format!(
            "字幕文件超过 {} MB，无法准备渲染",
            MAX_SUBTITLE_FILE_BYTES / (1024 * 1024)
        ));
    }
    let content = tokio::fs::read_to_string(subtitle_path)
        .await
        .map_err(|error| format!("无法读取字幕文件：{error}"))?;
    let track = match SubtitleTrack::from_format(&content, subtitle_format_for_path(subtitle_path))
    {
        Ok(track) => track,
        Err(_) => {
            // Keep the existing FFmpeg error/reporting path for malformed
            // external subtitle files instead of changing import semantics.
            return Ok(PreparedSubtitle {
                path: subtitle_path.to_path_buf(),
                cleanup_path: None,
            });
        }
    };
    let filename_marked = subtitle_path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .map(|stem| {
            let lower = stem.to_ascii_lowercase();
            lower.contains("bilingual") || stem.contains("双语")
        })
        .unwrap_or(false);
    if !looks_like_bilingual_track(&track, filename_marked) {
        return Ok(PreparedSubtitle {
            path: subtitle_path.to_path_buf(),
            cleanup_path: None,
        });
    }

    tokio::fs::create_dir_all(temp_dir)
        .await
        .map_err(|error| format!("无法创建字幕渲染临时目录：{error}"))?;
    let id = uuid::Uuid::new_v4().to_string();
    let partial_path = temp_dir.join(format!("finalsub-bilingual-{id}.ass.part"));
    let ass_path = temp_dir.join(format!("finalsub-bilingual-{id}.ass"));
    let ass = serialize_bilingual_ass(&track, style);
    if let Err(error) = tokio::fs::write(&partial_path, ass).await {
        let _ = tokio::fs::remove_file(&partial_path).await;
        return Err(format!("无法写入双语字幕渲染文件：{error}"));
    }
    if let Err(error) = tokio::fs::rename(&partial_path, &ass_path).await {
        let _ = tokio::fs::remove_file(&partial_path).await;
        return Err(format!("无法提交双语字幕渲染文件：{error}"));
    }
    Ok(PreparedSubtitle {
        path: ass_path.clone(),
        cleanup_path: Some(ass_path),
    })
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
    fn bilingual_ass_uses_netflix_reference_layout_without_legacy_scale() {
        let track = SubtitleTrack::from_srt(
            "1\n00:00:00,000 --> 00:00:03,000\nAI is useful.\nAI 很实用。\n\n",
        )
        .unwrap();
        let ass = serialize_bilingual_ass(
            &track,
            &BurnInStyleOptions {
                font_size: Some(24),
                alignment: Some(2),
                ..Default::default()
            },
        );
        assert!(ass.contains("PlayResX: 1280"));
        assert!(ass.contains("PlayResY: 720"));
        assert!(ass.contains("Style: Bilingual,PingFang SC,24,"));
        assert_eq!(ass.matches("Dialogue: ").count(), 2);
        assert!(ass.contains("\\an8\\pos(640,"));
        assert!(ass.contains("\\fs24"));
        assert!(ass.contains("AI is useful."));
        assert!(ass.contains("AI 很实用。"));
    }

    #[test]
    fn bilingual_ass_caps_oversized_font_and_preserves_opaque_background() {
        let track = SubtitleTrack::from_srt(
            "1\n00:00:00,000 --> 00:00:03,000\nAI is useful.\nAI 很实用。\n\n",
        )
        .unwrap();
        let ass = serialize_bilingual_ass(
            &track,
            &BurnInStyleOptions {
                font_size: Some(120),
                opaque_background: Some(true),
                ..Default::default()
            },
        );
        assert!(ass.contains("Style: Bilingual,PingFang SC,32,"));
        assert!(ass.contains(",0,0,0,0,100,100,0,0,3,2.00,0.00,8,"));
        assert!(ass.contains("\\fs32"));
        assert!(!ass.contains("\\fs120"));
    }

    #[test]
    fn bilingual_ass_segments_long_cues_to_two_lines_per_lane() {
        let track = SubtitleTrack::from_srt(
            "1\n00:00:00,000 --> 00:00:05,000\nThis is a deliberately long English sentence that should be segmented into multiple Netflix-sized subtitle events for readability.\n这是一条故意较长的中文句子，用于验证字幕会按照 Netflix 标准分成多个易读的事件。\n\n",
        )
        .unwrap();
        let segments = bilingual_render_segments(&track.cues[0], true);
        assert!(segments.len() > 1);
        assert_eq!(segments.first().unwrap().start_ms, 0);
        assert_eq!(segments.last().unwrap().end_ms, 5_000);
        for segment in &segments {
            assert!(segment.first_lines.len() <= BILINGUAL_MAX_LINES_PER_LANGUAGE);
            assert!(segment.second_lines.len() <= BILINGUAL_MAX_LINES_PER_LANGUAGE);
            assert!(
                segment.first_lines.len() + segment.second_lines.len() <= NETFLIX_MAX_EVENT_LINES
            );
        }
        let ass = serialize_bilingual_ass(&track, &BurnInStyleOptions::default());
        for event in ass.lines().filter(|line| line.starts_with("Dialogue: ")) {
            let text = event
                .split_once(",,{")
                .map(|(_, value)| value)
                .unwrap_or("");
            assert!(
                text.matches("\\N").count() <= 1,
                "event has more than two lines: {event}"
            );
        }
    }

    #[test]
    fn bilingual_ass_applies_language_specific_netflix_line_limits() {
        let track = SubtitleTrack::from_srt(
            "1\n00:00:00,000 --> 00:00:20,000\nThis is a long English sentence with more than forty-two characters so it must be split cleanly.\n当美国企业客户需要更多内存芯片时英伟达宣布提高价格并推出新的 GPU 产品。\n\n",
        )
        .unwrap();
        let segments = bilingual_render_segments(&track.cues[0], true);
        assert!(segments.len() >= 3);
        assert!(segments.iter().all(|segment| {
            segment.end_ms > segment.start_ms
                && segment.end_ms - segment.start_ms <= BILINGUAL_MAX_EVENT_MS
                && segment.end_ms - segment.start_ms >= BILINGUAL_MIN_EVENT_MS
        }));
        for segment in &segments {
            assert!(
                segment.first_lines.len() + segment.second_lines.len() <= NETFLIX_MAX_EVENT_LINES
            );
            for line in &segment.first_lines {
                assert!(
                    UnicodeSegmentation::graphemes(line.as_str(), true).count()
                        <= BILINGUAL_LATIN_MAX_LINE_CHARS,
                    "Latin line exceeds 42 characters: {line}"
                );
            }
            for line in &segment.second_lines {
                assert!(
                    UnicodeSegmentation::graphemes(line.as_str(), true).count()
                        <= BILINGUAL_CJK_MAX_LINE_CHARS,
                    "CJK line exceeds 16 characters: {line}"
                );
            }
        }
    }

    #[test]
    fn bilingual_ass_uses_two_total_lines_and_preserves_vertical_choice() {
        let track = SubtitleTrack::from_srt(
            "1\n00:00:00,000 --> 00:00:05,000\nThis English sentence is long enough to require another timed event under the Netflix limit.\n这条中文句子也足够长，需要拆分到下一个时间段显示。\n\n",
        )
        .unwrap();
        let segments = bilingual_render_segments(&track.cues[0], true);
        assert!(segments.len() > 1);
        assert!(segments.iter().all(|segment| {
            segment.first_lines.len() <= 1
                && segment.second_lines.len() <= 1
                && segment.first_lines.len() + segment.second_lines.len() <= 2
        }));

        let bottom = serialize_bilingual_ass(
            &track,
            &BurnInStyleOptions {
                alignment: Some(2),
                ..Default::default()
            },
        );
        let top = serialize_bilingual_ass(
            &track,
            &BurnInStyleOptions {
                alignment: Some(8),
                ..Default::default()
            },
        );
        assert!(bottom.contains("\\an8\\pos(640,620)"));
        assert!(top.contains("\\an8\\pos(640,30)"));
    }

    #[test]
    fn bilingual_render_repairs_word_split_only_at_adjacent_cues() {
        let track = SubtitleTrack {
            cues: vec![
                Cue {
                    index: 1,
                    start_ms: 0,
                    end_ms: 1_000,
                    text: "raising pr\n提高 pr".into(),
                },
                Cue {
                    index: 2,
                    start_ms: 1_000,
                    end_ms: 2_000,
                    text: "ices now\nices 价格".into(),
                },
            ],
        };
        let ass = serialize_bilingual_ass(&track, &BurnInStyleOptions::default());
        assert!(ass.contains("raising prices now"));
        assert!(!ass.contains("raising pr\\N"));
    }

    #[test]
    fn bilingual_render_does_not_merge_complete_short_words_or_abbreviations() {
        let track = SubtitleTrack {
            cues: vec![
                Cue {
                    index: 1,
                    start_ms: 0,
                    end_ms: 1_000,
                    text: "When the U.S.\n当美国".into(),
                },
                Cue {
                    index: 2,
                    start_ms: 1_000,
                    end_ms: 2_000,
                    text: "is ready\n已做好准备".into(),
                },
                Cue {
                    index: 3,
                    start_ms: 2_000,
                    end_ms: 3_000,
                    text: "about the\n关于这个".into(),
                },
                Cue {
                    index: 4,
                    start_ms: 3_000,
                    end_ms: 4_000,
                    text: "product\n产品".into(),
                },
                Cue {
                    index: 5,
                    start_ms: 4_000,
                    end_ms: 5_000,
                    text: "it's\n它正在".into(),
                },
                Cue {
                    index: 6,
                    start_ms: 5_000,
                    end_ms: 6_000,
                    text: "going well\n进展顺利".into(),
                },
            ],
        };
        let ass = serialize_bilingual_ass(&track, &BurnInStyleOptions::default());
        assert_eq!(ass.matches("Dialogue: ").count(), 12);
        assert!(!ass.contains("U.S.is ready"));
        assert!(!ass.contains("theproduct"));
        assert!(!ass.contains("it'sgoing"));
    }

    #[test]
    fn bilingual_ass_neutralizes_override_characters() {
        let track = SubtitleTrack::from_srt(
            "1\n00:00:00,000 --> 00:00:01,000\nHello {\\fs200}\\N world\n你好\n\n",
        )
        .unwrap();
        let ass = serialize_bilingual_ass(&track, &BurnInStyleOptions::default());
        assert!(!ass.contains("{\\fs200}"));
        assert!(ass.contains("｛／fs200｝"));
    }

    #[tokio::test]
    async fn prepare_bilingual_subtitle_creates_and_cleans_unique_ass_sidecar() {
        let fixture = tempfile::tempdir().unwrap();
        let subtitle = fixture.path().join("captions.finalsub.zh.bilingual.srt");
        std::fs::write(
            &subtitle,
            "1\n00:00:00,000 --> 00:00:01,000\nHello\n你好\n\n",
        )
        .unwrap();
        let prepared =
            prepare_subtitle_for_render(&subtitle, fixture.path(), &BurnInStyleOptions::default())
                .await
                .unwrap();
        let generated = prepared.path().to_path_buf();
        assert_eq!(
            generated.extension().and_then(|value| value.to_str()),
            Some("ass")
        );
        assert!(generated.is_file());
        assert_ne!(generated, subtitle);
        drop(prepared);
        assert!(!generated.exists());
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
