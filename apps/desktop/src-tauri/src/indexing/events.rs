//! Tauri event payloads and response types for the indexing system.

use std::sync::LazyLock;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use tauri_specta::Event;

use super::store::{IndexFailure, IndexStatus};

// ── Event payloads (Rust -> Frontend) ────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[tauri_specta(event_name = "index-scan-started")]
#[serde(rename_all = "camelCase")]
pub struct IndexScanStartedEvent {
    pub volume_id: String,
    /// The previous completed scan's final entry count, the tier-1 (calibrated)
    /// progress denominator. `None` on a first-ever scan (no prior calibration).
    pub prior_total_entries: Option<u64>,
    /// The previous completed scan's wall-clock duration, used to seed the tier-1
    /// ETA before the sliding window has samples. `None` on a first-ever scan.
    pub prior_scan_duration_ms: Option<u64>,
    /// The scanned volume's used bytes at scan start, the tier-2 (rough, first-scan)
    /// progress denominator. `None` when the space-info fetch failed.
    pub volume_used_bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[tauri_specta(event_name = "index-scan-progress")]
#[serde(rename_all = "camelCase")]
pub struct IndexScanProgressEvent {
    pub volume_id: String,
    pub entries_scanned: u64,
    pub dirs_found: u64,
    /// Resolved post-dedup physical bytes scanned so far, the tier-2 progress
    /// numerator (apples-to-apples with `volume_used_bytes`).
    pub bytes_scanned: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[tauri_specta(event_name = "index-scan-complete")]
#[serde(rename_all = "camelCase")]
pub struct IndexScanCompleteEvent {
    pub volume_id: String,
    pub total_entries: u64,
    pub total_dirs: u64,
    pub duration_ms: u64,
}

/// Emitted when a scan ends WITHOUT completing: a network (SMB/MTP) scan that
/// disconnected, was canceled, timed out, or otherwise aborted. Unlike
/// `index-scan-complete`, this writes no completion facts (the partial isn't a
/// finished index) — it exists purely so the frontend clears the volume's live
/// activity, so an aborted scan doesn't leave a stuck "scanning" row in the
/// corner indicator or the breadcrumb badge tooltip. Carries the `volume_id` so
/// only the aborted volume's activity is cleared.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[tauri_specta(event_name = "index-scan-aborted")]
#[serde(rename_all = "camelCase")]
pub struct IndexScanAbortedEvent {
    pub volume_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[tauri_specta(event_name = "index-dir-updated")]
#[serde(rename_all = "camelCase")]
pub struct IndexDirUpdatedEvent {
    pub paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[tauri_specta(event_name = "index-replay-progress")]
#[serde(rename_all = "camelCase")]
pub struct IndexReplayProgressEvent {
    pub volume_id: String,
    pub events_processed: u64,
    pub estimated_total: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[tauri_specta(event_name = "index-replay-complete")]
#[serde(rename_all = "camelCase")]
pub struct IndexReplayCompleteEvent {
    pub volume_id: String,
    pub duration_ms: u64,
}

/// Why a full rescan was triggered instead of incremental replay.
/// Sent to the frontend as `index-rescan-notification` so the UI can show
/// a transparent, user-friendly toast.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum RescanReason {
    /// Event ID gap too large: app hasn't run for a long time.
    StaleIndex,
    /// FSEvents journal unavailable (gap detected during replay).
    JournalGap,
    /// Replay processed too many events (safety limit exceeded).
    ReplayOverflow,
    /// DriveWatcher failed to start for replay.
    WatcherStartFailed,
    /// Reconciler event buffer overflowed during scan.
    ReconcilerBufferOverflow,
    /// Previous scan didn't complete (app crashed or was force-quit).
    IncompletePreviousScan,
    /// FSEvents channel overflowed: events were dropped.
    WatcherChannelOverflow,
    /// The unbounded ingestion queue grew past the RAM-guard hard cap: the event
    /// loop is hopelessly behind, so we deliberately fall back to a full scan (our
    /// decision, not a dropped-events overflow). See `event_loop::INGESTION_HARD_CAP`.
    IngestionBacklog,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[tauri_specta(event_name = "index-rescan-notification")]
#[serde(rename_all = "camelCase")]
pub struct IndexRescanNotificationEvent {
    pub volume_id: String,
    pub reason: RescanReason,
    /// Human-readable details for logs (not shown to user directly).
    pub details: String,
}

/// Emitted when a full-scan aggregation pass finishes and the UI can dismiss the
/// progress overlay. Carries the `volume_id` so the FE clears the right drive's
/// aggregation row (two volumes can aggregate concurrently).
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[tauri_specta(event_name = "index-aggregation-complete")]
#[serde(rename_all = "camelCase")]
pub struct IndexAggregationCompleteEvent {
    pub volume_id: String,
}

/// Emitted when the memory watchdog stops indexing to avoid a system crash.
/// Drives a user-visible toast.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[tauri_specta(event_name = "index-memory-warning")]
#[serde(rename_all = "camelCase")]
pub struct IndexMemoryWarningEvent {
    /// Resident set size (RSS) at the time, in GB. Over-counts GPU/WebView
    /// graphics memory, so it's kept for context but is NOT the trigger metric.
    pub resident_gb: u64,
    /// `phys_footprint` at the time, in GB — the machine-pressure metric the
    /// watchdog thresholds key on (what Activity Monitor shows, what jetsam
    /// watches). A large `resident_gb - phys_footprint_gb` gap is graphics
    /// memory, not the indexing heap.
    pub phys_footprint_gb: u64,
    /// The real Rust/C malloc heap in use, in MB — indexing's actual footprint.
    /// Tiny next to a multi-GB `resident_gb` means the spike isn't indexing.
    pub heap_mb: u64,
    /// What the watchdog did. Currently always `"stopped_indexing"`.
    pub action: String,
}

/// Emitted when a volume's freshness changes to a NEW value (blue/green/yellow
/// transitions). Drives the per-drive freshness UX: the always-visible badge
/// refreshes, and the FE's one-time stale dialog (D2) fires on the exact
/// Fresh→Stale edge.
/// Emitted from `state::apply_freshness_event` only when the value actually
/// changes, so the FE can subscribe rather than poll.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[tauri_specta(event_name = "index-freshness-changed")]
#[serde(rename_all = "camelCase")]
pub struct IndexFreshnessChangedEvent {
    pub volume_id: String,
    pub freshness: super::freshness::Freshness,
}

/// Emit an `index-rescan-notification` event and log the reason at INFO level.
pub(super) fn emit_rescan_notification(app: &AppHandle, volume_id: &str, reason: RescanReason, details: String) {
    log::info!("Index rescan triggered ({reason:?}): {details}");
    let _ = IndexRescanNotificationEvent {
        volume_id: volume_id.to_string(),
        reason,
        details,
    }
    .emit(app);
}

// ── Activity phase tracking ──────────────────────────────────────────

/// What the indexer is currently doing. More granular than `IndexPhase`
/// (which tracks lifecycle: Disabled/Initializing/Running/ShuttingDown).
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
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
    /// Idle: indexing initialized but no active work.
    Idle,
    /// Stopped after a fatal storage error: the DB is unusable, so the writer,
    /// watcher, and event loop are torn down and the volume sits in the `Failed`
    /// phase until the user rebuilds it. The terminal, unhappy sibling of `Idle`.
    Failed,
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
            Self::Failed => write!(f, "Failed"),
        }
    }
}

/// Emitted when a volume's top-level indexing phase changes (a step in the
/// `Scanning → Aggregating → Reconciling → Live` pipeline, plus `Replaying` and
/// `Idle`).
///
/// This is the PER-VOLUME counterpart to the global `DEBUG_STATS` phase timeline.
/// `DEBUG_STATS.set_phase` records ONE app-wide journal for the debug window,
/// which can't attribute a phase to a drive when two volumes index at once. This
/// event carries the `volumeId`, so the frontend's per-volume step checklist can
/// advance the right drive's steps. It's fired ALONGSIDE every `set_phase` call
/// where a `volumeId` is in scope (via [`set_phase_for`]), never replacing the
/// global record.
///
/// It fires only on TRANSITIONS, so a frontend that joins mid-scan (a window
/// reload) can't learn the current phase from it. The FE backfills the observable
/// steps from the scan/aggregation activity instead; the reconcile step is the one
/// transition with no other signal, so it's briefly unobservable after a reload
/// that lands mid-reconcile (an accepted, rare gap — see the frontend
/// `indexing/DETAILS.md`).
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[tauri_specta(event_name = "index-phase-changed")]
#[serde(rename_all = "camelCase")]
pub struct IndexPhaseChangedEvent {
    pub volume_id: String,
    pub phase: ActivityPhase,
}

/// A completed or in-progress phase in the indexing timeline.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
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

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct IndexStatusResponse {
    pub initialized: bool,
    pub scanning: bool,
    pub entries_scanned: u64,
    pub dirs_found: u64,
    /// Resolved post-dedup physical bytes scanned so far (live), the tier-2
    /// progress numerator. 0 when no scan is running. Rides the same
    /// `scan_handle` snapshot as `entries_scanned`/`dirs_found`.
    pub bytes_scanned: u64,
    pub index_status: Option<IndexStatus>,
    pub db_file_size: Option<u64>,
    /// The scanned volume's used bytes at the current scan's start, the tier-2
    /// (first-scan) progress denominator. Sourced from the stashed calibration,
    /// so it's present only while a scan is running (and only when the space-info
    /// fetch succeeded). Lets the FE backfill tier-2 progress after a mid-scan
    /// window reload, where the `index-scan-started` event was missed.
    pub volume_used_bytes: Option<u64>,
}

/// Per-volume index status for the per-drive freshness badge.
///
/// Unlike [`IndexStatusResponse`] (the local-disk scan-progress shape the debug
/// window and scan overlay consume), this is the *per-volume* status the badge
/// renders for every drive, local included: the freshness color plus the
/// last-completed-scan facts the tooltip/menu footer show. `enabled: false`
/// with `freshness: None` is the gray / not-indexed state (no registered index
/// for the volume); a registered index always carries a `freshness`.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct VolumeIndexStatus {
    /// The volume this status describes (`"root"`, `smb-…`, `mtp-…`).
    pub volume_id: String,
    /// Whether an index is registered (and thus being kept live) for this
    /// volume. `false` ⇒ gray / not-indexed.
    pub enabled: bool,
    /// The volume's freshness (gray = `None`/disabled; blue = `scanning`; green
    /// = `fresh`; yellow = `stale`; red = `failed`). Always `Some` when `enabled`,
    /// and `Some(Failed)` for a dead index even though `enabled` is `false` (the
    /// instance stays registered in the `Failed` phase so the badge is honest).
    pub freshness: Option<super::freshness::Freshness>,
    /// The typed fatal-storage reason, present ONLY when `freshness == Failed`.
    /// Carries the SQLite result codes so logs and any future detailed tooltip can
    /// be specific; the badge itself branches on `freshness`, not this.
    pub failure: Option<IndexFailure>,
    /// Unix seconds of the last completed scan, for the "Last indexed: …"
    /// tooltip/footer. From `meta.scan_completed_at`; `None` if none completed.
    pub scan_completed_at: Option<u64>,
    /// The last completed scan's wall-clock duration, for "… took N min, S s".
    /// From `meta.scan_duration_ms`; `None` if no scan has completed.
    pub scan_duration_ms: Option<u64>,
}

/// Extended debug status for the debug window. Includes live DB counts
/// and MustScanSubDirs tracking.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
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
    /// Directory listings seen at or over [`HUGE_DIR_CHILD_FLOOR`] children, across
    /// the guarded walker and the LOCAL reconcile walk (the pathological-directory
    /// census — see `DebugStats::record_dir_listing`).
    pub huge_dirs_seen: u64,
    /// The largest single-directory child count seen since start (0 if none).
    pub largest_dir_children: u64,
    /// Directories background verification declined outright (guard tooth 1).
    pub verify_declined_dirs: u64,
    /// Directories background verification diffed only partially (guard tooth 2).
    pub verify_truncated_dirs: u64,
    /// Main DB file size (bytes), excluding WAL/SHM
    pub db_main_size: Option<u64>,
    /// WAL file size (bytes)
    pub db_wal_size: Option<u64>,
    /// Total SQLite pages allocated
    pub db_page_count: Option<u64>,
    /// SQLite freelist pages (unused space)
    pub db_freelist_count: Option<u64>,
}

