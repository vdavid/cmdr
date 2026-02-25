//! Accent color stub for Linux/non-macOS platforms.
//!
//! Returns the Cmdr brand accent (mustard gold) as fallback since system
//! accent color detection is not available outside macOS.

/// Returns the brand fallback accent color on non-macOS platforms.
#[tauri::command]
pub fn get_accent_color() -> String {
    "#d4a006".to_owned()
}
