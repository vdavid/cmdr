//! What the Suggested ops dialog reads, and the one decision it can record on its own.
//!
//! The dialog's whole job is disclosure: the user decides, so everything they need to judge a
//! suggestion has to reach them in terms they can check. That shapes these views more than
//! anything else here.
//!
//! - **A group's `rationale` is the AGENT's words** and the UI labels it as such. Beside it go
//!   facts Cmdr knows by itself, which is what makes a hallucinated claim visible.
//! - **Per-op numbers are the CREATION SNAPSHOT**, what the index held when the group froze.
//!   The fields say `snapshot` for that reason: a size relayed as current would be a claim
//!   nothing here can back (the same call `agent/tools/suggestions/group.rs` made for the
//!   agent's own read).
//! - **Reversibility and "the target folder will be created" are DISCLOSED, never blocking.**
//!   Once the user approves, it is exactly as if they started the action.
//!
//! Ops are read a PAGE at a time and counted with `COUNT(*)`: a group of 60 000 is legitimate,
//! so nothing here loads a group to describe it.

use std::path::Path;
use std::time::Duration;

use serde::Serialize;
use tauri::AppHandle;

use super::{now_secs, with_read_connection};
use crate::agent::store::proposals::{AcceptanceOutcome, ClaimRefusal};
use crate::agent::store::proposals::{
    GroupSummary, ProposalOp, ProposalSweep, count_ops, get_sweep, list_groups, page_ops,
};
use crate::agent::suggested_ops::bridge::{ApprovalOutcome, ApprovalRefusal};
use crate::agent::types::{OpStatus, ProposalStatus, ProposalVerb, Reversibility};
use crate::commands::util::IpcError;

/// How long the destination check may take before the row says it doesn't know. A dead mount
/// must leave the dialog usable rather than hanging the whole list.
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
    pub verb: ProposalVerb,
    pub status: ProposalStatus,
    /// The friendly name the group leads with. Carries the selector's pattern as display text
    /// when a pattern produced the group.
    pub display_name: String,
    /// The agent's reason. Shown labelled as the agent's words, never as a fact Cmdr checked.
    pub rationale: Option<String>,
    pub source_volume_id: String,
    /// The shared destination folder, the rename parent, or the archive path. `None` for trash
    /// and delete, which bind no target at all.
    pub destination: Option<String>,
    /// How far an approved group could be taken back. A fact to DISCLOSE; per the guiding
    /// principle it is never a reason to refuse a group.
    pub reversible: Reversibility,
    /// Whether that destination is already there, and so whether approving creates it.
    pub destination_state: DestinationState,
    /// Ops in the live set: what would run.
    pub live_op_count: u64,
    /// Every op row the group has, including the ones a previous review deselected.
    pub total_op_count: u64,
    /// True when a pattern produced this group, so the dialog can show the pattern as the
    /// provenance of a list the user didn't watch being built.
    pub from_selector: bool,
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
    /// The volume didn't answer in time, or isn't mounted.
    Unknown,
}

/// One proposed op, as the review row shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SuggestedOpView {
    pub op_id: i64,
    pub source_path: String,
    /// The name this source becomes. `Some` only under a rename group.
    pub new_name: Option<String>,
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
pub async fn suggested_ops_list(app: AppHandle) -> Result<Vec<SuggestedSweepView>, IpcError> {
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
    .map_err(IpcError::from_err)?;

    // The destination checks touch the filesystem, so they run OUTSIDE the database read and
    // each under its own deadline: one unresponsive mount must cost one row's certainty, not
    // the list.
    let mut rows = Vec::with_capacity(summaries.len());
    for (summary, sweep) in summaries {
        let state = destination_state(&summary).await;
        rows.push((to_group_view(summary, state), sweep));
    }
    Ok(group_into_sweeps(rows))
}

/// One page of a group's ops, in `seq` order.
#[tauri::command]
#[specta::specta]
pub async fn suggested_ops_page(
    app: AppHandle,
    group_id: i64,
    offset: u32,
    limit: u32,
) -> Result<SuggestedOpPage, IpcError> {
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
    .map_err(IpcError::from_err)
}