// ── Debug stats (shared atomics for the debug window) ────────────────

/// Child count at or above which a directory listing is counted as huge.
///
/// Deliberately far below `verify_guard::HUGE_DIR_CHILDREN` (the decline
/// threshold): the census exists to answer "how many machines have a directory
/// like this, and how big?", so it has to see the shoulder of the distribution,
/// not only the directories we already refuse.
pub(crate) const HUGE_DIR_CHILD_FLOOR: u64 = 10_000;

/// Shared counters for MustScanSubDirs events and live FS events.
/// Updated by event loops, read by the debug status IPC command.
pub(crate) struct DebugStats {
    pub(crate) must_scan_sub_dirs_count: AtomicU64,
    pub(crate) must_scan_rescans_completed: AtomicU64,
    pub(crate) live_event_count: AtomicU64,
    pub(crate) watcher_active: AtomicBool,
    /// Recent MustScanSubDirs paths: (timestamp, path). Ring buffer.
    pub(crate) recent_must_scan_paths: std::sync::Mutex<Vec<(String, String)>>,
    /// Timeline of indexing phases. Append-only, capped at 20 entries.
    pub(crate) phase_history: std::sync::Mutex<Vec<PhaseRecord>>,
    /// When the current phase started (for duration computation).
    pub(crate) phase_started: std::sync::Mutex<Option<std::time::Instant>>,
    /// Whether background verification is running concurrently.
    pub(crate) verifying: AtomicBool,
    /// Directory listings seen at or over [`HUGE_DIR_CHILD_FLOOR`] children.
    pub(crate) huge_dirs_seen: AtomicU64,
    /// The largest single-directory child count seen since start.
    pub(crate) largest_dir_children: AtomicU64,
    /// Directories background verification declined outright (guard tooth 1).
    pub(crate) verify_declined_dirs: AtomicU64,
    /// Directories background verification diffed only partially (guard tooth 2).
    pub(crate) verify_truncated_dirs: AtomicU64,
}

