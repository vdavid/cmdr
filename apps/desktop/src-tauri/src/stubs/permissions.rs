//! Permission stubs for Linux/non-macOS platforms.
//!
//! On Linux, there's no Full Disk Access concept like macOS.
//! These stubs return sensible defaults.

/// Checks if the app has full disk access (stub: always returns true).
///
/// On Linux, filesystem permissions are handled by standard Unix permissions,
/// not a separate "Full Disk Access" grant like macOS.
#[tauri::command]
#[specta::specta]
pub fn check_full_disk_access() -> bool {
    true
}

/// Side-effect-free FDA poll (stub: always returns true). Mirrors the macOS
/// quiet poller so the generated bindings are identical across platforms.
#[tauri::command]
#[specta::specta]
pub fn check_full_disk_access_quiet() -> bool {
    true
}

/// Stub: macOS version is meaningless on this platform. Returns `0`.
#[tauri::command]
#[specta::specta]
pub fn get_macos_major_version() -> u32 {
    0
}

/// Opens privacy settings (stub: no-op, returns error).
///
/// There's no equivalent to macOS System Settings > Privacy on Linux.
#[tauri::command]
#[specta::specta]
pub fn open_privacy_settings() -> Result<(), String> {
    Err("Privacy settings not available on Linux".to_string())
}

/// Opens appearance settings (stub: no-op, returns error).
#[tauri::command]
#[specta::specta]
pub fn open_appearance_settings() -> Result<(), String> {
    Err("Appearance settings not available on Linux".to_string())
}

/// Opens an `x-apple.systempreferences:` URL (stub: not applicable on this platform).
#[tauri::command]
#[specta::specta]
pub fn open_system_settings_url(_url: String) -> Result<(), String> {
    Err("System Settings deep links only exist on macOS.".to_string())
}
