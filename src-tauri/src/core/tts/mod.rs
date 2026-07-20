mod download;
mod dubbing;
mod engine;
mod models;
mod providers;
mod voice_profiles;
mod volcengine;
mod volcengine_clone;

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
    delete_provider, list_cloud_voices, list_providers, save_provider, synthesize_cloud,
    CloudTtsSynthesisRequest, CloudVoiceSummary, SaveTtsProviderRequest, TtsProviderProfile,
};
pub use voice_profiles::{
    cleanup_transient_files as cleanup_voice_profile_transients, create_cloud_voice_profile,
    create_voice_profile, delete_cloud_voice_remote, discard_prepared_voice_sample,
    discard_voice_recording, export_voice_profile, import_voice_profile, inspect_voice_source,
    link_cloud_voice_profile, list_voice_subtitle_cues, load_profiles as load_voice_profiles,
    prepare_voice_sample, refresh_cloud_voice_status, remove_voice_profile, rename_voice_profile,
    retrain_cloud_voice_profile, save_voice_recording, CloudVoiceStatus,
    CreateCloudVoiceProfileRequest, CreateVoiceProfileRequest, LinkCloudVoiceProfileRequest,
    PrepareVoiceSampleRequest, PreparedVoiceSample, RetrainCloudVoiceProfileRequest,
    VoiceCloneEngine, VoiceProfile, VoiceProfileLanguage, VoiceQualityIssue, VoiceQualityIssueCode,
    VoiceQualityIssueSeverity, VoiceQualityReport, VoiceQualityVerdict, VoiceSourceInfo,
    VoiceSubtitleCue,
};

pub use dubbing::{
    accept_dubbing_overflow, complete_dubbing_cue, create_dubbing_session, export_dubbing_audio,
    export_dubbing_subtitle, fail_dubbing_cue, get_dubbing_session, prepare_dubbing_cue,
    update_dubbing_cue, write_back_dubbing_subtitle, DubbingCue, DubbingCueStatus,
    DubbingEngineSelection, DubbingRunConfig, DubbingSession, DubbingSubtitleWriteResult,
    DubbingSynthesizeCueRequest, PreparedDubbingCue, UpdateDubbingCueRequest,
};
pub(crate) use models::resolve_ready_model;
