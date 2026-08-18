//! One staged row's name, replaced by the one the user typed.
//!
//! A review row used to be allow-or-deny, so a plausible wrong name left the user only the
//! model's name or the old one. Revise is the third option, and it is deliberately its own
//! narrow operation rather than a re-staged plan: re-staging would re-run two gates that must
//! not fire for an edit. The evidence check refuses the WHOLE plan when one `call_id` was
//! revoked, so fixing row seven could destroy a 50-row review; and the pane's effective scope
//! has moved on by review time, so every row would refuse.
//!
//! What it must do instead, on the server:
//!
//! - **Validate the name.** This is the first destination name that crosses IPC, so it passes
//!   the same [`validate_destination_name`] the model's names pass. Apply never trusts a
//!   client-supplied name; it resolves this stored row by opaque row id.
//! - **Replace the evidence, not keep it.** The model's quote described the model's name.
//! - **Invalidate the accepted preflight**, so duplicate-destination, cycle, and case-only
//!   detection see the new name before anything reaches the filesystem (invariant 10). Twice
//!   over: the edited name changes the spine's binding digest, so the claim refuses, AND the
//!   fingerprints held for that acceptance are dropped here.

use rusqlite::Connection;
use tauri::{AppHandle, Manager, Runtime};

use super::plan::validate_destination_name;
use super::store::{AcceptedRenamePreflights, RenameProposalRowSnapshot};
use crate::agent::AgentDb;
use crate::mcp::ToolError;

/// Replace one row's destination name with the user's own, answering the row as the review
/// dialog should now show it.
pub fn revise_row<R: Runtime>(
    app: &AppHandle<R>,
    proposal_id: &str,
    row_id: &str,
    destination_name: &str,
) -> Result<RenameProposalRowSnapshot, ToolError> {
    let (Some(db), Some(accepted)) = (app.try_state::<AgentDb>(), app.try_state::<AcceptedRenamePreflights>()) else {
        return Err(review_is_over());
    };
    let conn = db.open_write_connection().map_err(|e| {
        log::warn!(target: "agent::propose", "revising a proposed name couldn't open main.db: {e}");
        review_is_over()
    })?;
    revise_staged_row(&conn, &accepted, proposal_id, row_id, destination_name)
}

/// The operation itself, over a connection alone: validate, then replace. Pure of Tauri so the
/// guardrails around it are testable without an app.
pub(super) fn revise_staged_row(
    conn: &Connection,
    accepted: &AcceptedRenamePreflights,
    proposal_id: &str,
    row_id: &str,
    destination_name: &str,
) -> Result<RenameProposalRowSnapshot, ToolError> {
    validate_destination_name(destination_name)?;
    let revised = super::store::revise_row(conn, proposal_id, row_id, destination_name)
        .map_err(|e| {
            log::warn!(target: "agent::propose", "revising a proposed name didn't land: {e}");
            review_is_over()
        })?
        .ok_or_else(review_is_over)?;
    // The second guard, and the one that doesn't depend on the spine noticing: the fingerprints
    // this group's acceptance paired with describe a plan whose names have moved on.
    accepted.forget(proposal_id);
    Ok(revised)
}

fn review_is_over() -> ToolError {
    ToolError::invalid_params("This rename review has expired. Ask Cmdr to prepare it again.")
}
