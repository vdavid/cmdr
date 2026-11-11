//! How each OS answers for Full Disk Access and opens its settings panes. A platform seam
//! (`docs/guides/platform-seams.md`).

#[cfg_attr(target_os = "macos", path = "macos.rs")]
#[cfg_attr(target_os = "linux", path = "linux.rs")]
#[cfg_attr(target_os = "windows", path = "windows.rs")]
mod imp;

const _: fn() -> bool = check_full_disk_access_quiet;
const _: fn() -> bool = check_full_disk_access;
const _: fn() -> u32 = os_major_version;
const _: fn() -> Result<(), String> = open_privacy_settings;
const _: fn() -> Result<(), String> = open_appearance_settings;
const _: fn(String) -> Result<(), String> = open_system_settings_url;

/// Whether the app may read everything the user can, with no side effects: safe to poll.
pub(super) use imp::check_full_disk_access_quiet;

/// The same answer, and on macOS a denial also gets Cmdr listed in System Settings.
pub(super) use imp::check_full_disk_access;

/// The OS's major version, `0` where nothing asks.
pub(super) use imp::os_major_version;

/// Opens the OS's privacy settings on the Full Disk Access pane. Blocks until the opener answers.
pub(super) use imp::open_privacy_settings;

/// Opens the OS's appearance settings.
pub(super) use imp::open_appearance_settings;

/// Opens a System Settings deep link (`x-apple.systempreferences:` only).
pub(super) use imp::open_system_settings_url;
