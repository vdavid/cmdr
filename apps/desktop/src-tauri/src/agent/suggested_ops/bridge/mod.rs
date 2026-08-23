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

use super::super::memory::MemoryStore;
use super::super::store::AgentStoreError;
use super::super::store::proposals::{
    AcceptanceOutcome, ClaimOutcome, ClaimRefusal, ProposalGroup, ProposalOp, get_group, page_ops, record_acceptance,
};
use super::super::types::{OpStatus, ProposalVerb};
use crate::file_system::volume::Volume;
use crate::file_system::write_operations::{
    BulkRenameRow, ExpectedSources, OperationEventSink, SourceFingerprint, VolumeCopyConfig, WriteOperationConfig,
    WriteOperationError, WriteOperationStartResult, delete_files_start, resolve_source_volume, start_bulk_rename,
    start_volume_compress, start_volume_copy, start_volume_move, trash_files_start,
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
    /// The volume the group's sources live on is no longer registered: a drive ejected or a
    /// share went away between the proposal and the review. Nothing was claimed.
    SourceVolumeGone { volume_id: String },
    /// The write engine refused to start the operation.
    EngineRefused { detail: String },
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
#[allow(
    clippy::too_many_arguments,
    reason = "the approval hand-off; every one of these outlives the caller and has to be moved in"
)]
pub async fn approve_and_execute(
    conn: &Connection,
    reporting_conn: Connection,
    events: Arc<dyn OperationEventSink>,
    group_id: i64,
    deselected_op_ids: &[i64],
    now: i64,
    memory: Option<MemoryStore>,
) -> Result<ApprovalOutcome, AgentStoreError> {
    match record_acceptance(conn, group_id, deselected_op_ids, now)? {
        AcceptanceOutcome::Accepted { .. } => {}
        other => return Ok(ApprovalOutcome::Refused(ApprovalRefusal::NotAccepted(other))),
    }

    // Read the accepted set and fingerprint it BEFORE the claim, so the binding describes the
    // files as they were while the user was deciding. The claim that follows is the last gate
    // before the engine.
    let ops = live_ops(conn, group_id)?;
    let op_ids: HashMap<PathBuf, i64> = ops.iter().map(|op| (PathBuf::from(&op.source_path), op.id)).collect();
    let sources: Vec<PathBuf> = ops.iter().map(|op| PathBuf::from(&op.source_path)).collect();

    let volume_id = source_volume_of(conn, group_id)?;
    let Some((source_volume, _)) = resolve_source_volume(&volume_id, sources.first()).await else {
        return Ok(ApprovalOutcome::Refused(ApprovalRefusal::SourceVolumeGone {
            volume_id,
        }));
    };
    let expected = capture_expected_sources(source_volume.as_ref(), &sources).await;

    // Through the service layer, so the approval metric is reported in the one place that
    // owns it rather than a second time here.
    let group = match super::approve(conn, group_id, now)? {
        ClaimOutcome::Claimed(claimed) => claimed.group,
        ClaimOutcome::Refused(refusal) => return Ok(ApprovalOutcome::Refused(ApprovalRefusal::Claim(refusal))),
    };

    let sink: Arc<dyn OperationEventSink> = Arc::new(ProposalReportingSink::new(
        events,
        group_id,
        op_ids,
        reporting_conn,
        memory,
    ));

    match start_for(&group, sources, &ops, expected, sink).await {
        Ok(operation) => Ok(ApprovalOutcome::Started(ApprovedGroup { group_id, operation })),
        Err(refusal) => Ok(ApprovalOutcome::Refused(refusal)),
    }
}

