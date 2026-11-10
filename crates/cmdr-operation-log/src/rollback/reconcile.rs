//! The startup reconcile: resolve every op a crash left `rolling_back`, from its own
//! rows (or its tree summary) and every inverse's (`DETAILS.md` § "The `rolling_back`
//! state machine + startup reconcile").

use crate::store::{any_inverse_reversed, has_rollback_outcome, open_read_connection, ops_in_rolling_back};
use crate::tree::summary::summary_shows_rollback_outcome;
use crate::types::RollbackState;
use crate::writer::OperationLogWriter;

/// On open, resolve every operation left `rolling_back` by a crash mid-rollback
/// deterministically, from its own rows and every inverse's ([`reconcile_one`]):
/// `partially_rolled_back` when any pass durably reversed or held back something,
/// else back to `rollbackable` (including a crash after setting `rolling_back` but
/// before the inverse op opened: nothing ran).
///
/// Either way a re-issued rollback resumes safely — every per-item inverse is an
/// idempotent recheck-then-act, so already-reversed items no-op. Called once at
/// [`OperationLog::open`](crate::OperationLog::open), before the journal is reachable.
pub fn reconcile_rolling_back_on_open(writer: &OperationLogWriter) {
    let conn = match open_read_connection(writer.db_path()) {
        Ok(c) => c,
        Err(e) => {
            log::warn!(target: "operation_log", "rollback reconcile: read connection failed: {e}");
            return;
        }
    };
    let stuck = match ops_in_rolling_back(&conn) {
        Ok(ops) => ops,
        Err(e) => {
            log::warn!(target: "operation_log", "rollback reconcile: query failed: {e}");
            return;
        }
    };
    for op in stuck {
        let resolved = reconcile_one(&conn, &op.op_id);
        if let Err(e) = writer.set_rollback_state(&op.op_id, resolved, None) {
            log::warn!(target: "operation_log", "rollback reconcile: set state for {} failed: {e}", op.op_id);
        } else {
            log::info!(target: "operation_log", "rollback reconcile: {} left rolling_back ⇒ {resolved:?}", op.op_id);
        }
    }
}

/// The reconcile verdict for one stuck op (see [`reconcile_rolling_back_on_open`]).
///
/// Partial when ANY earlier pass left a durable trace: the original has a row it
/// reversed or held back, or ANY inverse of it (not only the newest: a second pass
/// that crashed early has an empty one) has a `done` row. Each source alone misses a
/// crash: `RunAcc::flush` sends the inverse's rows THEN the original's outcomes, so a
/// crash between the two leaves `done` inverse rows over an untouched original, and a
/// dropped inverse row leaves the reverse. Else nothing was reversed or held back, and
/// the op is cleanly `rollbackable`. A read that fails resolves partial: never claim
/// nothing came back on an unanswered question (a retry is admitted either way).
///
/// A BUILT tree summary answers the original's clause from its volume-root rows (a
/// handful, kept exact in the outcome transactions themselves); only an unbuilt op
/// pays the `EXISTS` over its rows.
fn reconcile_one(conn: &rusqlite::Connection, op_id: &str) -> RollbackState {
    let original_touched = summary_shows_rollback_outcome(conn, op_id).and_then(|summary| match summary {
        Some(hit) => Ok(hit),
        None => has_rollback_outcome(conn, op_id),
    });
    let touched = original_touched.and_then(|hit| {
        if hit {
            Ok(true)
        } else {
            any_inverse_reversed(conn, op_id)
        }
    });
    match touched {
        Ok(true) => RollbackState::PartiallyRolledBack,
        Ok(false) => RollbackState::Rollbackable,
        Err(e) => {
            log::warn!(target: "operation_log", "rollback reconcile: reading {op_id}'s outcomes failed: {e}");
            RollbackState::PartiallyRolledBack
        }
    }
}
