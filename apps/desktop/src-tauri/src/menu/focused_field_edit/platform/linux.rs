use tauri::{AppHandle, Manager, Runtime};
use webkit2gtk::WebViewExt;

use super::NativeEditSelector;

/// WebKitGTK's own editing commands (`WEBKIT_EDITING_COMMAND_*`), run in the focused window's
/// view: they act on its focused field, as the key would have.
pub(crate) fn run<R: Runtime>(app: &AppHandle<R>, selector: NativeEditSelector) {
    let command = match selector {
        NativeEditSelector::Cut => "Cut",
        NativeEditSelector::Copy => "Copy",
        NativeEditSelector::Paste => "Paste",
        NativeEditSelector::SelectAll => "SelectAll",
    };
    let Some(window) = app
        .webview_windows()
        .into_values()
        .find(|window| window.is_focused().unwrap_or(false))
    else {
        return;
    };
    if let Err(error) = window.with_webview(move |webview| webview.inner().execute_editing_command(command)) {
        log::warn!(
            target: "menu",
            "Couldn't run the {command} edit in window {}: {error}",
            // allowed-log-private: a window label is Cmdr's own id (`settings`, `queue`), never the user's.
            window.label()
        );
    }
}
