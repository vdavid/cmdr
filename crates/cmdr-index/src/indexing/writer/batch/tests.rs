//! Unit tests for the implicit-batch state machine and the message classification.
//!
//! The end-to-end guarantees (a `Flush` reply means committed, an explicit
//! transaction still works, an error leaves no transaction open, `Shutdown` commits)
//! are pinned against a real `writer_loop` in `../tests.rs`.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc;
use std::thread;

use super::*;
use crate::indexing::store::{IndexStore, ROOT_ID};
use crate::indexing::writer::tests::{setup_db, wait_for_writer_to_settle};
use crate::indexing::writer::{AggSource, IndexWriter, MutationTracker, WRITER_CHANNEL_CAPACITY, writer_loop};

/// A write connection on a throwaway DB, plus the temp dir keeping it alive.
fn write_conn() -> (rusqlite::Connection, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = dir.path().join("batch-tests.db");
    let _store = IndexStore::open(&db_path).expect("open store");
    let conn = IndexStore::open_write_connection(&db_path).expect("open write conn");
    (conn, dir)
}

/// Everything the loop can send, so the classification tests enumerate the protocol
/// rather than a sample of it. One representative value per variant.
fn every_message() -> Vec<WriteMessage> {
    let mut all = vec![
        WriteMessage::InsertEntriesV2(Vec::new()),
        WriteMessage::UpsertEntryV2 {
            parent_id: ROOT_ID,
            name: "f.txt".into(),
            is_directory: false,
            is_symlink: false,
            logical_size: Some(1),
            physical_size: Some(1),
            modified_at: None,
            inode: None,
            nlink: None,
        },
        WriteMessage::MoveEntryV2 {
            entry_id: 2,
            new_parent_id: ROOT_ID,
            new_name: "g.txt".into(),
        },
        WriteMessage::DeleteEntryById(2),
        WriteMessage::DeleteSubtreeById(2),
        WriteMessage::DeleteDescendantsById(2),
        WriteMessage::PropagateDeltaById {
            entry_id: 2,
            logical_size_delta: 1,
            physical_size_delta: 1,
            file_count_delta: 1,
            dir_count_delta: 0,
        },
        WriteMessage::PropagateMinSubtreeEpoch(2),
        WriteMessage::ComputeAllAggregates { source: AggSource::Sql },
        WriteMessage::ComputePartialAggregates {
            hot_paths: Vec::new(),
            source: AggSource::Sql,
        },
        WriteMessage::ComputeSubtreeAggregates { root_id: 2 },
        WriteMessage::UpdateLastEventId(7),
        WriteMessage::UpdateMeta {
            key: "k".into(),
            value: "v".into(),
        },
        WriteMessage::DeleteMeta("k".into()),
        WriteMessage::MarkDirsListed { ids: vec![2], epoch: 1 },
        WriteMessage::BumpCurrentEpoch,
        WriteMessage::Flush(tokio::sync::oneshot::channel().0),
        WriteMessage::TruncateData,
        WriteMessage::SetDeltaPropagation(false),
        WriteMessage::BeginTransaction,
        WriteMessage::CommitTransaction,
        WriteMessage::BackfillMissingDirStats,
        WriteMessage::ArmLedgerHealLatch,
        WriteMessage::MarkLedgerUnpaid,
        WriteMessage::PayLedgerIfUnpaid,
        WriteMessage::IncrementalVacuum,
        WriteMessage::WalCheckpoint,
        WriteMessage::EmitDirUpdated(Vec::new()),
        WriteMessage::Shutdown,
    ];
    #[cfg(test)]
    all.push(WriteMessage::GetEntryCount(tokio::sync::oneshot::channel().0));
    all
}

