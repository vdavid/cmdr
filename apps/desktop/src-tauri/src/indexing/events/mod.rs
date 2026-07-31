//! The frontend event + scan-progress surface for the indexing system.
//!
//! This module root holds the Tauri event payloads and response types; the two
//! children drive scan progress off them:
//! - [`progress_reporter`]: the 500 ms progress + mid-scan partial-aggregation
//!   tick loop shared by every scan path.
//! - [`partial_agg`]: the pure send-decision + hot-path collection the loop uses.

use std::sync::LazyLock;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use serde::{Deserialize, Serialize};

use super::store::{IndexFailure, IndexStatus, ScanCalibrationKind};

pub(crate) mod partial_agg;
pub(crate) mod progress_reporter;
pub(crate) mod sink;

#[cfg(any(test, feature = "testing"))]
pub use sink::RecordingSink;
#[cfg(test)]
pub(crate) use sink::one_of_every_kind;
pub use sink::{Diagnostic, EventSink, IndexErrorReport, IndexEvent, IndexEventKind, NoopEventSink};

// ── Event payloads (Rust -> Frontend) ────────────────────────────────

/// What kind of run a scan is, decided by the backend at the scan-start funnel
/// and shipped to the frontend so the UI never has to guess from the calibration
/// numbers (which answer a different question and disagree on a partial index).
///
/// The user-visible difference is what happens to folder sizes: a walk that
/// truncates blanks them for its whole run, while a change check keeps the
/// last-good sizes on screen, marked stale.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum ScanRunKind {
    /// The volume's first index build: nothing usable on disk to keep, so the
    /// walk starts from an empty index and folder sizes appear at the end.
    FirstScan,
    /// A full rebuild of an existing index: the entries are truncated and
    /// re-walked, so folder sizes go blank until the run finishes. Taken when a
    /// rescan can't reconcile in place (for example a previous scan that never
    /// completed).
    FullRebuild,
    /// A rescan in place: every directory is diffed against the index and only
    /// the changes are written, so the last-good folder sizes stay visible
    /// (stale) for the whole run. Far slower per entry, far kinder to look at.
    ChangeCheck,
}

impl ScanRunKind {
    /// Classify the run about to start from the two facts the scan-start funnel
    /// already holds: whether this walk reconciles in place, and what a prior
    /// COMPLETED scan left behind as calibration.
    pub fn classify(reconciles_in_place: bool, prior_total_entries: Option<u64>) -> Self {
        if reconciles_in_place {
            Self::ChangeCheck
        } else if prior_total_entries.is_some_and(|n| n > 0) {
            Self::FullRebuild
        } else {
            Self::FirstScan
        }
    }

    /// Which calibration bucket this run's timing belongs in. A first scan and a
    /// full rebuild run the SAME walker, so they share one bucket; only the
    /// change check, which is roughly 5x slower, needs its own.
    pub fn calibration_kind(self) -> ScanCalibrationKind {
        match self {
            Self::FirstScan | Self::FullRebuild => ScanCalibrationKind::FullWalk,
            Self::ChangeCheck => ScanCalibrationKind::ChangeCheck,
        }
    }
}

/// Why a full rescan was triggered instead of incremental replay.
/// Sent to the frontend as `index-rescan-notification` so the UI can show
/// a transparent, user-friendly toast.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
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

/// What the memory watchdog did, as a typed variant rather than a string the
/// frontend would have to match on (`no-string-matching`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum MemoryWatchdogAction {
    /// The safety limit was crossed and every volume's index was stopped.
    StoppedIndexing,
    /// Memory kept climbing after that stop, so the growth isn't (only) indexing.
    StillGrowingAfterStop,
}

// ── Activity phase tracking ──────────────────────────────────────────

