//! Tauri commands for the file viewer.

use tokio::time::Duration;

use super::util::{IpcError, blocking_result_with_timeout};
use crate::file_viewer::{
    self, LineChunk, RangeEnd, SearchPollResult, SeekTarget, ViewerError, ViewerOpenResult, ViewerSessionStatus,
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
#[tauri::command]
#[specta::specta]
pub fn viewer_search_start(session_id: String, query: String) -> Result<(), String> {
    if query.is_empty() {
        return Err("Search query cannot be empty".to_string());
    }
    file_viewer::search_start(&session_id, query).map_err(|e| e.to_string())
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
