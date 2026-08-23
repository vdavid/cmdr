//! The sink decorator: how execution reports back into the proposal spine.
//!
//! The write engine knows nothing about the agent, and a check enforces that
//! (`write-ops-isolation`). So the reporting runs the other way: the bridge wraps the sink it
//! injects, watches the ordinary per-source stream go past, and writes what it sees into
//! `proposal_ops.status` and, at the end, `proposals.status`.
//!
//! Everything else is delegated untouched. The decorator adds a write; it never changes,
//! drops, or reorders an event, because every surface watching the same operation must see
//! exactly what it would have seen without an approval behind it.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use rusqlite::Connection;

use crate::agent::chat::runtime::now_secs;
use crate::agent::memory::MemoryStore;
use crate::agent::store::proposals::{mark_group_completed, record_op_outcome};
use crate::agent::types::OpStatus;
use crate::file_system::write_operations::{
    ConflictInfo, DryRunResult, OperationEventSink, ScanProgressEvent, SourceItemOutcome, WriteCancelledEvent,
    WriteCompleteEvent, WriteConflictEvent, WriteConflictResolvedEvent, WriteErrorEvent, WriteProgressEvent,
    WriteSettledEvent, WriteSourceItemDoneEvent,
};
use crate::ignore_poison::IgnorePoison;

/// Wraps the sink an approved group executes through, recording each source outcome and the
/// group own end.
pub(super) struct ProposalReportingSink {
    inner: Arc<dyn OperationEventSink>,
    group_id: i64,
    /// Source path to op id, built once from the group live ops. The engine reports paths;
    /// the store keys on ids, and the mapping cannot be re-derived later because a skipped
    /// source may no longer exist to look up.
    op_ids: HashMap<PathBuf, i64>,
    /// One connection for the operation lifetime rather than one per event: a group may
    /// carry 60 000 sources, and opening a connection per source would dominate the run.
    conn: Mutex<Connection>,
    /// Where the agent's lesson goes, resolved by the command layer and MOVED in for the same
    /// reason the connection is: the operation outlives the call that started it.
    ///
    /// ⚠️ **This seam is why the store is pure.** The write engine's thread has no `AppHandle`
    /// and may never name one (`write-ops-isolation`), so a memory store that needed one would
    /// have nowhere to be built here — and this is the only place an approval's REAL outcome
    /// is known.
    memory: Option<MemoryStore>,
}

impl ProposalReportingSink {
    pub(super) fn new(
        inner: Arc<dyn OperationEventSink>,
        group_id: i64,
        op_ids: HashMap<PathBuf, i64>,
        conn: Connection,
        memory: Option<MemoryStore>,
    ) -> Self {
        Self {
            inner,
            group_id,
            op_ids,
            conn: Mutex::new(conn),
            memory,
        }
    }

    /// Write one source verdict, keyed by the path the engine reported.
    ///
    /// A path this group never proposed is ignored: the engine reports every top-level source
    /// it was handed, and nothing stops a caller running a group beside sources of its own.
    fn record(&self, source_path: &str, outcome: OpStatus) {
        let Some(op_id) = self.op_ids.get(&PathBuf::from(source_path)) else {
            return;
        };
        let conn = self.conn.lock_ignore_poison();
        if let Err(e) = record_op_outcome(&conn, *op_id, outcome) {
            // A lost status line is a wrong review surface later, never a wrong file now, so
            // it must not interrupt an operation that is mid-write.
            log::warn!(
                target: "agent::suggested_ops",
                "couldn't record op {op_id} as {outcome:?} for group {}: {e}",
                self.group_id
            );
        }
    }
}

/// The store vocabulary for what the engine reported.
fn op_status_for(outcome: SourceItemOutcome) -> OpStatus {
    match outcome {
        SourceItemOutcome::Done => OpStatus::Done,
        SourceItemOutcome::Skipped => OpStatus::Skipped,
        SourceItemOutcome::Failed => OpStatus::Failed,
    }
}

impl OperationEventSink for ProposalReportingSink {
    /// The one event this decorator exists for.
    ///
    /// The LAST event a source gets is its verdict: a cross-filesystem move speaks twice for
    /// one source, and staging succeeding says nothing about where the item ended up. So this
    /// overwrites rather than accumulating, which is what `record_op_outcome` does.
    fn emit_source_item_done(&self, event: WriteSourceItemDoneEvent) {
        self.record(&event.source_path, op_status_for(event.outcome));
        self.inner.emit_source_item_done(event);
    }

    /// The group is no longer in flight, whichever way the operation ended.
    ///
    /// Settle fires exactly once per operation, after the task has fully torn down, on every
    /// outcome including a cancel or a panic, which is precisely the question
    /// `ProposalStatus::Completed` answers. Marking only on `write-complete` would leave a
    /// cancelled group `approved`, and the next launch would call it `interrupted`: a claim
    /// that the app died, about an operation the user deliberately stopped.
    fn emit_settled(&self, event: WriteSettledEvent) {
        {
            let conn = self.conn.lock_ignore_poison();
            match mark_group_completed(&conn, self.group_id) {
                Ok(outcome) => log::debug!(
                    target: "agent::suggested_ops",
                    "group {} settled after operation {}: {outcome:?}",
                    self.group_id, event.operation_id
                ),
                Err(e) => log::warn!(
                    target: "agent::suggested_ops",
                    "couldn't mark group {} completed: {e}", self.group_id
                ),
            }
            // ⚠️ HERE, and not at `approve`. An outcome recorded when the user clicked would
            // say "approved" for a group that then skipped every file, and the agent would
            // learn the user wanted something they never got. The per-op statuses above are
            // already written by the time settle fires, so this reads the truth.
            crate::agent::outcomes::record_completion(&conn, self.memory.as_ref(), self.group_id, now_secs());
        }
        self.inner.emit_settled(event);
    }

    // Everything below is pure delegation: the decorator adds a write, it never
    // changes what any other surface sees.

    fn emit_progress(&self, event: WriteProgressEvent) {
        self.inner.emit_progress(event);
    }
    fn emit_complete(&self, event: WriteCompleteEvent) {
        self.inner.emit_complete(event);
    }
    fn emit_cancelled(&self, event: WriteCancelledEvent) {
        self.inner.emit_cancelled(event);
    }
    fn emit_error(&self, event: WriteErrorEvent) {
        self.inner.emit_error(event);
    }
    fn emit_conflict(&self, event: WriteConflictEvent) {
        self.inner.emit_conflict(event);
    }
    fn emit_conflict_resolved(&self, event: WriteConflictResolvedEvent) {
        self.inner.emit_conflict_resolved(event);
    }
    fn emit_scan_progress(&self, event: ScanProgressEvent) {
        self.inner.emit_scan_progress(event);
    }
    fn emit_scan_conflict(&self, conflict: ConflictInfo) {
        self.inner.emit_scan_conflict(conflict);
    }
    fn emit_dry_run_complete(&self, result: DryRunResult) {
        self.inner.emit_dry_run_complete(result);
    }
    fn note_source_landed_clean(&self, source: &Path) {
        self.inner.note_source_landed_clean(source);
    }
}
