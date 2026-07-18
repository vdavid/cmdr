//! Integration tests for capture at the chokepoint (the capture layer): drive a real local-FS
//! op through the pipeline with a temp-DB journal installed globally, then read
//! back the journaled operation + item rows.
//!
//! These exercise the record points (`single_item.rs`, `walker.rs`,
//! `move_op.rs`) plus the open/finalize bracket, complementing the pure
//! eligibility/completeness unit tests in `operation_log::capture`.

use std::sync::Arc;
use std::time::Duration;

use super::journal;
use super::state::{WriteOperationState, get_operation_status, register_operation_status, unregister_operation_status};
use super::transfer::move_op::move_files_with_progress_inner;
use super::types::{CollectorEventSink, WriteOperationConfig, WriteOperationType};
use super::{copy_files_with_progress_inner, delete_files_with_progress_inner};

use crate::operation_log::capture::WriterJournal;
use crate::operation_log::store::{open_read_connection, operation_log_db_path, read_operation, read_operation_items};
use crate::operation_log::types::{EntryType, ExecutionStatus, Initiator, OpKind, RollbackState, RowRole};
use crate::operation_log::writer::OperationLogWriter;
use crate::operation_log::{clear_journal, set_journal};

/// Install a fresh temp-DB journal as the process-global one and hand back its
/// DB path + the temp dir (kept alive by the caller). nextest isolates each test
/// in its own process, so the global is hermetic.
fn install_journal() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = operation_log_db_path(dir.path());
    let writer = OperationLogWriter::spawn(&db).expect("spawn writer");
    set_journal(Arc::new(WriterJournal::new(writer)));
    (dir, db)
}

fn state() -> Arc<WriteOperationState> {
    Arc::new(WriteOperationState::new(Duration::from_millis(0)))
}

#[test]
fn grouped_copy_journals_leaf_files_and_created_dir_rows() {
    let (_jdir, jdb) = install_journal();
    let work = tempfile::tempdir().expect("work");
    // A source tree: src/a.txt and src/sub/b.txt.
    let src = work.path().join("src");
    std::fs::create_dir_all(src.join("sub")).expect("mk src");
    std::fs::write(src.join("a.txt"), b"aaa").expect("a");
    std::fs::write(src.join("sub").join("b.txt"), b"bbbb").expect("b");
    let dst = work.path().join("dst");
    std::fs::create_dir_all(&dst).expect("mk dst");

    let op_id = "op-copy-smoke";
    journal::open_local_op(op_id, OpKind::Copy, Initiator::User, 0, Some("root"));
    let events = CollectorEventSink::new();
    let cfg = WriteOperationConfig::default();
    copy_files_with_progress_inner(&events, op_id, &state(), std::slice::from_ref(&src), &dst, &cfg).expect("copy");
    journal::finalize_op(op_id, OpKind::Copy, ExecutionStatus::Done);
    clear_journal();

    let conn = open_read_connection(&jdb).expect("read conn");
    let row = read_operation(&conn, op_id).expect("read op").expect("op row");
    assert_eq!(row.kind, OpKind::Copy);
    assert_eq!(row.execution_status, ExecutionStatus::Done);
    // No overwrite ⇒ rollbackable.
    assert_eq!(row.rollback_state, RollbackState::Rollbackable);

    let items = read_operation_items(&conn, op_id, 1000).expect("items");
    let files: Vec<_> = items.iter().filter(|i| i.entry_type == EntryType::File).collect();
    let dirs: Vec<_> = items.iter().filter(|i| i.entry_type == EntryType::Dir).collect();
    // Two leaf files (a.txt, b.txt).
    assert_eq!(files.len(), 2, "expected 2 file rows, got {items:?}");
    // At least the two created dirs (dst/src, dst/src/sub).
    assert!(dirs.len() >= 2, "expected created-dir rows, got {items:?}");
    // Every item is a rollback_unit row for a copy.
    assert!(items.iter().all(|i| i.row_role == RowRole::RollbackUnit));
    // Dir rows are sequenced AFTER their contents (Finding 2): every dir seq is
    // greater than every file seq.
    let max_file_seq = files.iter().map(|i| i.seq).max().unwrap();
    let min_dir_seq = dirs.iter().map(|i| i.seq).min().unwrap();
    assert!(
        min_dir_seq > max_file_seq,
        "dir rows must follow file rows in seq (files max {max_file_seq}, dirs min {min_dir_seq})"
    );
}

