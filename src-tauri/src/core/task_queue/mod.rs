use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::core::style_presets::SubtitleStyle;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TaskStatus {
    Pending,
    Running,
    Paused,
    Cancelled,
    Review,
    Done,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TaskType {
    GenerateAndTranslate,
    GenerateOnly,
    TranslateOnly,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TranslationContentMode {
    #[default]
    TargetOnly,
    SourceAndTarget,
    TargetAndSource,
}

/// 用户可见的端到端处理节点。名称保持产品语义，前端不需要暴露内部
/// worker/pipe 概念；每个节点的状态会随任务快照持久化，应用重启后可以
/// 从最后一个节点继续，而不是把已经完成的字幕重新上传或重做。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PipelineStageKind {
    Transcribe,
    Translate,
    SubtitleReview,
    Dub,
    DubbingReview,
    Compose,
    Done,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PipelineStageStatus {
    Pending,
    Running,
    Review,
    Done,
    Skipped,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineStage {
    pub kind: PipelineStageKind,
    pub status: PipelineStageStatus,
    #[serde(default)]
    pub progress: f32,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub started_at: Option<String>,
    #[serde(default)]
    pub completed_at: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
}

impl PipelineStage {
    pub fn pending(kind: PipelineStageKind) -> Self {
        Self {
            kind,
            status: PipelineStageStatus::Pending,
            progress: 0.0,
            message: String::new(),
            started_at: None,
            completed_at: None,
            error: None,
        }
    }
}

/// 配音配置刻意使用字符串 ID，以便任务快照不依赖 TTS provider 的内部
/// Rust 类型；旧任务读取时缺少这些字段会自动保持为空。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineDubbingConfig {
    /// `local` 或 `cloud`。
    pub engine: String,
    /// 本地模型 ID 或在线 TTS provider 实例 UUID。
    pub model_or_provider_id: String,
    pub voice: String,
    #[serde(default = "default_dubbing_speed")]
    pub global_speed: f32,
    #[serde(default)]
    pub reference_audio_path: Option<String>,
    #[serde(default)]
    pub reference_text: Option<String>,
    #[serde(default)]
    pub num_steps: Option<i32>,
}

fn default_dubbing_speed() -> f32 {
    1.0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineComposeConfig {
    #[serde(default)]
    pub soft_subtitle: bool,
    #[serde(default = "default_compose_audio_mode")]
    pub audio_mode: String,
    #[serde(default = "default_compose_encoder_mode")]
    pub encoder_mode: String,
    /// 完整样式快照。旧任务没有该字段时继续使用历史默认样式。
    #[serde(default)]
    pub style: Option<SubtitleStyle>,
}

fn default_compose_audio_mode() -> String {
    "keep".into()
}

fn default_compose_encoder_mode() -> String {
    "auto".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineConfig {
    #[serde(default)]
    pub enable_dubbing: bool,
    #[serde(default)]
    pub enable_compose: bool,
    #[serde(default)]
    pub subtitle_review: bool,
    #[serde(default)]
    pub dubbing_review: bool,
    #[serde(default)]
    pub dubbing: Option<PipelineDubbingConfig>,
    #[serde(default)]
    pub compose: Option<PipelineComposeConfig>,
    #[serde(default)]
    pub stages: Vec<PipelineStage>,
    #[serde(default)]
    pub current_stage: Option<PipelineStageKind>,
    #[serde(default)]
    pub subtitle_output_path: Option<String>,
    #[serde(default)]
    pub dubbing_session_id: Option<String>,
    #[serde(default)]
    pub dubbed_audio_path: Option<String>,
    #[serde(default)]
    pub final_video_path: Option<String>,
}

impl PipelineConfig {
    pub fn for_task(
        task_type: TaskType,
        enable_dubbing: bool,
        enable_compose: bool,
        subtitle_review: bool,
        dubbing_review: bool,
        dubbing: Option<PipelineDubbingConfig>,
        compose: Option<PipelineComposeConfig>,
    ) -> Self {
        let mut stages = Vec::new();
        if task_type != TaskType::TranslateOnly {
            stages.push(PipelineStage::pending(PipelineStageKind::Transcribe));
        }
        if task_type == TaskType::GenerateAndTranslate || task_type == TaskType::TranslateOnly {
            stages.push(PipelineStage::pending(PipelineStageKind::Translate));
        }
        if subtitle_review {
            stages.push(PipelineStage::pending(PipelineStageKind::SubtitleReview));
        }
        if enable_dubbing {
            stages.push(PipelineStage::pending(PipelineStageKind::Dub));
            if dubbing_review {
                stages.push(PipelineStage::pending(PipelineStageKind::DubbingReview));
            }
        }
        if enable_compose {
            stages.push(PipelineStage::pending(PipelineStageKind::Compose));
        }
        stages.push(PipelineStage::pending(PipelineStageKind::Done));
        let current_stage = stages.first().map(|stage| stage.kind);
        Self {
            enable_dubbing,
            enable_compose,
            subtitle_review,
            dubbing_review,
            dubbing,
            compose,
            stages,
            current_stage,
            subtitle_output_path: None,
            dubbing_session_id: None,
            dubbed_audio_path: None,
            final_video_path: None,
        }
    }

    pub fn has_downstream(&self) -> bool {
        self.enable_dubbing || self.enable_compose
    }

    pub fn stage_mut(&mut self, kind: PipelineStageKind) -> Option<&mut PipelineStage> {
        self.stages.iter_mut().find(|stage| stage.kind == kind)
    }

    pub fn stage(&self, kind: PipelineStageKind) -> Option<&PipelineStage> {
        self.stages.iter().find(|stage| stage.kind == kind)
    }

    pub fn prepare_current_stage_for_resume(&mut self, message: &str) {
        let Some(kind) = self.current_stage else {
            return;
        };
        let Some(stage) = self.stage_mut(kind) else {
            return;
        };
        if matches!(
            stage.status,
            PipelineStageStatus::Running | PipelineStageStatus::Error
        ) {
            stage.status = PipelineStageStatus::Pending;
            stage.error = None;
            stage.completed_at = None;
            stage.message = message.to_string();
        }
    }
}

impl TranslationContentMode {
    pub fn is_bilingual(self) -> bool {
        !matches!(self, TranslationContentMode::TargetOnly)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub task_type: TaskType,
    pub status: TaskStatus,
    pub media_path: String,
    #[serde(default)]
    pub provided_subtitle_path: Option<String>,
    pub media_name: String,
    pub engine_id: String,
    pub model_id: String,
    pub source_language: Option<String>,
    pub target_language: Option<String>,
    #[serde(default)]
    pub translation_content_mode: TranslationContentMode,
    pub output_format: String,
    #[serde(default)]
    pub output_name: Option<String>,
    #[serde(default)]
    pub strip_chinese_punctuation: bool,
    #[serde(default)]
    pub review_required: bool,
    #[serde(default)]
    pub max_subtitle_chars: i32,
    #[serde(default)]
    pub reviewed_at: Option<String>,
    #[serde(default)]
    pub pipeline: Option<PipelineConfig>,
    pub progress: f32,
    pub status_message: String,
    pub output_path: Option<String>,
    pub error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

pub type TaskMap = Arc<RwLock<HashMap<String, Task>>>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateTaskParams {
    pub task_type: TaskType,
    pub media_path: String,
    #[serde(default)]
    pub provided_subtitle_path: Option<String>,
    pub media_name: String,
    pub engine_id: String,
    pub model_id: String,
    pub source_language: Option<String>,
    pub target_language: Option<String>,
    pub translation_content_mode: TranslationContentMode,
    pub output_format: Option<String>,
    pub output_name: Option<String>,
    pub strip_chinese_punctuation: bool,
    pub review_required: bool,
    pub max_subtitle_chars: i32,
    #[serde(default)]
    pub pipeline: Option<PipelineConfig>,
}

pub fn create_task(params: CreateTaskParams) -> Task {
    let now = chrono::Utc::now().to_rfc3339();
    Task {
        id: Uuid::new_v4().to_string(),
        task_type: params.task_type,
        status: TaskStatus::Pending,
        media_path: params.media_path,
        provided_subtitle_path: params.provided_subtitle_path,
        media_name: params.media_name,
        engine_id: params.engine_id,
        model_id: params.model_id,
        source_language: params.source_language,
        target_language: params.target_language,
        translation_content_mode: params.translation_content_mode,
        output_format: params.output_format.unwrap_or_else(|| "srt".into()),
        output_name: params.output_name,
        strip_chinese_punctuation: params.strip_chinese_punctuation,
        review_required: params.review_required,
        max_subtitle_chars: params.max_subtitle_chars,
        reviewed_at: None,
        pipeline: params.pipeline,
        progress: 0.0,
        status_message: "待处理".into(),
        output_path: None,
        error: None,
        created_at: now.clone(),
        updated_at: now,
    }
}

use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

static TASK_SAVE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

pub fn tasks_path(app_config_dir: &Path) -> PathBuf {
    app_config_dir.join("tasks").join("tasks.json")
}

pub fn load_tasks(app_config_dir: &Path) -> Result<HashMap<String, Task>, String> {
    let path = tasks_path(app_config_dir);
    if !path.exists() {
        return Ok(HashMap::new());
    }
    let content = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let tasks: Vec<Task> = serde_json::from_str(&content).map_err(|e| e.to_string())?;
    let mut map = HashMap::new();
    for task in tasks {
        map.insert(task.id.clone(), task);
    }
    Ok(map)
}

pub fn save_tasks(app_config_dir: &Path, tasks: &HashMap<String, Task>) -> Result<(), String> {
    let _save_guard = TASK_SAVE_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|_| "任务快照写入锁不可用".to_string())?;
    let path = tasks_path(app_config_dir);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let tasks_vec: Vec<&Task> = tasks.values().collect();
    let content = serde_json::to_string_pretty(&tasks_vec).map_err(|e| e.to_string())?;
    let tmp_path = path.with_extension("json.tmp");
    std::fs::write(&tmp_path, content).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp_path, &path).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn insert_tasks_atomically(
    app_config_dir: &Path,
    tasks: &mut HashMap<String, Task>,
    new_tasks: &[Task],
) -> Result<(), String> {
    let mut next = tasks.clone();
    for task in new_tasks {
        next.insert(task.id.clone(), task.clone());
    }
    save_tasks(app_config_dir, &next)?;
    *tasks = next;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_task(name: &str) -> Task {
        create_task(CreateTaskParams {
            task_type: TaskType::GenerateOnly,
            media_path: format!("/tmp/{name}.wav"),
            provided_subtitle_path: None,
            media_name: format!("{name}.wav"),
            engine_id: "whisper-cpp".into(),
            model_id: "small".into(),
            source_language: Some("auto".into()),
            target_language: None,
            translation_content_mode: TranslationContentMode::TargetOnly,
            output_format: Some("srt".into()),
            output_name: None,
            strip_chinese_punctuation: false,
            review_required: false,
            max_subtitle_chars: 0,
            pipeline: None,
        })
    }

    #[test]
    fn batch_insert_persists_all_tasks_together() {
        let temp = tempfile::tempdir().unwrap();
        let first = sample_task("first");
        let second = sample_task("second");
        let mut tasks = HashMap::new();

        insert_tasks_atomically(temp.path(), &mut tasks, &[first.clone(), second.clone()]).unwrap();

        assert_eq!(tasks.len(), 2);
        let loaded = load_tasks(temp.path()).unwrap();
        assert_eq!(loaded.len(), 2);
        assert!(loaded.contains_key(&first.id));
        assert!(loaded.contains_key(&second.id));
    }

    #[test]
    fn batch_insert_keeps_memory_unchanged_when_persistence_fails() {
        let temp = tempfile::tempdir().unwrap();
        let blocked_config_dir = temp.path().join("not-a-directory");
        std::fs::write(&blocked_config_dir, b"blocked").unwrap();
        let existing = sample_task("existing");
        let incoming = sample_task("incoming");
        let mut tasks = HashMap::from([(existing.id.clone(), existing.clone())]);

        let error = insert_tasks_atomically(
            &blocked_config_dir,
            &mut tasks,
            std::slice::from_ref(&incoming),
        )
        .unwrap_err();

        assert!(!error.is_empty());
        assert_eq!(tasks.len(), 1);
        assert!(tasks.contains_key(&existing.id));
        assert!(!tasks.contains_key(&incoming.id));
    }

    #[test]
    fn legacy_task_without_review_fields_remains_compatible() {
        let task = sample_task("legacy");
        let mut value = serde_json::to_value(task).unwrap();
        let object = value.as_object_mut().unwrap();
        object.remove("review_required");
        object.remove("reviewed_at");
        object.remove("max_subtitle_chars");
        object.remove("pipeline");

        let restored: Task = serde_json::from_value(value).unwrap();

        assert!(!restored.review_required);
        assert!(restored.reviewed_at.is_none());
        assert_eq!(restored.max_subtitle_chars, 0);
        assert!(restored.pipeline.is_none());
    }

    #[test]
    fn pipeline_plan_is_target_driven_and_serializable() {
        let pipeline = PipelineConfig::for_task(
            TaskType::GenerateAndTranslate,
            true,
            true,
            true,
            true,
            Some(PipelineDubbingConfig {
                engine: "local".into(),
                model_or_provider_id: "kokoro-multi-lang-v1_1".into(),
                voice: "10".into(),
                global_speed: 1.0,
                reference_audio_path: None,
                reference_text: None,
                num_steps: None,
            }),
            Some(PipelineComposeConfig {
                soft_subtitle: false,
                audio_mode: "replace".into(),
                encoder_mode: "auto".into(),
                style: Some(SubtitleStyle::default()),
            }),
        );
        assert_eq!(
            pipeline
                .stages
                .iter()
                .map(|stage| stage.kind)
                .collect::<Vec<_>>(),
            vec![
                PipelineStageKind::Transcribe,
                PipelineStageKind::Translate,
                PipelineStageKind::SubtitleReview,
                PipelineStageKind::Dub,
                PipelineStageKind::DubbingReview,
                PipelineStageKind::Compose,
                PipelineStageKind::Done,
            ]
        );
        assert!(pipeline.has_downstream());
        let restored: PipelineConfig =
            serde_json::from_value(serde_json::to_value(&pipeline).unwrap()).unwrap();
        assert_eq!(restored.current_stage, Some(PipelineStageKind::Transcribe));
        assert_eq!(restored.stages.len(), 7);
        assert_eq!(
            restored.compose.unwrap().style,
            Some(SubtitleStyle::default())
        );
    }

    #[test]
    fn translate_only_pipeline_never_schedules_media_stages() {
        let pipeline = PipelineConfig::for_task(
            TaskType::TranslateOnly,
            false,
            false,
            true,
            false,
            None,
            None,
        );
        assert_eq!(
            pipeline
                .stages
                .iter()
                .map(|stage| stage.kind)
                .collect::<Vec<_>>(),
            vec![
                PipelineStageKind::Translate,
                PipelineStageKind::SubtitleReview,
                PipelineStageKind::Done,
            ]
        );
    }

    #[test]
    fn interrupted_pipeline_stage_is_prepared_for_resume_without_losing_progress() {
        let mut pipeline = PipelineConfig::for_task(
            TaskType::GenerateOnly,
            false,
            false,
            false,
            false,
            None,
            None,
        );
        let transcribe = pipeline.stage_mut(PipelineStageKind::Transcribe).unwrap();
        transcribe.status = PipelineStageStatus::Error;
        transcribe.progress = 0.42;
        transcribe.error = Some("interrupted".into());

        pipeline.prepare_current_stage_for_resume("准备继续");

        let transcribe = pipeline.stage(PipelineStageKind::Transcribe).unwrap();
        assert_eq!(transcribe.status, PipelineStageStatus::Pending);
        assert_eq!(transcribe.progress, 0.42);
        assert!(transcribe.error.is_none());
        assert_eq!(transcribe.message, "准备继续");
    }
}