/// The messages whose contract is "everything before me is already durable" must all
/// force the batch shut. Named individually rather than derived from `role`, so the
/// test states the requirement instead of restating the implementation.
#[test]
fn every_durability_barrier_forces_the_batch_shut() {
    let barriers = [
        WriteMessage::Flush(tokio::sync::oneshot::channel().0),
        WriteMessage::BeginTransaction,
        WriteMessage::CommitTransaction,
        WriteMessage::TruncateData,
        WriteMessage::IncrementalVacuum,
        WriteMessage::WalCheckpoint,
        WriteMessage::Shutdown,
        WriteMessage::GetEntryCount(tokio::sync::oneshot::channel().0),
        WriteMessage::EmitDirUpdated(Vec::new()),
        WriteMessage::MarkLedgerUnpaid,
    ];
    for msg in barriers {
        assert_eq!(
            role(&msg),
            BatchRole::Barrier,
            "this message's contract requires its predecessors to be committed"
        );
    }
}

/// The live single-entry mutations — the ones paying one COMMIT + WAL frame write
/// each today — are what the batch exists to coalesce.
#[test]
fn the_live_mutations_open_a_batch() {
    let mutations = [
        WriteMessage::UpsertEntryV2 {
            parent_id: ROOT_ID,
            name: "f.txt".into(),
            is_directory: false,
            is_symlink: false,
            logical_size: Some(1),
            physical_size: Some(1),
            modified_at: None,
            inode: None,
            nlink: None,
        },
        WriteMessage::DeleteEntryById(2),
        WriteMessage::DeleteSubtreeById(2),
        WriteMessage::PropagateDeltaById {
            entry_id: 2,
            logical_size_delta: 1,
            physical_size_delta: 1,
            file_count_delta: 1,
            dir_count_delta: 0,
        },
    ];
    for msg in mutations {
        assert_eq!(role(&msg), BatchRole::Mutation, "a live mutation opens the batch");
    }
}

/// `InsertEntriesV2` already savepoints ~2000 rows per message, so it must not OPEN a
/// batch (a full scan's stream would balloon one transaction and the WAL with it) —
/// but it may join one a live mutation opened.
#[test]
fn a_scan_insert_batch_joins_a_batch_but_never_opens_one() {
    assert_eq!(role(&WriteMessage::InsertEntriesV2(Vec::new())), BatchRole::Neutral);
}

/// Every variant is classified. The match in `role` has no catch-all arm, so this is
/// really a guard that the list above keeps enumerating the whole protocol: a new
/// variant fails to compile there and shows up missing here.
#[test]
fn every_message_variant_has_a_role() {
    let roles: Vec<BatchRole> = every_message().iter().map(role).collect();
    assert_eq!(
        roles.len(),
        every_message().len(),
        "every protocol message classifies without panicking"
    );
    assert!(
        roles.contains(&BatchRole::Mutation) && roles.contains(&BatchRole::Barrier),
        "the protocol has both batchable work and barriers"
    );
}

/// An implicit batch never nests inside the scan path's explicit `BeginTransaction`:
/// SQLite has no nested transactions, so a `BEGIN` inside a `BEGIN` errors and the
/// writer would lose the message that tripped it.
#[test]
fn begin_does_not_nest_inside_an_explicit_transaction() {
    let (conn, _dir) = write_conn();
    conn.execute_batch("BEGIN IMMEDIATE").expect("explicit transaction");

    let mut batch = ImplicitBatch::new();
    batch.begin(&conn);

    assert!(
        !batch.is_open(&conn),
        "the batch must not claim a transaction it didn't open"
    );
    assert!(!conn.is_autocommit(), "the explicit transaction is still open");
    conn.execute_batch("COMMIT").expect("explicit commit still works");
}

/// A `close` on a connection whose transaction was rolled back underneath us (an
/// error, not our COMMIT) must simply forget the batch rather than run a `COMMIT`
/// that reports "cannot commit - no transaction is active".
#[test]
fn close_forgets_a_batch_that_was_rolled_back_underneath_it() {
    let (conn, _dir) = write_conn();
    let mut batch = ImplicitBatch::new();
    batch.begin(&conn);
    assert!(batch.is_open(&conn), "the batch opened");

    conn.execute_batch("ROLLBACK").expect("something rolled it back");
    assert!(conn.is_autocommit(), "the transaction is gone");

    let mut probe = ProbeStats::new("test-volume");
    let signal = IndexFailureSignal::new(crate::NoopEventSink::shared());
    let mut deferred = false;
    batch.close(&conn, &mut probe, &signal, &mut deferred);

    assert!(!batch.is_open(&conn), "the batch is closed");
    assert!(
        conn.is_autocommit(),
        "and the connection is left ready for the next message"
    );
}

