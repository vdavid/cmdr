//! The test doubles for the event seam: a sink that records every event, and one
//! event of every kind for a host to prove its mapping is complete. Kept apart
//! from `sink.rs` so the production catalog reads without them; the whole file is
//! gated on `test` / `testing` where `mod.rs` declares it.

use std::path::PathBuf;

use crate::indexing::aggregator::AggregationPhase;
use crate::indexing::lifecycle::freshness::Freshness;

use super::payload::{
    ActivityPhase, CoveragePhase, FolderChangeRollup, MemoryWatchdogAction, RescanReason, ScanRunKind,
};
use super::sink::{Diagnostic, EventSink, IndexErrorReport, IndexEvent, IndexEventKind, MediaEnrichTerminalReason};

/// One event of every kind, for a host to prove its mapping is complete.
///
/// Paired with [`IndexEventKind::ALL`], which the compiler keeps complete (see
/// its `slot_of`): a new variant fails to compile until it's listed there, and
/// the host's completeness test then fails until it's built here too. So neither
/// list can quietly fall behind the enum.
pub fn one_of_every_kind() -> Vec<IndexEvent> {
    vec![
        IndexEvent::ScanStarted {
            volume_id: "root".into(),
            run_kind: ScanRunKind::FirstScan,
            prior_total_entries: Some(1),
            prior_scan_duration_ms: Some(2),
            volume_used_bytes: Some(3),
            covered_in_phases: false,
            left_after_find_files_ms: Some(4),
            left_after_save_ms: Some(5),
            left_after_compute_ms: Some(6),
            left_after_catch_up_ms: Some(0),
        },
        IndexEvent::CoverageBranchStarted {
            volume_id: "root".into(),
            roots: vec!["/Users/someone/Downloads".into()],
        },
        IndexEvent::CoverageBranchEnded {
            volume_id: "root".into(),
            roots: vec!["/Users/someone/Downloads".into()],
        },
        IndexEvent::CoveragePhaseStarted {
            volume_id: "root".into(),
            phase: CoveragePhase::PriorityRoot,
            root: "/Users/someone/Downloads".into(),
        },
        IndexEvent::HomeCovered {
            volume_id: "root".into(),
        },
        IndexEvent::ScanProgress {
            volume_id: "root".into(),
            entries_scanned: 1,
            dirs_found: 2,
            bytes_scanned: 3,
        },
        IndexEvent::ScanComplete {
            volume_id: "root".into(),
            total_entries: 1,
            total_dirs: 2,
            duration_ms: 3,
        },
        IndexEvent::ScanAborted {
            volume_id: "root".into(),
        },
        IndexEvent::DirsUpdated {
            paths: vec!["/tmp".into()],
        },
        IndexEvent::ReplayProgress {
            volume_id: "root".into(),
            events_processed: 1,
            estimated_total: Some(2),
        },
        IndexEvent::ReplayComplete {
            volume_id: "root".into(),
            duration_ms: 1,
        },
        IndexEvent::RescanScheduled {
            volume_id: "root".into(),
            reason: RescanReason::StaleIndex,
            details: Diagnostic("stale".into()),
        },
        IndexEvent::AggregationProgress {
            volume_id: "root".into(),
            phase: AggregationPhase::Computing,
            current: 1,
            total: 2,
        },
        IndexEvent::AggregationComplete {
            volume_id: "root".into(),
        },
        IndexEvent::MemoryWarning {
            phys_footprint_bytes: 1,
            resident_bytes: 2,
            global_allocator: cmdr_fs::process_memory::GlobalAllocator::System,
            rust_heap_bytes: 3,
            system_malloc_bytes: 4,
            untracked_bytes: 5,
            action: MemoryWatchdogAction::StoppedIndexing,
        },
        IndexEvent::FreshnessChanged {
            volume_id: "root".into(),
            freshness: Freshness::Fresh,
        },
        IndexEvent::PhaseChanged {
            volume_id: "root".into(),
            phase: ActivityPhase::Live,
        },
        IndexEvent::MediaEnrichProgress {
            volume_id: "root".into(),
            done: 1,
            total: 2,
            bytes_done: 3,
            bytes_total: 4,
        },
        IndexEvent::MediaEnrichTerminal {
            volume_id: "root".into(),
            reason: MediaEnrichTerminalReason::Cancelled,
        },
        IndexEvent::Error {
            report: IndexErrorReport::WalkWorkerSpawnFailed {
                detail: Diagnostic("out of threads".into()),
            },
        },
        IndexEvent::PathAccessDenied {
            path: PathBuf::from("/Users/someone/Downloads"),
        },
        IndexEvent::FolderActivity {
            volume_id: "root".into(),
            observed_at: 1_780_000_027,
            folders: vec![FolderChangeRollup {
                folder: "/Users/someone/Downloads".into(),
                created: 3,
                modified: 1,
                removed: 0,
                renamed: 2,
                last_event_at: 1_780_000_027,
            }],
        },
        IndexEvent::IndexNeedsFreshScan {
            volume_id: "root".into(),
        },
    ]
}

/// A sink that keeps every event for a test to assert on.
#[derive(Default)]
pub struct RecordingSink {
    events: std::sync::Mutex<Vec<IndexEvent>>,
}

impl RecordingSink {
    /// An empty recorder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Everything recorded so far, in emit order.
    pub fn events(&self) -> Vec<IndexEvent> {
        use cmdr_fs::ignore_poison::IgnorePoison;
        self.events.lock_ignore_poison().clone()
    }

    /// The kinds recorded for `volume_id`, in emit order. The shape assertion
    /// most tests actually want.
    pub fn kinds_for(&self, volume_id: &str) -> Vec<IndexEventKind> {
        self.events()
            .iter()
            .filter(|e| e.volume_id() == Some(volume_id))
            .map(IndexEvent::kind)
            .collect()
    }
}

impl EventSink for RecordingSink {
    fn emit(&self, event: IndexEvent) {
        use cmdr_fs::ignore_poison::IgnorePoison;
        self.events.lock_ignore_poison().push(event);
    }
}