/// Header-aggregate rider: the op HEADER carries the real planned count, the completed count,
/// and the destination volume — not the zeros an earlier local-FS open left. The
/// count is refined at finalize from the status cache the queue UI drives, so it
/// reflects what the op actually scanned (not the provisional top-level count the
/// open used). The alpha dialog reads `item_count` to render "Copy N items", so a
/// zero here would show "Copy 0 items".
#[test]
fn local_copy_header_carries_planned_count_items_done_and_dest_volume() {
    let (_jdir, jdb) = install_journal();
    let work = tempfile::tempdir().expect("work");
    // One top-level source directory holding three files, so the scanned leaf
    // total (3) differs from the provisional top-level count (1) the open uses —
    // proving finalize refined the header from the status cache.
    let src = work.path().join("src");
    std::fs::create_dir_all(&src).expect("mk src");
    for name in ["a.txt", "b.txt", "c.txt"] {
        std::fs::write(src.join(name), b"data").expect("write");
    }
    let dst = work.path().join("dst");
    std::fs::create_dir_all(&dst).expect("mk dst");

    let op_id = "op-copy-header";
    // The status cache must exist for the copy's progress updates to land and for
    // finalize to read the terminal totals (the real pipeline registers this; the
    // direct-call tests above don't, so their headers stay at the open value).
    register_operation_status(op_id, WriteOperationType::Copy, vec![]);
    // Open with the provisional top-level count (1 source dir), dest on "root".
    journal::open_local_op(op_id, OpKind::Copy, Initiator::User, 1, Some("root"));
    let events = CollectorEventSink::new();
    let cfg = WriteOperationConfig::default();
    copy_files_with_progress_inner(&events, op_id, &state(), std::slice::from_ref(&src), &dst, &cfg).expect("copy");
    let status = get_operation_status(op_id).expect("status present after copy");
    journal::finalize_op(op_id, OpKind::Copy, ExecutionStatus::Done);
    unregister_operation_status(op_id);
    clear_journal();

    let conn = open_read_connection(&jdb).expect("read conn");
    let row = read_operation(&conn, op_id).expect("read op").expect("op row");
    assert_eq!(
        row.item_count, status.files_total as u64,
        "finalize refines item_count to the scanned total, not the provisional open count"
    );
    assert!(
        row.item_count >= 3,
        "the 3 copied files should be reflected, got {}",
        row.item_count
    );
    assert_ne!(
        row.item_count, 1,
        "the provisional open count (1) must be overwritten by the scanned total"
    );
    assert_eq!(
        row.items_done, status.files_done as u64,
        "items_done reflects the completed files"
    );
    assert_eq!(
        row.dest_volume_id.as_deref(),
        Some("root"),
        "a local copy's destination is the local FS volume"
    );
}

#[test]
fn overwriting_copy_finalizes_not_rollbackable() {
    let (_jdir, jdb) = install_journal();
    let work = tempfile::tempdir().expect("work");
    let src = work.path().join("src");
    std::fs::create_dir_all(&src).expect("mk src");
    std::fs::write(src.join("a.txt"), b"new").expect("a");
    // Pre-existing dest that the copy will overwrite.
    let dst = work.path().join("dst");
    std::fs::create_dir_all(dst.join("src")).expect("mk dst");
    std::fs::write(dst.join("src").join("a.txt"), b"old").expect("old");

    let op_id = "op-copy-overwrite";
    journal::open_local_op(op_id, OpKind::Copy, Initiator::User, 0, Some("root"));
    let events = CollectorEventSink::new();
    let cfg = WriteOperationConfig {
        conflict_resolution: super::ConflictResolution::Overwrite,
        ..Default::default()
    };
    copy_files_with_progress_inner(&events, op_id, &state(), std::slice::from_ref(&src), &dst, &cfg).expect("copy");
    journal::finalize_op(op_id, OpKind::Copy, ExecutionStatus::Done);
    clear_journal();

    let conn = open_read_connection(&jdb).expect("read conn");
    let row = read_operation(&conn, op_id).expect("read op").expect("op row");
    assert_eq!(
        row.rollback_state,
        RollbackState::NotRollbackable,
        "an overwriting copy can't be rolled back (originals gone)"
    );
}

#[test]
fn same_fs_move_journals_the_top_level_item_as_rollback_unit() {
    let (_jdir, jdb) = install_journal();
    let work = tempfile::tempdir().expect("work");
    // Source and dest in the same tempdir ⇒ same filesystem ⇒ rename path.
    let src = work.path().join("photos");
    std::fs::create_dir_all(src.join("sub")).expect("mk src");
    std::fs::write(src.join("p.jpg"), b"pic").expect("p");
    let dst = work.path().join("dest");
    std::fs::create_dir_all(&dst).expect("mk dst");

    let op_id = "op-move-smoke";
    journal::open_local_op(op_id, OpKind::Move, Initiator::User, 0, Some("root"));
    let events = CollectorEventSink::new();
    let cfg = WriteOperationConfig::default();
    move_files_with_progress_inner(&events, op_id, &state(), std::slice::from_ref(&src), &dst, &cfg).expect("move");
    journal::finalize_op(op_id, OpKind::Move, ExecutionStatus::Done);
    clear_journal();

    let conn = open_read_connection(&jdb).expect("read conn");
    let row = read_operation(&conn, op_id).expect("read op").expect("op row");
    assert_eq!(row.kind, OpKind::Move);
    assert_eq!(row.rollback_state, RollbackState::Rollbackable);
    let items = read_operation_items(&conn, op_id, 1000).expect("items");
    // A same-FS move records ONE top-level rollback_unit row (the whole `photos`
    // subtree moved by one rename); its `search_only` leaves come from the drive
    // index (search-leaf enumeration), not enumerated here.
    assert_eq!(items.len(), 1, "expected 1 top-level row, got {items:?}");
    assert_eq!(items[0].entry_type, EntryType::Dir);
    assert_eq!(items[0].row_role, RowRole::RollbackUnit);
    assert_eq!(items[0].source_name, "photos");
}

