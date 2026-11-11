//! How each OS reveals a file, shows its Get Info window, and opens it in a text editor. A
//! platform seam (`docs/guides/platform-seams.md`).

use std::future::Future;

use crate::file_system::get_info::GetInfoError;
use crate::file_system::text_editor::{EditorOpenReport, OpenInEditorError};

#[cfg_attr(target_os = "macos", path = "macos.rs")]
#[cfg_attr(target_os = "linux", path = "linux.rs")]
#[cfg_attr(target_os = "windows", path = "windows.rs")]
mod imp;

const _: fn(String) -> Result<(), String> = reveal_in_file_manager;
// An `async fn` doesn't coerce to a fn pointer, so these pins name each one's arguments and the
// output its future resolves to.
const _: fn() = || {
    fn resolves_to<T, F: Future<Output = T>>(_: F) {}
    resolves_to::<Result<(), GetInfoError>, _>(get_info(String::new()));
    resolves_to::<Result<EditorOpenReport, OpenInEditorError>, _>(open_in_editor(String::new(), String::new(), false));
};

/// Shows `path` in the OS's file manager.
pub(super) use imp::reveal_in_file_manager;

/// Opens the OS's Get Info window for `path`, or `Ok` with nothing to show where it has none.
pub(super) use imp::get_info;

/// Opens `path` in the text editor `app_choice` names, falling back to the OS's default.
pub(super) use imp::open_in_editor;
