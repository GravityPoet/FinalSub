use super::{
    quality::{self, QualityCue},
    Cue, SubtitleTrack, MAX_SUBTITLE_FILE_BYTES,
};
use crate::core::task_queue::{
    PipelineStage, PipelineStageKind, PipelineStageStatus, Task, TaskStatus, TaskType,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

#[derive(Serialize)]
pub struct ReviewSource {
    pub source_path: String,
    pub target_path: Option<String>,
    pub source_content: String,
    pub target_content: Option<String>,
    pub version: String,
    pub media_path: Option<String>,
    pub source_language: String,
    pub target_language: String,
}

fn read(path: &Path) -> Result<String, String> {
    let file = fs::File::open(path).map_err(|error| error.to_string())?;
    let mut bytes = Vec::new();
    file.take(MAX_SUBTITLE_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() as u64 > MAX_SUBTITLE_FILE_BYTES {
        return Err("字幕文件超过 20 MB".into());
    }
    String::from_utf8(bytes).map_err(|error| error.to_string())
}

#[derive(serde::Deserialize)]
pub struct OriginalSubtitleEdit {
    pub path: String,
    pub expected_content: String,
    pub content: String,
}

/// Independent proofreading uses the same backup/atomic-replacement discipline
/// as task publication, with a content guard against edits made in another app.
pub fn write_originals(root: &Path, edits: &[OriginalSubtitleEdit]) -> Result<Vec<String>, String> {
    if edits.is_empty() || edits.len() > 2 {
        return Err("Invalid subtitle writeback count".into());
    }
    let mut seen = std::collections::HashSet::new();
    for edit in edits {
        let path = Path::new(&edit.path);
        if !path.is_absolute()
            || !seen.insert(path)
            || edit.content.len() as u64 > MAX_SUBTITLE_FILE_BYTES
        {
            return Err("Invalid subtitle writeback".into());
        }
        if read(path)? != edit.expected_content {
            return Err("finalsub:subtitle-save-conflict".into());
        }
        if fs::metadata(path)
            .map_err(|error| error.to_string())?
            .permissions()
            .readonly()
        {
            return Err("字幕文件只读，请另存为新文件".into());
        }
    }
    let changed: Vec<_> = edits
        .iter()
        .filter(|edit| edit.content != edit.expected_content)
        .collect();
    if changed.is_empty() {
        return Ok(Vec::new());
    }
    let backup_dir = root
        .join("proofread-backups")
        .join(uuid::Uuid::new_v4().to_string());
    fs::create_dir_all(&backup_dir).map_err(|error| error.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&backup_dir, fs::Permissions::from_mode(0o700))
            .map_err(|error| error.to_string())?;
    }
    let mut originals = Vec::new();
    for (index, edit) in changed.iter().enumerate() {
        let backup = backup_dir.join(format!(
            "{index}-{}",
            Path::new(&edit.path).file_name().unwrap().to_string_lossy()
        ));
        fs::copy(&edit.path, &backup).map_err(|error| error.to_string())?;
        if read(&backup)? != edit.expected_content {
            return Err("finalsub:subtitle-save-conflict".into());
        }
        originals.push(backup);
    }
    for (replaced, edit) in changed.iter().enumerate() {
        let path = Path::new(&edit.path);
        let temporary = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
        let result = (|| -> Result<(), String> {
            if read(path)? != edit.expected_content {
                return Err("finalsub:subtitle-save-conflict".into());
            }
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)
                .map_err(|error| error.to_string())?;
            file.write_all(edit.content.as_bytes())
                .map_err(|error| error.to_string())?;
            file.set_permissions(
                fs::metadata(path)
                    .map_err(|error| error.to_string())?
                    .permissions(),
            )
            .map_err(|error| error.to_string())?;
            file.sync_all().map_err(|error| error.to_string())?;
            fs::rename(&temporary, path).map_err(|error| error.to_string())
        })();
        if let Err(error) = result {
            let _ = fs::remove_file(&temporary);
            for (original, backup) in changed.iter().zip(&originals).take(replaced) {
                let restore = Path::new(&original.path)
                    .with_extension(format!("{}.restore", uuid::Uuid::new_v4()));
                if fs::copy(backup, &restore)
                    .and_then(|_| fs::rename(&restore, &original.path))
                    .is_err()
                {
                    return Err(format!(
                        "{error}; restore original files from {}",
                        backup_dir.display()
                    ));
                }
            }
            return Err(error);
        }
    }
    Ok(originals
        .iter()
        .map(|path| path.to_string_lossy().into_owned())
        .collect())
}

