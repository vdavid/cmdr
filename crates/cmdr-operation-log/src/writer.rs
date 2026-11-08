//! `OperationLogWriter`: the single writer thread for `operation-log.db`.
//!
//! Mirrors the importance/index one-writer-per-DB discipline: exactly ONE thread
//! owns the single write connection, and all writes cross a bounded channel. The
//! handle is cloneable; every clone shares the one channel and thread. **Unlike
//! importance there is NO per-volume registry** — the operation log is a single
//! cross-volume DB, so one `OperationLogWriter` lives in the app's `OperationLog`.
//!
//! ## Send discipline
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
//! why finalize returns per-`row_role` durable-row counts: the capture layer
//! compares them against the items it issued and, on a shortfall, downgrades a
//! `rollback_unit` gap to `not_rollbackable(journal_incomplete)` or a
//! `search_only` gap to `search_coverage = top_level_only`. This writer
//! provides the counts; it does not itself compute eligibility (that's the capture layer and rollback engine).
//!
//! ## The browser build
//!
//! A page has no thread to put the writer on, so there (`wasm32`) the handle holds
//! the write connection itself and applies each message INLINE, on the caller, in
//! the same function the thread's loop runs ([`apply_message`]). Nothing queues, so
//! nothing can back up: a reply is already there when the caller reads it, and
//! the order is the call order, as the channel's is.

use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
#[cfg(not(target_family = "wasm"))]
use std::thread;

use rusqlite::Connection;

use super::store::{
    OperationLogStoreError, build_tree_item_indexes, fold_name, intern_child_dir, intern_dir, open_write_connection,
};
use super::tree::TreeAxis;
use super::tree::status::BackfillRegistry;
use super::tree::summary::{RowFacts, TreeDirRow};
use super::types::{
    ArchiveSubkind, EntryType, ExecutionStatus, Initiator, ItemOutcome, NotRollbackableReason, OpKind, OperationId,
    RollbackState, RowRole, SearchCoverage, SearchCoverageReason, SkipReason, TreeSummaryState,
};
use cmdr_fs::entry::TagsChange;
use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::volume::NodeIdentity;

/// Bounded channel capacity. Large enough that a burst of per-item records never
/// blocks a real op in practice (the writer drains far faster than file I/O
/// produces items); the bound is the backpressure backstop, not a hot path.
#[cfg(not(target_family = "wasm"))]
const CHANNEL_CAPACITY: usize = 1024;

/// The header of an operation, known when it opens. The subkind and terminal
/// state arrive later at finalize: `open` stays generic.
#[derive(Debug, Clone)]
pub struct OpenOperation {
    /// The pipeline's `operation_id` UUID, reused as the journal PK so the row
    /// correlates 1:1 with the live op.
    pub op_id: OperationId,
    /// What the operation does, in the journal's durable taxonomy.
    pub kind: OpKind,
    /// Who started it: the user, the agent, or both.
    pub initiator: Initiator,
    /// The volume the operation reads from, when it has one.
    pub source_volume_id: Option<String>,
    /// The volume it writes to, when that differs from the source.
    pub dest_volume_id: Option<String>,
    /// The planned total from the scan (informational — NOT the completeness
    /// yardstick — the capture layer compares ISSUED rows against written ones).
    pub item_count: u64,
    /// When the op started (opaque epoch integer; the caller owns the clock).
    pub started_at: i64,
    /// Set on a rollback op: the id of the operation it reverses.
    pub rolls_back_op_id: Option<OperationId>,
    /// Usually `Running`; a queued op may open as `Queued`.
    pub execution_status: ExecutionStatus,
}

