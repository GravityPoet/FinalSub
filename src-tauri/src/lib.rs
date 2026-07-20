pub mod commands;
pub mod core;
pub mod error;
pub mod state;

use state::AppState;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::Manager;

/// 遥测上报开关。默认 false，仅当用户在设置里 opt-in 时才真正发送事件。
/// Sentry 客户端始终初始化（保证 guard 生命周期与退出时 flush 正确），
/// 但 before_send 会在此开关为 false 时丢弃所有事件，做到「未授权零外发」。
static TELEMETRY_ENABLED: AtomicBool = AtomicBool::new(false);

/// 运行时切换遥测开关（设置保存/重置/导入时调用），无需重启即可生效。
pub fn set_telemetry_enabled(enabled: bool) {
    TELEMETRY_ENABLED.store(enabled, Ordering::Relaxed);
}

/// FinalSub 分发版 Sentry DSN（客户端 DSN，仅可写入上报，随二进制分发属正常做法）。
const SENTRY_DSN: &str = "https://62075ed1f5af0714d7070e2c1fa8ec9c@o4511452456222720.ingest.de.sentry.io/4511604576682064";

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // 进程级 guard：持有至 run() 返回（应用退出），保证退出前 flush 未发完的事件。
    let _sentry_guard = sentry::init((
        SENTRY_DSN,
        sentry::ClientOptions {
            release: sentry::release_name!(),
            // 桌面工具：不上报用户 IP / 敏感头（偏离 Sentry 默认 true，出于隐私考虑）。
            send_default_pii: false,
            before_send: Some(std::sync::Arc::new(|event| {
                if TELEMETRY_ENABLED.load(Ordering::Relaxed) {
                    Some(event)
                } else {
                    None
                }
            })),
            ..Default::default()
        },
    ));

    let mut builder = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_fs::init());
    if let Some(public_key) =
        option_env!("FINALSUB_UPDATER_PUBLIC_KEY").filter(|key| !key.trim().is_empty())
    {
        builder = builder.plugin(
            tauri_plugin_updater::Builder::new()
                .pubkey(public_key)
                .build(),
        );
    }

    builder
        .setup(|app| {
            let config_dir = app
                .path()
                .app_config_dir()
                .expect("failed to resolve app config dir");
            std::fs::create_dir_all(&config_dir).ok();
            let _ = crate::core::logs::cleanup_old_logs(&config_dir);
            // 读取 opt-in 设置决定是否真正上报遥测（默认关闭）。
            if let Ok(settings) = crate::core::settings::load_settings(&config_dir) {
                set_telemetry_enabled(settings.enable_telemetry);
            }
            app.manage(AppState::new(config_dir));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_app_info,
            commands::list_asr_models,
            commands::scan_models,
            commands::discover_batch_inputs,
            commands::delete_model,
            commands::import_local_model,
            commands::import_sensevoice_model,
            commands::get_model_status,
            commands::download_model,
            commands::cancel_model_download,
            commands::list_tts_models,
            commands::list_voice_profiles,
            commands::inspect_voice_source,
            commands::save_voice_recording,
            commands::discard_voice_recording,
            commands::prepare_voice_sample,
            commands::discard_prepared_voice_sample,
            commands::create_voice_profile,
            commands::rename_voice_profile,
            commands::remove_voice_profile,
            commands::export_voice_profile,
            commands::import_voice_profile,
            commands::download_tts_model,
            commands::cancel_tts_model_download,
            commands::delete_tts_model,
            commands::register_tts_model_path,
            commands::forget_tts_model_path,
            commands::set_tts_models_root,
            commands::synthesize_local_tts,
            commands::cancel_local_tts,
            commands::list_tts_providers,
            commands::save_tts_provider,
            commands::delete_tts_provider,
            commands::synthesize_cloud_tts,
            commands::test_tts_provider,
            commands::create_dubbing_session,
            commands::get_dubbing_session,
            commands::update_dubbing_cue,
            commands::export_dubbing_subtitle,
            commands::write_back_dubbing_subtitle,
            commands::synthesize_dubbing_cue,
            commands::accept_dubbing_overflow,
            commands::export_dubbing_audio,
            commands::create_task,
            commands::create_tasks,
            commands::create_preview_task,
            commands::list_tasks,
            commands::list_task_recipes,
            commands::save_task_recipe,
            commands::delete_task_recipe,
            commands::approve_task,
            commands::approve_tasks,
            commands::delete_task,
            commands::delete_tasks,
            commands::cancel_task,
            commands::pause_task,
            commands::resume_task,
            commands::retry_task,
            commands::get_task_logs,
            commands::get_logs,
            commands::get_log_dates,
            commands::clear_logs,
            commands::add_log,
            commands::normalize_srt,
            commands::convert_subtitle_opencc,
            commands::convert_strings_opencc,
            commands::extract_audio_plan,
            commands::extract_audio,
            commands::get_video_encoder_info,
            commands::burn_subtitle,
            commands::cancel_burn_subtitle,
            commands::get_video_metadata,
            commands::list_embedded_subtitles,
            commands::extract_embedded_subtitle,
            commands::generate_subtitle_preview,
            commands::transcribe_audio,
            commands::transcribe_parakeet,
            commands::list_translation_providers,
            commands::list_translation_models,
            commands::test_translation,
            commands::test_translation_proxy,
            commands::set_provider_secret,
            commands::has_provider_secret,
            commands::delete_provider_secret,
            commands::get_ffmpeg_version,
            commands::get_settings,
            commands::get_power_save_status,
            commands::save_settings_cmd,
            commands::reset_settings,
            commands::export_config,
            commands::import_config,
            commands::export_config_to_path,
            commands::import_config_from_path,
            commands::export_encrypted_config_to_path,
            commands::import_encrypted_config_from_path,
            commands::load_proofread_tasks,
            commands::save_proofread_tasks,
            commands::authorize_subtitle_directory,
            commands::check_for_update,
            commands::download_and_install_update,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
