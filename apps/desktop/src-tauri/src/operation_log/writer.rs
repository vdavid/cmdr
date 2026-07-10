//! `OperationLogWriter`: the single writer thread for `operation-log.db`.
//!
//! Mirrors the importance/index one-writer-per-DB discipline: exactly ONE thread
//! owns the single write connection, and all writes cross a bounded channel. The
//! handle is cloneable; every clone shares the one channel and thread. **Unlike
//! importance there is NO per-volume registry** — the operation log is a single
//! cross-volume DB, so one `OperationLogWriter` lives in managed state (D1).
//!
//! ## Send discipline (D4)
//!
//! The channel is a bounded `sync_channel`, so `record_items` BLOCKS briefly if
//! the writer is behind rather than dropping — lossless with backpressure,
//! matching importance's actual behavior (`SyncSender::send` blocks when full,
//! errors only on receiver disconnect). This is safe for the "logging never
//! slows an op" requirement: a batched row insert is far cheaper than the
//! per-item file I/O the op already does, so the writer outpaces every real op
//! and the channel effectively never fills; the block is a theoretical backstop.
//! The one thing that could stall the writer is a long retention vacuum on the
//! same thread. An age-only prune reclaims one bounded `incremental_vacuum` slice
//! and lets the periodic timer drain the rest over ticks; only a size-budget prune
//! reclaims fully (it must, to honor the budget), and that runs off the hot path
//! on the retention timer, not during a capture burst ([`handle_prune`]).
//!
//! A DB *error* on a single item row (not fullness) logs a warning and drops
//! THAT row — the operation never fails for a journal problem. That's exactly
//! why finalize returns per-`row_role` durable-row counts: the M2 capture layer
//! compares them against the items it issued and, on a shortfall, downgrades a
//! `rollback_unit` gap to `not_rollbackable(journal_incomplete)` or a
//! `search_only` gap to `search_coverage = top_level_only` (D4). This writer
//! provides the counts; it does not itself compute eligibility (that's M2/M3).

use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;

use rusqlite::{Connection, OptionalExtension};

use super::store::{OperationLogStoreError, fold_name, intern_dir, open_write_connection};
use super::types::{
    ArchiveSubkind, EntryType, ExecutionStatus, Initiator, ItemOutcome, NotRollbackableReason, OpKind, RollbackState,
    RowRole, SearchCoverage, SearchCoverageReason,
};
use crate::ignore_poison::IgnorePoison;

/// Bounded channel capacity. Large enough that a burst of per-item records never
/// blocks a real op in practice (the writer drains far faster than file I/O
/// produces items); the bound is the backpressure backstop, not a hot path.
const CHANNEL_CAPACITY: usize = 1024;

/// The header of an operation, known when it opens. The subkind and terminal
/// state arrive later at finalize (Finding 3): `open` stays generic.
#[derive(Debug, Clone)]
pub struct OpenOperation {
    /// The pipeline's `operation_id` UUID, reused as the journal PK so the row
    /// correlates 1:1 with the live op.
    pub op_id: String,
    pub kind: OpKind,
    pub initiator: Initiator,
    pub source_volume_id: Option<String>,
    pub dest_volume_id: Option<String>,
    /// The planned total from the scan (informational — NOT the completeness
    /// yardstick; see D4).
    pub item_count: u64,
    /// When the op started (opaque epoch integer; the caller owns the clock).
    pub started_at: i64,
    /// Set on a rollback op: the id of the operation it reverses.
    pub rolls_back_op_id: Option<String>,
    /// Usually `Running`; a queued op may open as `Queued`.
    pub execution_status: ExecutionStatus,
}

/// One item to journal. Dir prefixes are full paths here; the writer interns
/// them to `dir_id`s and folds the leaf names inside its transaction.
#[derive(Debug, Clone)]
pub struct JournalItem {
    pub seq: i64,
    pub entry_type: EntryType,
    pub row_role: RowRole,
    pub source_volume_id: String,
    /// The parent directory path of the source item.
    pub source_dir: String,
    pub source_name: String,
    pub dest_volume_id: Option<String>,
    pub dest_dir: Option<String>,
    pub dest_name: Option<String>,
    pub size: Option<i64>,
    pub mtime: Option<i64>,
    pub outcome: ItemOutcome,
    pub overwrote: bool,
}

