//! The Operation log's item tree, one window at a time: the request carries the
//! frontend's hand toggles (expanded folders, the selection) and where it's
//! scrolled, the answer every row it renders with everything the row says. Stateless,
//! and O(expansion + window + toggles) for any op size (`DETAILS.md` § "The tree
//! view"). The walk itself is `layout.rs`; the wire types both build on are `wire.rs`.

use super::layout::Layout;
use super::read::OpReader;
use super::selection::{RollbackSelection, Selection};
use super::status::{BackfillRegistry, TreeSummaryStatus, read_tree_summary_status};
use super::summary::read_op_tree_context;
use super::wire::clamp_u32;
pub use super::wire::{
    OperationTreeRequest, OperationTreeRow, OperationTreeView, TreeCheckbox, TreeRowKey, TreeRowRole, TreeSummaryView,
    TreeTally, UnexpandableReason,
};
use crate::store::{OperationLogStoreError, read_operation};
use crate::types::RollbackState;

/// The most rows one answer carries.
pub(crate) const MAX_LIMIT: u32 = 200;

/// The most expanded folders a request carries; the frontend drops its oldest past it.
pub(crate) const MAX_EXPANDED: usize = 2_000;

/// One window of `op_id`'s item tree, `None` for an unknown op. Reads only what the
/// window, the expansion, and the toggles touch.
pub(crate) fn operation_tree(
    conn: &rusqlite::Connection,
    op_id: &str,
    request: &OperationTreeRequest,
    registry: &BackfillRegistry,
) -> Result<Option<OperationTreeView>, OperationLogStoreError> {
    let Some(op) = read_operation(conn, op_id)? else {
        return Ok(None);
    };
    let Some(context) = read_op_tree_context(conn, op_id)? else {
        return Ok(None);
    };
    let Some(status) = read_tree_summary_status(conn, op_id, registry)? else {
        return Ok(None);
    };
    let read_only = !matches!(
        op.rollback_state,
        RollbackState::Rollbackable | RollbackState::PartiallyRolledBack
    );
    let wire_selection = request.selection.clone().unwrap_or_else(RollbackSelection::everything);
    let summary = match status {
        TreeSummaryStatus::Built => TreeSummaryView::Built,
        TreeSummaryStatus::IndexesBuilding => TreeSummaryView::IndexesBuilding,
        TreeSummaryStatus::IndexesUnavailable => TreeSummaryView::IndexesUnavailable,
        TreeSummaryStatus::Building(progress) => TreeSummaryView::Building { progress },
        TreeSummaryStatus::Unbuilt(reason) => TreeSummaryView::Unbuilt { reason },
    };
    if summary != TreeSummaryView::Built {
        return Ok(Some(OperationTreeView {
            summary,
            read_only,
            root_path: None,
            other_root: None,
            total_rows: 0,
            offset: 0,
            rows: Vec::new(),
            ancestors: Vec::new(),
            cursor_index: None,
            tally: TreeTally::default(),
            selection: wire_selection,
            toggles_capped: false,
        }));
    }

    let mut reader = OpReader::new(conn, op_id, context);
    let mut selection = Selection::resolve(&mut reader, &wire_selection)?;
    let expanded: Vec<i64> = request
        .expanded
        .iter()
        .take(MAX_EXPANDED)
        .filter_map(|key| match key {
            TreeRowKey::Folder { dir_id } => Some(*dir_id),
            TreeRowKey::Item { .. } => None,
        })
        .collect();
    let mut layout = Layout::new(&mut reader, &op, &expanded, read_only)?;
    let total_rows = layout.total_rows();
    let offset = u64::from(request.offset).min(total_rows);
    let limit = u64::from(request.limit.min(MAX_LIMIT));
    let (rows, ancestors) = layout.window(&mut reader, &mut selection, offset, limit)?;
    let cursor_index = match request.cursor {
        Some(key) => layout.cursor_index(&mut reader, key)?,
        None => None,
    };
    let tally = layout.op_tally(&mut reader, &selection)?;
    Ok(Some(OperationTreeView {
        summary,
        read_only,
        root_path: layout.root_path(),
        other_root: layout.other_root(),
        total_rows: clamp_u32(total_rows),
        offset: clamp_u32(offset),
        rows,
        ancestors,
        cursor_index: cursor_index.map(clamp_u32),
        tally,
        selection: selection.normalized().clone(),
        toggles_capped: selection.capped(),
    }))
}
