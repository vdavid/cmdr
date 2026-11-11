//! How a child window (Settings, the file viewer, Debug, Keyboard shortcuts, the operation queue)
//! appears and goes away. On macOS it slides in once its page is ready and slides back out on every
//! close, unless the user has Reduce motion on. Every close, however it was asked for, then hides
//! the window and destroys it on a delay that keeps WebKit's teardown safe. `CLAUDE.md` and
//! `DETAILS.md` here.

#[cfg(target_os = "macos")]
mod appkit;
mod ledger;

use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use tauri::{AppHandle, Manager, WebviewWindow};

use crate::ignore_poison::IgnorePoison;
use ledger::{Cancelled, Ledger};

/// How far a child window travels as it slides in or out, in points.
#[cfg(target_os = "macos")]
const SLIDE_DISTANCE_PT: f64 = 24.0;

/// How long a slide in or out takes.
const SLIDE_DURATION: Duration = Duration::from_millis(220);

/// How long a child window stays hidden before it's destroyed.
///
/// Destroying a content-heavy webview while a layer-tree commit from its web content process is
/// still in flight makes WebKit's UI-side `RemoteLayerTreeDrawingAreaProxy::commitLayerTree`
/// dereference freed state and take the whole app down with a SIGSEGV. Hiding first pulls the view
/// out of the compositor so no NEW commits are produced, and the wait lets in-flight ones drain.
///
/// Measured with a repro harness driving real Escape keypresses (macOS 26.5.2, 2026-07-23): at 0 ms
/// the app crashed after 36 close cycles; at 100 ms it survived 80 consecutive cycles clean. Treat
/// it as a floor, ❌ never lower it, and the slide-out runs BEFORE it, never inside it.
/// `docs/notes/child-window-close-webkit-crash.md`.
///
/// On Linux the same delay covers a different problem: destroying the webview on the same GTK
/// main-loop tick that handled the close stalls IPC queued behind it from other webviews.
const HIDE_TO_DESTROY: Duration = Duration::from_millis(100);

static LEDGER: LazyLock<Mutex<Ledger>> = LazyLock::new(|| Mutex::new(Ledger::default()));

fn ledger() -> std::sync::MutexGuard<'static, Ledger> {
    LEDGER.lock_ignore_poison()
}

/// Whether this module owns the window's show and close: every window but the main one, whose
/// close quits the app instead (`app_lifecycle.rs`).
pub fn is_child_window(label: &str) -> bool {
    label != "main"
}

