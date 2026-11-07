//! `OperationLog`: the operation log's state as a value the host builds and
//! holds, instead of process statics.
//!
//! It owns the journal slot (empty until [`OperationLog::open`] fills it), the
//! recording gate, and the writer once opened. A handle starts detached, so an
//! operation opened before the journal journals nothing, and the host opens it
//! once its data dir is known. Two handles share nothing: a test builds its own.
//! `DETAILS.md` § "The seam: an operation's journal, fixed at open".

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};

use cmdr_fs::ignore_poison::RwLockIgnorePoison;
use cmdr_fs::log_private::LogPrivate;

use super::capture::{FinalizeInputs, OperationJournal, WriterJournal};
use super::recording::Recording;
use super::retention::{self, RetentionLimitsSource};
use super::rollback::reconcile_rolling_back_on_open;
use super::store::{OperationLogStore, OperationLogStoreError};
use super::tree::status::BackfillRegistry;
use super::types::{NotRollbackableReason, OperationId, SearchCoverage, SearchCoverageReason};
use super::writer::{FinalizeOutcome, JournalItem, OpenOperation, OperationLogWriter};

/// The operation log: a cheap-to-clone handle, every clone sharing one state.
#[derive(Clone)]
pub struct OperationLog {
    inner: Arc<Inner>,
}

struct Inner {
    /// The journal, and the writer behind it, published together in ONE write.
    slot: RwLock<Option<Published>>,
    recording: Recording,
    /// Set by the first [`OperationLog::open`], whatever its outcome.
    open_claimed: AtomicBool,
    /// The tree-summary backfills running now, one per op.
    backfills: Arc<BackfillRegistry>,
}

#[derive(Clone)]
struct Published {
    journal: Arc<dyn OperationJournal>,
    /// `None` for a journal a test or bench installed by hand.
    writer: Option<OperationLogWriter>,
}

/// Why [`OperationLog::open`] left the handle without a journal.
#[derive(Debug)]
pub enum OpenError {
    /// This handle already ran `open`; there's one writer per handle.
    AlreadyOpen,
    /// The store couldn't open the DB (migrate, recreate, or a refused downgrade).
    Store(OperationLogStoreError),
    /// The writer thread couldn't open its connection or start.
    Writer(OperationLogStoreError),
}

impl std::fmt::Display for OpenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OpenError::AlreadyOpen => write!(f, "the operation log is already open"),
            OpenError::Store(e) => write!(f, "the operation log store didn't open: {e}"),
            OpenError::Writer(e) => write!(f, "the operation log writer didn't start: {e}"),
        }
    }
}

impl std::error::Error for OpenError {}

impl OperationLog {
    /// An empty handle: recording on, no journal. It journals nothing until
    /// [`Self::open`].
    pub fn detached() -> Self {
        Self {
            inner: Arc::new(Inner {
                slot: RwLock::new(None),
                recording: Recording::new(),
                open_claimed: AtomicBool::new(false),
                backfills: Arc::default(),
            }),
        }
    }

    /// Open `db_path` and start journaling into it: the store migrates the schema
    /// forward (or hands an unparseable file to `set_aside` and recreates it, or
    /// refuses a downgrade), the
    /// writer thread starts, the operations a crash left mid-rollback resolve,
    /// and only THEN do the writer and the journal become reachable, together.
    /// So a rollback can never be dispatched into a reconcile.
    ///
    /// Once per handle: a second call is [`OpenError::AlreadyOpen`] and touches
    /// nothing, and a failed open leaves the handle without a journal for good,
    /// which the app survives (it runs without history rather than refusing to
    /// start).
    pub fn open(&self, db_path: &Path, set_aside: &dyn Fn(&Path) -> Option<PathBuf>) -> Result<(), OpenError> {
        if self.inner.open_claimed.swap(true, Ordering::SeqCst) {
            return Err(OpenError::AlreadyOpen);
        }
        // The store owns the schema lifecycle; the writer then opens its own
        // write connection over the now-current schema.
        OperationLogStore::open(db_path, set_aside).map_err(OpenError::Store)?;
        let writer = OperationLogWriter::spawn(db_path).map_err(OpenError::Writer)?;
        // Resolve any operation a crash left mid-rollback: from its unfinalized
        // inverse op's recorded outcomes, or straight back to rollbackable when no
        // inverse ever opened. Before anything can journal, so a re-issued
        // rollback resumes cleanly.
        reconcile_rolling_back_on_open(&writer);
        // AFTER the reconcile, whose writes block on their reply: the first build
        // after an upgrade can take seconds on a big journal, and `open` runs on the
        // app's main thread. Queued here, it runs on the writer thread, and what's
        // journaled meanwhile waits behind it (`store/tree_indexes.rs`).
        if let Err(e) = writer.build_tree_indexes() {
            log::warn!(target: "operation_log", "couldn't queue the item tree's folder indexes: {e}");
        }
        *self.inner.slot.write_ignore_poison() = Some(Published {
            journal: Arc::new(WriterJournal::new(writer.clone())),
            writer: Some(writer),
        });
        log::debug!(target: "operation_log", "operation log ready at {}", LogPrivate::file_path(db_path));
        Ok(())
    }

