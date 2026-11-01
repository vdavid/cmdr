//! Review and apply a server-owned rename proposal.
//!
//! Paths and destination names never cross this IPC boundary: the frontend submits only
//! opaque row ids, and this layer looks the real work up in the proposal spine.
//!
//! Checking has no time limit: a 500-row job on a busy NAS can take minutes while working fine.
//! It reports progress and stops on `cancel_bulk_rename_check` (`propose/rename/check.rs`).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tauri::{AppHandle, Manager};

use crate::agent::AgentDb;
use crate::agent::store::proposals::ClaimOutcome;
use crate::tools::propose::rename::{
    AcceptedRenamePreflights, BulkRenamePreflight, BulkRenamePreflightStatus, PreflightStopped, RenameChecks,
    RenameSourceFingerprint, ReviseRefusal,
};
/// Why a rename review couldn't be checked or applied.
///
/// ❌ Not prose: the review dialog words each variant from the catalog
/// (`rename-review-status.ts`); a `detail` here is for the log.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum BulkRenameError {
    /// The review this plan belonged to is gone: the app restarted, or somebody
    /// already answered the group.
    ReviewExpired,
    /// Something moved under the review (a revised name, a different subset), so
    /// the plan has to be looked at again before it can run.
    NeedsAnotherLook,
    /// The agent amended the plan since the revision the review showed, so nothing ran. The
    /// review re-reads the plan.
    ListChanged,
    /// The plan has no rows left to apply.
    NothingToApply,
    /// The row edit was turned down (the name, the row id, or the store).
    RowRefused {
        /// What the propose layer reported, for the log.
        detail: String,
    },
    /// The rename batch wouldn't start, and nothing was renamed.
    CouldntStart {
        /// Why, typed: a volume refusal carries the `WriteOperationError` the transfer
        /// dialogs already word.
        reason: cmdr_operations::RenameStartError,
    },
    /// The check was stopped before it answered: the user pressed Stop checking, the review
    /// closed, or a newer check for this plan replaced it. Nothing ran.
    Cancelled,
    /// Nothing above classifies it (a panicked task, Cmdr's own database not answering).
    Unexpected {
        /// What the runtime reported, for the log.
        detail: String,
    },
}

impl std::fmt::Display for BulkRenameError {
    /// ❗ For logs and debugging only.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ReviewExpired => f.write_str("the review has expired"),
            Self::NeedsAnotherLook => f.write_str("the plan changed under the review"),
            Self::ListChanged => f.write_str("the agent amended the plan since the review showed it"),
            Self::NothingToApply => f.write_str("no rows to apply"),
            Self::RowRefused { detail } => write!(f, "row refused: {detail}"),
            Self::CouldntStart { reason } => write!(f, "couldn't start: {reason}"),
            Self::Cancelled => f.write_str("the check was stopped"),
            Self::Unexpected { detail } => write!(f, "unexpected: {detail}"),
        }
    }
}

impl std::error::Error for BulkRenameError {}

impl From<PreflightStopped> for BulkRenameError {
    fn from(stopped: PreflightStopped) -> Self {
        match stopped {
            PreflightStopped::Cancelled => Self::Cancelled,
            PreflightStopped::StoreUnavailable { detail } => Self::Unexpected { detail },
        }
    }
}

impl From<ReviseRefusal> for BulkRenameError {
    fn from(refusal: ReviseRefusal) -> Self {
        match refusal {
            ReviseRefusal::NameRefused { detail } => Self::RowRefused { detail },
            ReviseRefusal::ReviewOver => Self::ReviewExpired,
            ReviseRefusal::StoreUnavailable { detail } => Self::Unexpected { detail },
        }
    }
}

/// An applied rename plan: the operation it started, and the renames it left
/// out for the thread's result line to mention.
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct BulkRenameStarted {
    pub operation_id: String,
    /// Renames that swap names with each other, left out of a plan that runs
    /// as a move (a rename that copies on S3): a move onto a folder still there
    /// would merge the two.
    pub swaps_left_out: u32,
}
use cmdr_operations::{LocalContent, RemoteContent, SourceFingerprint};