/// The terminal update at finalize. The capture layer (M2) computes the
/// eligibility (`rollback_state` + reason) and coverage from what actually
/// happened — the archive subkind and net-new flag feed THAT computation
/// upstream; this writer stores the already-typed result and reports the durable
/// counts.
#[derive(Debug, Clone)]
pub struct FinalizeOperation {
    pub op_id: String,
    pub execution_status: ExecutionStatus,
    pub rollback_state: RollbackState,
    pub not_rollbackable_reason: Option<NotRollbackableReason>,
    /// The `archive_edit` subkind, supplied by the capturing driver (Finding 3);
    /// `None` for non-archive ops.
    pub archive_subkind: Option<ArchiveSubkind>,
    pub search_coverage: SearchCoverage,
    pub search_coverage_reason: Option<SearchCoverageReason>,
    pub ended_at: i64,
    /// The planned total, refined at finalize from what the op actually scanned
    /// (M6 rider). `None` keeps the value stored at `open` — the finalize paths
    /// that can't observe a real total (instant creates, archive edits, or a
    /// direct-call test with no status cache) leave it untouched.
    pub item_count: Option<u64>,
    pub items_done: u64,
    pub bytes_total: u64,
    /// An optional dev-only summary for the Debug panel / dump bin. NEVER shown
    /// in the alpha dialog (that label is formatted client-side from typed
    /// fields so it localizes — D2).
    pub dev_summary: Option<String>,
}

/// Durable row counts per `row_role`, returned by finalize — the input to the M2
/// completeness check (D4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FinalizeOutcome {
    pub rollback_unit_rows: u64,
    pub search_only_rows: u64,
}

/// A retention request (D9). Prunes whole operations by age and/or a size budget,
/// GCs the interned dirs the pruned ops orphaned, then reclaims freed pages.
#[derive(Debug, Clone)]
pub struct PruneRequest {
    /// Prune operations that ended more than this many seconds before
    /// `now_secs`. `None` ⇒ no age prune.
    pub max_age_secs: Option<u64>,
    /// A disk budget in bytes. When the DB's live size exceeds it, prune the
    /// oldest whole operations until it fits, then reclaim so the file actually
    /// shrinks. `None` ⇒ no size prune.
    pub max_size_bytes: Option<u64>,
    /// The current time (seconds), supplied by the caller (no clock in the
    /// store — keeps pruning testable).
    pub now_secs: u64,
    /// Run a bounded `incremental_vacuum` slice after an age-only prune to return
    /// freed pages to the OS. A size prune always reclaims fully (it must, to honor
    /// the budget), regardless of this flag.
    pub vacuum: bool,
}

enum WriteMessage {
    OpenOperation(Box<OpenOperation>),
    RecordItems {
        op_id: String,
        items: Vec<JournalItem>,
    },
    FinalizeOperation {
        finalize: Box<FinalizeOperation>,
        reply: mpsc::Sender<FinalizeOutcome>,
    },
    SetRollbackState {
        op_id: String,
        state: RollbackState,
        reason: Option<NotRollbackableReason>,
        reply: mpsc::Sender<()>,
    },
    SetItemOutcomes {
        op_id: String,
        updates: Vec<(i64, ItemOutcome)>,
        reply: mpsc::Sender<()>,
    },
    Prune(PruneRequest),
    Flush(mpsc::Sender<()>),
    Shutdown,
}

/// A cloneable handle to the operation-log writer thread.
#[derive(Clone)]
pub struct OperationLogWriter {
    sender: mpsc::SyncSender<WriteMessage>,
    thread_handle: Arc<Mutex<Option<thread::JoinHandle<()>>>>,
    db_path: PathBuf,
}

