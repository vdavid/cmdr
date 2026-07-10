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
//! The one thing that could stall the writer — a long retention vacuum on the
//! same thread — is avoided by running `incremental_vacuum` in bounded slices
//! ([`handle_prune`]), never one stop-the-world pass.
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

use rusqlite::Connection;

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

/// A retention request. The MECHANISM lands here (M1); M4 wires the periodic
/// enforcement and the size-budget policy. `max_age_secs` prunes whole
/// operations whose `ended_at` is older than `now_secs - max_age_secs`.
#[derive(Debug, Clone)]
pub struct PruneRequest {
    /// Prune operations that ended more than this many seconds before
    /// `now_secs`. `None` ⇒ no age prune (M4 adds the size-budget prune).
    pub max_age_secs: Option<u64>,
    /// The current time (seconds), supplied by the caller (no clock in the
    /// store — keeps pruning testable).
    pub now_secs: u64,
    /// Run a bounded `incremental_vacuum` slice after pruning to return freed
    /// pages to the OS.
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
            items_done = ?9,
            bytes_total = ?10,
            dev_summary = ?11
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

/// Prune whole operations by age, GC orphaned interned dirs, then run a bounded
/// vacuum slice. Whole-operation pruning keeps rollback pairs consistent: a
/// pruned op's dangling `rolls_back_op_id` links (in surviving ops) are nulled,
/// never left dangling. Ops currently `rolling_back` are skipped so a live
/// rollback's streamed source rows can't vanish mid-stream (Finding 6/7).
fn handle_prune(conn: &mut Connection, request: &PruneRequest) -> Result<(), OperationLogStoreError> {
    if let Some(max_age) = request.max_age_secs {
        let cutoff = request.now_secs.saturating_sub(max_age) as i64;
        let rolling_back = RollbackState::RollingBack.as_token();
        // The set of ops this pass prunes: finished, older than the cutoff, and
        // NOT reversing or being reversed (a live rollback streams its source
        // op's rows across successive reads, so pruning them mid-stream would
        // under-restore — Finding 6/7). Ops the pruned set is reversing are also
        // held: skipping the `rolling_back` state covers the in-flight window.
        const PRUNE_PREDICATE: &str = "ended_at IS NOT NULL AND ended_at < ?1 AND rollback_state <> ?2";

        let tx = conn.unchecked_transaction()?;
        // Null any SURVIVING op's rollback link that points at an op about to be
        // pruned, BEFORE the delete — otherwise the self-FK
        // (`rolls_back_op_id REFERENCES operations`) would reject deleting a
        // referenced op. A rolled-back pair whose both halves fall in the prune
        // set deletes together; a split pair leaves the survivor with a nulled
        // link, never a dangling one.
        tx.execute(
            &format!(
                "UPDATE operations SET rolls_back_op_id = NULL
                 WHERE rolls_back_op_id IN (SELECT op_id FROM operations WHERE {PRUNE_PREDICATE})"
            ),
            rusqlite::params![cutoff, rolling_back],
        )?;
        tx.execute(
            &format!("DELETE FROM operation_items WHERE op_id IN (SELECT op_id FROM operations WHERE {PRUNE_PREDICATE})"),
            rusqlite::params![cutoff, rolling_back],
        )?;
        tx.execute(
            &format!("DELETE FROM operations WHERE {PRUNE_PREDICATE}"),
            rusqlite::params![cutoff, rolling_back],
        )?;
        gc_orphan_dirs(&tx)?;
        tx.commit()?;
    }

    if request.vacuum {
        run_bounded_vacuum(conn);
    }
    Ok(())
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
mod tests;
