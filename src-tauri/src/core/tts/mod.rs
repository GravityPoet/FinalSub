mod download;
mod dubbing;
mod engine;
mod models;
mod providers;

pub(crate) use download::download_model_impl;
pub(crate) use engine::{synthesize_local, TtsEngineCache};
pub use engine::{LocalTtsSynthesisRequest, TtsSynthesisResult};
pub(crate) use models::find_spec;
pub use models::{
    delete_managed_model, list_models, register_external_model, remove_external_registration,
    set_models_root, TtsModelFamily, TtsModelInfo, TtsModelLocation, TtsModelSpec, TtsModelStatus,
    TtsVoice,
};
pub use providers::{
    delete_provider, list_providers, save_provider, synthesize_cloud, CloudTtsSynthesisRequest,
    SaveTtsProviderRequest, TtsProviderProfile,
};

pub use dubbing::{
    accept_dubbing_overflow, complete_dubbing_cue, create_dubbing_session, export_dubbing_audio,
    fail_dubbing_cue, get_dubbing_session, prepare_dubbing_cue, DubbingCue, DubbingCueStatus,
    DubbingEngineSelection, DubbingRunConfig, DubbingSession, DubbingSynthesizeCueRequest,
    PreparedDubbingCue,
};
pub(crate) use models::resolve_ready_model;
