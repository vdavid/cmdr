//! The `interrupted` recovery sweep.

use rusqlite::{Connection, params};

use super::super::AgentStoreError;
use crate::agent::types::ProposalStatus;

/// Mark every still-`approved` group `interrupted`, and report how many.
///
/// An `approved` group means "handed to the queue, execution in flight". At startup nothing
/// is in flight by definition, so an approved group found here is one the app died on: some
/// of its ops may have run and nothing in the store knows which. `Interrupted` says exactly
/// that, and it is FROZEN — the user re-approves (minting a new group with a fresh preflight)
/// or discards, and no agent path can touch it meanwhile.
///
/// ❌ Call this exactly ONCE per launch, from `agent::start`. It must NOT live in
/// `open_write_connection`, which runs the migration ladder on every connection open: a
/// sweep there would fire in the middle of a session and reclassify a group that is genuinely
/// executing right now.
///
/// Idempotent: a second run matches nothing, because the first left no `approved` rows.
/// `Completed` groups are untouched — they finished, and re-approving them would run their
/// ops a second time.
pub fn recover_interrupted_groups(conn: &Connection) -> Result<u64, AgentStoreError> {
    let swept = conn
        .prepare_cached("UPDATE proposals SET status = ?2 WHERE status = ?1")?
        .execute(params![
            ProposalStatus::Approved.as_token(),
            ProposalStatus::Interrupted.as_token()
        ])?;
    if swept > 0 {
        log::info!(
            target: "agent::store",
            "{swept} approved proposal group(s) were still in flight at the last quit; marked interrupted"
        );
    }
    Ok(swept as u64)
}
