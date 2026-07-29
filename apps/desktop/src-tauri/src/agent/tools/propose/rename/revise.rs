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
//!   detection see the new name before anything reaches the filesystem (invariant 10).

use tauri::{AppHandle, Manager, Runtime};

use super::plan::validate_destination_name;
use super::store::{RenameProposalRowSnapshot, RenameProposalStore};
use crate::mcp::ToolError;

/// Replace one row's destination name with the user's own, answering the row as the review
/// dialog should now show it.
pub fn revise_row<R: Runtime>(
    app: &AppHandle<R>,
    proposal_id: &str,
    row_id: &str,
    destination_name: &str,
) -> Result<RenameProposalRowSnapshot, ToolError> {
    let store = app
        .try_state::<RenameProposalStore>()
        .ok_or_else(|| ToolError::internal("This rename review has expired. Ask Cmdr to prepare it again."))?;
    revise_staged_row(&store, proposal_id, row_id, destination_name)
}

/// The operation itself, over the store alone: validate, then replace. Pure of Tauri so the
/// guardrails around it are testable without an app.
pub(super) fn revise_staged_row(
    store: &RenameProposalStore,
    proposal_id: &str,
    row_id: &str,
    destination_name: &str,
) -> Result<RenameProposalRowSnapshot, ToolError> {
    validate_destination_name(destination_name)?;
    store
        .revise_row(proposal_id, row_id, destination_name)
        .ok_or_else(|| ToolError::invalid_params("This rename review has expired. Ask Cmdr to prepare it again."))
}
