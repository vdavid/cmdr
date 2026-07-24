//! Tests for the index writer thread: the `WriteMessage` protocol, the
//! `IndexWriter` handle, backpressure/queue-depth accounting, mutation-generation
//! bumps, and the writer-loop dispatch. The shared helpers (`setup_db`, `open_read`)
//! live here and are imported by each writer submodule's `tests`. Extracted verbatim
//! from `writer/mod.rs`'s `tests` module; pure code movement.
use super::*;
use crate::indexing::lifecycle::state::IndexVolumeKind;
use crate::indexing::store::{EntryRow, IndexStore, ROOT_ID};
use crate::indexing::stress_test_helpers::TestInstanceGuard;
use crate::test_support::wait_until;

/// The budget for the end-of-iteration hooks (tracker clear, queue drain) to land after a
/// `flush_blocking`. `flush_blocking` replies from inside `process_message`, so the hooks run one
/// loop iteration later; that gap is microseconds, and 1 s means a genuine regression.
const DRAIN_LANDS_WITHIN: Duration = Duration::from_secs(1);

// ── Search-generation gating (D7: search is single-volume / root-only) ──

/// A search-feeding (root) writer's mutation bumps BOTH its per-writer
/// counter and the global `WRITER_GENERATION` the in-memory search index
/// watches. This is the only writer that may invalidate the search index.
///
/// Asserts on the tracker's own `global_generation_bumps` probe, not the
/// process-global `WRITER_GENERATION`: the per-writer count is immune to other
/// concurrent writers in the same test binary (see that field's doc), so this
/// passes under both `cargo test` (threads-in-one-process) and `nextest`.
#[test]
fn search_feeding_tracker_bumps_global_generation() {
    let tracker = MutationTracker::new(true);
    tracker.bump();
    assert_eq!(tracker.count(), 1, "the per-writer counter always ticks");
    assert_eq!(
        tracker.global_generation_bumps(),
        1,
        "a root (search-feeding) mutation bumps the global search generation"
    );
}

/// A non-search-feeding (SMB/MTP) writer's mutation ticks ONLY its own
/// counter and must leave the global `WRITER_GENERATION` untouched — an
/// SMB/MTP write must never invalidate the root search index it doesn't
/// feed (else every NAS/phone change-notify event thrashes a full root
/// search reload). This is the search-isolation guarantee.
///
/// Asserts on the tracker's own `global_generation_bumps` probe, not the
/// process-global `WRITER_GENERATION`: the per-writer count is immune to
/// concurrent feeding writers in other tests (see that field's doc).
#[test]
fn non_search_feeding_tracker_does_not_bump_global_generation() {
    let tracker = MutationTracker::new(false);
    tracker.bump();
    tracker.bump();
    tracker.bump();
    assert_eq!(tracker.count(), 3, "the per-writer counter still ticks for SMB/MTP");
    assert_eq!(
        tracker.global_generation_bumps(),
        0,
        "a non-root (SMB/MTP) mutation must NOT bump the root search generation"
    );
}

/// A spawned non-feeding writer (the real SMB/MTP path) must not bump the
/// global generation when it actually processes a mutating message end to
/// end (covers the `spawn_for(.., false)` → `writer_loop` → handler →
/// `MutationTracker::bump` wiring, not just the tracker in isolation).
#[test]
fn spawned_non_feeding_writer_does_not_bump_global_generation() {
    let (db_path, _dir) = setup_db();
    let writer = IndexWriter::spawn_for(&db_path, None, false, "root".to_string()).unwrap();

    writer
        .send(WriteMessage::InsertEntriesV2(vec![EntryRow {
            id: 10,
            parent_id: ROOT_ID,
            name: "smb-file.txt".into(),
            is_directory: false,
            is_symlink: false,
            logical_size: Some(5),
            physical_size: Some(5),
            modified_at: None,
            inode: None,
        }]))
        .unwrap();
    writer.flush_blocking().unwrap();

    assert_eq!(
        writer.mutation_count(),
        1,
        "the SMB/MTP writer did process the mutation (its own counter moved)"
    );
    assert_eq!(
        writer.global_generation_bumps(),
        0,
        "the SMB/MTP writer's mutation must not bump the root search generation"
    );
    writer.shutdown();
}

