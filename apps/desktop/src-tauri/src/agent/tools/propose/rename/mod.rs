//! The `propose_rename_plan` tool. It stages a bounded, cache-validated rename
//! proposal without touching the filesystem.
//!
//! Every row carries typed [`RenameEvidence`](super::evidence::RenameEvidence) for where
//! its name came from, and a row claiming image content is checked against
//! [`ImageFactsLedger`](super::evidence::ImageFactsLedger) before anything is staged. A
//! plan with even one unbacked claim stages NOTHING and comes back to the model as a typed
//! refusal, so the user is never shown a name whose evidence didn't check out. See
//! `evidence.rs` for the guardrail itself.
//!
//! Four concerns, one per file:
//! - [`plan`]: the tool boundary — schema, dispatch, and everything a plan must survive
//!   before a single row is staged (scope, validation, the evidence check).
//! - [`store`]: what a staged proposal IS and how long it lives — the rows, the display
//!   snapshot, the accepted-preflight handoff, and the TTL'd store.
//! - [`preflight`](mod@preflight): user-action-time revalidation of the subset the user allows, and the
//!   fingerprints apply later checks the sources against.
//! - [`revise`]: the user's own name for one row, replacing the model's without re-running the
//!   plan boundary's whole-plan gates.

mod plan;
mod preflight;
mod revise;
mod store;

pub use plan::{
    RenameDispatchOutcome, dispatch, execute_propose_rename_plan, note_image_facts_delivered,
    propose_rename_plan_schema, revoke_image_facts_evidence,
};
pub use preflight::{
    BulkRenameBlockReason, BulkRenamePreflight, BulkRenamePreflightRow, BulkRenamePreflightStatus, BulkRenameRowStatus,
    BulkRenameWarning, preflight,
};
pub use revise::revise_row;
pub use store::{
    AcceptedPreflight, RenameProposal, RenameProposalRow, RenameProposalRowSnapshot, RenameProposalSnapshot,
    RenameProposalStore, RenameSourceFingerprint,
};

#[cfg(test)]
mod tests;