#[test]
fn initiator_threads_from_the_op_into_the_journal_row() {
    // An AI-client-initiated copy records `ai_client`; the default path records
    // `user`. Provenance (D5) crosses as a typed enum, not a string.
    for initiator in [Initiator::User, Initiator::AiClient] {
        let (jdir, jdb) = install_journal();
        let work = tempfile::tempdir().expect("work");
        let src = work.path().join("src");
        std::fs::create_dir_all(&src).expect("mk src");
        std::fs::write(src.join("a.txt"), b"a").expect("a");
        let dst = work.path().join("dst");
        std::fs::create_dir_all(&dst).expect("mk dst");

        let op_id = "op-initiator";
        journal::open_local_op(op_id, OpKind::Copy, initiator, 0, Some("root"));
        let events = CollectorEventSink::new();
        copy_files_with_progress_inner(&events, op_id, &state(), &[src], &dst, &WriteOperationConfig::default())
            .expect("copy");
        journal::finalize_op(op_id, OpKind::Copy, ExecutionStatus::Done);
        clear_journal();

        let conn = open_read_connection(&jdb).expect("read conn");
        let row = read_operation(&conn, op_id).expect("read").expect("row");
        assert_eq!(row.initiator, initiator);
        drop(jdir);
    }
}

#[test]
fn delete_journals_search_leaves_and_stays_not_rollbackable() {
    let (_jdir, jdb) = install_journal();
    let work = tempfile::tempdir().expect("work");
    let a = work.path().join("dog.jpg");
    let b = work.path().join("cat.jpg");
    std::fs::write(&a, b"dog").expect("a");
    std::fs::write(&b, b"cat").expect("b");

    let op_id = "op-delete-smoke";
    journal::open_local_op(op_id, OpKind::Delete, Initiator::User, 0, None);
    let events = CollectorEventSink::new();
    let cfg = WriteOperationConfig::default();
    delete_files_with_progress_inner(&events, op_id, &state(), &[a.clone(), b.clone()], &cfg).expect("delete");
    journal::finalize_op(op_id, OpKind::Delete, ExecutionStatus::Done);
    clear_journal();

    let conn = open_read_connection(&jdb).expect("read conn");
    let row = read_operation(&conn, op_id).expect("read op").expect("op row");
    // Delete is never rollbackable.
    assert_eq!(row.rollback_state, RollbackState::NotRollbackable);
    let items = read_operation_items(&conn, op_id, 1000).expect("items");
    // Both deleted leaves are journaled so "when did I delete dog.jpg" is searchable.
    assert_eq!(items.len(), 2, "expected 2 deleted-leaf rows, got {items:?}");
    assert!(items.iter().any(|i| i.source_name == "dog.jpg"));
}

// ── Search-leaf enumeration for trash ────────────────────────────
//
// These drive the real trash pipeline with a CANNED enumeration (the test hook),
// so the wiring — enumerate-before, persist-after-success, coverage notes — is
// exercised without standing up a live drive index + registry. The enumeration
// CORE honesty (stale / over-cap / full from a real index) is unit-tested in
// `journal_search::tests`.

#[cfg(target_os = "macos")]
use crate::operation_log::types::{SearchCoverage, SearchCoverageReason};

/// A canned drive-index enumeration for the trashed subtree.
#[cfg(target_os = "macos")]
fn install_canned_leaves(coverage: SearchCoverage, reason: Option<SearchCoverageReason>, names: &[&str]) {
    let leaves: Vec<_> = names
        .iter()
        .map(|n| super::journal_search::Leaf {
            rel: std::path::PathBuf::from(n),
            entry_type: EntryType::File,
            size: Some(1),
            mtime: None,
        })
        .collect();
    super::journal_search::test_hook::install(move |_path| {
        Some(super::journal_search::BufferedLeaves {
            coverage,
            reason,
            leaves: leaves.clone(),
        })
    });
}

#[cfg(target_os = "macos")]
fn trash(op_id: &str, sources: &[std::path::PathBuf]) {
    use super::delete::trash::trash_files_with_progress;
    journal::open_local_op(op_id, OpKind::Trash, Initiator::User, 0, None);
    let events = CollectorEventSink::new();
    let st = state();
    // A missing source in the batch is a per-item failure, not a whole-op failure.
    let _ = trash_files_with_progress(&events, op_id, &st, sources, None);
    journal::finalize_op(op_id, OpKind::Trash, ExecutionStatus::Done);
    super::journal_search::test_hook::clear();
    clear_journal();
}

