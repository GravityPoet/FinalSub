#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if let Some(exit_code) = finalsubtauri_lib::core::asr::maybe_run_parakeet_worker() {
        if exit_code != 0 {
            std::process::exit(exit_code);
        }
        return;
    }
    if let Some(exit_code) = finalsubtauri_lib::core::tts::maybe_run_tts_worker() {
        if exit_code != 0 {
            std::process::exit(exit_code);
        }
        return;
    }
    finalsubtauri_lib::run()
}