impl OperationLogWriter {
    /// Spawn the writer thread with its own write connection to `db_path`. The
    /// schema is created/migrated by the connection factory; opening the
    /// [`OperationLogStore`](super::store::OperationLogStore) first is the
    /// canonical schema-lifecycle owner, but the factory migrates idempotently
    /// either way.
    pub fn spawn(db_path: &Path) -> Result<Self, OperationLogStoreError> {
        let conn = open_write_connection(db_path)?;
        let (sender, receiver) = mpsc::sync_channel::<WriteMessage>(CHANNEL_CAPACITY);
        let handle = thread::Builder::new()
            .name("operation-log-writer".into())
            .spawn(move || writer_loop(conn, receiver))
            .map_err(OperationLogStoreError::Io)?;
        Ok(Self {
            sender,
            thread_handle: Arc::new(Mutex::new(Some(handle))),
            db_path: db_path.to_path_buf(),
        })
    }

    /// The DB file this writer serves.
    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    /// Open an operation row (typically `Running`). Blocks briefly under
    /// backpressure (never drops).
    pub fn open_operation(&self, open: OpenOperation) -> Result<(), OperationLogStoreError> {
        self.send(WriteMessage::OpenOperation(Box::new(open)))
    }

    /// Record a batch of items for an open operation. Coalesced into one
    /// transaction; a per-row DB error logs and drops that row (never fails the
    /// caller). Blocks briefly under backpressure.
    pub fn record_items(&self, op_id: &str, items: Vec<JournalItem>) -> Result<(), OperationLogStoreError> {
        self.send(WriteMessage::RecordItems {
            op_id: op_id.to_string(),
            items,
        })
    }

    /// Finalize an operation: write its terminal state and return the durable
    /// per-`row_role` row counts (the M2 completeness input). Blocks for the
    /// reply, so it also acts as a barrier for this op's prior records.
    pub fn finalize_operation(&self, finalize: FinalizeOperation) -> Result<FinalizeOutcome, OperationLogStoreError> {
        let (tx, rx) = mpsc::channel();
        self.send(WriteMessage::FinalizeOperation {
            finalize: Box::new(finalize),
            reply: tx,
        })?;
        rx.recv().map_err(writer_gone)
    }

    /// Transition an operation's `rollback_state` (+ nullable reason). Blocks for
    /// the reply, so it doubles as a barrier: on return the change is durable. The
    /// rollback engine (M3) uses it to set `rolling_back` (as late as possible, at
    /// a successful spawn), to reset on a synchronous spawn failure, and to resolve
    /// the original op when the inverse finalizes.
    pub fn set_rollback_state(
        &self,
        op_id: &str,
        state: RollbackState,
        reason: Option<NotRollbackableReason>,
    ) -> Result<(), OperationLogStoreError> {
        let (tx, rx) = mpsc::channel();
        self.send(WriteMessage::SetRollbackState {
            op_id: op_id.to_string(),
            state,
            reason,
            reply: tx,
        })?;
        rx.recv().map_err(writer_gone)
    }

    /// Set the per-item `outcome` for the given `(seq, outcome)` pairs of an op.
    /// The rollback engine marks an original op's reversed items `rolled_back` and
    /// its skipped items `skipped`. Blocks for the reply (a barrier).
    pub fn set_item_outcomes(
        &self,
        op_id: &str,
        updates: Vec<(i64, ItemOutcome)>,
    ) -> Result<(), OperationLogStoreError> {
        let (tx, rx) = mpsc::channel();
        self.send(WriteMessage::SetItemOutcomes {
            op_id: op_id.to_string(),
            updates,
            reply: tx,
        })?;
        rx.recv().map_err(writer_gone)
    }

    /// Enqueue a retention prune (+ optional bounded vacuum). Fire-and-forget.
    pub fn prune(&self, request: PruneRequest) -> Result<(), OperationLogStoreError> {
        self.send(WriteMessage::Prune(request))
    }

    /// Block until all prior messages are committed.
    pub fn flush_blocking(&self) -> Result<(), OperationLogStoreError> {
        let (tx, rx) = mpsc::channel();
        self.send(WriteMessage::Flush(tx))?;
        let _ = rx.recv();
        Ok(())
    }

    /// Shut the writer down and join its thread. Idempotent.
    pub fn shutdown(&self) {
        let _ = self.sender.send(WriteMessage::Shutdown);
        if let Some(handle) = self.thread_handle.lock_ignore_poison().take() {
            let _ = handle.join();
        }
    }

    fn send(&self, msg: WriteMessage) -> Result<(), OperationLogStoreError> {
        self.sender.send(msg).map_err(|_| writer_gone(mpsc::RecvError))
    }
}

