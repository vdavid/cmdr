//! Direct file-action commands invoked from the command palette, context menus,
//! and menu items: reveal in Finder, Get Info, open in the default editor, copy
//! text to the clipboard, and the iCloud make-available-offline / remove-download
//! pair. Thin pass-throughs that shell out or delegate to `file_system`.

#[cfg(any(target_os = "macos", target_os = "linux"))]
use std::process::Command;
use tauri::{AppHandle, Runtime};
use tauri_plugin_clipboard_manager::ClipboardExt;

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

/// Open file in the system's default text editor (macOS only)
#[tauri::command]
#[specta::specta]
#[cfg(target_os = "macos")]
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
#[cfg(target_os = "linux")]
pub fn open_in_editor(path: String) -> Result<(), String> {
    Command::new("xdg-open").arg(&path).spawn().map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
#[specta::specta]
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub fn open_in_editor(_path: String) -> Result<(), String> {
    Err("Open in editor is not available on this platform".to_string())
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
/// Specs can assert intent via `open_mock::snapshot` / `open_mock::clear`.
#[tauri::command]
#[specta::specta]
#[cfg(feature = "playwright-e2e")]
pub fn open_path(path: String) -> Result<(), String> {
    open_mock::record(path);
    Ok(())
}

/// In-process record of `open_path` requests for the `playwright-e2e` build.
/// Mirrors the clipboard mock (`crate::clipboard`): compiled only under the
/// feature so prod/dev binaries never link it, and it never touches the OS.
#[cfg(feature = "playwright-e2e")]
mod open_mock {
    use std::path::PathBuf;
    use std::sync::{LazyLock, Mutex};

    use crate::ignore_poison::IgnorePoison;

    static OPENED: LazyLock<Mutex<Vec<PathBuf>>> = LazyLock::new(|| Mutex::new(Vec::new()));

    /// Records an open request without launching anything.
    pub fn record(path: String) {
        log::info!(target: "file_actions", "[mock] open_path recorded (not launched): {path}");
        OPENED.lock_ignore_poison().push(PathBuf::from(path));
    }

    /// Returns the paths opened so far, oldest first.
    #[allow(dead_code, reason = "Exported for future Playwright specs that assert open intent.")]
    pub fn snapshot() -> Vec<PathBuf> {
        OPENED.lock_ignore_poison().clone()
    }

    /// Clears the recorded open requests (per-test reset).
    #[allow(dead_code, reason = "Exported for future Playwright specs that reset open state.")]
    pub fn clear() {
        OPENED.lock_ignore_poison().clear();
    }
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
    match tokio::time::timeout(tokio::time::Duration::from_secs(30), work).await {
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
    match tokio::time::timeout(tokio::time::Duration::from_secs(30), work).await {
        Ok(joined) => joined.map_err(|e| e.to_string())?,
        Err(_elapsed) => Err("Timed out reaching iCloud — give it another try".to_string()),
    }
}
