//! Tauri commands for native drag operations and self-drag overlay.

#[cfg(target_os = "macos")]
use crate::file_system::get_paths_at_indices as ops_get_paths_at_indices;
#[cfg(target_os = "macos")]
use crate::native_drag::{self, DragSessionLocality};
#[cfg(target_os = "macos")]
use std::path::PathBuf;
#[cfg(target_os = "macos")]
use std::sync::mpsc::channel;
#[cfg(target_os = "macos")]
use tauri::Manager;

/// Resolves a source volume id to its drag-session locality.
///
/// Local FS and OS-mounted shares (`supports_local_fs_access() == true`) keep the
/// legacy file-url layout; protocol-only / virtual volumes (MTP, direct SMB,
/// search-results) advertise nothing materializable. An unknown or absent id
/// (the back-compatible default for callers that don't know their volume)
/// resolves to `Local` — the conservative choice that preserves today's layout.
#[cfg(target_os = "macos")]
fn locality_for_volume(volume_id: Option<&str>) -> DragSessionLocality {
    let Some(volume_id) = volume_id else {
        return DragSessionLocality::Local;
    };
    match crate::file_system::get_volume_manager().get(volume_id) {
        Some(volume) if !volume.supports_local_fs_access() => DragSessionLocality::Virtual,
        _ => DragSessionLocality::Local,
    }
}

/// Begins a native drag with the given file paths. Used for single-file drags
/// where the frontend has the path directly (no listing-cache lookup needed),
/// and for the search-results pane's paths-by-value drag.
///
/// `source_volume_id` lets the caller declare the session's source volume so the
/// pasteboard layout can strip materializable representations for virtual
/// volumes (the FE drag-start path has this id since the recorded-identity work).
/// Absent (`None`) defaults to a local session — back-compatible.
#[cfg(target_os = "macos")]
#[tauri::command]
#[specta::specta]
pub fn start_drag_paths(
    app: tauri::AppHandle,
    paths: Vec<String>,
    icon_path: String,
    source_volume_id: Option<String>,
) -> Result<(), String> {
    let path_bufs: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).collect();
    if path_bufs.is_empty() {
        return Err("No valid files to drag".to_string());
    }
    // Archive-inner paths aren't materializable as local file URLs (they live
    // inside the `.zip`), so they force a VIRTUAL session — a file-promise
    // provider — even though the source volume id is the local parent drive.
    let locality = if path_bufs
        .iter()
        .any(|p| crate::file_system::volume::backends::archive::path_crosses_archive_boundary(p))
    {
        DragSessionLocality::Virtual
    } else {
        locality_for_volume(source_volume_id.as_deref())
    };
    run_drag_on_main_thread(&app, path_bufs, PathBuf::from(icon_path), locality, source_volume_id)
}

/// Stub for non-macOS platforms. Returns an error since drag is not yet implemented.
#[cfg(not(target_os = "macos"))]
#[tauri::command]
#[specta::specta]
pub fn start_drag_paths(
    _app: tauri::AppHandle,
    _paths: Vec<String>,
    _icon_path: String,
    _source_volume_id: Option<String>,
) -> Result<(), String> {
    Err("Drag operation is not yet supported on this platform".to_string())
}

/// Initiates native drag from Rust directly, looking up paths from `LISTING_CACHE` (macOS only).
#[cfg(target_os = "macos")]
#[tauri::command]
#[specta::specta]
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

    // The listing knows its own volume, so the session's locality is derivable
    // here without a new parameter.
    let volume_id = crate::file_system::listing::get_listing_volume_id_and_path(&listing_id).map(|(vid, _)| vid);
    let locality = locality_for_volume(volume_id.as_deref());

    run_drag_on_main_thread(&app, paths, PathBuf::from(icon_path), locality, volume_id)
}

/// Stub for non-macOS platforms. Returns an error since drag is not yet implemented.
#[cfg(not(target_os = "macos"))]
#[tauri::command]
#[specta::specta]
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
fn run_drag_on_main_thread(
    app: &tauri::AppHandle,
    paths: Vec<PathBuf>,
    icon_path: PathBuf,
    locality: DragSessionLocality,
    source_volume_id: Option<String>,
) -> Result<(), String> {
    let window = app.get_webview_window("main").ok_or("Main window not found")?;
    let (tx, rx) = channel();

    app.run_on_main_thread(move || {
        let result = native_drag::start_drag(&window, paths, &icon_path, locality, source_volume_id.as_deref());
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
#[specta::specta]
pub fn prepare_self_drag_overlay(rich_image_path: String) {
    crate::drag_image_swap::set_self_drag_active(rich_image_path);
}

/// No-op on non-macOS platforms.
#[cfg(not(target_os = "macos"))]
#[tauri::command]
#[specta::specta]
pub fn prepare_self_drag_overlay(_rich_image_path: String) {}

/// Clears self-drag state after drop or cancellation.
#[cfg(target_os = "macos")]
#[tauri::command]
#[specta::specta]
pub fn clear_self_drag_overlay() {
    crate::drag_image_swap::clear_self_drag_state();
}

/// No-op on non-macOS platforms.
#[cfg(not(target_os = "macos"))]
#[tauri::command]
#[specta::specta]
pub fn clear_self_drag_overlay() {}

/// Pushes the resolved drop operation for the current self-drag down to the native swizzle.
/// The swizzled `draggingEntered:`/`draggingUpdated:` reads this and overrides wry's hardcoded
/// `Copy` return so the OS-rendered "+" badge tracks our chosen op (Copy → +, Move → no badge).
/// Unknown values are ignored.
#[cfg(target_os = "macos")]
#[tauri::command]
#[specta::specta]
pub fn set_self_drag_resolved_op(operation: String) {
    use crate::drag_image_swap::SelfDragOp;
    let op = match operation.as_str() {
        "copy" => SelfDragOp::Copy,
        "move" => SelfDragOp::Move,
        _ => return,
    };
    crate::drag_image_swap::set_self_drag_resolved_op(op);
}

/// No-op on non-macOS platforms.
#[cfg(not(target_os = "macos"))]
#[tauri::command]
#[specta::specta]
pub fn set_self_drag_resolved_op(_operation: String) {}