/// The queue running dry is the primary close: with nothing queued behind it there is
/// nothing left to coalesce, so holding the transaction would only add latency. This
/// is what makes batching free — an idle writer commits exactly as eagerly as
/// autocommit did.
#[test]
fn an_empty_queue_closes_the_batch_immediately() {
    let batch = ImplicitBatch::new();
    assert!(batch.should_close(true), "an empty queue closes the batch");
    assert!(
        !batch.should_close(false),
        "a fresh batch with work still queued stays open"
    );
}

/// A sustained flood that never lets the queue drain still can't hold one transaction
/// open forever: the message cap closes it.
#[test]
fn the_message_cap_closes_a_batch_the_queue_never_drains() {
    let mut batch = ImplicitBatch::new();
    for _ in 0..MAX_MESSAGES - 1 {
        batch.note_message();
    }
    assert!(!batch.should_close(false), "still under the cap");
    batch.note_message();
    assert!(batch.should_close(false), "the message cap closes the batch");
}

/// …and so does the elapsed-time cap, which is what bounds the window in which a
/// live mutation's rows are invisible to every read connection.
#[test]
fn the_time_cap_closes_a_batch_the_queue_never_drains() {
    let mut batch = ImplicitBatch::new();
    batch.started = Instant::now() - MAX_DURATION;
    assert!(batch.should_close(false), "the elapsed-time cap closes the batch");
}

// ── The loop's end-to-end guarantees ─────────────────────────────────

/// One live upsert of a file directly under the root.
fn upsert(name: &str) -> WriteMessage {
    WriteMessage::UpsertEntryV2 {
        parent_id: ROOT_ID,
        name: name.into(),
        is_directory: false,
        is_symlink: false,
        logical_size: Some(4096),
        physical_size: Some(4096),
        modified_at: Some(1_700_000_000),
        inode: None,
        nlink: None,
    }
}

/// Rows in `entries`, read on a SECOND connection — so only COMMITTED work counts.
fn committed_entry_count(db_path: &Path) -> u64 {
    let conn = IndexStore::open_write_connection(db_path).expect("read-back conn");
    IndexStore::get_entry_count(&conn).expect("count entries")
}

/// Drive a real `writer_loop` over a channel filled BEFORE it starts, and return how
/// many transactions SQLite actually committed (its own `commit_hook`, not our
/// bookkeeping).
///
/// Pre-filling is what makes the batching deterministic: the queue never runs dry
/// mid-run, so no assertion depends on out-racing the writer. The `queue_depth`
/// accounting mirrors `IndexWriter::send`, since that counter is what the loop's close
/// decision reads. Dropping the sender before the loop starts also exercises the
/// disconnect path's commit.
fn run_prefilled_loop(db_path: &Path, messages: Vec<WriteMessage>) -> usize {
    let conn = IndexStore::open_write_connection(db_path).expect("write conn");
    let commits = Arc::new(AtomicUsize::new(0));
    {
        let commits = Arc::clone(&commits);
        // `false` = let the commit through; we're only counting.
        conn.commit_hook(Some(move || {
            commits.fetch_add(1, Ordering::Relaxed);
            false
        }))
        .expect("install the commit hook");
    }

    let (sender, receiver) = mpsc::sync_channel::<WriteMessage>(WRITER_CHANNEL_CAPACITY);
    let queue_depth = Arc::new(AtomicUsize::new(0));
    for msg in messages {
        queue_depth.fetch_add(1, Ordering::Relaxed);
        sender
            .send(msg)
            .expect("the channel holds the whole run (no consumer yet)");
    }
    drop(sender);

    let queue_depth_for_loop = Arc::clone(&queue_depth);
    let handle = thread::spawn(move || {
        writer_loop(
            conn,
            receiver,
            crate::NoopEventSink::shared(),
            "root".to_string(),
            Arc::new(AtomicU64::new(0)),
            Arc::new(AtomicI64::new(2)),
            Arc::new(MutationTracker::new(true)),
            queue_depth_for_loop,
            Arc::new(AtomicU64::new(0)),
            Arc::new(AtomicU64::new(0)),
            Arc::new(IndexFailureSignal::new(crate::NoopEventSink::shared())),
        );
    });
    handle.join().expect("writer thread join");
    commits.load(Ordering::Relaxed)
}

