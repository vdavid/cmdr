//! The writer's half of a tree-summary backfill (`crate::tree::backfill`): the two
//! writes a backfill can't make on its read connection. The aggregation itself never
//! runs here: a 2M-row read on this thread would block `record_items` and, through
//! backpressure, a running copy.
//!
//! - `fill_self_dirs`: interns the own folder of DIR rows journaled before the summary
//!   existed, the same way an insert does, so a backfill can place them.
//! - `write_tree_summary`: swaps in a backfill's aggregate and stamps the op `built`,
//!   in one transaction, only if nothing it read has changed since.

use std::sync::mpsc;

use rusqlite::{Connection, OptionalExtension};

use super::{OperationLogWriter, WriteMessage, writer_gone};
use crate::store::{OperationLogStoreError, intern_child_dir};
use crate::tree::summary::{TreeDirRow, read_op_tree_context};
use crate::types::{EntryType, TreeSummaryState};

impl OperationLogWriter {
    /// Intern the own folder of `op_id`'s DIR rows at `seqs` that have none yet (rows
    /// journaled before the tree summary existed), for a backfill. Blocks for the
    /// reply (a barrier: a read after it sees the filled rows).
    pub(crate) fn fill_self_dirs(&self, op_id: &str, seqs: Vec<i64>) -> Result<(), OperationLogStoreError> {
        let (tx, rx) = mpsc::channel();
        self.send(WriteMessage::FillSelfDirs {
            op_id: op_id.to_string(),
            seqs,
            reply: tx,
        })?;
        rx.recv().map_err(writer_gone)
    }

    /// Swap in a backfill's summary for `op_id`, if the op still exists, isn't built,
    /// and still has `generation`. Blocks for the reply; `None` when the write itself
    /// failed (nothing changed).
    pub(crate) fn write_tree_summary(
        &self,
        op_id: &str,
        generation: i64,
        rows: Vec<TreeDirRow>,
    ) -> Result<Option<TreeSummaryWrite>, OperationLogStoreError> {
        let (tx, rx) = mpsc::channel();
        self.send(WriteMessage::WriteTreeSummary {
            op_id: op_id.to_string(),
            generation,
            rows,
            reply: tx,
        })?;
        rx.recv().map_err(writer_gone)
    }
}

/// `FillSelfDirs` on the writer thread. A failure leaves the rows unfilled, and the
/// backfill ends unbuilt (see [`fill_self_dirs`]).
pub(super) fn apply_fill_self_dirs(conn: &mut Connection, op_id: &str, seqs: &[i64]) {
    if let Err(e) = fill_self_dirs(conn, op_id, seqs) {
        log::warn!(target: "operation_log", "fill_self_dirs({op_id}) failed: {e}");
    }
}

/// `WriteTreeSummary` on the writer thread: what it did, `None` if it failed.
pub(super) fn apply_write_tree_summary(
    conn: &mut Connection,
    op_id: &str,
    generation: i64,
    rows: &[TreeDirRow],
) -> Option<TreeSummaryWrite> {
    write_tree_summary(conn, op_id, generation, rows)
        .inspect_err(|e| log::warn!(target: "operation_log", "write_tree_summary({op_id}) failed: {e}"))
        .ok()
}

/// What the writer did with a backfill's result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TreeSummaryWrite {
    /// Swapped in; the op reads built.
    Built,
    /// An outcome batch landed after the backfill started reading: nothing written,
    /// read the op again.
    Stale,
    /// The op is gone (retention pruned it): nothing written, so no summary row keeps
    /// its dirs alive past `gc_orphan_dirs`.
    Gone,
    /// The op is already built: nothing written.
    AlreadyBuilt,
}

