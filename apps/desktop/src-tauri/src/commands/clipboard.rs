//! Tauri commands for clipboard file operations (copy/cut/paste).

#[cfg(target_os = "macos")]
use std::collections::HashSet;
#[cfg(target_os = "macos")]
use std::path::PathBuf;

#[cfg(target_os = "macos")]
use objc2::MainThreadMarker;

#[cfg(target_os = "macos")]
use crate::file_system::get_paths_at_indices as ops_get_paths_at_indices;

use crate::clipboard;

#[derive(serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ClipboardReadResult {
    paths: Vec<String>,
    is_cut: bool,
    /// Per-path top-level kind, index-aligned with `paths`: `Some(true)` =
    /// directory, `Some(false)` = file, `None` = unknown (stat failed). Lets the
    /// paste path split the completion toast into files vs. folders without
    /// walking trees. Clipboard file URLs are always real local paths, so in
    /// practice these resolve; `None` falls back to the flattened wording.
    is_directory: Vec<Option<bool>>,
}

/// Resolves selected file paths and writes them to the system clipboard.
/// Clears any existing cut state (this is a copy, not a cut).
#[cfg(target_os = "macos")]
#[tauri::command]
#[specta::specta]
pub async fn copy_files_to_clipboard(
    app: tauri::AppHandle,
    listing_id: String,
    selected_indices: Vec<usize>,
    cursor_index: usize,
    has_parent: bool,
    include_hidden: bool,
) -> Result<usize, String> {
    let indices = resolve_indices(&selected_indices, cursor_index, has_parent);
    let paths = ops_get_paths_at_indices(&listing_id, &indices, include_hidden, has_parent)?;

    if paths.is_empty() {
        return Err("No files to copy".to_string());
    }

    let count = paths.len();

    // Write to pasteboard on the main thread (NSPasteboard requires it)
    let (tx, rx) = std::sync::mpsc::channel();
    app.run_on_main_thread(move || {
        let mtm = MainThreadMarker::new().expect("run_on_main_thread runs on the main thread");
        let result = clipboard::write_file_urls_to_clipboard(mtm, &paths);
        let _ = tx.send(result);
    })
    .map_err(|e| format!("Couldn't run on main thread: {e}"))?;

    rx.recv()
        .map_err(|e| format!("Couldn't receive pasteboard result: {e}"))??;

    clipboard::clear_cut_state();

    Ok(count)
}

/// Writes the given paths directly to the system clipboard. Used when the
/// caller already has the absolute paths (search-results pane, where there's
/// no backend listing to resolve indices against). Clears any cut state.
///
/// Mirrors `copy_files_to_clipboard` but bypasses the listing-cache lookup.
#[cfg(target_os = "macos")]
#[tauri::command]
#[specta::specta]
pub async fn copy_paths_to_clipboard(app: tauri::AppHandle, paths: Vec<String>) -> Result<usize, String> {
    let paths: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).collect();

    if paths.is_empty() {
        return Err("No files to copy".to_string());
    }

    let count = paths.len();

    let (tx, rx) = std::sync::mpsc::channel();
    app.run_on_main_thread(move || {
        let mtm = MainThreadMarker::new().expect("run_on_main_thread runs on the main thread");
        let result = clipboard::write_file_urls_to_clipboard(mtm, &paths);
        let _ = tx.send(result);
    })
    .map_err(|e| format!("Couldn't run on main thread: {e}"))?;

    rx.recv()
        .map_err(|e| format!("Couldn't receive pasteboard result: {e}"))??;

    clipboard::clear_cut_state();

    Ok(count)
}