fn writer_gone(_: mpsc::RecvError) -> OperationLogStoreError {
    OperationLogStoreError::Io(std::io::Error::new(
        std::io::ErrorKind::BrokenPipe,
        "operation-log writer thread is gone",
    ))
}

/// The writer thread's main loop: own the write connection, apply each message,
/// exit on `Shutdown` or channel close.
fn writer_loop(mut conn: Connection, receiver: mpsc::Receiver<WriteMessage>) {
    while let Ok(msg) = receiver.recv() {
        match msg {
            WriteMessage::OpenOperation(open) => {
                if let Err(e) = apply_open(&conn, &open) {
                    log::warn!(target: "operation_log", "open_operation({}) failed: {e}", open.op_id);
                }
            }
            WriteMessage::RecordItems { op_id, items } => apply_record_items(&mut conn, &op_id, &items),
            WriteMessage::FinalizeOperation { finalize, reply } => {
                let outcome = apply_finalize(&conn, &finalize).unwrap_or_else(|e| {
                    log::warn!(target: "operation_log", "finalize_operation({}) failed: {e}", finalize.op_id);
                    FinalizeOutcome {
                        rollback_unit_rows: 0,
                        search_only_rows: 0,
                    }
                });
                let _ = reply.send(outcome);
            }
            WriteMessage::SetRollbackState {
                op_id,
                state,
                reason,
                reply,
            } => {
                if let Err(e) = apply_set_rollback_state(&conn, &op_id, state, reason) {
                    log::warn!(target: "operation_log", "set_rollback_state({op_id}) failed: {e}");
                }
                let _ = reply.send(());
            }
            WriteMessage::SetItemOutcomes { op_id, updates, reply } => {
                if let Err(e) = apply_set_item_outcomes(&mut conn, &op_id, &updates) {
                    log::warn!(target: "operation_log", "set_item_outcomes({op_id}) failed: {e}");
                }
                let _ = reply.send(());
            }
            WriteMessage::Prune(request) => {
                if let Err(e) = handle_prune(&mut conn, &request) {
                    log::warn!(target: "operation_log", "prune failed: {e}");
                }
            }
            WriteMessage::Flush(done) => {
                let _ = done.send(());
            }
            WriteMessage::Shutdown => break,
        }
    }
}

/// Insert the operation header. `rollback_state` opens as `not_rollbackable`
/// (finalize computes the real eligibility); a crash before finalize thus leaves
/// the op honestly not-rollbackable.
fn apply_open(conn: &Connection, open: &OpenOperation) -> Result<(), OperationLogStoreError> {
    conn.execute(
        "INSERT INTO operations
            (op_id, kind, initiator, execution_status, rollback_state, rolls_back_op_id,
             source_volume_id, dest_volume_id, started_at, item_count, search_coverage)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        rusqlite::params![
            open.op_id,
            open.kind.as_token(),
            open.initiator.as_token(),
            open.execution_status.as_token(),
            RollbackState::NotRollbackable.as_token(),
            open.rolls_back_op_id,
            open.source_volume_id,
            open.dest_volume_id,
            open.started_at,
            open.item_count as i64,
            SearchCoverage::Full.as_token(),
        ],
    )?;
    Ok(())
}

/// Insert an item batch in ONE transaction, interning dirs and folding names. A
/// per-row error logs and drops that row (D4: a journal problem never fails the
/// op); the surviving rows still commit.
fn apply_record_items(conn: &mut Connection, op_id: &str, items: &[JournalItem]) {
    let tx = match conn.unchecked_transaction() {
        Ok(tx) => tx,
        Err(e) => {
            log::warn!(target: "operation_log", "record_items({op_id}): couldn't open transaction: {e}");
            return;
        }
    };
    for item in items {
        if let Err(e) = insert_one_item(&tx, op_id, item) {
            // Drop this row, keep going — the op must not fail for a journal
            // problem, and finalize's completeness count will catch the hole.
            log::warn!(target: "operation_log", "record_items({op_id}): dropping item seq {}: {e}", item.seq);
        }
    }
    if let Err(e) = tx.commit() {
        log::warn!(target: "operation_log", "record_items({op_id}): commit failed: {e}");
    }
}

