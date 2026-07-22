//! How the `Volume`-trait walk yields to navigation (`indexing/network_scanner/scan_pace.rs`).
//!
//! Split out of `tests.rs` because it's a distinct concern with its own setup:
//! these drive the process-global foreground-activity tracker rather than the
//! walk's error/coverage contracts. The pure budget decision is unit-tested in
//! `network_scanner/scan_pace.rs`; these prove the WALK actually honors it.
//!
//! Each test uses a `test://` volume id unique to itself, so the process-global
//! tracker can't cross-talk between tests running in parallel.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use super::tests::{ConcurrencyTrackingVolume, progress, wide_tree};
use super::{ScanPacer, scan_volume_via_trait};
use crate::file_system::volume::Volume;
use crate::indexing::network_scanner::scan_pace::FULL_LISTING_BUDGET;
use crate::indexing::store::{IndexStore, ROOT_ID};

/// THE navigation-responsiveness guard. While the user is browsing the share, the
/// walk must drop to ONE listing in flight, so a navigation queues behind a single
/// background round trip instead of a 64-deep backlog. (On a real QNAP, an
/// unthrottled scan made a 40-entry folder take 10.7 s to open.)
#[tokio::test]
async fn browsing_the_share_throttles_the_scan_to_one_listing_in_flight() {
    use crate::indexing::writer::IndexWriter;

    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = dir.path().join("vol-scan-yield.db");
    let _store = IndexStore::open(&db_path).expect("open store");
    let writer = IndexWriter::spawn(&db_path, None).expect("spawn writer");

    let in_flight = Arc::new(AtomicU64::new(0));
    let max_in_flight = Arc::new(AtomicU64::new(0));
    let vol: Arc<dyn Volume> = Arc::new(ConcurrencyTrackingVolume {
        inner: wide_tree(FULL_LISTING_BUDGET * 2),
        in_flight: Arc::clone(&in_flight),
        max_in_flight: Arc::clone(&max_in_flight),
    });

    // The user just navigated this share. A long quiet window keeps it "busy" for
    // the whole (fast) test, so the assertion can't flake on timing.
    let volume_id = "test://network_scanner/browsed";
    crate::media_index::foreground::note_foreground_activity_on(volume_id);
    let pacer = ScanPacer::with_threshold(volume_id, Duration::from_secs(60));

    let cancelled = Arc::new(AtomicBool::new(false));
    scan_volume_via_trait(vol, PathBuf::from("/"), writer.clone(), progress(), cancelled, pacer)
        .await
        .expect("scan completes");
    writer.flush().await.expect("flush");
    writer.shutdown();

    let max = max_in_flight.load(Ordering::SeqCst) as usize;
    assert_eq!(
        max, 1,
        "a browsed share must be scanned one listing at a time (max in flight = {max})"
    );
}

/// THE anti-starvation guarantee, end to end: a user who never stops browsing must
/// not stop the scan. The throttled budget is 1, never 0, so the walk keeps making
/// forward progress and still indexes the whole tree — there is no floor or quota
/// to expire, because the scan is never fully parked.
#[tokio::test]
async fn a_continuously_browsed_share_still_finishes_its_scan() {
    use crate::indexing::writer::IndexWriter;

    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = dir.path().join("vol-scan-no-starve.db");
    let _store = IndexStore::open(&db_path).expect("open store");
    let writer = IndexWriter::spawn(&db_path, None).expect("spawn writer");

    let n_subdirs = 40;
    let in_flight = Arc::new(AtomicU64::new(0));
    let max_in_flight = Arc::new(AtomicU64::new(0));
    let vol: Arc<dyn Volume> = Arc::new(ConcurrencyTrackingVolume {
        inner: wide_tree(n_subdirs),
        in_flight: Arc::clone(&in_flight),
        max_in_flight: Arc::clone(&max_in_flight),
    });

    // Someone arrow-keying through the share the entire time the scan runs.
    let volume_id = "test://network_scanner/never_quiet";
    let stop = Arc::new(AtomicBool::new(false));
    let browsing = tokio::spawn({
        let stop = Arc::clone(&stop);
        let volume_id = volume_id.to_string();
        async move {
            while !stop.load(Ordering::Relaxed) {
                crate::media_index::foreground::note_foreground_activity_on(&volume_id);
                tokio::time::sleep(Duration::from_millis(2)).await;
            }
        }
    });

    let cancelled = Arc::new(AtomicBool::new(false));
    let summary = scan_volume_via_trait(
        vol,
        PathBuf::from("/"),
        writer.clone(),
        progress(),
        cancelled,
        // A window far longer than the test, so the share is busy at EVERY top-up.
        ScanPacer::with_threshold(volume_id, Duration::from_secs(60)),
    )
    .await
    .expect("a throttled scan must still complete, not stall forever");

    stop.store(true, Ordering::Relaxed);
    browsing.await.expect("browsing task");
    writer.flush().await.expect("flush");
    writer.shutdown();

    assert_eq!(
        summary.total_entries, n_subdirs as u64,
        "every directory must be indexed despite non-stop browsing"
    );
    assert_eq!(
        max_in_flight.load(Ordering::SeqCst),
        1,
        "…and it got there at the throttled pace, so this really is the yielding path"
    );

    let store = IndexStore::open(&db_path).expect("reopen");
    assert_eq!(store.list_children(ROOT_ID).expect("list root").len(), n_subdirs);
}

/// The SCOPE decision: the contention is one share's SMB session, so browsing a
/// DIFFERENT volume (a local folder, another share) must not slow this scan down.
#[tokio::test]
async fn browsing_a_different_volume_does_not_throttle_the_scan() {
    use crate::indexing::writer::IndexWriter;

    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = dir.path().join("vol-scan-scope.db");
    let _store = IndexStore::open(&db_path).expect("open store");
    let writer = IndexWriter::spawn(&db_path, None).expect("spawn writer");

    let in_flight = Arc::new(AtomicU64::new(0));
    let max_in_flight = Arc::new(AtomicU64::new(0));
    let vol: Arc<dyn Volume> = Arc::new(ConcurrencyTrackingVolume {
        inner: wide_tree(FULL_LISTING_BUDGET * 2),
        in_flight: Arc::clone(&in_flight),
        max_in_flight: Arc::clone(&max_in_flight),
    });

    // The user is busy in a local folder; the share being scanned is untouched.
    crate::media_index::foreground::note_foreground_activity_on("root");
    let pacer = ScanPacer::with_threshold("test://network_scanner/untouched", Duration::from_secs(60));

    let cancelled = Arc::new(AtomicBool::new(false));
    scan_volume_via_trait(vol, PathBuf::from("/"), writer.clone(), progress(), cancelled, pacer)
        .await
        .expect("scan completes");
    writer.flush().await.expect("flush");
    writer.shutdown();

    let max = max_in_flight.load(Ordering::SeqCst) as usize;
    assert!(
        max > 1,
        "browsing another volume must leave this scan at full speed (max in flight = {max})"
    );
}