/// `MarkDirsListed` must NOT bump the global search generation, even on the
/// search-feeding (root) writer: stamping coverage changes nothing search
/// indexes, so a scan's marks must not thrash a full root-search reload (N4).
/// It still does its work (the row's `listed_epoch` is stamped).
#[test]
fn mark_dirs_listed_does_not_bump_global_generation() {
    let (db_path, _dir) = setup_db();
    // A root (search-feeding) writer — the one that WOULD bump on a mutation.
    let writer = IndexWriter::spawn_for(&db_path, None, true, "root".to_string()).unwrap();

    // Insert a dir to stamp, then flush so its row is committed.
    writer
        .send(WriteMessage::InsertEntriesV2(vec![EntryRow {
            id: 10,
            parent_id: ROOT_ID,
            name: "dir".into(),
            is_directory: true,
            is_symlink: false,
            logical_size: None,
            physical_size: None,
            modified_at: None,
            inode: None,
        }]))
        .unwrap();
    writer.flush_blocking().unwrap();

    // This writer's own bump count after the insert (per-writer, so unaffected by
    // other concurrent tests). MarkDirsListed must not move it.
    let before = writer.global_generation_bumps();
    writer
        .send(WriteMessage::MarkDirsListed {
            ids: vec![10],
            epoch: 4,
        })
        .unwrap();
    writer.flush_blocking().unwrap();
    let after = writer.global_generation_bumps();

    assert_eq!(
        before, after,
        "MarkDirsListed must not bump the search generation (it's not a search-relevant mutation)"
    );

    // It still stamped the row.
    let conn = IndexStore::open_read_connection(&db_path).unwrap();
    assert_eq!(
        IndexStore::get_listed_epoch_by_id(&conn, 10).unwrap(),
        Some(4),
        "the mark was actually applied",
    );
    writer.shutdown();
}

/// `BumpCurrentEpoch` persists the next epoch and, like `MarkDirsListed`,
/// must NOT bump the global search generation (a meta-only write touches
/// nothing search indexes). Round-trips: a fresh DB reads epoch 1, one bump
/// makes it 2.
#[test]
fn bump_current_epoch_persists_and_does_not_bump_global_generation() {
    let (db_path, _dir) = setup_db();
    let writer = IndexWriter::spawn_for(&db_path, None, true, "root".to_string()).unwrap();

    let conn = IndexStore::open_read_connection(&db_path).unwrap();
    assert_eq!(
        IndexStore::read_current_epoch(&conn).unwrap(),
        1,
        "a fresh DB reads as epoch 1 (absent ⇒ 1)",
    );

    // Per-writer bump count (immune to other concurrent writers): BumpCurrentEpoch,
    // a meta-only write, must leave it unchanged.
    let before = writer.global_generation_bumps();
    writer.send(WriteMessage::BumpCurrentEpoch).unwrap();
    writer.flush_blocking().unwrap();
    let after = writer.global_generation_bumps();

    assert_eq!(
        before, after,
        "BumpCurrentEpoch must not bump the search generation (meta-only write)"
    );
    assert_eq!(
        IndexStore::read_current_epoch(&conn).unwrap(),
        2,
        "one bump takes the epoch from 1 to 2",
    );
    writer.shutdown();
}