fn insert_one_item(conn: &Connection, op_id: &str, item: &JournalItem) -> Result<(), OperationLogStoreError> {
    let source_dir_id = intern_dir(conn, &item.source_volume_id, &item.source_dir)?;
    let (dest_dir_id, dest_name_folded) = match (&item.dest_volume_id, &item.dest_dir, &item.dest_name) {
        (Some(vol), Some(dir), Some(name)) => (Some(intern_dir(conn, vol, dir)?), Some(fold_name(name))),
        _ => (None, None),
    };
    conn.execute(
        "INSERT INTO operation_items
            (op_id, seq, entry_type, row_role, source_dir_id, source_name, source_name_folded,
             dest_dir_id, dest_name, dest_name_folded, size, mtime, outcome, overwrote)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
        rusqlite::params![
            op_id,
            item.seq,
            item.entry_type.as_token(),
            item.row_role.as_token(),
            source_dir_id,
            item.source_name,
            fold_name(&item.source_name),
            dest_dir_id,
            item.dest_name,
            dest_name_folded,
            item.size,
            item.mtime,
            item.outcome.as_token(),
            item.overwrote as i64,
        ],
    )?;
    Ok(())
}

/// Write the terminal state and return the per-`row_role` durable counts.
fn apply_finalize(conn: &Connection, f: &FinalizeOperation) -> Result<FinalizeOutcome, OperationLogStoreError> {
    conn.execute(
        "UPDATE operations SET
            execution_status = ?2,
            rollback_state = ?3,
            not_rollbackable_reason = ?4,
            archive_subkind = ?5,
            search_coverage = ?6,
            search_coverage_reason = ?7,
            ended_at = ?8,
            -- COALESCE keeps the open-time item_count when finalize supplies NULL
            -- (instant/archive/no-status paths), else refines it to the scanned total.
            item_count = COALESCE(?9, item_count),
            items_done = ?10,
            bytes_total = ?11,
            dev_summary = ?12
         WHERE op_id = ?1",
        rusqlite::params![
            f.op_id,
            f.execution_status.as_token(),
            f.rollback_state.as_token(),
            f.not_rollbackable_reason.map(|r| r.as_token()),
            f.archive_subkind.map(|s| s.as_token()),
            f.search_coverage.as_token(),
            f.search_coverage_reason.map(|r| r.as_token()),
            f.ended_at,
            f.item_count.map(|c| c as i64),
            f.items_done as i64,
            f.bytes_total as i64,
            f.dev_summary,
        ],
    )?;
    count_rows_by_role(conn, &f.op_id)
}

fn count_rows_by_role(conn: &Connection, op_id: &str) -> Result<FinalizeOutcome, OperationLogStoreError> {
    let count_for = |role: RowRole| -> Result<u64, OperationLogStoreError> {
        let n: i64 = conn.query_row(
            "SELECT COUNT(*) FROM operation_items WHERE op_id = ?1 AND row_role = ?2",
            rusqlite::params![op_id, role.as_token()],
            |row| row.get(0),
        )?;
        Ok(n as u64)
    };
    Ok(FinalizeOutcome {
        rollback_unit_rows: count_for(RowRole::RollbackUnit)?,
        search_only_rows: count_for(RowRole::SearchOnly)?,
    })
}

/// Transition an op's `rollback_state` + nullable reason (M3). A no-op if the
/// op_id is unknown (the row count is 0, not an error).
fn apply_set_rollback_state(
    conn: &Connection,
    op_id: &str,
    state: RollbackState,
    reason: Option<NotRollbackableReason>,
) -> Result<(), OperationLogStoreError> {
    conn.execute(
        "UPDATE operations SET rollback_state = ?2, not_rollbackable_reason = ?3 WHERE op_id = ?1",
        rusqlite::params![op_id, state.as_token(), reason.map(|r| r.as_token())],
    )?;
    Ok(())
}

