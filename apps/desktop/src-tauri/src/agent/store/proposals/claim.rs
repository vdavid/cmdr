//! Preflight, the claim transaction, and rejection.

use rusqlite::{Connection, Transaction, TransactionBehavior, params};
use sha2::{Digest, Sha256};

use super::super::AgentStoreError;
use super::read::get_group;
use super::{ProposalGroup, decode_token};
use crate::agent::types::{OpStatus, ProposalStatus};

/// What a group's live op set is, in constant memory: how many ops it holds and a hash of
/// the values they carry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpBinding {
    pub op_count: u64,
    pub digest: String,
}

/// What preflight did, or why it didn't.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AcceptanceOutcome {
    Accepted { binding: OpBinding },
    NotPending { found: ProposalStatus },
    Unknown,
}

/// A group the claim transaction moved into `approved`, with the binding it ran against.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaimedGroup {
    pub group: ProposalGroup,
    pub binding: OpBinding,
}

/// Why a claim didn't happen. Two refusals, because the user-facing recovery differs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClaimRefusal {
    /// The group already left `pending` — somebody else approved it, or it was rejected.
    StaleStatus { found: ProposalStatus },
    /// The live op set isn't what preflight accepted (or nothing was ever accepted).
    BindingMismatch {
        accepted: Option<OpBinding>,
        live: OpBinding,
    },
    /// No group with that id.
    Unknown,
}

/// The result of a claim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClaimOutcome {
    Claimed(ClaimedGroup),
    Refused(ClaimRefusal),
}

/// The result of a rejection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RejectOutcome {
    Rejected,
    NotPending { found: ProposalStatus },
    Unknown,
}

/// Record what the user approved, as a server-owned acceptance record.
///
/// The client presents a group id and the op ids it DESELECTED — never values, and never the
/// 60 000 ids it kept. Preflight turns that into the live op set (deselected ops become
/// `excluded` rows, previously excluded ops come back) and stores a hash plus count of what
/// that set carries. From here on, that record is the only thing the claim trusts about what
/// the user saw.
pub fn record_acceptance(
    conn: &Connection,
    group_id: i64,
    deselected_op_ids: &[i64],
    now: i64,
) -> Result<AcceptanceOutcome, AgentStoreError> {
    let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
    match read_status(&tx, group_id)? {
        None => return Ok(AcceptanceOutcome::Unknown),
        Some(ProposalStatus::Pending) => {}
        Some(found) => return Ok(AcceptanceOutcome::NotPending { found }),
    }

    // Start from what the group offers: an op excluded by an earlier review comes back unless
    // this review deselects it again. Only `excluded` rows are touched, so an op that already
    // ran keeps its outcome.
    tx.prepare_cached("UPDATE proposal_ops SET status = ?2 WHERE group_id = ?1 AND status = ?3")?
        .execute(params![
            group_id,
            OpStatus::Pending.as_token(),
            OpStatus::Excluded.as_token()
        ])?;
    let mut exclude = tx.prepare_cached(
        "UPDATE proposal_ops SET status = ?3 WHERE group_id = ?1 AND id = ?2 AND status = ?4",
    )?;
    for op_id in deselected_op_ids {
        exclude.execute(params![
            group_id,
            op_id,
            OpStatus::Excluded.as_token(),
            OpStatus::Pending.as_token()
        ])?;
    }
    drop(exclude);

    let binding = live_binding(&tx, group_id)?;
    tx.prepare_cached(
        "INSERT INTO proposal_acceptances (group_id, op_count, op_digest, created_at)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(group_id) DO UPDATE SET
            op_count = excluded.op_count, op_digest = excluded.op_digest, created_at = excluded.created_at",
    )?
    .execute(params![group_id, binding.op_count, binding.digest, now])?;
    tx.commit()?;
    Ok(AcceptanceOutcome::Accepted { binding })
}