/// What the indexer is currently doing. More granular than `IndexPhase`
/// (which tracks lifecycle: Disabled/Initializing/Running/ShuttingDown).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
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
    /// The scan's kind, from the same stashed calibration, so a mid-scan window
    /// reload recovers the run-kind header and its per-step copy instead of
    /// dropping them. `None` before this volume's first scan of the session; like
    /// `volume_used_bytes` it describes the LATEST scan, so read it only when
    /// `scanning` is true.
    pub scan_run_kind: Option<ScanRunKind>,
    /// The tier-1 calibration the RUNNING scan is actually using, straight off the
    /// stash — the per-kind bucket, not the unsuffixed `meta` keys `index_status`
    /// carries. A reload that fell back to those would seed a full walk's ETA off
    /// the ~5x slower change check that happened to run last. Same read-only-while-
    /// `scanning` rule as the two fields above.
    pub prior_total_entries: Option<u64>,
    pub prior_scan_duration_ms: Option<u64>,
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
    pub freshness: Option<super::lifecycle::freshness::Freshness>,
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
    /// How many shallow `MustScanSubDirs` anchors were coalesced SINCE THE LAST
    /// COMPLETED SWEEP — the times macOS told us it lost track of changes and we
    /// deliberately didn't rescan (see `reconcile/reconciler/rescan_route.rs`). `0` means
    /// nothing was skipped. Feeds the tooltip's "macOS lost track of file system
    /// changes N times" line; the badge itself does NOT branch on this, because
    /// once-a-day sweeping is the designed operating state, not a fault.
    ///
    /// Deliberately NOT a lifetime total, which would only measure how long the
    /// app has been installed.
    pub coalesced_signals_since_sweep: u32,
    /// Unix seconds when the next full sweep becomes due for this volume (the last
    /// sweep plus the volume's window). Lets the tooltip say "next full check in N
    /// hours" without duplicating the policy constant in the frontend. `None` until
    /// a first sweep has been recorded.
    pub next_sweep_due_at: Option<u64>,
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
    /// Directories background verification declined outright (guard tooth 1).
    pub verify_declined_dirs: u64,
    /// Directories background verification diffed only partially (guard tooth 2).
    pub verify_truncated_dirs: u64,
    /// Subtrees the reconcile walk stopped descending into because too high a
    /// share of their reads was pathologically slow
    /// (`reconcile/local_reconcile/cost_budget.rs`).
    pub reconcile_budget_subtrees: u64,
    /// Directories the reconcile walk left undescended inside those subtrees.
    pub reconcile_budget_skipped_dirs: u64,
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
    /// Directories background verification declined outright (guard tooth 1).
    pub(crate) verify_declined_dirs: AtomicU64,
    /// Directories background verification diffed only partially (guard tooth 2).
    pub(crate) verify_truncated_dirs: AtomicU64,
    /// Subtrees the reconcile walk stopped descending into on its read-time
    /// budget, and the directories that left undescended.
    pub(crate) reconcile_budget_subtrees: AtomicU64,
    pub(crate) reconcile_budget_skipped_dirs: AtomicU64,
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
            verify_declined_dirs: AtomicU64::new(0),
            verify_truncated_dirs: AtomicU64::new(0),
            reconcile_budget_subtrees: AtomicU64::new(0),
            reconcile_budget_skipped_dirs: AtomicU64::new(0),
        }
    }

    /// One subtree just crossed the reconcile walk's per-subtree read-time
    /// budget. Counted once per subtree, however many directories it then leaves
    /// undescended.
    pub(crate) fn record_reconcile_budget_trip(&self) {
        self.reconcile_budget_subtrees.fetch_add(1, Ordering::Relaxed);
    }

    /// One directory the reconcile walk didn't descend into because its subtree
    /// was over budget.
    pub(crate) fn record_reconcile_budget_skip(&self) {
        self.reconcile_budget_skipped_dirs.fetch_add(1, Ordering::Relaxed);
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
        self.verify_declined_dirs.store(0, Ordering::Relaxed);
        self.verify_truncated_dirs.store(0, Ordering::Relaxed);
        self.reconcile_budget_subtrees.store(0, Ordering::Relaxed);
        self.reconcile_budget_skipped_dirs.store(0, Ordering::Relaxed);
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
/// per-volume [`IndexEvent::PhaseChanged`] event.
///
/// Call this instead of `DEBUG_STATS.set_phase` wherever a `volume_id` and a sink
/// are in scope, so the two never drift: the debug window keeps its app-wide
/// journal, and the frontend's per-volume step checklist learns which drive
/// changed to which phase. Fire-and-forget (a missed UI update is harmless; the
/// next transition or a status query reconciles it).
pub(super) fn set_phase_for(events: &dyn EventSink, volume_id: &str, phase: ActivityPhase, trigger: &str) {
    DEBUG_STATS.set_phase(phase.clone(), trigger);
    events.emit(IndexEvent::PhaseChanged {
        volume_id: volume_id.to_string(),
        phase,
    });
}

/// Report that a full rescan was chosen over incremental replay, and log why.
///
/// The log line stays here (it's the diagnostic the scan path wants at the moment
/// it decides); the event carries the typed reason plus the same details for the
/// host to ship on.
pub(super) fn emit_rescan_notification(events: &dyn EventSink, volume_id: &str, reason: RescanReason, details: String) {
    log::info!("Index rescan triggered ({reason:?}): {details}");
    events.emit(IndexEvent::RescanScheduled {
        volume_id: volume_id.to_string(),
        reason,
        details: Diagnostic(details),
    });
}

#[cfg(test)]
mod tests;
