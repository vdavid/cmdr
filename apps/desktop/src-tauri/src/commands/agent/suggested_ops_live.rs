//! What a group's files are like on disk NOW, a window at a time, beside the creation snapshot
//! `suggested_ops_page` carries. Its own file because it's the one suggested-ops read that
//! touches the source volume per row; the deadline and the shared stat budget live in
//! `agent/suggested_ops/facts.rs`.

use std::path::Path;
use std::sync::Arc;

use cmdr_fs::deadline::Deadline;
use serde::Serialize;
use tauri::AppHandle;

use super::suggested_ops::op_window;
use crate::agent::store::proposals::{ProposalGroup, ProposalOp, get_group, page_ops};
use crate::agent::suggested_ops::facts::{SourceStat, stat_deadline, stat_sources};
use crate::agent::types::ProposalVerb;
use crate::file_system::volume::Volume;

/// What one op's source is like on disk right now, beside its creation snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SuggestedOpLiveFact {
    pub op_id: i64,
    pub state: LiveFileState,
    /// Whether the name this op gives its file is free where the file lands. `None` when the op
    /// keeps its name: a plain move's collision is the transfer's own conflict prompt, as for a
    /// clicked move.
    pub new_name: Option<NewNameState>,
}

/// What's at the path a renamed file would land on, now (#774). The run itself still answers a
/// collision with the transfer's conflict prompt, as a clicked move would; this says so before
/// the click.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum NewNameState {
    /// Nothing is there (or only the file itself, under a rename that only changes case).
    Free,
    /// Something else already has that name there.
    Taken,
    /// The target volume didn't answer, or isn't connected, so Cmdr can't tell.
    Unknown,
}

/// One source as its volume answers for it now. Three answers the dialog words differently,
/// so none collapses into another: a missing file is a fact, an unanswered stat is not.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum LiveFileState {
    Present {
        /// Bytes. `None` for a folder, whose own stat size means nothing to a user.
        size: Option<u64>,
        /// Unix seconds.
        modified: Option<i64>,
    },
    /// The volume says nothing is at that path any more.
    Missing,
    /// The volume didn't answer within the deadline, or refused the stat.
    Unknown,
}

/// Why a live-facts read didn't happen at all. Per-file trouble is never an error: it's that
/// file's `Unknown`.
///
/// ❌ Not prose: the dialog words each variant itself; the log records the detail.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum SuggestedOpsLiveFactsError {
    /// The suggestion store isn't open: agent features are off, or the app is still starting.
    StoreNotOpen,
    /// The store refused the read.
    Store {
        /// What SQLite reported, for the log.
        detail: String,
    },
    /// No group with that id, which usually means the list on screen is stale.
    UnknownGroup,
    /// The group's source volume is one Cmdr knows (a saved share or server, a listed phone)
    /// but nothing has connected it, so there's nothing to ask.
    SourceNotConnected,
    /// No volume is registered under the group's source volume id any more (a drive that left).
    SourceGone,
    /// Nothing above classifies it (a panicked task).
    Unexpected {
        /// What the runtime reported, for the log.
        detail: String,
    },
}

/// The current size and date of one window of a group's ops, so the dialog can draw a page
/// from the frozen rows first and fill in what's true now as it arrives.
///
/// Every stat goes through the group's SOURCE volume, under one shared deadline per call
/// (`io_budget`: 2 s, at least 10 s on a live session), with the stats capped across every
/// caller by `facts.rs`'s `BlockingBudget`: the dialog re-asks on scroll faster than a slow
/// share answers.
#[tauri::command]
#[specta::specta]
pub async fn suggested_ops_live_facts(
    app: AppHandle,
    group_id: i64,
    offset: u32,
    limit: u32,
) -> Result<Vec<SuggestedOpLiveFact>, SuggestedOpsLiveFactsError> {
    // The same cap as the page: a window is stat'ed whole, so an unbounded one would be an
    // unbounded wait on a slow share.
    let limit = op_window(limit);
    let db_path = super::db_path(&app).ok_or(SuggestedOpsLiveFactsError::StoreNotOpen)?;
    let (group, ops) = tauri::async_runtime::spawn_blocking(move || {
        let read = || -> Result<_, crate::agent::store::AgentStoreError> {
            let conn = crate::agent::store::open_read_connection(&db_path)?;
            let Some(group) = get_group(&conn, group_id)? else {
                return Ok(None);
            };
            Ok(Some((group, page_ops(&conn, group_id, limit, offset)?)))
        };
        read()
    })
    .await
    .map_err(|e| SuggestedOpsLiveFactsError::Unexpected { detail: e.to_string() })?
    .map_err(|e| SuggestedOpsLiveFactsError::Store { detail: e.to_string() })?
    .ok_or(SuggestedOpsLiveFactsError::UnknownGroup)?;

    let volume_id = group.source_volume_id.as_str();
    let Some(volume) = crate::file_system::volume::manager::get_volume_manager().get(volume_id) else {
        return Err(match crate::unregistered_volumes::why_unregistered(volume_id).await {
            crate::unregistered_volumes::Unregistered::NotConnected => SuggestedOpsLiveFactsError::SourceNotConnected,
            crate::unregistered_volumes::Unregistered::Gone => SuggestedOpsLiveFactsError::SourceGone,
        });
    };
    let deadline = stat_deadline(volume.as_ref());
    let paths = ops.iter().map(|op| op.source_path.clone()).collect();
    let stats = stat_sources(volume.clone(), paths, &deadline).await;
    let landings = new_name_states(&group, volume, &ops, &stats, &deadline).await;
    Ok(ops
        .into_iter()
        .zip(stats)
        .zip(landings)
        .map(|((op, stat), new_name)| SuggestedOpLiveFact {
            op_id: op.id,
            state: live_state(stat),
            new_name,
        })
        .collect())
}