/// A trashed folder records the top-level `rollback_unit` row AND the subtree's
/// `search_only` leaves enumerated from the drive index — so "when did I trash
/// `b.jpg`" hits even though `b.jpg` sat inside a trashed folder.
#[cfg(target_os = "macos")]
#[test]
fn trashed_dir_records_search_leaves_and_stays_full() {
    let (_jdir, jdb) = install_journal();
    let work = tempfile::tempdir().expect("work");
    let dir = work.path().join("photos");
    std::fs::create_dir_all(dir.join("sub")).expect("mk");
    std::fs::write(dir.join("a.jpg"), b"a").expect("a");
    std::fs::write(dir.join("sub").join("b.jpg"), b"b").expect("b");

    install_canned_leaves(SearchCoverage::Full, None, &["a.jpg", "sub/b.jpg"]);
    trash("op-trash-leaves", std::slice::from_ref(&dir));

    let conn = open_read_connection(&jdb).expect("read conn");
    let row = read_operation(&conn, "op-trash-leaves").expect("read").expect("row");
    assert_eq!(row.kind, OpKind::Trash);
    assert_eq!(row.rollback_state, RollbackState::Rollbackable);
    assert_eq!(row.search_coverage, SearchCoverage::Full);

    let items = read_operation_items(&conn, "op-trash-leaves", 1000).expect("items");
    let units: Vec<_> = items.iter().filter(|i| i.row_role == RowRole::RollbackUnit).collect();
    let leaves: Vec<_> = items.iter().filter(|i| i.row_role == RowRole::SearchOnly).collect();
    assert_eq!(units.len(), 1, "one top-level trash unit, got {items:?}");
    assert_eq!(units[0].source_name, "photos");
    assert_eq!(leaves.len(), 2, "two search leaves from the index, got {items:?}");
    assert!(
        leaves.iter().any(|i| i.source_name == "b.jpg"),
        "leaf search finds b.jpg"
    );
}

/// A trash op whose one top-level item FAILS records no `search_only` rows for
/// that item's subtree, while a sibling that succeeded keeps its leaves — so
/// search can't return a trash that never happened (persist-after-success).
#[cfg(target_os = "macos")]
#[test]
fn failed_trash_item_records_no_search_leaves_but_a_sibling_keeps_its_own() {
    let (_jdir, jdb) = install_journal();
    let work = tempfile::tempdir().expect("work");
    let good = work.path().join("good");
    std::fs::create_dir_all(&good).expect("mk good");
    std::fs::write(good.join("keep.jpg"), b"k").expect("k");
    // A missing source: fails at the existence check before it's ever trashed.
    let missing = work.path().join("gone");

    install_canned_leaves(SearchCoverage::Full, None, &["keep.jpg"]);
    trash("op-trash-partial", &[good.clone(), missing.clone()]);

    let conn = open_read_connection(&jdb).expect("read conn");
    let items = read_operation_items(&conn, "op-trash-partial", 1000).expect("items");
    // The succeeded item: its top-level row + its one search leaf.
    assert!(
        items
            .iter()
            .any(|i| i.source_name == "good" && i.row_role == RowRole::RollbackUnit)
    );
    assert!(
        items
            .iter()
            .any(|i| i.source_name == "keep.jpg" && i.row_role == RowRole::SearchOnly)
    );
    // The failed item contributed NOTHING (no top-level row, no leaves).
    assert!(
        !items.iter().any(|i| i.source_name == "gone"),
        "a failed item records nothing, got {items:?}"
    );
}

// ── Performance — capture stays off the operation's hot path ────────────

/// Create `n` tiny files in a fresh tempdir; return the dir (keep alive) + paths.
fn make_files(n: usize) -> (tempfile::TempDir, Vec<std::path::PathBuf>) {
    let dir = tempfile::tempdir().expect("dir");
    let paths: Vec<_> = (0..n)
        .map(|i| {
            let p = dir.path().join(format!("f{i}.bin"));
            std::fs::write(&p, b"x").expect("write");
            p
        })
        .collect();
    (dir, paths)
}

/// Time a delete of `paths` with the op id already opened/finalized around it.
fn time_delete(op_id: &str, paths: &[std::path::PathBuf]) -> Duration {
    journal::open_local_op(op_id, OpKind::Delete, Initiator::User, 0, None);
    let events = CollectorEventSink::new();
    let t = std::time::Instant::now();
    delete_files_with_progress_inner(&events, op_id, &state(), paths, &WriteOperationConfig::default())
        .expect("delete");
    let elapsed = t.elapsed();
    journal::finalize_op(op_id, OpKind::Delete, ExecutionStatus::Done);
    elapsed
}

