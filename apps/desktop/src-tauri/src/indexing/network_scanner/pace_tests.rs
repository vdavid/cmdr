//! How the `Volume`-trait walk yields to navigation (`indexing/network_scanner/scan_pace.rs`).
//!
//! Split out of `tests.rs` because it's a distinct concern with its own setup:
//! these drive a controllable host policy rather than the walk's error/coverage
//! contracts. The pure budget decision is unit-tested in
//! `network_scanner/scan_pace.rs`; these prove the WALK actually honors it.
//!
//! Each test builds its own `FakeHostPolicy`, so nothing here touches a
//! process-global signal and parallel tests can't cross-talk. The per-volume SCOPE
//! of the real signals (browsing one share must not throttle another) is the host's
//! contract, proven in `priority::host_policy`'s tests and, at the decision, by
//! `scan_pace::tests::app_wide_activity_alone_does_not_throttle_a_quiet_share`.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use super::tests::{ConcurrencyTrackingVolume, entry, progress, wide_tree};
use super::{ScanPacer, scan_volume_via_trait};
use crate::indexing::host::policy::FakeHostPolicy;
use crate::indexing::network_scanner::scan_pace::FULL_LISTING_BUDGET;
use crate::indexing::scanner::ScanSummary;
use crate::indexing::store::{IndexStore, ROOT_ID};
use crate::indexing::writer::IndexWriter;
use cmdr_fs::volume::{InMemoryVolume, Volume};

/// A window far longer than any of these tests. The fake reports a decision rather
/// than a clock, so the value only has to be plausible at the call site.
const LONG_WINDOW: Duration = Duration::from_secs(60);

/// One writer over a fresh per-test DB.
fn writer_in(dir: &std::path::Path, name: &str) -> IndexWriter {
    let db_path = dir.join(name);
    let _store = IndexStore::open(&db_path).expect("open store");
    IndexWriter::spawn(&db_path, crate::indexing::NoopEventSink::shared()).expect("spawn writer")
}

/// A `wide_tree(n)` volume that records peak listing concurrency.
fn tracking_wide_tree(n_subdirs: usize, max_in_flight: &Arc<AtomicU64>) -> Arc<dyn Volume> {
    Arc::new(ConcurrencyTrackingVolume {
        inner: wide_tree(n_subdirs),
        in_flight: Arc::new(AtomicU64::new(0)),
        max_in_flight: Arc::clone(max_in_flight),
    })
}

/// Run a full trait scan of `vol` under `pacer`, flushing the writer.
async fn scan(vol: Arc<dyn Volume>, writer: &IndexWriter, pacer: ScanPacer) -> ScanSummary {
    let cancelled = Arc::new(AtomicBool::new(false));
    let summary = scan_volume_via_trait(vol, PathBuf::from("/"), writer.clone(), progress(), cancelled, pacer)
        .await
        .expect("scan completes");
    writer.flush().await.expect("flush");
    summary
}

/// THE navigation-responsiveness guard. While the user is browsing the share, the
/// walk must drop to ONE listing in flight, so a navigation queues behind a single
/// background round trip instead of a 64-deep backlog. (On a real QNAP, an
/// unthrottled scan made a 40-entry folder take 10.7 s to open.)
#[tokio::test]
async fn browsing_the_share_throttles_the_scan_to_one_listing_in_flight() {
    let dir = tempfile::tempdir().expect("temp dir");
    let writer = writer_in(dir.path(), "vol-scan-yield.db");
    let max_in_flight = Arc::new(AtomicU64::new(0));
    let vol = tracking_wide_tree(FULL_LISTING_BUDGET * 2, &max_in_flight);

    let host = FakeHostPolicy::shared();
    host.note_foreground_activity();
    let pacer = ScanPacer::with_policy("test://network_scanner/browsed", LONG_WINDOW, host);

    scan(vol, &writer, pacer).await;
    writer.shutdown();

    let max = max_in_flight.load(Ordering::SeqCst);
    assert_eq!(
        max, 1,
        "a browsed share must be scanned one listing at a time (max in flight = {max})"
    );
}

/// The other half of that guard: with nothing competing, the walk uses the wide
/// budget. Without this, every throttling assertion here would also pass on a walk
/// that had been serial all along.
#[tokio::test]
async fn a_clear_host_leaves_the_scan_at_full_speed() {
    let dir = tempfile::tempdir().expect("temp dir");
    let writer = writer_in(dir.path(), "vol-scan-clear.db");
    let max_in_flight = Arc::new(AtomicU64::new(0));
    let vol = tracking_wide_tree(FULL_LISTING_BUDGET * 2, &max_in_flight);

    let pacer = ScanPacer::with_policy(
        "test://network_scanner/untouched",
        LONG_WINDOW,
        FakeHostPolicy::shared(),
    );

    scan(vol, &writer, pacer).await;
    writer.shutdown();

    let max = max_in_flight.load(Ordering::SeqCst);
    assert!(
        max > 1,
        "nothing is competing, so the walk must run listings in parallel (max in flight = {max})"
    );
}