/// Where an op's file lands under its NEW name, or `None` for an op that keeps its name (or a
/// verb that names nothing). A rename lands beside its source, in the parent the group binds; a
/// move or copy lands in the group's destination folder.
fn landing_path(group: &ProposalGroup, op: &ProposalOp) -> Option<String> {
    let name = op.destination.as_deref()?;
    match group.verb {
        ProposalVerb::Move | ProposalVerb::Copy | ProposalVerb::Rename => {}
        _ => return None,
    }
    let folder = group.destination.as_deref()?;
    Some(Path::new(folder).join(name).display().to_string())
}

/// Stat every renamed op's landing path, under the same deadline as the sources. The target
/// volume is the source's for a rename, and the destination's for a move or copy; one that
/// isn't registered answers `Unknown` for every renamed row rather than guessing.
async fn new_name_states(
    group: &ProposalGroup,
    source_volume: Arc<dyn Volume>,
    ops: &[ProposalOp],
    sources: &[SourceStat],
    deadline: &Deadline,
) -> Vec<Option<NewNameState>> {
    let landings: Vec<Option<String>> = ops.iter().map(|op| landing_path(group, op)).collect();
    if landings.iter().all(Option::is_none) {
        return vec![None; ops.len()];
    }
    let source_volume_id = group.source_volume_id.as_str();
    let target_id = match group.verb {
        ProposalVerb::Rename => source_volume_id,
        _ => group.destination_volume_id.as_deref().unwrap_or(source_volume_id),
    };
    let same_volume = target_id == source_volume_id;
    let target = if same_volume {
        Some(source_volume)
    } else {
        crate::file_system::volume::manager::get_volume_manager().get(target_id)
    };
    let Some(target) = target else {
        return landings
            .iter()
            .map(|landing| landing.as_ref().map(|_| NewNameState::Unknown))
            .collect();
    };
    let asked: Vec<String> = landings.iter().flatten().cloned().collect();
    let mut answers = stat_sources(target, asked, deadline).await.into_iter();
    landings
        .iter()
        .zip(sources)
        .map(|(landing, source)| {
            landing.as_ref()?;
            let landed = answers.next().unwrap_or(SourceStat::Unknown);
            Some(new_name_state(landed, *source, same_volume))
        })
        .collect()
}

/// What a landing stat means. On the source's own volume, a hit with the source's inode is the
/// file itself (a rename that only changes case, on a case-insensitive disk), so the name is
/// free. A name another op of the same group is about to vacate still reads `Taken`: it IS taken
/// right now, and the run's own order decides the rest.
fn new_name_state(landed: SourceStat, source: SourceStat, same_volume: bool) -> NewNameState {
    match (landed, source) {
        (SourceStat::Missing, _) => NewNameState::Free,
        (SourceStat::Unknown, _) => NewNameState::Unknown,
        (
            SourceStat::Present {
                inode: Some(landed), ..
            },
            SourceStat::Present {
                inode: Some(source), ..
            },
        ) if same_volume && landed == source => NewNameState::Free,
        (SourceStat::Present { .. }, _) => NewNameState::Taken,
    }
}

