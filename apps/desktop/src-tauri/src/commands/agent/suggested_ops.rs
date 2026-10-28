//! What the Suggested ops dialog reads, and the one decision it can record on its own.
//!
//! The dialog's whole job is disclosure: the user decides, so everything they need to judge a
//! suggestion has to reach them in terms they can check. That shapes these views more than
//! anything else here.
//!
//! - **A group's `rationale` is the AGENT's words** and the UI labels it as such. Beside it go
//!   facts Cmdr knows by itself, which is what makes a hallucinated claim visible.
//! - **Per-op numbers come in two kinds, never mixed.** The page carries the CREATION SNAPSHOT
//!   (what the index, or one stat, held when the group froze; the fields say `snapshot`), and
//!   `suggested_ops_live_facts` answers what the file is NOW, a window at a time. The dialog
//!   shows both when they differ.
//! - **What a group would run as is read NOW from its source volume** (`runsAs`): a trash group
//!   on a volume with no Trash is offered, and approved, as a permanent delete
//!   (`agent/suggested_ops/facts.rs`).
//! - **Reversibility and "the target folder will be created" are DISCLOSED, never blocking.**
//!   Once the user approves, it is exactly as if they started the action.
//!
//! Ops are read a PAGE at a time and counted with `COUNT(*)`: a group of 60 000 is legitimate,
//! so nothing here loads a group to describe it.

use std::path::Path;
use std::time::Duration;

use cmdr_fs::deadline::{io_budget, timeout_detached_typed};
use cmdr_fs::log_error::LogError;
use serde::Serialize;
use tauri::AppHandle;

use super::{now_secs, with_read_connection};
use crate::agent::store::proposals::{AcceptanceOutcome, ClaimRefusal, OpSelection};
use crate::agent::store::proposals::{
    GroupSummary, ProposalGroup, ProposalOp, ProposalSweep, count_ops, get_sweep, list_groups, page_ops,
};
use crate::agent::suggested_ops::bridge::{ApprovalOutcome, ApprovalRefusal};
use crate::agent::suggested_ops::facts::{RunsAs, runs_as};
use crate::agent::suggested_ops::server::{SavedServers, ServerProposalView, server_card};
use crate::agent::types::{OpStatus, ProposalStatus, ProposalVerb, Reversibility};
use crate::file_system::volume::{Volume, VolumeError};
use cmdr_operations::{RenameStartError, WriteOperationError};
/// Why a suggested-ops read or answer didn't happen.
///
/// ❌ Not prose: the review dialog shows its own translated notice
/// (`suggestedOps.loadFailed`) and the badge quietly stays at zero; the variant
/// is what the log records.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum SuggestedOpsError {
    /// The suggestion store isn't open: agent features are off, or the app is
    /// still starting.
    StoreNotOpen,
    /// The store refused the read or the write.
    Store {
        /// What SQLite reported, for the log.
        detail: String,
    },
    /// The approval ran on its own thread and never answered, so what happened
    /// is genuinely unknown; the dialog re-reads rather than guessing.
    ApprovalDidntFinish,
    /// Nothing above classifies it (a panicked task).
    Unexpected {
        /// What the runtime reported, for the log.
        detail: String,
    },
}

impl std::fmt::Display for SuggestedOpsError {
    /// ❗ For logs and debugging only.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StoreNotOpen => f.write_str("the suggestion store isn't open"),
            Self::Store { detail } => write!(f, "store: {detail}"),
            Self::ApprovalDidntFinish => f.write_str("the approval never answered"),
            Self::Unexpected { detail } => write!(f, "unexpected: {detail}"),
        }
    }
}

impl std::error::Error for SuggestedOpsError {}

/// How long the destination check may take on a local disk or a kernel mount before the row
/// says it doesn't know; a volume with a live session gets `io_budget`'s longer wait. A dead
/// mount must leave the dialog usable rather than hanging the whole list.
const DESTINATION_CHECK_TIMEOUT: Duration = Duration::from_secs(2);

/// One sweep, with the groups still waiting on the user.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SuggestedSweepView {
    pub sweep_id: i64,
    /// Unix seconds. The dialog renders it with the house date component.
    pub created_at: i64,
    /// The agent's words for the sweep as a whole, shown LABELLED as the agent's.
    pub rationale: Option<String>,
    pub groups: Vec<SuggestedGroupView>,
}

