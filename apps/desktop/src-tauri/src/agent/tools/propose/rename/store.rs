//! Where a staged rename proposal LIVES: one group on the proposal spine, in `main.db`.
//!
//! A proposal is server-owned data: the tool boundary stages it, the review surface sees only a
//! display snapshot, and the frontend hands back opaque ROW IDS plus (on a revise) one name it
//! typed. That's the authority boundary: source fingerprints never leave this process, and every
//! later step (preflight, apply) resolves paths and names from the stored proposal by row id,
//! so a client-supplied value is never trusted. The snapshot's own path is display data — the
//! review dialog previews the file the user is being asked to rename.
//!
//! Rename is the spine's documented exception ([`GroupIntent::Rename`]): its ops carry their own
//! destinations and the group binds the shared PARENT, because `start_bulk_rename` refuses a row
//! whose source and destination parents differ.
//!
//! Two halves, and each is durable in a different way ON PURPOSE:
//!
//! - **The proposal is durable.** It has no expiry; a suggestion waits until the user acts on it,
//!   and it survives a restart because the spine holds it.
//! - **The accepted preflight is NOT** ([`AcceptedRenamePreflights`], process-local). It carries
//!   the source fingerprints apply rechecks, and those describe files as they were minutes ago.
//!   A restart must force a fresh preflight rather than resurrect an approval given before the
//!   app died.
//!
//! The rows are immutable to the AGENT: [`revise_row`] is the one mutation, it belongs to the
//! user, and it invalidates the accepted preflight so the new name can't reach the filesystem
//! unchecked (see [`super::revise`]).

use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

use rusqlite::{Connection, Transaction, TransactionBehavior, params};
use serde::Serialize;

use crate::agent::store::AgentStoreError;
use crate::agent::store::proposals::{GroupIntent, NewGroup, NewRename, NewSweep, ProposalOp, get_group, page_ops};
use crate::agent::tools::propose::evidence::{EvidenceCoverage, EvidenceSource, RenameEvidence};
use crate::agent::types::ProposalStatus;
use crate::ignore_poison::IgnorePoison;

/// The most rows one plan can stage, from the tool boundary's own cap. Paging a rename group
/// would buy nothing: the review dialog shows every row at once.
const MAX_RENAME_ROWS: u32 = super::plan::MAX_RENAMES as u32;

// ── What the tool boundary hands over, before the store gives its rows ids ────

/// A validated rename plan on its way into the store. It has no ids yet: the spine assigns
/// them, which is what makes them opaque to everything above.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenameDraft {
    /// The one volume every source lives on.
    pub volume_id: String,
    /// The one folder every source lives in. A rename group binds a shared parent because
    /// `start_bulk_rename` refuses a row that would change it.
    pub parent: String,
    pub rows: Vec<RenameDraftRow>,
}

/// One row of a plan the store hasn't taken yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenameDraftRow {
    pub source_path: String,
    pub destination_name: String,
    pub evidence: RenameEvidence,
    /// Filled in by the evidence check AFTER it accepted the row, so it describes a delivery
    /// the ledger recorded; `None` for every source that makes no content claim.
    pub coverage: Option<EvidenceCoverage>,
}

// ── What a staged proposal is, once loaded back ───────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenameProposal {
    pub proposal_id: String,
    /// The one volume every row lives on. A GROUP field, never a per-row one: a rename group
    /// binds one source volume, so "do all these rows agree about their volume?" is a question
    /// that can't be asked here, let alone answered wrong.
    pub volume_id: String,
    pub rows: Vec<RenameProposalRow>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenameProposalRow {
    pub row_id: String,
    pub source_path: String,
    pub destination_name: String,
    pub evidence: RenameEvidence,
    /// How much of the delivered text this row's quote covers, for an accepted `imageText`
    /// claim; `None` for every other source.
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
            rows: self.rows.iter().map(|row| row.snapshot(&self.volume_id)).collect(),
        }
    }
}

impl RenameProposalRow {
    /// The row as the dialog shows it. It takes the volume from its PROPOSAL, the only place a
    /// rename's volume exists.
    pub fn snapshot(&self, volume_id: &str) -> RenameProposalRowSnapshot {
        RenameProposalRowSnapshot {
            row_id: self.row_id.clone(),
            source_name: Path::new(&self.source_path)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(&self.source_path)
                .to_string(),
            destination_name: self.destination_name.clone(),
            source_path: self.source_path.clone(),
            volume_id: volume_id.to_string(),
            evidence: self.evidence.clone(),
            coverage: self.coverage.clone(),
        }
    }
}

