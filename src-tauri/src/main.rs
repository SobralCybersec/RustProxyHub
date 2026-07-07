// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    #[cfg(target_os = "linux")]
    {
        // ponytail: X11/XWayland — native Wayland throws protocol error 71 in GTK3
        unsafe { std::env::set_var("GDK_BACKEND", "x11") };
        // ponytail: disables WebKit DMABuf renderer — GBM buffer allocation fails on many Mesa/KMS configs
        unsafe { std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1") };
        // ponytail: disables accelerated compositing — without this WebKit silently stalls on Linux (tauri#10566)
        unsafe { std::env::set_var("WEBKIT_DISABLE_COMPOSITING_MODE", "1") };
    }
    tauri_app_lib::run()
}
