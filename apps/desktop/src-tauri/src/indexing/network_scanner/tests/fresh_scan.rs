//! The fresh BFS walk: what it indexes, which dirs it stamps as listed, when it
//! refuses to claim completion, and how it honors cancellation.

use super::*;

/// Walk a small in-memory tree over the `Volume` trait and assert the index
/// reflects its contents: the writer/aggregator reuse is exercised end to
/// end (entries land under ROOT_ID, sizes flow into dir_stats). This is the
/// backend-agnostic half of the SMB-fixture integration test; the live SMB
/// scan is pinned by `smb_integration_volume_scan_indexes_share` (Docker).
#[tokio::test]
async fn scans_in_memory_tree_into_index() {
    use crate::indexing::writer::IndexWriter;

    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = dir.path().join("vol-scan.db");
    let _store = IndexStore::open(&db_path).expect("open store");
    let writer = IndexWriter::spawn(&db_path, crate::indexing::NoopEventSink::shared()).expect("spawn writer");

    // Build an in-memory volume with a known tree:
    //   /sub/         (dir)
    //   /sub/leaf.txt (11 bytes)
    //   /top.txt      (5 bytes)
    let vol = InMemoryVolume::with_entries(
        "Test",
        vec![
            entry("sub", "/sub", true, None),
            entry("leaf.txt", "/sub/leaf.txt", false, Some(11)),
            entry("top.txt", "/top.txt", false, Some(5)),
        ],
    );
    let vol: Arc<dyn Volume> = Arc::new(vol);

    let cancelled = Arc::new(AtomicBool::new(false));
    let summary = scan_volume_via_trait(
        vol,
        PathBuf::from("/"),
        writer.clone(),
        progress(),
        cancelled,
        ScanPacer::unpaced(),
    )
    .await
    .expect("scan should complete");

    assert!(!summary.was_cancelled);
    assert_eq!(summary.total_entries, 3, "2 files + 1 dir");
    assert_eq!(summary.total_dirs, 1);

    // Async test: await the flush rather than `flush_blocking` (which would
    // `block_on` the current runtime thread and panic).
    writer.flush().await.expect("flush");
    writer.shutdown();

    let store = IndexStore::open(&db_path).expect("reopen");
    let children = store.list_children(ROOT_ID).expect("list root");
    assert_eq!(children.len(), 2, "root has sub/ and top.txt");
    let sub = children.iter().find(|e| e.name == "sub").expect("sub dir present");
    assert!(sub.is_directory);
    let sub_children = store.list_children(sub.id).expect("list sub");
    assert_eq!(sub_children.len(), 1);
    assert_eq!(sub_children[0].name, "leaf.txt");
    assert_eq!(sub_children[0].logical_size, Some(11));
}