/// Freshness-layer consistency: the per-volume `Freshness` badge stays consistent with
/// `root.min_subtree_epoch == current_epoch ⇒ Fresh` (modulo Scanning). This
/// pins the data-layer half of that invariant — that a clean scan leaves the
/// root's coverage epoch EQUAL to `current_epoch` (Fresh-consistent), and a
/// continuity-break bump makes it STRICTLY LESS (Stale-consistent) — so the
/// two layers can't silently drift. The freshness-enum half is pinned in
/// `state::tests::disconnect_keeps_instance_stale_user_cancel_resets_to_gray`.
#[test]
fn root_coverage_epoch_tracks_current_epoch_across_a_continuity_break() {
    use crate::indexing::store::ROOT_ID;

    let (db_path, _dir) = setup_db();
    let writer = IndexWriter::spawn_for(&db_path, None, false, "root".to_string()).unwrap();

    // A clean scan stamps the root listed at the current epoch, then
    // aggregates. (One root dir, no children: a fully-covered tree.)
    writer
        .send(WriteMessage::MarkDirsListed {
            ids: vec![ROOT_ID],
            epoch: 1,
        })
        .unwrap();
    writer
        .send(WriteMessage::ComputeAllAggregates {
            source: AggSource::Maps,
        })
        .unwrap();
    writer.flush_blocking().unwrap();

    let conn = IndexStore::open_read_connection(&db_path).unwrap();
    let current = IndexStore::read_current_epoch(&conn).unwrap();
    let root_cov = IndexStore::get_dir_stats_by_id(&conn, ROOT_ID)
        .unwrap()
        .unwrap()
        .min_subtree_epoch;
    assert_eq!(
        root_cov, current,
        "after a clean scan, root coverage == current_epoch ⇒ Fresh-consistent"
    );

    // A continuity break bumps current_epoch; the root's coverage doesn't move
    // (no rescan stamped it), so it's now strictly behind ⇒ Stale-consistent.
    writer.send(WriteMessage::BumpCurrentEpoch).unwrap();
    writer.flush_blocking().unwrap();
    let bumped = IndexStore::read_current_epoch(&conn).unwrap();
    let root_cov_after = IndexStore::get_dir_stats_by_id(&conn, ROOT_ID)
        .unwrap()
        .unwrap()
        .min_subtree_epoch;
    assert!(
        root_cov_after < bumped,
        "after a continuity-break bump, root coverage ({root_cov_after}) < current_epoch ({bumped}) ⇒ Stale-consistent"
    );

    writer.shutdown();
}

/// Create a temp DB, open the store (to init schema), and return the path + temp dir guard.
pub(super) fn setup_db() -> (PathBuf, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("failed to create temp dir");
    let db_path = dir.path().join("test-writer.db");
    let _store = IndexStore::open(&db_path).expect("failed to open store");
    (db_path, dir)
}

/// Open a read connection to the DB for assertions.
pub(super) fn open_read(db_path: &Path) -> IndexStore {
    IndexStore::open(db_path).expect("failed to open read store")
}

// ── Basic lifecycle tests ────────────────────────────────────────

#[test]
fn spawn_and_shutdown() {
    let (db_path, _dir) = setup_db();
    let writer = IndexWriter::spawn(&db_path, None).unwrap();
    writer.shutdown();
    // Further sends should fail
    let result = writer.send(WriteMessage::Shutdown);
    // Might succeed or fail depending on timing, but shouldn't panic
    let _ = result;
}

/// A NON-root writer draining its queue routes its pending-sizes clear to ITS OWN
/// volume's tracker (`get_pending_sizes_for(volume_id)`), never the root-only
/// `get_pending_sizes`. Pre-fix the drain called the root-only lookup from every
/// volume's writer, so a non-root drain wiped root's hourglass early AND non-root
/// trackers never cleared.
///
/// Register the writer's volume with its own instance (see [`TestInstanceGuard`]),
/// mark a path, drain, and assert the writer CLEARED that tracker. A root-only
/// lookup would never reach this non-root instance's tracker, so the mark would
/// survive and the assertion would fail.
#[test]
fn non_root_writer_drain_clears_its_own_tracker_not_root() {
    use crate::indexing::lifecycle::state::get_instance_pending_sizes;

    let volume_id = "smb://writer-test-nonroot";
    let (db_path, _dir) = setup_db();
    let writer = IndexWriter::spawn_for(&db_path, None, false, volume_id.to_string()).unwrap();
    let instance = TestInstanceGuard::register(volume_id, &db_path, IndexVolumeKind::Smb);

    assert!(
        Arc::ptr_eq(
            &get_instance_pending_sizes(volume_id).expect("instance tracker registered"),
            &instance.tracker
        ),
        "the writer's volume resolves to the tracker we registered"
    );

    instance.tracker.mark("/aaa/bbb/ccc");
    assert!(
        instance.tracker.is_pending("/aaa/bbb/ccc"),
        "mark registers before the drain"
    );

    // Drain the non-root writer. The end-of-iteration clear hook resolves the
    // tracker by volume id and clears its transient marks once the queue empties.
    writer.flush_blocking().unwrap();
    wait_until(
        DRAIN_LANDS_WITHIN,
        "the non-root writer's drain to clear ITS OWN tracker (routed by volume id, not root-only)",
        || !instance.tracker.is_pending("/aaa/bbb/ccc"),
    );

    writer.shutdown();
}