/// The volume every source in the group lives on. A sweep may span volumes; a group may not,
/// which is what lets one capture rule cover the whole batch.
fn source_volume_of(conn: &Connection, group_id: i64) -> Result<String, AgentStoreError> {
    Ok(get_group(conn, group_id)?
        .map(|group| group.source_volume_id)
        .unwrap_or_default())
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
    ops: &[ProposalOp],
    expected: ExpectedSources,
    events: Arc<dyn OperationEventSink>,
) -> Result<WriteOperationStartResult, ApprovalRefusal> {
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
                Some(expected),
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
                Some(expected),
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
            Some(expected),
        )
        .await
        .map_err(ApprovalRefusal::Engine),
        ProposalVerb::Delete => delete_files_start(
            events,
            sources,
            WriteOperationConfig::default(),
            Some(group.source_volume_id.clone()),
            Initiator::Agent,
            Some(expected),
        )
        .await
        .map_err(ApprovalRefusal::Engine),
        // Rename is the one verb whose executor takes per-row destinations and a fingerprint
        // per row, which is exactly what the live preflight capture produces. The group binds
        // the shared PARENT, and each op carries the NAME it becomes.
        ProposalVerb::Rename => {
            let (parent, _) = target_of(group)?;
            let rows = rename_rows(&parent, ops, &expected)?;
            start_bulk_rename(events, group.source_volume_id.clone(), rows, Initiator::Agent)
                .map_err(|detail| ApprovalRefusal::EngineRefused { detail })
        }
    }
}

/// The rows `start_bulk_rename` takes, built from the group's ops and the live preflight.
///
/// The stored `destination` is a NAME, not a path, because the executor refuses a row whose
/// source and destination parents differ — so the group binds the shared parent and this
/// rejoins the two. A row whose source went unfingerprinted at preflight is dropped, which is
/// the same answer the binding gives every other verb: an unreadable source is not the source
/// anybody reviewed.
fn rename_rows(
    parent: &str,
    ops: &[ProposalOp],
    expected: &ExpectedSources,
) -> Result<Vec<BulkRenameRow>, ApprovalRefusal> {
    let mut rows = Vec::with_capacity(ops.len());
    for op in ops {
        let Some(new_name) = op.destination.as_deref() else {
            return Err(ApprovalRefusal::TargetMissing {
                verb: ProposalVerb::Rename,
            });
        };
        let source = PathBuf::from(&op.source_path);
        let Some(fingerprint) = expected.fingerprint_of(&source) else {
            continue;
        };
        rows.push(BulkRenameRow {
            row_id: op.id.to_string(),
            source,
            destination: PathBuf::from(parent).join(new_name),
            expected_fingerprint: fingerprint.clone(),
        });
    }
    Ok(rows)
}

/// Stat every source as it is RIGHT NOW, and bind the operation to what it finds.
///
/// **This is a live capture, not the stored snapshot, and the two answer different
/// questions.** The creation snapshot on `proposal_ops` came from the drive index when the
/// agent proposed the group: second-precision, often absent, and its job is the review-time
/// question "has this changed since the agent looked at it?" — a stale BELIEF the dialog
/// surfaces so the user can re-judge. An execution binding answers the other question, "has
/// this changed since I showed it to you?", which is a RACE: the window is the review plus
/// however long the operation then waits for its lane, and catching it needs the full
/// nanosecond precision a live stat gives.
///
/// So this stats now, the way the rename preflight does, and the fingerprints never reach the
/// database. A restart must force a fresh preflight rather than resurrect one, because a
/// fingerprint describes a file as it was at review time and nothing else.
///
/// A source that can't be read gets no entry, and the binding drops what it doesn't name, so
/// a source that vanished between the proposal and the review is skipped and reported rather
/// than acted on.
async fn capture_expected_sources(volume: &dyn Volume, sources: &[PathBuf]) -> ExpectedSources {
    // The rule the binding itself documents: a local-FS volume answers with the identity the
    // kernel maintains, anything else answers through its own backend.
    let local = volume.local_path().is_some();
    let mut entries = Vec::with_capacity(sources.len());
    for source in sources {
        let captured = if local {
            SourceFingerprint::capture_local(source)
        } else {
            SourceFingerprint::capture_remote(volume, source).await
        };
        match captured {
            Some(fingerprint) => entries.push((source.clone(), fingerprint)),
            None => log::info!(
                target: "agent::suggested_ops",
                "{} couldn't be read at preflight, so the operation won't touch it",
                source.display()
            ),
        }
    }
    ExpectedSources::new(entries)
}

/// The target a destination-binding verb stored, as the routed entry points take it.
fn target_of(group: &ProposalGroup) -> Result<(String, String), ApprovalRefusal> {
    match (group.destination.clone(), group.destination_volume_id.clone()) {
        (Some(path), Some(volume)) => Ok((path, volume)),
        _ => Err(ApprovalRefusal::TargetMissing { verb: group.verb }),
    }
}
