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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexReplayCompleteEvent {
    pub volume_id: String,
    pub duration_ms: u64,
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

// ── Activity phase tracking ──────────────────────────────────────────

/// What the indexer is currently doing. More granular than `IndexPhase`
/// (which tracks lifecycle: Disabled/Initializing/Running/ShuttingDown).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivityPhase {
    /// Processing FSEvents journal replay on cold start.
    Replaying,
    /// Full volume scan in progress.
    Scanning,
    /// Computing directory size aggregates after scan.
    Aggregating,
    /// Replaying buffered watcher events after scan.
    Reconciling,
    /// Processing live filesystem events in real time.
    Live,
    /// Idle — indexing initialized but no active work.
    Idle,
}

impl std::fmt::Display for ActivityPhase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Replaying => write!(f, "Replaying"),
            Self::Scanning => write!(f, "Scanning"),
            Self::Aggregating => write!(f, "Aggregating"),
            Self::Reconciling => write!(f, "Reconciling"),
            Self::Live => write!(f, "Live"),
            Self::Idle => write!(f, "Idle"),
        }
    }
}

/// A completed or in-progress phase in the indexing timeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhaseRecord {
    pub phase: ActivityPhase,
    /// HH:MM:SS.mmm format
    pub started_at: String,
    /// None = still in progress
    pub duration_ms: Option<u64>,
    /// Why we entered this phase (for example, "app launch, 7,284 pending FSEvents")
    pub trigger: String,
    /// Phase-specific stats: flat key-value pairs.
    /// For example, {"raw_events": "7284", "unique_events": "3836", "dedup_pct": "47"}
    pub stats: Vec<(String, String)>,
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
    /// Current activity phase
    pub activity_phase: ActivityPhase,
    /// When the current phase started (HH:MM:SS.mmm)
    pub phase_started_at: String,
    /// How long the current phase has been running (ms)
    pub phase_duration_ms: u64,
    /// Timeline of past and current phases
    pub phase_history: Vec<PhaseRecord>,
    /// Whether background verification is running concurrently with the current phase
    pub verifying: bool,
    /// Main DB file size (bytes), excluding WAL/SHM
    pub db_main_size: Option<u64>,
    /// WAL file size (bytes)
    pub db_wal_size: Option<u64>,
    /// Total SQLite pages allocated
    pub db_page_count: Option<u64>,
    /// SQLite freelist pages (unused space)
    pub db_freelist_count: Option<u64>,
}