/// Revalidates the user-selected subset of a server-owned rename proposal. The
/// frontend supplies opaque ids only, never source paths or destination names.
#[tauri::command]
#[specta::specta]
pub async fn preflight_bulk_rename(
    app: AppHandle,
    proposal_id: String,
    revision: i64,
    allowed_row_ids: Vec<String>,
) -> Result<BulkRenamePreflight, BulkRenameError> {
    crate::tools::propose::rename::preflight(&app, proposal_id, revision, allowed_row_ids)
        .await
        .map_err(BulkRenameError::from)
}

/// Stops a running check for this proposal, leaving the review open. Answers whether one was
/// running. The stopped `preflight_bulk_rename` or `apply_bulk_rename` answers `Cancelled`.
#[tauri::command]
#[specta::specta]
pub async fn cancel_bulk_rename_check(app: AppHandle, proposal_id: String) -> bool {
    app.try_state::<RenameChecks>()
        .is_some_and(|checks| checks.cancel(&proposal_id))
}

/// Starts the user-approved subset of a server-owned rename plan. Paths and
/// names never cross this IPC boundary: the frontend submits only opaque ids.
#[tauri::command]
#[specta::specta]
pub async fn apply_bulk_rename(
    app: AppHandle,
    proposal_id: String,
    revision: i64,
    allowed_row_ids: Vec<String>,
) -> Result<BulkRenameStarted, BulkRenameError> {
    let (Some(db), Some(accepted_preflights)) =
        (app.try_state::<AgentDb>(), app.try_state::<AcceptedRenamePreflights>())
    else {
        return Err(review_is_over());
    };

    // A review that checked every row arrives here with a subset whenever the user denied one,
    // and a stale client with anything: both rerun the authoritative check for exactly these
    // rows rather than trust old ones. It reports progress and stops like any other check.
    if accepted_preflights.matching(&proposal_id, &allowed_row_ids).is_none() {
        let preflight =
            crate::tools::propose::rename::preflight(&app, proposal_id.clone(), revision, allowed_row_ids.clone())
                .await?;
        match preflight.status {
            BulkRenamePreflightStatus::Ready => {}
            BulkRenamePreflightStatus::ListChanged => return Err(BulkRenameError::ListChanged),
            BulkRenamePreflightStatus::Blocked | BulkRenamePreflightStatus::Expired => return Err(review_again()),
        }
    }

    let Some(accepted) = accepted_preflights.take_matching(&proposal_id, &allowed_row_ids) else {
        return Err(review_is_over());
    };
    let conn = db.open_write_connection().map_err(store_hiccup)?;
    let Some(proposal) = crate::tools::propose::rename::load(&conn, &proposal_id).map_err(store_hiccup)? else {
        return Err(review_is_over());
    };
    // The plan the user read, or nothing: an amend since the review (even one that landed after
    // the preflight) is a different plan. The claim below refuses it too, since an amend tears up
    // the acceptance; this says WHY.
    if proposal.revision != revision {
        return Err(BulkRenameError::ListChanged);
    }
    if proposal.rows.is_empty() {
        return Err(BulkRenameError::NothingToApply);
    }
    // One volume for the whole plan, straight off the group. There is nothing to cross-check:
    // a rename group binds one source volume and its rows can't carry a different answer.
    let volume_id = proposal.volume_id.clone();

    let fingerprints: HashMap<_, _> = accepted
        .fingerprints
        .into_iter()
        .map(|fingerprint| (fingerprint_row_id(&fingerprint).to_string(), fingerprint))
        .collect();
    let mut rows = Vec::with_capacity(allowed_row_ids.len());
    let mut applied_rows = Vec::with_capacity(allowed_row_ids.len());
    for row_id in &allowed_row_ids {
        let Some(proposal_row) = proposal.rows.iter().find(|row| &row.row_id == row_id) else {
            return Err(review_again());
        };
        let Some(fingerprint) = fingerprints.get(row_id) else {
            return Err(review_again());
        };
        applied_rows.push(proposal_row);
        rows.push(cmdr_operations::BulkRenameRow {
            row_id: row_id.clone(),
            source: PathBuf::from(&proposal_row.source_path),
            destination: Path::new(&proposal_row.source_path)
                .parent()
                .unwrap_or_else(|| Path::new(""))
                .join(&proposal_row.destination_name),
            expected_fingerprint: map_bulk_rename_fingerprint(fingerprint),
        });
    }

    // The claim is what makes this plan un-replayable: one conditional transaction moves the
    // group out of `pending`, and it refuses when the op set no longer matches what preflight
    // accepted (a revised name) or when somebody already answered. Last, so a refusal leaves
    // the group reviewable rather than approved-but-unstarted.
    let group_id = group_id_of(&proposal_id)?;
    let claimed = crate::agent::suggested_ops::approve(&conn, group_id, now_secs()).map_err(store_hiccup)?;
    let ClaimOutcome::Claimed(claimed) = claimed else {
        return Err(review_again());
    };

    let initiator = bulk_rename_initiator(&applied_rows);
    // A second connection, MOVED into the sink decorator: the operation outlives this call.
    let reporting = db.open_write_connection().map_err(store_hiccup)?;
    let memory = crate::agent::memory::store_for(&app);
    // Routed: where a rename copies (an S3 folder), the batch runs as one move.
    let started = crate::agent::suggested_ops::bridge::start_reviewed_renames(
        conn,
        reporting,
        Arc::new(crate::operations_host::TauriEventSink::new(app)),
        claimed,
        volume_id,
        rows,
        initiator,
        memory,
        now_secs(),
    )
    .await
    .map_err(|reason| BulkRenameError::CouldntStart { reason })?;
    Ok(BulkRenameStarted {
        operation_id: started.operation.operation_id,
        swaps_left_out: u32::try_from(started.swaps_left_out).unwrap_or(u32::MAX),
    })
}

