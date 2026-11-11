//! Direct file-action commands invoked from the command palette, context menus,
//! and menu items: reveal in the file manager, Get Info, open in the text editor the user
//! chose (plus the editor list its Settings row shows), copy text to the clipboard,
//! and the iCloud make-available-offline / remove-download pair. Thin pass-throughs
//! that shell out or delegate to `file_system`. What differs per OS is `platform/`.

mod platform;

use tauri::{AppHandle, Runtime};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tokio::time::Duration;

use crate::file_system::default_app::OpenPathError;
use crate::file_system::get_info::GetInfoError;
use crate::file_system::google_drive::DriveItemLinks;
#[cfg(target_os = "macos")]
use crate::file_system::terminal::{OpenTerminalError, OpenTerminalOutcome, TerminalAppList};
#[cfg(target_os = "macos")]
use crate::file_system::text_editor::TextEditorList;
use crate::file_system::text_editor::{EditorOpenReport, OpenInEditorError};
#[cfg(target_os = "macos")]
use cmdr_fs::deadline::{TimedOut, blocking_typed_result_with_timeout, blocking_with_timeout_flag};

/// Listing terminals is a handful of LaunchServices lookups and bundle-icon
/// reads. Generous enough for a custom pick sitting on a slow mount, short enough
/// that the settings row doesn't hang on one.
#[cfg(target_os = "macos")]
const TERMINAL_APPS_TIMEOUT: Duration = Duration::from_secs(2);

/// Launching is one `spawn` of `open`; the wait is for the installed-app lookup
/// in front of it.
#[cfg(target_os = "macos")]
const OPEN_TERMINAL_TIMEOUT: Duration = Duration::from_secs(5);

/// Listing text editors is one LaunchServices query plus a name and an icon read
/// per app. Same bound as the terminal list, for a picked app on a slow mount.
#[cfg(target_os = "macos")]
const TEXT_EDITORS_TIMEOUT: Duration = Duration::from_secs(2);

/// The default-app name is a LaunchServices lookup plus a name read, no file I/O. The
/// toast is worth a short wait at most, so it words the nameless variant past this.
#[cfg(target_os = "macos")]
const DEFAULT_APP_NAME_TIMEOUT: Duration = Duration::from_millis(500);

/// Reveals a file in the OS's file manager: Finder on macOS, the default file manager (its parent
/// folder) on Linux.
#[tauri::command]
#[specta::specta]
pub fn reveal_in_file_manager(path: String) -> Result<(), String> {
    platform::reveal_in_file_manager(path)
}

/// Opens the OS's Get Info window for a file, or says why it won't (`file_system::get_info`).
/// A no-op where the OS has no such window.
#[tauri::command]
#[specta::specta]
pub async fn get_info(path: String) -> Result<(), GetInfoError> {
    platform::get_info(path).await
}

/// Opens a file in the text editor `app_choice` names.
///
/// Backs the `file.edit` command (F4, in both the regular and the search-results pane)
/// and the "open the freshly created file" step of the new-file flow. `app_choice` is
/// the stored `behavior.textEditorApp` value (`system`, a bundle id, or an `.app`
/// path), passed in because the frontend owns the settings store.
///
/// Answers with a report rather than a bare success: a chosen app that's gone falls
/// back to the system default and says so, and `ask_about_other_editors` adds whether
/// macOS lists any other editor, which the one-time hint needs. The `playwright-e2e`
/// build records the file into `crate::open_mock` instead of launching (inside
/// `file_system::text_editor`), so a suite run piles up no editor windows.
#[tauri::command]
#[specta::specta]
pub async fn open_in_editor(
    path: String,
    app_choice: String,
    ask_about_other_editors: bool,
) -> Result<EditorOpenReport, OpenInEditorError> {
    platform::open_in_editor(path, app_choice, ask_about_other_editors).await
}

/// The text editors on this Mac, the system default, and which one `app_choice`
/// names.
///
/// The settings row calls this on mount and after every write of its setting, and a
/// "Choose an app…" pick calls it with the picked path to learn what to store
/// (`chosenId`). One LaunchServices query plus a name and an icon read per app, so
/// nothing caches it; the timeout only bounds a read on a stalled mount.
#[tauri::command]
#[specta::specta]
#[cfg(target_os = "macos")]
pub async fn list_text_editors(app_choice: String) -> TimedOut<TextEditorList> {
    blocking_with_timeout_flag(TEXT_EDITORS_TIMEOUT, TextEditorList::default(), move || {
        crate::file_system::text_editor::list_text_editors(&app_choice)
    })
    .await
}

