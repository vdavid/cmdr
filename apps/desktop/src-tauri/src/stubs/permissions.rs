//! Permission stubs for Linux/non-macOS platforms.
//!
//! On Linux, there's no Full Disk Access concept like macOS.
//! These stubs return sensible defaults.

/// Checks if the app has full disk access (stub: always returns true).
///
/// On Linux, filesystem permissions are handled by standard Unix permissions,
/// not a separate "Full Disk Access" grant like macOS.
#[tauri::command]
pub fn check_full_disk_access() -> bool {
    true
}

/// Opens privacy settings (stub: no-op, returns error).
///
/// There's no equivalent to macOS System Settings > Privacy on Linux.
#[tauri::command]
pub fn open_privacy_settings() -> Result<(), String> {
    Err("Privacy settings not available on Linux".to_string())
}

/// Opens appearance settings (stub: no-op, returns error).
#[tauri::command]
pub fn open_appearance_settings() -> Result<(), String> {
    Err("Appearance settings not available on Linux".to_string())
}
