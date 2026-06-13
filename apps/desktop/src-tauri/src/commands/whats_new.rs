//! IPC commands for the "What's new" popup.
//!
//! Thin pass-throughs over the `whats_new` module. The changelog is embedded in
//! the binary (`include_str!`), so these commands touch no filesystem at runtime
//! and need no `blocking_with_timeout`: there's nothing to hang on.

use crate::whats_new::{self, WhatsNewRelease};

/// Returns the displayable releases in `since_version < v <= current`, newest
/// first, truncated to `max`. `since_version = None` means no lower bound (the
/// latest `max`). "Current" is the running binary's version.
#[tauri::command]
#[specta::specta]
pub fn get_whats_new(since_version: Option<String>, max: u32) -> Vec<WhatsNewRelease> {
    let current = env!("CARGO_PKG_VERSION");
    whats_new::releases_between(since_version.as_deref(), current, max as usize)
}

/// Surfaces the `CMDR_SIMULATE_UPDATE_FROM` dev flag to the frontend: when set,
/// returns the version string to diff from so a dev session can force the startup
/// popup without hand-editing `settings.json`. The env var is a backend-process
/// value the Vite frontend can't read directly, so it crosses through here.
/// Mirrors the `CMDR_MOCK_LICENSE` / `CMDR_MOCK_FDA` dev-flag family.
#[tauri::command]
#[specta::specta]
pub fn whats_new_dev_override() -> Option<String> {
    std::env::var("CMDR_SIMULATE_UPDATE_FROM")
        .ok()
        .filter(|v| !v.is_empty())
}
