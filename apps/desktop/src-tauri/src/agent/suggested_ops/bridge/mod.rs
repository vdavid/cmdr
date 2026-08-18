//! The approval bridge: from a claimed group to a running operation.
//!
//! Approval is one hand-off, and this module is all of it. It claims the group through the
//! store transaction, turns the stored row plus its live ops into the executor call the verb
//! describes, and injects a sink wrapped so the ops report their own outcomes back
//! (`decorator.rs`).
//!
//! ## What this deliberately does not do
//!
//! It builds an ORDINARY executor call. No conflict policy of its own, no auto-skip, no
//! refusal to create a destination folder or to overwrite: once the user approves, it is
//! exactly as if they started the action, because they did. Everything this module adds is
//! bookkeeping about a proposal, none of it reaches the filesystem.
//!
//! The write engine, correspondingly, knows nothing about any of this. It reports per-source
//! outcomes through the sink every operation already emits through, and `write-ops-isolation`
//! fails the build if the engine ever names `agent::`.

mod decorator;

#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use rusqlite::Connection;

use decorator::ProposalReportingSink;

use super::super::store::AgentStoreError;
use super::super::store::proposals::{
    AcceptanceOutcome, ClaimOutcome, ClaimRefusal, ProposalGroup, ProposalOp, page_ops, record_acceptance,
};
use super::super::types::{OpStatus, ProposalVerb};
use crate::file_system::write_operations::{
    OperationEventSink, VolumeCopyConfig, WriteOperationConfig, WriteOperationError, WriteOperationStartResult,
    delete_files_start, start_volume_compress, start_volume_copy, start_volume_move, trash_files_start,
};
use crate::operation_log::types::Initiator;

/// A group whose operation is running, and the operation it became.
#[derive(Debug, Clone)]
pub struct ApprovedGroup {
    pub group_id: i64,
    pub operation: WriteOperationStartResult,
}

/// Why an approval did not become a running operation.
#[derive(Debug, Clone)]
pub enum ApprovalRefusal {
    /// Preflight would not accept: the group had already left `pending`, or is unknown.
    NotAccepted(AcceptanceOutcome),
    /// The claim transaction refused. Its two variants mean different recoveries.
    Claim(ClaimRefusal),
    /// The verb has no executor route on this spine yet. Rename is the one: its executor
    /// takes rows carrying a server-owned fingerprint per source, which the spine cannot
    /// build from a frozen snapshot. It gets its route when the shipped rename feature moves
    /// onto the spine (plan M6).
    NoRouteYet { verb: ProposalVerb },
    /// The stored row lacks the target its verb binds. Unreachable through `GroupIntent`,
    /// which pairs each verb with its target at construction; reported rather than panicked
    /// because the row is read back from SQLite, where a hand-edit could produce it.
    TargetMissing { verb: ProposalVerb },
    /// The group claimed cleanly and the write engine refused to start.
    Engine(WriteOperationError),
}

/// What approving a group did.
#[derive(Debug, Clone)]
pub enum ApprovalOutcome {
    Started(ApprovedGroup),
    Refused(ApprovalRefusal),
}

/// How many op rows one read pulls back. A group of 60 000 is legitimate, and the executor
/// needs every path, so this is the one place the spine does materialize them: paging keeps
/// the peak at a page rather than the whole group plus the whole group again.
const OP_PAGE: u32 = 1_000;

/// Approve a group and start the operation it describes.
///
/// The whole hand-off, in order: preflight records what the user accepted, the claim
/// transaction binds against it and moves the group to `approved`, the live ops become an
/// ordinary executor call, and the sink goes in wrapped so each source reports its outcome
/// back into the spine.
///
/// **Two connections, deliberately.** The claim and the reads run on the caller connection;
/// `reporting_conn` is MOVED into the decorator because the operation outlives this call by
/// minutes or hours, so its writer cannot borrow anything the caller owns.
///
/// ❌ Nothing here adds an execution behaviour. The config is the default one a user-started
/// operation gets, the destination is created if missing exactly as it would be, and a
/// conflict is answered exactly as it would be. Approval transfers responsibility; it does
/// not change what runs.
pub async fn approve_and_execute(
    conn: &Connection,
    reporting_conn: Connection,
    events: Arc<dyn OperationEventSink>,
    group_id: i64,
    deselected_op_ids: &[i64],
    now: i64,
) -> Result<ApprovalOutcome, AgentStoreError> {
    match record_acceptance(conn, group_id, deselected_op_ids, now)? {
        AcceptanceOutcome::Accepted { .. } => {}
        other => return Ok(ApprovalOutcome::Refused(ApprovalRefusal::NotAccepted(other))),
    }

    // Through the service layer, so the approval metric is reported in the one place that
    // owns it rather than a second time here.
    let group = match super::approve(conn, group_id, now)? {
        ClaimOutcome::Claimed(claimed) => claimed.group,
        ClaimOutcome::Refused(refusal) => return Ok(ApprovalOutcome::Refused(ApprovalRefusal::Claim(refusal))),
    };

    let live = live_ops(conn, group_id)?;
    let op_ids: HashMap<PathBuf, i64> = live.iter().map(|op| (PathBuf::from(&op.source_path), op.id)).collect();
    let sources: Vec<PathBuf> = live.into_iter().map(|op| PathBuf::from(op.source_path)).collect();

    let sink: Arc<dyn OperationEventSink> =
        Arc::new(ProposalReportingSink::new(events, group_id, op_ids, reporting_conn));

    match start_for(&group, sources, sink).await {
        Ok(operation) => Ok(ApprovalOutcome::Started(ApprovedGroup { group_id, operation })),
        Err(refusal) => Ok(ApprovalOutcome::Refused(refusal)),
    }
}

