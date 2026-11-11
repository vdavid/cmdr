//! Tauri commands for clipboard file operations (copy/cut/paste).

use std::collections::HashSet;
use std::path::PathBuf;

use crate::clipboard;
use cmdr_operations::MutationError;

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

/// Writes the given paths to the system clipboard and clears any cut state.
///
/// Paths by value, never row numbers: the frontend resolves a pane's selection
/// through the revision-checked `get_selection_snapshot` first, because a watcher
/// insert renumbers rows and old row numbers would name the neighbor.
#[tauri::command]
#[specta::specta]
pub async fn copy_paths_to_clipboard(app: tauri::AppHandle, paths: Vec<String>) -> Result<usize, String> {
    let paths: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).collect();

    if paths.is_empty() {
        return Err("No files to copy".to_string());
    }

    let count = paths.len();

    clipboard::platform::write_file_urls(app, paths)?;

    clipboard::clear_cut_state();

    Ok(count)
}

/// Writes the given paths directly to the system clipboard and marks them as cut.
#[tauri::command]
#[specta::specta]
pub async fn cut_paths_to_clipboard(app: tauri::AppHandle, paths: Vec<String>) -> Result<usize, String> {
    let paths: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).collect();

    if paths.is_empty() {
        return Err("No files to cut".to_string());
    }

    let count = paths.len();
    let cut_paths = paths.clone();

    clipboard::platform::write_file_urls(app, paths)?;

    clipboard::set_cut_state(cut_paths);

    Ok(count)
}

/// Reads file URLs from the system clipboard and checks whether they were cut.
///
/// If the clipboard contents no longer match the recorded cut state (the user copied
/// something else), the stale cut state is automatically cleared.
#[tauri::command]
#[specta::specta]
pub async fn read_clipboard_files(app: tauri::AppHandle) -> Result<ClipboardReadResult, String> {
    let clipboard_paths = clipboard::platform::read_file_urls(app)?;

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
#[tauri::command]
#[specta::specta]
pub async fn read_clipboard_text(app: tauri::AppHandle) -> Result<Option<String>, String> {
    clipboard::platform::read_text(app)
}

/// Reads the highest-intent non-file clipboard flavor (image / PDF / text) and
/// writes it into `directory` as a new `pasted.<ext>` file. Answers
/// `Done { file }` with the created file's name + kind (`file: None` = nothing
/// pasteable, the typed no-op the frontend treats as "no file created", NOT an
/// error), or `StillRunning` past the reply deadline with the real end on
/// `clipboard-paste-settled`.
///
/// Thin edge: reads the RAW pasteboard flavors on the main thread (NSPasteboard is
/// main-thread-only), then picks the flavor + converts TIFF→PNG OFF the main
/// thread (that decode can be hundreds of ms — never on the UI thread), and hands
/// the result to `clipboard::paste_file::write_payload_replying`.
/// `directory` is tilde-expanded for the local `root` volume only.
#[tauri::command]
#[specta::specta]
pub async fn paste_clipboard_as_file(
    app: tauri::AppHandle,
    volume_id: Option<String>,
    directory: String,
) -> Result<clipboard::PasteClipboardReply, MutationError> {
    clipboard::platform::paste_as_file(app, volume_id, directory).await
}

/// Clears the in-process cut state without touching the system clipboard.
#[tauri::command]
#[specta::specta]
pub fn clear_clipboard_cut_state() {
    clipboard::clear_cut_state();
}
