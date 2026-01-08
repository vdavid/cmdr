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

/// Checks if an I/O error is a permission denied error.
#[allow(dead_code)] // Utility for future use
pub fn is_permission_denied_error(error: &std::io::Error) -> bool {
    error.kind() == std::io::ErrorKind::PermissionDenied
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_has_full_disk_access_returns_bool() {
        // Just verify it doesn't panic and returns a bool
        let result = check_full_disk_access();
        assert!(result == true || result == false);
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
