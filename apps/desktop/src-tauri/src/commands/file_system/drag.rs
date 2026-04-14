//! Tauri commands for native drag operations and self-drag overlay.

use crate::file_system::get_paths_at_indices as ops_get_paths_at_indices;
use std::path::PathBuf;
#[cfg(target_os = "macos")]
use std::sync::mpsc::channel;
#[cfg(target_os = "macos")]
use tauri::Manager;

/// Initiates native drag from Rust directly, looking up paths from LISTING_CACHE (macOS only).
#[cfg(target_os = "macos")]
#[tauri::command]
pub fn start_selection_drag(
    app: tauri::AppHandle,
    listing_id: String,
    selected_indices: Vec<usize>,
    include_hidden: bool,
    has_parent: bool,
    mode: String,
    icon_path: String,
) -> Result<(), String> {
    // Get file paths from the cached listing
    let paths = ops_get_paths_at_indices(&listing_id, &selected_indices, include_hidden, has_parent)?;

    if paths.is_empty() {
        return Err("No valid files to drag".to_string());
    }

    // Get the main window
    let window = app.get_webview_window("main").ok_or("Main window not found")?;

    // Determine drag mode (Send-safe)
    let is_copy_mode = mode == "copy";

    // Store icon path for use in closure (PathBuf is Send)
    let icon_path_buf = PathBuf::from(icon_path);

    // Use a channel to get the result from the main thread
    let (tx, rx) = channel();

    // Run on main thread (required by macOS for drag operations)
    // Create DragItem inside the closure since it's not Send
    app.run_on_main_thread(move || {
        // Build DragItem inside the closure (not Send due to Data variant)
        let item = drag::DragItem::Files(paths);

        // Load icon from file path
        let icon = drag::Image::File(icon_path_buf);

        // Create options with the drag mode
        let options = drag::Options {
            skip_animatation_on_cancel_or_failure: false,
            mode: if is_copy_mode {
                drag::DragMode::Copy
            } else {
                drag::DragMode::Move
            },
        };

        let result = drag::start_drag(
            &window,
            item,
            icon,
            |_result, _cursor_pos| {
                // Callback when drag completes - we don't need to do anything here
            },
            options,
        );
        let _ = tx.send(result);
    })
    .map_err(|e| format!("Failed to run on main thread: {}", e))?;

    // Wait for the result
    rx.recv()
        .map_err(|_| "Failed to receive drag result")?
        .map_err(|e| format!("Drag operation failed: {}", e))
}

/// Stub for non-macOS platforms. Returns an error since drag is not yet implemented.
#[cfg(not(target_os = "macos"))]
#[tauri::command]
pub fn start_selection_drag(
    _app: tauri::AppHandle,
    _listing_id: String,
    _selected_indices: Vec<usize>,
    _include_hidden: bool,
    _has_parent: bool,
    _mode: String,
    _icon_path: String,
) -> Result<(), String> {
    Err("Drag operation is not yet supported on this platform".to_string())
}

// ============================================================================
// Self-drag overlay (dynamic drag image swapping)
// ============================================================================

/// Marks a self-drag as active and stores the rich image path so the native swizzle can:
/// - Hide the OS drag image over our window (swap to transparent in `draggingEntered:`)
/// - Show the rich image outside the window (swap back in `draggingExited:`)
#[cfg(target_os = "macos")]
#[tauri::command]
pub fn prepare_self_drag_overlay(rich_image_path: String) {
    crate::drag_image_swap::set_self_drag_active(rich_image_path);
}

/// No-op on non-macOS platforms.
#[cfg(not(target_os = "macos"))]
#[tauri::command]
pub fn prepare_self_drag_overlay(_rich_image_path: String) {}

/// Clears self-drag state after drop or cancellation.
#[cfg(target_os = "macos")]
#[tauri::command]
pub fn clear_self_drag_overlay() {
    crate::drag_image_swap::clear_self_drag_state();
}

/// No-op on non-macOS platforms.
#[cfg(not(target_os = "macos"))]
#[tauri::command]
pub fn clear_self_drag_overlay() {}
