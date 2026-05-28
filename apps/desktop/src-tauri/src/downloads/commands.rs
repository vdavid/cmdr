//! Tauri command surface for the downloads watcher.
//!
//! Two commands at this milestone: [`reveal_latest_download`] (the future
//! `⌘J` handler picks this up; for now MCP / dev panels can drive it) and
//! [`downloads_watcher_status`] (FE / debug surface to inspect the running
//! state). A third — [`recheck_downloads_watcher_gate`] — exists so the
//! Settings pane's mount-time belt-and-braces re-check has a typed entry
//! point.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::AppHandle;

use super::watcher::resolved_downloads_dir;

/// Successful reveal: the path to surface plus the pre-split parent dir +
/// file name so the frontend doesn't have to parse the path itself.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RevealedDownload {
    pub path: String,
    pub parent_dir: String,
    pub file_name: String,
}

/// Typed errors returned by [`reveal_latest_download`].
///
/// Tagged enum — `kind` discriminator, no string matching at the call site.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum RevealError {
    /// The watcher hasn't started yet (FDA gate closed, or startup not done).
    /// Frontend should show the "Cmdr needs FDA" toast.
    WatcherUnavailable,
    /// No eligible download exists. Frontend shows the empty-Downloads INFO
    /// toast offering navigation to the Downloads dir.
    Empty,
    /// Downloads dir couldn't be resolved (no `HOME`, no `dirs::download_dir`).
    DownloadsDirUnresolved,
}

impl std::fmt::Display for RevealError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::WatcherUnavailable => write!(f, "Downloads watcher isn't running"),
            Self::Empty => write!(f, "No eligible downloads found"),
            Self::DownloadsDirUnresolved => write!(f, "Couldn't resolve the Downloads directory"),
        }
    }
}

/// Status snapshot for the FE / debug surface.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DownloadsWatcherStatus {
    /// `true` when the watcher is currently active.
    pub running: bool,
    /// Resolved Downloads root. `None` if `dirs::download_dir()` + `$HOME`
    /// fallback both failed.
    pub downloads_dir: Option<String>,
    /// Mirrors `crate::fda_gate::is_fda_pending_runtime()` at call time so
    /// the FE doesn't need a second IPC.
    pub fda_pending: bool,
}

/// Reveal the most recently observed eligible download.
///
/// Tries the ring first; falls back to a recursive Downloads-dir scan when
/// the ring is empty (cold start). Returns a typed [`RevealError`] for the
/// frontend to branch on — no string matching.
#[tauri::command]
#[specta::specta]
pub async fn reveal_latest_download() -> Result<RevealedDownload, RevealError> {
    let from_ring = super::runtime::with_watcher(|w| w.latest_download()).flatten();
    let path = match from_ring {
        Some(p) => Some(p),
        None => {
            // Cold-start fallback: the ring is empty (no event landed yet),
            // so walk the Downloads dir. The walk is bounded by
            // `SCAN_MAX_DEPTH`, but it's still I/O and we don't want the IPC
            // handler thread to block on it for thousands of entries. Move
            // it off via `spawn_blocking`.
            tauri::async_runtime::spawn_blocking(|| {
                super::runtime::with_watcher(|w| w.scan_latest_fallback()).flatten()
            })
            .await
            .unwrap_or_else(|err| {
                log::warn!(
                    target: "downloads::watcher",
                    "scan_latest_fallback spawn_blocking failed: {err}",
                );
                None
            })
        }
    };
    let Some(path) = path else {
        // Distinguish "watcher dormant" from "watcher running but ring +
        // scan turned up nothing." Without the runtime we have no resolved
        // Downloads dir to scan (we wouldn't trust an unguarded scan from
        // command space anyway since that could fire TCC popups).
        if super::runtime::is_running() {
            return Err(RevealError::Empty);
        }
        return Err(RevealError::WatcherUnavailable);
    };

    let parent_dir = PathBuf::from(&path)
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();
    let file_name = PathBuf::from(&path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    Ok(RevealedDownload {
        path: path.to_string_lossy().to_string(),
        parent_dir,
        file_name,
    })
}

/// Read-only snapshot of the watcher's state. Used by debug / MCP surfaces
/// and the Settings pane to render the "watcher is running" indicator.
#[tauri::command]
#[specta::specta]
pub async fn downloads_watcher_status() -> Result<DownloadsWatcherStatus, String> {
    Ok(DownloadsWatcherStatus {
        running: super::runtime::is_running(),
        downloads_dir: resolved_downloads_dir().map(|p| p.to_string_lossy().to_string()),
        fda_pending: crate::fda_gate::is_fda_pending_runtime(),
    })
}

/// Typed error returned by [`recheck_downloads_watcher_gate`]. Only one
/// variant today: starting the underlying `notify` watcher failed (missing
/// Downloads dir, permission denied, OS-level limits). The FE logs and moves
/// on — the next focus event retries.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum WatcherGateError {
    /// `notify-debouncer-full` couldn't attach to the resolved Downloads dir.
    /// `message` carries the underlying error for the log line.
    WatcherStartFailed { message: String },
}