/// A subdir whose listing errors is NOT stamped (`listed_epoch` stays 0),
/// while its successfully-listed siblings (including an empty-but-listed dir)
/// and the root get the current epoch. The unit-level disconnect anchor.
#[tokio::test]
async fn errored_listing_is_not_marked() {
    use crate::indexing::writer::IndexWriter;

    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = dir.path().join("vol-scan-mark.db");
    let _store = IndexStore::open(&db_path).expect("open store");
    let writer = IndexWriter::spawn(&db_path, crate::indexing::NoopEventSink::shared()).expect("spawn writer");

    // Tree:
    //   /good/        (dir, lists fine, has one file)
    //   /good/a.txt
    //   /empty/       (dir, lists fine but empty → empty-but-listed)
    //   /bad/         (dir, listing ERRORS transiently → must stay listed_epoch=0)
    //   /bad/hidden   (file under bad; never discovered because bad won't list)
    let inner = InMemoryVolume::with_entries(
        "Test",
        vec![
            entry("good", "/good", true, None),
            entry("a.txt", "/good/a.txt", false, Some(7)),
            entry("empty", "/empty", true, None),
            entry("bad", "/bad", true, None),
            entry("hidden", "/bad/hidden", false, Some(3)),
        ],
    );
    let vol: Arc<dyn Volume> = Arc::new(FailingListVolume {
        inner,
        fail_path: PathBuf::from("/bad"),
    });

    let cancelled = Arc::new(AtomicBool::new(false));
    let summary = scan_volume_via_trait(
        vol,
        PathBuf::from("/"),
        writer.clone(),
        progress(),
        cancelled,
        ScanPacer::unpaced(),
    )
    .await
    .expect("scan should complete (a single bad subdir is skipped)");
    assert!(!summary.was_cancelled);

    writer.flush().await.expect("flush");
    writer.shutdown();

    let conn = IndexStore::open_read_connection(&db_path).expect("read conn");
    let epoch = IndexStore::read_current_epoch(&conn).expect("epoch");
    assert_eq!(epoch, 1, "first scan stamps epoch 1");

    let id_of = |p: &str| -> i64 { resolve_path(&conn, p).expect("resolve").expect("present") };

    // Root and the dirs that listed successfully (incl. empty) are stamped.
    assert_eq!(
        IndexStore::get_listed_epoch_by_id(&conn, ROOT_ID).expect("root epoch"),
        Some(1),
        "root listed",
    );
    assert_eq!(
        IndexStore::get_listed_epoch_by_id(&conn, id_of("/good")).expect("good epoch"),
        Some(1),
        "good listed",
    );
    assert_eq!(
        IndexStore::get_listed_epoch_by_id(&conn, id_of("/empty")).expect("empty epoch"),
        Some(1),
        "empty-but-listed dir is stamped",
    );

    // The errored subdir's row exists (parent listed it) but stays unlisted.
    assert_eq!(
        IndexStore::get_listed_epoch_by_id(&conn, id_of("/bad")).expect("bad epoch"),
        Some(0),
        "a dir whose own listing errored stays listed_epoch=0 (honest unknown)",
    );
}

/// THE speedup regression guard: the walk lists directories CONCURRENTLY, capped at
/// `FULL_LISTING_BUDGET`. With many sibling dirs queued, multiple `list_directory` round
/// trips are in flight at once — a revert to a serial walk would record a max of 1.
#[tokio::test]
async fn walk_lists_directories_concurrently() {
    use crate::indexing::writer::IndexWriter;

    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = dir.path().join("vol-scan-concurrency.db");
    let _store = IndexStore::open(&db_path).expect("open store");
    let writer = IndexWriter::spawn(&db_path, crate::indexing::NoopEventSink::shared()).expect("spawn writer");

    // Root with many empty subdirs (≫ FULL_LISTING_BUDGET): the root listing discovers
    // them all, then they list concurrently up to the cap.
    let in_flight = Arc::new(AtomicU64::new(0));
    let max_in_flight = Arc::new(AtomicU64::new(0));
    let vol: Arc<dyn Volume> = Arc::new(ConcurrencyTrackingVolume {
        inner: wide_tree(FULL_LISTING_BUDGET * 2),
        in_flight: Arc::clone(&in_flight),
        max_in_flight: Arc::clone(&max_in_flight),
    });

    let cancelled = Arc::new(AtomicBool::new(false));
    scan_volume_via_trait(
        vol,
        PathBuf::from("/"),
        writer.clone(),
        progress(),
        cancelled,
        ScanPacer::unpaced(),
    )
    .await
    .expect("scan completes");
    writer.flush().await.expect("flush");
    writer.shutdown();

    let max = max_in_flight.load(Ordering::SeqCst) as usize;
    assert!(
        max > 1,
        "the walk must list concurrently, not serially (max in flight = {max})"
    );
    assert!(
        max <= FULL_LISTING_BUDGET,
        "concurrency must stay capped at FULL_LISTING_BUDGET (max in flight = {max})",
    );
}

