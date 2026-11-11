//! Quick Look commands (native `QLPreviewPanel` on macOS, no-op stubs elsewhere).
//!
//! Three commands rather than one because the panel is a process-wide singleton
//! owned by AppKit — we can `open` it, re-target it via `set_path`, or `close`
//! it, but we don't get to construct fresh instances. The frontend tracks
//! `isOpen` and picks the right call. See `crate::quick_look` for the full
//! design (and the why-singleton, why-main-thread, why-events arguments).
//!
//! Each OS's half is `quick_look/platform/`; on macOS all three wrap their main-thread hop in
//! `blocking_with_timeout` (2 s) so a wedged AppKit pump never freezes the IPC blocking pool.

use tauri::AppHandle;

use crate::quick_look::platform;

/// Open (or re-open) Quick Look on the given path. Answers whether the panel opened: `false`
/// for a volume whose paths macOS can't preview, so the frontend drops its optimistic
/// open state (no close event will ever follow a panel that never showed).
#[tauri::command]
#[specta::specta]
pub async fn quick_look_open(app: AppHandle, path: String, volume_id: String) -> Result<bool, String> {
    platform::open(app, path, volume_id).await
}

#[tauri::command]
#[specta::specta]
pub async fn quick_look_set_path(app: AppHandle, path: String, volume_id: String) -> Result<(), String> {
    platform::set_path(app, path, volume_id).await
}

#[tauri::command]
#[specta::specta]
pub async fn quick_look_close(app: AppHandle) -> Result<(), String> {
    platform::close(app).await
}
