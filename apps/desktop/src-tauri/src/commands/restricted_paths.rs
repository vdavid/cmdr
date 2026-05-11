//! Tauri command: bootstrap query for the current restricted-paths set.
//!
//! The frontend store calls this once at init to hydrate, then patches
//! the in-memory set via `restricted-paths-changed` events. See
//! `crate::restricted_paths` for the full design.

/// Returns the current restricted-paths snapshot (sorted, absolute paths).
#[tauri::command]
#[specta::specta]
pub fn get_restricted_paths() -> Vec<String> {
    crate::restricted_paths::snapshot()
}