/// One item to journal. Dir prefixes are full paths here; the writer interns
/// them to `dir_id`s and folds the leaf names inside its transaction.
#[derive(Debug, Clone)]
pub struct JournalItem {
    /// The item's position within its operation, unique per op.
    pub seq: i64,
    /// File, directory, or symlink.
    pub entry_type: EntryType,
    /// Whether rollback reverses this row or search only finds it.
    pub row_role: RowRole,
    /// The volume the item came from.
    pub source_volume_id: String,
    /// The parent directory path of the source item.
    pub source_dir: String,
    /// The source item's leaf name.
    pub source_name: String,
    /// The volume the item landed on, when it moved or was copied.
    pub dest_volume_id: Option<String>,
    /// The parent directory path of where the item landed.
    pub dest_dir: Option<String>,
    /// The leaf name it landed under.
    pub dest_name: Option<String>,
    /// The size snapshot rollback verifies against, when known.
    pub size: Option<i64>,
    /// The mtime snapshot rollback verifies against, when known.
    pub mtime: Option<i64>,
    /// What happened to the item.
    pub outcome: ItemOutcome,
    /// Whether landing it replaced an existing entry.
    pub overwrote: bool,
    /// Which node the item landed as at its dest, read right after it landed.
    /// Only a trash records it: a restore then refuses a stranger that took the
    /// in-Trash name since.
    pub dest_identity: Option<NodeIdentity>,
    /// The file's Finder-tags value before and after, for a tag write's row only.
    pub tags: Option<TagsChange>,
}

/// The terminal update at finalize. The capture layer computes the
/// eligibility (`rollback_state` + reason) and coverage from what actually
/// happened — the archive subkind and net-new flag feed THAT computation
/// upstream; this writer stores the already-typed result and reports the durable
/// counts.
#[derive(Debug, Clone)]
pub struct FinalizeOperation {
    /// The operation to finalize.
    pub op_id: String,
    /// How it ended.
    pub execution_status: ExecutionStatus,
    /// Whether it can be rolled back, as the capture layer computed it.
    pub rollback_state: RollbackState,
    /// Why not, when it can't.
    pub not_rollbackable_reason: Option<NotRollbackableReason>,
    /// The `archive_edit` subkind, supplied by the capturing driver (the journal
    /// can't derive it); `None` for non-archive ops.
    pub archive_subkind: Option<ArchiveSubkind>,
    /// How completely name search covers the operation's items.
    pub search_coverage: SearchCoverage,
    /// Why coverage is less than full, when it is.
    pub search_coverage_reason: Option<SearchCoverageReason>,
    /// When the operation ended (opaque epoch integer; the caller owns the clock).
    pub ended_at: i64,
    /// The planned total, refined at finalize from what the op actually scanned
    /// (the header-aggregate rider). `None` keeps the value stored at `open` — the finalize paths
    /// that can't observe a real total (instant creates, archive edits, or a
    /// direct-call test with no status cache) leave it untouched.
    pub item_count: Option<u64>,
    /// Items the operation finished.
    pub items_done: u64,
    /// Bytes it moved or wrote.
    pub bytes_total: u64,
    /// An optional dev-only summary for the Debug panel / dump bin. NEVER shown
    /// in the alpha dialog (that label is formatted client-side from typed
    /// fields so it localizes).
    pub dev_summary: Option<String>,
}

/// One item's terminal outcome as a rollback resolved it: the row to update (by `seq`),
/// the outcome, and — for a skip — the typed reason.
///
/// `skip_reason` is `Some` only alongside [`ItemOutcome::Skipped`]; a reversed item
/// carries `None`, which CLEARS any reason a previous attempt recorded, so the column
/// never outlives the outcome it explains.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ItemOutcomeUpdate {
    /// The item row to update.
    pub(crate) seq: i64,
    /// The outcome the rollback reached for it.
    pub(crate) outcome: ItemOutcome,
    /// Why it was skipped, for a skip.
    pub(crate) skip_reason: Option<SkipReason>,
}

/// Durable row counts per `row_role`, returned by finalize — the input to the capture
/// completeness check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FinalizeOutcome {
    /// `rollback_unit` rows durably written.
    pub rollback_unit_rows: u64,
    /// `search_only` rows durably written.
    pub search_only_rows: u64,
}