impl DebugStats {
    fn new() -> Self {
        let now = chrono::Local::now().format("%H:%M:%S%.3f").to_string();
        Self {
            must_scan_sub_dirs_count: AtomicU64::new(0),
            must_scan_rescans_completed: AtomicU64::new(0),
            live_event_count: AtomicU64::new(0),
            watcher_active: AtomicBool::new(false),
            recent_must_scan_paths: std::sync::Mutex::new(Vec::new()),
            phase_history: std::sync::Mutex::new(vec![PhaseRecord {
                phase: ActivityPhase::Idle,
                started_at: now,
                duration_ms: None,
                trigger: "app launch".to_string(),
                stats: Vec::new(),
            }]),
            phase_started: std::sync::Mutex::new(Some(std::time::Instant::now())),
            verifying: AtomicBool::new(false),
            huge_dirs_seen: AtomicU64::new(0),
            largest_dir_children: AtomicU64::new(0),
            verify_declined_dirs: AtomicU64::new(0),
            verify_truncated_dirs: AtomicU64::new(0),
        }
    }

    /// Record one directory listing's child count — the pathological-directory
    /// census.
    ///
    /// **Lock-free on purpose.** The guarded walker calls this from every rayon
    /// thread, once per directory; `record_must_scan`'s `Mutex<Vec<_>>` ring would
    /// put a lock on the scan hot path. Both atomics are `Relaxed`: the census is
    /// an instrument, and an interleaved max between two threads costs at most one
    /// slightly-low reading of a monotone gauge.
    ///
    /// **Call it from every walk that materialises a full directory listing.** A
    /// populated, previously-completed index never runs the guarded walker (it
    /// reconciles), so a walker-only census reads zero on exactly the established
    /// machines worth sampling. The three hooked walks are
    /// `scanner::InsertVisitor::visit_dir` (fresh scan / subtree scan),
    /// `local_reconcile::build_live_children` (full rescan), and
    /// `reconciler::reconcile_subtree` (the DEEP `MustScanSubDirs` drain and the
    /// other small-scope live fills, which is where a huge churny directory gets
    /// re-listed most often).
    ///
    /// **Deliberately not hooked:** `event_loop/verification.rs`'s per-navigation
    /// diff. Its `read_dir` loop is a stream that `verify_guard` stops at
    /// `HUGE_DIR_CHILDREN` iterations, so the only count it could report is
    /// censored exactly at the pathological end this census exists to measure.
    /// `verify_declined_dirs` / `verify_truncated_dirs` cover that route instead.
    pub(crate) fn record_dir_listing(&self, child_count: usize) {
        let n = child_count as u64;
        if n >= HUGE_DIR_CHILD_FLOOR {
            self.huge_dirs_seen.fetch_add(1, Ordering::Relaxed);
        }
        // Read-then-max: after the first few directories the load fails and no
        // read-modify-write touches the shared cache line at all.
        if n > self.largest_dir_children.load(Ordering::Relaxed) {
            self.largest_dir_children.fetch_max(n, Ordering::Relaxed);
        }
    }