/// One reviewable group: everything the user weighs before approving it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SuggestedGroupView {
    pub group_id: i64,
    pub sweep_id: i64,
    /// Which version of the group this is: every amend bumps it. Send it back with the approval,
    /// and an approval of a version the agent has since replaced answers `listChanged`.
    pub revision: i64,
    pub verb: ProposalVerb,
    pub status: ProposalStatus,
    /// The friendly name the group leads with. Carries the selector's pattern as display text
    /// when a pattern produced the group.
    pub display_name: String,
    /// The agent's reason. Shown labelled as the agent's words, never as a fact Cmdr checked.
    pub rationale: Option<String>,
    pub source_volume_id: String,
    /// What the source volume is called: the registered volume's own name, or a saved place's
    /// while nobody has connected it. `None` when nothing knows that id.
    pub source_volume_name: Option<String>,
    /// The shared destination folder, the rename parent, or the archive path. `None` for trash
    /// and delete, which bind no target at all.
    pub destination: Option<String>,
    /// What the destination's volume is called ("naspi on 192.168.1.5", "Macintosh HD"): the
    /// registered volume's own name, or a saved place's while nobody has connected it. `None`
    /// when there's no destination, or nothing knows that id.
    pub destination_volume_name: Option<String>,
    /// The id of the volume `destination` is on (the source volume's for a rename), so the dialog
    /// can look the volume up the way the switcher does (its icon). `None` exactly when
    /// `destination` is: trash and delete bind no target.
    pub destination_volume_id: Option<String>,
    /// The folder every op sits directly in, so the dialog can name it once instead of on every
    /// row. `None` when the ops span folders.
    pub source_folder: Option<String>,
    /// How far an approved group could be taken back, once it runs. A fact to DISCLOSE; per the
    /// guiding principle it is never a reason to refuse a group.
    pub reversible: Reversibility,
    /// What this group would run as when that isn't its verb, worked out NOW from its source
    /// volume (a trash group on a volume with no Trash runs as a permanent delete). `None` when
    /// it runs as itself. Send it back with the approval: the fallback runs only when the user
    /// approved it. Agrees with the agent's own read.
    pub runs_as: Option<RunsAs>,
    /// Whether that destination is already there, and so whether approving creates it.
    pub destination_state: DestinationState,
    /// Ops in the live set: what would run.
    pub live_op_count: u64,
    /// Every op row the group has, including the ones a previous review deselected.
    pub total_op_count: u64,
    /// True when a pattern produced this group, so the dialog can show the pattern as the
    /// provenance of a list the user didn't watch being built.
    pub from_selector: bool,
    /// What a server card saves, present exactly for `addServer` and `editServer`, which have
    /// no ops: approve those with `suggested_ops_approve_server`.
    pub server: Option<ServerProposalView>,
}

/// Whether approving a group would create its target folder.
///
/// `Unknown` is a real answer, not a failure: a remote volume that isn't responding leaves the
/// row honest instead of guessing, and the user still sees the destination path itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum DestinationState {
    /// The verb binds no target (trash, delete), so there is nothing to create.
    NotApplicable,
    Exists,
    /// Approving creates it, exactly as a user-started move into a missing folder would.
    WillBeCreated,
    /// The destination's volume is one Cmdr knows (a saved share or server, a listed phone) but
    /// nothing has connected it, so there's nothing to ask. Approving refuses until it's
    /// connected, the same as a clicked move onto it would.
    NotConnected,
    /// The volume didn't answer in time, refused the check, or left the registry.
    Unknown,
}

/// One proposed op, as the review row shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SuggestedOpView {
    pub op_id: i64,
    pub source_path: String,
    /// The name this source becomes: under a rename, or a move or copy that renames it.
    pub new_name: Option<String>,
    /// `excluded` when an earlier review turned this op off (an approval the engine then refused to
    /// start leaves it so). Seed the tick from it and approve with `fromStored`, so that "no"
    /// survives without the review naming ops it never loaded.
    pub status: OpStatus,
    /// What the index held for this file when the group was frozen. `None` when the index had
    /// nothing, which the row says in words: never a zero, which reads as an empty file.
    pub snapshot_size: Option<u64>,
    /// Modification time in unix seconds, from the same frozen snapshot.
    pub snapshot_modified: Option<i64>,
}