/// Requirement 8 (logging never measurably slows an op) under three writer loads:
/// (a) no journal, (b) a keeping-up journal, (c) a journal whose writer thread is
/// concurrently hammered with retention prunes + `incremental_vacuum` — the arm a
/// naive test would miss. All three must finish the op within a generous budget:
/// capture rides a bounded channel that BLOCKS only if the writer falls behind, and
/// the vacuum runs in bounded slices between batches, so even the loaded writer
/// can't stall the op. A capture that went synchronous on the op thread would blow
/// the budget in (b)/(c).
///
/// `#[ignore]`d: this is a wall-clock latency assertion (three real 1,500-file
/// create+delete passes plus a core-hogging vacuum spinner), which is meaningless
/// under `cargo nextest`'s process-per-test parallelism — a descheduled run blows
/// the 8 s cap even though the internal `base*6 + 3s` budget is contention-robust.
/// Run it explicitly to check the "capture stays off the hot path" property.
#[test]
#[ignore = "wall-clock latency assertion; meaningless under nextest parallelism — run explicitly"]
fn capture_stays_off_the_hot_path_under_writer_load() {
    const N: usize = 1_500;

    // Arm (a): no journal — the baseline op cost (pure file I/O).
    let (_da, pa) = make_files(N);
    let events = CollectorEventSink::new();
    let t = std::time::Instant::now();
    delete_files_with_progress_inner(&events, "perf-a", &state(), &pa, &WriteOperationConfig::default())
        .expect("delete a");
    let base = t.elapsed();

    // Arm (b): a keeping-up journal.
    let (_jb, jdb) = install_journal();
    let (_db, pb) = make_files(N);
    let kept_up = time_delete("perf-b", &pb);
    {
        let conn = open_read_connection(&jdb).expect("read");
        let items = read_operation_items(&conn, "perf-b", 100_000).expect("items");
        assert_eq!(items.len(), N, "every deleted leaf journaled under normal load");
    }
    clear_journal();

    // Arm (c): a journal whose writer is under concurrent prune + vacuum load.
    let cdir = tempfile::tempdir().expect("cdir");
    let cdb = operation_log_db_path(cdir.path());
    let writer = OperationLogWriter::spawn(&cdb).expect("spawn writer");
    // Seed churn so the freelist has pages for `incremental_vacuum` to reclaim.
    for i in 0..40 {
        let id = format!("seed-{i}");
        writer
            .open_operation(crate::operation_log::writer::OpenOperation {
                op_id: id.clone(),
                kind: OpKind::Delete,
                initiator: Initiator::User,
                source_volume_id: Some("root".into()),
                dest_volume_id: None,
                item_count: 0,
                started_at: 1,
                rolls_back_op_id: None,
                execution_status: ExecutionStatus::Running,
            })
            .expect("seed open");
    }
    writer.flush_blocking().expect("flush seed");
    set_journal(Arc::new(WriterJournal::new(writer.clone())));

    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let pruner = {
        let writer = writer.clone();
        let stop = Arc::clone(&stop);
        std::thread::spawn(move || {
            while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                let _ = writer.prune(crate::operation_log::writer::PruneRequest {
                    max_age_secs: Some(0),
                    max_size_bytes: None,
                    now_secs: 10,
                    vacuum: true,
                });
            }
        })
    };

    let (_dc, pc) = make_files(N);
    let stalled = time_delete("perf-c", &pc);
    stop.store(true, std::sync::atomic::Ordering::Relaxed);
    pruner.join().expect("join pruner");
    clear_journal();

    // Generous budget: journaling must not MULTIPLY op time. A synchronous capture
    // would make (b)/(c) an order of magnitude over the baseline; a bounded-channel
    // capture stays within a small multiple plus fixed slack (loose enough for CI
    // noise, tight enough to catch a hot-path regression).
    let budget = base * 6 + Duration::from_secs(3);
    assert!(
        kept_up < budget,
        "journaling put capture on the hot path: base={base:?} kept_up={kept_up:?} budget={budget:?}"
    );
    assert!(
        stalled < budget,
        "a loaded writer stalled the op: base={base:?} stalled={stalled:?} budget={budget:?}"
    );
}

/// BENCHMARK (ignored): background persist throughput — how fast the writer drains
/// a burst of `search_only` leaf rows. Feeds `docs/notes/operation-log-capture-bench.md`.
#[test]
#[ignore = "benchmark; run explicitly to collect numbers"]
#[allow(clippy::print_stdout, reason = "a benchmark prints its measurements")]
fn bench_persist_throughput() {
    use crate::operation_log::writer::JournalItem;
    const N: usize = 50_000;
    let dir = tempfile::tempdir().expect("dir");
    let db = operation_log_db_path(dir.path());
    let writer = OperationLogWriter::spawn(&db).expect("spawn");
    writer
        .open_operation(crate::operation_log::writer::OpenOperation {
            op_id: "bench".into(),
            kind: OpKind::Delete,
            initiator: Initiator::User,
            source_volume_id: Some("root".into()),
            dest_volume_id: None,
            item_count: 0,
            started_at: 1,
            rolls_back_op_id: None,
            execution_status: ExecutionStatus::Running,
        })
        .expect("open");
    let items: Vec<_> = (0..N)
        .map(|i| JournalItem {
            seq: i as i64,
            entry_type: EntryType::File,
            row_role: RowRole::SearchOnly,
            source_volume_id: "root".into(),
            source_dir: "/x".into(),
            source_name: format!("f{i}.jpg"),
            dest_volume_id: None,
            dest_dir: None,
            dest_name: None,
            size: Some(10),
            mtime: Some(1),
            outcome: crate::operation_log::types::ItemOutcome::Done,
            overwrote: false,
        })
        .collect();
    let t = std::time::Instant::now();
    // Batch as the capture layer does, then flush (barrier) for the true drain time.
    for chunk in items.chunks(512) {
        writer.record_items("bench", chunk.to_vec()).expect("record");
    }
    writer.flush_blocking().expect("flush");
    let elapsed = t.elapsed();
    println!(
        // allowed-pluralize-noun: benchmark output, N is a fixed 50_000 (never 1)
        "persist {N} leaves in {elapsed:?} ({:.0} rows/s)",
        N as f64 / elapsed.as_secs_f64()
    );
}

