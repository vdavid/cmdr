//! The operation header's state writes: how an op ended (finalize, or the next
//! launch's end for one a crash interrupted) and its rollback state transitions.

use rusqlite::Connection;

use super::{FinalizeOperation, FinalizeOutcome};
use crate::store::OperationLogStoreError;
use crate::types::{
    ExecutionStatus, NotRollbackableReason, RollbackState, RowRole, SearchCoverage, SearchCoverageReason,
};

/// Write the terminal state and return the per-`row_role` durable counts.
pub(super) fn apply_finalize(
    conn: &Connection,
    f: &FinalizeOperation,
) -> Result<FinalizeOutcome, OperationLogStoreError> {
    conn.execute(
        "UPDATE operations SET
            execution_status = ?2,
            rollback_state = ?3,
            not_rollbackable_reason = ?4,
            archive_subkind = ?5,
            search_coverage = ?6,
            search_coverage_reason = ?7,
            ended_at = ?8,
            -- COALESCE keeps the open-time item_count when finalize supplies NULL
            -- (instant/archive/no-status paths), else refines it to the scanned total.
            item_count = COALESCE(?9, item_count),
            items_done = ?10,
            bytes_total = ?11,
            dev_summary = ?12
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
            f.item_count.map(|c| c as i64),
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

/// Transition an op's `rollback_state` + nullable reason (rollback). A no-op if the
/// op_id is unknown (the row count is 0, not an error).
pub(super) fn apply_set_rollback_state(
    conn: &Connection,
    op_id: &str,
    state: RollbackState,
    reason: Option<NotRollbackableReason>,
) -> Result<(), OperationLogStoreError> {
    conn.execute(
        "UPDATE operations SET rollback_state = ?2, not_rollbackable_reason = ?3 WHERE op_id = ?1",
        rusqlite::params![op_id, state.as_token(), reason.map(|r| r.as_token())],
    )?;
    Ok(())
}

/// End an interrupted op in one statement: `failed`, `ended_at = started_at`, search
/// coverage `top_level_only(interrupted)` (rows it never wrote are missing), and the
/// rollback state `eligibility` names (`None` keeps what it stored). Only while it's
/// still `queued` or `running`, so a finalize that landed first wins.
pub(super) fn apply_end_interrupted(
    conn: &Connection,
    op_id: &str,
    eligibility: Option<(RollbackState, Option<NotRollbackableReason>)>,
) -> Result<(), OperationLogStoreError> {
    let (state, reason) = eligibility.unzip();
    conn.execute(
        "UPDATE operations SET
            execution_status = ?2,
            ended_at = started_at,
            rollback_state = COALESCE(?3, rollback_state),
            not_rollbackable_reason = CASE WHEN ?3 IS NULL THEN not_rollbackable_reason ELSE ?4 END,
            search_coverage = ?7,
            search_coverage_reason = ?8
         WHERE op_id = ?1 AND execution_status IN (?5, ?6)",
        rusqlite::params![
            op_id,
            ExecutionStatus::Failed.as_token(),
            state.map(|s| s.as_token()),
            reason.flatten().map(|r| r.as_token()),
            ExecutionStatus::Queued.as_token(),
            ExecutionStatus::Running.as_token(),
            SearchCoverage::TopLevelOnly.as_token(),
            SearchCoverageReason::Interrupted.as_token(),
        ],
    )?;
    Ok(())
}