/// Transfers trump indexing (the priority order): while a user-initiated write
/// operation touches this share, the walk drops to ONE listing in flight — the same
/// yield shape as browsing, driven by the transfer gauge instead of the foreground
/// timestamp. The copy the user is watching gets the wire.
#[tokio::test]
async fn a_transfer_on_the_share_throttles_the_scan_to_one_listing_in_flight() {
    let dir = tempfile::tempdir().expect("temp dir");
    let writer = writer_in(dir.path(), "vol-scan-transfer-yield.db");
    let max_in_flight = Arc::new(AtomicU64::new(0));
    let vol = tracking_wide_tree(FULL_LISTING_BUDGET * 2, &max_in_flight);

    // A copy off this share is running for the whole scan; nobody is browsing.
    let host = FakeHostPolicy::shared();
    host.note_transfer_started();
    let pacer = ScanPacer::with_policy("test://network_scanner/transfer_busy", LONG_WINDOW, host);

    scan(vol, &writer, pacer).await;
    writer.shutdown();

    let max = max_in_flight.load(Ordering::SeqCst);
    assert_eq!(
        max, 1,
        "a share with a running transfer must be scanned one listing at a time (max in flight = {max})"
    );
}

/// THE anti-starvation guarantee, end to end: a user who never stops browsing must
/// not stop the scan. The throttled budget is 1, never 0, so the walk keeps making
/// forward progress and still indexes the whole tree — there is no floor or quota to
/// expire, because the scan is never fully parked.
#[tokio::test]
async fn a_continuously_browsed_share_still_finishes_its_scan() {
    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = dir.path().join("vol-scan-no-starve.db");
    let _store = IndexStore::open(&db_path).expect("open store");
    let writer = IndexWriter::spawn(&db_path, crate::indexing::NoopEventSink::shared()).expect("spawn writer");

    let n_subdirs = 40;
    let max_in_flight = Arc::new(AtomicU64::new(0));
    let vol = tracking_wide_tree(n_subdirs, &max_in_flight);

    // Someone arrow-keying through the share the whole time the scan runs: the host
    // never once reports it clear, at any top-up.
    let host = FakeHostPolicy::shared();
    host.note_foreground_activity();
    let pacer = ScanPacer::with_policy("test://network_scanner/never_quiet", LONG_WINDOW, host);

    let summary = scan(vol, &writer, pacer).await;
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

/// THE dispatch-rule guard for the host policy seam: the host is asked once per
/// LISTING, never once per entry.
///
/// A real scan visits millions of entries, so a policy question on the per-entry
/// path would put a `dyn` call and a lock acquisition on the hot path and defeat the
/// point of returning a cheap `Copy` snapshot. The fixture makes the two rates
/// impossible to confuse: four directories holding 250 files each, so entries
/// outnumber listings 200:1. Asking per entry would push the count past a thousand;
/// asking per top-up keeps it in the low tens.
#[tokio::test]
async fn the_policy_is_consulted_per_listing_not_per_entry() {
    let n_dirs = 4_u64;
    let files_per_dir = 250;

    let mut entries = Vec::new();
    for d in 0..n_dirs {
        entries.push(entry(&format!("d{d}"), &format!("/d{d}"), true, None));
        for f in 0..files_per_dir {
            entries.push(entry(&format!("f{f}"), &format!("/d{d}/f{f}"), false, Some(1_024)));
        }
    }
    let total_entries = entries.len() as u64;
    let listings = 1 + n_dirs; // the root, plus each subdirectory

    let dir = tempfile::tempdir().expect("temp dir");
    let writer = writer_in(dir.path(), "vol-scan-cadence.db");
    let vol: Arc<dyn Volume> = Arc::new(InMemoryVolume::with_entries("Test", entries));

    let host = FakeHostPolicy::shared();
    let pacer = ScanPacer::with_policy("test://network_scanner/cadence", LONG_WINDOW, host.clone());

    let summary = scan(vol, &writer, pacer).await;
    writer.shutdown();

    assert_eq!(summary.total_entries, total_entries, "the whole fixture was scanned");

    let asked = host.call_count() as u64;
    assert!(
        asked < total_entries,
        "the host must not be asked per entry ({asked} questions for {total_entries} entries)"
    );
    // The walk asks while topping up its in-flight set, so a handful of questions per
    // dispatched listing is expected; anything that scales with entries is not.
    let ceiling = listings * 10;
    assert!(
        asked <= ceiling,
        "policy questions must scale with listings, not entries \
         ({asked} questions for {listings} listings, ceiling {ceiling})"
    );
}