/// BENCHMARK (ignored): the op-latency delta of a same-FS move with journaling ON
/// vs OFF (target ~zero — a move records ONE top-level row + a bounded enumerate).
#[test]
#[ignore = "benchmark; run explicitly to collect numbers"]
#[allow(clippy::print_stdout, reason = "a benchmark prints its measurements")]
fn bench_same_fs_move_latency_delta() {
    const K: usize = 200;

    // OFF: no journal — the baseline rename cost.
    let off = {
        let work = tempfile::tempdir().expect("work");
        let dst = work.path().join("dst");
        std::fs::create_dir_all(&dst).expect("dst");
        let srcs: Vec<_> = (0..K)
            .map(|i| {
                let d = work.path().join(format!("d{i}"));
                std::fs::create_dir_all(&d).expect("mk");
                std::fs::write(d.join("f.bin"), b"x").expect("w");
                d
            })
            .collect();
        let events = CollectorEventSink::new();
        let t = std::time::Instant::now();
        for (i, s) in srcs.iter().enumerate() {
            move_files_with_progress_inner(
                &events,
                &format!("off-{i}"),
                &state(),
                std::slice::from_ref(s),
                &dst,
                &WriteOperationConfig::default(),
            )
            .expect("move");
        }
        t.elapsed()
    };

    // ON: journal installed (no drive index registered, so each move records one
    // top-level row + a fast VolumeNotLive enumerate).
    let (_j, _jdb) = install_journal();
    let on = {
        let work = tempfile::tempdir().expect("work");
        let dst = work.path().join("dst");
        std::fs::create_dir_all(&dst).expect("dst");
        let srcs: Vec<_> = (0..K)
            .map(|i| {
                let d = work.path().join(format!("d{i}"));
                std::fs::create_dir_all(&d).expect("mk");
                std::fs::write(d.join("f.bin"), b"x").expect("w");
                d
            })
            .collect();
        let events = CollectorEventSink::new();
        let t = std::time::Instant::now();
        for (i, s) in srcs.iter().enumerate() {
            let id = format!("on-{i}");
            journal::open_local_op(&id, OpKind::Move, Initiator::User, 0, Some("root"));
            move_files_with_progress_inner(
                &events,
                &id,
                &state(),
                std::slice::from_ref(s),
                &dst,
                &WriteOperationConfig::default(),
            )
            .expect("move");
            journal::finalize_op(&id, OpKind::Move, ExecutionStatus::Done);
        }
        t.elapsed()
    };
    clear_journal();
    println!(
        "same-FS move x{K}: off={off:?} ({:?}/op) on={on:?} ({:?}/op)",
        off / K as u32,
        on / K as u32
    );
}

/// A same-FS move of a folder records the top-level `rollback_unit` row AND the
/// subtree's `search_only` leaves from the index, rebased onto the moved-to path.
#[cfg(target_os = "macos")]
#[test]
fn same_fs_move_dir_records_search_leaves() {
    use super::transfer::move_op::move_files_with_progress_inner;
    let (_jdir, jdb) = install_journal();
    let work = tempfile::tempdir().expect("work");
    let src = work.path().join("photos");
    std::fs::create_dir_all(&src).expect("mk src");
    std::fs::write(src.join("p.jpg"), b"pic").expect("p");
    let dst = work.path().join("dest");
    std::fs::create_dir_all(&dst).expect("mk dst");

    install_canned_leaves(SearchCoverage::Full, None, &["p.jpg"]);
    let op_id = "op-move-leaves";
    journal::open_local_op(op_id, OpKind::Move, Initiator::User, 0, Some("root"));
    let events = CollectorEventSink::new();
    move_files_with_progress_inner(
        &events,
        op_id,
        &state(),
        std::slice::from_ref(&src),
        &dst,
        &WriteOperationConfig::default(),
    )
    .expect("move");
    journal::finalize_op(op_id, OpKind::Move, ExecutionStatus::Done);
    super::journal_search::test_hook::clear();
    clear_journal();

    let conn = open_read_connection(&jdb).expect("read conn");
    let items = read_operation_items(&conn, op_id, 1000).expect("items");
    let units: Vec<_> = items.iter().filter(|i| i.row_role == RowRole::RollbackUnit).collect();
    let leaves: Vec<_> = items.iter().filter(|i| i.row_role == RowRole::SearchOnly).collect();
    assert_eq!(units.len(), 1, "one top-level move unit, got {items:?}");
    assert_eq!(leaves.len(), 1, "one search leaf, got {items:?}");
    assert_eq!(leaves[0].source_name, "p.jpg");
}

/// An over-cap subtree records the top-level `rollback_unit` row only, downgrades
/// coverage to `top_level_only` with the `capped` reason (distinct from stale /
/// absent), and STILL rolls back fully (the cap never touches the undo unit).
#[cfg(target_os = "macos")]
#[test]
fn over_cap_trash_is_top_level_only_capped_but_still_rollbackable() {
    let (_jdir, jdb) = install_journal();
    let work = tempfile::tempdir().expect("work");
    let dir = work.path().join("huge");
    std::fs::create_dir_all(&dir).expect("mk");
    std::fs::write(dir.join("x.bin"), b"x").expect("x");

    install_canned_leaves(SearchCoverage::TopLevelOnly, Some(SearchCoverageReason::Capped), &[]);
    trash("op-trash-capped", std::slice::from_ref(&dir));

    let conn = open_read_connection(&jdb).expect("read conn");
    let row = read_operation(&conn, "op-trash-capped").expect("read").expect("row");
    assert_eq!(row.search_coverage, SearchCoverage::TopLevelOnly);
    assert_eq!(row.search_coverage_reason, Some(SearchCoverageReason::Capped));
    // The cap only bounds search — the top-level unit is still the undo unit.
    assert_eq!(row.rollback_state, RollbackState::Rollbackable);
    let items = read_operation_items(&conn, "op-trash-capped", 1000).expect("items");
    assert_eq!(items.len(), 1, "top-level row only, no search leaves, got {items:?}");
    assert_eq!(items[0].row_role, RowRole::RollbackUnit);
}