/// The writer clears the transient "size updating" marks once its queue drains to
/// empty (the hourglass turns off when the indexer catches up), but a HELD rescan
/// root outlives the wholesale clear — the held tier exists precisely so a
/// seconds-long coalesced rescan's hourglass isn't dropped every time the writer
/// oscillates empty mid-walk. Pins both halves against `PendingSizes::clear`.
///
/// Runs against a per-volume instance (see [`TestInstanceGuard`]) rather than the
/// process-global root `PENDING_SIZES`, so it's immune to concurrent root writers
/// clearing the shared tracker (and never poisons `PENDING_SIZES_TEST_MUTEX`).
#[test]
fn writer_drain_clears_transient_marks_but_preserves_held_roots() {
    let volume_id = "smb://writer-test-held-roots";
    let (db_path, _dir) = setup_db();
    let writer = IndexWriter::spawn_for(&db_path, None, false, volume_id.to_string()).unwrap();
    let instance = TestInstanceGuard::register(volume_id, &db_path, IndexVolumeKind::Smb);

    // A transient mark (dropped wholesale on drain) and a held rescan root (kept).
    instance.tracker.mark("/aaa/bbb/ccc");
    instance.tracker.hold("/aaa/rescan");
    assert!(
        instance.tracker.is_pending("/aaa/bbb/ccc"),
        "the transient mark registers before the drain"
    );
    assert!(
        instance.tracker.is_pending("/aaa/rescan"),
        "the held root registers before the drain"
    );

    // Drain to empty: the end-of-iteration hook clears transient marks wholesale.
    writer.flush_blocking().unwrap();
    wait_until(
        DRAIN_LANDS_WITHIN,
        "the drain to clear the transient mark once the writer queue emptied",
        || !instance.tracker.is_pending("/aaa/bbb/ccc"),
    );
    assert!(
        instance.tracker.is_pending("/aaa/rescan"),
        "the held rescan root survives the wholesale clear (the whole point of the held tier)"
    );

    writer.shutdown();
}

#[test]
fn get_entry_count_via_writer() {
    let (db_path, _dir) = setup_db();
    let writer = IndexWriter::spawn(&db_path, None).unwrap();

    // Insert using integer-keyed API (simpler, no path resolution needed)
    let entries = vec![
        EntryRow {
            id: 10,
            parent_id: ROOT_ID,
            name: "a".into(),
            is_directory: true,
            is_symlink: false,
            logical_size: None,
            physical_size: None,
            modified_at: None,
            inode: None,
        },
        EntryRow {
            id: 11,
            parent_id: 10,
            name: "b.txt".into(),
            is_directory: false,
            is_symlink: false,
            logical_size: Some(100),
            physical_size: Some(100),
            modified_at: None,
            inode: None,
        },
    ];
    writer.send(WriteMessage::InsertEntriesV2(entries)).unwrap();
    writer.flush_blocking().unwrap();

    let (tx, rx) = oneshot::channel();
    writer.send(WriteMessage::GetEntryCount(tx)).unwrap();

    let count = rx.blocking_recv().unwrap().unwrap();
    // 2 inserted + 1 root sentinel = 3
    assert_eq!(count, 3);

    writer.shutdown();
}

#[test]
fn update_meta_via_writer() {
    let (db_path, _dir) = setup_db();
    let writer = IndexWriter::spawn(&db_path, None).unwrap();

    writer
        .send(WriteMessage::UpdateMeta {
            key: "test_key".into(),
            value: "test_value".into(),
        })
        .unwrap();
    writer.flush_blocking().unwrap();

    let store = open_read(&db_path);
    let status = store.get_index_status().unwrap();
    // test_key is not in IndexStatus struct, read directly via connection
    let conn = IndexStore::open_write_connection(&db_path).unwrap();
    let val = IndexStore::get_meta(&conn, "test_key").unwrap();
    assert_eq!(val.as_deref(), Some("test_value"));
    drop(store);
    drop(status);

    writer.shutdown();
}

