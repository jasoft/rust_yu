#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if !rust_yu_tauri_lib::bootstrap::run_startup_bootstrap() {
        return;
    }
    rust_yu_tauri_lib::run()
}