// ── Volume (SMB / MTP) capture: the honesty-critical volume-id plumbing ──────
//
// These drive the REAL volume copy/move/delete bodies with in-memory volumes and
// a temp-DB journal, then read the rows back. The load-bearing assertion is that a
// volume op's rows carry the REAL volume id, never the local `"root"` the local
// helpers bake in — a wrong volume id would corrupt history silently.

use super::types::VolumeCopyConfig;
use super::{copy_volumes_with_progress, move_volumes_with_progress};
use crate::file_system::volume::{InMemoryVolume, Volume};

/// Every distinct `volume_id` interned in the `dirs` table (fresh DB per test, so
/// this is exactly the set the op journaled under).
fn dir_volume_ids(conn: &rusqlite::Connection) -> Vec<String> {
    let mut stmt = conn.prepare("SELECT DISTINCT volume_id FROM dirs").expect("prepare");

    stmt.query_map([], |r| r.get::<_, String>(0))
        .expect("query")
        .collect::<Result<Vec<_>, _>>()
        .expect("collect")
}

fn in_memory(name: &str) -> Arc<InMemoryVolume> {
    Arc::new(InMemoryVolume::new(name).with_space_info(1_000_000, 900_000))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn volume_copy_journals_under_the_real_volume_ids_not_root() {
    let (_jdir, jdb) = install_journal();
    let source = in_memory("Src");
    source
        .create_file(std::path::Path::new("/a.txt"), b"aaa")
        .await
        .expect("a");
    source
        .create_file(std::path::Path::new("/b.txt"), b"bbbb")
        .await
        .expect("b");
    let dest = in_memory("Dst");

    let op_id = "op-vol-copy";
    let state = Arc::new(
        WriteOperationState::new(Duration::from_millis(0)).with_journal_volumes("smb-src".into(), "smb-dst".into()),
    );
    journal::open_volume_op(op_id, OpKind::Copy, Initiator::AiClient, "smb-src", Some("smb-dst"), 0);
    copy_volumes_with_progress(
        Arc::new(CollectorEventSink::new()),
        op_id,
        &state,
        source as Arc<dyn Volume>,
        &[std::path::PathBuf::from("/a.txt"), std::path::PathBuf::from("/b.txt")],
        dest as Arc<dyn Volume>,
        std::path::Path::new("/"),
        &VolumeCopyConfig::default(),
    )
    .await
    .expect("volume copy");
    journal::finalize_op(op_id, OpKind::Copy, ExecutionStatus::Done);
    clear_journal();

    let conn = open_read_connection(&jdb).expect("read conn");
    let row = read_operation(&conn, op_id).expect("read").expect("op row");
    // The operation header carries the REAL volume ids + the AI-client provenance.
    assert_eq!(row.source_volume_id.as_deref(), Some("smb-src"));
    assert_eq!(row.dest_volume_id.as_deref(), Some("smb-dst"));
    assert_eq!(row.initiator, Initiator::AiClient);
    assert_eq!(row.kind, OpKind::Copy);
    // No overwrite ⇒ rollbackable.
    assert_eq!(row.rollback_state, RollbackState::Rollbackable);

    let items = read_operation_items(&conn, op_id, 1000).expect("items");
    assert_eq!(items.len(), 2, "two leaf rows, got {items:?}");
    assert!(items.iter().all(|i| i.row_role == RowRole::RollbackUnit));

    // The honesty invariant: every interned dir is on a REAL volume, never "root".
    let vols = dir_volume_ids(&conn);
    assert!(
        vols.iter().all(|v| v == "smb-src" || v == "smb-dst"),
        "volume copy dirs must carry the real volume ids, got {vols:?}"
    );
    assert!(
        !vols.iter().any(|v| v == "root"),
        "a volume op must never journal under root"
    );
    assert!(vols.iter().any(|v| v == "smb-src") && vols.iter().any(|v| v == "smb-dst"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn overwriting_volume_copy_finalizes_not_rollbackable() {
    let (_jdir, jdb) = install_journal();
    let source = in_memory("Src");
    source
        .create_file(std::path::Path::new("/dup.txt"), b"new")
        .await
        .expect("src dup");
    let dest = in_memory("Dst");
    // Pre-existing dest file with the same name ⇒ the copy overwrites it.
    dest.create_file(std::path::Path::new("/dup.txt"), b"old")
        .await
        .expect("dst dup");

    let op_id = "op-vol-copy-ow";
    let cfg = VolumeCopyConfig {
        conflict_resolution: super::types::ConflictResolution::Overwrite,
        ..Default::default()
    };
    let state = Arc::new(
        WriteOperationState::new(Duration::from_millis(0)).with_journal_volumes("smb-src".into(), "smb-dst".into()),
    );
    journal::open_volume_op(op_id, OpKind::Copy, Initiator::User, "smb-src", Some("smb-dst"), 0);
    copy_volumes_with_progress(
        Arc::new(CollectorEventSink::new()),
        op_id,
        &state,
        source as Arc<dyn Volume>,
        &[std::path::PathBuf::from("/dup.txt")],
        dest as Arc<dyn Volume>,
        std::path::Path::new("/"),
        &cfg,
    )
    .await
    .expect("volume copy");
    journal::finalize_op(op_id, OpKind::Copy, ExecutionStatus::Done);
    clear_journal();

    let conn = open_read_connection(&jdb).expect("read conn");
    let row = read_operation(&conn, op_id).expect("read").expect("op row");
    // Overwriting an existing dest ⇒ not rollbackable (the original is gone).
    assert_eq!(row.rollback_state, RollbackState::NotRollbackable);
    let items = read_operation_items(&conn, op_id, 1000).expect("items");
    assert!(
        items.iter().any(|i| i.overwrote),
        "the overwriting leaf must be flagged, got {items:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cross_volume_move_journals_per_leaf_move_rows() {
    let (_jdir, jdb) = install_journal();
    let source = in_memory("Src");
    source
        .create_file(std::path::Path::new("/one.txt"), b"1")
        .await
        .expect("one");
    source
        .create_file(std::path::Path::new("/two.txt"), b"22")
        .await
        .expect("two");
    let dest = in_memory("Dst");

    let op_id = "op-vol-move";
    let state = Arc::new(
        WriteOperationState::new(Duration::from_millis(0)).with_journal_volumes("smb-src".into(), "smb-dst".into()),
    );
    journal::open_volume_op(op_id, OpKind::Move, Initiator::User, "smb-src", Some("smb-dst"), 0);
    move_volumes_with_progress(
        Arc::new(CollectorEventSink::new()),
        op_id,
        &state,
        source as Arc<dyn Volume>,
        &[
            std::path::PathBuf::from("/one.txt"),
            std::path::PathBuf::from("/two.txt"),
        ],
        dest as Arc<dyn Volume>,
        std::path::Path::new("/"),
        &VolumeCopyConfig::default(),
    )
    .await
    .expect("volume move");
    journal::finalize_op(op_id, OpKind::Move, ExecutionStatus::Done);
    clear_journal();

    let conn = open_read_connection(&jdb).expect("read conn");
    let row = read_operation(&conn, op_id).expect("read").expect("op row");
    assert_eq!(row.kind, OpKind::Move);
    // A cross-volume move is per-leaf (D-granularity): no overwrite ⇒ rollbackable.
    assert_eq!(row.rollback_state, RollbackState::Rollbackable);

    // Per-leaf rows: one `rollback_unit` per moved FILE, source on the source
    // volume, dest on the dest volume.
    let items = read_operation_items(&conn, op_id, 1000).expect("items");
    let files: Vec<_> = items.iter().filter(|i| i.entry_type == EntryType::File).collect();
    assert_eq!(files.len(), 2, "one leaf row per moved file, got {items:?}");
    assert!(files.iter().all(|i| i.row_role == RowRole::RollbackUnit));
    let vols = dir_volume_ids(&conn);
    assert!(
        !vols.iter().any(|v| v == "root"),
        "a cross-volume move must never journal under root"
    );
    assert!(vols.iter().any(|v| v == "smb-src") && vols.iter().any(|v| v == "smb-dst"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn volume_delete_journals_per_leaf_under_the_real_volume_id() {
    let (_jdir, jdb) = install_journal();
    let volume = in_memory("Src");
    volume
        .create_file(std::path::Path::new("/gone1.txt"), b"x")
        .await
        .expect("g1");
    volume
        .create_file(std::path::Path::new("/gone2.txt"), b"yy")
        .await
        .expect("g2");

    let op_id = "op-vol-delete";
    let state = state();
    journal::open_volume_op(op_id, OpKind::Delete, Initiator::User, "smb-src", None, 0);
    super::delete_volume_files_with_progress_inner(
        volume as Arc<dyn Volume>,
        "smb-src",
        &CollectorEventSink::new(),
        op_id,
        &state,
        &[
            std::path::PathBuf::from("/gone1.txt"),
            std::path::PathBuf::from("/gone2.txt"),
        ],
        &WriteOperationConfig::default(),
    )
    .await
    .expect("volume delete");
    journal::finalize_op(op_id, OpKind::Delete, ExecutionStatus::Done);
    clear_journal();

    let conn = open_read_connection(&jdb).expect("read conn");
    let row = read_operation(&conn, op_id).expect("read").expect("op row");
    assert_eq!(row.kind, OpKind::Delete);
    // Delete is never rollbackable.
    assert_eq!(row.rollback_state, RollbackState::NotRollbackable);
    let items = read_operation_items(&conn, op_id, 1000).expect("items");
    let files: Vec<_> = items.iter().filter(|i| i.entry_type == EntryType::File).collect();
    assert_eq!(files.len(), 2, "one leaf row per deleted file, got {items:?}");
    let vols = dir_volume_ids(&conn);
    assert_eq!(
        vols,
        vec!["smb-src".to_string()],
        "the delete must journal under the real volume id"
    );
}
