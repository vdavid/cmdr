//! A rollback's selection, resolved for the engine: the gate's refusals, and the
//! per-row filter the reversal runs on each raw journal row before it resolves a
//! single path (`DETAILS.md` § "Streaming + ordering").
//!
//! The decision itself is THE resolver's (`selection.rs::Selection::includes`); this
//! only adapts it to the engine's raw rows and refuses what a run can't honor.
//!
//! The filter owns no connection: the reversal is an async task that must be `Send`,
//! and a `&Connection` held across its awaits isn't. It borrows the run's connection
//! for one page at a time ([`RollbackFilter::on`]). Its decision cache AND its summary
//! row cache outlive the page (each page's reader borrows the run's [`DirCache`] and
//! hands it back), so each distinct folder is read once per run, up to
//! [`DIR_CACHE_CAP`] folders.

use rusqlite::Connection;

use super::read::{DirCache, OpReader};
use super::selection::{Picked, RollbackSelection, Selection, SelectionBase};
use super::summary::{OpTreeContext, RowFacts, read_op_tree_context};
use crate::rollback::SelectionRefusal;
use crate::store::{OperationItemRow, OperationLogStoreError};

/// What a wire selection comes to for one op.
pub(crate) enum Narrowed {
    /// It takes every row an unfiltered run would: a run with no selection at all,
    /// so its reports and progress totals are exactly an unfiltered run's.
    Everything,
    /// It takes some rows: filter each streamed row through this.
    Some(Box<RollbackFilter>),
    /// The run can't honor it.
    Refused(SelectionRefusal),
}

/// Resolve `wire` against `op_id` on `conn`: normalized by the crate (❌ never
/// trusted from the wire) and counted off the tree summary.
///
/// Refuses a selection on an op whose summary isn't built (the decision reads it),
/// one past the toggle cap (normalizing would drop the newest toggles, and a dropped
/// `off` toggle would reverse a row the person unchecked), and one that takes no
/// selectable unit (`N = 0`).
pub(crate) fn narrow(
    conn: &Connection,
    op_id: &str,
    wire: &RollbackSelection,
) -> Result<Narrowed, OperationLogStoreError> {
    let Some(context) = read_op_tree_context(conn, op_id)? else {
        return Ok(Narrowed::Refused(SelectionRefusal::SummaryNotBuilt));
    };
    if !context.built {
        return Ok(Narrowed::Refused(SelectionRefusal::SummaryNotBuilt));
    }
    let mut reader = OpReader::new(conn, op_id, context);
    let selection = Selection::resolve(&mut reader, wire)?;
    if selection.capped() {
        return Ok(Narrowed::Refused(SelectionRefusal::TooManyToggles));
    }
    let normalized = selection.normalized();
    if normalized.base == SelectionBase::Everything && normalized.toggles.is_empty() {
        return Ok(Narrowed::Everything);
    }
    if selection.chosen().units == 0 {
        return Ok(Narrowed::Refused(SelectionRefusal::NothingSelected));
    }
    Ok(Narrowed::Some(Box::new(RollbackFilter {
        op_id: op_id.to_string(),
        context,
        selection,
        dirs: DirCache::new(),
        dir_reads: 0,
    })))
}

/// The most summary rows a run keeps between pages. Past it the cache starts over at
/// the next page: a copy of a deep tree can hold a million folders, and the stream's
/// `seq DESC` order keeps a folder's rows close together, so what's dropped is
/// mostly behind the stream. About 200 bytes a row, so this bounds it near 13 MB.
const DIR_CACHE_CAP: usize = 65_536;

/// The engine's per-row filter for one run: no DB read per row, one summary read per
/// distinct folder (cached for the run).
pub(crate) struct RollbackFilter {
    op_id: String,
    context: OpTreeContext,
    selection: Selection,
    /// The run's summary rows, lent to each page's reader.
    dirs: DirCache,
    /// Summary point reads the run made so far.
    dir_reads: u64,
}

impl RollbackFilter {
    /// The filter over `conn`, for one page's rows.
    pub(crate) fn on<'f>(&'f mut self, conn: &'f Connection) -> PageFilter<'f> {
        if self.dirs.len() > DIR_CACHE_CAP {
            self.dirs.clear();
        }
        let dirs = std::mem::take(&mut self.dirs);
        PageFilter {
            reader: OpReader::with_dirs(conn, &self.op_id, self.context, dirs),
            selection: &mut self.selection,
            home: &mut self.dirs,
            dir_reads: &mut self.dir_reads,
        }
    }

    /// Summary point reads the run's pages made, for the read-count gate.
    #[cfg(test)]
    pub(crate) fn dir_reads(&self) -> u64 {
        self.dir_reads
    }

    /// The selected units and their file bytes: the run's progress totals, the same
    /// rows [`PageFilter::admits`] takes that the engine credits (`credits_progress`).
    pub(crate) fn progress_totals(&self) -> (u64, u64) {
        let Picked { units, bytes, .. } = self.selection.chosen();
        (units.max(0) as u64, bytes.max(0) as u64)
    }

    /// The selection as the crate normalized it, which is what the run should carry.
    pub(crate) fn normalized(&self) -> &RollbackSelection {
        self.selection.normalized()
    }

    /// One row the selection takes, to name where the reversal will act.
    pub(crate) fn sample_seq(&self, conn: &Connection) -> Result<Option<i64>, OperationLogStoreError> {
        let mut reader = OpReader::new(conn, &self.op_id, self.context);
        self.selection.sample_seq(&mut reader)
    }
}

/// [`RollbackFilter`] over one page's connection borrow.
pub(crate) struct PageFilter<'f> {
    reader: OpReader<'f>,
    selection: &'f mut Selection,
    /// Where the page's reader hands its summary rows back.
    home: &'f mut DirCache,
    dir_reads: &'f mut u64,
}

impl Drop for PageFilter<'_> {
    fn drop(&mut self) {
        *self.home = self.reader.take_dirs();
        *self.dir_reads += self.reader.dir_reads();
    }
}

impl PageFilter<'_> {
    /// Whether the run acts on `row`. A row whose folder has no summary row means the
    /// summary disagrees with the rows, and an error here stops the run like a page
    /// it couldn't read: ❌ never guess a decision.
    pub(crate) fn admits(&mut self, row: &OperationItemRow) -> Result<bool, OperationLogStoreError> {
        let facts = RowFacts::from(row);
        let dir = facts.placement(self.reader.context.axis);
        if self.reader.dir(dir)?.is_none() {
            return Err(OperationLogStoreError::Decode {
                column: "operation_tree_dirs",
                value: format!("no summary row for dir {dir}, which item {} sits in", row.seq),
            });
        }
        self.selection.includes(&mut self.reader, &facts)
    }
}
