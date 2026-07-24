//! The operation log: a durable, cross-volume journal of every file mutation,
//! the foundation for search, rollback, and a future undo.
//!
//! This module is the DURABLE STORE: the schema, the forward-migration
//! ladder, the single writer thread, and dir interning. The capture layer, the
//! rollback engine, search/retention, the MCP tools, and the UI build on it.
//!
//! Unlike every other on-disk store in the app (the drive index and
//! `importance.db` are disposable per-volume caches that delete-and-recreate on
//! a schema bump), this DB lives for years, so it introduces the codebase's
//! first forward-migration ladder (`store`'s migration ladder) and retention
//! discipline. Design rationale, the migration-ladder template, and the capture/
//! rollback contracts: `CLAUDE.md` + `DETAILS.md`.

pub mod capture;
pub mod query;
pub mod retention;
pub mod rollback;
pub mod store;
pub mod types;
pub mod writer;

use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

use tauri::{AppHandle, Manager};

use crate::ignore_poison::RwLockIgnorePoison;
use capture::{FinalizeInputs, OperationJournal, WriterJournal};
use types::{SearchCoverage, SearchCoverageReason};
use writer::{FinalizeOutcome, JournalItem, OpenOperation};

/// The process-global journal handle. The write pipeline reaches it BY `op_id`
/// through the free functions below, mirroring the existing per-op-keyed
/// `update_operation_status(op_id, …)` status cache written at the very same
/// record points and the `manager()` operation-manager singleton — rather than
/// threading an `OperationObservers` context through the whole transfer/delete
/// signature chain. This is a recorded deviation from D4's threaded-observers
/// mechanism (its hard constraint — never extend `OperationEventSink` — is kept),
/// chosen for consistency with those two established patterns and to keep the
/// safety-critical pipeline signatures untouched. See `capture.rs` +
/// `DETAILS.md` § Capture. `None` until `start` (or a test) installs one, so a
/// build whose journal DB failed to open simply doesn't journal.
static JOURNAL: RwLock<Option<Arc<dyn OperationJournal>>> = RwLock::new(None);

/// Install the process-global journal. Called once at [`start`]; tests install
/// their own (a `CapturingJournal` or a temp-DB `WriterJournal`).
pub(crate) fn set_journal(journal: Arc<dyn OperationJournal>) {
    *JOURNAL.write_ignore_poison() = Some(journal);
}

/// Clear the global journal (test teardown; nextest isolates per process, so this
/// is belt-and-suspenders). Tests never call this directly — [`TestJournalGuard`]
/// owns install AND teardown.
#[cfg(test)]
fn clear_journal() {
    *JOURNAL.write_ignore_poison() = None;
}

/// Test isolation for the process-global [`JOURNAL`] slot.
///
/// nextest forks a process per test, so the slot is hermetic there — but plain
/// `cargo test` runs the whole crate's tests as threads in ONE process, where
/// concurrent installs and teardowns race: one test's teardown clears another's
/// journal mid-operation, and rows land in a foreign test's DB. The slot is a
/// single value, not a keyed map, so the per-test-key pattern
/// (`write_operations::test_support::TestOperationGuard` over
/// `WRITE_OPERATION_STATE`) can't isolate it; tests that install a journal
/// SERIALIZE on this guard's lock for their whole duration instead.
///
/// `Drop` clears the slot (it runs on unwind too), so a panicking test can't leak
/// its journal into the next. Hold exactly ONE guard per test, on the stack, for
/// the test's whole body; the lock is not reentrant, so constructing a second
/// guard in the same test deadlocks under plain `cargo test`. Multi-arm tests
/// that swap journals mid-test start from [`TestJournalGuard::hold_empty`] and
/// use [`TestJournalGuard::set`] / [`TestJournalGuard::clear`].
#[cfg(test)]
pub(crate) struct TestJournalGuard {
    _lock: std::sync::MutexGuard<'static, ()>,
}

#[cfg(test)]
impl TestJournalGuard {
    /// Lock the slot and install `journal` for this test's duration.
    pub(crate) fn install(journal: Arc<dyn OperationJournal>) -> Self {
        let guard = Self::hold_empty();
        set_journal(journal);
        guard
    }