/// Open a file (or folder) with the system's default application.
///
/// Backs the frontend "open" action (Enter / double-click / the context menu's Open /
/// MCP `open_under_cursor` on a file entry). `volume_id` is the volume the row lives
/// on, so a volume whose paths other apps can't open right now refuses before
/// anything launches. Waits for the opener's answer, so a refusal comes back typed
/// rather than vanishing (`file_system::default_app`). Kept in Rust rather than the
/// frontend opener plugin so the `playwright-e2e` build can record the open instead
/// of spawning TextEdit/Preview/etc. per open, which the E2E suite has no way to close.
#[tauri::command]
#[specta::specta]
pub async fn open_path(path: String, volume_id: Option<String>) -> Result<(), OpenPathError> {
    crate::file_system::default_app::open_with_default_app(path, volume_id).await
}

/// E2E: the paths `open_path` recorded (oldest first), so a spec can assert that
/// "Open with default app" launched the right file. Never touches the OS.
#[tauri::command]
#[specta::specta]
#[cfg(feature = "playwright-e2e")]
pub fn e2e_opened_paths() -> Vec<String> {
    crate::open_mock::snapshot()
        .into_iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect()
}

/// E2E: reset the recorded open requests between tests.
#[tauri::command]
#[specta::specta]
#[cfg(feature = "playwright-e2e")]
pub fn e2e_clear_opened_paths() {
    crate::open_mock::clear();
}

/// The terminal apps installed on this machine, plus which one `app_choice` names.
///
/// `app_choice` is the stored `behavior.openTerminalHereApp` value, passed in
/// rather than read here: the frontend owns the settings store, and Rust's own
/// loader is the startup-time read only (`settings/CLAUDE.md`).
///
/// The settings row calls this on every render. Each app costs one LaunchServices
/// lookup plus a bundle-icon read, so there's nothing to cache; the timeout only
/// bounds an icon read on a stalled mount.
#[tauri::command]
#[specta::specta]
#[cfg(target_os = "macos")]
pub async fn list_terminal_apps(app_choice: String) -> TimedOut<TerminalAppList> {
    blocking_with_timeout_flag(TERMINAL_APPS_TIMEOUT, TerminalAppList::default(), move || {
        crate::file_system::terminal::list_terminal_apps(&app_choice)
    })
    .await
}

/// Opens `path` in the terminal app `app_choice` names.
///
/// `path` is the folder the pane resolved (the cursor's folder, or the pane's own),
/// and `volume_id` is the volume it came from: the refusal for MTP, ADB, and other
/// path-less locations keys on the volume's capabilities, never on the path string.
///
/// Answers with an outcome rather than a bare success, so the frontend can word the
/// uninstalled-app fallback and the path-less refusal without parsing anything.
#[tauri::command]
#[specta::specta]
#[cfg(target_os = "macos")]
pub async fn open_terminal_here(
    path: String,
    volume_id: String,
    app_choice: String,
) -> Result<OpenTerminalOutcome, OpenTerminalError> {
    blocking_typed_result_with_timeout(
        OPEN_TERMINAL_TIMEOUT,
        || OpenTerminalError::TimedOut,
        |detail| {
            crate::log_error!(target: "file_actions", "open_terminal_here panicked: {detail}");
            OpenTerminalError::TimedOut
        },
        move || crate::file_system::terminal::open_terminal_here(&volume_id, std::path::Path::new(&path), &app_choice),
    )
    .await
}

/// What to call the app `app_choice` names, for the toast that says it's gone.
///
/// `null` when Cmdr carries no name for it, so the caller words the nameless
/// variant instead of showing a bundle id. Sync and I/O-free: it's a table lookup,
/// which is the only thing left once the app itself has been uninstalled.
#[tauri::command]
#[specta::specta]
#[cfg(target_os = "macos")]
pub fn terminal_app_display_name(app_choice: String) -> Option<String> {
    crate::file_system::terminal::choice_display_name(&app_choice)
}