    pub(crate) fn record_must_scan(&self, path: &str) {
        self.must_scan_sub_dirs_count.fetch_add(1, Ordering::Relaxed);
        if let Ok(mut paths) = self.recent_must_scan_paths.lock() {
            let now = chrono::Local::now().format("%H:%M:%S%.3f").to_string();
            paths.push((now, path.to_string()));
            if paths.len() > 50 {
                let excess = paths.len() - 50;
                paths.drain(..excess);
            }
        }
    }

    pub(crate) fn record_rescan_completed(&self) {
        self.must_scan_rescans_completed.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn reset(&self) {
        self.must_scan_sub_dirs_count.store(0, Ordering::Relaxed);
        self.must_scan_rescans_completed.store(0, Ordering::Relaxed);
        self.live_event_count.store(0, Ordering::Relaxed);
        self.watcher_active.store(false, Ordering::Relaxed);
        if let Ok(mut paths) = self.recent_must_scan_paths.lock() {
            paths.clear();
        }
        let now = chrono::Local::now().format("%H:%M:%S%.3f").to_string();
        if let Ok(mut history) = self.phase_history.lock() {
            history.clear();
            history.push(PhaseRecord {
                phase: ActivityPhase::Idle,
                started_at: now,
                duration_ms: None,
                trigger: "reset".to_string(),
                stats: Vec::new(),
            });
        }
        if let Ok(mut started) = self.phase_started.lock() {
            *started = Some(std::time::Instant::now());
        }
        self.verifying.store(false, Ordering::Relaxed);
        self.huge_dirs_seen.store(0, Ordering::Relaxed);
        self.largest_dir_children.store(0, Ordering::Relaxed);
        self.verify_declined_dirs.store(0, Ordering::Relaxed);
        self.verify_truncated_dirs.store(0, Ordering::Relaxed);
    }

    pub(crate) fn set_phase(&self, phase: ActivityPhase, trigger: &str) {
        let now_formatted = chrono::Local::now().format("%H:%M:%S%.3f").to_string();
        let now_instant = std::time::Instant::now();

        if let Ok(mut history) = self.phase_history.lock() {
            // Close the current (last) entry if it's still in progress
            if let Some(last) = history.last_mut()
                && last.duration_ms.is_none()
                && let Ok(started) = self.phase_started.lock()
                && let Some(start) = *started
            {
                last.duration_ms = Some(start.elapsed().as_millis() as u64);
            }

            // Append new phase
            history.push(PhaseRecord {
                phase,
                started_at: now_formatted,
                duration_ms: None,
                trigger: trigger.to_string(),
                stats: Vec::new(),
            });

            // Cap at 20 entries
            if history.len() > 20 {
                let excess = history.len() - 20;
                history.drain(..excess);
            }
        }

        if let Ok(mut started) = self.phase_started.lock() {
            *started = Some(now_instant);
        }
    }

    pub(crate) fn close_phase_with_stats(&self, stats: Vec<(&str, String)>) {
        if let Ok(mut history) = self.phase_history.lock()
            && let Some(last) = history.last_mut()
        {
            last.stats = stats.into_iter().map(|(k, v)| (k.to_string(), v)).collect();
        }
    }
}

pub(crate) static DEBUG_STATS: LazyLock<DebugStats> = LazyLock::new(DebugStats::new);

/// Record a top-level phase transition in BOTH the global debug timeline and the
/// per-volume `index-phase-changed` event.
///
/// Call this instead of `DEBUG_STATS.set_phase` wherever a `volume_id` and an
/// `AppHandle` are in scope, so the two never drift: the debug window keeps its
/// app-wide journal, and the frontend's per-volume step checklist learns which
/// drive changed to which phase. See [`IndexPhaseChangedEvent`] for why the
/// per-volume event exists. The event is fire-and-forget (a missed UI update is
/// harmless; the next transition or a status query reconciles it).
pub(super) fn set_phase_for(app: &AppHandle, volume_id: &str, phase: ActivityPhase, trigger: &str) {
    DEBUG_STATS.set_phase(phase.clone(), trigger);
    let _ = IndexPhaseChangedEvent {
        volume_id: volume_id.to_string(),
        phase,
    }
    .emit(app);
}

#[cfg(test)]
mod tests {
    //! ActivityPhase transition tests.
    //!
    //! `DebugStats` is the journal the debug window reads. Transitions are
    //! one-way appends (each `set_phase` closes the previous entry and pushes
    //! a new one). This isn't a strict state machine, but it does encode a
    //! pipeline order (`Replaying -> Live`, `Scanning -> Aggregating ->
    //! Reconciling -> Live`) that the UI relies on for the timeline strip.
    //!
    //! We construct a fresh `DebugStats` per test (not the global) so tests
    //! don't fight over the singleton.
    use super::*;
    use std::time::Duration;

