use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use chrono::{Local, NaiveDate};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::Mutex;

pub const LOG_EVENT: &str = "new-log";
const RETENTION_DAYS: i64 = 7;
const MAX_LOG_MESSAGE_BYTES: usize = 16 * 1024;
const MAX_QUERY_LIMIT: usize = 500;

static WRITE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn write_lock() -> &'static Mutex<()> {
    WRITE_LOCK.get_or_init(|| Mutex::new(()))
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LogEntry {
    pub timestamp: String,
    pub level: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct LogQuery {
    pub date: Option<String>,
    pub limit: Option<usize>,
    pub levels: Option<Vec<String>>,
    pub project_id: Option<String>,
    pub task_id: Option<String>,
}

fn today_key() -> String {
    Local::now().format("%Y-%m-%d").to_string()
}

fn log_path(app_config_dir: &Path, date: &str) -> PathBuf {
    app_config_dir.join("logs").join(format!("{date}.jsonl"))
}

fn normalize_level(level: &str) -> Result<String, String> {
    let normalized = level.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "info" | "warn" | "error" => Ok(normalized),
        _ => Err("日志级别只能是 info、warn 或 error".into()),
    }
}

fn infer_level(message: &str) -> &'static str {
    let lower = message.to_ascii_lowercase();
    if lower.contains("error")
        || lower.contains("failed")
        || lower.contains("failure")
        || message.contains("失败")
        || message.contains("错误")
        || message.contains("异常")
    {
        "error"
    } else if lower.contains("warn")
        || lower.contains("warning")
        || message.contains("警告")
        || message.contains("注意")
    {
        "warn"
    } else {
        "info"
    }
}

fn bounded_message(message: &str) -> String {
    if message.len() <= MAX_LOG_MESSAGE_BYTES {
        return message.to_string();
    }
    let mut end = MAX_LOG_MESSAGE_BYTES;
    while !message.is_char_boundary(end) {
        end -= 1;
    }
    let mut bounded = message[..end].to_string();
    bounded.push('…');
    bounded
}

fn validate_date(date: Option<&str>) -> Result<String, String> {
    let value = date.unwrap_or("");
    if value.is_empty() {
        return Ok(today_key());
    }
    NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map(|_| value.to_string())
        .map_err(|_| "日期格式必须是 YYYY-MM-DD".into())
}

fn validate_project_id(project_id: &str) -> Result<(), String> {
    if project_id.is_empty() || project_id.len() > 128 || project_id.chars().any(char::is_control) {
        return Err("工程标识无效".into());
    }
    Ok(())
}

pub fn task_entry(task_id: &str, message: &str) -> LogEntry {
    LogEntry {
        timestamp: Local::now().to_rfc3339(),
        level: infer_level(message).into(),
        message: bounded_message(message),
        task_id: Some(task_id.to_string()),
        project_id: Some(task_id.to_string()),
    }
}

pub fn manual_entry(
    level: &str,
    message: &str,
    task_id: Option<String>,
    project_id: Option<String>,
) -> Result<LogEntry, String> {
    let level = normalize_level(level)?;
    if let Some(id) = task_id.as_deref() {
        validate_project_id(id)?;
    }
    // 前端只给 taskId 时仍保持与任务运行日志一致，确保“清除此任务”
    // 能覆盖同一任务的手动日志。
    let project_id = project_id.or_else(|| task_id.clone());
    if let Some(id) = project_id.as_deref() {
        validate_project_id(id)?;
    }
    Ok(LogEntry {
        timestamp: Local::now().to_rfc3339(),
        level,
        message: bounded_message(message),
        task_id,
        project_id,
    })
}

pub async fn append_entry(app_config_dir: &Path, entry: LogEntry) -> Result<LogEntry, String> {
    let _guard = write_lock().lock().await;
    let directory = app_config_dir.join("logs");
    tokio::fs::create_dir_all(&directory)
        .await
        .map_err(|error| format!("创建日志目录失败：{error}"))?;
    let path = log_path(app_config_dir, &today_key());
    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .await
        .map_err(|error| format!("打开日志文件失败：{error}"))?;
    let line = serde_json::to_vec(&entry).map_err(|error| format!("序列化日志失败：{error}"))?;
    file.write_all(&line)
        .await
        .map_err(|error| format!("写入日志失败：{error}"))?;
    file.write_all(b"\n")
        .await
        .map_err(|error| format!("写入日志换行失败：{error}"))?;
    file.flush()
        .await
        .map_err(|error| format!("刷新日志失败：{error}"))?;
    Ok(entry)
}