impl std::fmt::Display for WatcherGateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::WatcherStartFailed { message } => write!(f, "Watcher start failed: {message}"),
        }
    }
}

/// Settings-pane belt-and-braces hook. Re-evaluates the FDA gate and
/// starts/stops the watcher accordingly. Idempotent.
///
/// Returns `Err` only if the watcher couldn't start due to a `notify` error;
/// the frontend logs and moves on. Most call sites won't flip the gate
/// state, in which case this is a no-op.
#[tauri::command]
#[specta::specta]
pub async fn recheck_downloads_watcher_gate(app: AppHandle) -> Result<(), WatcherGateError> {
    super::runtime::refresh_runtime(&app).map_err(|e| WatcherGateError::WatcherStartFailed { message: e.to_string() })
}

/// Result of [`set_global_reveal_shortcut`]: the new status the Settings row
/// should display. The FE caches this until the next register/unregister, so
/// the row's "Registered" / "Couldn't register" indicator stays in sync
/// without an extra round trip.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct GlobalRevealShortcutState {
    pub status: super::global_shortcut::RegistrationStatus,
    pub binding: String,
    pub enabled: bool,
}

/// Apply a Settings change (toggle + binding) to the live global-shortcut
/// registration. Idempotent; safe to call repeatedly with the same args.
///
/// Returns the resulting status so the FE row can render the indicator
/// without another round trip. Errors are wrapped in the typed
/// [`super::global_shortcut::RegistrationError`] enum.
#[tauri::command]
#[specta::specta]
pub async fn set_global_reveal_shortcut(
    app: AppHandle,
    enabled: bool,
    binding: String,
) -> Result<GlobalRevealShortcutState, super::global_shortcut::RegistrationError> {
    super::runtime::apply_global_reveal_shortcut(&app, enabled, &binding)
}

#[cfg(test)]
mod tests {
    //! Tests for the `reveal_latest_download` branches. The process-global
    //! `runtime::RUNTIME` is shared across the crate; serialize through the
    //! same `install_lock` the runtime module uses, drained via a private
    //! `with_clean_runtime` helper so a panicking test can't leave a
    //! lingering watcher behind.
    use super::*;
    use crate::downloads::DownloadsWatcher;
    use crate::downloads::runtime::install_for_test;
    use crate::downloads::watcher::{DownloadDetectedEvent, EventSink};
    use std::sync::Arc;
    use std::sync::Mutex as StdMutex;
    use std::sync::OnceLock;
    use std::sync::mpsc;

