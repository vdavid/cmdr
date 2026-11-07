//! The startup step that ends every operation a crash left `queued` or `running`
//! (`DETAILS.md` § "Operations a crash interrupted"). At open no engine runs yet, so
//! each one is stale: without this it would read "still running" forever, retention
//! by age would never prune it (no `ended_at`), its tree would never backfill, and it
//! would sit at the `not_rollbackable` it opened with.

use crate::store::{OperationLogStoreError, OperationRow, any_row_overwrote, open_read_connection, ops_left_running};
use crate::types::{NotRollbackableReason, OpKind, RollbackState};
use crate::writer::OperationLogWriter;

/// End every operation still `queued` or `running`: `failed`, ended when it started
/// (the only time the journal knows; it never claims a duration it didn't see, and
/// never jumps to "now" to outlive its retention), with the rollback state
/// [`interrupted_eligibility`] decides. Called once by
/// [`OperationLog::open`](crate::OperationLog::open), AFTER the rolling-back reconcile
/// (which reads an interrupted inverse's rows) and before the journal is reachable, so
/// no operation this launch journals can be mistaken for a stale one.
pub(crate) fn end_interrupted_on_open(writer: &OperationLogWriter) {
    let conn = match open_read_connection(writer.db_path()) {
        Ok(c) => c,
        Err(e) => {
            log::warn!(target: "operation_log", "interrupted ops: read connection failed: {e}");
            return;
        }
    };
    let stale = match ops_left_running(&conn) {
        Ok(ops) => ops,
        Err(e) => {
            log::warn!(target: "operation_log", "interrupted ops: query failed: {e}");
            return;
        }
    };
    for op in stale {
        let eligibility = interrupted_eligibility(&op, || any_row_overwrote(&conn, &op.op_id));
        if let Err(e) = writer.end_interrupted(&op.op_id, eligibility) {
            log::warn!(target: "operation_log", "interrupted ops: ending {} failed: {e}", op.op_id);
        } else {
            log::info!(target: "operation_log", "interrupted ops: {} ended ⇒ {eligibility:?}", op.op_id);
        }
    }
}

/// The rollback state an interrupted op ends with, decided from its rows the way
/// finalize would have, failing safe where the deciding facts were never written.
/// `None` keeps what it stored: an inverse op (a rollback is never rolled back, and
/// its original's state is the reconcile's call).
///
/// - `delete` can't be restored: `permanent_delete`.
/// - `copy` with a row that overwrote: `overwrote`. Without one, rollbackable.
/// - `move` and `archive_edit`: `journal_incomplete`. A directory merge, a staged
///   conflict resolved at the destination, and a compress over an existing archive are
///   notes the driver hands finalize, which never came; guessing could carry untouched
///   files away.
/// - Every other kind is rollbackable: every inverse rechecks before it acts, and the
///   rows the crash lost simply aren't reversed.
///
/// `overwrote` is asked only for a copy. A read that fails answers `overwrote`.
fn interrupted_eligibility(
    op: &OperationRow,
    overwrote: impl FnOnce() -> Result<bool, OperationLogStoreError>,
) -> Option<(RollbackState, Option<NotRollbackableReason>)> {
    if op.rolls_back_op_id.is_some() {
        return None;
    }
    let refused = |reason| Some((RollbackState::NotRollbackable, Some(reason)));
    match op.kind {
        OpKind::Delete => refused(NotRollbackableReason::PermanentDelete),
        OpKind::Copy => match overwrote() {
            Ok(false) => Some((RollbackState::Rollbackable, None)),
            Ok(true) => refused(NotRollbackableReason::Overwrote),
            Err(e) => {
                log::warn!(target: "operation_log", "interrupted ops: reading {}'s overwrites failed: {e}", op.op_id);
                refused(NotRollbackableReason::Overwrote)
            }
        },
        OpKind::Move | OpKind::ArchiveEdit => refused(NotRollbackableReason::JournalIncomplete),
        OpKind::Trash | OpKind::Rename | OpKind::CreateFolder | OpKind::CreateFile | OpKind::Tag => {
            Some((RollbackState::Rollbackable, None))
        }
    }
}

#[cfg(test)]
mod tests;
