//! Liquid Glass slider stub for non-macOS platforms.
//!
//! Only macOS has the slider, so there's no value to report (the frontend then uses its
//! middle-of-the-slider default).

/// Returns `None` (no system slider) on non-macOS platforms.
#[tauri::command]
#[specta::specta]
pub async fn get_glass_tint_amount() -> Option<f32> {
    None
}
