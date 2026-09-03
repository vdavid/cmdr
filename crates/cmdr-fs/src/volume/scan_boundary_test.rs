//! The copy-scan boundary's two contracts: what it counts, and what it stops.
//!
//! It lives here rather than beside a backend because every remote backend
//! shares the one boundary: a per-backend copy would let the cumulative-counts
//! promise drift between them, and a per-backend stop would let one of them
//! quietly not have one.

use super::*;
use crate::volume::ScanStopSignal;
use crate::volume::scan_stop::TestScanStop;
use std::sync::Arc;

/// The counts a scan reports must be CUMULATIVE for the call: the caller shifts
/// them by its own baseline across several calls, so a per-entry (or per-path)
/// reset would make the dialog's counters jump backwards.
#[tokio::test]
async fn the_boundary_reports_running_totals() {
    use crate::ignore_poison::IgnorePoison;
    use std::sync::Mutex;

    let seen: Mutex<Vec<ListingProgress>> = Mutex::new(Vec::new());
    let record = |p: ListingProgress| seen.lock_ignore_poison().push(p);
    let boundary = ScanBoundary::new(Some(&record));

    boundary.dir().await.expect("nothing is stopping this walk");
    boundary.file(1_000).await.expect("nothing is stopping this walk");
    boundary.file(24).await.expect("nothing is stopping this walk");

    let seen = seen.lock_ignore_poison();
    assert_eq!(seen.len(), 3, "every entry reports, so a slow walk still looks alive");
    assert_eq!((seen[0].files, seen[0].dirs, seen[0].bytes), (0, 1, 0));
    assert_eq!((seen[1].files, seen[1].dirs, seen[1].bytes), (1, 1, 1_000));
    assert_eq!((seen[2].files, seen[2].dirs, seen[2].bytes), (2, 1, 1_024));
}

/// A scan with nobody listening still counts, so `scan_for_copy` (which the
/// trait gives no callback) can share one implementation with the batch path.
#[tokio::test]
async fn a_silent_boundary_still_counts() {
    let boundary = ScanBoundary::silent();
    boundary.dir().await.expect("a silent boundary stops nothing");
    boundary.file(512).await.expect("a silent boundary stops nothing");

    let counts = boundary.counts();
    assert_eq!((counts.files, counts.dirs, counts.bytes), (1, 1, 512));
}

/// Both entry calls carry the stop, so a walk can't reach one shape of entry
/// without passing the boundary.
#[tokio::test]
async fn a_stopping_boundary_refuses_both_kinds_of_entry() {
    let signal = TestScanStop::already_stopping();
    let stop = ScanStop::new(Arc::clone(&signal) as Arc<dyn ScanStopSignal>);

    let boundary = ScanBoundary::silent().stopping_at(stop.clone());
    assert!(
        matches!(boundary.dir().await, Err(VolumeError::Cancelled(_))),
        "a directory boundary must refuse"
    );
    assert!(
        matches!(boundary.file(1).await, Err(VolumeError::Cancelled(_))),
        "a file boundary must refuse"
    );
    assert!(
        matches!(ScanBoundary::silent().stopping_at(stop).check().await, Err(VolumeError::Cancelled(_))),
        "and so must the bare check a backend uses between groups"
    );
}

/// The counts still climb across a stopped boundary: a caller that reports what
/// it got so far (the watchdog, the dialog) reads the same field either way.
#[tokio::test]
async fn a_stopped_boundary_still_counted_the_entry_it_refused() {
    let signal = TestScanStop::already_stopping();
    let boundary = ScanBoundary::silent().stopping_at(ScanStop::new(signal as Arc<dyn ScanStopSignal>));

    let _ = boundary.file(64).await;
    assert_eq!(boundary.counts().files, 1);
    assert_eq!(boundary.counts().bytes, 64);
}

/// `stop()` hands the signal out for a walk that can't `.await` — the local
/// backend's blocking `WalkDir` loop.
#[tokio::test]
async fn the_boundary_hands_its_stop_to_a_blocking_walk() {
    let signal = TestScanStop::already_stopping();
    let boundary = ScanBoundary::silent().stopping_at(ScanStop::new(signal as Arc<dyn ScanStopSignal>));
    assert!(boundary.stop().should_stop_blocking());
    assert!(!ScanBoundary::silent().stop().should_stop_blocking());
}
