//! Tauri commands for the file viewer.

use crate::file_viewer::{self, LineChunk, SearchPollResult, SeekTarget, ViewerOpenResult, ViewerSessionStatus};
use log::debug;
use tauri::Manager;
use tauri::menu::MenuItemKind;

/// Opens a viewer session for the given file.
/// Returns session metadata + initial lines from the start of the file.
#[tauri::command]
pub fn viewer_open(path: String) -> Result<ViewerOpenResult, String> {
    file_viewer::open_session(&path).map_err(|e| e.to_string())
}

/// Fetches a range of lines from a viewer session.
///
/// # Arguments
/// * `session_id` - The session ID from `viewer_open`.
/// * `target_type` - One of "line", "byte", or "fraction".
/// * `target_value` - The seek value (line number, byte offset, or fraction 0.0-1.0).
/// * `count` - Number of lines to fetch.
#[tauri::command]
pub fn viewer_get_lines(
    session_id: String,
    target_type: String,
    target_value: f64,
    count: usize,
) -> Result<LineChunk, String> {
    let target = match target_type.as_str() {
        "line" => SeekTarget::Line(target_value as usize),
        "byte" => SeekTarget::ByteOffset(target_value as u64),
        "fraction" => SeekTarget::Fraction(target_value),
        other => {
            return Err(format!(
                "Unknown target type: {}. Use 'line', 'byte', or 'fraction'.",
                other
            ));
        }
    };

    debug!(
        "viewer_get_lines: session={}, target_type={}, target_value={}, count={}",
        session_id, target_type, target_value, count
    );

    let result = file_viewer::get_lines(&session_id, target, count).map_err(|e| e.to_string())?;

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
pub fn viewer_search_start(session_id: String, query: String) -> Result<(), String> {
    if query.is_empty() {
        return Err("Search query cannot be empty".to_string());
    }
    file_viewer::search_start(&session_id, query).map_err(|e| e.to_string())
}

/// Polls search progress and current matches.
#[tauri::command]
pub fn viewer_search_poll(session_id: String) -> Result<SearchPollResult, String> {
    file_viewer::search_poll(&session_id).map_err(|e| e.to_string())
}

/// Cancels an ongoing search.
#[tauri::command]
pub fn viewer_search_cancel(session_id: String) -> Result<(), String> {
    file_viewer::search_cancel(&session_id).map_err(|e| e.to_string())
}

/// Gets the current status of a viewer session (backend type, indexing state).
#[tauri::command]
pub fn viewer_get_status(session_id: String) -> Result<ViewerSessionStatus, String> {
    file_viewer::get_session_status(&session_id).map_err(|e| e.to_string())
}

/// Closes a viewer session and frees resources.
#[tauri::command]
pub fn viewer_close(session_id: String) -> Result<(), String> {
    file_viewer::close_session(&session_id).map_err(|e| e.to_string())
}

/// Sets up a viewer-specific menu on the given window (adds "Word wrap" to View submenu).
#[tauri::command]
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