fn group_id_of(proposal_id: &str) -> Result<i64, BulkRenameError> {
    crate::tools::propose::rename::numeric_id(proposal_id).ok_or(BulkRenameError::ReviewExpired)
}

fn review_is_over() -> BulkRenameError {
    BulkRenameError::ReviewExpired
}

fn review_again() -> BulkRenameError {
    BulkRenameError::NeedsAnotherLook
}

/// Cmdr's own database didn't answer. The plan may be fine, so this mustn't read as an expired
/// review: that tells the user to throw it away.
fn store_hiccup(error: impl std::fmt::Display) -> BulkRenameError {
    BulkRenameError::Unexpected {
        detail: error.to_string(),
    }
}

fn now_secs() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Replaces one row's proposed name with the one the user typed in the review, and answers the
/// row as the dialog should now show it. The name is validated server-side; the row keeps no
/// evidence afterwards, and the edit invalidates the accepted preflight, so the new name is
/// rechecked before it can reach the filesystem.
#[tauri::command]
#[specta::specta]
pub async fn revise_bulk_rename_row(
    app: AppHandle,
    proposal_id: String,
    row_id: String,
    destination_name: String,
) -> Result<crate::tools::propose::rename::RenameProposalRowSnapshot, BulkRenameError> {
    // Off the IPC thread: the edit lands in `main.db`, and a command that opens a database
    // must not block the handler.
    tauri::async_runtime::spawn_blocking(move || {
        crate::tools::propose::rename::revise_row(&app, &proposal_id, &row_id, &destination_name)
            .map_err(BulkRenameError::from)
    })
    .await
    .map_err(|e| BulkRenameError::Unexpected { detail: e.to_string() })?
}

