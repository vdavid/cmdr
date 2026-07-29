//! What a staged rename proposal is, and how long it lives.
//!
//! A proposal is immutable server-owned data: the tool boundary stages it, the review surface
//! sees only a display snapshot, and the frontend hands back opaque ROW IDS and nothing else.
//! That's the authority boundary: source fingerprints never leave this process, and every
//! later step (preflight, apply) resolves paths and names from the stored proposal by row id,
//! so a client-supplied value is never trusted. The snapshot's own path is display data — the
//! review dialog previews the file the user is being asked to rename.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::agent::tools::propose::evidence::{EvidenceCoverage, RenameEvidence};
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
    /// How much of the delivered text this row's quote covers, for an accepted `imageText`
    /// claim. Filled in by the evidence check AFTER it accepted the row, so it describes a
    /// delivery the ledger recorded; `None` for every other source.
    pub coverage: Option<EvidenceCoverage>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameProposalSnapshot {
    pub proposal_id: String,
    pub rows: Vec<RenameProposalRowSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RenameProposalRowSnapshot {
    pub row_id: String,
    pub source_name: String,
    pub destination_name: String,
    /// The file this row renames, so the dialog can show the user the thing itself: a
    /// thumbnail per row, and the full viewer for the focused one. DISPLAY ONLY — apply
    /// resolves the path from the stored proposal by row id and never from the client.
    pub source_path: String,
    /// Which volume `source_path` lives on, so the viewer can pull a file on a remote parent.
    pub volume_id: String,
    /// Why this name, for the review dialog's evidence column. The frontend maps `source`
    /// to a localized label and renders `detail` as PLAIN TEXT (it's model-authored, so
    /// never `{@html}`); its length is bounded by the evidence check.
    pub evidence: RenameEvidence,
    /// How thin the match behind this name is (`imageText` rows only). The dialog renders the
    /// quote inside its surrounding line plus a coverage figure, so a sliver of a page of OCR
    /// can't look as strong as a decisive quote.
    pub coverage: Option<EvidenceCoverage>,
}

impl RenameProposal {
    pub fn snapshot(&self) -> RenameProposalSnapshot {
        RenameProposalSnapshot {
            proposal_id: self.proposal_id.clone(),
            rows: self.rows.iter().map(RenameProposalRow::snapshot).collect(),
        }
    }

    /// The destination names of the given rows, in the order they were allowed. An unknown id
    /// contributes nothing, so a subset that names a row this proposal doesn't have can never
    /// produce a name list that matches a recorded one.
    pub(super) fn destination_names_for(&self, row_ids: &[String]) -> Vec<String> {
        row_ids
            .iter()
            .filter_map(|row_id| self.rows.iter().find(|row| row.row_id == *row_id))
            .map(|row| row.destination_name.clone())
            .collect()
    }
}

impl RenameProposalRow {
    pub fn snapshot(&self) -> RenameProposalRowSnapshot {
        RenameProposalRowSnapshot {
            row_id: self.row_id.clone(),
            source_name: Path::new(&self.source_path)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(&self.source_path)
                .to_string(),
            destination_name: self.destination_name.clone(),
            source_path: self.source_path.clone(),
            volume_id: self.volume_id.clone(),
            evidence: self.evidence.clone(),
            coverage: self.coverage.clone(),
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
    /// The destination names those rows carried when preflight cleared them, in the same
    /// order. Row ids alone don't say WHICH names were checked, and duplicate-destination,
    /// cycle, and case-only detection all live in preflight: binding the names is what stops
    /// an edited name from riding an older acceptance onto the filesystem (invariant 10).
    pub allowed_destination_names: Vec<String>,
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

    /// Replaces one staged row's destination name with the one the user typed, and drops
    /// everything that described the model's name for it: the evidence becomes the
    /// `UserEdited` marker and the coverage goes (invariant 10).
    ///
    /// The name arrives already validated — the caller ([`super::revise`]) is the boundary
    /// that checks it, exactly as the tool boundary checks the model's.
    pub fn revise_row(
        &self,
        proposal_id: &str,
        row_id: &str,
        destination_name: &str,
    ) -> Option<RenameProposalRowSnapshot> {
        let mut proposals = self.proposals.lock_ignore_poison();
        let stored = proposals.get_mut(proposal_id)?;
        if stored.expires_at <= Instant::now() {
            proposals.remove(proposal_id);
            return None;
        }
        let row = stored.proposal.rows.iter_mut().find(|row| row.row_id == row_id)?;
        row.destination_name = destination_name.to_string();
        row.evidence = RenameEvidence::user_edited();
        row.coverage = None;
        let snapshot = row.snapshot();
        // Apply skips its own re-check when the allowed row ids match the acceptance, so a name
        // that changed since that check has to force a fresh preflight.
        stored.accepted_preflight = None;
        Some(snapshot)
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
        accepted_matches(&accepted, &stored.proposal, allowed_row_ids).then_some(accepted)
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
            .is_none_or(|accepted| !accepted_matches(accepted, &stored.proposal, allowed_row_ids))
        {
            return None;
        }
        let stored = proposals.remove(proposal_id)?;
        Some((stored.proposal, stored.accepted_preflight?))
    }
}

/// Whether an acceptance still describes what the caller is asking to apply: the same rows AND
/// the same names. Row ids alone can't say that — a revised row keeps its id — and every
/// name-level check (duplicate destinations, cycles, case-only edges, target-exists) happened
/// during the preflight that recorded this. So a mismatch means "check it again", never
/// "apply it anyway".
fn accepted_matches(accepted: &AcceptedPreflight, proposal: &RenameProposal, allowed_row_ids: &[String]) -> bool {
    accepted.allowed_row_ids == allowed_row_ids
        && accepted.allowed_destination_names == proposal.destination_names_for(allowed_row_ids)
}
