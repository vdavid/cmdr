//! Running a Custom Edit item's text action (cut, copy, paste, select all) in the focused field of
//! a window other than the main one.
//!
//! The main bar's Edit items are Custom (`menu_handlers.rs` says why), so outside the main window
//! nothing does the text edit for them: the bar takes the accelerator before the page sees the
//! key, and the item's handler is the key's only road. macOS sends the selector down the responder
//! chain; Linux asks the focused window's WebKitGTK view to run the editing command (verified with
//! real X keys in the Linux lane, `menu-mnemonic-mask.spec.ts`: without it, `Ctrl+X` in a Settings
//! field reached no keydown and cut nothing, 2026-11-12). Windows has no bar yet (task 63).

mod platform;

use super::menu_handlers::NativeEditSelector;
use tauri::{AppHandle, Runtime};

/// Runs `selector` in whichever window has focus, in its focused text field.
pub(crate) fn run_in_focused_window<R: Runtime>(app: &AppHandle<R>, selector: NativeEditSelector) {
    platform::run(app, selector);
}