/// Who the operation log credits for a batch. The agent proposed it, but a row the user
/// retyped in the review was the user's own choice, so a batch carrying one is mixed
/// provenance rather than the agent's work.
fn bulk_rename_initiator(
    rows: &[&crate::tools::propose::rename::RenameProposalRow],
) -> cmdr_operation_log::types::Initiator {
    let user_edited = rows
        .iter()
        .any(|row| row.evidence.source == crate::tools::propose::evidence::EvidenceSource::UserEdited);
    if user_edited {
        cmdr_operation_log::types::Initiator::AgentEdited
    } else {
        cmdr_operation_log::types::Initiator::Agent
    }
}

fn fingerprint_row_id(fingerprint: &RenameSourceFingerprint) -> &str {
    match fingerprint {
        RenameSourceFingerprint::Local { row_id, .. } | RenameSourceFingerprint::Remote { row_id, .. } => row_id,
    }
}

fn map_bulk_rename_fingerprint(fingerprint: &RenameSourceFingerprint) -> SourceFingerprint {
    match fingerprint {
        RenameSourceFingerprint::Local {
            device,
            inode,
            size,
            modified_nanos,
            ..
        } => SourceFingerprint::Local {
            device: *device,
            inode: *inode,
            // The rename review only ever accepts files, so a directory that
            // turned up under a reviewed name mismatches on the variant alone.
            content: LocalContent::File {
                size: *size,
                modified_nanos: *modified_nanos,
            },
        },
        RenameSourceFingerprint::Remote {
            normalized_path,
            size,
            modified,
            ..
        } => SourceFingerprint::Remote {
            normalized_path: normalized_path.clone(),
            content: RemoteContent::File {
                size: *size,
                modified: *modified,
            },
        },
    }
}