/// One page of a group's ops, plus the counts that place it in the whole.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SuggestedOpPage {
    pub ops: Vec<SuggestedOpView>,
    /// Where this page starts, so a late answer can be matched to the window that asked.
    pub offset: u32,
    /// Every op row in the group. The dialog sizes its scrollbar from this, never from `ops`.
    pub total: u64,
}

/// Every sweep with at least one group still waiting on the user, newest first.
///
/// Counts only: not one op row is read here, because a group of 60 000 is legitimate and a list
/// that loaded them to count them would stall the dialog it exists to open.
#[tauri::command]
#[specta::specta]
pub async fn suggested_ops_list(app: AppHandle) -> Result<Vec<SuggestedSweepView>, SuggestedOpsError> {
    let summaries = with_read_connection(app.clone(), Vec::new(), move |conn| {
        let groups = list_groups(conn, Some(ProposalStatus::Pending))?;
        let mut out = Vec::with_capacity(groups.len());
        for summary in groups {
            let sweep = get_sweep(conn, summary.group.set_id)?;
            out.push((summary, sweep));
        }
        Ok(out)
    })
    .await
    .map_err(|detail| SuggestedOpsError::Store { detail })?;

    // The destination checks touch the filesystem, so they run OUTSIDE the database read, all
    // at once, each under its own deadline: one unresponsive mount costs one row's certainty
    // and the list one budget, never N of them back to back.
    let states =
        futures_util::future::join_all(summaries.iter().map(|(summary, _)| destination_state(&summary.group))).await;
    let manager = crate::file_system::volume::manager::get_volume_manager();
    // In memory, ❌ never the network, and only when a server card is there to describe.
    let saved = summaries
        .iter()
        .any(|(summary, _)| summary.server.is_some())
        .then(|| SavedServers::now(&app));
    let rows = summaries
        .into_iter()
        .zip(states)
        .map(|((summary, sweep), destination_state)| {
            let facts = read_facts(
                &summary.group,
                VolumeFacts {
                    registered: &|id| manager.get(id),
                    saved_name: &crate::server_volumes::saved_place_name,
                },
                destination_state,
            );
            let server = summary
                .server
                .as_ref()
                .zip(saved.as_ref())
                .map(|(proposal, saved)| server_card(proposal, saved));
            (to_group_view(summary, facts, server), sweep)
        })
        .collect();
    Ok(group_into_sweeps(rows))
}

/// What Cmdr itself knows about a group at read time, beside what the store froze.
#[derive(Debug, Clone, PartialEq, Eq)]
struct GroupReadFacts {
    destination_state: DestinationState,
    source_volume_name: Option<String>,
    destination_volume_name: Option<String>,
    runs_as: Option<RunsAs>,
}

/// Where a group's volume facts come from: the registry for a connected volume, the saved-place
/// stores for one nobody connected. Both are cached state, so asking costs no I/O, and the
/// seam lets tests hand in volumes without registering them.
struct VolumeFacts<'a> {
    registered: &'a dyn Fn(&str) -> Option<std::sync::Arc<dyn Volume>>,
    saved_name: &'a dyn Fn(&str) -> Option<String>,
}

/// The volume `destination` is on: its own id, or the source volume's for a rename (whose parent
/// lives there). `None` when the verb binds no destination.
fn destination_volume_id_of(group: &ProposalGroup) -> Option<&str> {
    group.destination.as_ref()?;
    Some(
        group
            .destination_volume_id
            .as_deref()
            .unwrap_or(group.source_volume_id.as_str()),
    )
}

/// Fold what the volumes say into a group's facts. No I/O: a registry lookup and a predicate
/// per group, so the list never runs a volume discovery to describe a suggestion.
fn read_facts(group: &ProposalGroup, volumes: VolumeFacts<'_>, destination_state: DestinationState) -> GroupReadFacts {
    let name_of = |id: &str| match (volumes.registered)(id) {
        Some(volume) => Some(volume.name().to_string()),
        None => (volumes.saved_name)(id),
    };
    let source = (volumes.registered)(&group.source_volume_id);
    GroupReadFacts {
        destination_state,
        source_volume_name: name_of(&group.source_volume_id),
        destination_volume_name: destination_volume_id_of(group).and_then(name_of),
        runs_as: runs_as(group.verb, source.as_deref()),
    }
}

