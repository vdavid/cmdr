//! macOS permission checking and system settings helpers.

/// Checks if the app has full disk access by probing ~/Library/Mail.
/// This is a standard technique used by macOS apps - Mail is always protected.
#[tauri::command]
#[specta::specta]
pub fn check_full_disk_access() -> bool {
    let mail_path = dirs::home_dir().map(|h| h.join("Library/Mail")).unwrap_or_default();

    // Try to read the directory - if we can, we have FDA
    std::fs::read_dir(&mail_path).is_ok()
}

/// Opens System Settings > Privacy & Security > Privacy.
#[tauri::command]
#[specta::specta]
pub fn open_privacy_settings() -> Result<(), String> {
    std::process::Command::new("open")
        .arg("x-apple.systempreferences:com.apple.preference.security?Privacy")
        .spawn()
        .map_err(|e| format!("Failed to open System Settings: {}", e))?;
    Ok(())
}

/// Opens System Settings > Appearance.
#[tauri::command]
#[specta::specta]
pub fn open_appearance_settings() -> Result<(), String> {
    std::process::Command::new("open")
        .arg("x-apple.systempreferences:com.apple.Appearance-Settings.extension")
        .spawn()
        .map_err(|e| format!("Failed to open System Settings: {}", e))?;
    Ok(())
}

/// Opens an `x-apple.systempreferences:` deep link.
///
/// The frontend uses this for friendly-error markdown links that point at specific
/// System Settings panes. We don't go through the Tauri opener plugin because its
/// default URL allowlist only covers `http`/`https`/`mailto`/`tel` and would reject
/// the `x-apple.systempreferences:` scheme silently. Restricting the input to that
/// scheme keeps the surface tight (no arbitrary URL execution from the webview).
#[tauri::command]
#[specta::specta]
pub fn open_system_settings_url(url: String) -> Result<(), String> {
    if !url.starts_with("x-apple.systempreferences:") {
        return Err(format!(
            "Refusing to open URL with scheme other than `x-apple.systempreferences:`: {url}"
        ));
    }
    std::process::Command::new("open")
        .arg(&url)
        .spawn()
        .map_err(|e| format!("Failed to open System Settings: {}", e))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_has_full_disk_access_returns_bool() {
        // Just verify it doesn't panic - the return value is a bool by type system
        let _result: bool = check_full_disk_access();
    }
}