#[test]
fn update_meta_total_physical_bytes_round_trip() {
    let (db_path, _dir) = setup_db();
    let writer = IndexWriter::spawn(&db_path, None).unwrap();

    writer
        .send(WriteMessage::UpdateMeta {
            key: "total_physical_bytes".into(),
            value: "123456789".into(),
        })
        .unwrap();
    writer.flush_blocking().unwrap();

    let conn = IndexStore::open_write_connection(&db_path).unwrap();
    let val = IndexStore::get_meta(&conn, "total_physical_bytes").unwrap();
    assert_eq!(val.as_deref(), Some("123456789"));

    writer.shutdown();
}

#[test]
fn delete_meta_via_writer_clears_key() {
    let (db_path, _dir) = setup_db();
    let writer = IndexWriter::spawn(&db_path, None).unwrap();

    // Set, then delete, then expect the key to read back as None.
    writer
        .send(WriteMessage::UpdateMeta {
            key: "scan_completed_at".into(),
            value: "1700000000".into(),
        })
        .unwrap();
    writer.flush_blocking().unwrap();

    let conn = IndexStore::open_write_connection(&db_path).unwrap();
    assert_eq!(
        IndexStore::get_meta(&conn, "scan_completed_at").unwrap().as_deref(),
        Some("1700000000")
    );

    writer
        .send(WriteMessage::DeleteMeta("scan_completed_at".into()))
        .unwrap();
    writer.flush_blocking().unwrap();

    assert_eq!(
        IndexStore::get_meta(&conn, "scan_completed_at").unwrap(),
        None,
        "DeleteMeta must remove the key entirely"
    );

    // Deleting an absent key is a harmless no-op.
    writer.send(WriteMessage::DeleteMeta("never_set".into())).unwrap();
    writer.flush_blocking().unwrap();
    assert_eq!(IndexStore::get_meta(&conn, "never_set").unwrap(), None);

    writer.shutdown();
}

#[tokio::test]
async fn flush_confirms_prior_writes() {
    let (db_path, _dir) = setup_db();
    let writer = IndexWriter::spawn(&db_path, None).unwrap();

    // Insert using integer-keyed API
    let entries = vec![EntryRow {
        id: 10,
        parent_id: ROOT_ID,
        name: "test.txt".into(),
        is_directory: false,
        is_symlink: false,
        logical_size: Some(512),
        physical_size: Some(512),
        modified_at: Some(1700000000),
        inode: None,
    }];
    writer.send(WriteMessage::InsertEntriesV2(entries)).unwrap();
    writer.flush().await.unwrap();

    // Data should be readable immediately after flush
    let store = open_read(&db_path);
    let children = store.list_children(ROOT_ID).unwrap();
    assert_eq!(children.len(), 1);
    assert_eq!(children[0].name, "test.txt");
    assert_eq!(children[0].logical_size, Some(512));

    writer.shutdown();
}

#[test]
fn update_last_event_id_via_writer() {
    let (db_path, _dir) = setup_db();
    let writer = IndexWriter::spawn(&db_path, None).unwrap();

    writer.send(WriteMessage::UpdateLastEventId(12345)).unwrap();
    writer.flush_blocking().unwrap();

    let store = open_read(&db_path);
    let status = store.get_index_status().unwrap();
    assert_eq!(status.last_event_id.as_deref(), Some("12345"));

    writer.shutdown();
}

#[test]
fn db_path_is_available() {
    let (db_path, _dir) = setup_db();
    let writer = IndexWriter::spawn(&db_path, None).unwrap();
    assert_eq!(writer.db_path(), db_path);
    writer.shutdown();
}

// ── try_send / queue_depth ───────────────────────────────────────

/// Happy path on a live writer: `try_send` enqueues without blocking and
/// bumps `queue_depth`; once the writer drains the message the depth returns
/// to 0. Pins both the `Ok(true)` outcome and the depth accounting.
#[test]
fn try_send_enqueues_and_tracks_queue_depth() {
    let (db_path, _dir) = setup_db();
    let writer = IndexWriter::spawn(&db_path, None).unwrap();

    let sent = writer
        .try_send(WriteMessage::ComputePartialAggregates {
            hot_paths: vec![],
            source: AggSource::Maps,
        })
        .expect("try_send on a live writer should not error");
    assert!(sent, "try_send into an empty channel should enqueue (Ok(true))");

    // After a flush barrier the writer has processed every prior message,
    // so the depth is back to 0.
    writer.flush_blocking().unwrap();
    wait_until(
        DRAIN_LANDS_WITHIN,
        "queue_depth to return to 0 once the writer drains",
        || writer.queue_depth() == 0,
    );

    writer.shutdown();
}