/// The name of the app a file opens in by default, for the "Opening in …" toast that
/// Enter and double-click raise.
///
/// Asked by the file's extension only (`macos_icons::default_app_path_for_extension`),
/// so a file on a slow share answers as fast as a local one. `None` when the file has
/// no extension, nothing claims it, or the lookup overran: the toast then words its
/// nameless variant.
#[tauri::command]
#[specta::specta]
#[cfg(target_os = "macos")]
pub async fn default_app_name(path: String) -> Option<String> {
    blocking_with_timeout_flag(DEFAULT_APP_NAME_TIMEOUT, None, move || {
        let ext = std::path::Path::new(&path).extension()?.to_str()?.to_owned();
        let app_path = crate::macos_icons::default_app_path_for_extension(&ext)?;
        Some(crate::file_system::open_with::read_app_display_name(&app_path))
    })
    .await
    .data
}

/// Copy text to clipboard
#[tauri::command]
#[specta::specta]
pub fn copy_to_clipboard<R: Runtime>(app: AppHandle<R>, text: String) -> Result<(), String> {
    app.clipboard().write_text(text).map_err(|e| e.to_string())
}

/// The Google Drive web URLs for an item, or `None` when the path isn't one we can
/// identify. Backs "Open in Google Drive", "Copy Google Drive link", and "Ask Gemini".
///
/// One call, one resolution: the caller picks the URL its command needs rather than
/// asking again per action. Resolution (an xattr read, or a small JSON stub for
/// Google-native docs, or Drive's mirror databases) lives in
/// `file_system::google_drive`; this is the pass-through. Async with the usual
/// blocking hop because it touches the filesystem.
#[tauri::command]
#[specta::specta]
pub async fn google_drive_links(path: String) -> Result<Option<DriveItemLinks>, String> {
    let work = tokio::task::spawn_blocking(move || {
        let path = std::path::PathBuf::from(path);
        let is_directory = path.is_dir();
        crate::file_system::google_drive::item_links(&path, is_directory)
    });
    match tokio::time::timeout(Duration::from_secs(30), work).await {
        Ok(joined) => joined.map_err(|e| e.to_string()),
        // A wedged File Provider can hang a `getxattr`; a missing link just means
        // the caller shows nothing, so a timeout is not worth a scary message.
        Err(_elapsed) => Ok(None),
    }
}

/// Make a cloud-managed file available offline (download it). **iCloud Drive only**:
/// this routes through the `FileManager` ubiquity APIs, which accept iCloud URLs and
/// reject everything else. Third-party providers (Dropbox, Google Drive, OneDrive,
/// Box) can't be driven this way; see `file_system/cloud_actions.rs`.
#[tauri::command]
#[specta::specta]
pub async fn cloud_make_available_offline(path: String) -> Result<(), String> {
    // 30s timeout like every other fs-touching command: a wedged File Provider
    // extension can hang the blocking call indefinitely. The download request is
    // fire-and-forget server-side, so releasing the IPC on timeout is correct.
    let work = tokio::task::spawn_blocking(move || {
        crate::file_system::cloud_actions::request_download(std::path::Path::new(&path))
    });
    match tokio::time::timeout(Duration::from_secs(30), work).await {
        Ok(joined) => joined.map_err(|e| e.to_string())?,
        Err(_elapsed) => Err("Timed out reaching iCloud — give it another try".to_string()),
    }
}

/// Evict a cloud-managed file's local copy, leaving a placeholder. Counterpart to
/// `cloud_make_available_offline`.
#[tauri::command]
#[specta::specta]
pub async fn cloud_remove_download(path: String) -> Result<(), String> {
    // 30s timeout: same hung-File-Provider risk as `cloud_make_available_offline`.
    // Eviction is fire-and-forget server-side, so releasing the IPC on timeout is fine.
    let work =
        tokio::task::spawn_blocking(move || crate::file_system::cloud_actions::evict_item(std::path::Path::new(&path)));
    match tokio::time::timeout(Duration::from_secs(30), work).await {
        Ok(joined) => joined.map_err(|e| e.to_string())?,
        Err(_elapsed) => Err("Timed out reaching iCloud — give it another try".to_string()),
    }
}