pub async fn append_task_log(
    app_config_dir: &Path,
    task_id: &str,
    message: &str,
) -> Result<LogEntry, String> {
    append_entry(app_config_dir, task_entry(task_id, message)).await
}

fn entry_matches(entry: &LogEntry, query: &LogQuery, levels: &[String]) -> bool {
    if !levels.is_empty() && !levels.iter().any(|level| level == &entry.level) {
        return false;
    }
    if let Some(project_id) = query.project_id.as_deref() {
        if entry.project_id.as_deref() != Some(project_id) {
            return false;
        }
    }
    if let Some(task_id) = query.task_id.as_deref() {
        if entry.task_id.as_deref() != Some(task_id) {
            return false;
        }
    }
    true
}

pub async fn query_logs(app_config_dir: &Path, query: LogQuery) -> Result<Vec<LogEntry>, String> {
    if let Some(project_id) = query.project_id.as_deref() {
        validate_project_id(project_id)?;
    }
    if let Some(task_id) = query.task_id.as_deref() {
        validate_project_id(task_id)?;
    }
    let date = validate_date(query.date.as_deref())?;
    let limit = query.limit.unwrap_or(100).clamp(1, MAX_QUERY_LIMIT);
    let levels = query
        .levels
        .as_deref()
        .unwrap_or_default()
        .iter()
        .map(|level| normalize_level(level))
        .collect::<Result<Vec<_>, _>>()?;
    let path = log_path(app_config_dir, &date);
    if !path.is_file() {
        return Ok(Vec::new());
    }

    let file = tokio::fs::File::open(path)
        .await
        .map_err(|error| format!("读取日志失败：{error}"))?;
    let mut lines = BufReader::new(file).lines();
    let mut tail = VecDeque::with_capacity(limit);
    while let Some(line) = lines
        .next_line()
        .await
        .map_err(|error| format!("读取日志行失败：{error}"))?
    {
        let Ok(entry) = serde_json::from_str::<LogEntry>(&line) else {
            continue;
        };
        if !entry_matches(&entry, &query, &levels) {
            continue;
        }
        if tail.len() == limit {
            tail.pop_front();
        }
        tail.push_back(entry);
    }
    Ok(tail.into_iter().collect())
}

pub fn available_dates(app_config_dir: &Path) -> Result<Vec<String>, String> {
    let directory = app_config_dir.join("logs");
    if !directory.is_dir() {
        return Ok(Vec::new());
    }
    let mut dates = std::fs::read_dir(directory)
        .map_err(|error| format!("读取日志日期失败：{error}"))?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let path = entry.path();
            if path.extension().and_then(|value| value.to_str()) != Some("jsonl") {
                return None;
            }
            let date = path.file_stem()?.to_str()?.to_string();
            NaiveDate::parse_from_str(&date, "%Y-%m-%d")
                .ok()
                .map(|_| date)
        })
        .collect::<Vec<_>>();
    dates.sort_unstable_by(|left, right| right.cmp(left));
    dates.dedup();
    Ok(dates)
}

pub async fn clear_logs(app_config_dir: &Path, project_id: Option<&str>) -> Result<(), String> {
    if let Some(project_id) = project_id {
        validate_project_id(project_id)?;
    }
    let _guard = write_lock().lock().await;
    let directory = app_config_dir.join("logs");
    if !directory.is_dir() {
        return Ok(());
    }
    let paths = std::fs::read_dir(&directory)
        .map_err(|error| format!("读取日志目录失败：{error}"))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("jsonl"))
        .collect::<Vec<_>>();

    if project_id.is_none() {
        for path in paths {
            let _ = tokio::fs::remove_file(path).await;
        }
        return Ok(());
    }
    let project_id = project_id.expect("checked above");
    for path in paths {
        let contents = tokio::fs::read_to_string(&path)
            .await
            .map_err(|error| format!("读取日志失败：{error}"))?;
        let mut kept = String::new();
        for line in contents.lines() {
            let remove = serde_json::from_str::<LogEntry>(line)
                .ok()
                .and_then(|entry| entry.project_id)
                .as_deref()
                == Some(project_id);
            if !remove {
                kept.push_str(line);
                kept.push('\n');
            }
        }
        if kept.is_empty() {
            let _ = tokio::fs::remove_file(&path).await;
        } else {
            let temp = path.with_extension("jsonl.tmp");
            tokio::fs::write(&temp, kept)
                .await
                .map_err(|error| format!("写入日志临时文件失败：{error}"))?;
            tokio::fs::rename(&temp, &path)
                .await
                .map_err(|error| format!("替换日志文件失败：{error}"))?;
        }
    }
    Ok(())
}

