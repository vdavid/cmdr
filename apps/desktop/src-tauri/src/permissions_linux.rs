//! Linux permission helpers.
//!
//! Linux doesn't have macOS-style Full Disk Access. Standard Unix
//! permissions govern file access, so `check_full_disk_access` always
//! returns `true`. System settings openers attempt `xdg-open` for the
//! appropriate settings panels.

/// Always returns `true` on Linux (no app sandboxing).
#[tauri::command]
pub fn check_full_disk_access() -> bool {
    true
}

/// Opens the system privacy/security settings if a desktop environment is available.
#[tauri::command]
pub fn open_privacy_settings() -> Result<(), String> {
    // GNOME: gnome-control-center privacy
    // KDE: systemsettings (no direct privacy section URL)
    // Fallback: xdg-open is unlikely to have a privacy URI, so return an error.
    Err("Privacy settings are not applicable on Linux".to_string())
}

/// Opens the system appearance settings via the desktop environment.
#[tauri::command]
pub fn open_appearance_settings() -> Result<(), String> {
    std::process::Command::new("xdg-open")
        .arg("gnome-control-center")
        .spawn()
        .map_err(|e| format!("Failed to open appearance settings: {e}"))?;
    Ok(())
}
