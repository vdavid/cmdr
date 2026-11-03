//! Handing a file to its default app (Enter, double-click, the context menu's Open,
//! MCP `open_under_cursor`), and saying so when that didn't happen.
//!
//! Two answers the launch used to swallow, both now typed:
//!
//! 1. **The path isn't one other apps can open.** A direct SMB volume browses on
//!    over its own smb2 session after its macOS mount is gone, so its rows still
//!    read `/Volumes/<share>/…` while nothing lives there
//!    (`Volume::paths_are_os_visible`). The registry knows; the frontend can't,
//!    since a stranded volume has no switcher row to read. So the caller names the
//!    volume and this asks it before anything launches.
//! 2. **`open` said no.** It exits non-zero for a missing file or a file no app
//!    takes, and the launch used to `spawn` and walk away. Now it waits for the
//!    exit status, under a deadline (`crate::subprocess`), off the IPC thread.
//!
//! ❌ Never read `open`'s stderr to tell those apart: its wording is the OS's, and
//! a missing file and an app-less one both exit 1. The exit code is the answer.
//!
//! The `playwright-e2e` build asks the same volume question, then records the path
//! in `crate::open_mock` instead of launching, so a suite run piles up no windows.

/// How long `open` gets to say whether LaunchServices took the file. A normal open
/// answers in well under a second; a wedged mount can hold LaunchServices' read of
/// the file far longer, and then no answer is coming.
#[cfg(any(target_os = "macos", target_os = "linux"))]
#[cfg(not(feature = "playwright-e2e"))]
const OPEN_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);

/// Why a file didn't go to its default app. The frontend words every variant.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum OpenPathError {
    /// The file's volume hands out paths no other app can open right now (a direct
    /// SMB share whose macOS mount is gone). Nothing was launched.
    NotOnThisMac,
    /// The opener ran and refused: the file isn't there, or no app takes it.
    /// `exit_code` is `None` when a signal ended it.
    NotOpened { exit_code: Option<i32> },
    /// The opener couldn't be started. Carries the OS errno where there is one.
    LaunchRefused { errno: Option<i32> },
    /// The opener didn't answer inside the deadline and was stopped. The app may
    /// still open the file.
    TimedOut,
    /// There's no opener on this platform.
    Unsupported,
}

impl std::fmt::Display for OpenPathError {
    /// ❗ For logs only; the frontend words the variant.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotOnThisMac => f.write_str("its volume's paths aren't visible to other apps"),
            Self::NotOpened { exit_code } => write!(f, "the opener refused (exit code {exit_code:?})"),
            Self::LaunchRefused { errno } => write!(f, "the opener couldn't start (errno {errno:?})"),
            Self::TimedOut => f.write_str("the opener didn't answer in time"),
            Self::Unsupported => f.write_str("no opener on this platform"),
        }
    }
}

impl std::error::Error for OpenPathError {}

/// Opens `path` with its default app, unless `volume_id` names a registered volume
/// whose paths other apps can't open.
///
/// `volume_id` is the volume the ROW lives on. `None`, or an id nothing is
/// registered under (the search-results snapshot, a favorite), skips the check:
/// the opener then answers for the path itself.
pub async fn open_with_default_app(path: String, volume_id: Option<String>) -> Result<(), OpenPathError> {
    if let Some(id) = volume_id.as_deref()
        && !paths_reach_other_apps(id)
    {
        log::info!(
            target: "file_actions",
            "Not opening {}: volume {id} has no macOS mount behind its paths right now",
            cmdr_fs::log_private::LogPrivate::path(&path)
        );
        return Err(OpenPathError::NotOnThisMac);
    }
    launch(path).await
}

/// Whether the volume registered under `id` hands out paths other apps can open.
/// An unregistered id answers `true`: there's no volume to ask.
fn paths_reach_other_apps(id: &str) -> bool {
    crate::file_system::volume::manager::get_volume_manager()
        .get(id)
        .is_none_or(|volume| volume.paths_are_os_visible())
}

/// [`paths_reach_other_apps`] for the volume a listing reads through, the question
/// an "Open with" click asks (`menu/open_with.rs::launch_with`): the menu knows the
/// pane's listing, not its volume. An empty id or a listing that's gone answers
/// `true`, leaving the launch to go ahead as it always did. macOS-only, like "Open with".
#[cfg(target_os = "macos")]
pub(crate) fn listing_paths_reach_other_apps(listing_id: &str) -> bool {
    if listing_id.is_empty() {
        return true;
    }
    crate::listing_host::listings()
        .location(&cmdr_listing::ListingId::new(listing_id.to_string()))
        .is_none_or(|(volume_id, _)| paths_reach_other_apps(&volume_id))
}

/// `open-with-not-on-this-mac`: an "Open with" click on a file whose volume hands
/// out paths no other app can open right now, so no app was launched. The main
/// window says so in a toast; without it, the app would open on a file that isn't
/// there, or the click would do nothing.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct OpenWithNotOnThisMac {
    /// The chosen app's display name (its bundle name without `.app`).
    pub app_name: String,
}

#[cfg(all(target_os = "macos", not(feature = "playwright-e2e")))]
async fn launch(path: String) -> Result<(), OpenPathError> {
    run_opener("open", path).await
}

#[cfg(all(target_os = "linux", not(feature = "playwright-e2e")))]
async fn launch(path: String) -> Result<(), OpenPathError> {
    run_opener("xdg-open", path).await
}

#[cfg(all(not(any(target_os = "macos", target_os = "linux")), not(feature = "playwright-e2e")))]
async fn launch(_path: String) -> Result<(), OpenPathError> {
    Err(OpenPathError::Unsupported)
}

/// E2E: record the open request instead of launching an external app, so the suite
/// never floods the desktop with orphan TextEdit/Preview windows.
#[cfg(feature = "playwright-e2e")]
async fn launch(path: String) -> Result<(), OpenPathError> {
    crate::open_mock::record(path);
    Ok(())
}

/// Runs `program path` to its exit, under [`OPEN_TIMEOUT`].
#[cfg(any(target_os = "macos", target_os = "linux"))]
#[cfg(not(feature = "playwright-e2e"))]
async fn run_opener(program: &str, path: String) -> Result<(), OpenPathError> {
    use crate::subprocess::{SubprocessError, output_within};

    let mut command = tokio::process::Command::new(program);
    command.arg(&path);
    match output_within(program, OPEN_TIMEOUT, &mut command).await {
        Ok(output) if output.status.success() => Ok(()),
        Ok(output) => {
            log::info!(
                target: "file_actions",
                "{program} didn't open {} (exit code {:?})",
                cmdr_fs::log_private::LogPrivate::path(&path),
                output.status.code()
            );
            Err(OpenPathError::NotOpened {
                exit_code: output.status.code(),
            })
        }
        Err(SubprocessError::Spawn(e)) => Err(OpenPathError::LaunchRefused {
            errno: e.raw_os_error(),
        }),
        Err(SubprocessError::TimedOut { .. }) => Err(OpenPathError::TimedOut),
    }
}

#[cfg(test)]
#[path = "default_app_test.rs"]
mod tests;