// ── The accepted preflight, held for this process only ────────────────────────

/// The exact user-approved subset that passed the latest preflight, and the server-only
/// source fingerprints apply rechecks each source against.
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

/// Every group whose latest preflight came back Ready, keyed by proposal id.
///
/// **In memory on purpose, where the proposal itself is durable.** A fingerprint describes a
/// file as it was at review time, and an approval given before the app died says nothing about
/// the disk the app came back to. So a restart drops these and apply falls back to a fresh
/// authoritative preflight, rather than resurrecting an acceptance.
///
/// This holds one half of the binding (the exact allowed row ids). The other half — the VALUES
/// those rows carried — is the spine's own server-owned acceptance record, whose digest covers
/// each live op's id, source path, and destination name. A revised name changes that digest, so
/// the claim refuses and the approval can't ride onto a name preflight never checked.
#[derive(Default)]
pub struct AcceptedRenamePreflights {
    entries: Mutex<HashMap<String, AcceptedPreflight>>,
}

impl AcceptedRenamePreflights {
    pub fn record(&self, proposal_id: &str, accepted: AcceptedPreflight) {
        self.entries
            .lock_ignore_poison()
            .insert(proposal_id.to_string(), accepted);
    }

    /// The acceptance for exactly this subset, or `None` — including when the subset differs,
    /// because an acceptance describes the rows it checked and no others.
    pub fn matching(&self, proposal_id: &str, allowed_row_ids: &[String]) -> Option<AcceptedPreflight> {
        let entries = self.entries.lock_ignore_poison();
        entries
            .get(proposal_id)
            .filter(|accepted| accepted.allowed_row_ids == allowed_row_ids)
            .cloned()
    }

    /// Consume the acceptance for exactly this subset. Apply takes it, so a dialog can't
    /// replay an already-started plan.
    pub fn take_matching(&self, proposal_id: &str, allowed_row_ids: &[String]) -> Option<AcceptedPreflight> {
        let mut entries = self.entries.lock_ignore_poison();
        if entries
            .get(proposal_id)
            .is_none_or(|accepted| accepted.allowed_row_ids != allowed_row_ids)
        {
            return None;
        }
        entries.remove(proposal_id)
    }

    /// Drop this group's acceptance: a revise did, or the review is over.
    pub fn forget(&self, proposal_id: &str) {
        self.entries.lock_ignore_poison().remove(proposal_id);
    }
}

// ── Staging, loading, revising ────────────────────────────────────────────────

/// Stage a plan as one group in a sweep of its own, and answer the review snapshot.
///
/// Two commits, not one: the spine owns group creation, so the evidence rows can only be
/// written once their ops have ids. A crash in between leaves rename ops with no evidence, and
/// [`load`] refuses such a group outright — a name whose backing is missing must never be shown
/// with invented backing.
pub fn stage(
    conn: &Connection,
    conversation_id: Option<i64>,
    draft: &RenameDraft,
    now: i64,
) -> Result<Option<RenameProposalSnapshot>, AgentStoreError> {
    let sweep = NewSweep {
        conversation_id,
        created_by_model: None,
        rationale: None,
    };
    let group = NewGroup {
        intent: GroupIntent::Rename {
            parent: draft.parent.clone(),
            renames: draft
                .rows
                .iter()
                .map(|row| NewRename {
                    source_path: row.source_path.clone(),
                    new_name: row.destination_name.clone(),
                    snapshot: None,
                })
                .collect(),
        },
        source_volume_id: draft.volume_id.clone(),
        display_name: display_name(&draft.parent, draft.rows.len()),
        rationale: None,
        selector: None,
    };
    let proposed = crate::agent::suggested_ops::propose(conn, &sweep, std::slice::from_ref(&group), now)?;
    let Some(group_id) = proposed.group_ids.first().copied() else {
        return Ok(None);
    };

    let ops = page_ops(conn, group_id, MAX_RENAME_ROWS, 0)?;
    let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
    {
        let mut stmt = tx.prepare_cached(
            "INSERT INTO proposal_rename_evidence (
                op_id, source, detail,
                coverage_match_offset, coverage_matched_chars, coverage_delivered_chars,
                coverage_context_before, coverage_matched_text, coverage_context_after,
                coverage_trimmed_before, coverage_trimmed_after
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        )?;
        // The ops come back in `seq` order, which is the order they were inserted in, so a
        // row's evidence is the evidence of the draft row at the same index.
        for (op, row) in ops.iter().zip(&draft.rows) {
            let c = row.coverage.as_ref();
            stmt.execute(params![
                op.id,
                row.evidence.source.as_token(),
                row.evidence.detail,
                c.map(|c| c.match_offset as i64),
                c.map(|c| c.matched_chars as i64),
                c.map(|c| c.delivered_chars as i64),
                c.map(|c| c.context_before.as_str()),
                c.map(|c| c.matched_text.as_str()),
                c.map(|c| c.context_after.as_str()),
                c.map(|c| c.trimmed_before),
                c.map(|c| c.trimmed_after),
            ])?;
        }
    }
    tx.commit()?;

    Ok(load(conn, &group_id.to_string())?.map(|proposal| proposal.snapshot()))
}

