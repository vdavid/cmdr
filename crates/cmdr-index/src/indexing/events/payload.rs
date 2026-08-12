//! The typed values an [`IndexEvent`](super::IndexEvent) carries: what kind of
//! run a scan is, why a rescan happened, what the indexer is doing, and what the
//! memory watchdog did.
//!
//! A leaf on purpose. Both the event envelope (`sink.rs`) and the IPC response
//! types (`mod.rs`) name this vocabulary, so it lives below both and neither has
//! to import the other. ❌ A new value an event carries belongs HERE, not in
//! `mod.rs` — putting it there is what made the envelope import its own parent.
//!
//! These keep their `specta::Type` derives. A schema derive on a value is fine
//! in this crate; a presentation decision isn't, and none of these makes one.

use serde::{Deserialize, Serialize};

use crate::indexing::store::ScanCalibrationKind;

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
/// frontend would have to match on.
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