/// A fresh scan whose ROOT listing SUCCEEDS but returns ZERO children must
/// NOT report a clean completion: it returns the typed `EmptyRoot` error so
/// the completion handler leaves `scan_completed_at` unwritten. This is the
/// guard against the real-hardware bug where a NAS scan that walked nothing
/// stamped a false "complete" marker and stranded the index forever. (The
/// completion handler's persistence of the marker is asserted at the manager
/// level; here we pin the typed error the handler routes on.)
#[tokio::test]
async fn empty_root_fresh_scan_does_not_complete() {
    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = dir.path().join("vol-scan-empty-root.db");
    let _store = IndexStore::open(&db_path).expect("open store");
    let writer = IndexWriter::spawn(&db_path, crate::indexing::NoopEventSink::shared()).expect("spawn writer");

    // Root lists fine but has no children at all.
    let vol: Arc<dyn Volume> = Arc::new(InMemoryVolume::with_entries("Test", vec![]));

    let cancelled = Arc::new(AtomicBool::new(false));
    let result = scan_volume_via_trait(
        vol,
        PathBuf::from("/"),
        writer.clone(),
        progress(),
        cancelled,
        ScanPacer::unpaced(),
    )
    .await;

    match result {
        Err(VolumeScanError::EmptyRoot) => {}
        other => panic!("expected EmptyRoot (no completion), got {other:?}"),
    }
    // EmptyRoot is NOT a terminal disconnect: the completion handler discards
    // and resets to gray rather than keeping a "stale" empty partial.
    assert!(
        !VolumeScanError::EmptyRoot.is_terminal_disconnect(),
        "an empty root is a failed scan to discard, not an honest partial to keep",
    );

    writer.flush().await.expect("flush");
    writer.shutdown();
}

/// The root-fatal case stays fatal: a ROOT listing that ERRORS (not empty,
/// not a disconnect) surfaces the error so no completion marker is written.
/// Distinguishes "root listing FAILED" (`Volume`) from "root listed EMPTY"
/// (`EmptyRoot`) — both refuse completion, via different typed variants.
#[tokio::test]
async fn failed_root_listing_does_not_complete() {
    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = dir.path().join("vol-scan-root-fail.db");
    let _store = IndexStore::open(&db_path).expect("open store");
    let writer = IndexWriter::spawn(&db_path, crate::indexing::NoopEventSink::shared()).expect("spawn writer");

    let vol: Arc<dyn Volume> = Arc::new(RootFailsVolume {
        inner: InMemoryVolume::with_entries("Test", vec![entry("a.txt", "/a.txt", false, Some(1))]),
    });

    let cancelled = Arc::new(AtomicBool::new(false));
    let result = scan_volume_via_trait(
        vol,
        PathBuf::from("/"),
        writer.clone(),
        progress(),
        cancelled,
        ScanPacer::unpaced(),
    )
    .await;

    match result {
        Err(VolumeScanError::Volume(VolumeError::PermissionDenied(_))) => {}
        other => panic!("expected the root-fatal Volume error (no completion), got {other:?}"),
    }

    writer.flush().await.expect("flush");
    writer.shutdown();
}

/// A pre-set cancel flag stops the walk immediately and reports
/// `was_cancelled` (the caller then discards the partial — D-interrupted).
#[tokio::test]
async fn honors_cancellation_before_first_listing() {
    use crate::indexing::writer::IndexWriter;

    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = dir.path().join("vol-scan-cancel.db");
    let _store = IndexStore::open(&db_path).expect("open store");
    let writer = IndexWriter::spawn(&db_path, crate::indexing::NoopEventSink::shared()).expect("spawn writer");

    let vol = InMemoryVolume::with_entries("Test", vec![entry("a.txt", "/a.txt", false, Some(1))]);
    let vol: Arc<dyn Volume> = Arc::new(vol);

    let cancelled = Arc::new(AtomicBool::new(true));
    let summary = scan_volume_via_trait(
        vol,
        PathBuf::from("/"),
        writer.clone(),
        progress(),
        cancelled,
        ScanPacer::unpaced(),
    )
    .await
    .expect("cancelled scan still returns Ok");
    assert!(summary.was_cancelled);
    assert_eq!(summary.total_entries, 0, "nothing scanned after immediate cancel");

    writer.shutdown();
}
