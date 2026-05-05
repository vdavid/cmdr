//! Tauri commands for native drag operations and self-drag overlay.

#[cfg(target_os = "macos")]
use crate::file_system::get_paths_at_indices as ops_get_paths_at_indices;
#[cfg(target_os = "macos")]
use crate::native_drag;
#[cfg(target_os = "macos")]
use std::path::PathBuf;
#[cfg(target_os = "macos")]
use std::sync::mpsc::channel;
#[cfg(target_os = "macos")]
use tauri::Manager;

/// Begins a native drag with the given file paths. Used for single-file drags
/// where the frontend has the path directly (no listing-cache lookup needed).
#[cfg(target_os = "macos")]
#[tauri::command]
pub fn start_drag_paths(app: tauri::AppHandle, paths: Vec<String>, icon_path: String) -> Result<(), String> {
    let path_bufs: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).collect();
    if path_bufs.is_empty() {
        return Err("No valid files to drag".to_string());
    }
    run_drag_on_main_thread(&app, path_bufs, PathBuf::from(icon_path))
}

/// Stub for non-macOS platforms. Returns an error since drag is not yet implemented.
#[cfg(not(target_os = "macos"))]
#[tauri::command]
pub fn start_drag_paths(_app: tauri::AppHandle, _paths: Vec<String>, _icon_path: String) -> Result<(), String> {
    Err("Drag operation is not yet supported on this platform".to_string())
}

/// Initiates native drag from Rust directly, looking up paths from `LISTING_CACHE` (macOS only).
#[cfg(target_os = "macos")]
#[tauri::command]
pub fn start_selection_drag(
    app: tauri::AppHandle,
    listing_id: String,
    selected_indices: Vec<usize>,
    include_hidden: bool,
    has_parent: bool,
    icon_path: String,
) -> Result<(), String> {
    let paths = ops_get_paths_at_indices(&listing_id, &selected_indices, include_hidden, has_parent)?;

    if paths.is_empty() {
        return Err("No valid files to drag".to_string());
    }

    run_drag_on_main_thread(&app, paths, PathBuf::from(icon_path))
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
    _icon_path: String,
) -> Result<(), String> {
    Err("Drag operation is not yet supported on this platform".to_string())
}

/// Hops to the AppKit main thread, builds the drag session, and returns synchronously.
/// `NSDraggingItem`s and the source class are not `Send`, so everything happens inside
/// the closure; the result travels back via a one-shot channel.
#[cfg(target_os = "macos")]
fn run_drag_on_main_thread(app: &tauri::AppHandle, paths: Vec<PathBuf>, icon_path: PathBuf) -> Result<(), String> {
    let window = app.get_webview_window("main").ok_or("Main window not found")?;
    let (tx, rx) = channel();

    app.run_on_main_thread(move || {
        let result = native_drag::start_drag(&window, paths, &icon_path);
        let _ = tx.send(result);
    })
    .map_err(|e| format!("Failed to run on main thread: {}", e))?;

    rx.recv().map_err(|_| "Failed to receive drag result")?
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