/// The most op rows one window reads (`suggested_ops_page`) or stats (`suggested_ops_live_facts`).
///
/// The dialog asks for `OP_WINDOW_SIZE` (`suggested-ops-trigger.svelte.ts`, 200) at a time, the
/// rows a list around the viewport needs. This is the server's ceiling on that intent, with
/// headroom: a caller asking for more (a bug, or a 60 000-row "page") gets this many and pages
/// for the rest, since `offset` and `total` say where it is.
pub const MAX_OP_WINDOW: u32 = 500;

/// `limit`, capped at [`MAX_OP_WINDOW`].
pub(super) fn op_window(limit: u32) -> u32 {
    limit.min(MAX_OP_WINDOW)
}

/// One page of a group's ops, in `seq` order.
#[tauri::command]
#[specta::specta]
pub async fn suggested_ops_page(
    app: AppHandle,
    group_id: i64,
    offset: u32,
    limit: u32,
) -> Result<SuggestedOpPage, SuggestedOpsError> {
    let limit = op_window(limit);
    let empty = SuggestedOpPage {
        ops: Vec::new(),
        offset,
        total: 0,
    };
    with_read_connection(app, empty, move |conn| {
        let total = count_ops(conn, group_id, None)?;
        let ops = page_ops(conn, group_id, limit, offset)?;
        Ok(SuggestedOpPage {
            ops: ops.into_iter().map(to_op_view).collect(),
            offset,
            total,
        })
    })
    .await
    .map_err(|detail| SuggestedOpsError::Store { detail })
}

/// The user said no to a group.
///
/// Conditional on `pending`, like every other transition: a group that already left it keeps
/// the answer it has, and the dialog is told which so it can say what happened rather than
/// silently doing nothing.
#[tauri::command]
#[specta::specta]
pub async fn suggested_ops_reject(app: AppHandle, group_id: i64) -> Result<RejectResultView, SuggestedOpsError> {
    let db_path = super::db_path(&app).ok_or(SuggestedOpsError::StoreNotOpen)?;
    let now = now_secs();
    // Resolved here because the agent's own seams are pure: the store is parameterized on a
    // root, and this is the layer that still holds an `AppHandle`.
    let memory = crate::agent::memory::store_for(&app);
    tauri::async_runtime::spawn_blocking(move || {
        let conn = crate::agent::store::open_write_connection(&db_path)
            .map_err(|e| SuggestedOpsError::Store { detail: e.to_string() })?;
        // A real "no" from the review, which is the one rejection the agent learns from.
        let outcome = crate::agent::suggested_ops::reject(
            &conn,
            group_id,
            now,
            crate::agent::outcomes::RejectSource::Review,
            memory.as_ref(),
        )
        .map_err(|e| SuggestedOpsError::Store { detail: e.to_string() })?;
        Ok(match outcome {
            crate::agent::store::proposals::RejectOutcome::Rejected => RejectResultView::Rejected,
            crate::agent::store::proposals::RejectOutcome::NotPending { found } => {
                RejectResultView::AlreadyAnswered { found }
            }
            crate::agent::store::proposals::RejectOutcome::Unknown => RejectResultView::Unknown,
        })
    })
    .await
    .map_err(|e| SuggestedOpsError::Unexpected { detail: e.to_string() })?
}

/// What a rejection did, or why it didn't.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum RejectResultView {
    Rejected,
    /// Somebody already answered this group. The dialog re-reads rather than insisting.
    AlreadyAnswered {
        found: ProposalStatus,
    },
    /// No group with that id, which usually means the list on screen is stale.
    Unknown,
}

/// Fold groups under the sweep that produced them, keeping the store's newest-first order.
///
/// Pure, because it is the one piece of arranging the dialog depends on: a sweep is how the
/// user sees "one thing the agent noticed", and groups scattered across duplicate headers
/// would read as several.
fn group_into_sweeps(rows: Vec<(SuggestedGroupView, Option<ProposalSweep>)>) -> Vec<SuggestedSweepView> {
    let mut sweeps: Vec<SuggestedSweepView> = Vec::new();
    for (group, sweep) in rows {
        match sweeps.iter_mut().find(|s| s.sweep_id == group.sweep_id) {
            Some(existing) => existing.groups.push(group),
            None => {
                let header = sweep.unwrap_or_else(|| orphan_sweep(group.sweep_id));
                sweeps.push(SuggestedSweepView {
                    sweep_id: header.id,
                    created_at: header.created_at,
                    rationale: header.rationale,
                    groups: vec![group],
                });
            }
        }
    }
    sweeps
}

