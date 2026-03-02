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
    Err("Privacy settings aren't needed on Linux — file access is governed by standard Unix permissions, not app sandboxing.".to_string())
}

/// Opens the system appearance settings via the desktop environment.
/// Detects the DE from `$XDG_CURRENT_DESKTOP` and launches the appropriate settings app.
#[tauri::command]
pub fn open_appearance_settings() -> Result<(), String> {
    let desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default().to_uppercase();

    let (cmd, args): (&str, &[&str]) = if desktop.contains("GNOME") {
        ("gnome-control-center", &["appearance"])
    } else if desktop.contains("KDE") {
        ("systemsettings", &["kcm_lookandfeel"])
    } else if desktop.contains("XFCE") {
        ("xfce4-appearance-settings", &[])
    } else {
        return Err("Appearance settings are not available for your desktop environment.".to_string());
    };

    std::process::Command::new(cmd)
        .args(args)
        .spawn()
        .map_err(|e| format!("Failed to open appearance settings: {e}"))?;
    Ok(())
}