fn paths(root: &Path, task: &Task) -> Vec<PathBuf> {
    let mut paths = vec![
        root.join("review-source.srt"),
        root.join("review-target.srt"),
    ];
    for path in task.output_paths.iter().chain(task.output_path.iter()) {
        let path = PathBuf::from(path);
        if !paths.contains(&path) {
            paths.push(path);
        }
    }
    paths
}

pub fn version(root: &Path, task: &Task) -> Result<String, String> {
    let mut hash = Sha256::new();
    for path in paths(root, task) {
        hash.update(path.to_string_lossy().as_bytes());
        match read(&path) {
            Ok(content) => {
                hash.update([1]);
                hash.update(content.as_bytes());
            }
            Err(_) if !path.exists() => hash.update([0]),
            Err(error) => return Err(error),
        }
    }
    Ok(hex::encode(hash.finalize()))
}

pub fn load(root: &Path, task: &Task) -> Result<ReviewSource, String> {
    if matches!(task.status, TaskStatus::Pending | TaskStatus::Running) {
        return Err("请等待任务完成或暂停后再校对".into());
    }
    let source = root.join("review-source.srt");
    let target = root.join("review-target.srt");
    let source = if source.is_file() {
        source
    } else {
        PathBuf::from(task.output_path.as_deref().ok_or("还没有可校对的字幕")?)
    };
    Ok(ReviewSource {
        source_content: read(&source)?,
        target_content: if target.is_file() {
            Some(read(&target)?)
        } else {
            None
        },
        source_path: source.to_string_lossy().into_owned(),
        target_path: target
            .is_file()
            .then(|| target.to_string_lossy().into_owned()),
        version: version(root, task)?,
        media_path: (task.task_type != TaskType::TranslateOnly).then(|| task.media_path.clone()),
        source_language: task
            .source_language
            .clone()
            .unwrap_or_else(|| "auto".into()),
        target_language: task.target_language.clone().unwrap_or_else(|| "zh".into()),
    })
}

struct Replacement {
    path: PathBuf,
    backup: Option<PathBuf>,
    written: bool,
}
pub struct Publication {
    pub task: Task,
    changes: Vec<Replacement>,
    committed: bool,
}
impl Publication {
    pub fn commit(mut self) {
        self.committed = true;
    }
}
impl Drop for Publication {
    fn drop(&mut self) {
        if self.committed {
            return;
        }
        for change in self.changes.iter().rev().filter(|change| change.written) {
            if let Some(backup) = &change.backup {
                let recovery = change
                    .path
                    .with_extension(format!("{}.restore", uuid::Uuid::new_v4()));
                if fs::copy(backup, &recovery).is_ok() {
                    let _ = fs::rename(&recovery, &change.path);
                }
            } else {
                let _ = fs::remove_file(&change.path);
            }
        }
    }
}