/// Read back a staged proposal, or `None` when there is nothing to review.
///
/// `None` covers every way a review can be over: an unknown or non-numeric id, a group that
/// left `pending` (approved, rejected, or interrupted), and a group whose rows lost their
/// evidence. Callers report all of them as an expired review, because that is what they are —
/// there is nothing here the user can still decide.
pub fn load(conn: &Connection, proposal_id: &str) -> Result<Option<RenameProposal>, AgentStoreError> {
    let Some(group_id) = numeric_id(proposal_id) else {
        return Ok(None);
    };
    let Some(group) = get_group(conn, group_id)? else {
        return Ok(None);
    };
    // Only a pending group is still the user's to answer. An approved one is in flight, and
    // re-reading it as a live review is how a plan would be applied twice.
    if group.status != ProposalStatus::Pending {
        return Ok(None);
    }
    let ops = page_ops(conn, group_id, MAX_RENAME_ROWS, 0)?;
    let mut evidence = read_evidence(conn, group_id)?;
    let mut rows = Vec::with_capacity(ops.len());
    for op in ops {
        // Fails closed: no evidence row means nothing can say where this name came from, so
        // the whole proposal is unreviewable rather than partly believable.
        let Some((row_evidence, coverage)) = evidence.remove(&op.id) else {
            return Ok(None);
        };
        rows.push(RenameProposalRow {
            row_id: op.id.to_string(),
            source_path: op.source_path,
            destination_name: op.destination.unwrap_or_default(),
            evidence: row_evidence,
            coverage,
        });
    }
    Ok(Some(RenameProposal {
        proposal_id: group_id.to_string(),
        volume_id: group.source_volume_id,
        rows,
    }))
}

/// Replace one staged row's destination name with the one the user typed, and drop everything
/// that described the model's name for it: the evidence becomes the `UserEdited` marker and
/// the coverage goes (invariant 10).
///
/// The name arrives already validated — the caller ([`super::revise`]) is the boundary that
/// checks it, exactly as the tool boundary checks the model's. Only a row of a PENDING group
/// moves: an approved group is the user's answer, already given.
pub fn revise_row(
    conn: &Connection,
    proposal_id: &str,
    row_id: &str,
    destination_name: &str,
) -> Result<Option<RenameProposalRowSnapshot>, AgentStoreError> {
    let (Some(group_id), Some(op_id)) = (numeric_id(proposal_id), numeric_id(row_id)) else {
        return Ok(None);
    };
    let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
    let status: Option<String> = tx
        .prepare_cached("SELECT status FROM proposals WHERE id = ?1")?
        .query_row(params![group_id], |row| row.get(0))
        .ok();
    if status.as_deref() != Some(ProposalStatus::Pending.as_token()) {
        return Ok(None);
    }
    let updated = tx
        .prepare_cached("UPDATE proposal_ops SET destination = ?3 WHERE id = ?1 AND group_id = ?2")?
        .execute(params![op_id, group_id, destination_name])?;
    if updated == 0 {
        return Ok(None);
    }
    tx.prepare_cached(
        "UPDATE proposal_rename_evidence SET
            source = ?2, detail = '',
            coverage_match_offset = NULL, coverage_matched_chars = NULL, coverage_delivered_chars = NULL,
            coverage_context_before = NULL, coverage_matched_text = NULL, coverage_context_after = NULL,
            coverage_trimmed_before = NULL, coverage_trimmed_after = NULL
         WHERE op_id = ?1",
    )?
    .execute(params![op_id, EvidenceSource::UserEdited.as_token()])?;
    tx.commit()?;

    Ok(load(conn, proposal_id)?.and_then(|proposal| {
        proposal
            .rows
            .iter()
            .find(|row| row.row_id == row_id)
            .map(|row| row.snapshot(&proposal.volume_id))
    }))
}