pub fn cleanup_old_logs(app_config_dir: &Path) -> Result<usize, String> {
    let directory = app_config_dir.join("logs");
    if !directory.is_dir() {
        return Ok(0);
    }
    // 保留今天及之前 RETENTION_DAYS - 1 个自然日，共 RETENTION_DAYS 个文件。
    let cutoff =
        Local::now().date_naive() - chrono::Duration::days(RETENTION_DAYS.saturating_sub(1));
    let mut removed = 0;
    for entry in
        std::fs::read_dir(directory).map_err(|error| format!("读取日志目录失败：{error}"))?
    {
        let entry = entry.map_err(|error| format!("读取日志目录项失败：{error}"))?;
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("jsonl") {
            continue;
        }
        let Some(date) = path
            .file_stem()
            .and_then(|value| value.to_str())
            .and_then(|value| NaiveDate::parse_from_str(value, "%Y-%m-%d").ok())
        else {
            continue;
        };
        if date < cutoff && std::fs::remove_file(path).is_ok() {
            removed += 1;
        }
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn appends_jsonl_and_queries_latest_filtered_entries() {
        let temp = TempDir::new().unwrap();
        append_task_log(temp.path(), "task-a", "处理开始\n下一行")
            .await
            .unwrap();
        append_entry(
            temp.path(),
            manual_entry(
                "error",
                "something failed",
                Some("task-b".into()),
                Some("project-b".into()),
            )
            .unwrap(),
        )
        .await
        .unwrap();

        let entries = query_logs(
            temp.path(),
            LogQuery {
                limit: Some(10),
                levels: Some(vec!["error".into()]),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].task_id.as_deref(), Some("task-b"));
        assert_eq!(entries[0].level, "error");

        let file = std::fs::read_to_string(
            temp.path()
                .join("logs")
                .join(format!("{}.jsonl", today_key())),
        )
        .unwrap();
        assert_eq!(file.lines().count(), 2);
        assert!(file.contains("\\n"));
    }

    #[tokio::test]
    async fn clears_only_one_project_and_keeps_other_entries() {
        let temp = TempDir::new().unwrap();
        append_entry(
            temp.path(),
            manual_entry(
                "info",
                "keep",
                Some("task-a".into()),
                Some("project-a".into()),
            )
            .unwrap(),
        )
        .await
        .unwrap();
        append_entry(
            temp.path(),
            manual_entry(
                "warn",
                "remove",
                Some("task-b".into()),
                Some("project-b".into()),
            )
            .unwrap(),
        )
        .await
        .unwrap();
        clear_logs(temp.path(), Some("project-b")).await.unwrap();
        let entries = query_logs(temp.path(), LogQuery::default()).await.unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].project_id.as_deref(), Some("project-a"));
    }

    #[test]
    fn task_only_manual_entry_inherits_project_identifier() {
        let entry = manual_entry("info", "manual", Some("task-a".into()), None).unwrap();
        assert_eq!(entry.task_id.as_deref(), Some("task-a"));
        assert_eq!(entry.project_id.as_deref(), Some("task-a"));
    }

    #[test]
    fn cleanup_removes_only_dates_older_than_retention_window() {
        let temp = TempDir::new().unwrap();
        let logs = temp.path().join("logs");
        std::fs::create_dir_all(&logs).unwrap();
        let old = (Local::now().date_naive() - chrono::Duration::days(RETENTION_DAYS + 1))
            .format("%Y-%m-%d")
            .to_string();
        let recent = (Local::now().date_naive() - chrono::Duration::days(1))
            .format("%Y-%m-%d")
            .to_string();
        std::fs::write(logs.join(format!("{old}.jsonl")), "{}\n").unwrap();
        std::fs::write(logs.join(format!("{recent}.jsonl")), "{}\n").unwrap();
        assert_eq!(cleanup_old_logs(temp.path()).unwrap(), 1);
        assert!(!logs.join(format!("{old}.jsonl")).exists());
        assert!(logs.join(format!("{recent}.jsonl")).exists());
    }

    #[test]
    fn cleanup_keeps_exact_retention_boundary() {
        let temp = TempDir::new().unwrap();
        let logs = temp.path().join("logs");
        std::fs::create_dir_all(&logs).unwrap();
        let boundary = (Local::now().date_naive()
            - chrono::Duration::days(RETENTION_DAYS.saturating_sub(1)))
        .format("%Y-%m-%d")
        .to_string();
        std::fs::write(logs.join(format!("{boundary}.jsonl")), "{}\n").unwrap();
        assert_eq!(cleanup_old_logs(temp.path()).unwrap(), 0);
        assert!(logs.join(format!("{boundary}.jsonl")).exists());
    }
}