pub fn publish(
    root: &Path,
    task: &Task,
    cues: &[QualityCue],
    expected: &str,
) -> Result<Publication, String> {
    if matches!(task.status, TaskStatus::Pending | TaskStatus::Running) {
        return Err("任务仍在运行，请暂停后保存校对".into());
    }
    quality::validate_cues(cues)?;
    if cues.is_empty()
        || cues.iter().any(|cue| {
            cue.start_ms >= cue.end_ms
                || cue.end_ms > 365 * 24 * 60 * 60 * 1000
                || cue.source.trim().is_empty()
        })
    {
        return Err("字幕时间范围或正文无效".into());
    }
    if version(root, task)? != expected {
        return Err("字幕被其他窗口或应用修改，请重新打开后合并更改".into());
    }
    let translated =
        task.task_type != TaskType::GenerateOnly && root.join("review-target.srt").is_file();
    let make_track = |target: bool| SubtitleTrack {
        cues: cues
            .iter()
            .enumerate()
            .map(|(index, cue)| Cue {
                index: (index + 1) as u32,
                start_ms: cue.start_ms,
                end_ms: cue.end_ms,
                text: if target {
                    cue.target.clone().unwrap_or_default()
                } else {
                    cue.source.clone()
                },
            })
            .collect(),
    };
    let source = make_track(false);
    let target = make_track(true);
    let report = quality::inspect(
        cues,
        task.source_language.as_deref().unwrap_or("auto"),
        task.target_language.as_deref().unwrap_or(""),
    );
    if translated
        && cues.iter().any(|cue| {
            cue.target
                .as_deref()
                .is_none_or(|text| text.trim().is_empty() || text.contains("[翻译失败："))
        })
    {
        return Err("仍有未翻译的字幕，请重译或填写后再保存".into());
    }
    let mut output = if translated {
        crate::core::task_runner::build_translation_output_track(
            &source,
            &target,
            task.translation_content_mode,
        )
    } else {
        source.clone()
    };
    if task.strip_chinese_punctuation {
        for cue in &mut output.cues {
            cue.text = crate::core::task_runner::strip_chinese_punctuation(&cue.text);
        }
    }
    let mut task = task.clone();
    let mut publication = Publication {
        task: task.clone(),
        changes: Vec::new(),
        committed: false,
    };
    let mut files = vec![(root.join("review-source.srt"), source.to_srt())];
    if translated {
        files.push((root.join("review-target.srt"), target.to_srt()));
        files.push((root.join("dubbing-source.srt"), target.to_srt()));
    }
    let mut outputs = task.output_paths.clone();
    if outputs.is_empty() {
        outputs.extend(task.output_path.clone());
    }
    if outputs.is_empty() {
        for format in crate::core::task_runner::task_output_formats(&task) {
            let path = crate::core::task_runner::reserve_unique_output_path(
                Path::new(&task.media_path),
                None,
                ".finalsub.reviewed",
                &format,
            )?;
            publication.changes.push(Replacement {
                path: path.clone(),
                backup: None,
                written: true,
            });
            outputs.push(path.to_string_lossy().into_owned());
        }
    }
    for output_path in &outputs {
        let path = PathBuf::from(output_path);
        let format = path
            .extension()
            .and_then(|ext| ext.to_str())
            .ok_or("输出格式无效")?;
        files.push((
            path.clone(),
            output
                .to_format(format)
                .map_err(|error| error.to_string())?,
        ));
    }
    let backup_dir = root
        .join("review-backups")
        .join(uuid::Uuid::new_v4().to_string());
    fs::create_dir_all(&backup_dir).map_err(|error| error.to_string())?;
    for (index, (path, content)) in files.iter().enumerate() {
        if !path.is_absolute() {
            return Err("字幕输出必须是绝对路径".into());
        }
        let backup = if path.exists() {
            let backup = backup_dir.join(format!("{index}.bak"));
            fs::copy(path, &backup).map_err(|error| error.to_string())?;
            Some(backup)
        } else {
            None
        };
        let temporary = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
        let result = (|| -> std::io::Result<()> {
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)?;
            file.write_all(content.as_bytes())?;
            file.sync_all()?;
            fs::rename(&temporary, path)
        })();
        if let Err(error) = result {
            let _ = fs::remove_file(&temporary);
            return Err(error.to_string());
        }
        publication.changes.push(Replacement {
            path: path.clone(),
            backup,
            written: true,
        });
    }
    task.output_path = outputs.first().cloned();
    task.output_paths = outputs;
    task.quality_report = Some(report);
    task.error = None;
    if let Some(pipeline) = task.pipeline.as_mut() {
        pipeline.subtitle_output_path = task.output_path.clone();
        if pipeline.has_downstream() {
            for stage in &mut pipeline.stages {
                if matches!(
                    stage.kind,
                    PipelineStageKind::Transcribe | PipelineStageKind::Translate
                ) {
                    stage.status = PipelineStageStatus::Done;
                } else {
                    stage.status = PipelineStageStatus::Pending;
                    stage.progress = 0.0;
                }
            }
            if !pipeline
                .stages
                .iter()
                .any(|stage| stage.kind == PipelineStageKind::SubtitleReview)
            {
                let index = pipeline
                    .stages
                    .iter()
                    .position(|stage| {
                        !matches!(
                            stage.kind,
                            PipelineStageKind::Transcribe | PipelineStageKind::Translate
                        )
                    })
                    .unwrap_or(pipeline.stages.len());
                pipeline.stages.insert(
                    index,
                    PipelineStage::pending(PipelineStageKind::SubtitleReview),
                );
            }
            pipeline
                .stage_mut(PipelineStageKind::SubtitleReview)
                .unwrap()
                .status = PipelineStageStatus::Review;
            pipeline.current_stage = Some(PipelineStageKind::SubtitleReview);
            pipeline.dubbing_session_id = None;
            pipeline.dubbed_audio_path = None;
            pipeline.final_video_path = None;
            task.status = TaskStatus::Review;
        }
    }
    if task.status == TaskStatus::Error
        && task.pipeline.as_ref().is_none_or(|p| !p.has_downstream())
    {
        task.status = TaskStatus::Done;
        task.progress = 1.0;
    }
    task.updated_at = chrono::Utc::now().to_rfc3339();
    task.status_message = "校对已保存，全部字幕格式已更新".into();
    publication.task = task;
    Ok(publication)
}

