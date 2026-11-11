//! How each OS reaches its clipboard. A platform seam (`docs/guides/platform-seams.md`); the
//! commands above it (`commands/clipboard.rs`) and the cut state are portable.

use std::future::Future;
use std::path::PathBuf;

use cmdr_operations::MutationError;
use tauri::AppHandle;

use super::PasteClipboardReply;

#[cfg_attr(target_os = "macos", path = "macos.rs")]
#[cfg_attr(target_os = "linux", path = "linux.rs")]
#[cfg_attr(target_os = "windows", path = "windows.rs")]
mod imp;

#[cfg(any(target_os = "linux", target_os = "windows"))]
mod unsupported;

const _: fn(AppHandle, Vec<PathBuf>) -> Result<(), String> = write_file_urls;
const _: fn(AppHandle) -> Result<Vec<PathBuf>, String> = read_file_urls;
const _: fn(AppHandle) -> Result<Option<String>, String> = read_text;
// An `async fn` doesn't coerce to a fn pointer, so this pin names its arguments and the output its
// future resolves to.
const _: fn(AppHandle) = |app| {
    fn resolves_to<T, F: Future<Output = T>>(_: F) {}
    resolves_to::<Result<PasteClipboardReply, MutationError>, _>(paste_as_file(app, None, String::new()));
};

/// Puts `paths` on the clipboard as files, and as their newline-joined text. Blocks until the
/// OS answers.
pub(crate) use imp::write_file_urls;

/// The files on the clipboard. Blocks until the OS answers.
pub(crate) use imp::read_file_urls;

/// The plain text on the clipboard, or `None` when there's none. Blocks until the OS answers.
pub(crate) use imp::read_text;

/// Writes the clipboard's highest-intent non-file content (image, PDF, text) into `directory`
/// as a new `pasted.<ext>` file, under the instant-mutation reply deadline.
pub(crate) use imp::paste_as_file;
