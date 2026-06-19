//! "Reduce transparency" stub for non-macOS platforms.
//!
//! There's no cross-platform "reduce transparency" accessibility setting we read
//! today, so we return `false` (the safe "don't reduce" default) and the observer
//! is a no-op.

use tauri::{AppHandle, Runtime};

/// Returns `false` (don't reduce transparency) on non-macOS platforms.
#[tauri::command]
#[specta::specta]
pub fn get_should_reduce_transparency() -> bool {
    false
}

/// No-op observer on non-macOS platforms; there's nothing to watch.
pub fn observe_reduce_transparency_changes<R: Runtime>(_app_handle: AppHandle<R>) {}