/// Records the user's "no" after they close a review. There is no agent-controlled approval
/// route: only a user action decides a proposal, and closing the dialog is one.
///
/// The group is REJECTED rather than deleted, so what the user was asked and what they answered
/// stays in the decision record.
#[tauri::command]
#[specta::specta]
pub async fn cancel_bulk_rename_proposal(app: AppHandle, proposal_id: String) {
    if let Some(accepted) = app.try_state::<AcceptedRenamePreflights>() {
        accepted.forget(&proposal_id);
    }
    if let Some(checks) = app.try_state::<RenameChecks>() {
        // The review is closing whether or not a check was running.
        let _ = checks.cancel(&proposal_id);
    }
    let (Some(db_path), Some(group_id)) = (
        app.try_state::<AgentDb>().map(|db| db.db_path().to_path_buf()),
        crate::tools::propose::rename::numeric_id(&proposal_id),
    ) else {
        return;
    };
    // Off the IPC thread, like every other command that opens `main.db`.
    let _ = tauri::async_runtime::spawn_blocking(move || {
        let Ok(conn) = crate::agent::store::open_write_connection(&db_path) else {
            return;
        };
        // ⚠️ **A dismissal, ❌ not a rejection.** The group needs an answer and gets one, but
        // the user said nothing about the proposal by pressing Escape. Recording a lesson from
        // it would teach the agent an opinion nobody gave, and asking "why did you say no?"
        // would spend a model call and land a turn in whatever thread they had open — this
        // sweep's `conversation_id` is the RAIL conversation.
        if let Err(e) = crate::agent::suggested_ops::reject(
            &conn,
            group_id,
            now_secs(),
            crate::agent::outcomes::RejectSource::DialogDismissed,
            None,
        ) {
            log::warn!(target: "agent::propose", "closing a rename review didn't record the rejection: {e}");
        }
    })
    .await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::propose::evidence::{EvidenceSource, RenameEvidence};
    use crate::tools::propose::rename::RenameProposalRow;
    use cmdr_operation_log::types::Initiator;

    fn row_from(source: EvidenceSource) -> RenameProposalRow {
        RenameProposalRow {
            row_id: "row".into(),
            source_path: "/shots/one.png".into(),
            destination_name: "renamed.png".into(),
            evidence: RenameEvidence {
                source,
                detail: "Invoice 4021 total".into(),
            },
            coverage: None,
        }
    }

    /// Provenance has to stay honest. The agent proposed the batch, but a name the user
    /// retyped in the review was not the agent's choice, so recording plain `Agent` for that
    /// batch would credit the model for the user's correction — and would tell a later reader
    /// of the log that a name they fixed themselves came from the model.
    #[test]
    fn a_batch_carrying_a_user_edited_name_records_mixed_provenance() {
        assert_eq!(
            bulk_rename_initiator(&[&row_from(EvidenceSource::ImageText)]),
            Initiator::Agent
        );
        assert_eq!(
            bulk_rename_initiator(&[
                &row_from(EvidenceSource::ImageText),
                &row_from(EvidenceSource::Metadata),
            ]),
            Initiator::Agent
        );
        assert_eq!(
            bulk_rename_initiator(&[
                &row_from(EvidenceSource::ImageText),
                &row_from(EvidenceSource::UserEdited),
            ]),
            Initiator::AgentEdited,
            "one retyped name makes the whole batch mixed provenance"
        );
    }

    /// The fingerprint is what apply re-checks each source against just before renaming
    /// it, so every identity field has to survive this mapping. Dropping one widens the
    /// window in which a file that changed since review still gets renamed.
    #[test]
    fn a_local_fingerprint_maps_every_identity_field_apply_rechecks() {
        let fingerprint = RenameSourceFingerprint::Local {
            row_id: "row".into(),
            device: 17,
            inode: 4_242,
            size: 9_001,
            modified_nanos: Some(1_780_000_000_000_000_000),
        };

        assert_eq!(fingerprint_row_id(&fingerprint), "row");
        assert_eq!(
            map_bulk_rename_fingerprint(&fingerprint),
            SourceFingerprint::Local {
                device: 17,
                inode: 4_242,
                content: LocalContent::File {
                    size: 9_001,
                    modified_nanos: Some(1_780_000_000_000_000_000),
                },
            }
        );
    }

    /// A remote source has no inode, so apply identifies it by normalized path plus size
    /// and mtime instead.
    #[test]
    fn a_remote_fingerprint_maps_the_path_size_and_mtime_it_is_identified_by() {
        let fingerprint = RenameSourceFingerprint::Remote {
            row_id: "row".into(),
            normalized_path: "/photos/one.png".into(),
            size: Some(2_048),
            modified: Some(1_780_000_000),
        };

        assert_eq!(fingerprint_row_id(&fingerprint), "row");
        assert_eq!(
            map_bulk_rename_fingerprint(&fingerprint),
            SourceFingerprint::Remote {
                normalized_path: "/photos/one.png".into(),
                content: RemoteContent::File {
                    size: Some(2_048),
                    modified: Some(1_780_000_000),
                },
            }
        );
    }

    /// Absent optional fields must stay absent, never become a placeholder zero: a zero
    /// size or mtime reads as a real value a changed source could match.
    #[test]
    fn absent_fingerprint_fields_stay_absent_rather_than_becoming_zero() {
        assert_eq!(
            map_bulk_rename_fingerprint(&RenameSourceFingerprint::Local {
                row_id: "row".into(),
                device: 1,
                inode: 2,
                size: 3,
                modified_nanos: None,
            }),
            SourceFingerprint::Local {
                device: 1,
                inode: 2,
                content: LocalContent::File {
                    size: 3,
                    modified_nanos: None,
                },
            }
        );
        assert_eq!(
            map_bulk_rename_fingerprint(&RenameSourceFingerprint::Remote {
                row_id: "row".into(),
                normalized_path: "/x/a.png".into(),
                size: None,
                modified: None,
            }),
            SourceFingerprint::Remote {
                normalized_path: "/x/a.png".into(),
                content: RemoteContent::File {
                    size: None,
                    modified: None,
                },
            }
        );
    }

    /// Apply keys its fingerprint lookup by row id, and both variants must answer with
    /// their own. A variant that returned the wrong id would pair a row with another
    /// row's expected fingerprint.
    #[test]
    fn both_fingerprint_variants_report_their_own_row_id() {
        assert_eq!(
            fingerprint_row_id(&RenameSourceFingerprint::Local {
                row_id: "local-row".into(),
                device: 0,
                inode: 0,
                size: 0,
                modified_nanos: None,
            }),
            "local-row"
        );
        assert_eq!(
            fingerprint_row_id(&RenameSourceFingerprint::Remote {
                row_id: "remote-row".into(),
                normalized_path: String::new(),
                size: None,
                modified: None,
            }),
            "remote-row"
        );
    }
}

