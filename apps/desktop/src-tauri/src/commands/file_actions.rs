//! Direct file-action commands invoked from the command palette, context menus,
//! and menu items: reveal in Finder, Get Info, open in the default editor, copy
//! text to the clipboard, and the iCloud make-available-offline / remove-download
//! pair. Thin pass-throughs that shell out or delegate to `file_system`.

#[cfg(any(target_os = "macos", target_os = "linux"))]
use std::process::Command;
use tauri::{AppHandle, Runtime};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tokio::time::Duration;

#[cfg(target_os = "macos")]
use super::util::{TimedOut, blocking_typed_result_with_timeout, blocking_with_timeout_flag};
#[cfg(target_os = "macos")]
use crate::file_system::terminal::{OpenTerminalError, OpenTerminalOutcome, TerminalAppList};

/// Listing terminals is a handful of LaunchServices lookups and bundle-icon
/// reads. Generous enough for a custom pick sitting on a slow mount, short enough
/// that the settings row doesn't hang on one.
#[cfg(target_os = "macos")]
const TERMINAL_APPS_TIMEOUT: Duration = Duration::from_secs(2);

/// Launching is one `spawn` of `open`; the wait is for the installed-app lookup
/// in front of it.
#[cfg(target_os = "macos")]
const OPEN_TERMINAL_TIMEOUT: Duration = Duration::from_secs(5);

/// Show a file in Finder (reveal in parent folder)
#[tauri::command]
#[specta::specta]
#[cfg(target_os = "macos")]
pub fn show_in_finder(path: String) -> Result<(), String> {
    Command::new("open")
        .arg("-R")
        .arg(&path)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Show a file in the default file manager (open parent folder via xdg-open)
#[tauri::command]
#[specta::specta]
#[cfg(target_os = "linux")]
pub fn show_in_finder(path: String) -> Result<(), String> {
    let parent = std::path::Path::new(&path)
        .parent()
        .unwrap_or(std::path::Path::new("/"));
    Command::new("xdg-open")
        .arg(parent)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
#[specta::specta]
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub fn show_in_finder(_path: String) -> Result<(), String> {
    Err("Show in file manager is not available on this platform".to_string())
}

/// Open the Get Info window for a file (macOS only, no-op on other platforms)
#[tauri::command]
#[specta::specta]
#[cfg(target_os = "macos")]
pub fn get_info(path: String) -> Result<(), String> {
    // Pass the path as a positional argument via `on run argv` to avoid AppleScript injection.
    let script = r#"on run argv
        tell application "Finder"
            activate
            open information window of (POSIX file (item 1 of argv) as alias)
        end tell
    end run"#;

    Command::new("osascript")
        .arg("-e")
        .arg(script)
        .arg(&path)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
#[specta::specta]
#[cfg(not(target_os = "macos"))]
pub fn get_info(_path: String) -> Result<(), String> {
    Ok(())
}

/// Open file in the system's default text editor (macOS only).
///
/// Backs the `file.edit` command and the "open the freshly created file" step of the
/// new-file flow. Like `open_path`, the `playwright-e2e` build swaps in a launch-free
/// variant: `open -t` spawns a TextEdit window per call, and the E2E suite (which
/// creates files and opens them in the editor) has no way to close them, so they pile
/// up across runs. The E2E variant records into the same `crate::open_mock` store as
/// `open_path`, so specs assert intent via `e2e_opened_paths`.
#[tauri::command]
#[specta::specta]
#[cfg(all(target_os = "macos", not(feature = "playwright-e2e")))]
pub fn open_in_editor(path: String) -> Result<(), String> {
    Command::new("open")
        .arg("-t")
        .arg(&path)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
#[specta::specta]
#[cfg(all(target_os = "linux", not(feature = "playwright-e2e")))]
pub fn open_in_editor(path: String) -> Result<(), String> {
    Command::new("xdg-open").arg(&path).spawn().map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
#[specta::specta]
#[cfg(all(not(any(target_os = "macos", target_os = "linux")), not(feature = "playwright-e2e")))]
pub fn open_in_editor(_path: String) -> Result<(), String> {
    Err("Open in editor is not available on this platform".to_string())
}

/// E2E variant: record the editor-open request instead of launching TextEdit,
/// funneling into the same `crate::open_mock` store as `open_path` so no orphan windows leak.
#[tauri::command]
#[specta::specta]
#[cfg(feature = "playwright-e2e")]
pub fn open_in_editor(path: String) -> Result<(), String> {
    crate::open_mock::record(path);
    Ok(())
}

/// Open a file (or folder) with the system's default application.
///
/// Backs the frontend "open" action (Enter / double-click / MCP `open_under_cursor`
/// on a file entry). Kept in Rust rather than the frontend opener plugin so the
/// `playwright-e2e` build can swap in a launch-free variant: the real one spawns
/// TextEdit/Preview/etc. per open, and the E2E suite (which creates and opens
/// files) has no way to close them, so they pile up unbounded across runs.
#[tauri::command]
#[specta::specta]
#[cfg(all(target_os = "macos", not(feature = "playwright-e2e")))]
pub fn open_path(path: String) -> Result<(), String> {
    Command::new("open").arg(&path).spawn().map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
#[specta::specta]
#[cfg(all(target_os = "linux", not(feature = "playwright-e2e")))]
pub fn open_path(path: String) -> Result<(), String> {
    Command::new("xdg-open").arg(&path).spawn().map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
#[specta::specta]
#[cfg(all(not(any(target_os = "macos", target_os = "linux")), not(feature = "playwright-e2e")))]
pub fn open_path(_path: String) -> Result<(), String> {
    Err("Open is not available on this platform".to_string())
}

/// E2E variant: record the open request instead of launching an external app,
/// so the suite never floods the desktop with orphan TextEdit/Preview windows.
/// Specs can assert intent via `crate::open_mock`.
#[tauri::command]
#[specta::specta]
#[cfg(feature = "playwright-e2e")]
pub fn open_path(path: String) -> Result<(), String> {
    crate::open_mock::record(path);
    Ok(())
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

/// Copy text to clipboard
#[tauri::command]
#[specta::specta]
pub fn copy_to_clipboard<R: Runtime>(app: AppHandle<R>, text: String) -> Result<(), String> {
    app.clipboard().write_text(text).map_err(|e| e.to_string())
}

/// Make a cloud-managed file available offline (download it). On macOS, talks to the
/// File Provider extension responsible for the file (iCloud Drive, Dropbox, GDrive,
/// OneDrive, Box, etc.).
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
