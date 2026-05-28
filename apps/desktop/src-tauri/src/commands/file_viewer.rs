//! Tauri commands for the file viewer.

use tokio::time::Duration;

use super::util::{IpcError, blocking_result_with_timeout};
use crate::file_viewer::{
    self, EncodingOptions, FileEncoding, LineChunk, RangeEnd, SearchMode, SearchPollResult, SeekTarget, ViewerError,
    ViewerOpenResult, ViewerSessionStatus,
};
use log::debug;
use tauri::Manager;
use tauri::menu::MenuItemKind;

const VIEWER_TIMEOUT: Duration = Duration::from_secs(2);

/// Maximum read timeout for `viewer_read_range`. The 100 MiB hard ceiling (enforced
/// at the FE) means even on a slow disk we shouldn't blow this. The backend's per-read
/// cancel flag covers the actually-stuck case via Escape.
const READ_RANGE_TIMEOUT: Duration = Duration::from_secs(60);

/// Opens a viewer session for the given file.
/// Returns session metadata + initial lines from the start of the file.
#[tauri::command]
#[specta::specta]
pub async fn viewer_open(path: String) -> Result<ViewerOpenResult, IpcError> {
    blocking_result_with_timeout(VIEWER_TIMEOUT, move || {
        file_viewer::open_session(&path).map_err(|e| e.to_string())
    })
    .await
}

/// Fetches a range of lines from a viewer session.
///
/// # Arguments
/// * `session_id` - The session ID from `viewer_open`.
/// * `target_type` - One of "line", "byte", or "fraction".
/// * `target_value` - The seek value (line number, byte offset, or fraction 0.0-1.0).
/// * `count` - Number of lines to fetch.
#[tauri::command]
#[specta::specta]
pub async fn viewer_get_lines(
    session_id: String,
    target_type: String,
    target_value: f64,
    count: usize,
) -> Result<LineChunk, IpcError> {
    let target = match target_type.as_str() {
        "line" => SeekTarget::Line(target_value as usize),
        "byte" => SeekTarget::ByteOffset(target_value as u64),
        "fraction" => SeekTarget::Fraction(target_value),
        other => {
            return Err(IpcError::from_err(format!(
                "Unknown target type: {}. Use 'line', 'byte', or 'fraction'.",
                other
            )));
        }
    };

    debug!(
        "viewer_get_lines: session={}, target_type={}, target_value={}, count={}",
        session_id, target_type, target_value, count
    );

    let result = blocking_result_with_timeout(VIEWER_TIMEOUT, move || {
        file_viewer::get_lines(&session_id, target, count).map_err(|e| e.to_string())
    })
    .await?;

    debug!(
        "viewer_get_lines: returned {} lines, first_line_number={}, byte_offset={}, first_line_preview={:?}",
        result.lines.len(),
        result.first_line_number,
        result.byte_offset,
        result.lines.first().map(|s| s.chars().take(50).collect::<String>())
    );

    Ok(result)
}

/// Starts a background search in the viewer session.
/// Poll with `viewer_search_poll` to get results.
///
/// `mode` carries the case-sensitivity and literal-vs-regex toggles. Invalid regex
/// patterns and multiline patterns surface via `viewer_search_poll` as
/// `SearchStatus::InvalidQuery`, not as a command-level error: the session moves
/// into a "you typed something the engine can't run" state, and the FE renders the
/// typed message.
#[tauri::command]
#[specta::specta]
pub fn viewer_search_start(session_id: String, query: String, mode: SearchMode) -> Result<(), String> {
    if query.is_empty() {
        return Err("Search query cannot be empty".to_string());
    }
    file_viewer::search_start(&session_id, query, mode).map_err(|e| e.to_string())
}

/// Polls search progress and new matches since `since_index`.
#[tauri::command]
#[specta::specta]
pub fn viewer_search_poll(session_id: String, since_index: usize) -> Result<SearchPollResult, String> {
    file_viewer::search_poll(&session_id, since_index).map_err(|e| e.to_string())
}

/// Cancels an ongoing search.
#[tauri::command]
#[specta::specta]
pub fn viewer_search_cancel(session_id: String) -> Result<(), String> {
    file_viewer::search_cancel(&session_id).map_err(|e| e.to_string())
}

/// Gets the current status of a viewer session (backend type, indexing state).
#[tauri::command]
#[specta::specta]
pub fn viewer_get_status(session_id: String) -> Result<ViewerSessionStatus, String> {
    file_viewer::get_session_status(&session_id).map_err(|e| e.to_string())
}

/// Closes a viewer session and frees resources.
#[tauri::command]
#[specta::specta]
pub fn viewer_close(session_id: String) -> Result<(), String> {
    file_viewer::close_session(&session_id).map_err(|e| e.to_string())
}

