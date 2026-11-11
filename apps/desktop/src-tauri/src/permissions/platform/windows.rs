//! Windows: no Full Disk Access grant and no settings deep links yet, so every answer is the
//! "nothing is withheld" default the other non-macOS platforms give.

/// `true`: Windows has no app-wide file-access grant to lack.
pub(crate) fn check_full_disk_access() -> bool {
    true
}

/// `true`, as above.
pub(crate) fn check_full_disk_access_quiet() -> bool {
    true
}

/// `0`: the number only ever meant macOS's version.
pub(crate) fn os_major_version() -> u32 {
    0
}

/// No Full Disk Access pane to open.
pub(crate) fn open_privacy_settings() -> Result<(), String> {
    Err("Privacy settings not available on Windows yet".to_string())
}

/// No appearance-settings opener yet.
pub(crate) fn open_appearance_settings() -> Result<(), String> {
    Err("Appearance settings not available on Windows yet".to_string())
}

/// `x-apple.systempreferences:` links exist only on macOS.
pub(crate) fn open_system_settings_url(_url: String) -> Result<(), String> {
    Err("System Settings deep links only exist on macOS.".to_string())
}
