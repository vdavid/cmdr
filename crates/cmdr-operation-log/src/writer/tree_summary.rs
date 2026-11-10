//! The writer's half of the tree summary: every insert batch and every outcome batch
//! carries its summary delta in the SAME transaction, so the rows and the summary
//! can't disagree after a crash (`DETAILS.md` § "Tree summary").
//!
//! A summary write that fails never costs an item row or an outcome: the delta rolls
//! back to its savepoint, the op's `tree_summary` goes back to NULL in the same
//! transaction, and the next open of its tree rebuilds it exactly from the rows. From
//! then on the writer maintains nothing for that op.

use rusqlite::Connection;

use crate::store::OperationLogStoreError;
use crate::tree::TreeAxis;
use crate::tree::summary::{
    FACT_COLUMNS, OpTreeContext, RowFacts, TreeDelta, apply_delta, map_facts, mark_unbuilt, read_op_tree_context,
};
use crate::types::EntryType;

/// One writer batch's summary change, gathered while its rows are written.
pub(super) struct SummaryBatch {
    /// `None` for an op with no header (its rows won't insert either).
    context: Option<OpTreeContext>,
    delta: TreeDelta,
    /// Something the summary needed couldn't be had (a read, a DIR row's own folder),
    /// so the op's summary can't stay exact: mark it unbuilt.
    broken: bool,
}

impl SummaryBatch {
    /// Start a batch for `op_id`, reading its kind, axis, and whether it's built.
    pub(super) fn begin(conn: &Connection, op_id: &str) -> Self {
        let (context, broken) = match read_op_tree_context(conn, op_id) {
            Ok(context) => (context, false),
            Err(e) => {
                log::warn!(target: "operation_log", "tree summary({op_id}): reading the op failed: {e}");
                (None, true)
            }
        };
        Self {
            context,
            delta: TreeDelta::default(),
            broken,
        }
    }

    /// The axis a DIR row's own folder is interned on. An op whose header couldn't be
    /// read gets the source side, which every row has; its summary is marked unbuilt,
    /// and the backfill that rebuilds it re-derives nothing from this choice.
    pub(super) fn axis(&self) -> TreeAxis {
        self.context.map_or(TreeAxis::Source, |c| c.axis)
    }

    /// Whether this batch maintains a summary at all (the op's is built).
    pub(super) fn maintains(&self) -> bool {
        self.context.is_some_and(|c| c.built) && !self.broken
    }

    /// Count a row that just inserted.
    pub(super) fn add_inserted(&mut self, facts: &RowFacts) {
        if !self.maintains() {
            return;
        }
        if facts.entry_type == EntryType::Dir && facts.self_dir_id.is_none() {
            // Its own folder didn't intern: the row stands, the summary can't place it.
            self.broken = true;
            return;
        }
        if let Some(context) = self.context {
            self.delta.add_row(context.kind, context.axis, facts, 1);
        }
    }

    /// Move a row from its old outcome to its new one.
    pub(super) fn add_change(&mut self, before: &RowFacts, after: &RowFacts) {
        if let (true, Some(context)) = (self.maintains(), self.context) {
            self.delta.add_row(context.kind, context.axis, before, -1);
            self.delta.add_row(context.kind, context.axis, after, 1);
        }
    }

    /// A read the summary needed failed.
    pub(super) fn fail(&mut self, op_id: &str, e: &OperationLogStoreError) {
        log::warn!(target: "operation_log", "tree summary({op_id}): reading a row failed: {e}");
        self.broken = true;
    }

    /// Write the batch's delta, or mark the op unbuilt when that can't be exact. Call
    /// inside the batch's transaction, after its rows.
    pub(super) fn finish(self, conn: &Connection, op_id: &str) {
        let was_built = self.context.is_none_or(|c| c.built);
        if self.broken {
            if was_built {
                give_up(conn, op_id);
            }
            return;
        }
        if !self.maintains() || self.delta.is_empty() {
            return;
        }
        if let Err(e) = apply_in_savepoint(conn, op_id, self.delta) {
            log::warn!(target: "operation_log", "tree summary({op_id}): writing the summary failed, marking it unbuilt: {e}");
            give_up(conn, op_id);
        }
    }
}

/// The rows a `(op_id, seq)` outcome update touches, before it does.
pub(super) fn read_row_facts(
    conn: &Connection,
    op_id: &str,
    seq: i64,
) -> Result<Vec<RowFacts>, OperationLogStoreError> {
    let sql = format!("SELECT {FACT_COLUMNS} FROM operation_items WHERE op_id = ?1 AND seq = ?2");
    let mut stmt = conn.prepare_cached(&sql)?;
    let mut rows = stmt.query(rusqlite::params![op_id, seq])?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        out.push(map_facts(row)?);
    }
    Ok(out)
}

fn apply_in_savepoint(conn: &Connection, op_id: &str, delta: TreeDelta) -> Result<(), OperationLogStoreError> {
    conn.execute_batch("SAVEPOINT tree_summary")?;
    match apply_delta(conn, op_id, delta) {
        Ok(()) => {
            conn.execute_batch("RELEASE tree_summary")?;
            Ok(())
        }
        Err(e) => {
            if let Err(undo) = conn.execute_batch("ROLLBACK TO tree_summary; RELEASE tree_summary") {
                log::warn!(target: "operation_log", "tree summary({op_id}): rolling back its savepoint failed: {undo}");
            }
            Err(e)
        }
    }
}

fn give_up(conn: &Connection, op_id: &str) {
    if let Err(e) = mark_unbuilt(conn, op_id) {
        log::warn!(target: "operation_log", "tree summary({op_id}): marking it unbuilt failed: {e}");
    }
}
