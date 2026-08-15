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
use crate::indexing::store::ScanCalibrationKind;

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
    stats.set_phase(ActivityPhase::Live, "live");

    let history = stats.phase_history.lock().unwrap();
    // Entry index 1 is Scanning; it should be closed (duration_ms = Some). `set_phase` stamps
    // the closed entry with `Some(elapsed)` unconditionally, so no elapsed time is needed to
    // make the field populated; a same-instant close still records `Some(0)`.
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
fn scan_run_kind_serializes_to_snake_case_wire_values() {
    // The FE maps each wire string to a run-kind header and its per-step
    // copy, so a rename here silently mislabels a running scan.
    use serde_json::json;
    assert_eq!(
        serde_json::to_value(ScanRunKind::FirstScan).unwrap(),
        json!("first_scan")
    );
    assert_eq!(
        serde_json::to_value(ScanRunKind::FullRebuild).unwrap(),
        json!("full_rebuild")
    );
    assert_eq!(
        serde_json::to_value(ScanRunKind::ChangeCheck).unwrap(),
        json!("change_check")
    );
}

#[test]
fn scan_run_kind_classifies_the_three_runs() {
    // Reconciling in place is a change check whatever the prior totals say.
    assert_eq!(ScanRunKind::classify(true, Some(5_000_000)), ScanRunKind::ChangeCheck);
    assert_eq!(ScanRunKind::classify(true, None), ScanRunKind::ChangeCheck);
    // A truncating walk with a completed scan behind it is a full rebuild…
    assert_eq!(ScanRunKind::classify(false, Some(5_000_000)), ScanRunKind::FullRebuild);
    // …and without one it's the volume's first build. Pins the case the FE
    // used to get wrong: a populated but never-completed index truncates, so
    // it's a rebuild, not a change check.
    assert_eq!(ScanRunKind::classify(false, None), ScanRunKind::FirstScan);
    assert_eq!(ScanRunKind::classify(false, Some(0)), ScanRunKind::FirstScan);
}

#[test]
fn both_truncating_runs_share_one_calibration_bucket() {
    // They run the same walker, so a first scan's timing calibrates a later
    // full rebuild; only the ~5x slower change check needs its own bucket.
    assert_eq!(ScanRunKind::FirstScan.calibration_kind(), ScanCalibrationKind::FullWalk);
    assert_eq!(
        ScanRunKind::FullRebuild.calibration_kind(),
        ScanCalibrationKind::FullWalk
    );
    assert_eq!(
        ScanRunKind::ChangeCheck.calibration_kind(),
        ScanCalibrationKind::ChangeCheck
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

/// A walk that takes a volume whole reports its ground in the SAME shape a
/// phase's branch does, so a host runs one membership test over one list of
/// paths and never has to ask which kind of run this is.
///
/// The volume root is what makes that work: every path on the volume is at or
/// under it, so a consumer's bidirectional test matches every row without a
/// sentinel value or a mode flag.
#[test]
fn a_whole_volume_walk_reports_the_volume_root_as_its_ground() {
    use crate::indexing::events::{RecordingSink, announce_whole_volume_walk};

    let sink = RecordingSink::new();
    announce_whole_volume_walk(&sink, "root", "/".to_string());

    assert_eq!(
        sink.events(),
        vec![IndexEvent::CoverageBranchStarted {
            volume_id: "root".to_string(),
            roots: vec!["/".to_string()],
        }],
    );
}

/// The end of a whole-volume walk is NOT emitted here, and that is deliberate:
/// the host closes a volume's open ground on the run's terminal event, which is
/// the only thing that covers the paths that abort rather than complete. A
/// remembered end here would be one more place to forget it.
#[test]
fn a_whole_volume_walk_leaves_its_end_to_the_run_that_owns_it() {
    use crate::indexing::events::{RecordingSink, announce_whole_volume_walk};

    let sink = RecordingSink::new();
    announce_whole_volume_walk(&sink, "smb-nas", "/Volumes/nas".to_string());

    assert!(
        !sink
            .events()
            .iter()
            .any(|e| e.kind() == IndexEventKind::CoverageBranchEnded),
    );
}