/// Writes the given paths directly to the system clipboard and marks them as cut.
/// Sibling of `cut_files_to_clipboard` for paths-by-value callers.
#[cfg(target_os = "macos")]
#[tauri::command]
#[specta::specta]
pub async fn cut_paths_to_clipboard(app: tauri::AppHandle, paths: Vec<String>) -> Result<usize, String> {
    let paths: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).collect();

    if paths.is_empty() {
        return Err("No files to cut".to_string());
    }

    let count = paths.len();
    let cut_paths = paths.clone();

    let (tx, rx) = std::sync::mpsc::channel();
    app.run_on_main_thread(move || {
        let mtm = MainThreadMarker::new().expect("run_on_main_thread runs on the main thread");
        let result = clipboard::write_file_urls_to_clipboard(mtm, &paths);
        let _ = tx.send(result);
    })
    .map_err(|e| format!("Couldn't run on main thread: {e}"))?;

    rx.recv()
        .map_err(|e| format!("Couldn't receive pasteboard result: {e}"))??;

    clipboard::set_cut_state(cut_paths);

    Ok(count)
}

/// Resolves selected file paths, writes them to the system clipboard, and marks them as cut.
/// On paste, files will be moved instead of copied.
#[cfg(target_os = "macos")]
#[tauri::command]
#[specta::specta]
pub async fn cut_files_to_clipboard(
    app: tauri::AppHandle,
    listing_id: String,
    selected_indices: Vec<usize>,
    cursor_index: usize,
    has_parent: bool,
    include_hidden: bool,
) -> Result<usize, String> {
    let indices = resolve_indices(&selected_indices, cursor_index, has_parent);
    let paths = ops_get_paths_at_indices(&listing_id, &indices, include_hidden, has_parent)?;

    if paths.is_empty() {
        return Err("No files to cut".to_string());
    }

    let count = paths.len();
    let cut_paths = paths.clone();

    // Write to pasteboard on the main thread
    let (tx, rx) = std::sync::mpsc::channel();
    app.run_on_main_thread(move || {
        let mtm = MainThreadMarker::new().expect("run_on_main_thread runs on the main thread");
        let result = clipboard::write_file_urls_to_clipboard(mtm, &paths);
        let _ = tx.send(result);
    })
    .map_err(|e| format!("Couldn't run on main thread: {e}"))?;

    rx.recv()
        .map_err(|e| format!("Couldn't receive pasteboard result: {e}"))??;

    clipboard::set_cut_state(cut_paths);

    Ok(count)
}

/// Reads file URLs from the system clipboard and checks whether they were cut.
///
/// If the clipboard contents no longer match the recorded cut state (the user copied
/// something else), the stale cut state is automatically cleared.
#[cfg(target_os = "macos")]
#[tauri::command]
#[specta::specta]
pub async fn read_clipboard_files(app: tauri::AppHandle) -> Result<ClipboardReadResult, String> {
    // Read from pasteboard on the main thread
    let (tx, rx) = std::sync::mpsc::channel();
    app.run_on_main_thread(move || {
        let mtm = MainThreadMarker::new().expect("run_on_main_thread runs on the main thread");
        let result = clipboard::read_file_urls_from_clipboard(mtm);
        let _ = tx.send(result);
    })
    .map_err(|e| format!("Couldn't run on main thread: {e}"))?;

    let clipboard_paths = rx
        .recv()
        .map_err(|e| format!("Couldn't receive pasteboard result: {e}"))??;

    // Check cut state: if set, verify paths match (order-insensitive)
    let is_cut = if let Some(cut_paths) = clipboard::get_cut_state() {
        let clipboard_set: HashSet<&PathBuf> = clipboard_paths.iter().collect();
        let cut_set: HashSet<&PathBuf> = cut_paths.iter().collect();

        if clipboard_set == cut_set {
            true
        } else {
            // Clipboard changed since the cut -- clear stale state
            clipboard::clear_cut_state();
            false
        }
    } else {
        false
    };

    let paths: Vec<String> = clipboard_paths
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();

    // Resolve each path's top-level kind so the paste completion toast can split
    // files vs. folders. One batched stat off the main thread (the pasteboard
    // read already happened above); per-item failures map to `None`, never an
    // error for the batch. Clipboard URLs are real local paths, so this is fast.
    let paths_for_stat = paths.clone();
    let is_directory =
        tokio::task::spawn_blocking(move || crate::commands::file_system::stat_paths_kinds_blocking(&paths_for_stat))
            .await
            .unwrap_or_else(|_| vec![None; paths.len()]);

    Ok(ClipboardReadResult {
        paths,
        is_cut,
        is_directory,
    })
}

