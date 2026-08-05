mod control_room;
mod runtime;

pub use crate::runtime::{browser_bridge, ids, proxy_core};

pub fn run() {
    configure_linux_display();
    control_room::run()
}

/// Make the app run natively under Wayland.
///
/// WebKitGTK's DMABUF renderer triggers `Gdk-Message: Error 71 (Protocol
/// error) dispatching to Wayland display` on a number of Wayland compositors,
/// which tears down the Wayland connection the instant the window maps — the
/// window "opens and closes" immediately and the process exits.
///
/// Disabling the DMABUF renderer keeps us on a native Wayland session (no
/// forced XWayland fallback) and avoids the crash. We only touch the
/// environment when we actually detect a Wayland session, and we never override
/// an explicit choice already made by the user/launcher — so `GDK_BACKEND=x11`
/// or `WEBKIT_DISABLE_DMABUF_RENDERER=0` still win.
///
/// Must run before GTK/WebKit initialise — i.e. before `tauri::Builder` — which
/// is why it's the first thing `run()` does.
#[cfg(target_os = "linux")]
fn configure_linux_display() {
    use std::env;

    // Backend-independent: WebKit silently stalls on Linux without this
    // (tauri#10566), on X11 and Wayland alike — so set it either way, unless
    // the user already made the call.
    if env::var_os("WEBKIT_DISABLE_COMPOSITING_MODE").is_none() {
        unsafe { env::set_var("WEBKIT_DISABLE_COMPOSITING_MODE", "1") };
    }

    // Respect an explicit backend choice: if the user already forced GTK to
    // X11, native-Wayland tuning is irrelevant and we leave the rest alone.
    if env::var_os("GDK_BACKEND")
        .map(|v| v.to_string_lossy().to_ascii_lowercase().contains("x11"))
        .unwrap_or(false)
    {
        return;
    }

    let is_wayland = env::var_os("WAYLAND_DISPLAY").is_some()
        || env::var("XDG_SESSION_TYPE")
            .map(|v| v.eq_ignore_ascii_case("wayland"))
            .unwrap_or(false);

    // Only when we're on Wayland and the user hasn't already expressed a
    // preference, so an explicit `WEBKIT_DISABLE_DMABUF_RENDERER=0` still wins.
    if is_wayland && env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
        unsafe { env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1") };
    }
}

#[cfg(not(target_os = "linux"))]
fn configure_linux_display() {}
