//! Creating sweeps and groups, and re-proposing a pending one.
//!
//! Creation is where a proposal FREEZES: whatever produced the op list (an explicit path
//! list or a resolved selector) hands over concrete rows here, and nothing re-derives them
//! later. Re-propose is the one way an op list changes afterwards, it belongs to the group's
//! author, and it works only while the group is `pending`.

use rusqlite::{Connection, Transaction, TransactionBehavior, params};

use super::super::AgentStoreError;
use super::GroupIntent;
use crate::agent::types::{OpStatus, ProposalStatus};

/// A new sweep: one agent wake's output, before its groups exist.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NewSweep {
    /// The chat thread this came out of, when it came out of one. A background wake has
    /// none, and deleting the thread later NULLs this rather than deleting the sweep.
    pub conversation_id: Option<i64>,
    /// Which model produced it. Provenance only: no logic reads it (agent-spec D32).
    pub created_by_model: Option<String>,
    /// The agent's words for the sweep as a whole.
    pub rationale: Option<String>,
}

/// One proposed op under a verb whose group binds the target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewOp {
    pub source_path: String,
    /// What the source looked like when the proposal was created, when the producer knew.
    /// `None` is normal (an explicit path list carries no index row behind it).
    pub snapshot: Option<OpSnapshot>,
}

/// One proposed rename. Rename is the ONE verb whose ops carry their own destinations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewRename {
    pub source_path: String,
    /// The name the source becomes. A NAME, not a path: `start_bulk_rename` refuses a row
    /// that would change the parent.
    pub new_name: String,
    pub snapshot: Option<OpSnapshot>,
}

/// What a source looked like at creation, for the executor's drift check at apply. Every
/// field is optional because the drive index doesn't always hold all three.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct OpSnapshot {
    pub size: Option<u64>,
    /// Modification time, unix seconds.
    pub mtime: Option<i64>,
    pub inode: Option<u64>,
}

/// A group to create (or to re-propose an existing one into).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewGroup {
    /// The verb, its target, and its ops, in the one shape they're valid in.
    pub intent: GroupIntent,
    /// The volume every source lives on. A sweep may span volumes; a group may not.
    pub source_volume_id: String,
    /// The friendly name the review dialog leads with. Carries the selector's pattern as
    /// display text when a selector produced the group.
    pub display_name: String,
    /// The agent's reason, shown LABELLED as the agent's words next to facts Cmdr knows.
    pub rationale: Option<String>,
    /// The selector this group froze, as JSON, for display and provenance.
    pub selector: Option<String>,
}

/// Insert a sweep and return its id.
pub fn create_sweep(conn: &Connection, sweep: &NewSweep, now: i64) -> Result<i64, AgentStoreError> {
    conn.prepare_cached(
        "INSERT INTO proposal_sets (conversation_id, created_at, created_by_model, rationale)
         VALUES (?1, ?2, ?3, ?4)",
    )?
    .execute(params![
        sweep.conversation_id,
        now,
        sweep.created_by_model,
        sweep.rationale
    ])?;
    Ok(conn.last_insert_rowid())
}

/// Insert a group and all of its ops into `set_id`, in one transaction, and return the group
/// id. The group's `seq` is the next ordinal in its sweep.
///
/// The op rows go in with `prepare_cached` inside the same transaction, so a 60 000-op group
/// is one commit and one compiled statement rather than 60 000 of either.
pub fn create_group(conn: &Connection, set_id: i64, group: &NewGroup, now: i64) -> Result<i64, AgentStoreError> {
    let tx = conn.unchecked_transaction()?;
    let seq: i64 = tx
        .prepare_cached("SELECT COALESCE(MAX(seq) + 1, 0) FROM proposals WHERE set_id = ?1")?
        .query_row(params![set_id], |row| row.get(0))?;
    let (destination, destination_volume_id) = group.intent.stored_destination();
    tx.prepare_cached(
        "INSERT INTO proposals (
            set_id, seq, verb, status, source_volume_id, destination, destination_volume_id,
            reversible, display_name, rationale, selector, created_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
    )?
    .execute(params![
        set_id,
        seq,
        group.intent.verb().as_token(),
        ProposalStatus::Pending.as_token(),
        group.source_volume_id,
        destination,
        destination_volume_id,
        group.intent.reversibility().as_token(),
        group.display_name,
        group.rationale,
        group.selector,
        now,
    ])?;
    let group_id = tx.last_insert_rowid();
    insert_ops(&tx, group_id, &group.intent, now)?;
    tx.commit()?;
    Ok(group_id)
}

