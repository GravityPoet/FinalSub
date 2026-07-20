use crate::error::{FinalSubError, Result};
use serde::Serialize;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

const MAX_BATCH_INPUTS: usize = 10_000;
const MEDIA_EXTENSIONS: &[&str] = &[
    "mp4", "mkv", "mov", "avi", "webm", "m4v", "mpeg", "mpg", "ts", "m2ts", "mp3", "wav", "m4a",
    "flac", "aac", "ogg", "opus", "wma",
];
const SUBTITLE_EXTENSIONS: &[&str] = &["srt", "vtt", "ass", "lrc"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BatchInputKind {
    Media,
    Subtitle,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MixedBatchInputs {
    pub media: Vec<String>,
    pub subtitles: Vec<String>,
}

impl BatchInputKind {
    fn supports(self, path: &Path) -> bool {
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .map(str::to_ascii_lowercase);
        let Some(extension) = extension else {
            return false;
        };
        match self {
            Self::Media => MEDIA_EXTENSIONS.contains(&extension.as_str()),
            Self::Subtitle => SUBTITLE_EXTENSIONS.contains(&extension.as_str()),
        }
    }
}

pub fn discover_inputs(
    raw_paths: &[String],
    kind: BatchInputKind,
    recursive: bool,
) -> Result<Vec<String>> {
    let discovered = discover_inputs_allow_empty(raw_paths, kind, recursive)?;
    if discovered.is_empty() {
        let label = match kind {
            BatchInputKind::Media => "媒体",
            BatchInputKind::Subtitle => "字幕",
        };
        return Err(FinalSubError::Validation(format!(
            "所选位置中没有受支持的{label}文件"
        )));
    }
    Ok(discovered)
}

pub fn discover_mixed_inputs(raw_paths: &[String], recursive: bool) -> Result<MixedBatchInputs> {
    let media = discover_inputs_allow_empty(raw_paths, BatchInputKind::Media, recursive)?;
    let subtitles = discover_inputs_allow_empty(raw_paths, BatchInputKind::Subtitle, recursive)?;
    if media.is_empty() && subtitles.is_empty() {
        return Err(FinalSubError::Validation(
            "所选位置中没有受支持的媒体或字幕文件".into(),
        ));
    }
    if media.len().saturating_add(subtitles.len()) > MAX_BATCH_INPUTS {
        return Err(FinalSubError::Validation(format!(
            "单次批处理最多支持 {MAX_BATCH_INPUTS} 个文件"
        )));
    }
    Ok(MixedBatchInputs { media, subtitles })
}

fn discover_inputs_allow_empty(
    raw_paths: &[String],
    kind: BatchInputKind,
    recursive: bool,
) -> Result<Vec<String>> {
    if raw_paths.is_empty() {
        return Err(FinalSubError::Validation("请选择至少一个文件或目录".into()));
    }

    let mut discovered = Vec::new();
    let mut seen = HashSet::new();
    for raw_path in raw_paths {
        let path = PathBuf::from(raw_path.trim());
        if !path.is_absolute() {
            return Err(FinalSubError::Validation(format!(
                "批量输入必须使用绝对路径：{}",
                path.display()
            )));
        }
        let metadata = std::fs::symlink_metadata(&path).map_err(|error| {
            FinalSubError::Validation(format!("无法读取批量输入 {}：{error}", path.display()))
        })?;
        if metadata.file_type().is_symlink() {
            continue;
        }
        if metadata.is_file() {
            add_file(&path, kind, &mut discovered, &mut seen)?;
        } else if metadata.is_dir() {
            discover_directory(&path, kind, recursive, &mut discovered, &mut seen)?;
        }
    }

    discovered.sort();
    Ok(discovered)
}

fn discover_directory(
    root: &Path,
    kind: BatchInputKind,
    recursive: bool,
    discovered: &mut Vec<String>,
    seen: &mut HashSet<PathBuf>,
) -> Result<()> {
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        let mut entries = std::fs::read_dir(&directory)
            .map_err(|error| {
                FinalSubError::Validation(format!("无法扫描目录 {}：{error}", directory.display()))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        entries.sort_by_key(|entry| entry.path());

        for entry in entries {
            let path = entry.path();
            let metadata = std::fs::symlink_metadata(&path)?;
            if metadata.file_type().is_symlink() {
                continue;
            }
            if metadata.is_file() {
                add_file(&path, kind, discovered, seen)?;
            } else if recursive && metadata.is_dir() {
                pending.push(path);
            }
        }
    }
    Ok(())
}

fn add_file(
    path: &Path,
    kind: BatchInputKind,
    discovered: &mut Vec<String>,
    seen: &mut HashSet<PathBuf>,
) -> Result<()> {
    if !kind.supports(path) {
        return Ok(());
    }
    let canonical = std::fs::canonicalize(path)?;
    if seen.insert(canonical.clone()) {
        if discovered.len() >= MAX_BATCH_INPUTS {
            return Err(FinalSubError::Validation(format!(
                "单次批处理最多支持 {MAX_BATCH_INPUTS} 个文件"
            )));
        }
        discovered.push(canonical.to_string_lossy().to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recursively_discovers_supported_media_in_stable_order() {
        let temp = tempfile::tempdir().unwrap();
        let nested = temp.path().join("nested");
        std::fs::create_dir(&nested).unwrap();
        std::fs::write(temp.path().join("b.mp4"), b"x").unwrap();
        std::fs::write(nested.join("a.wav"), b"x").unwrap();
        std::fs::write(nested.join("ignore.txt"), b"x").unwrap();

        let paths = discover_inputs(
            &[temp.path().to_string_lossy().to_string()],
            BatchInputKind::Media,
            true,
        )
        .unwrap();
        assert_eq!(paths.len(), 2);
        assert!(paths[0] < paths[1]);
        assert!(paths.iter().all(|path| !path.ends_with("ignore.txt")));
    }

    #[test]
    fn non_recursive_scan_ignores_nested_files() {
        let temp = tempfile::tempdir().unwrap();
        let nested = temp.path().join("nested");
        std::fs::create_dir(&nested).unwrap();
        std::fs::write(nested.join("inside.srt"), b"x").unwrap();
        std::fs::write(temp.path().join("outside.vtt"), b"x").unwrap();

        let paths = discover_inputs(
            &[temp.path().to_string_lossy().to_string()],
            BatchInputKind::Subtitle,
            false,
        )
        .unwrap();
        assert_eq!(paths.len(), 1);
        assert!(paths[0].ends_with("outside.vtt"));
    }

    #[test]
    fn deduplicates_the_same_file() {
        let temp = tempfile::tempdir().unwrap();
        let media = temp.path().join("clip.mov");
        std::fs::write(&media, b"x").unwrap();
        let raw = media.to_string_lossy().to_string();
        let paths = discover_inputs(&[raw.clone(), raw], BatchInputKind::Media, true).unwrap();
        assert_eq!(paths.len(), 1);
    }

    #[test]
    fn mixed_discovery_returns_media_and_subtitles_together() {
        let temp = tempfile::tempdir().unwrap();
        let nested = temp.path().join("nested");
        std::fs::create_dir(&nested).unwrap();
        std::fs::write(temp.path().join("episode.mp4"), b"x").unwrap();
        std::fs::write(nested.join("episode.zh.srt"), b"x").unwrap();
        std::fs::write(nested.join("ignore.txt"), b"x").unwrap();

        let discovered =
            discover_mixed_inputs(&[temp.path().to_string_lossy().to_string()], true).unwrap();

        assert_eq!(discovered.media.len(), 1);
        assert_eq!(discovered.subtitles.len(), 1);
        assert!(discovered.media[0].ends_with("episode.mp4"));
        assert!(discovered.subtitles[0].ends_with("episode.zh.srt"));
    }

    #[test]
    fn rejects_relative_input_roots() {
        let error = discover_inputs(&["relative/path".into()], BatchInputKind::Media, true)
            .unwrap_err()
            .to_string();
        assert!(error.contains("绝对路径"));
    }
}