/// Set per-item `outcome`s by `(op_id, seq)` in one transaction (M3). A seq with
/// no matching row updates nothing (not an error).
fn apply_set_item_outcomes(
    conn: &mut Connection,
    op_id: &str,
    updates: &[(i64, ItemOutcome)],
) -> Result<(), OperationLogStoreError> {
    let tx = conn.unchecked_transaction()?;
    {
        let mut stmt = tx.prepare_cached("UPDATE operation_items SET outcome = ?3 WHERE op_id = ?1 AND seq = ?2")?;
        for (seq, outcome) in updates {
            stmt.execute(rusqlite::params![op_id, seq, outcome.as_token()])?;
        }
    }
    tx.commit()?;
    Ok(())
}

// ── Retention (mechanism; M4 wires enforcement) ──────────────────────────────

/// Tiered `incremental_vacuum` caps, mirroring `indexing/writer/maintenance.rs`:
/// skip the lock below `MIN`, hold a steady cap for a modest freelist, ramp to
/// drain a real backlog. Bounded so a big prune never stops the world.
const VACUUM_MIN_FREELIST: i64 = 1_000;
const VACUUM_STEADY_CAP: i64 = 2_000;
const VACUUM_BACKLOG_THRESHOLD: i64 = 20_000;
const VACUUM_BACKLOG_CAP: i64 = 20_000;

fn pick_vacuum_cap(freelist: i64) -> Option<i64> {
    if freelist < VACUUM_MIN_FREELIST {
        None
    } else if freelist < VACUUM_BACKLOG_THRESHOLD {
        Some(VACUUM_STEADY_CAP)
    } else {
        Some(VACUUM_BACKLOG_CAP)
    }
}

/// Prune whole operations by age and/or a size budget, GC orphaned interned dirs,
/// then reclaim freed pages. Whole-operation pruning keeps rollback pairs
/// consistent: a pruned op's dangling `rolls_back_op_id` links (in surviving ops)
/// are nulled, never left dangling. Ops a live rollback touches are never pruned
/// (see [`prunable_ops_fragment`]) so its streamed source rows can't vanish
/// mid-stream (Finding 6/7).
fn handle_prune(conn: &mut Connection, request: &PruneRequest) -> Result<(), OperationLogStoreError> {
    if let Some(max_age) = request.max_age_secs {
        let cutoff = request.now_secs.saturating_sub(max_age) as i64;
        prune_by_age(conn, cutoff)?;
    }

    if let Some(budget) = request.max_size_bytes {
        prune_by_size(conn, budget)?;
    }

    // GC the dirs the pruned ops orphaned, once, covering both passes.
    {
        let tx = conn.unchecked_transaction()?;
        gc_orphan_dirs(&tx)?;
        tx.commit()?;
    }

    // A size prune must actually return the freed pages to the OS to honor the
    // budget, so it drains the freelist fully and truncates. An age-only prune
    // just does one bounded slice (the periodic timer drains the rest over ticks),
    // keeping the writer responsive to capture.
    if request.max_size_bytes.is_some() {
        reclaim_fully(conn);
    } else if request.vacuum {
        run_bounded_vacuum(conn);
    }
    Ok(())
}

/// The SQL predicate for an op that IS safe to prune — i.e. NOT protected by an
/// in-flight rollback. It excludes any op in `rolling_back` (the original, whose
/// rows a live rollback streams across successive read connections) and the
/// `rolls_back_op_id` target of one. Interpolates the stable `rolling_back` token
/// (a compile-time constant, no injection surface); the unfinished inverse op is
/// separately protected by the `ended_at IS NOT NULL` gate every prune applies.
fn prunable_ops_fragment() -> String {
    let rolling_back = RollbackState::RollingBack.as_token();
    format!(
        "rollback_state <> '{rolling_back}' \
         AND op_id NOT IN (SELECT rolls_back_op_id FROM operations \
                           WHERE rollback_state = '{rolling_back}' AND rolls_back_op_id IS NOT NULL)"
    )
}