/// Reads plain text from the system clipboard.
///
/// Used by the frontend to paste text into input fields. Going through Rust bypasses
/// WebKit's `navigator.clipboard.readText()` permission popup.
#[cfg(target_os = "macos")]
#[tauri::command]
#[specta::specta]
pub async fn read_clipboard_text(app: tauri::AppHandle) -> Result<Option<String>, String> {
    let (tx, rx) = std::sync::mpsc::channel();
    app.run_on_main_thread(move || {
        let mtm = MainThreadMarker::new().expect("run_on_main_thread runs on the main thread");
        let text = clipboard::read_text_from_clipboard(mtm);
        let _ = tx.send(text);
    })
    .map_err(|e| format!("Couldn't run on main thread: {e}"))?;

    rx.recv()
        .map_err(|e| format!("Couldn't receive pasteboard result: {e}"))
}

#[cfg(not(target_os = "macos"))]
#[tauri::command]
#[specta::specta]
pub async fn read_clipboard_text(_app: tauri::AppHandle) -> Result<Option<String>, String> {
    Err("Clipboard operations are not yet supported on this platform".to_string())
}

/// Clears the in-process cut state without touching the system clipboard.
#[tauri::command]
#[specta::specta]
pub fn clear_clipboard_cut_state() {
    clipboard::clear_cut_state();
}

// --- Linux stubs ---

#[cfg(not(target_os = "macos"))]
#[tauri::command]
#[specta::specta]
pub async fn copy_files_to_clipboard(
    _app: tauri::AppHandle,
    _listing_id: String,
    _selected_indices: Vec<usize>,
    _cursor_index: usize,
    _has_parent: bool,
    _include_hidden: bool,
) -> Result<usize, String> {
    Err("Clipboard file operations are not yet supported on this platform".to_string())
}

#[cfg(not(target_os = "macos"))]
#[tauri::command]
#[specta::specta]
pub async fn cut_files_to_clipboard(
    _app: tauri::AppHandle,
    _listing_id: String,
    _selected_indices: Vec<usize>,
    _cursor_index: usize,
    _has_parent: bool,
    _include_hidden: bool,
) -> Result<usize, String> {
    Err("Clipboard file operations are not yet supported on this platform".to_string())
}

#[cfg(not(target_os = "macos"))]
#[tauri::command]
#[specta::specta]
pub async fn read_clipboard_files(_app: tauri::AppHandle) -> Result<ClipboardReadResult, String> {
    Err("Clipboard file operations are not yet supported on this platform".to_string())
}

#[cfg(not(target_os = "macos"))]
#[tauri::command]
#[specta::specta]
pub async fn copy_paths_to_clipboard(_app: tauri::AppHandle, _paths: Vec<String>) -> Result<usize, String> {
    Err("Clipboard file operations are not yet supported on this platform".to_string())
}

#[cfg(not(target_os = "macos"))]
#[tauri::command]
#[specta::specta]
pub async fn cut_paths_to_clipboard(_app: tauri::AppHandle, _paths: Vec<String>) -> Result<usize, String> {
    Err("Clipboard file operations are not yet supported on this platform".to_string())
}

// --- Helpers ---

/// When no files are selected, falls back to the cursor index (adjusting for the ".." entry).
#[cfg(target_os = "macos")]
fn resolve_indices(selected_indices: &[usize], cursor_index: usize, has_parent: bool) -> Vec<usize> {
    if !selected_indices.is_empty() {
        return selected_indices.to_vec();
    }

    // Nothing selected -- use the cursor position.
    // If the cursor is on ".." (index 0 with has_parent), skip it.
    if has_parent && cursor_index == 0 {
        return Vec::new();
    }

    vec![cursor_index]
}