/// Claim a group for execution: the one transition that hands ops to the write engine, and
/// the one place a bug would apply ops to real files twice.
///
/// ONE `BEGIN IMMEDIATE`, in this order:
///
/// 1. read the **stored** acceptance record (server-owned; the client presented ids only),
/// 2. re-read the live op set and compare it as a hash plus count, so this is O(1) in memory
///    at any group size,
/// 3. `UPDATE ... WHERE id = ? AND status = 'pending'`,
/// 4. refuse on a binding mismatch **or** `rows_affected == 0`, as two distinct typed
///    variants — the user-facing recovery differs ("review it again" versus "somebody already
///    answered this").
///
/// ❌ The op statuses are deliberately NOT touched here. Leaving the live set alone is what
/// makes a losing concurrent claim report the honest reason (stale status) instead of a
/// binding mismatch caused by the winner.
pub fn claim_group_for_execution(
    conn: &Connection,
    group_id: i64,
    now: i64,
) -> Result<ClaimOutcome, AgentStoreError> {
    let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;

    let accepted = read_acceptance(&tx, group_id)?;
    let live = live_binding(&tx, group_id)?;
    if accepted.as_ref() != Some(&live) {
        // No record at all can also mean the group is gone; say so rather than reporting a
        // mismatch against an op set that doesn't exist.
        if accepted.is_none() && read_status(&tx, group_id)?.is_none() {
            return Ok(ClaimOutcome::Refused(ClaimRefusal::Unknown));
        }
        return Ok(ClaimOutcome::Refused(ClaimRefusal::BindingMismatch { accepted, live }));
    }

    let updated = tx
        .prepare_cached("UPDATE proposals SET status = ?3, decided_at = ?4 WHERE id = ?1 AND status = ?2")?
        .execute(params![
            group_id,
            ProposalStatus::Pending.as_token(),
            ProposalStatus::Approved.as_token(),
            now
        ])?;
    if updated == 0 {
        let refusal = match read_status(&tx, group_id)? {
            Some(found) => ClaimRefusal::StaleStatus { found },
            None => ClaimRefusal::Unknown,
        };
        return Ok(ClaimOutcome::Refused(refusal));
    }

    let group = get_group(&tx, group_id)?.ok_or(rusqlite::Error::QueryReturnedNoRows)?;
    tx.commit()?;
    Ok(ClaimOutcome::Claimed(ClaimedGroup { group, binding: live }))
}

/// Reject a group. The same conditional shape as the claim: only a `pending` group moves, so
/// a rejection can never overwrite an answer already given.
pub fn reject_group(conn: &Connection, group_id: i64, now: i64) -> Result<RejectOutcome, AgentStoreError> {
    let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
    let updated = tx
        .prepare_cached("UPDATE proposals SET status = ?3, decided_at = ?4 WHERE id = ?1 AND status = ?2")?
        .execute(params![
            group_id,
            ProposalStatus::Pending.as_token(),
            ProposalStatus::Rejected.as_token(),
            now
        ])?;
    if updated == 0 {
        let outcome = match read_status(&tx, group_id)? {
            Some(found) => RejectOutcome::NotPending { found },
            None => RejectOutcome::Unknown,
        };
        return Ok(outcome);
    }
    tx.commit()?;
    Ok(RejectOutcome::Rejected)
}

/// The group's live op set as a hash plus a count, computed by STREAMING the rows.
///
/// One row is in memory at a time, so this is O(1) in memory at any group size: a 60 000-op
/// group compares as cheaply as a three-op one, which is what lets the spine carry a group
/// that large without a cap. ❌ Never rewrite this to load the ops and hash the list.
///
/// Each field is length-prefixed before it's hashed, so no two different op sets can
/// concatenate to the same bytes.
pub fn live_binding(conn: &Connection, group_id: i64) -> Result<OpBinding, AgentStoreError> {
    let mut stmt = conn.prepare_cached(
        "SELECT id, source_path, destination FROM proposal_ops
         WHERE group_id = ?1 AND status = ?2 ORDER BY seq",
    )?;
    let mut rows = stmt.query(params![group_id, OpStatus::Pending.as_token()])?;
    let mut hasher = Sha256::new();
    let mut op_count: u64 = 0;
    while let Some(row) = rows.next()? {
        let id: i64 = row.get(0)?;
        let source_path: String = row.get(1)?;
        let destination: Option<String> = row.get(2)?;
        hasher.update(id.to_le_bytes());
        hash_field(&mut hasher, Some(&source_path));
        hash_field(&mut hasher, destination.as_deref());
        op_count += 1;
    }
    let digest = Sha256::finalize(hasher).iter().map(|b| format!("{b:02x}")).collect();
    Ok(OpBinding { op_count, digest })
}

/// Feed one optional field into the hash, length-prefixed and presence-tagged.
fn hash_field(hasher: &mut Sha256, value: Option<&str>) {
    match value {
        Some(text) => {
            hasher.update([1u8]);
            hasher.update((text.len() as u64).to_le_bytes());
            hasher.update(text.as_bytes());
        }
        None => hasher.update([0u8]),
    }
}

/// The group's stored status, or `None` when there's no such group.
fn read_status(conn: &Connection, group_id: i64) -> Result<Option<ProposalStatus>, AgentStoreError> {
    let mut stmt = conn.prepare_cached("SELECT status FROM proposals WHERE id = ?1")?;
    let mut rows = stmt.query(params![group_id])?;
    match rows.next()? {
        Some(row) => Ok(Some(decode_token(
            "proposals.status",
            row.get(0)?,
            ProposalStatus::from_token,
        )?)),
        None => Ok(None),
    }
}

/// The acceptance record preflight stored for this group, if any.
fn read_acceptance(conn: &Connection, group_id: i64) -> Result<Option<OpBinding>, AgentStoreError> {
    let mut stmt =
        conn.prepare_cached("SELECT op_count, op_digest FROM proposal_acceptances WHERE group_id = ?1")?;
    let mut rows = stmt.query(params![group_id])?;
    match rows.next()? {
        Some(row) => Ok(Some(OpBinding {
            op_count: row.get(0)?,
            digest: row.get(1)?,
        })),
        None => Ok(None),
    }
}
