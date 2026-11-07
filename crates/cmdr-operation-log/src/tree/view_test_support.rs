//! Reading a fixture's tree the way the dialog does, and rendering rows one line each
//! so a cell reads as the tree it pins.

use super::selection::{RollbackSelection, SelectionBase, SelectionToggle};
use super::status::BackfillRegistry;
use super::test_support::Journal;
use super::view::{
    OperationTreeRequest, OperationTreeRow, OperationTreeView, TreeCheckbox, TreeRowKey, operation_tree,
};
use crate::types::{ExecutionStatus, RollbackState, SearchCoverage, SearchCoverageReason};
use crate::writer::FinalizeOperation;

/// A request with no hand toggles, the first 200 rows.
pub(super) fn request() -> OperationTreeRequest {
    OperationTreeRequest {
        expanded: Vec::new(),
        selection: None,
        cursor: None,
        offset: 0,
        limit: 200,
    }
}

impl OperationTreeRequest {
    pub(super) fn open(mut self, keys: &[TreeRowKey]) -> Self {
        self.expanded.extend_from_slice(keys);
        self
    }

    pub(super) fn select(mut self, base: SelectionBase, toggles: &[SelectionToggle]) -> Self {
        self.selection = Some(RollbackSelection {
            base,
            toggles: toggles.to_vec(),
        });
        self
    }

    pub(super) fn window(mut self, offset: u32, limit: u32) -> Self {
        self.offset = offset;
        self.limit = limit;
        self
    }

    pub(super) fn cursor(mut self, key: TreeRowKey) -> Self {
        self.cursor = Some(key);
        self
    }
}

/// The op's tree for `request`.
pub(super) fn tree(j: &Journal, op_id: &str, request: &OperationTreeRequest) -> OperationTreeView {
    let conn = j.read();
    operation_tree(&conn, op_id, request, &BackfillRegistry::default())
        .expect("tree read")
        .expect("the op exists")
}

/// One line per row: indent by level, label, then what the row says.
pub(super) fn render(rows: &[OperationTreeRow]) -> Vec<String> {
    rows.iter().map(line).collect()
}

fn line(row: &OperationTreeRow) -> String {
    let indent = "  ".repeat(usize::from(row.level.saturating_sub(1)));
    let mut out = format!("{indent}{}", row.label);
    if row.expandable {
        out.push_str(if row.expanded { " [-]" } else { " [+]" });
    }
    out.push_str(match row.checkbox {
        TreeCheckbox::On => " (x)",
        TreeCheckbox::Off => " ( )",
        TreeCheckbox::Mixed => " (~)",
        TreeCheckbox::None => "",
    });
    if let Some(role) = row.role {
        out.push_str(&format!(" {role:?}"));
    }
    if let Some(reason) = row.unexpandable {
        out.push_str(&format!(" !{reason:?}"));
    }
    if let Some(other) = &row.other_end {
        out.push_str(&format!(" <- {other}"));
    }
    out
}

/// The key of the row labelled `label` in `rows`.
pub(super) fn key(rows: &[OperationTreeRow], label: &str) -> TreeRowKey {
    rows.iter()
        .find(|row| row.label == label)
        .unwrap_or_else(|| panic!("no row {label} in {:?}", render(rows)))
        .key
}

/// The dir id of the folder row labelled `label`.
pub(super) fn dir(rows: &[OperationTreeRow], label: &str) -> i64 {
    match key(rows, label) {
        TreeRowKey::Folder { dir_id } => dir_id,
        TreeRowKey::Item { .. } => panic!("{label} is a file"),
    }
}

/// Finish `op_id` with `state` and the given coverage.
pub(super) fn finish(
    j: &Journal,
    op_id: &str,
    state: RollbackState,
    coverage: SearchCoverage,
    reason: Option<SearchCoverageReason>,
) {
    j.writer
        .finalize_operation(FinalizeOperation {
            op_id: op_id.into(),
            execution_status: ExecutionStatus::Done,
            rollback_state: state,
            not_rollbackable_reason: None,
            archive_subkind: None,
            search_coverage: coverage,
            search_coverage_reason: reason,
            ended_at: 2,
            item_count: None,
            items_done: 0,
            bytes_total: 0,
            dev_summary: None,
        })
        .expect("finalize");
}