/// A `try_send` to a shut-down writer reports the disconnect as an error AND
/// undoes its depth bump, so a dead channel can't leave `queue_depth` drifted.
#[test]
fn try_send_after_shutdown_errors_and_undoes_depth() {
    let (db_path, _dir) = setup_db();
    let writer = IndexWriter::spawn(&db_path, None).unwrap();
    writer.shutdown();

    let depth_before = writer.queue_depth();
    let result = writer.try_send(WriteMessage::ComputePartialAggregates {
        hot_paths: vec![],
        source: AggSource::Maps,
    });
    assert!(
        result.is_err(),
        "try_send to a disconnected writer should be Err, got {result:?}"
    );
    assert_eq!(
        writer.queue_depth(),
        depth_before,
        "the depth bump must be undone on a disconnected send"
    );
}

/// The bump/undo accounting against a raw `sync_channel(1)`: the first send
/// fills the single slot (`Ok(true)`, depth +1), the second finds it full
/// (`Ok(false)`, no error, depth unchanged — the bump is undone). This pins
/// the Full path deterministically without a draining writer thread.
#[test]
fn try_send_with_depth_undoes_bump_on_full() {
    let (sender, _receiver) = mpsc::sync_channel::<WriteMessage>(1);
    let depth = AtomicUsize::new(0);

    let first = try_send_with_depth(
        &sender,
        &depth,
        WriteMessage::ComputePartialAggregates {
            hot_paths: vec![],
            source: AggSource::Maps,
        },
    )
    .expect("first send into an open channel should not error");
    assert!(first, "first send fills the single slot (Ok(true))");
    assert_eq!(depth.load(Ordering::Relaxed), 1, "successful send bumps depth");

    let second = try_send_with_depth(
        &sender,
        &depth,
        WriteMessage::ComputePartialAggregates {
            hot_paths: vec![],
            source: AggSource::Maps,
        },
    )
    .expect("a full channel is Ok(false), not Err");
    assert!(!second, "second send finds the channel full (Ok(false))");
    assert_eq!(
        depth.load(Ordering::Relaxed),
        1,
        "a dropped (full) send must leave depth unchanged — bump undone"
    );
}

/// A send that doesn't park costs the caller nothing, and a send that DOES park
/// records the wait — that's what lets a producer say how much of its own
/// duration was the writer queue rather than its own work. Pinned against a raw
/// `sync_channel(1)` so the park is deterministic.
#[test]
fn a_parked_send_records_its_wait_and_an_immediate_one_does_not() {
    fn partial_agg() -> WriteMessage {
        WriteMessage::ComputePartialAggregates {
            hot_paths: vec![],
            source: AggSource::Maps,
        }
    }

    let (sender, receiver) = mpsc::sync_channel::<WriteMessage>(1);
    let depth = AtomicUsize::new(0);

    wait_probe::take();
    send_blocking_with_depth(&sender, &depth, partial_agg()).expect("the single slot is free");
    assert_eq!(
        wait_probe::take(),
        Duration::ZERO,
        "a send into a free slot never parks, so it records nothing"
    );

    // The slot is full now, so this send parks until the receiver drains it.
    let drain_after = Duration::from_millis(50);
    let drainer = thread::spawn(move || {
        thread::sleep(drain_after);
        let _ = receiver.recv();
        // Keep the channel alive so the parked send lands rather than erroring.
        thread::sleep(Duration::from_millis(100));
    });
    send_blocking_with_depth(&sender, &depth, partial_agg()).expect("the drain lets the parked send land");
    let waited = wait_probe::take();
    assert!(
        waited >= drain_after,
        "a parked send must record the time it waited; got {waited:?}"
    );
    drainer.join().unwrap();
}

// ── Busy-handler escalation policy ───────────────────────────────────