/// The point of the whole change: a run of queued live mutations pays ONE commit (and
/// one WAL frame write) for the lot instead of one each. Pre-fix every `UpsertEntryV2`
/// autocommitted, so this counted 500+; the probe measured that at 31.1 µs per row
/// against 7.0 µs inside a transaction.
#[test]
fn a_queued_run_of_live_mutations_commits_once_instead_of_once_per_message() {
    let (db_path, _dir) = setup_db();
    // Under MAX_MESSAGES, so the cap can't be what closes the batch.
    const MESSAGES: usize = 500;

    let messages: Vec<WriteMessage> = (0..MESSAGES).map(|i| upsert(&format!("f{i}.txt"))).collect();
    let commits = run_prefilled_loop(&db_path, messages);

    assert!(
        commits <= 4,
        "{MESSAGES} queued mutations should coalesce into a handful of commits, not one each (was {commits})"
    );
    assert_eq!(
        committed_entry_count(&db_path),
        MESSAGES as u64 + 1,
        "every row still landed (+1 for the root sentinel)"
    );
}

/// A `Flush` reply must mean "durable". It is a shipping backpressure barrier for the
/// scan-start funnels and both reconcilers, which flush precisely so their own
/// connection can read what the writer just wrote.
///
/// The flush sits in the MIDDLE of the queued run, so when it is handled there is
/// still work behind it and the end-of-iteration close hasn't fired: only `Flush` being
/// a batch barrier makes the early rows visible to the second connection at that
/// moment. `BEFORE_FLUSH` stays small so the elapsed-time cap can't be what committed
/// them instead. (That `Flush` IS a barrier is pinned outright by
/// `every_durability_barrier_forces_the_batch_shut`; this is the end-to-end check.)
#[test]
fn a_flush_reply_means_the_batched_rows_are_already_committed() {
    let (db_path, _dir) = setup_db();
    const BEFORE_FLUSH: usize = 50;
    const AFTER_FLUSH: usize = 3000;

    let conn = IndexStore::open_write_connection(&db_path).expect("write conn");
    let (sender, receiver) = mpsc::sync_channel::<WriteMessage>(WRITER_CHANNEL_CAPACITY);
    let queue_depth = Arc::new(AtomicUsize::new(0));
    let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();

    let mut messages: Vec<WriteMessage> = (0..BEFORE_FLUSH).map(|i| upsert(&format!("early{i}.txt"))).collect();
    messages.push(WriteMessage::Flush(reply_tx));
    messages.extend((0..AFTER_FLUSH).map(|i| upsert(&format!("late{i}.txt"))));
    for msg in messages {
        queue_depth.fetch_add(1, Ordering::Relaxed);
        sender.send(msg).expect("the channel holds the whole run");
    }
    drop(sender);

    let queue_depth_for_loop = Arc::clone(&queue_depth);
    let handle = thread::spawn(move || {
        writer_loop(
            conn,
            receiver,
            crate::NoopEventSink::shared(),
            "root".to_string(),
            Arc::new(AtomicU64::new(0)),
            Arc::new(AtomicI64::new(2)),
            Arc::new(MutationTracker::new(true)),
            queue_depth_for_loop,
            Arc::new(AtomicU64::new(0)),
            Arc::new(AtomicU64::new(0)),
            Arc::new(IndexFailureSignal::new(crate::NoopEventSink::shared())),
        );
    });

    // Opened BEFORE the reply, so the check after it is one query and can't lose a
    // race with the writer's next commit. Only committed rows are visible to it.
    let reader = IndexStore::open_write_connection(&db_path).expect("read-back conn");

    reply_rx.blocking_recv().expect("the writer replies to the flush");
    let visible = IndexStore::get_entry_count(&reader).expect("count entries");
    // The rows before the flush, plus the root sentinel.
    let expected = BEFORE_FLUSH as u64 + 1;
    assert!(
        visible >= expected,
        "a flush reply must mean every mutation before it is committed and visible to \
         another connection (it saw {visible}, expected at least {expected})"
    );

    handle.join().expect("writer thread join");
}