#[cfg(test)]
mod wire_tests {
    use super::BulkRenameError;

    /// A review that MOVED ON and a review that EXPIRED are different answers:
    /// the dialog re-reads for one and starts over for the other. Flattened into
    /// sentences they were only distinguishable by reading them.
    #[test]
    fn the_two_review_refusals_stay_distinguishable_by_variant() {
        assert_eq!(
            serde_json::to_value(BulkRenameError::ReviewExpired).unwrap(),
            serde_json::json!({ "type": "reviewExpired" })
        );
        assert_eq!(
            serde_json::to_value(BulkRenameError::NeedsAnotherLook).unwrap(),
            serde_json::json!({ "type": "needsAnotherLook" })
        );
        assert_eq!(
            serde_json::to_value(BulkRenameError::RowRefused {
                detail: "that name is taken".to_string()
            })
            .unwrap(),
            serde_json::json!({ "type": "rowRefused", "detail": "that name is taken" })
        );
    }

    /// Stopping a check is the user's own choice, and a refused name, a gone review, and a store
    /// hiccup ask for three different things from them. Flattened, they all read as the first.
    #[test]
    fn each_way_a_check_or_an_edit_can_end_keeps_its_own_variant() {
        use super::{PreflightStopped, ReviseRefusal};

        assert_eq!(
            serde_json::to_value(BulkRenameError::from(PreflightStopped::Cancelled)).unwrap(),
            serde_json::json!({ "type": "cancelled" })
        );
        assert_eq!(
            serde_json::to_value(BulkRenameError::from(PreflightStopped::StoreUnavailable {
                detail: "locked".to_string()
            }))
            .unwrap(),
            serde_json::json!({ "type": "unexpected", "detail": "locked" })
        );
        assert_eq!(
            serde_json::to_value(BulkRenameError::from(ReviseRefusal::NameRefused {
                detail: "bad".to_string()
            }))
            .unwrap(),
            serde_json::json!({ "type": "rowRefused", "detail": "bad" })
        );
        assert_eq!(
            serde_json::to_value(BulkRenameError::from(ReviseRefusal::ReviewOver)).unwrap(),
            serde_json::json!({ "type": "reviewExpired" })
        );
        assert_eq!(
            serde_json::to_value(BulkRenameError::from(ReviseRefusal::StoreUnavailable {
                detail: "locked".to_string()
            }))
            .unwrap(),
            serde_json::json!({ "type": "unexpected", "detail": "locked" })
        );
    }

    /// A batch that wouldn't start says why as a variant, ❌ never a sentence: an
    /// English string here was only ever loggable, never wordable (#184).
    #[test]
    fn a_batch_that_wouldnt_start_carries_a_typed_reason() {
        use cmdr_operations::{RenameStartError, WriteOperationError};

        assert_eq!(
            serde_json::to_value(BulkRenameError::CouldntStart {
                reason: RenameStartError::NothingToRename
            })
            .unwrap(),
            serde_json::json!({ "type": "couldntStart", "reason": { "type": "nothingToRename" } })
        );
        assert_eq!(
            serde_json::to_value(BulkRenameError::CouldntStart {
                reason: RenameStartError::Engine {
                    error: WriteOperationError::SourceNotConnected { path: "/a.png".into() }
                }
            })
            .unwrap(),
            serde_json::json!({
                "type": "couldntStart",
                "reason": { "type": "engine", "error": { "type": "source_not_connected", "path": "/a.png" } }
            })
        );
    }
}