/// Whether the "target folder will be created" marker means anything for this verb. Pure, so
/// the verbs it deliberately stays silent about are pinned by a test rather than by a comment.
fn destination_check_applies(verb: ProposalVerb) -> bool {
    matches!(verb, ProposalVerb::Move | ProposalVerb::Copy | ProposalVerb::Extract)
}

fn to_group_view(
    summary: GroupSummary,
    facts: GroupReadFacts,
    server: Option<ServerProposalView>,
) -> SuggestedGroupView {
    let group = summary.group;
    let destination_volume_id = destination_volume_id_of(&group).map(str::to_string);
    SuggestedGroupView {
        group_id: group.id,
        sweep_id: group.set_id,
        revision: group.revision,
        verb: group.verb,
        status: group.status,
        display_name: group.display_name,
        rationale: group.rationale,
        source_volume_id: group.source_volume_id,
        source_volume_name: facts.source_volume_name,
        destination: group.destination,
        destination_volume_name: facts.destination_volume_name,
        destination_volume_id,
        source_folder: group.source_folder,
        reversible: group.reversible,
        runs_as: facts.runs_as,
        destination_state: facts.destination_state,
        live_op_count: summary.live_op_count,
        total_op_count: summary.total_op_count,
        from_selector: group.selector.is_some(),
        server,
    }
}

fn to_op_view(op: ProposalOp) -> SuggestedOpView {
    SuggestedOpView {
        op_id: op.id,
        source_path: op.source_path,
        new_name: op.destination,
        status: op.status,
        snapshot_size: op.snapshot_size,
        snapshot_modified: op.snapshot_mtime,
    }
}

/// A sweep row that has gone missing under its own groups. It can't happen through any code
/// path here (the groups cascade with their sweep), so rather than dropping the groups the
/// dialog would otherwise never show, they get a placeholder header.
fn orphan_sweep(sweep_id: i64) -> ProposalSweep {
    ProposalSweep {
        id: sweep_id,
        conversation_id: None,
        created_at: 0,
        created_by_model: None,
        rationale: None,
    }
}

/// Whether approving this group would create its target folder.
///
/// Only the verbs binding a shared destination FOLDER answer here. A rename binds the parent
/// its sources already live in, and a compress binds an archive path whose overwrite risk is
/// already carried by `reversible`; a second marker there would look the same while meaning
/// something else.
///
/// The check goes through the volume manager for EVERY volume, root included, so there is no
/// second copy of "is this path local" to drift against the one the rename preflight owns.
///
/// It asks for the entry rather than `exists`, because `exists` folds "the volume refused" into
/// "not there", and that would read as "will be created" on a share that merely didn't answer.
/// Every answer short of a real one is logged with its reason.
async fn destination_state(group: &ProposalGroup) -> DestinationState {
    if !destination_check_applies(group.verb) {
        return DestinationState::NotApplicable;
    }
    let Some(destination) = group.destination.clone() else {
        return DestinationState::NotApplicable;
    };
    let volume_id = group
        .destination_volume_id
        .as_deref()
        .unwrap_or(group.source_volume_id.as_str());
    let Some(volume) = crate::file_system::volume::manager::get_volume_manager().get(volume_id) else {
        // A `saved` share or server row is published only while its volume ISN'T registered, so
        // this is the branch every such destination lands in. Asking never dials.
        return match crate::unregistered_volumes::why_unregistered(volume_id).await {
            crate::unregistered_volumes::Unregistered::NotConnected => {
                log::debug!(
                    target: "agent::suggested_ops",
                    "group {}: the destination's volume {volume_id} isn't connected",
                    group.id
                );
                DestinationState::NotConnected
            }
            crate::unregistered_volumes::Unregistered::Gone => {
                log::warn!(
                    target: "agent::suggested_ops",
                    "group {}: no volume is registered as {volume_id}, so its destination can't be checked",
                    group.id
                );
                DestinationState::Unknown
            }
        };
    };
    let budget = io_budget(volume.connection_state(), DESTINATION_CHECK_TIMEOUT);
    let answer = timeout_detached_typed(
        budget,
        || DestinationGap::TimedOut,
        DestinationGap::Panicked,
        async move { Ok(volume.get_metadata(Path::new(&destination)).await) },
    )
    .await;
    match answer {
        Ok(Ok(_)) => DestinationState::Exists,
        Ok(Err(VolumeError::NotFound(_))) => DestinationState::WillBeCreated,
        Ok(Err(e)) => {
            log::debug!(
                target: "agent::suggested_ops",
                "group {}: the destination check was refused: {:?}",
                group.id,
                LogError(&e)
            );
            DestinationState::Unknown
        }
        Err(DestinationGap::TimedOut) => {
            log::debug!(
                target: "agent::suggested_ops",
                "group {}: the destination's volume {volume_id} didn't answer within {budget:?}",
                group.id
            );
            DestinationState::Unknown
        }
        Err(DestinationGap::Panicked(detail)) => {
            log::warn!(
                target: "agent::suggested_ops",
                "group {}: the destination check panicked: {detail}",
                group.id
            );
            DestinationState::Unknown
        }
    }
}