    fn last_phase(stats: &DebugStats) -> ActivityPhase {
        let history = stats.phase_history.lock().expect("phase_history poisoned");
        history
            .last()
            .expect("phase_history must always have an entry")
            .phase
            .clone()
    }

    fn nth_phase(stats: &DebugStats, n: usize) -> ActivityPhase {
        let history = stats.phase_history.lock().expect("phase_history poisoned");
        history.get(n).expect("phase_history index out of bounds").phase.clone()
    }

    fn history_len(stats: &DebugStats) -> usize {
        stats.phase_history.lock().expect("phase_history poisoned").len()
    }

    #[test]
    fn debug_stats_initial_phase_is_idle() {
        let stats = DebugStats::new();
        assert!(matches!(last_phase(&stats), ActivityPhase::Idle));
    }

    #[test]
    fn set_phase_idle_to_replaying_transition() {
        // Pins `manager.rs:184`: app launch with pending FSEvents.
        let stats = DebugStats::new();
        stats.set_phase(ActivityPhase::Replaying, "app launch, pending FSEvents");
        assert!(matches!(last_phase(&stats), ActivityPhase::Replaying));
    }

    #[test]
    fn set_phase_replaying_to_live_transition() {
        // Pins `event_loop.rs:769`: post-replay handoff to live event processing.
        let stats = DebugStats::new();
        stats.set_phase(ActivityPhase::Replaying, "replay start");
        stats.set_phase(ActivityPhase::Live, "replay complete");
        assert!(matches!(last_phase(&stats), ActivityPhase::Live));
    }

