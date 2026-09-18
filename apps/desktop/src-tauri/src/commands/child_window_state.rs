//! Tauri commands for the in-session child-window position cache, and the one safe way for a
//! child window to close itself. See `crate::child_window_state` for the position-cache design.

use tauri::{Manager, State};

use crate::child_window_state::{ChildWindowRect, ChildWindowRectStore};

/// How long a child window stays hidden before it's destroyed.
///
/// Destroying a content-heavy webview while a layer-tree commit from its web content process is
/// still in flight makes WebKit's UI-side `RemoteLayerTreeDrawingAreaProxy::commitLayerTree`
/// dereference freed state and take the whole app down with a SIGSEGV. Hiding first pulls the view
/// out of the compositor so no NEW commits are produced, and the wait lets in-flight ones drain.
///
/// Measured with a repro harness driving real Escape keypresses (macOS 26.5.2, 2026-07-23): at 0 ms
/// the app crashed after 36 close cycles; at 100 ms it survived 80 consecutive cycles clean. Treat
/// it as a floor, ❌ never lower it. `docs/notes/child-window-close-webkit-crash.md`.
///
/// On Linux the same delay covers a different problem: destroying the webview on the same GTK
/// main-loop tick that handled the key stalls IPC queued behind it from other webviews.
const WINDOW_CLOSE_DEFER_MS: u64 = 100;

/// Hide a child window, then destroy it a moment later.
///
/// ❗ This runs in RUST rather than in the window's own webview on purpose. WebKit throttles timers
/// in a hidden page to roughly 1 Hz, so a `setTimeout` inside a webview that just hid itself can
/// slip by a second or more, leaving an invisible window alive and still holding a web content
/// process. A Rust timer can't be throttled by the page it's about to close. It also means the
/// windows need no `core:window:allow-hide` in their capabilities, since nothing calls `hide` from
/// JavaScript.
///
/// The destroy is unconditional once scheduled: an early return between the hide and the close
/// would leak exactly the invisible-but-alive window this is meant to prevent.
#[tauri::command]
#[specta::specta]
pub fn close_child_window(app: tauri::AppHandle, label: String) {
    let Some(window) = app.get_webview_window(&label) else {
        // Already gone: a double-close (Escape plus the red button) is ordinary, not a fault.
        log::debug!("Child window close: no window labelled {label}, nothing to close");
        return;
    };

    // macOS only. `orderOut:` is what stops new layer-tree commits, and the crash it defends
    // against is a macOS WebKit one; on Linux the defer alone is the whole fix, and hiding there
    // would be an untested change to how webkit2gtk tears a window down.
    #[cfg(target_os = "macos")]
    if let Err(e) = window.hide() {
        // Not fatal: without the hide we're back to the old behavior, which the delay alone made
        // rare. Closing anyway beats leaving the window up.
        log::warn!("Child window close: couldn't hide {label} before closing: {e}");
    }

    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(WINDOW_CLOSE_DEFER_MS)).await;
        if let Err(e) = window.close() {
            log::warn!("Child window close: couldn't close {label}: {e}");
        }
    });
}

/// Returns the saved rect for a child window label, or `None` if no entry
/// exists (first open in this session, or never opened).
#[tauri::command]
#[specta::specta]
pub fn get_child_window_rect(label: String, store: State<'_, ChildWindowRectStore>) -> Option<ChildWindowRect> {
    store.get(&label)
}

/// Saves the rect for a child window label. Called from the frontend's
/// move/resize listeners on Settings and Debug.
#[tauri::command]
#[specta::specta]
pub fn set_child_window_rect(label: String, rect: ChildWindowRect, store: State<'_, ChildWindowRectStore>) {
    store.set(label, rect);
}