    /// Lock the slot and leave it EMPTY: a no-journal baseline no concurrent test
    /// can pollute. Install later with [`Self::set`].
    pub(crate) fn hold_empty() -> Self {
        use crate::ignore_poison::IgnorePoison;
        static JOURNAL_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        // A panicking journal test poisons the lock, but its `Drop` already cleared
        // the slot, so the next holder starts from a clean baseline regardless.
        let lock = JOURNAL_TEST_LOCK.lock_ignore_poison();
        clear_journal();
        Self { _lock: lock }
    }

    /// Swap the installed journal while keeping the lock (multi-arm tests).
    pub(crate) fn set(&self, journal: Arc<dyn OperationJournal>) {
        set_journal(journal);
    }

    /// Drop the installed journal early while keeping the lock (a subsequent arm
    /// runs journal-less, or the test wants the writer joined before reading).
    pub(crate) fn clear(&self) {
        clear_journal();
    }
}

#[cfg(test)]
impl Drop for TestJournalGuard {
    fn drop(&mut self) {
        clear_journal();
    }
}

fn current_journal() -> Option<Arc<dyn OperationJournal>> {
    JOURNAL.read_ignore_poison().clone()
}

/// Seconds since the Unix epoch — the journal's opaque clock (the store owns no
/// clock, so callers supply the time). Shared by the write pipeline's capture glue
/// and the rollback engine.
pub(crate) fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Open an operation row. No-op when no journal is installed.
pub fn journal_open(open: OpenOperation) {
    if let Some(j) = current_journal() {
        j.open(open);
    }
}

/// Record item rows for an open operation (buffered + batched by the journal).
/// No-op when no journal is installed.
pub fn journal_record_items(op_id: &str, items: Vec<JournalItem>) {
    if let Some(j) = current_journal() {
        j.record_items(op_id, items);
    }
}

/// Downgrade an op's search coverage (worst-wins). No-op when no journal.
pub fn journal_note_coverage(op_id: &str, coverage: SearchCoverage, reason: Option<SearchCoverageReason>) {
    if let Some(j) = current_journal() {
        j.note_search_coverage(op_id, coverage, reason);
    }
}

/// Finalize an operation, returning the durable per-`row_role` counts (mostly for
/// tests). No-op — returning zero counts — when no journal is installed.
pub fn journal_finalize(op_id: &str, inputs: FinalizeInputs) -> FinalizeOutcome {
    match current_journal() {
        Some(j) => j.finalize(op_id, inputs),
        None => FinalizeOutcome {
            rollback_unit_rows: 0,
            search_only_rows: 0,
        },
    }
}

/// Open `operation-log.db` and spawn its single writer thread, placing the
/// [`OperationLogWriter`](writer::OperationLogWriter) handle in managed state so
/// the capture layer can journal through it. A single cross-volume writer,
/// no per-volume registry (D1). Failure is non-fatal: the app runs without the
/// journal rather than refusing to start.
pub fn start(app: &AppHandle) {
    let data_dir = match crate::config::resolved_app_data_dir(app) {
        Ok(dir) => dir,
        Err(e) => {
            log::warn!(target: "operation_log", "operation log not started: {e}");
            return;
        }
    };
    let db_path = store::operation_log_db_path(&data_dir);

    // Open the store first — it owns the schema lifecycle (migrate forward, or
    // recreate a genuinely unparseable file, or refuse a downgrade). The writer
    // then opens its own write connection over the now-current schema.
    if let Err(e) = store::OperationLogStore::open(&db_path) {
        log::warn!(target: "operation_log", "operation log store not opened: {e}");
        return;
    }
    match writer::OperationLogWriter::spawn(&db_path) {
        Ok(writer) => {
            // Resolve any operation a crash left mid-rollback (Finding 7): from its
            // unfinalized inverse op's recorded outcomes, or straight back to
            // rollbackable when no inverse ever opened. Runs before anything can
            // journal, so a re-issued rollback resumes cleanly.
            rollback::reconcile_rolling_back_on_open(&writer);
            // The global journal holds a clone (the capture record points reach it
            // by op_id); managed state keeps the writer for retention + shutdown.
            set_journal(Arc::new(WriterJournal::new(writer.clone())));
            // Enforce retention: prune on startup + a periodic timer, with the
            // settings-driven age/size limits. Runs before the app is under
            // load; the size loop is a no-op while the DB is under budget.
            retention::spawn(app, writer.clone());
            app.manage(writer);
            log::debug!(target: "operation_log", "operation log ready at {}", db_path.display());
        }
        Err(e) => log::warn!(target: "operation_log", "operation log writer not spawned: {e}"),
    }
}
