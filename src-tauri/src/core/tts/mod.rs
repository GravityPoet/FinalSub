mod download;
mod dubbing;
mod engine;
mod models;
mod providers;
mod voice_profiles;
mod volcengine;

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
pub use voice_profiles::{
    cleanup_transient_files as cleanup_voice_profile_transients, create_voice_profile,
    discard_prepared_voice_sample, discard_voice_recording, export_voice_profile,
    import_voice_profile, inspect_voice_source, load_profiles as load_voice_profiles,
    prepare_voice_sample, remove_voice_profile, rename_voice_profile, save_voice_recording,
    CreateVoiceProfileRequest, PrepareVoiceSampleRequest, PreparedVoiceSample, VoiceProfile,
    VoiceProfileLanguage, VoiceQualityIssue, VoiceQualityIssueCode, VoiceQualityIssueSeverity,
    VoiceQualityReport, VoiceQualityVerdict, VoiceSourceInfo,
};

pub use dubbing::{
    accept_dubbing_overflow, complete_dubbing_cue, create_dubbing_session, export_dubbing_audio,
    export_dubbing_subtitle, fail_dubbing_cue, get_dubbing_session, prepare_dubbing_cue,
    update_dubbing_cue, write_back_dubbing_subtitle, DubbingCue, DubbingCueStatus,
    DubbingEngineSelection, DubbingRunConfig, DubbingSession, DubbingSubtitleWriteResult,
    DubbingSynthesizeCueRequest, PreparedDubbingCue, UpdateDubbingCueRequest,
};
pub(crate) use models::resolve_ready_model;