/// Why the destination check gave no answer. Only ever logged.
enum DestinationGap {
    TimedOut,
    Panicked(String),
}

#[cfg(test)]
#[path = "suggested_ops_test.rs"]
mod suggested_ops_test;

/// What approving a group did, in the terms the dialog acts on.
///
/// Every refusal is a typed variant rather than a sentence, because the recoveries genuinely
/// differ: "somebody already answered this" closes the group, "the list changed" sends the user
/// back to re-read it, and a refusal to START gives the group back with its reason under it.
///
/// The last three are that give-back: nothing ran, and the group is `pending` again (refused
/// before the claim, or released after it by the bridge), so the dialog keeps it on the list.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase", rename_all_fields = "camelCase", tag = "kind")]
pub enum ApprovalResultView {
    /// The ops are queued and running. The dialog closes and the queue takes over.
    Started { operation_id: String },
    /// The group left `pending` before this arrived: approved, rejected, or gone.
    AlreadyAnswered,
    /// The group isn't the version the user read (the agent amended it), or its op set isn't what
    /// preflight accepted, so nothing ran. The user re-reads it.
    ListChanged,
    /// No group with that id.
    Unknown,
    /// The write engine, or the volume check in front of it, refused. The error is the one a
    /// clicked operation would have shown, so the dialog words it with the same copy: a phone
    /// or server nobody connected is `source_not_connected`, a drive that left is
    /// `source_no_longer_connected`.
    Refused { error: WriteOperationError },
    /// Every file was gone or unreadable by the time the user approved, so there was nothing
    /// to run.
    NothingToRun,
    /// The selection left nothing to run (an "only these" with no files ticked). Nothing was
    /// recorded or claimed; the group stays `pending` with the last review's selection.
    NothingSelected,
    /// The stored group can't become an operation (a target its verb needs is missing). Not
    /// reachable through `GroupIntent`; the detail goes to the log, never to the dialog.
    CouldNotStart,
}