/// The scan path's explicit `BeginTransaction` / `CommitTransaction` pair still owns
/// the connection: the implicit batch commits BEFORE the `BEGIN` (SQLite has no nested
/// transactions, so a `BEGIN` inside a `BEGIN` errors) and the explicit `COMMIT` closes
/// the explicit transaction, not a batch of ours.
///
/// The commit COUNT is what discriminates: nesting would swallow both halves into one
/// implicit batch and commit once.
#[test]
fn an_explicit_transaction_is_neither_nested_nor_swallowed() {
    let (db_path, _dir) = setup_db();
    let mut messages: Vec<WriteMessage> = (0..10).map(|i| upsert(&format!("implicit{i}.txt"))).collect();
    messages.push(WriteMessage::BeginTransaction);
    messages.extend((0..10).map(|i| upsert(&format!("explicit{i}.txt"))));
    messages.push(WriteMessage::CommitTransaction);

    let commits = run_prefilled_loop(&db_path, messages);

    assert_eq!(
        commits, 2,
        "the implicit batch commits before the BEGIN, then the explicit transaction commits itself"
    );
    assert_eq!(
        committed_entry_count(&db_path),
        21,
        "both halves' rows landed (+1 for the root sentinel)"
    );
}

/// `Shutdown` must commit the batch it finds open, or a clean quit silently rolls the
/// last live writes back when the connection drops.
///
/// The messages queued BEHIND the shutdown are what make this load-bearing: with
/// `queue_depth` still non-zero the end-of-iteration close hasn't fired, so the ten
/// rows survive only because `Shutdown` is a batch barrier.
#[test]
fn shutdown_commits_the_batch_it_finds_open() {
    let (db_path, _dir) = setup_db();
    let mut messages: Vec<WriteMessage> = (0..10).map(|i| upsert(&format!("f{i}.txt"))).collect();
    messages.push(WriteMessage::Shutdown);
    messages.extend((0..5).map(|i| upsert(&format!("never{i}.txt"))));

    run_prefilled_loop(&db_path, messages);

    assert_eq!(
        committed_entry_count(&db_path),
        11,
        "the ten mutations queued before the shutdown are committed (+1 for the root sentinel)"
    );
}

/// A handler that fails mid-batch must never park the connection in an open
/// transaction: the next iteration would write into a transaction nothing closes, and
/// the write lock would be held against every other connection for as long as the
/// writer lives.
///
/// Probed from the outside, which is the only honest way to ask: with the writer
/// provably idle, a second connection's `BEGIN IMMEDIATE` succeeds if and only if the
/// writer is holding no transaction.
#[test]
fn a_failing_handler_never_parks_the_connection_in_a_transaction() {
    let (db_path, _dir) = setup_db();
    // A real SQLite failure (the same trick the deferred-repair tests use), so the
    // handler sees the `Err` shape a locked DB gives it — no mocks, no timing.
    let setup = IndexStore::open_write_connection(&db_path).expect("setup conn");
    setup
        .execute_batch(
            "CREATE TRIGGER block_insert BEFORE INSERT ON entries
             BEGIN SELECT RAISE(ABORT, 'entry insert blocked'); END;",
        )
        .expect("install the blocking trigger");
    drop(setup);

    let writer = IndexWriter::spawn(&db_path, crate::NoopEventSink::shared()).expect("spawn writer");
    let before = writer.idle_epoch();
    for i in 0..50 {
        writer.send(upsert(&format!("f{i}.txt"))).expect("send");
    }
    writer.flush_blocking().expect("flush");
    wait_for_writer_to_settle(&writer, before);

    let probe = IndexStore::open_write_connection(&db_path).expect("probe conn");
    probe.busy_timeout(Duration::from_millis(250)).expect("busy timeout");
    probe
        .execute_batch("BEGIN IMMEDIATE")
        .expect("the idle writer must not be holding the write lock in an open transaction");
    probe.execute_batch("COMMIT").expect("release the probe transaction");

    writer.shutdown();
}
