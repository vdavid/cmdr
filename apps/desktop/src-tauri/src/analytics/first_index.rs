//! Whether covering a drive in phases actually delivers what it promises.
//!
//! The change is justified by a user-experience claim — "your own folders are
//! searchable and sized in seconds, and quitting never costs you the work" — and a
//! claim nobody measures is a claim nobody can be wrong about. Three of the four
//! numbers come from here, off the index's own event stream:
//!
//! 1. **How long until home is covered** (`first_index_home_covered`), the moment
//!    the user's own files start answering.
//! 2. **How long until the whole drive is covered** (`first_index_completed`).
//! 3. **How often a first index never finishes at all** — the case the
//!    truncate-and-rebuild design lost entirely, since an interrupted run left
//!    nothing to count. Counted as a RATIO of `first_index_completed` to
//!    `first_index_started` rather than as a terminal "interrupted" event: a run
//!    that ends with the process (a quit, a crash, a power cut) has no moment left
//!    to report in, so anything counted at the end under-counts exactly the case
//!    being measured.
//!
//! The fourth, time from launch to the first honest folder size on a folder the
//! user opened, is the frontend's (`$lib/indexing/first-size-timing.ts`): only it
//! knows what is on screen.
//!
//! ❌ Nothing here carries a path, a folder name, or a volume id: buckets and the
//! volume KIND only, in line with the PII rules in `CLAUDE.md`.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use cmdr_index::IndexEvent;

use crate::ignore_poison::IgnorePoison;

/// When each volume's phased first index started, keyed by volume id.
///
/// An entry exists only while a PHASED run is in flight, so its presence is also
/// the answer to "is this completion one of ours?" — a completed change check on
/// an already-indexed drive fires the same `ScanComplete` and must not be counted
/// as a first index.
fn clocks() -> &'static Mutex<HashMap<String, Instant>> {
    static CLOCKS: OnceLock<Mutex<HashMap<String, Instant>>> = OnceLock::new();
    CLOCKS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Take one index event and report what it says about a first index.
///
/// Called for every event on the sink's own thread, so it stays a hash lookup in
/// the common case and touches PostHog only at the three moments that matter.
pub(crate) fn observe(event: &IndexEvent) {
    match event {
        IndexEvent::ScanStarted {
            volume_id,
            covered_in_phases: true,
            ..
        } => started(volume_id),
        IndexEvent::HomeCovered { volume_id } => home_covered(volume_id),
        IndexEvent::ScanComplete { volume_id, .. } => completed(volume_id),
        // A run that ended without covering the drive is counted by its ABSENCE
        // from the completed side, so the clock simply goes.
        IndexEvent::ScanAborted { volume_id } => {
            clocks().lock_ignore_poison().remove(volume_id);
        }
        _ => {}
    }
}

fn started(volume_id: &str) {
    clocks()
        .lock_ignore_poison()
        .insert(volume_id.to_string(), Instant::now());
    super::posthog::capture("first_index_started", serde_json::json!({}));
}

fn home_covered(volume_id: &str) {
    let Some(elapsed) = clocks().lock_ignore_poison().get(volume_id).map(Instant::elapsed) else {
        return;
    };
    super::posthog::capture(
        "first_index_home_covered",
        serde_json::json!({ "duration_bucket": short_bucket(elapsed) }),
    );
}

fn completed(volume_id: &str) {
    let Some(started_at) = clocks().lock_ignore_poison().remove(volume_id) else {
        return; // Not a phased run: a change check or a rebuild finishing.
    };
    super::posthog::capture(
        "first_index_completed",
        serde_json::json!({ "duration_bucket": long_bucket(started_at.elapsed()) }),
    );
}

/// Home coverage is meant to land in seconds, so its buckets are fine there and
/// coarse past it. Measured on a real boot disk, home lands around 16 s with
/// `~/Library` deferred (`docs/notes/phased-vs-bulk-index-2026-08-14.md`), so the
/// first two buckets are where the claim lives or dies.
fn short_bucket(elapsed: Duration) -> &'static str {
    match elapsed.as_secs() {
        0..10 => "<10s",
        10..30 => "10-30s",
        30..120 => "30s-2m",
        120..300 => "2-5m",
        _ => "5m+",
    }
}