/// The user said no to a group.
///
/// Conditional on `pending`, like every other transition: a group that already left it keeps
/// the answer it has, and the dialog is told which so it can say what happened rather than
/// silently doing nothing.
#[tauri::command]
#[specta::specta]
pub async fn suggested_ops_reject(app: AppHandle, group_id: i64) -> Result<RejectResultView, IpcError> {
    let db_path = super::db_path(&app).ok_or_else(|| IpcError::from_err("Cmdr's suggestion store isn't open."))?;
    let now = now_secs();
    tauri::async_runtime::spawn_blocking(move || {
        let conn = crate::agent::store::open_write_connection(&db_path).map_err(IpcError::from_err)?;
        let outcome = crate::agent::suggested_ops::reject(&conn, group_id, now).map_err(IpcError::from_err)?;
        Ok(match outcome {
            crate::agent::store::proposals::RejectOutcome::Rejected => RejectResultView::Rejected,
            crate::agent::store::proposals::RejectOutcome::NotPending { found } => {
                RejectResultView::AlreadyAnswered { found }
            }
            crate::agent::store::proposals::RejectOutcome::Unknown => RejectResultView::Unknown,
        })
    })
    .await
    .map_err(IpcError::from_err)?
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

fn to_group_view(summary: GroupSummary, destination_state: DestinationState) -> SuggestedGroupView {
    let group = summary.group;
    SuggestedGroupView {
        group_id: group.id,
        sweep_id: group.set_id,
        verb: group.verb,
        status: group.status,
        display_name: group.display_name,
        rationale: group.rationale,
        source_volume_id: group.source_volume_id,
        destination: group.destination,
        reversible: group.reversible,
        destination_state,
        live_op_count: summary.live_op_count,
        total_op_count: summary.total_op_count,
        from_selector: group.selector.is_some(),
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
async fn destination_state(summary: &GroupSummary) -> DestinationState {
    let group = &summary.group;
    if !destination_check_applies(group.verb) {
        return DestinationState::NotApplicable;
    }
    let Some(destination) = group.destination.as_deref() else {
        return DestinationState::NotApplicable;
    };
    let volume_id = group
        .destination_volume_id
        .as_deref()
        .unwrap_or(group.source_volume_id.as_str());
    let Some(volume) = crate::file_system::volume::manager::get_volume_manager().get(volume_id) else {
        return DestinationState::Unknown;
    };
    let path = Path::new(destination).to_path_buf();
    match tokio::time::timeout(DESTINATION_CHECK_TIMEOUT, async move { volume.exists(&path).await }).await {
        Ok(true) => DestinationState::Exists,
        Ok(false) => DestinationState::WillBeCreated,
        Err(_) => DestinationState::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::store::proposals::ProposalGroup;

    /// Every refusal sends the user somewhere different, so the mapping is the thing worth
    /// pinning: "somebody already answered" closes the group, "the list changed" sends them back
    /// to re-read it, and an unmounted drive is neither. Collapsing any two would make the
    /// dialog say the wrong thing in one of the cases.
    #[test]
    fn each_refusal_keeps_the_recovery_it_implies() {
        assert_eq!(
            refusal_view(ApprovalRefusal::NotAccepted(AcceptanceOutcome::NotPending {
                found: ProposalStatus::Approved
            })),
            ApprovalResultView::AlreadyAnswered
        );
        assert_eq!(
            refusal_view(ApprovalRefusal::Claim(ClaimRefusal::StaleStatus {
                found: ProposalStatus::Rejected
            })),
            ApprovalResultView::AlreadyAnswered
        );
        assert_eq!(
            refusal_view(ApprovalRefusal::Claim(ClaimRefusal::BindingMismatch {
                accepted: None,
                live: crate::agent::store::proposals::OpBinding {
                    op_count: 3,
                    digest: "abc".into()
                }
            })),
            ApprovalResultView::ListChanged,
            "a changed op set is re-readable, never an already-answered group"
        );
        assert_eq!(
            refusal_view(ApprovalRefusal::NotAccepted(AcceptanceOutcome::Unknown)),
            ApprovalResultView::Unknown
        );
        assert_eq!(
            refusal_view(ApprovalRefusal::Claim(ClaimRefusal::Unknown)),
            ApprovalResultView::Unknown
        );
        assert_eq!(
            refusal_view(ApprovalRefusal::SourceVolumeGone {
                volume_id: "smb-nas".into()
            }),
            ApprovalResultView::SourceVolumeGone {
                volume_id: "smb-nas".into()
            },
            "the drive is named, because that is what the user has to reconnect"
        );
    }

    fn group(id: i64, set_id: i64, verb: ProposalVerb) -> GroupSummary {
        GroupSummary {
            group: ProposalGroup {
                id,
                set_id,
                seq: 0,
                verb,
                status: ProposalStatus::Pending,
                source_volume_id: "root".into(),
                destination: Some("/Users/someone/Documents/Invoices".into()),
                destination_volume_id: None,
                reversible: Reversibility::RestoreMove,
                display_name: "five invoices".into(),
                rationale: Some("They all look like invoices.".into()),
                selector: None,
                created_at: 100,
                decided_at: None,
            },
            live_op_count: 5,
            total_op_count: 7,
        }
    }

    fn sweep(id: i64) -> ProposalSweep {
        ProposalSweep {
            id,
            conversation_id: Some(3),
            created_at: 100,
            created_by_model: Some("some-model".into()),
            rationale: Some("Ten new files in Downloads.".into()),
        }
    }

    /// Every field here is something the user weighs before approving. A mapping that dropped
    /// one would take a disclosure off the screen while the dialog still looked complete,
    /// which is the failure this whole surface exists to prevent.
    #[test]
    fn a_group_view_carries_every_fact_the_user_judges_it_by() {
        let view = to_group_view(group(7, 2, ProposalVerb::Move), DestinationState::WillBeCreated);

        assert_eq!(view.group_id, 7);
        assert_eq!(view.sweep_id, 2);
        assert_eq!(view.verb, ProposalVerb::Move);
        assert_eq!(view.status, ProposalStatus::Pending);
        assert_eq!(view.display_name, "five invoices");
        assert_eq!(view.rationale.as_deref(), Some("They all look like invoices."));
        assert_eq!(view.source_volume_id, "root");
        assert_eq!(view.destination.as_deref(), Some("/Users/someone/Documents/Invoices"));
        assert_eq!(view.reversible, Reversibility::RestoreMove);
        assert_eq!(view.destination_state, DestinationState::WillBeCreated);
        assert_eq!(view.live_op_count, 5);
        assert_eq!(view.total_op_count, 7, "the deselected rows are part of the record");
        assert!(!view.from_selector);
    }

    /// A group a pattern produced says so, because "why is this file here?" has a different
    /// answer for a list the user never watched being built.
    #[test]
    fn a_selector_built_group_reports_its_provenance() {
        let mut summary = group(1, 1, ProposalVerb::Trash);
        summary.group.selector = Some("{\"root\":\"~/Downloads\"}".into());

        assert!(to_group_view(summary, DestinationState::NotApplicable).from_selector);
    }

    /// An index that held nothing must stay absent all the way to the row. A zero would render
    /// as an empty file and a 1970 date, both of which are claims nothing can back.
    #[test]
    fn an_op_with_no_snapshot_reports_absence_rather_than_zero() {
        let bare = to_op_view(ProposalOp {
            id: 4,
            group_id: 1,
            seq: 0,
            source_path: "/Users/someone/Downloads/one.dmg".into(),
            destination: None,
            status: OpStatus::Pending,
            snapshot_size: None,
            snapshot_mtime: None,
            snapshot_inode: None,
        });

        assert_eq!(bare.snapshot_size, None);
        assert_eq!(bare.snapshot_modified, None);
        let wire = serde_json::to_value(&bare).expect("serializes");
        assert!(wire["snapshotSize"].is_null());
        assert!(wire["snapshotModified"].is_null());
    }

    /// The marker answers for the verbs that create a folder, and stays quiet for the ones
    /// where it would mean something else: a rename binds the parent its sources already sit
    /// in, and a compress binds an archive path whose overwrite risk `reversible` already
    /// carries.
    #[test]
    fn only_the_verbs_that_can_create_a_folder_answer_the_destination_question() {
        for verb in [ProposalVerb::Move, ProposalVerb::Copy, ProposalVerb::Extract] {
            assert!(destination_check_applies(verb), "{verb:?} binds a destination folder");
        }
        for verb in [
            ProposalVerb::Rename,
            ProposalVerb::Compress,
            ProposalVerb::Trash,
            ProposalVerb::Delete,
        ] {
            assert!(
                !destination_check_applies(verb),
                "{verb:?} must not claim to create one"
            );
        }
    }

    /// A sweep is how the user sees one thing the agent noticed, so its groups arrive under one
    /// header however many there are.
    #[test]
    fn groups_fold_under_the_sweep_that_produced_them() {
        let rows = vec![
            (
                to_group_view(group(1, 10, ProposalVerb::Move), DestinationState::Exists),
                Some(sweep(10)),
            ),
            (
                to_group_view(group(2, 10, ProposalVerb::Trash), DestinationState::NotApplicable),
                Some(sweep(10)),
            ),
            (
                to_group_view(group(3, 11, ProposalVerb::Delete), DestinationState::NotApplicable),
                Some(sweep(11)),
            ),
        ];

        let sweeps = group_into_sweeps(rows);

        assert_eq!(sweeps.len(), 2, "two wakes, not three groups: {sweeps:?}");
        assert_eq!(sweeps[0].sweep_id, 10);
        assert_eq!(sweeps[0].groups.len(), 2);
        assert_eq!(sweeps[0].rationale.as_deref(), Some("Ten new files in Downloads."));
        assert_eq!(sweeps[1].sweep_id, 11);
        assert_eq!(sweeps[1].groups.len(), 1);
    }

    /// A sweep row missing under its own groups can't happen through any path here, but
    /// dropping the groups would hide work the user is being asked about. They get a
    /// placeholder header instead.
    #[test]
    fn groups_whose_sweep_row_is_missing_are_still_shown() {
        let rows = vec![(
            to_group_view(group(1, 42, ProposalVerb::Move), DestinationState::Exists),
            None,
        )];

        let sweeps = group_into_sweeps(rows);

        assert_eq!(sweeps.len(), 1);
        assert_eq!(sweeps[0].sweep_id, 42);
        assert_eq!(sweeps[0].groups.len(), 1);
        assert_eq!(sweeps[0].rationale, None);
    }
}

/// What approving a group did, in the terms the dialog acts on.
///
/// Every refusal is a typed variant rather than a sentence, because the recoveries genuinely
/// differ: "somebody already answered this" closes the group, "the list changed" sends the user
/// back to re-read it, and a missing drive is neither.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum ApprovalResultView {
    /// The ops are queued and running. The dialog closes and the queue takes over.
    Started { operation_id: String },
    /// The group left `pending` before this arrived: approved, rejected, or gone.
    AlreadyAnswered,
    /// The op set is not what preflight accepted, so nothing ran. The user re-reads it.
    ListChanged,
    /// No group with that id.
    Unknown,
    /// The drive the sources live on isn't mounted any more.
    SourceVolumeGone { volume_id: String },
    /// The group claimed, but the write engine wouldn't start it.
    CouldNotStart { detail: String },
}

/// Approve a group: claim it and hand its ops to the queue.
///
/// The client sends the ids it turned OFF, never the ones it kept, so a 60,000-op group
/// approved whole carries an empty list.
///
/// **On its own thread with its own runtime.** `approve_and_execute` holds a `Connection`
/// across awaits, which a Tauri command's future can't (it must be `Send`). The result comes
/// back over a oneshot, so the command still answers the caller directly rather than making the
/// dialog wait on an event.
#[tauri::command]
#[specta::specta]
pub async fn suggested_ops_approve(
    app: AppHandle,
    group_id: i64,
    deselected_op_ids: Vec<i64>,
) -> Result<ApprovalResultView, IpcError> {
    let db_path = super::db_path(&app).ok_or_else(|| IpcError::from_err("Cmdr's suggestion store isn't open."))?;
    let now = now_secs();
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
            let sink = std::sync::Arc::new(crate::file_system::write_operations::TauriEventSink::new(app));
            crate::agent::suggested_ops::bridge::approve_and_execute(
                &conn,
                reporting,
                sink,
                group_id,
                &deselected_op_ids,
                now,
            )
            .await
            .map_err(|e| e.to_string())
        });
        let _ = send.send(outcome);
    });

    let outcome = receive
        .await
        .map_err(|_| IpcError::from_err("Approving didn't finish. Open the review again."))?
        .map_err(IpcError::from_err)?;
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
        ApprovalRefusal::NotAccepted(_) | ApprovalRefusal::Claim(ClaimRefusal::StaleStatus { .. }) => {
            ApprovalResultView::AlreadyAnswered
        }
        ApprovalRefusal::Claim(ClaimRefusal::BindingMismatch { .. }) => ApprovalResultView::ListChanged,
        ApprovalRefusal::SourceVolumeGone { volume_id } => ApprovalResultView::SourceVolumeGone { volume_id },
        ApprovalRefusal::EngineRefused { detail } => ApprovalResultView::CouldNotStart { detail },
        ApprovalRefusal::Engine(error) => ApprovalResultView::CouldNotStart {
            detail: format!("{error:?}"),
        },
        ApprovalRefusal::TargetMissing { verb } => ApprovalResultView::CouldNotStart {
            detail: format!("the stored group has no target for {}", verb.as_token()),
        },
    }
}