/// Prune every finished, unprotected op older than `cutoff` in one transaction.
fn prune_by_age(conn: &mut Connection, cutoff: i64) -> Result<(), OperationLogStoreError> {
    let prunable = prunable_ops_fragment();
    let predicate = format!("ended_at IS NOT NULL AND ended_at < {cutoff} AND {prunable}");
    let selector = format!("SELECT op_id FROM operations WHERE {predicate}");

    let tx = conn.unchecked_transaction()?;
    // Null any SURVIVING op's rollback link that points at an op about to be
    // pruned, BEFORE the delete — otherwise the self-FK
    // (`rolls_back_op_id REFERENCES operations`) rejects deleting a referenced op.
    // A rolled-back pair whose both halves fall in the prune set deletes together;
    // a split pair leaves the survivor with a nulled link, never a dangling one.
    tx.execute(
        &format!("UPDATE operations SET rolls_back_op_id = NULL WHERE rolls_back_op_id IN ({selector})"),
        [],
    )?;
    tx.execute(&format!("DELETE FROM operation_items WHERE op_id IN ({selector})"), [])?;
    tx.execute(&format!("DELETE FROM operations WHERE {predicate}"), [])?;
    tx.commit()?;
    Ok(())
}

/// Prune the oldest whole operations until the DB's live size is within `budget`.
/// Live size is `(page_count - freelist) * page_size` — the size the file would
/// have after a full vacuum — so the loop makes progress even before pages are
/// reclaimed (each delete grows the freelist, shrinking live size). Stops when
/// under budget or nothing prunable remains (e.g. everything left is protected by
/// an in-flight rollback).
fn prune_by_size(conn: &mut Connection, budget: u64) -> Result<(), OperationLogStoreError> {
    let prunable = prunable_ops_fragment();
    let oldest_sql = format!(
        "SELECT op_id FROM operations WHERE ended_at IS NOT NULL AND {prunable} \
         ORDER BY ended_at ASC, started_at ASC, op_id ASC LIMIT 1"
    );
    loop {
        if live_size_bytes(conn)? <= budget {
            return Ok(());
        }
        let seed: Option<String> = conn
            .prepare_cached(&oldest_sql)?
            .query_row([], |row| row.get(0))
            .optional()?;
        let Some(seed) = seed else {
            // Nothing left we're allowed to prune; the vacuum still reclaims what
            // the age/earlier passes freed.
            return Ok(());
        };
        let set = rollback_pair_component(conn, &seed)?;
        let tx = conn.unchecked_transaction()?;
        delete_op_set(&tx, &set)?;
        tx.commit()?;
    }
}

/// The op plus its rollback pair partners (the op it rolls back, and any op that
/// rolls it back), so a rolled-back pair prunes together. Protected partners are
/// excluded from the delete set — [`delete_op_set`] nulls the dangling link to
/// them instead. `seed` itself is never protected (the caller selects only
/// unprotected ops).
fn rollback_pair_component(conn: &Connection, seed: &str) -> Result<Vec<String>, OperationLogStoreError> {
    let prunable = prunable_ops_fragment();
    let mut set = vec![seed.to_string()];
    let mut add = |op_id: Option<String>| {
        if let Some(id) = op_id
            && !set.contains(&id)
        {
            set.push(id);
        }
    };
    // The op this one rolls back (if any), unless it's protected.
    let target: Option<String> = conn
        .prepare_cached(&format!(
            "SELECT rolls_back_op_id FROM operations \
             WHERE op_id = ?1 AND rolls_back_op_id IS NOT NULL \
             AND rolls_back_op_id IN (SELECT op_id FROM operations WHERE {prunable})"
        ))?
        .query_row(rusqlite::params![seed], |row| row.get(0))
        .optional()?;
    add(target);
    // Ops that roll this one back, unless protected.
    let mut stmt = conn.prepare_cached(&format!(
        "SELECT op_id FROM operations WHERE rolls_back_op_id = ?1 AND {prunable}"
    ))?;
    let inverses = stmt.query_map(rusqlite::params![seed], |row| row.get::<_, String>(0))?;
    for inv in inverses {
        add(Some(inv?));
    }
    Ok(set)
}