/// Whole-drive coverage is minutes, so its buckets start where the other's end.
fn long_bucket(elapsed: Duration) -> &'static str {
    match elapsed.as_secs() {
        0..60 => "<1m",
        60..180 => "1-3m",
        180..600 => "3-10m",
        600..1800 => "10-30m",
        _ => "30m+",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The clocks are process-wide, so a test picks a volume id nobody else uses
    /// rather than clearing the map out from under a neighbour.
    fn scan_started(volume_id: &str, phased: bool) -> IndexEvent {
        IndexEvent::ScanStarted {
            volume_id: volume_id.to_string(),
            run_kind: cmdr_index::ScanRunKind::FirstScan,
            prior_total_entries: None,
            prior_scan_duration_ms: None,
            volume_used_bytes: None,
            covered_in_phases: phased,
        }
    }

    fn scan_complete(volume_id: &str) -> IndexEvent {
        IndexEvent::ScanComplete {
            volume_id: volume_id.to_string(),
            total_entries: 1,
            total_dirs: 1,
            duration_ms: 1,
        }
    }

    fn is_clocked(volume_id: &str) -> bool {
        clocks().lock_ignore_poison().contains_key(volume_id)
    }

    /// The whole shape in one pass: a phased run is clocked from its start, home
    /// coverage reports without ending it, and the completion ends it exactly
    /// once — a second `ScanComplete` (a later change check on the same drive)
    /// must not count as a second first index.
    #[test]
    fn a_phased_run_is_clocked_from_its_start_and_ends_once() {
        let volume = "analytics-first-index-happy";
        observe(&scan_started(volume, true));
        assert!(is_clocked(volume), "a phased run starts the clock");

        observe(&IndexEvent::HomeCovered {
            volume_id: volume.to_string(),
        });
        assert!(is_clocked(volume), "home coverage reports without ending the run");

        observe(&scan_complete(volume));
        assert!(!is_clocked(volume), "the completion ends it");

        observe(&scan_complete(volume));
        assert!(
            !is_clocked(volume),
            "and a later completion on the same drive has no run to end"
        );
    }

    /// A whole-volume scan (a rebuild, a change check) fires the same events, and
    /// counting its completion would inflate the numerator of the very ratio the
    /// interruption rate is read from.
    #[test]
    fn a_run_that_is_not_phased_is_not_a_first_index() {
        let volume = "analytics-first-index-bulk";
        observe(&scan_started(volume, false));
        assert!(!is_clocked(volume), "❌ only a phased run is a first index here");

        observe(&scan_complete(volume));
        assert!(!is_clocked(volume));
    }

    /// A run that ends without covering the drive leaves nothing behind: it is
    /// counted by not appearing on the completed side.
    #[test]
    fn an_aborted_run_leaves_no_clock_running() {
        let volume = "analytics-first-index-aborted";
        observe(&scan_started(volume, true));
        observe(&IndexEvent::ScanAborted {
            volume_id: volume.to_string(),
        });
        assert!(!is_clocked(volume));
    }

    /// Two drives covering at once are two runs, and one finishing says nothing
    /// about the other.
    #[test]
    fn one_drive_finishing_leaves_another_drives_run_alone() {
        let (first, second) = ("analytics-first-index-a", "analytics-first-index-b");
        observe(&scan_started(first, true));
        observe(&scan_started(second, true));

        observe(&scan_complete(first));

        assert!(!is_clocked(first));
        assert!(is_clocked(second), "❌ per-volume, like every other index invariant");
        observe(&scan_complete(second));
    }

    #[test]
    fn the_buckets_cover_their_ranges() {
        assert_eq!(short_bucket(Duration::from_secs(9)), "<10s");
        assert_eq!(short_bucket(Duration::from_secs(10)), "10-30s");
        assert_eq!(short_bucket(Duration::from_secs(299)), "2-5m");
        assert_eq!(short_bucket(Duration::from_secs(300)), "5m+");
        assert_eq!(long_bucket(Duration::from_secs(59)), "<1m");
        assert_eq!(long_bucket(Duration::from_secs(60)), "1-3m");
        assert_eq!(long_bucket(Duration::from_secs(1799)), "10-30m");
        assert_eq!(long_bucket(Duration::from_secs(1800)), "30m+");
    }
}
