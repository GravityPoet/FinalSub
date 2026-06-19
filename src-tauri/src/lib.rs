pub mod commands;
pub mod core;
pub mod error;
pub mod state;

use state::AppState;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            let config_dir = app
                .path()
                .app_config_dir()
                .expect("failed to resolve app config dir");
            std::fs::create_dir_all(&config_dir).ok();
            app.manage(AppState::new(config_dir));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_app_info,
            commands::list_asr_models,
            commands::get_model_status,
            commands::create_task,
            commands::create_preview_task,
            commands::list_tasks,
            commands::cancel_task,
            commands::normalize_srt,
            commands::extract_audio_plan,
            commands::get_ffmpeg_version,
            commands::get_settings,
            commands::save_settings_cmd,
            commands::reset_settings,
            commands::export_config,
            commands::import_config,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
