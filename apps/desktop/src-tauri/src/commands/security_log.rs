//! The security event log's switch and its "Show in Finder" button. Pass-throughs to
//! `crate::security_log`.

use std::time::Duration;

use tauri::AppHandle;

/// Checking whether the file exists is one local `stat`, so the read tier.
const REVEAL_DEADLINE: Duration = Duration::from_secs(2);

/// Why the log couldn't be shown.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum RevealSecurityLogError {
    /// The app data dir didn't resolve, so there's no place the log could be.
    NoDataDir,
    /// The file manager didn't open, or the check didn't finish in time.
    Unavailable,
}

/// Pushed live from the frontend whenever `securityLog.enabled` changes.
#[tauri::command]
#[specta::specta]
pub fn set_security_log_enabled(value: bool) {
    crate::security_log::set_user_enabled(value);
}

/// Shows the log file in Finder, or its folder while there's no file yet.
#[tauri::command]
#[specta::specta]
pub async fn reveal_security_log(app: AppHandle) -> Result<(), RevealSecurityLogError> {
    let data_dir = crate::config::resolved_app_data_dir(&app).map_err(|_| RevealSecurityLogError::NoDataDir)?;
    cmdr_fs::deadline::blocking_with_timeout(REVEAL_DEADLINE, Err(RevealSecurityLogError::Unavailable), move || {
        let target = crate::security_log::reveal_target(&data_dir);
        crate::commands::file_actions::reveal_in_file_manager(target.to_string_lossy().into_owned())
            .map_err(|_| RevealSecurityLogError::Unavailable)
    })
    .await
}
