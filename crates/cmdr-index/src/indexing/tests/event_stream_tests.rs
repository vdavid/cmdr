//! What a scan tells its host, asserted at the sink.
//!
//! These drive a REAL `IndexManager` over a temp-dir fixture with a
//! `RecordingSink` in place of the app, which is only possible because nothing in
//! the scan path names Tauri any more. They pin two things: the scan lifecycle's
//! shape, and that two concurrent volumes produce two independent streams (the
//! subsystem's one cross-area invariant, now visible at the boundary).

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crate::indexing::events::{IndexEventKind, RecordingSink};
use crate::indexing::lifecycle::manager::IndexManager;
use crate::indexing::lifecycle::progress_reporter::ScanProgressReporter;
use crate::indexing::lifecycle::state::VolumeSignals;
use crate::indexing::scanner::ScanProgress;
use crate::indexing::volume::IndexVolumeKind;
use crate::indexing::writer::{AggSource, IndexWriter};
use cmdr_fs::testing::wait_until_async;

/// A temp volume root with a couple of files and a subdirectory, plus the temp
/// dir the index DB lives in. Both are returned so the caller keeps them alive.
fn fixture_volume(tag: &str) -> (tempfile::TempDir, tempfile::TempDir) {
    let root = tempfile::tempdir().expect("volume root");
    let data = tempfile::tempdir().expect("index data dir");
    std::fs::write(root.path().join("a.txt"), format!("{tag} a")).expect("write a");
    std::fs::write(root.path().join("b.txt"), format!("{tag} b")).expect("write b");
    let sub = root.path().join("sub");
    std::fs::create_dir(&sub).expect("mkdir sub");
    std::fs::write(sub.join("c.txt"), format!("{tag} c")).expect("write c");
    (root, data)
}

/// Build a manager for `volume_id` over `root`, reporting into `events`.
fn manager_for(
    volume_id: &str,
    root: &std::path::Path,
    data_dir: &std::path::Path,
    events: Arc<RecordingSink>,
) -> IndexManager {
    IndexManager::new_for_kind(
        volume_id.to_string(),
        root.to_path_buf(),
        data_dir.join(format!("index-{volume_id}.db")),
        // A mount-rooted local drive: the guarded walker, scoped to the fixture
        // root rather than to `/`.
        IndexVolumeKind::LocalExternal,
        true,
        VolumeSignals::new(Arc::new(std::sync::Mutex::new(None)), events),
    )
    .expect("build the index manager")
}

