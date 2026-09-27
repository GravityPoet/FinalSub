use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityCue {
    pub start_ms: u64,
    pub end_ms: u64,
    pub source: String,
    #[serde(default)]
    pub target: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityIssue {
    pub cue_index: usize,
    pub start_ms: u64,
    pub end_ms: u64,
    pub code: String,
    pub value: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct QualityReport {
    pub cue_count: usize,
    pub affected_cues: usize,
    pub issues: Vec<QualityIssue>,
}

pub fn validate_cues(cues: &[QualityCue]) -> Result<(), String> {
    if cues.len() > 100_000
        || cues
            .iter()
            .map(|cue| cue.source.len() + cue.target.as_ref().map_or(0, String::len))
            .sum::<usize>()
            > 20 * 1024 * 1024
    {
        return Err("字幕巡检最多支持 100000 行或 20 MB 文本".into());
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UntranslatedEvidence {
    None,
    Weak,
    Strong,
}

fn language_base(language: &str) -> String {
    language
        .trim()
        .split(['-', '_'])
        .next()
        .unwrap_or("")
        .to_ascii_lowercase()
}

fn script(ch: char) -> u8 {
    match ch as u32 {
        0x3400..=0x9fff | 0x20000..=0x3134f => 1,
        0x3040..=0x30ff => 2,
        0xac00..=0xd7af | 0x1100..=0x11ff => 3,
        0x0400..=0x052f => 4,
        0x0600..=0x06ff | 0x0750..=0x077f => 5,
        _ if ch.is_alphabetic() => 6,
        _ => 0,
    }
}

fn target_scripts(language: &str) -> &'static [u8] {
    match language {
        "zh" | "yue" => &[1],
        "ja" => &[1, 2],
        "ko" => &[3],
        "ru" | "uk" | "bg" | "sr" => &[4],
        "ar" | "fa" | "ur" => &[5],
        "en" | "fr" | "de" | "es" | "pt" | "it" | "nl" | "sv" => &[6],
        _ => &[],
    }
}

pub fn untranslated_evidence(
    source: &str,
    target: &str,
    from: &str,
    to: &str,
) -> UntranslatedEvidence {
    let from = language_base(from);
    let to = language_base(to);
    let expected = target_scripts(&to);
    if expected.is_empty() || (!from.is_empty() && from != "auto" && from == to) {
        return UntranslatedEvidence::None;
    }
    let normalized = |text: &str| {
        text.chars()
            .filter(|ch| ch.is_alphanumeric())
            .flat_map(char::to_lowercase)
            .collect::<String>()
    };
    let source_key = normalized(source);
    if source_key.is_empty() || source_key != normalized(target) {
        return UntranslatedEvidence::None;
    }
    let words = source.split_whitespace().collect::<Vec<_>>();
    if source.chars().all(|ch| !ch.is_alphabetic())
        || words
            .iter()
            .all(|word| word.contains("://") || word.contains('@') || word.starts_with("www."))
        || source.chars().any(|ch| expected.contains(&script(ch)))
    {
        // Same-script translations, names, numbers and URLs may legitimately be unchanged.
        return UntranslatedEvidence::None;
    }
    let letters = source.chars().filter(|ch| ch.is_alphabetic()).count();
    let latin = source.chars().any(|ch| script(ch) == 6);
    let has_function_word = words.iter().any(|word| {
        matches!(
            word.trim_matches(|ch: char| !ch.is_alphabetic())
                .to_ascii_lowercase()
                .as_str(),
            "a" | "an"
                | "the"
                | "is"
                | "are"
                | "was"
                | "were"
                | "i"
                | "we"
                | "you"
                | "they"
                | "it"
                | "this"
                | "that"
                | "to"
                | "and"
                | "of"
                | "for"
                | "in"
                | "on"
                | "with"
        )
    });
    if letters >= 10 && ((!latin) || (words.len() >= 3 && has_function_word)) {
        UntranslatedEvidence::Strong
    } else {
        UntranslatedEvidence::Weak
    }
}

pub fn inspect(cues: &[QualityCue], from: &str, to: &str) -> QualityReport {
    let mut issues = Vec::new();
    let mut previous_end = 0;
    let mut previous_text = String::new();
    for (index, cue) in cues.iter().enumerate() {
        let mut add = |code: &str, value: f64| {
            issues.push(QualityIssue {
                cue_index: index,
                start_ms: cue.start_ms,
                end_ms: cue.end_ms,
                code: code.into(),
                value,
            })
        };
        let duration = cue.end_ms.saturating_sub(cue.start_ms);
        let texts = std::iter::once(cue.source.as_str()).chain(cue.target.as_deref());
        let mut max_cps: f64 = 0.0;
        let mut overlong = 0;
        for text in texts {
            for line in text.lines() {
                let visible = line.chars().filter(|ch| !ch.is_whitespace()).count();
                let cjk = line.chars().any(|ch| matches!(script(ch), 1..=3));
                let limit = if cjk { 23 } else { 42 };
                if visible > limit {
                    overlong = overlong.max(visible);
                }
                // Measure each language separately, never sum bilingual lines.
                let cps = visible as f64 / (duration as f64 / 1000.0).max(0.001);
                let threshold = if cjk { 12.0 } else { 22.0 };
                if visible > 4 && cps > threshold + 2.0 {
                    max_cps = max_cps.max(cps);
                }
            }
        }
        if duration == 0 {
            add("invalid-time", 0.0);
        } else if duration < 700 {
            add("short-duration", duration as f64);
        }
        if cue.start_ms < previous_end {
            add("overlap", (previous_end - cue.start_ms) as f64);
        }
        if max_cps > 0.0 {
            add("fast-reading", (max_cps * 10.0).round() / 10.0);
        }
        if overlong > 0 {
            add("long-line", overlong as f64);
        }
        if cue.source.trim().is_empty() {
            add("empty", 0.0);
        }
        if cue
            .source
            .chars()
            .chain(cue.target.as_deref().unwrap_or("").chars())
            .any(|ch| ch == '\u{fffd}' || (ch.is_control() && !ch.is_whitespace()))
        {
            add("invalid-text", 0.0);
        }
        if let Some(target) = cue.target.as_deref() {
            if target.trim().is_empty()
                || target.contains("[翻译失败：")
                || untranslated_evidence(&cue.source, target, from, to)
                    == UntranslatedEvidence::Strong
            {
                add("untranslated", 0.0);
            }
        }
        if !previous_text.is_empty()
            && previous_text == cue.source.trim()
            && cue.start_ms.saturating_sub(previous_end) < 1000
        {
            add("repeated", 0.0);
        }
        previous_end = previous_end.max(cue.end_ms);
        previous_text = cue.source.trim().into();
    }
    let affected_cues = issues
        .iter()
        .map(|issue| issue.cue_index)
        .collect::<std::collections::HashSet<_>>()
        .len();
    QualityReport {
        cue_count: cues.len(),
        affected_cues,
        issues,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn names_same_script_codes_and_unknown_targets_are_not_hard_failures() {
        for (source, from, to) in [
            ("OpenAI", "en", "zh"),
            ("San Francisco", "en", "zh"),
            ("中华人民共和国", "zh", "ja"),
            ("Bonjour", "fr", "en"),
            ("123.45%", "auto", "zh"),
            ("https://example.com", "en", "zh"),
            ("hello world", "en", "unknown"),
            ("This is unchanged", "en-US", "en-GB"),
        ] {
            assert_ne!(
                untranslated_evidence(source, source, from, to),
                UntranslatedEvidence::Strong,
                "{source}"
            );
        }
        assert_eq!(
            untranslated_evidence(
                "This is a copied sentence.",
                "This is a copied sentence.",
                "auto",
                "zh-CN"
            ),
            UntranslatedEvidence::Strong
        );
        assert_eq!(
            untranslated_evidence("This is a copied sentence.", "这是译文。", "en", "zh"),
            UntranslatedEvidence::None
        );
    }
    #[test]
    fn quality_checks_preserve_bilingual_reading_speed_and_detect_timing() {
        let good = QualityCue {
            start_ms: 1000,
            end_ms: 4000,
            source: "This is a readable sentence.".into(),
            target: Some("这是一句易读的字幕。".into()),
        };
        assert!(inspect(std::slice::from_ref(&good), "en", "zh")
            .issues
            .is_empty());
        let broken = QualityCue {
            start_ms: 3500,
            end_ms: 3900,
            source: "This is a very long subtitle line that cannot be read in time.".into(),
            target: Some(String::new()),
        };
        let report = inspect(&[good, broken], "en", "zh");
        for code in [
            "overlap",
            "short-duration",
            "long-line",
            "fast-reading",
            "untranslated",
        ] {
            assert!(report.issues.iter().any(|i| i.code == code), "{code}");
        }
        assert_eq!(report.affected_cues, 1);
    }
}
