//! macOS permission checking and system settings helpers.

/// Checks if the app has full disk access by probing ~/Library/Mail.
/// This is a standard technique used by macOS apps - Mail is always protected.
#[tauri::command]
pub fn check_full_disk_access() -> bool {
    let mail_path = dirs::home_dir().map(|h| h.join("Library/Mail")).unwrap_or_default();

    // Try to read the directory - if we can, we have FDA
    std::fs::read_dir(&mail_path).is_ok()
}

/// Opens System Settings > Privacy & Security > Privacy.
#[tauri::command]
pub fn open_privacy_settings() -> Result<(), String> {
    std::process::Command::new("open")
        .arg("x-apple.systempreferences:com.apple.preference.security?Privacy")
        .spawn()
        .map_err(|e| format!("Failed to open System Settings: {}", e))?;
    Ok(())
}

/// Opens System Settings > Appearance.
#[tauri::command]
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

/// Checks if an I/O error is a permission denied error.
#[allow(dead_code, reason = "Utility for future use")]
pub fn is_permission_denied_error(error: &std::io::Error) -> bool {
    error.kind() == std::io::ErrorKind::PermissionDenied
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_has_full_disk_access_returns_bool() {
        // Just verify it doesn't panic - the return value is a bool by type system
        let _result: bool = check_full_disk_access();
    }

    #[test]
    fn test_is_permission_denied_error_detects_correctly() {
        let perm_err = std::io::Error::from_raw_os_error(13);
        assert!(is_permission_denied_error(&perm_err));

        let not_found = std::io::Error::from_raw_os_error(2);
        assert!(!is_permission_denied_error(&not_found));
    }

    #[test]
    fn test_is_permission_denied_error_with_error_kind() {
        let perm_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "test");
        assert!(is_permission_denied_error(&perm_err));

        let other_err = std::io::Error::new(std::io::ErrorKind::NotFound, "test");
        assert!(!is_permission_denied_error(&other_err));
    }
}
