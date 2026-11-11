//! Full Disk Access and the OS's settings panes: one command each, answered per OS by
//! `platform/`. How Full Disk Access works on macOS (detect vs register) is the macOS arm's module
//! doc, `platform/macos.rs`.

mod platform;

/// Polls FDA status without TCC-registration side effects.
///
/// Same return contract as `check_full_disk_access` (`true` = granted) and
/// honors the same `CMDR_MOCK_FDA` override, but skips the multi-trigger
/// `mmap` / `NSData` / `read_dir` storm and the per-call logging. Built for
/// the onboarding FDA step, which calls this every 500 ms while visible and
/// not-yet-granted to flip to a success state the moment the user toggles
/// Cmdr on in System Settings. Keep `check_full_disk_access` for the
/// one-shot registration moments (it's the one that gets Cmdr into the FDA
/// list); this one is purely for detection.
#[tauri::command]
#[specta::specta]
pub fn check_full_disk_access_quiet() -> bool {
    platform::check_full_disk_access_quiet()
}

/// Detects FDA by probing TCC-protected files, and on a denial fires every
/// known list-registration trigger so Cmdr shows up in System Settings. Returns
/// `true` if granted. See the module doc for the detect-vs-register mechanism
/// and the macOS-version split. For repeated, side-effect-free polling, use
/// `check_full_disk_access_quiet`.
#[tauri::command]
#[specta::specta]
pub fn check_full_disk_access() -> bool {
    platform::check_full_disk_access()
}

/// The OS's major version: on macOS, `13` for Ventura and `14` for Sonoma; `0` off macOS, where
/// nothing asks.
///
/// Used by the onboarding modal to tailor copy + the deep-link host:
/// Ventura+ has the new System Settings app with the
/// `PrivacySecurity.extension` URL host and an alphabetical FDA list; older
/// macOS uses the legacy System Preferences `preference.security` host with
/// new entries appended at the end.
#[tauri::command]
#[specta::specta]
pub fn get_os_major_version() -> u32 {
    platform::os_major_version()
}

/// Opens System Settings directly on the Full Disk Access pane.
///
/// Picks the deep-link host based on macOS version: Ventura+ uses the new
/// `PrivacySecurity.extension`, older macOS uses the legacy
/// `preference.security` host. Both anchor on `Privacy_AllFiles`.
#[tauri::command]
#[specta::specta]
pub async fn open_privacy_settings() -> Result<(), String> {
    // Off the IPC thread: waiting on `open` (and `sw_vers` for the host) takes a few hundred
    // milliseconds, which a sync command would spend blocking the app.
    tauri::async_runtime::spawn_blocking(platform::open_privacy_settings)
        .await
        .map_err(|e| format!("the task opening System Settings stopped: {e}"))?
}

/// Opens System Settings > Appearance.
#[tauri::command]
#[specta::specta]
pub fn open_appearance_settings() -> Result<(), String> {
    platform::open_appearance_settings()
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
    platform::open_system_settings_url(url)
}