    /// Turn recording of new operations on or off. Operations already open keep
    /// the decision they were opened with.
    pub fn set_recording(&self, enabled: bool) {
        self.inner.recording.set_enabled(enabled);
    }

    /// The writer, once [`Self::open`] succeeded: for rollback, which journals its
    /// inverse op through it directly, and for the read side's DB path.
    pub fn writer(&self) -> Option<OperationLogWriter> {
        self.inner.slot.read_ignore_poison().as_ref()?.writer.clone()
    }

    /// The tree-summary backfills running on this log, one per op: what the tree view
    /// asks to answer `Building`, what `TreeBackfill::start` claims an op in, and where
    /// a host cancels one.
    pub fn tree_backfills(&self) -> &Arc<BackfillRegistry> {
        &self.inner.backfills
    }

    /// Prune by age and size now and on a slow timer, on `runtime`, with the
    /// limits `limits` reads fresh each tick. Does nothing until [`Self::open`]
    /// succeeded.
    pub fn spawn_retention(&self, runtime: &cmdr_fs::rt::Runtime, limits: RetentionLimitsSource) {
        if let Some(writer) = self.writer() {
            retention::spawn(runtime, writer, limits);
        }
    }

    /// Open an operation's journal. Decides ONCE whether the operation is
    /// journaled: with recording off, or no journal yet, it journals nothing for
    /// its whole life, even if either changes before it finalizes.
    pub fn journal_open(&self, open: OpenOperation) -> OpJournal {
        let journal = if self.inner.recording.is_enabled() {
            self.current_journal()
        } else {
            None
        };
        let op_id = open.op_id.clone();
        if let Some(journal) = &journal {
            journal.open(open);
        }
        OpJournal { op_id, journal }
    }

    fn current_journal(&self) -> Option<Arc<dyn OperationJournal>> {
        Some(Arc::clone(&self.inner.slot.read_ignore_poison().as_ref()?.journal))
    }

    /// Put `journal` in the slot (or empty it) with no writer, bypassing
    /// [`Self::open`]: a test's `CapturingJournal` or temp-DB `WriterJournal`, or
    /// the transfer bench's.
    #[cfg(any(test, feature = "testing"))]
    pub fn set_journal(&self, journal: Option<Arc<dyn OperationJournal>>) {
        *self.inner.slot.write_ignore_poison() = journal.map(|journal| Published { journal, writer: None });
    }
}

/// What a finalize wrote when nothing was journaled.
const NOTHING_WRITTEN: FinalizeOutcome = FinalizeOutcome {
    rollback_unit_rows: 0,
    search_only_rows: 0,
};

/// One operation's journal, fixed when it opened: the operation is wholly
/// journaled or wholly not. Every method no-ops for an unjournaled operation.
///
/// The operation carries it from open to finalize (the write engine keeps it in
/// the operation's scope); a clone is the same operation's journal.
#[derive(Clone)]
pub struct OpJournal {
    op_id: OperationId,
    journal: Option<Arc<dyn OperationJournal>>,
}

impl OpJournal {
    /// The operation this journal records.
    pub fn op_id(&self) -> &OperationId {
        &self.op_id
    }

    /// Record item rows (buffered and batched by the journal).
    pub fn record_items(&self, items: Vec<JournalItem>) {
        if let Some(journal) = &self.journal {
            journal.record_items(&self.op_id, items);
        }
    }

    /// Downgrade the operation's search coverage (worst wins).
    pub fn note_coverage(&self, coverage: SearchCoverage, reason: Option<SearchCoverageReason>) {
        if let Some(journal) = &self.journal {
            journal.note_search_coverage(&self.op_id, coverage, reason);
        }
    }

    /// Rule the operation out of rollback for a reason only the driver can see
    /// (a directory merge, a conflict resolved after the rows were written).
    /// Beats the per-kind eligibility at finalize.
    pub fn note_not_rollbackable(&self, reason: NotRollbackableReason) {
        if let Some(journal) = &self.journal {
            journal.note_not_rollbackable(&self.op_id, reason);
        }
    }

    /// Finalize the operation, returning the durable per-`row_role` counts. Zero
    /// counts for an unjournaled operation.
    pub fn finalize(self, inputs: FinalizeInputs) -> FinalizeOutcome {
        match &self.journal {
            Some(journal) => journal.finalize(&self.op_id, inputs),
            None => NOTHING_WRITTEN,
        }
    }
}

#[cfg(test)]
mod tests;
