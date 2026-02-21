//! Accent color stub for Linux/non-macOS platforms.
//!
//! Returns the macOS default blue as fallback since system accent color
//! detection is not available outside macOS.

/// Returns the default accent color (macOS blue) on non-macOS platforms.
#[tauri::command]
pub fn get_accent_color() -> String {
    "#007aff".to_owned()
}