/// A retention request. Prunes whole operations by age and/or a size budget,
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
        updates: Vec<ItemOutcomeUpdate>,
        reply: mpsc::Sender<()>,
    },
    EndInterrupted {
        op_id: String,
        eligibility: Option<(RollbackState, Option<NotRollbackableReason>)>,
        reply: mpsc::Sender<()>,
    },
    FillSelfDirs {
        op_id: String,
        seqs: Vec<i64>,
        reply: mpsc::Sender<()>,
    },
    WriteTreeSummary {
        op_id: String,
        generation: i64,
        rows: Vec<TreeDirRow>,
        reply: mpsc::Sender<Option<TreeSummaryWrite>>,
    },
    Prune(PruneRequest),
    /// The registry hears when they exist ([`BackfillRegistry::when_indexes_ready`]).
    BuildTreeIndexes(Arc<BackfillRegistry>),
    Flush(mpsc::Sender<()>),
    Shutdown,
}

/// A cloneable handle to the operation-log writer thread.
#[derive(Clone)]
pub struct OperationLogWriter {
    #[cfg(not(target_family = "wasm"))]
    sender: mpsc::SyncSender<WriteMessage>,
    #[cfg(not(target_family = "wasm"))]
    thread_handle: Arc<Mutex<Option<thread::JoinHandle<()>>>>,
    /// The browser build's write connection, applied to inline; `None` once shut
    /// down (as a gone thread is).
    #[cfg(target_family = "wasm")]
    inline: Arc<Mutex<Option<Connection>>>,
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
        #[cfg(not(target_family = "wasm"))]
        {
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
        // The browser build's writer: the connection, applied to inline (module docs).
        #[cfg(target_family = "wasm")]
        Ok(Self {
            inline: Arc::new(Mutex::new(Some(conn))),
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
    /// per-`row_role` row counts (the capture completeness input). Blocks for the
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
    /// rollback engine uses it to set `rolling_back` (as late as possible, at
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

    /// Set the per-item terminal outcomes of an op, by `seq`. The rollback engine marks
    /// an original op's reversed items `rolled_back` and its skipped items `skipped`,
    /// each skip carrying the typed [`SkipReason`] that produced it. Blocks for the
    /// reply (a barrier).
    pub(crate) fn set_item_outcomes(
        &self,
        op_id: &str,
        updates: Vec<ItemOutcomeUpdate>,
    ) -> Result<(), OperationLogStoreError> {
        let (tx, rx) = mpsc::channel();
        self.send(WriteMessage::SetItemOutcomes {
            op_id: op_id.to_string(),
            updates,
            reply: tx,
        })?;
        rx.recv().map_err(writer_gone)
    }

    /// End an operation a crash left `queued` or `running` (`interrupted.rs`): `failed`,
    /// ended when it started (the only time the journal knows), and, with
    /// `eligibility`, the rollback state the facts allow. Touches nothing once the op
    /// has ended. Blocks for the reply (a barrier).
    pub(crate) fn end_interrupted(
        &self,
        op_id: &str,
        eligibility: Option<(RollbackState, Option<NotRollbackableReason>)>,
    ) -> Result<(), OperationLogStoreError> {
        let (tx, rx) = mpsc::channel();
        self.send(WriteMessage::EndInterrupted {
            op_id: op_id.to_string(),
            eligibility,
            reply: tx,
        })?;
        rx.recv().map_err(writer_gone)
    }

    /// Enqueue a retention prune (+ optional bounded vacuum). Fire-and-forget.
    pub fn prune(&self, request: PruneRequest) -> Result<(), OperationLogStoreError> {
        self.send(WriteMessage::Prune(request))
    }

    /// Enqueue building the item tree's folder indexes if they're missing
    /// (`store/tree_indexes.rs`). Fire-and-forget: what's sent after it waits behind
    /// it, so call it once, right after open, off the app's main thread's path. Tells
    /// `registry` once they exist, so a tree waiting on them hears it.
    pub(crate) fn build_tree_indexes(&self, registry: Arc<BackfillRegistry>) -> Result<(), OperationLogStoreError> {
        self.send(WriteMessage::BuildTreeIndexes(registry))
    }

    /// Block until all prior messages are committed.
    pub fn flush_blocking(&self) -> Result<(), OperationLogStoreError> {
        let (tx, rx) = mpsc::channel();
        self.send(WriteMessage::Flush(tx))?;
        let _ = rx.recv();
        Ok(())
    }

    /// Shut the writer down and join its thread (in the browser build, close its
    /// connection). Idempotent.
    pub fn shutdown(&self) {
        #[cfg(not(target_family = "wasm"))]
        {
            let _ = self.sender.send(WriteMessage::Shutdown);
            if let Some(handle) = self.thread_handle.lock_ignore_poison().take() {
                let _ = handle.join();
            }
        }
        // Already shut is the only refusal, and the answer is the same.
        #[cfg(target_family = "wasm")]
        let _ = self.send(WriteMessage::Shutdown);
    }

    #[cfg(not(target_family = "wasm"))]
    fn send(&self, msg: WriteMessage) -> Result<(), OperationLogStoreError> {
        self.sender.send(msg).map_err(|_| writer_gone(mpsc::RecvError))
    }

    /// Applies `msg` now, on the caller (the browser build has no thread).
    #[cfg(target_family = "wasm")]
    fn send(&self, msg: WriteMessage) -> Result<(), OperationLogStoreError> {
        let mut inline = self.inline.lock_ignore_poison();
        let Some(conn) = inline.as_mut() else {
            return Err(writer_gone(mpsc::RecvError));
        };
        if apply_message(conn, msg).is_break() {
            inline.take();
        }
        Ok(())
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
#[cfg(not(target_family = "wasm"))]
fn writer_loop(mut conn: Connection, receiver: mpsc::Receiver<WriteMessage>) {
    while let Ok(msg) = receiver.recv() {
        if apply_message(&mut conn, msg).is_break() {
            break;
        }
    }
}

/// Applies one message to the write connection: what the writer thread does with
/// each (and the browser build's inline writer, with each call). `Break` is
/// `Shutdown`.
fn apply_message(conn: &mut Connection, msg: WriteMessage) -> std::ops::ControlFlow<()> {
    match msg {
        WriteMessage::OpenOperation(open) => {
            if let Err(e) = apply_open(conn, &open) {
                log::warn!(target: "operation_log", "open_operation({}) failed: {e}", open.op_id);
            }
        }
        WriteMessage::RecordItems { op_id, items } => apply_record_items(conn, &op_id, &items),
        WriteMessage::FinalizeOperation { finalize, reply } => {
            let outcome = apply_finalize(conn, &finalize).unwrap_or_else(|e| {
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
            if let Err(e) = apply_set_rollback_state(conn, &op_id, state, reason) {
                log::warn!(target: "operation_log", "set_rollback_state({op_id}) failed: {e}");
            }
            let _ = reply.send(());
        }
        WriteMessage::SetItemOutcomes { op_id, updates, reply } => {
            if let Err(e) = apply_set_item_outcomes(conn, &op_id, &updates) {
                log::warn!(target: "operation_log", "set_item_outcomes({op_id}) failed: {e}");
            }
            let _ = reply.send(());
        }
        WriteMessage::EndInterrupted {
            op_id,
            eligibility,
            reply,
        } => {
            if let Err(e) = apply_end_interrupted(conn, &op_id, eligibility) {
                log::warn!(target: "operation_log", "end_interrupted({op_id}) failed: {e}");
            }
            let _ = reply.send(());
        }
        WriteMessage::FillSelfDirs { op_id, seqs, reply } => {
            apply_fill_self_dirs(conn, &op_id, &seqs);
            let _ = reply.send(());
        }
        WriteMessage::WriteTreeSummary {
            op_id,
            generation,
            rows,
            reply,
        } => {
            let _ = reply.send(apply_write_tree_summary(conn, &op_id, generation, &rows));
        }
        WriteMessage::Prune(request) => {
            if let Err(e) = handle_prune(conn, &request) {
                log::warn!(target: "operation_log", "prune failed: {e}");
            }
        }
        WriteMessage::BuildTreeIndexes(registry) => match build_tree_item_indexes(conn) {
            Ok(()) => registry.mark_indexes_ready(),
            Err(e) => log::warn!(target: "operation_log", "building the item tree's folder indexes failed: {e}"),
        },
        WriteMessage::Flush(done) => {
            let _ = done.send(());
        }
        WriteMessage::Shutdown => return std::ops::ControlFlow::Break(()),
    }
    std::ops::ControlFlow::Continue(())
}

/// Insert the operation header. `rollback_state` opens as `not_rollbackable`
/// (finalize computes the real eligibility); a crash before finalize leaves it so
/// until the next open ends it (`interrupted.rs`). Its tree summary opens `built`: it has no rows
/// yet, and every batch keeps it exact from here (`tree_summary.rs`).
fn apply_open(conn: &Connection, open: &OpenOperation) -> Result<(), OperationLogStoreError> {
    conn.execute(
        "INSERT INTO operations
            (op_id, kind, initiator, execution_status, rollback_state, rolls_back_op_id,
             source_volume_id, dest_volume_id, started_at, item_count, search_coverage, tree_summary)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        rusqlite::params![
            open.op_id.as_str(),
            open.kind.as_token(),
            open.initiator.as_token(),
            open.execution_status.as_token(),
            RollbackState::NotRollbackable.as_token(),
            open.rolls_back_op_id.as_ref().map(OperationId::as_str),
            open.source_volume_id,
            open.dest_volume_id,
            open.started_at,
            open.item_count as i64,
            SearchCoverage::Full.as_token(),
            TreeSummaryState::Built.as_token(),
        ],
    )?;
    Ok(())
}

/// Insert an item batch in ONE transaction, interning dirs and folding names, with
/// the op's tree-summary delta for exactly the rows that inserted. A per-row error
/// logs and drops that row (a journal problem never fails the op); the surviving
/// rows still commit.
fn apply_record_items(conn: &mut Connection, op_id: &str, items: &[JournalItem]) {
    let tx = match conn.unchecked_transaction() {
        Ok(tx) => tx,
        Err(e) => {
            log::warn!(target: "operation_log", "record_items({op_id}): couldn't open transaction: {e}");
            return;
        }
    };
    let mut summary = SummaryBatch::begin(&tx, op_id);
    for item in items {
        match insert_one_item(&tx, op_id, item, summary.axis()) {
            Ok(facts) => summary.add_inserted(&facts),
            // Drop this row, keep going — the op must not fail for a journal
            // problem, and finalize's completeness count will catch the hole.
            Err(e) => {
                log::warn!(target: "operation_log", "record_items({op_id}): dropping item seq {}: {e}", item.seq);
            }
        }
    }
    summary.finish(&tx, op_id);
    if let Err(e) = tx.commit() {
        log::warn!(target: "operation_log", "record_items({op_id}): commit failed: {e}");
    }
}

/// Insert one row and answer what the tree summary reads of it. A DIR row's own
/// folder is interned on the op's `axis` (`self_dir_id`); if that fails the row still
/// inserts with none, and the summary batch marks the op unbuilt.
fn insert_one_item(
    conn: &Connection,
    op_id: &str,
    item: &JournalItem,
    axis: TreeAxis,
) -> Result<RowFacts, OperationLogStoreError> {
    let source_dir_id = intern_dir(conn, &item.source_volume_id, &item.source_dir)?;
    let (dest_dir_id, dest_name_folded) = match (&item.dest_volume_id, &item.dest_dir, &item.dest_name) {
        (Some(vol), Some(dir), Some(name)) => (Some(intern_dir(conn, vol, dir)?), Some(fold_name(name))),
        _ => (None, None),
    };
    let dest = dest_dir_id.zip(item.dest_name.clone());
    let self_dir_id = match item.entry_type {
        EntryType::Dir => {
            let (parent, name) = axis.side(
                (source_dir_id, &item.source_name),
                dest.as_ref().map(|(dir, name)| (*dir, name.as_str())),
            );
            intern_child_dir(conn, parent, name)
                .inspect_err(|e| {
                    log::warn!(target: "operation_log", "record_items({op_id}): seq {} has no own folder: {e}", item.seq);
                })
                .ok()
        }
        EntryType::File => None,
    };
    conn.execute(
        "INSERT INTO operation_items
            (op_id, seq, entry_type, row_role, source_dir_id, source_name, source_name_folded,
             dest_dir_id, dest_name, dest_name_folded, size, mtime, outcome, overwrote,
             dest_inode, dest_birth_ns, tags_before, tags_after, self_dir_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19)",
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
            item.dest_identity.map(|id| id.inode as i64),
            item.dest_identity.map(|id| id.birth_nanos),
            item.tags.as_ref().map(|tags| tags.before.as_slice()),
            item.tags.as_ref().map(|tags| tags.after.as_slice()),
            self_dir_id,
        ],
    )?;
    Ok(RowFacts {
        seq: item.seq,
        entry_type: item.entry_type,
        row_role: item.row_role,
        outcome: item.outcome,
        skip_reason: None,
        size: item.size,
        source_dir_id,
        source_name: item.source_name.clone(),
        dest,
        self_dir_id,
    })
}

/// Set per-item outcomes (+ the nullable rollback skip reason) by `(op_id, seq)` in one
/// transaction (rollback). A seq with no matching row updates nothing (not an error).
///
/// Both columns are written together on purpose: a reason without its `skipped` outcome
/// — or one left behind by an earlier attempt that has since reversed the item — would
/// be a stale explanation of something that no longer happened.
///
/// The same transaction moves each row's tree-summary counts from its old outcome to
/// its new one (read here, on the writer, by `(op_id, seq)`, so the rollback engine
/// pays no per-row read), and bumps `outcome_generation` so a backfill that read the op
/// before this batch knows its result is stale.
fn apply_set_item_outcomes(
    conn: &mut Connection,
    op_id: &str,
    updates: &[ItemOutcomeUpdate],
) -> Result<(), OperationLogStoreError> {
    let tx = conn.unchecked_transaction()?;
    let mut summary = SummaryBatch::begin(&tx, op_id);
    {
        let mut stmt = tx.prepare_cached(
            "UPDATE operation_items SET outcome = ?3, rollback_skip_reason = ?4 WHERE op_id = ?1 AND seq = ?2",
        )?;
        for update in updates {
            let before = summary.maintains().then(|| read_row_facts(&tx, op_id, update.seq));
            stmt.execute(rusqlite::params![
                op_id,
                update.seq,
                update.outcome.as_token(),
                update.skip_reason.map(|r| r.as_token()),
            ])?;
            match before {
                Some(Ok(rows)) => {
                    for old in rows {
                        let new = RowFacts {
                            outcome: update.outcome,
                            skip_reason: update.skip_reason,
                            ..old.clone()
                        };
                        summary.add_change(&old, &new);
                    }
                }
                Some(Err(e)) => summary.fail(op_id, &e),
                None => {}
            }
        }
    }
    tx.prepare_cached("UPDATE operations SET outcome_generation = outcome_generation + 1 WHERE op_id = ?1")?
        .execute(rusqlite::params![op_id])?;
    summary.finish(&tx, op_id);
    tx.commit()?;
    Ok(())
}

/// Retention: dropping old operations and reclaiming their space, bounded so it
/// never stops a capture burst. `retention.rs` decides when to ask.
mod prune;
use prune::handle_prune;

/// The writes a tree-summary backfill hands the writer: own folders, then the result.
mod backfill;
pub(crate) use backfill::TreeSummaryWrite;
use backfill::{apply_fill_self_dirs, apply_write_tree_summary};

/// How an op ended and its rollback state transitions: the header's state writes.
mod op_state;
use op_state::{apply_end_interrupted, apply_finalize, apply_set_rollback_state};

/// The tree summary's delta, written in the same transaction as the rows it counts.
mod tree_summary;
use tree_summary::{SummaryBatch, read_row_facts};

#[cfg(test)]
mod retention_tests;
#[cfg(test)]
mod tests;
