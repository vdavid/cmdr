//! Text-size stub for non-macOS platforms.
//!
//! Returns 1.0 (no system scaling) since the Sonoma+ Accessibility text-size
//! signal is macOS-only. The user's in-app `appearance.textSize` slider still
//! works on every platform — only the system-side compounding is skipped.

/// Returns 1.0 on non-macOS platforms.
#[tauri::command]
#[specta::specta]
pub fn get_system_text_size_multiplier() -> f32 {
    1.0
}