/// Fill `self_dir_id` for `op_id`'s DIR rows at `seqs` that have none: each row's
/// folder on the op's axis, interned as `insert_one_item` does. Only for an op whose
/// summary isn't built: a built summary placed such a row in its parent, and moving
/// it now would make the writer's later deltas disagree with it. A row whose folder
/// doesn't intern keeps none, and the backfill then leaves the op unbuilt
/// (`tree::backfill`), as the writer's insert path does.
fn fill_self_dirs(conn: &mut Connection, op_id: &str, seqs: &[i64]) -> Result<(), OperationLogStoreError> {
    let tx = conn.unchecked_transaction()?;
    let Some(context) = read_op_tree_context(&tx, op_id)? else {
        return Ok(());
    };
    if context.built {
        return Ok(());
    }
    {
        let mut rows = tx.prepare_cached(
            "SELECT item_id, source_dir_id, source_name, dest_dir_id, dest_name FROM operation_items \
             WHERE op_id = ?1 AND seq = ?2 AND entry_type = ?3 AND self_dir_id IS NULL",
        )?;
        let mut fill = tx.prepare_cached("UPDATE operation_items SET self_dir_id = ?2 WHERE item_id = ?1")?;
        for &seq in seqs {
            let found = rows
                .query_row(rusqlite::params![op_id, seq, EntryType::Dir.as_token()], |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, Option<i64>>(3)?,
                        row.get::<_, Option<String>>(4)?,
                    ))
                })
                .optional()?;
            let Some((item_id, source_dir, source_name, dest_dir, dest_name)) = found else {
                continue;
            };
            let dest = dest_dir.zip(dest_name);
            let (parent, name) = context.axis.side(
                (source_dir, &source_name),
                dest.as_ref().map(|(dir, name)| (*dir, name.as_str())),
            );
            match intern_child_dir(&tx, parent, name) {
                Ok(own) => {
                    fill.execute(rusqlite::params![item_id, own])?;
                }
                Err(e) => {
                    log::warn!(target: "operation_log", "tree backfill({op_id}): seq {seq} has no own folder: {e}");
                }
            }
        }
    }
    tx.commit()?;
    Ok(())
}

/// Replace `op_id`'s summary with `rows` and stamp it built, if the op still exists,
/// isn't built, and its `outcome_generation` is still `generation` (the value the
/// backfill read before its first chunk). All in one transaction, so a crash leaves
/// either the old unbuilt op or the new built one.
fn write_tree_summary(
    conn: &mut Connection,
    op_id: &str,
    generation: i64,
    rows: &[TreeDirRow],
) -> Result<TreeSummaryWrite, OperationLogStoreError> {
    let tx = conn.unchecked_transaction()?;
    let header = tx
        .prepare_cached("SELECT tree_summary, outcome_generation FROM operations WHERE op_id = ?1")?
        .query_row(rusqlite::params![op_id], |row| {
            Ok((row.get::<_, Option<String>>(0)?, row.get::<_, i64>(1)?))
        })
        .optional()?;
    let answer = match header {
        None => TreeSummaryWrite::Gone,
        Some((Some(_), _)) => TreeSummaryWrite::AlreadyBuilt,
        Some((None, now)) if now != generation => TreeSummaryWrite::Stale,
        Some((None, _)) => {
            write_rows(&tx, op_id, rows)?;
            tx.prepare_cached("UPDATE operations SET tree_summary = ?2 WHERE op_id = ?1")?
                .execute(rusqlite::params![op_id, TreeSummaryState::Built.as_token()])?;
            TreeSummaryWrite::Built
        }
    };
    tx.commit()?;
    Ok(answer)
}

fn write_rows(conn: &Connection, op_id: &str, rows: &[TreeDirRow]) -> Result<(), OperationLogStoreError> {
    // Whatever a failed or half-kept summary left behind goes first: the backfill
    // REPLACES the op's rows.
    conn.prepare_cached("DELETE FROM operation_tree_dirs WHERE op_id = ?1")?
        .execute(rusqlite::params![op_id])?;
    let mut insert = conn.prepare_cached(
        "INSERT INTO operation_tree_dirs (op_id, dir_id, parent_dir_id, name_folded, self_seq, child_dirs, \
            direct_files, files, bytes, units_selectable, unit_files_selectable, unit_bytes_selectable, \
            units_rolled_back, units_held_back, units_unfinished, housekeeping_waiting) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
    )?;
    for row in rows {
        let t = &row.totals;
        insert.execute(rusqlite::params![
            op_id,
            row.dir_id,
            row.parent_dir_id,
            row.name_folded,
            row.self_seq,
            row.child_dirs,
            row.direct_files,
            t.files,
            t.bytes,
            t.units_selectable,
            t.unit_files_selectable,
            t.unit_bytes_selectable,
            t.units_rolled_back,
            t.units_held_back,
            t.units_unfinished,
            t.housekeeping_waiting,
        ])?;
    }
    Ok(())
}