/// Write a group's op rows, numbered from 0.
fn insert_ops(conn: &Connection, group_id: i64, intent: &GroupIntent, now: i64) -> Result<(), AgentStoreError> {
    let mut stmt = conn.prepare_cached(
        "INSERT INTO proposal_ops (
            group_id, seq, source_path, destination, status, snapshot_size, snapshot_mtime, snapshot_inode, created_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
    )?;
    for (seq, (source_path, destination, snapshot)) in intent.rows().enumerate() {
        stmt.execute(params![
            group_id,
            seq as i64,
            source_path,
            destination,
            OpStatus::Pending.as_token(),
            snapshot.and_then(|s| s.size),
            snapshot.and_then(|s| s.mtime),
            snapshot.and_then(|s| s.inode),
            now,
        ])?;
    }
    Ok(())
}

/// What a re-propose did, or why it didn't.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReproposeOutcome {
    /// The group's ops, target, and text were replaced, and any acceptance record was torn
    /// up (so the next approval goes through a fresh preflight).
    Reproposed,
    /// The group isn't `pending`, so it's frozen to its author. `Approved`, `Interrupted`,
    /// `Completed`, and `Rejected` groups are the USER's; an agent that could still rewrite
    /// them could rewrite what the user already answered.
    NotPending { found: ProposalStatus },
    /// No group with that id.
    Unknown,
}

/// Replace a PENDING group's op list and text with `group`'s. The one way an op list changes
/// after creation, and the author's own path — the agent re-proposing against a sweep it owns.
///
/// Guarded twice over, both inside one `BEGIN IMMEDIATE`: the conditional `UPDATE ... WHERE
/// status = 'pending'` both checks and locks, and the acceptance record is DELETED, so a
/// preflight taken against the old op list can never carry an approval onto the new one.
pub fn repropose_group(
    conn: &Connection,
    group_id: i64,
    group: &NewGroup,
    now: i64,
) -> Result<ReproposeOutcome, AgentStoreError> {
    let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
    let (destination, destination_volume_id) = group.intent.stored_destination();
    let updated = tx
        .prepare_cached(
            "UPDATE proposals SET
                verb = ?2, source_volume_id = ?3, destination = ?4, destination_volume_id = ?5,
                reversible = ?6, display_name = ?7, rationale = ?8, selector = ?9, created_at = ?10
             WHERE id = ?1 AND status = ?11",
        )?
        .execute(params![
            group_id,
            group.intent.verb().as_token(),
            group.source_volume_id,
            destination,
            destination_volume_id,
            group.intent.reversibility().as_token(),
            group.display_name,
            group.rationale,
            group.selector,
            now,
            ProposalStatus::Pending.as_token(),
        ])?;

    if updated == 0 {
        // Nothing moved: either the group is gone or it left `pending`. Read which, so the
        // caller gets the typed reason rather than a bare "no".
        let found: Option<String> = tx
            .prepare_cached("SELECT status FROM proposals WHERE id = ?1")?
            .query_row(params![group_id], |row| row.get(0))
            .ok();
        tx.commit()?;
        return match found {
            Some(token) => Ok(ReproposeOutcome::NotPending {
                found: super::decode_token("proposals.status", token, ProposalStatus::from_token)?,
            }),
            None => Ok(ReproposeOutcome::Unknown),
        };
    }

    tx.prepare_cached("DELETE FROM proposal_ops WHERE group_id = ?1")?
        .execute(params![group_id])?;
    tx.prepare_cached("DELETE FROM proposal_acceptances WHERE group_id = ?1")?
        .execute(params![group_id])?;
    insert_ops(&tx, group_id, &group.intent, now)?;
    tx.commit()?;
    Ok(ReproposeOutcome::Reproposed)
}
