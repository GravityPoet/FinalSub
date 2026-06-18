pub mod commands;
pub mod core;
pub mod error;
pub mod state;

use state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_shell::init())
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
            commands::get_app_info,
            commands::list_asr_models,
            commands::get_model_status,
            commands::create_task,
            commands::list_tasks,
            commands::cancel_task,
            commands::normalize_srt,
            commands::extract_audio_plan,
            commands::get_ffmpeg_version,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