    fn install_lock() -> &'static StdMutex<()> {
        static LOCK: OnceLock<StdMutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| StdMutex::new(()))
    }

    struct ChannelSink(mpsc::Sender<DownloadDetectedEvent>);
    impl EventSink for ChannelSink {
        fn emit(&self, event: DownloadDetectedEvent) {
            let _ = self.0.send(event);
        }
    }

    fn unhidden_tempdir() -> tempfile::TempDir {
        tempfile::Builder::new()
            .prefix("cmdr-reveal-test-")
            .tempdir()
            .expect("tempdir")
    }

    /// Build a single-threaded tokio runtime per test. Wrapping the `await` in
    /// `block_on` lets us hold the std `Mutex` install-lock across the
    /// reveal future without tripping `clippy::await_holding_lock` (the lock
    /// stays on the sync caller stack; the runtime only drives the future).
    /// The existing `runtime` module's tests use the same shape (plain
    /// `#[test]`); matching the pattern keeps the install-serialization
    /// guard consistent across the crate.
    fn block_on<F: Future>(fut: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tokio runtime")
            .block_on(fut)
    }

    #[test]
    fn reveal_returns_watcher_unavailable_when_runtime_is_dormant() {
        // No watcher installed → distinguish "dormant" from "running but
        // empty." The FE branches on this to show the FDA-required toast
        // instead of the empty-Downloads toast.
        let _lock = install_lock().lock().unwrap_or_else(|p| p.into_inner());
        assert!(
            !super::super::runtime::is_running(),
            "precondition: no watcher installed"
        );

        let err = block_on(reveal_latest_download()).expect_err("expected error");
        assert!(matches!(err, RevealError::WatcherUnavailable), "got {err:?}");
    }

    #[test]
    fn reveal_returns_empty_when_ring_and_scan_both_turn_up_nothing() {
        // Watcher running, no events have arrived, Downloads dir is empty.
        // This is the cold-start "you just launched Cmdr, no downloads yet"
        // path — distinct from `WatcherUnavailable` so the FE can show the
        // empty-Downloads toast (with a "Go to Downloads anyway?" action)
        // instead of the FDA toast.
        let _lock = install_lock().lock().unwrap_or_else(|p| p.into_inner());

        let tempdir = unhidden_tempdir();
        let (tx, _rx) = mpsc::channel::<DownloadDetectedEvent>();
        let sink: Arc<dyn EventSink> = Arc::new(ChannelSink(tx));
        let watcher = DownloadsWatcher::start_at(tempdir.path().to_path_buf(), sink).expect("watcher start");
        let _guard = install_for_test(watcher);

        let err = block_on(reveal_latest_download()).expect_err("expected error");
        assert!(matches!(err, RevealError::Empty), "got {err:?}");
    }

    #[test]
    fn reveal_returns_revealed_download_from_scan_fallback_when_ring_is_empty() {
        // The scan fallback finds an eligible file even though the ring is
        // empty. The returned shape splits the path into `parent_dir` and
        // `file_name` so the FE doesn't have to parse paths.
        let _lock = install_lock().lock().unwrap_or_else(|p| p.into_inner());

        let tempdir = unhidden_tempdir();
        let canonical_root = std::fs::canonicalize(tempdir.path()).expect("canonicalize");
        let file_path = canonical_root.join("downloaded-from-cli.bin");
        std::fs::write(&file_path, b"hi").expect("write");

        let (tx, _rx) = mpsc::channel::<DownloadDetectedEvent>();
        let sink: Arc<dyn EventSink> = Arc::new(ChannelSink(tx));
        let watcher = DownloadsWatcher::start_at(canonical_root.clone(), sink).expect("watcher start");
        let _guard = install_for_test(watcher);

        let revealed = block_on(reveal_latest_download()).expect("expected Ok");
        assert_eq!(revealed.path, file_path.to_string_lossy());
        assert_eq!(revealed.parent_dir, canonical_root.to_string_lossy());
        assert_eq!(revealed.file_name, "downloaded-from-cli.bin");
    }
}