/// The busy handler escalates to warn only for sustained contention (attempt
/// >= 20) AND only outside the WAL checkpoint's deliberate reader wait. Inside
/// > the checkpoint the wait is working-as-designed, so it stays at debug — this
/// > is what stops the ~32 warn lines per checkpoint that met a persistent reader.
#[test]
fn busy_handler_escalates_only_for_unexpected_sustained_contention() {
    use super::maintenance::busy_handler_escalates;
    // Outside a checkpoint: quiet below 20, warns at/above.
    assert!(!busy_handler_escalates(0, false));
    assert!(!busy_handler_escalates(19, false));
    assert!(busy_handler_escalates(20, false));
    assert!(busy_handler_escalates(51, false));
    // Inside the checkpoint's reader wait: never escalate, even past attempt 20.
    assert!(!busy_handler_escalates(0, true));
    assert!(!busy_handler_escalates(20, true));
    assert!(!busy_handler_escalates(51, true));
}

// ── Fatal-storage failure stops the writer (resilience) ──────────────

/// A fatal storage error (here `SQLITE_READONLY`, a deterministic stand-in for the
/// dead-disk `SQLITE_IOERR` from the real incident) must STOP the writer thread and
/// trip its failure signal, instead of logging-and-retrying forever (the
/// 12,700-warning livelock). Drives `writer_loop` directly with a `query_only`
/// connection: reads succeed, every write fails `READONLY`.
#[test]
fn a_fatal_storage_error_stops_the_writer_and_trips_the_signal() {
    use std::sync::atomic::AtomicUsize;

    let (db_path, _dir) = setup_db();

    // A writable connection put into query_only mode: reads work, writes fail with
    // SQLITE_READONLY — no real dead disk needed.
    let conn = IndexStore::open_write_connection(&db_path).expect("open write conn");
    conn.execute_batch("PRAGMA query_only = ON").expect("enable query_only");

    let (sender, receiver) = mpsc::sync_channel::<WriteMessage>(WRITER_CHANNEL_CAPACITY);
    let signal = Arc::new(IndexFailureSignal::new());
    let queue_depth = Arc::new(AtomicUsize::new(0));

    // Buffer a write that fails READONLY, then MANY more, BEFORE spawning the loop.
    // The channel capacity (20K) far exceeds 1,000, so all of them queue without a
    // consumer; spawning the loop next lets it stop right after the first fatal
    // error with the rest still buffered. Buffering first is what makes the test
    // deterministic: if we sent WHILE the loop ran, under load the loop could stop
    // and drop the receiver mid-send, failing the send (the loop stopping early is
    // exactly what we're asserting, so that must not be read as a test failure).
    // Mirror `IndexWriter::send`'s depth accounting.
    for i in 0..1000 {
        queue_depth.fetch_add(1, Ordering::Relaxed);
        sender
            .send(WriteMessage::UpsertEntryV2 {
                parent_id: ROOT_ID,
                name: format!("f{i}.txt"),
                is_directory: false,
                is_symlink: false,
                logical_size: Some(1),
                physical_size: Some(1),
                modified_at: None,
                inode: None,
                nlink: None,
            })
            .expect("channel has room for all 1,000 (no consumer yet)");
    }

    let signal_for_loop = Arc::clone(&signal);
    let queue_depth_for_loop = Arc::clone(&queue_depth);
    let handle = thread::spawn(move || {
        writer_loop(
            conn,
            receiver,
            None,
            "root".to_string(),
            Arc::new(AtomicU64::new(0)),
            Arc::new(AtomicI64::new(2)),
            Arc::new(MutationTracker::new(true)),
            queue_depth_for_loop,
            signal_for_loop,
        );
    });

    // The loop must terminate ON ITS OWN — we keep the sender alive, so a
    // still-running loop would block on recv, not exit.
    wait_until(
        Duration::from_secs(5),
        "the writer loop to stop after a fatal storage error rather than retry forever",
        || handle.is_finished(),
    );
    handle.join().expect("writer thread join");

    assert!(
        signal.is_tripped(),
        "the fatal write error must trip the failure signal"
    );
    let reason = signal.reason().expect("a reason is recorded");
    assert_eq!(
        reason.code,
        rusqlite::ffi::SQLITE_READONLY,
        "the recorded reason is the READONLY write failure"
    );

    // Bounded work: the loop stopped near the first message, not after draining all
    // 1,000 — most stay unprocessed in the channel.
    assert!(
        queue_depth.load(Ordering::Relaxed) > 900,
        "the writer stopped early, leaving most messages unprocessed (was {})",
        queue_depth.load(Ordering::Relaxed),
    );
}
