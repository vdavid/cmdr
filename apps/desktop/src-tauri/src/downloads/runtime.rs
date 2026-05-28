//! Process-wide handle to the running [`super::DownloadsWatcher`].
//!
//! The watcher is FDA-gated: it's alive whenever
//! `crate::fda_gate::is_fda_pending_runtime() == false`. `lib.rs` calls
//! [`refresh_runtime`] at startup and on main-window focus transitions to
//! keep the state aligned. The Settings pane also calls it on mount as a
//! belt-and-braces re-check (the focus event may have fired on a stale gate
//! read).
//!
//! Stored in a `Mutex<Option<...>>` rather than `OnceLock` because the
//! handle's lifetime tracks the FDA gate: it can flip from `None` to `Some`
//! and back over a single session (the deny path keeps the app running and
//! re-enables features as the user grants individual TCC prompts; an allow
//! requires a restart but a mid-session revoke in System Settings is also
//! possible).

use std::sync::Mutex;

use tauri::AppHandle;

use super::{DownloadsWatcher, WatcherError, desired_running};

/// Process-global handle. `None` when the watcher isn't running.
static RUNTIME: Mutex<Option<DownloadsWatcher>> = Mutex::new(None);

/// Start the watcher if the FDA gate is open and we aren't already running;
/// stop it if the gate is closed and we are. Idempotent.
///
/// Returns `Err` only when the watcher couldn't start due to `notify`
/// errors (missing Downloads dir, permission-denied watch attach). Callers
/// log and move on; the next focus event will retry.
pub fn refresh_runtime(app: &AppHandle) -> Result<(), WatcherError> {
    let should_run = desired_running(crate::fda_gate::is_fda_pending_runtime());
    let mut guard = RUNTIME.lock().expect("downloads runtime poisoned");
    match (should_run, guard.is_some()) {
        (true, false) => {
            let watcher = DownloadsWatcher::start(app)?;
            *guard = Some(watcher);
            log::info!(target: "downloads::watcher", "Downloads watcher started (FDA gate open)");
        }
        (false, true) => {
            if let Some(watcher) = guard.take() {
                watcher.stop();
                log::info!(target: "downloads::watcher", "Downloads watcher stopped (FDA gate closed)");
            }
        }
        _ => {
            // Already aligned with desired state; nothing to do.
        }
    }
    Ok(())
}

/// Is the watcher currently running?
pub fn is_running() -> bool {
    RUNTIME.lock().expect("downloads runtime poisoned").is_some()
}

/// Apply a closure to the running watcher, returning its value if the
/// watcher exists. `None` when the watcher is dormant (FDA gate closed or
/// startup hasn't completed).
pub fn with_watcher<R>(f: impl FnOnce(&DownloadsWatcher) -> R) -> Option<R> {
    let guard = RUNTIME.lock().expect("downloads runtime poisoned");
    guard.as_ref().map(f)
}
