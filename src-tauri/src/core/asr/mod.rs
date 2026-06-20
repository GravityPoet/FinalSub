use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::core::subtitle::SubtitleTrack;

pub mod parakeet;
pub mod whisper;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AsrCapabilities {
    pub supports_streaming: bool,
    pub supported_languages: Vec<String>,
    pub requires_model_download: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AsrModelRef {
    pub engine_id: String,
    pub model_id: String,
    pub model_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscribeJob {
    pub audio_path: String,
    pub output_path: String,
    pub language: Option<String>,
    pub model: AsrModelRef,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressUpdate {
    pub progress: f32,
    pub message: String,
}

pub type ProgressSink = tokio::sync::mpsc::Sender<ProgressUpdate>;

#[async_trait]
pub trait AsrEngine: Send + Sync {
    fn id(&self) -> &'static str;
    fn capabilities(&self) -> AsrCapabilities;
    async fn prepare(&self, model: &AsrModelRef) -> crate::error::Result<()>;
    async fn transcribe(
        &self,
        job: TranscribeJob,
        progress: ProgressSink,
        cancel_rx: Option<tokio::sync::watch::Receiver<bool>>,
    ) -> crate::error::Result<SubtitleTrack>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AsrEngineStatus {
    pub id: String,
    pub ready: bool,
    pub status_label: String,
}
