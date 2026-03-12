//! Tauri event payloads and response types for the indexing system.

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

use super::store::IndexStatus;

// ── Event payloads (Rust -> Frontend) ────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexScanStartedEvent {
    pub volume_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexScanProgressEvent {
    pub volume_id: String,
    pub entries_scanned: u64,
    pub dirs_found: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexScanCompleteEvent {
    pub volume_id: String,
    pub total_entries: u64,
    pub total_dirs: u64,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexDirUpdatedEvent {
    pub paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexReplayProgressEvent {
    pub volume_id: String,
    pub events_processed: u64,
    pub estimated_total: Option<u64>,
}

/// Why a full rescan was triggered instead of incremental replay.
/// Sent to the frontend as `index-rescan-notification` so the UI can show
/// a transparent, user-friendly toast.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RescanReason {
    /// Event ID gap too large — app hasn't run for a long time.
    StaleIndex,
    /// FSEvents journal unavailable (gap detected during replay).
    JournalGap,
    /// Replay processed too many events (safety limit exceeded).
    ReplayOverflow,
    /// Too many MustScanSubDirs events during replay.
    TooManySubdirRescans,
    /// DriveWatcher failed to start for replay.
    WatcherStartFailed,
    /// Reconciler event buffer overflowed during scan.
    ReconcilerBufferOverflow,
    /// Previous scan didn't complete (app crashed or was force-quit).
    IncompletePreviousScan,
    /// FSEvents channel overflowed — events were dropped.
    WatcherChannelOverflow,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexRescanNotificationEvent {
    pub volume_id: String,
    pub reason: RescanReason,
    /// Human-readable details for logs (not shown to user directly).
    pub details: String,
}

/// Emit an `index-rescan-notification` event and log the reason at INFO level.
pub(super) fn emit_rescan_notification(app: &AppHandle, volume_id: &str, reason: RescanReason, details: String) {
    log::info!("Index rescan triggered ({reason:?}): {details}");
    let _ = app.emit(
        "index-rescan-notification",
        IndexRescanNotificationEvent {
            volume_id: volume_id.to_string(),
            reason,
            details,
        },
    );
}

// ── Response types ───────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexStatusResponse {
    pub initialized: bool,
    pub scanning: bool,
    pub entries_scanned: u64,
    pub dirs_found: u64,
    pub index_status: Option<IndexStatus>,
    pub db_file_size: Option<u64>,
}

/// Extended debug status for the debug window. Includes live DB counts
/// and MustScanSubDirs tracking.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexDebugStatusResponse {
    /// Base status (same as `get_index_status`)
    #[serde(flatten)]
    pub base: IndexStatusResponse,
    /// Whether the filesystem watcher is active
    pub watcher_active: bool,
    /// Total live FS events received since indexing started
    pub live_event_count: u64,
    /// Total MustScanSubDirs events received
    pub must_scan_count: u64,
    /// Total MustScanSubDirs rescans completed
    pub must_scan_rescans_completed: u64,
    /// Live entry count from the DB
    pub live_entry_count: Option<u64>,
    /// Live directory count from the DB
    pub live_dir_count: Option<u64>,
    /// Directories that have dir_stats rows
    pub dirs_with_stats: Option<u64>,
    /// Recent MustScanSubDirs paths: (timestamp, path)
    pub recent_must_scan_paths: Vec<(String, String)>,
}
