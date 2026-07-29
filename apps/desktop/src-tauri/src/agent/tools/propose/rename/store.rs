//! What a staged rename proposal is, and how long it lives.
//!
//! A proposal is immutable server-owned data: the tool boundary stages it, the review
//! surface sees only a display snapshot, and the frontend hands back opaque row ids. Paths,
//! destination names, and source fingerprints never leave this process.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::agent::tools::propose::evidence::RenameEvidence;
use crate::ignore_poison::IgnorePoison;

const PROPOSAL_TTL: Duration = Duration::from_secs(15 * 60);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenameProposal {
    pub proposal_id: String,
    pub rows: Vec<RenameProposalRow>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenameProposalRow {
    pub row_id: String,
    pub source_path: String,
    pub volume_id: String,
    pub destination_name: String,
    pub evidence: RenameEvidence,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameProposalSnapshot {
    pub proposal_id: String,
    pub rows: Vec<RenameProposalRowSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameProposalRowSnapshot {
    pub row_id: String,
    pub source_name: String,
    pub destination_name: String,
    /// Why this name, for the review dialog's rightmost column. The frontend maps `source`
    /// to a localized label and renders `detail` as PLAIN TEXT (it's model-authored, so
    /// never `{@html}`); its length is bounded by the evidence check.
    pub evidence: RenameEvidence,
}

impl RenameProposal {
    pub fn snapshot(&self) -> RenameProposalSnapshot {
        RenameProposalSnapshot {
            proposal_id: self.proposal_id.clone(),
            rows: self
                .rows
                .iter()
                .map(|row| RenameProposalRowSnapshot {
                    row_id: row.row_id.clone(),
                    source_name: Path::new(&row.source_path)
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or(&row.source_path)
                        .to_string(),
                    destination_name: row.destination_name.clone(),
                    evidence: row.evidence.clone(),
                })
                .collect(),
        }
    }
}

struct StoredProposal {
    proposal: RenameProposal,
    expires_at: Instant,
    accepted_preflight: Option<AcceptedPreflight>,
}

/// The exact user-approved subset that passed the latest preflight. The apply
/// command consumes this later; the frontend never receives fingerprints.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcceptedPreflight {
    pub allowed_row_ids: Vec<String>,
    pub fingerprints: Vec<RenameSourceFingerprint>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenameSourceFingerprint {
    Local {
        row_id: String,
        device: u64,
        inode: u64,
        size: u64,
        modified_nanos: Option<u128>,
    },
    Remote {
        row_id: String,
        normalized_path: String,
        size: Option<u64>,
        modified: Option<i64>,
    },
}

#[derive(Default)]
pub struct RenameProposalStore {
    proposals: Mutex<HashMap<String, StoredProposal>>,
}

impl RenameProposalStore {
    pub fn stage(&self, proposal: RenameProposal) -> RenameProposalSnapshot {
        let snapshot = proposal.snapshot();
        let mut proposals = self.proposals.lock_ignore_poison();
        proposals.retain(|_, stored| stored.expires_at > Instant::now());
        proposals.insert(
            proposal.proposal_id.clone(),
            StoredProposal {
                proposal,
                expires_at: Instant::now() + PROPOSAL_TTL,
                accepted_preflight: None,
            },
        );
        snapshot
    }

    /// Gets an immutable proposal for repeated review-time checks. Expired
    /// records are removed and indistinguishable from a missing id to callers.
    pub fn get(&self, proposal_id: &str) -> Option<RenameProposal> {
        let mut proposals = self.proposals.lock_ignore_poison();
        let is_live = proposals
            .get(proposal_id)
            .is_some_and(|stored| stored.expires_at > Instant::now());
        if !is_live {
            proposals.remove(proposal_id);
            return None;
        }
        proposals.get(proposal_id).map(|stored| stored.proposal.clone())
    }

    /// Discards a proposal after an explicit user cancellation or terminal apply.
    pub fn consume(&self, proposal_id: &str) -> Option<RenameProposal> {
        let stored = self.proposals.lock_ignore_poison().remove(proposal_id)?;
        (stored.expires_at > Instant::now()).then_some(stored.proposal)
    }

    pub fn record_accepted_preflight(&self, proposal_id: &str, accepted: AcceptedPreflight) -> bool {
        let mut proposals = self.proposals.lock_ignore_poison();
        let Some(stored) = proposals.get_mut(proposal_id) else {
            return false;
        };
        if stored.expires_at <= Instant::now() {
            proposals.remove(proposal_id);
            return false;
        }
        stored.accepted_preflight = Some(accepted);
        true
    }

    pub fn accepted_preflight(&self, proposal_id: &str, allowed_row_ids: &[String]) -> Option<AcceptedPreflight> {
        let proposal = self.get(proposal_id)?;
        let proposals = self.proposals.lock_ignore_poison();
        let stored = proposals.get(&proposal.proposal_id)?;
        let accepted = stored.accepted_preflight.clone()?;
        (accepted.allowed_row_ids == allowed_row_ids).then_some(accepted)
    }

    /// Atomically consumes the exact user-approved subset after a successful
    /// preflight. Once apply begins, the proposal cannot be replayed or altered.
    pub fn take_accepted_preflight(
        &self,
        proposal_id: &str,
        allowed_row_ids: &[String],
    ) -> Option<(RenameProposal, AcceptedPreflight)> {
        let mut proposals = self.proposals.lock_ignore_poison();
        let stored = proposals.get(proposal_id)?;
        if stored.expires_at <= Instant::now() {
            proposals.remove(proposal_id);
            return None;
        }
        if stored
            .accepted_preflight
            .as_ref()
            .is_none_or(|accepted| accepted.allowed_row_ids != allowed_row_ids)
        {
            return None;
        }
        let stored = proposals.remove(proposal_id)?;
        Some((stored.proposal, stored.accepted_preflight?))
    }
}