    #[test]
    fn set_phase_full_scan_pipeline_transitions() {
        // Pins the documented scan pipeline order:
        // Idle -> Scanning -> Aggregating -> Reconciling -> Live.
        // The UI's timeline strip depends on this exact sequence.
        let stats = DebugStats::new();
        stats.set_phase(ActivityPhase::Scanning, "user-initiated scan");
        stats.set_phase(ActivityPhase::Aggregating, "scan complete");
        stats.set_phase(ActivityPhase::Reconciling, "aggregation complete");
        stats.set_phase(ActivityPhase::Live, "reconciliation complete");

        // Initial Idle + 4 transitions = 5 history entries.
        assert_eq!(history_len(&stats), 5);
        assert!(matches!(nth_phase(&stats, 0), ActivityPhase::Idle));
        assert!(matches!(nth_phase(&stats, 1), ActivityPhase::Scanning));
        assert!(matches!(nth_phase(&stats, 2), ActivityPhase::Aggregating));
        assert!(matches!(nth_phase(&stats, 3), ActivityPhase::Reconciling));
        assert!(matches!(nth_phase(&stats, 4), ActivityPhase::Live));
    }

    #[test]
    fn set_phase_to_idle_on_shutdown_transition() {
        // Pins `manager.rs:621,746`: any phase can be closed out to Idle
        // when the indexer is stopped or shut down.
        let stats = DebugStats::new();
        stats.set_phase(ActivityPhase::Scanning, "user scan");
        stats.set_phase(ActivityPhase::Idle, "shutdown");
        assert!(matches!(last_phase(&stats), ActivityPhase::Idle));
    }

    #[test]
    fn set_phase_closes_previous_entry_with_duration() {
        // Pins the "close last entry's duration_ms before appending the new
        // one" branch (events.rs:296–303). If this regresses, the timeline
        // strip would show only the latest phase without elapsed times.
        let stats = DebugStats::new();
        stats.set_phase(ActivityPhase::Scanning, "scan");
        // Sleep a tiny bit so the next set_phase computes a non-zero duration
        // for the Scanning entry it just closed.
        std::thread::sleep(Duration::from_millis(2));
        stats.set_phase(ActivityPhase::Live, "live");

        let history = stats.phase_history.lock().unwrap();
        // Entry index 1 is Scanning; it should be closed (duration_ms = Some).
        assert!(matches!(history[1].phase, ActivityPhase::Scanning));
        assert!(
            history[1].duration_ms.is_some(),
            "previous phase must be closed with a duration when a new phase begins"
        );
        // The newest entry (Live) is still in progress.
        assert!(history[2].duration_ms.is_none());
    }

