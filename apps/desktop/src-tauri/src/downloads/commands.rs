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

use super::watcher::{DownloadDetectedEvent, resolved_downloads_dir};

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
    /// Carries the [`DownloadDetectedEvent`] type into the type graph so its
    /// shape lands in `bindings.ts`. Always `None` today; reserved for a
    /// future "last detected" surface (M5 territory).
    pub last_detected: Option<DownloadDetectedEvent>,
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
    let fallback = || super::runtime::with_watcher(|w| w.scan_latest_fallback()).flatten();
    let path = from_ring.or_else(fallback);
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
        last_detected: None,
    })
}

/// Settings-pane belt-and-braces hook. Re-evaluates the FDA gate and
/// starts/stops the watcher accordingly. Idempotent.
///
/// Returns `Err(String)` only if the watcher couldn't start due to a
/// `notify` error; the frontend logs and moves on. Most call sites won't
/// flip the gate state, in which case this is a no-op.
#[tauri::command]
#[specta::specta]
pub async fn recheck_downloads_watcher_gate(app: AppHandle) -> Result<(), String> {
    super::runtime::refresh_runtime(&app).map_err(|e| e.to_string())
}