fn live_state(stat: SourceStat) -> LiveFileState {
    match stat {
        SourceStat::Present { size, modified, .. } => LiveFileState::Present { size, modified },
        SourceStat::Missing => LiveFileState::Missing,
        SourceStat::Unknown => LiveFileState::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A live fact crosses the wire as a tagged union the dialog can switch on, and a folder's or
    /// an undated file's missing number stays `null`, never a zero.
    #[test]
    fn a_live_fact_keeps_its_three_answers_apart_on_the_wire() {
        let present = live_state(SourceStat::Present {
            size: Some(7),
            modified: Some(1_700_000_000),
            inode: Some(12),
        });
        assert_eq!(
            serde_json::to_value(SuggestedOpLiveFact {
                op_id: 4,
                state: present,
                new_name: Some(NewNameState::Taken),
            })
            .unwrap(),
            serde_json::json!({
                "opId": 4,
                "state": { "kind": "present", "size": 7, "modified": 1_700_000_000 },
                "newName": "taken",
            })
        );
        let folder = live_state(SourceStat::Present {
            size: None,
            modified: None,
            inode: None,
        });
        assert_eq!(
            serde_json::to_value(folder).unwrap(),
            serde_json::json!({ "kind": "present", "size": null, "modified": null })
        );
        assert_eq!(
            serde_json::to_value(live_state(SourceStat::Missing)).unwrap(),
            serde_json::json!({ "kind": "missing" })
        );
        assert_eq!(
            serde_json::to_value(live_state(SourceStat::Unknown)).unwrap(),
            serde_json::json!({ "kind": "unknown" })
        );
    }

    fn present(inode: u64) -> SourceStat {
        SourceStat::Present {
            size: Some(1),
            modified: Some(1),
            inode: Some(inode),
        }
    }

    /// A renamed file's landing name reads taken only when something ELSE is there (#774): the
    /// file itself under a case-only rename is free, and a target that didn't answer is unknown,
    /// never "free".
    #[test]
    fn a_landing_name_is_taken_only_by_something_else() {
        assert_eq!(
            new_name_state(SourceStat::Missing, present(1), true),
            NewNameState::Free
        );
        assert_eq!(new_name_state(present(2), present(1), true), NewNameState::Taken);
        assert_eq!(
            new_name_state(present(1), present(1), true),
            NewNameState::Free,
            "a case-only rename finds the file itself"
        );
        assert_eq!(
            new_name_state(present(1), present(1), false),
            NewNameState::Taken,
            "an inode on another volume names another file"
        );
        assert_eq!(
            new_name_state(SourceStat::Unknown, present(1), true),
            NewNameState::Unknown
        );
    }

    /// Only an op that carries a new name has a landing to check, and a move lands in its group's
    /// destination folder.
    #[test]
    fn only_a_renamed_op_has_a_landing_path() {
        let group = ProposalGroup {
            id: 1,
            set_id: 1,
            seq: 0,
            verb: ProposalVerb::Move,
            status: crate::agent::types::ProposalStatus::Pending,
            source_volume_id: "root".into(),
            destination: Some("/papers/invoices".into()),
            destination_volume_id: Some("naspi".into()),
            reversible: crate::agent::types::Reversibility::RestoreMove,
            display_name: "invoices".into(),
            rationale: None,
            selector: None,
            created_at: 0,
            decided_at: None,
            revision: 0,
            source_folder: None,
        };
        let op = |destination: Option<&str>| ProposalOp {
            id: 1,
            group_id: 1,
            seq: 0,
            source_path: "/Users/sam/Downloads/invoice-07.pdf".into(),
            destination: destination.map(str::to_string),
            status: crate::agent::types::OpStatus::Pending,
            snapshot_size: None,
            snapshot_mtime: None,
            snapshot_inode: None,
        };
        assert_eq!(
            landing_path(&group, &op(Some("2026-07 Telia.pdf"))).as_deref(),
            Some("/papers/invoices/2026-07 Telia.pdf")
        );
        assert_eq!(landing_path(&group, &op(None)), None);
    }

    /// A source volume nobody connected and one that left call for different words, so they stay
    /// two variants.
    #[test]
    fn a_live_facts_refusal_names_why_the_source_cant_be_asked() {
        assert_eq!(
            serde_json::to_value(SuggestedOpsLiveFactsError::SourceNotConnected).unwrap(),
            serde_json::json!({ "type": "sourceNotConnected" })
        );
        assert_eq!(
            serde_json::to_value(SuggestedOpsLiveFactsError::SourceGone).unwrap(),
            serde_json::json!({ "type": "sourceGone" })
        );
        assert_eq!(
            serde_json::to_value(SuggestedOpsLiveFactsError::UnknownGroup).unwrap(),
            serde_json::json!({ "type": "unknownGroup" })
        );
    }
}