/// Every op in the group live set, paged.
fn live_ops(conn: &Connection, group_id: i64) -> Result<Vec<ProposalOp>, AgentStoreError> {
    let mut out = Vec::new();
    let mut offset = 0u32;
    loop {
        let page = page_ops(conn, group_id, OP_PAGE, offset)?;
        let read = page.len() as u32;
        out.extend(page.into_iter().filter(|op| op.status == OpStatus::Pending));
        if read < OP_PAGE {
            return Ok(out);
        }
        offset += read;
    }
}

/// Turn a claimed group into the executor call its verb describes.
///
/// Every cross-volume verb goes through the ROUTED entry points, the same ones the transfer
/// commands use, so an approved transfer resolves its volumes and its destination path the
/// way a clicked one does. Extract needs no arm of its own: its sources resolve to an
/// `ArchiveVolume` inside that routing, which is the whole reason extract has no operation
/// type.
async fn start_for(
    group: &ProposalGroup,
    sources: Vec<PathBuf>,
    events: Arc<dyn OperationEventSink>,
) -> Result<WriteOperationStartResult, ApprovalRefusal> {
    let source_paths: Vec<String> = sources.iter().map(|p| p.to_string_lossy().into_owned()).collect();
    match group.verb {
        ProposalVerb::Move => {
            let (path, volume) = target_of(group)?;
            start_volume_move(
                events,
                group.source_volume_id.clone(),
                sources,
                volume,
                path,
                VolumeCopyConfig::default(),
                Initiator::Agent,
                None,
            )
            .await
            .map_err(ApprovalRefusal::Engine)
        }
        ProposalVerb::Copy | ProposalVerb::Extract => {
            let (path, volume) = target_of(group)?;
            start_volume_copy(
                events,
                group.source_volume_id.clone(),
                sources,
                volume,
                path,
                VolumeCopyConfig::default(),
                Initiator::Agent,
                None,
            )
            .await
            .map_err(ApprovalRefusal::Engine)
        }
        ProposalVerb::Compress => {
            let (path, volume) = target_of(group)?;
            start_volume_compress(
                events,
                group.source_volume_id.clone(),
                sources,
                volume,
                path,
                VolumeCopyConfig::default(),
                Initiator::Agent,
            )
            .await
            .map_err(ApprovalRefusal::Engine)
        }
        ProposalVerb::Trash => trash_files_start(
            events,
            sources,
            None,
            WriteOperationConfig::default(),
            Initiator::Agent,
            None,
        )
        .await
        .map_err(ApprovalRefusal::Engine),
        ProposalVerb::Delete => delete_files_start(
            events,
            sources,
            WriteOperationConfig::default(),
            Some(group.source_volume_id.clone()),
            Initiator::Agent,
            None,
        )
        .await
        .map_err(ApprovalRefusal::Engine),
        ProposalVerb::Rename => {
            let _ = source_paths;
            Err(ApprovalRefusal::NoRouteYet { verb: group.verb })
        }
    }
}

/// The target a destination-binding verb stored, as the routed entry points take it.
fn target_of(group: &ProposalGroup) -> Result<(String, String), ApprovalRefusal> {
    match (group.destination.clone(), group.destination_volume_id.clone()) {
        (Some(path), Some(volume)) => Ok((path, volume)),
        _ => Err(ApprovalRefusal::TargetMissing { verb: group.verb }),
    }
}