    #[test]
    fn set_phase_caps_history_at_20_entries() {
        // Pins the ring-buffer cap (events.rs:315–318). 30 transitions in
        // and we keep only the most recent 20, oldest dropped first.
        let stats = DebugStats::new();
        // The Idle initial entry counts toward the cap, so 30 more pushes
        // means the cap drains the oldest entries (the initial Idle + early
        // Scanning entries).
        for i in 0..30 {
            let phase = if i % 2 == 0 {
                ActivityPhase::Scanning
            } else {
                ActivityPhase::Live
            };
            stats.set_phase(phase, "stress");
        }
        assert_eq!(history_len(&stats), 20);
        // The newest entry (index 19) must be the last one pushed.
        // i=29 is odd -> Live.
        assert!(matches!(nth_phase(&stats, 19), ActivityPhase::Live));
    }

    #[test]
    fn reset_collapses_history_to_a_single_idle_entry() {
        // Pins `reset()` (events.rs:266): after a stop+restart, the timeline
        // should start from a fresh Idle, not from the residual phases.
        let stats = DebugStats::new();
        stats.set_phase(ActivityPhase::Scanning, "scan");
        stats.set_phase(ActivityPhase::Aggregating, "aggregate");
        stats.reset();

        assert_eq!(history_len(&stats), 1, "reset must collapse history");
        assert!(matches!(last_phase(&stats), ActivityPhase::Idle));
        // Counters must also be cleared.
        assert_eq!(stats.must_scan_sub_dirs_count.load(Ordering::Relaxed), 0);
        assert_eq!(stats.live_event_count.load(Ordering::Relaxed), 0);
        assert!(!stats.watcher_active.load(Ordering::Relaxed));
    }

    #[test]
    fn activity_phase_serializes_to_snake_case_wire_values() {
        // The per-volume `index-phase-changed` event ships the `ActivityPhase`
        // variant verbatim; the frontend maps each wire string to a checklist
        // step (no string-matching on labels). Pin the wire values so a rename
        // can't silently break the FE step map.
        use serde_json::json;
        assert_eq!(
            serde_json::to_value(ActivityPhase::Replaying).unwrap(),
            json!("replaying")
        );
        assert_eq!(
            serde_json::to_value(ActivityPhase::Scanning).unwrap(),
            json!("scanning")
        );
        assert_eq!(
            serde_json::to_value(ActivityPhase::Aggregating).unwrap(),
            json!("aggregating")
        );
        assert_eq!(
            serde_json::to_value(ActivityPhase::Reconciling).unwrap(),
            json!("reconciling")
        );
        assert_eq!(serde_json::to_value(ActivityPhase::Live).unwrap(), json!("live"));
        assert_eq!(serde_json::to_value(ActivityPhase::Idle).unwrap(), json!("idle"));
    }

    #[test]
    fn index_phase_changed_event_serializes_volume_id_as_camel_case() {
        // The payload crosses IPC as `{ volumeId, phase }`; the FE binding and
        // `index-state` read exactly those keys.
        use serde_json::json;
        let ev = IndexPhaseChangedEvent {
            volume_id: "smb-nas".to_string(),
            phase: ActivityPhase::Reconciling,
        };
        assert_eq!(
            serde_json::to_value(&ev).unwrap(),
            json!({ "volumeId": "smb-nas", "phase": "reconciling" })
        );
    }

    #[test]
    fn close_phase_with_stats_attaches_to_current_phase_only() {
        // Pins `close_phase_with_stats`: attaches to the LAST entry, not to
        // a closed historical one. If this regresses, scan-completion stats
        // would land on the wrong phase or on no phase at all.
        let stats = DebugStats::new();
        stats.set_phase(ActivityPhase::Scanning, "scan");
        stats.close_phase_with_stats(vec![("entries", "1234".to_string())]);

        let history = stats.phase_history.lock().unwrap();
        // index 0 = Idle (no stats), index 1 = Scanning (with stats).
        assert!(history[0].stats.is_empty());
        assert_eq!(history[1].stats, vec![("entries".to_string(), "1234".to_string())]);
    }
}