/// Delete a set of whole operations (their items too), nulling any surviving op's
/// `rolls_back_op_id` that points into the set first (the self-FK would otherwise
/// reject the delete).
fn delete_op_set(conn: &Connection, op_ids: &[String]) -> Result<(), OperationLogStoreError> {
    if op_ids.is_empty() {
        return Ok(());
    }
    let placeholders = std::iter::repeat_n("?", op_ids.len()).collect::<Vec<_>>().join(", ");
    let params: Vec<&dyn rusqlite::ToSql> = op_ids.iter().map(|s| s as &dyn rusqlite::ToSql).collect();
    conn.execute(
        &format!(
            "UPDATE operations SET rolls_back_op_id = NULL \
             WHERE rolls_back_op_id IN ({placeholders}) AND op_id NOT IN ({placeholders})"
        ),
        [params.as_slice(), params.as_slice()].concat().as_slice(),
    )?;
    conn.execute(
        &format!("DELETE FROM operation_items WHERE op_id IN ({placeholders})"),
        params.as_slice(),
    )?;
    conn.execute(
        &format!("DELETE FROM operations WHERE op_id IN ({placeholders})"),
        params.as_slice(),
    )?;
    Ok(())
}

/// The DB's live size in bytes: `(page_count - freelist) * page_size` — what the
/// file would occupy after a full vacuum. Used as the size-budget yardstick so
/// pruning makes progress before pages are physically reclaimed.
fn live_size_bytes(conn: &Connection) -> Result<u64, OperationLogStoreError> {
    let page_count: i64 = conn.pragma_query_value(None, "page_count", |row| row.get(0))?;
    let freelist: i64 = conn.pragma_query_value(None, "freelist_count", |row| row.get(0))?;
    let page_size: i64 = conn.pragma_query_value(None, "page_size", |row| row.get(0))?;
    Ok(((page_count - freelist).max(0) as u64) * page_size as u64)
}

/// Fully reclaim freed pages to the OS after a size prune: drain the ENTIRE
/// freelist, then TRUNCATE the WAL so the truncation reaches the physical file.
/// Unlike [`run_bounded_vacuum`] this ignores the `pick_vacuum_cap` floor — a size
/// budget can only be honored once the pages actually leave the file, however
/// small the freelist. Retention runs off the hot path, so a full drain here is
/// the point, not a stall to avoid.
fn reclaim_fully(conn: &Connection) {
    // No cap: reclaim every free page in one pass.
    if let Err(e) = conn.execute_batch("PRAGMA incremental_vacuum;") {
        log::warn!(target: "operation_log", "incremental_vacuum failed: {e}");
    }
    // TRUNCATE so the vacuum's page-count reduction reaches the on-disk file (in
    // WAL mode it otherwise lands only in the WAL until the next checkpoint).
    if let Err(e) = conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(())) {
        log::warn!(target: "operation_log", "wal_checkpoint(TRUNCATE) failed: {e}");
    }
}

/// GC interned dirs no longer live: iterate leaf-up, deleting dirs referenced by
/// no item AND no child dir, until stable. This deletes exactly the complement
/// of the referenced-dirs-plus-their-ancestors closure — a referenced dir's
/// whole parent chain survives (path reconstruction walks it), and a pruned
/// dir's ancestors fall away only once nothing live remains under them (D9).
fn gc_orphan_dirs(conn: &Connection) -> Result<(), OperationLogStoreError> {
    loop {
        let deleted = conn.execute(
            "DELETE FROM dirs
             WHERE dir_id NOT IN (SELECT source_dir_id FROM operation_items)
               AND dir_id NOT IN (SELECT dest_dir_id FROM operation_items WHERE dest_dir_id IS NOT NULL)
               AND dir_id NOT IN (SELECT parent_dir_id FROM dirs WHERE parent_dir_id IS NOT NULL)",
            [],
        )?;
        if deleted == 0 {
            break;
        }
    }
    Ok(())
}

/// Run one bounded `incremental_vacuum` slice sized to the current freelist.
fn run_bounded_vacuum(conn: &Connection) {
    let free = match conn.pragma_query_value(None, "freelist_count", |row| row.get::<_, i64>(0)) {
        Ok(n) => n,
        Err(e) => {
            log::warn!(target: "operation_log", "freelist_count query failed: {e}");
            return;
        }
    };
    let Some(cap) = pick_vacuum_cap(free) else {
        return;
    };
    if let Err(e) = conn.execute_batch(&format!("PRAGMA incremental_vacuum({cap});")) {
        log::warn!(target: "operation_log", "incremental_vacuum failed: {e}");
    }
}

#[cfg(test)]
mod retention_tests;
#[cfg(test)]
mod tests;
