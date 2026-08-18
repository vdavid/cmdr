//! Rename-proposal tests, one file per concern:
//!
//! - `plan.rs`: the tool boundary — the pane's effective scope, per-row validation, and the
//!   evidence guardrail.
//! - `store.rs`: what staging into `main.db` makes true — durability, the acceptance
//!   binding, and the user's own revise.
//! - `preflight.rs`: the preflight engine's blocks and warnings.

mod plan;
mod preflight;
mod store;

use super::store::RenameDraftRow;
use crate::agent::tools::propose::evidence::{EvidenceScope, EvidenceSource, RenameEvidence};

/// The chat thread these tests deliver into and propose from.
const THREAD: EvidenceScope = EvidenceScope::Thread(11);

/// One row of a plan on its way in: no id yet, because the store hands those out.
fn draft_row(source_path: &str, destination_name: &str, source: EvidenceSource, detail: &str) -> RenameDraftRow {
    RenameDraftRow {
        source_path: source_path.into(),
        destination_name: destination_name.into(),
        evidence: RenameEvidence {
            source,
            detail: detail.into(),
        },
        coverage: None,
    }
}
