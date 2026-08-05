// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // Linux display setup (native Wayland, WebKit stall guards) lives in
    // `run()` so it happens before GTK/WebKit initialise. See lib.rs.
    tauri_app_lib::run()
}