#[cfg(test)]
mod original_tests {
    use super::*;
    #[cfg(unix)]
    #[test]
    fn a_second_file_io_failure_rolls_back_the_first_original() {
        use std::os::unix::fs::PermissionsExt;
        let root = tempfile::tempdir().unwrap();
        let locked = root.path().join("locked");
        fs::create_dir(&locked).unwrap();
        let first = root.path().join("source.srt");
        let second = locked.join("target.srt");
        fs::write(&first, "source before").unwrap();
        fs::write(&second, "target before").unwrap();
        let edits = [
            OriginalSubtitleEdit {
                path: first.to_string_lossy().into_owned(),
                expected_content: "source before".into(),
                content: "source after".into(),
            },
            OriginalSubtitleEdit {
                path: second.to_string_lossy().into_owned(),
                expected_content: "target before".into(),
                content: "target after".into(),
            },
        ];
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o500)).unwrap();
        let result = write_originals(root.path(), &edits);
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o700)).unwrap();
        assert!(result.is_err());
        assert_eq!(fs::read_to_string(first).unwrap(), "source before");
        assert_eq!(fs::read_to_string(second).unwrap(), "target before");
    }
    #[test]
    fn writeback_keeps_backup_and_rejects_external_change_and_duplicate_path() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("original.ass");
        fs::write(&source, "original style").unwrap();
        let edit = OriginalSubtitleEdit {
            path: source.to_string_lossy().into_owned(),
            expected_content: "original style".into(),
            content: "edited style".into(),
        };
        let backups = write_originals(root.path(), &[edit]).unwrap();
        assert_eq!(fs::read_to_string(&backups[0]).unwrap(), "original style");
        assert_eq!(fs::read_to_string(&source).unwrap(), "edited style");
        let stale = || OriginalSubtitleEdit {
            path: source.to_string_lossy().into_owned(),
            expected_content: "original style".into(),
            content: "stale edit".into(),
        };
        assert!(write_originals(root.path(), &[stale()])
            .unwrap_err()
            .contains("conflict"));
        assert_eq!(fs::read_to_string(&source).unwrap(), "edited style");
        let current = || OriginalSubtitleEdit {
            path: source.to_string_lossy().into_owned(),
            expected_content: "edited style".into(),
            content: "second edit".into(),
        };
        assert!(write_originals(root.path(), &[current(), current()]).is_err());
        assert_eq!(fs::read_to_string(&source).unwrap(), "edited style");
    }
}
