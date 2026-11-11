//! The answer where there's no drag-out yet: a drag is refused and the self-drag overlay has
//! nothing to swap. Linux and Windows re-export it.

/// Not supported yet.
pub(crate) fn start_drag(
    _app: tauri::AppHandle,
    _paths: Vec<String>,
    _icon_path: String,
    _source_volume_id: Option<String>,
) -> Result<(), String> {
    Err("Drag operation is not yet supported on this platform".to_string())
}

/// Nothing to swap.
pub(crate) fn prepare_self_drag_overlay(_rich_image_path: String) {}

/// Nothing to clear.
pub(crate) fn clear_self_drag_overlay() {}

/// Nothing to push.
pub(crate) fn set_self_drag_resolved_op(_operation: String) {}