/// Wait for `volume_id`'s stream to reach `ScanComplete`.
async fn wait_for_scan_complete(events: &RecordingSink, volume_id: &str) {
    wait_until_async(Duration::from_secs(20), "the fixture scan to complete", || {
        events.kinds_for(volume_id).contains(&IndexEventKind::ScanComplete)
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_fixture_scan_reports_started_then_complete_for_its_volume() {
    let (root, data) = fixture_volume("solo");
    let events = Arc::new(RecordingSink::new());
    let mut manager = manager_for("evt-solo", root.path(), data.path(), Arc::clone(&events));

    manager.start_scan("event stream test").expect("start the scan");
    wait_for_scan_complete(&events, "evt-solo").await;
    manager.stop_scan();

    let kinds = events.kinds_for("evt-solo");
    assert_eq!(
        kinds.first(),
        Some(&IndexEventKind::ScanStarted),
        "a scan announces itself before anything else: {kinds:?}"
    );
    let complete_at = kinds
        .iter()
        .position(|k| *k == IndexEventKind::ScanComplete)
        .expect("a completed scan reports ScanComplete");
    assert!(
        complete_at > 0,
        "ScanComplete must follow ScanStarted, not lead it: {kinds:?}"
    );
    assert!(
        kinds[..complete_at].contains(&IndexEventKind::PhaseChanged),
        "the pipeline's phase transitions ride the same stream: {kinds:?}"
    );
    // Progress ticks every 500 ms, so a fixture this small may finish before the
    // first one; what must always hold is that any tick falls INSIDE the scan.
    // `the_progress_reporter_reports_progress_for_its_own_volume` pins that ticks
    // happen at all.
    for (i, kind) in kinds.iter().enumerate() {
        if *kind == IndexEventKind::ScanProgress {
            assert!(
                i > 0 && i < complete_at,
                "progress must fall between ScanStarted and ScanComplete: {kinds:?}"
            );
        }
    }
    // Nothing in this scan may speak for another volume.
    for event in events.events() {
        if let Some(vid) = event.volume_id() {
            assert_eq!(vid, "evt-solo", "a scan reported under the wrong volume: {event:?}");
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn the_progress_reporter_reports_progress_for_its_own_volume() {
    // The reporter's first tick lands after its 500 ms interval, so a fixture
    // scan usually finishes before one fires. Drive the reporter directly to pin
    // that a tick reports `ScanProgress` under the scanned volume's id.
    let (_root, data) = fixture_volume("tick");
    let db_path = data.path().join("index-evt-tick.db");
    // Opening the store creates the schema the writer's queue-depth probe reads.
    let _store = crate::indexing::store::IndexStore::open(&db_path).expect("open store");
    let writer = IndexWriter::spawn(&db_path, crate::NoopEventSink::shared()).expect("spawn writer");
    let events = Arc::new(RecordingSink::new());
    let scan_done = Arc::new(AtomicBool::new(false));

    let reporter = ScanProgressReporter::new(
        Arc::new(ScanProgress::new()),
        writer.clone(),
        Arc::clone(&events) as Arc<dyn crate::EventSink>,
        "evt-tick".to_string(),
        AggSource::Maps,
    );
    let handle = reporter.spawn(Arc::clone(&scan_done));

    wait_until_async(Duration::from_secs(5), "the reporter's first progress tick", || {
        events.kinds_for("evt-tick").contains(&IndexEventKind::ScanProgress)
    })
    .await;

    scan_done.store(true, Ordering::Relaxed);
    handle.abort();
    writer.shutdown();
}

#[tokio::test(flavor = "multi_thread")]
async fn two_concurrent_scans_produce_two_independent_streams() {
    // Per-volume isolation is the subsystem's one cross-area invariant. At the
    // event boundary it means: each volume's sink sees its own scan and nothing
    // of the other's, even though the two scans overlap in time.
    let (root_a, data_a) = fixture_volume("alpha");
    let (root_b, data_b) = fixture_volume("beta");
    let events_a = Arc::new(RecordingSink::new());
    let events_b = Arc::new(RecordingSink::new());

    let mut manager_a = manager_for("evt-alpha", root_a.path(), data_a.path(), Arc::clone(&events_a));
    let mut manager_b = manager_for("evt-beta", root_b.path(), data_b.path(), Arc::clone(&events_b));

    manager_a.start_scan("event stream test").expect("start scan a");
    manager_b.start_scan("event stream test").expect("start scan b");

    wait_for_scan_complete(&events_a, "evt-alpha").await;
    wait_for_scan_complete(&events_b, "evt-beta").await;
    manager_a.stop_scan();
    manager_b.stop_scan();

    for (sink, own, other) in [
        (&events_a, "evt-alpha", "evt-beta"),
        (&events_b, "evt-beta", "evt-alpha"),
    ] {
        assert!(
            sink.kinds_for(own).contains(&IndexEventKind::ScanComplete),
            "{own} must complete on its own stream"
        );
        assert!(
            sink.kinds_for(other).is_empty(),
            "{own}'s sink must never hear about {other}"
        );
        for event in sink.events() {
            if let Some(vid) = event.volume_id() {
                assert_eq!(vid, own, "{own}'s stream carried a foreign volume: {event:?}");
            }
        }
    }
}