/// Reads a logical range of the file (`anchor` to `focus`) and returns the bytes as a
/// UTF-8 string. Endpoints are normalised internally; either may be `Eof` (used by ⌘A
/// in ByteSeek-no-index mode where the FE doesn't know `totalLines`). Offsets on the
/// wire are UTF-16 code units; the backend clamps lone surrogates to the nearest
/// codepoint boundary.
///
/// Errors come through the typed `ViewerError` enum. The FE matches on the variant tag
/// (per the no-string-classification rule); `Cancelled` and `TimedOut` are the two the
/// copy flow specifically handles.
#[tauri::command]
#[specta::specta]
pub async fn viewer_read_range(
    session_id: String,
    read_id: u64,
    anchor: RangeEnd,
    focus: RangeEnd,
) -> Result<String, ViewerError> {
    match tokio::time::timeout(
        READ_RANGE_TIMEOUT,
        tokio::task::spawn_blocking(move || file_viewer::read_range(&session_id, read_id, anchor, focus)),
    )
    .await
    {
        Ok(Ok(result)) => result,
        Ok(Err(join_err)) => Err(ViewerError::Io {
            message: join_err.to_string(),
        }),
        Err(_) => Err(ViewerError::TimedOut),
    }
}

/// Flips the cancel flag for an in-flight range read. The reader sees the flag at its
/// next per-chunk check and returns `ViewerError::Cancelled`. If the read has already
/// finished, this is a no-op.
#[tauri::command]
#[specta::specta]
pub fn viewer_cancel_read(session_id: String, read_id: u64) -> Result<(), ViewerError> {
    file_viewer::cancel_read(&session_id, read_id)
}

/// Reads a logical range and writes it to `dest_path` atomically (temp+rename). Used
/// by the "Save as file…" action in the > 100 MB refuse dialog and the 10 to 100 MB
/// confirm dialog. Cancellation works the same as `viewer_read_range`.
#[tauri::command]
#[specta::specta]
pub async fn viewer_write_range_to_file(
    session_id: String,
    read_id: u64,
    anchor: RangeEnd,
    focus: RangeEnd,
    dest_path: String,
) -> Result<(), ViewerError> {
    match tokio::time::timeout(
        READ_RANGE_TIMEOUT,
        tokio::task::spawn_blocking(move || {
            file_viewer::write_range_to_file(&session_id, read_id, anchor, focus, std::path::Path::new(&dest_path))
        }),
    )
    .await
    {
        Ok(Ok(result)) => result,
        Ok(Err(join_err)) => Err(ViewerError::Io {
            message: join_err.to_string(),
        }),
        Err(_) => Err(ViewerError::TimedOut),
    }
}

/// Returns the encoding dropdown payload: current selection, detected encoding, and the
/// full list of selectable encodings (with their labels and groups). The FE renders the
/// dropdown directly from this payload — no encoding list lives on the FE.
#[tauri::command]
#[specta::specta]
pub fn viewer_get_encoding_options(session_id: String) -> Result<EncodingOptions, String> {
    file_viewer::get_encoding_options(&session_id).map_err(|e| e.to_string())
}

/// Switches the active encoding for a session. Returns immediately; if the swap
/// requires a background reindex (most cases except UTF-8 ↔ Windows-1252-family),
/// the FE polls `viewer_get_status` for `is_indexing` to track completion.
#[tauri::command]
#[specta::specta]
pub fn viewer_set_encoding(session_id: String, encoding: FileEncoding) -> Result<(), String> {
    file_viewer::set_encoding(&session_id, encoding).map_err(|e| e.to_string())
}

/// Toggles tail mode for a viewer session. When enabled, the backend extends its line index
/// in response to filesystem `Grew` events so the viewport can auto-follow new bytes.
/// When disabled, the FE still receives `viewer:file-changed:<sid>` events and renders a
/// persistent reload toast.
#[tauri::command]
#[specta::specta]
pub fn viewer_set_tail_mode(session_id: String, enabled: bool) -> Result<(), String> {
    file_viewer::set_tail_mode(&session_id, enabled).map_err(|e| e.to_string())
}

/// Reopens the viewer's backend against the file on disk under the session's current
/// encoding. Called by the FE reload toast and on file rotation.
#[tauri::command]
#[specta::specta]
pub fn viewer_reload(session_id: String) -> Result<(), String> {
    file_viewer::reload(&session_id).map_err(|e| e.to_string())
}

/// Sets up a viewer-specific menu on the given window (adds "Word wrap" to View submenu).
#[tauri::command]
#[specta::specta]
pub fn viewer_setup_menu(app_handle: tauri::AppHandle, label: String) -> Result<(), String> {
    let window = app_handle
        .get_webview_window(&label)
        .ok_or_else(|| format!("Window '{}' not found", label))?;
    let menu = crate::menu::build_viewer_menu(&app_handle).map_err(|e| e.to_string())?;
    window.set_menu(menu).map_err(|e| e.to_string())?;
    Ok(())
}

/// Syncs the viewer menu "Word wrap" check state (called when toggled via keyboard).
#[tauri::command]
#[specta::specta]
pub fn viewer_set_word_wrap(app_handle: tauri::AppHandle, label: String, checked: bool) -> Result<(), String> {
    let window = app_handle
        .get_webview_window(&label)
        .ok_or_else(|| format!("Window '{}' not found", label))?;
    let Some(menu) = window.menu() else {
        return Ok(());
    };
    for item in menu.items().map_err(|e| e.to_string())? {
        if let MenuItemKind::Submenu(submenu) = item
            && submenu.text().map_err(|e| e.to_string())? == "View"
        {
            for sub_item in submenu.items().map_err(|e| e.to_string())? {
                if let MenuItemKind::Check(check) = sub_item
                    && check.id().as_ref() == crate::menu::VIEWER_WORD_WRAP_ID
                {
                    check.set_checked(checked).map_err(|e| e.to_string())?;
                    return Ok(());
                }
            }
        }
    }
    Ok(())
}