/// Approve a group: claim it and hand its ops to the queue.
///
/// The client sends whichever op ids are FEWER: the ones it turned off (`allExcept`) or the
/// ones it kept (`only`), so a 60,000-op group approved whole carries an empty list and one
/// with three files ticked carries three. Ids only; the acceptance record is computed
/// server-side either way.
///
/// **On its own thread with its own runtime.** `approve_and_execute` holds a `Connection`
/// across awaits, which a Tauri command's future can't (it must be `Send`). The result comes
/// back over a oneshot, so the command still answers the caller directly rather than making the
/// dialog wait on an event.
///
/// `runs_as` is the fallback the review showed (`SuggestedGroupView::runs_as`), so a trash group
/// on a drive with no Trash deletes permanently only when the user approved it as that delete.
#[tauri::command]
#[specta::specta]
pub async fn suggested_ops_approve(
    app: AppHandle,
    group_id: i64,
    revision: i64,
    selection: OpSelection,
    runs_as: Option<RunsAs>,
) -> Result<ApprovalResultView, SuggestedOpsError> {
    let db_path = super::db_path(&app).ok_or(SuggestedOpsError::StoreNotOpen)?;
    let now = now_secs();
    // Resolved before the handle is moved into the sink: what actually happened is only known
    // when the operation settles, on a thread that may never name an `AppHandle`.
    let memory = crate::agent::memory::store_for(&app);
    let (send, receive) = tokio::sync::oneshot::channel();

    std::thread::spawn(move || {
        let runtime = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
            Ok(runtime) => runtime,
            Err(e) => {
                let _ = send.send(Err(format!("{e}")));
                return;
            }
        };
        let outcome = runtime.block_on(async move {
            let conn = crate::agent::store::open_write_connection(&db_path).map_err(|e| e.to_string())?;
            // A second connection, MOVED into the sink decorator: the operation outlives this
            // call by minutes, so its writer can't borrow the one above.
            let reporting = crate::agent::store::open_write_connection(&db_path).map_err(|e| e.to_string())?;
            let sink = std::sync::Arc::new(crate::operations_host::TauriEventSink::new(app));
            crate::agent::suggested_ops::bridge::approve_and_execute(
                &conn, reporting, sink, group_id, revision, &selection, runs_as, now, memory,
            )
            .await
            .map_err(|e| e.to_string())
        });
        let _ = send.send(outcome);
    });

    let outcome = receive
        .await
        .map_err(|_| SuggestedOpsError::ApprovalDidntFinish)?
        .map_err(|detail| SuggestedOpsError::Store { detail })?;
    Ok(to_approval_view(outcome))
}

/// Map the bridge's outcome onto what the dialog acts on. Pure, so the mapping is pinned by
/// tests rather than read out of the command.
fn to_approval_view(outcome: ApprovalOutcome) -> ApprovalResultView {
    match outcome {
        ApprovalOutcome::Started(group) => ApprovalResultView::Started {
            operation_id: group.operation.operation_id,
        },
        ApprovalOutcome::Refused(refusal) => refusal_view(refusal),
    }
}

/// Collapsing two refusals that want different recoveries would make the dialog say the wrong
/// thing in one of the two cases, which is why the bridge types them apart.
fn refusal_view(refusal: ApprovalRefusal) -> ApprovalResultView {
    match refusal {
        ApprovalRefusal::NotAccepted(AcceptanceOutcome::Unknown) | ApprovalRefusal::Claim(ClaimRefusal::Unknown) => {
            ApprovalResultView::Unknown
        }
        ApprovalRefusal::NotAccepted(AcceptanceOutcome::NothingSelected) => ApprovalResultView::NothingSelected,
        // `Accepted` never arrives as a refusal; it's named so a new variant can't slip into this arm.
        ApprovalRefusal::NotAccepted(AcceptanceOutcome::NotPending { .. } | AcceptanceOutcome::Accepted { .. })
        | ApprovalRefusal::Claim(ClaimRefusal::StaleStatus { .. }) => ApprovalResultView::AlreadyAnswered,
        ApprovalRefusal::Claim(ClaimRefusal::BindingMismatch { .. })
        | ApprovalRefusal::NotAccepted(AcceptanceOutcome::RevisionChanged { .. }) => ApprovalResultView::ListChanged,
        ApprovalRefusal::SourceUnavailable(error)
        | ApprovalRefusal::Engine(error)
        | ApprovalRefusal::Rename(RenameStartError::Engine { error }) => ApprovalResultView::Refused { error },
        ApprovalRefusal::Rename(RenameStartError::NothingToRename) => ApprovalResultView::NothingToRun,
        ApprovalRefusal::Rename(RenameStartError::NotInOneFolder) => {
            log::warn!(target: "agent::suggested_ops", "a stored rename group spans folders, so it can't start");
            ApprovalResultView::CouldNotStart
        }
        // A server card approves through its own command; the dialog never sends one here.
        ApprovalRefusal::NotAccepted(AcceptanceOutcome::NotAFileGroup) => {
            log::warn!(
                target: "agent::suggested_ops",
                "a server group reached the file approval, which never saves a server"
            );
            ApprovalResultView::CouldNotStart
        }
        ApprovalRefusal::TargetMissing { verb } => {
            log::warn!(
                target: "agent::suggested_ops",
                "the stored group has no target for {}, so it can't start",
                verb.as_token()
            );
            ApprovalResultView::CouldNotStart
        }
    }
}