/// Shows a child window that was created hidden, sliding it in on macOS. Its page calls this once
/// it has rendered; the opener calls it again after a delay as a fallback. Does nothing to a window
/// that's already on screen (so the fallback can't take the front back from wherever the user went
/// meanwhile), minimized, or closing.
pub fn show(app: &AppHandle, label: String) {
    #[cfg(target_os = "macos")]
    {
        // The raw NSWindow pointer isn't `Send`, so resolve the window and obtain it INSIDE the
        // main-thread closure.
        let app_for_main = app.clone();
        let hopped = app.run_on_main_thread(move || {
            let Some(window) = app_for_main.get_webview_window(&label) else {
                // allowed-log-private: a Tauri window label of our own
                log::debug!(target: "ui", "Child window show: no window with label {label}");
                return;
            };
            if ledger().is_closing(&label) {
                return;
            }
            match appkit::show(&window) {
                Ok(appkit::Shown::Slid) => {
                    ledger().record_slide_in(&label, ledger::settles_at(Instant::now(), SLIDE_DURATION));
                }
                Ok(appkit::Shown::Plain | appkit::Shown::AlreadyShowing) => {}
                Err(e) => log::warn!(target: "ui", "Child window show: {e}"),
            }
        });
        if let Err(e) = hopped {
            log::warn!(target: "ui", "Child window show: couldn't reach the main thread: {e}");
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let Some(window) = app.get_webview_window(&label) else {
            // allowed-log-private: a Tauri window label of our own
            log::debug!(target: "ui", "Child window show: no window with label {label}");
            return;
        };
        if ledger().is_closing(&label) {
            return;
        }
        if let Err(e) = window.show() {
            log::warn!(target: "ui", "Child window show: {e}");
        }
    }
}

/// Closes a child window: slides it out if it slid in, hides it, and destroys it
/// `HIDE_TO_DESTROY` later.
///
/// ❗ Every close of a child window lands here: Escape and the in-page close buttons through the
/// `close_child_window` command, and the red button, ⌘W, the Window menu, and any `close()` call
/// (Rust or JS, MCP included) through the `CloseRequested` handler in `app_lifecycle.rs`, which
/// prevents the default close and calls this. That's also why the final step is `destroy()`, which
/// doesn't emit `CloseRequested` again.
///
/// ❗ The timers run in RUST on purpose. WebKit throttles timers in a hidden page to roughly 1 Hz,
/// so a `setTimeout` inside a webview that just hid itself can slip by a second or more, leaving an
/// invisible window alive and still holding a web content process.
///
/// Only a reopen (`cancel_close`, which shows the window again) may stop a scheduled destroy. Any
/// other early return between the hide and the destroy would leak exactly the
/// invisible-but-alive window this is meant to prevent.
pub fn close(app: &AppHandle, label: &str) {
    let Some(window) = app.get_webview_window(label) else {
        // Already gone: a double close (Escape plus the red button) is ordinary, not a fault.
        // allowed-log-private: a Tauri window label of our own
        log::debug!(target: "ui", "Child window close: no window labelled {label}, nothing to close");
        return;
    };
    let Some(close) = ledger().begin_close(label) else {
        // allowed-log-private: a Tauri window label of our own
        log::debug!(target: "ui", "Child window close: {label} is already closing");
        return;
    };

    // macOS only. `orderOut:` is what stops new layer-tree commits, and the crash it defends
    // against is a macOS WebKit one; on Linux the defer alone is the whole fix, and hiding there
    // would be an untested change to how webkit2gtk tears a window down.
    #[cfg(target_os = "macos")]
    {
        let window_for_main = window.clone();
        let label_for_main = label.to_owned();
        let hopped = window.run_on_main_thread(move || {
            let rest = if close.slid_in {
                appkit::start_slide_out(&window_for_main).unwrap_or_else(|e| {
                    log::warn!(target: "ui", "Child window close: no slide-out: {e}");
                    None
                })
            } else {
                None
            };
            match rest {
                Some(rest) => ledger().record_slide_out(&label_for_main, close.ticket, rest),
                None => hide(&window_for_main),
            }
            tauri::async_runtime::spawn(finish_close(window_for_main, close.ticket, rest.is_some()));
        });
        if let Err(e) = hopped {
            // Not fatal: without the hide we're back to the delay alone, which made the crash
            // rare. Closing anyway beats leaving the window up.
            // allowed-log-private: a Tauri window label of our own
            log::warn!(target: "ui", "Child window close: couldn't reach the main thread for {label}: {e}");
            tauri::async_runtime::spawn(finish_close(window, close.ticket, false));
        }
    }
    #[cfg(not(target_os = "macos"))]
    tauri::async_runtime::spawn(finish_close(window, close.ticket, false));
}

/// The rest of a close: let a slide-out land, hide, wait out `HIDE_TO_DESTROY`, destroy. Each step
/// first checks the close wasn't cancelled by a reopen meanwhile.
async fn finish_close(window: WebviewWindow, ticket: u64, slid_out: bool) {
    let label = window.label().to_owned();
    if slid_out {
        tokio::time::sleep(SLIDE_DURATION + ledger::SETTLE_MARGIN).await;
        if !ledger().close_is_current(&label, ticket) {
            return;
        }
        // Only macOS slides, and only macOS hides (see `close`).
        #[cfg(target_os = "macos")]
        hide(&window);
    }
    tokio::time::sleep(HIDE_TO_DESTROY).await;
    if !ledger().take_close(&label, ticket) {
        return;
    }
    if let Err(e) = window.destroy() {
        // allowed-log-private: a Tauri window label of our own
        log::warn!(target: "ui", "Child window close: couldn't destroy {label}: {e}");
    }
}

#[cfg(target_os = "macos")]
fn hide(window: &WebviewWindow) {
    if let Err(e) = window.hide() {
        // Not fatal: without the hide we're back to the delay alone, which made the crash rare.
        log::warn!(target: "ui", "Child window close: couldn't hide a window before destroying it: {e}");
    }
}

/// A reopen of a window that's mid-close: stops the close and, on macOS, slides the window back to
/// where it stood. The caller then brings the window to the front as usual. MUST run on the AppKit
/// main thread on macOS (`raise_child_window` calls it from its main-thread closure).
pub fn cancel_close(window: &WebviewWindow) {
    let cancelled = ledger().cancel_close(window.label(), ledger::settles_at(Instant::now(), SLIDE_DURATION));
    match cancelled {
        Cancelled::NothingToCancel => {}
        Cancelled::Plain => {
            // allowed-log-private: a Tauri window label of our own
            log::debug!(target: "ui", "Child window close of {} cancelled by a reopen", window.label());
        }
        Cancelled::SlideBack { rest } => {
            // allowed-log-private: a Tauri window label of our own
            log::debug!(target: "ui", "Child window close of {} cancelled by a reopen, sliding back", window.label());
            #[cfg(target_os = "macos")]
            if let Err(e) = appkit::slide_back(window, rest) {
                log::warn!(target: "ui", "Child window close: couldn't slide back: {e}");
            }
            #[cfg(not(target_os = "macos"))]
            let _ = rest;
        }
    }
}

/// The window is destroyed, however it went (`app_lifecycle.rs` calls this on `Destroyed`).
pub fn forget(label: &str) {
    ledger().forget(label);
}

/// Whether a rect a child window's page reports now is one to remember. Mid-slide it isn't: the
/// slide moves the window, and saving that would make every reopen land a little higher.
pub fn accepts_rect_write(label: &str) -> bool {
    ledger().accepts_rect_write(label, Instant::now())
}
