//! Tauri commands for native drag operations and self-drag overlay. Each OS's half is
//! `platform/`.

mod platform;

/// Begins a native drag with the given file paths. Used for single-file drags
/// where the frontend has the path directly (no listing-cache lookup needed),
/// and for the search-results pane's paths-by-value drag.
///
/// `source_volume_id` lets the caller declare the session's source volume so the
/// pasteboard layout can strip materializable representations for virtual
/// volumes (the FE drag-start path has this id since the recorded-identity work).
/// Absent (`None`) defaults to a local session — back-compatible.
#[tauri::command]
#[specta::specta]
pub fn start_drag_paths(
    app: tauri::AppHandle,
    paths: Vec<String>,
    icon_path: String,
    source_volume_id: Option<String>,
) -> Result<(), String> {
    platform::start_drag(app, paths, icon_path, source_volume_id)
}

// ============================================================================
// Self-drag overlay (dynamic drag image swapping)
// ============================================================================

/// Marks a self-drag as active and stores the rich image path so the native swizzle can:
/// - Hide the OS drag image over our window (swap to transparent in `draggingEntered:`)
/// - Show the rich image outside the window (swap back in `draggingExited:`)
#[tauri::command]
#[specta::specta]
pub fn prepare_self_drag_overlay(rich_image_path: String) {
    platform::prepare_self_drag_overlay(rich_image_path);
}

/// Clears self-drag state after drop or cancellation.
#[tauri::command]
#[specta::specta]
pub fn clear_self_drag_overlay() {
    platform::clear_self_drag_overlay();
}

/// Pushes the resolved drop operation for the current self-drag down to the native swizzle.
/// The swizzled `draggingEntered:`/`draggingUpdated:` reads this and overrides wry's hardcoded
/// `Copy` return so the OS-rendered "+" badge tracks our chosen op (Copy → +, Move → no badge).
/// Unknown values are ignored.
#[tauri::command]
#[specta::specta]
pub fn set_self_drag_resolved_op(operation: String) {
    platform::set_self_drag_resolved_op(operation);
}

/// Lets a promised drop's folder go (`drop_in/folders.rs`): its transfer dialog was
/// cancelled, or the drop hit no target. A folder a started copy reads from stays
/// until that copy ends. A few local removals, so the write tier's 5 s.
#[tauri::command]
#[specta::specta]
pub async fn release_promise_drop(drop_id: String) {
    // Nothing to release (claimed, or already gone) is the normal no-op, so the answer
    // goes nowhere: the frontend has nothing to say about it either way.
    let _released = cmdr_fs::deadline::blocking_with_timeout(std::time::Duration::from_secs(5), false, move || {
        crate::drop_in::folders::release_drop(&drop_id)
    })
    .await;
}
