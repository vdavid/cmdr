//! Liquid Glass slider stub for non-macOS platforms.
//!
//! Only macOS has the slider, so there's no value to report (the frontend then uses its
//! middle-of-the-slider default) and the observer is a no-op.

use tauri::{AppHandle, Runtime};

/// Returns `None` (no system slider) on non-macOS platforms.
#[tauri::command]
#[specta::specta]
pub async fn get_glass_tint_amount() -> Option<f32> {
    None
}

/// No-op observer on non-macOS platforms; there's nothing to watch.
pub fn observe_glass_tint_changes<R: Runtime>(_app_handle: AppHandle<R>) {}