/// The stored id behind an opaque id the frontend handed back — a group id for a proposal, an
/// op id for a row. A non-numeric one is simply unknown: ids are ours, so anything else was
/// never issued here.
pub fn numeric_id(id: &str) -> Option<i64> {
    id.parse::<i64>().ok()
}

/// Every op's evidence in this group, by op id.
fn read_evidence(
    conn: &Connection,
    group_id: i64,
) -> Result<HashMap<i64, (RenameEvidence, Option<EvidenceCoverage>)>, AgentStoreError> {
    let mut stmt = conn.prepare_cached(
        "SELECT e.op_id, e.source, e.detail,
                e.coverage_match_offset, e.coverage_matched_chars, e.coverage_delivered_chars,
                e.coverage_context_before, e.coverage_matched_text, e.coverage_context_after,
                e.coverage_trimmed_before, e.coverage_trimmed_after
         FROM proposal_rename_evidence e
         JOIN proposal_ops o ON o.id = e.op_id
         WHERE o.group_id = ?1",
    )?;
    let mut rows = stmt.query(params![group_id])?;
    let mut out = HashMap::new();
    while let Some(row) = rows.next()? {
        let op_id: i64 = row.get(0)?;
        let token: String = row.get(1)?;
        let source = EvidenceSource::from_token(&token).ok_or(AgentStoreError::Decode {
            column: "proposal_rename_evidence.source",
            value: token,
        })?;
        let evidence = RenameEvidence {
            source,
            detail: row.get(2)?,
        };
        // All or nothing: a coverage is written as one set of columns, and a partial one would
        // describe a match nobody measured.
        let coverage = match (
            row.get::<_, Option<i64>>(3)?,
            row.get::<_, Option<i64>>(4)?,
            row.get::<_, Option<i64>>(5)?,
            row.get::<_, Option<String>>(6)?,
            row.get::<_, Option<String>>(7)?,
            row.get::<_, Option<String>>(8)?,
            row.get::<_, Option<bool>>(9)?,
            row.get::<_, Option<bool>>(10)?,
        ) {
            (
                Some(match_offset),
                Some(matched_chars),
                Some(delivered_chars),
                Some(context_before),
                Some(matched_text),
                Some(context_after),
                Some(trimmed_before),
                Some(trimmed_after),
            ) => Some(EvidenceCoverage {
                match_offset: match_offset.max(0) as usize,
                matched_chars: matched_chars.max(0) as usize,
                delivered_chars: delivered_chars.max(0) as usize,
                context_before,
                matched_text,
                context_after,
                trimmed_before,
                trimmed_after,
            }),
            _ => None,
        };
        out.insert(op_id, (evidence, coverage));
    }
    Ok(out)
}

/// The friendly name a group leads with in a review list. English display text stored with the
/// group, as the spine takes it; the rename dialog renders its own localized copy.
fn display_name(parent: &str, count: usize) -> String {
    let folder = Path::new(parent)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(parent);
    let files = if count == 1 {
        "one file".to_string()
    } else {
        format!("{} files", spelled_count(count))
    };
    format!("Rename {files} in {folder}")
}

/// A count the way the style guide writes one: spelled out through nine, thousands separators
/// above.
fn spelled_count(count: usize) -> String {
    match count {
        1 => "one".into(),
        2 => "two".into(),
        3 => "three".into(),
        4 => "four".into(),
        5 => "five".into(),
        6 => "six".into(),
        7 => "seven".into(),
        8 => "eight".into(),
        9 => "nine".into(),
        _ => {
            let digits = count.to_string();
            let mut out = String::with_capacity(digits.len() + digits.len() / 3);
            for (index, digit) in digits.chars().enumerate() {
                if index > 0 && (digits.len() - index).is_multiple_of(3) {
                    out.push(',');
                }
                out.push(digit);
            }
            out
        }
    }
}

/// The ops of a group, for callers that need the whole live set (apply, and preflight's
/// deselection arithmetic). Rename groups are capped at [`MAX_RENAME_ROWS`], so this is a
/// bounded read.
pub fn group_ops(conn: &Connection, group_id: i64) -> Result<Vec<ProposalOp>, AgentStoreError> {
    page_ops(conn, group_id, MAX_RENAME_ROWS, 0)
}
