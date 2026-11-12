use tauri::{AppHandle, Runtime};

use super::NativeEditSelector;

/// The responder chain finds the key window's first responder, so the window needs no lookup. The
/// menu handler runs on the main thread, as AppKit requires.
pub(crate) fn run<R: Runtime>(_app: &AppHandle<R>, selector: NativeEditSelector) {
    crate::menu::menu_handlers::send_native_edit_selector(selector);
}
