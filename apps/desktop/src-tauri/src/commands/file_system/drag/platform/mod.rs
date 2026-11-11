//! How each OS starts a drag out of the app and swaps the image of a drag inside it. A platform
//! seam (`docs/guides/platform-seams.md`).

#[cfg_attr(target_os = "macos", path = "macos.rs")]
#[cfg_attr(target_os = "linux", path = "linux.rs")]
#[cfg_attr(target_os = "windows", path = "windows.rs")]
mod imp;
#[cfg(any(target_os = "linux", target_os = "windows"))]
mod unsupported;

type StartDrag = fn(tauri::AppHandle, Vec<String>, String, Option<String>) -> Result<(), String>;

const _: StartDrag = start_drag;
const _: fn(String) = prepare_self_drag_overlay;
const _: fn() = clear_self_drag_overlay;
const _: fn(String) = set_self_drag_resolved_op;

/// Starts a native drag of `paths` with `icon_path` as its image; `source_volume_id` decides
/// whether the session hands out file URLs or promises the bytes.
pub(super) use imp::start_drag;

/// Marks a drag of ours as active, with the rich image to show outside the window.
pub(super) use imp::prepare_self_drag_overlay;

/// Clears that state after the drop or a cancel.
pub(super) use imp::clear_self_drag_overlay;

/// The operation a drop would do (`copy` or `move`), so the OS badge matches it.
pub(super) use imp::set_self_drag_resolved_op;
