//! Review and apply a server-owned rename proposal.
//!
//! Paths and destination names never cross this IPC boundary: the frontend submits only
//! opaque row ids, and this layer looks the real work up in `RenameProposalStore`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use tauri::{AppHandle, Manager};

use crate::agent::tools::propose::rename::{
    BulkRenamePreflight, BulkRenamePreflightStatus, RenameProposalStore, RenameSourceFingerprint,
};
use crate::commands::util::IpcError;
use crate::file_system::write_operations::{LocalContent, RemoteContent, SourceFingerprint};

const BULK_RENAME_PREFLIGHT_TIMEOUT: Duration = Duration::from_secs(5);
const BULK_RENAME_APPLY_TIMEOUT: Duration = Duration::from_secs(5);

/// Revalidates the user-selected subset of a server-owned rename proposal. The
/// frontend supplies opaque ids only, never source paths or destination names.
#[tauri::command]
#[specta::specta]
pub async fn preflight_bulk_rename(
    app: AppHandle,
    proposal_id: String,
    allowed_row_ids: Vec<String>,
) -> Result<BulkRenamePreflight, IpcError> {
    tokio::time::timeout(
        BULK_RENAME_PREFLIGHT_TIMEOUT,
        crate::agent::tools::propose::rename::preflight(&app, proposal_id, allowed_row_ids),
    )
    .await
    .map_err(|_| IpcError::timeout())
}

/// Starts the user-approved subset of a server-owned rename plan. Paths and
/// names never cross this IPC boundary: the frontend submits only opaque ids.
#[tauri::command]
#[specta::specta]
pub async fn apply_bulk_rename(
    app: AppHandle,
    proposal_id: String,
    allowed_row_ids: Vec<String>,
) -> Result<crate::file_system::write_operations::WriteOperationStartResult, IpcError> {
    let Some(store) = app.try_state::<RenameProposalStore>() else {
        return Err(IpcError::from_err(
            "This rename review has expired. Ask Cmdr to prepare it again.",
        ));
    };

    // The normal dialog path always arrives with this exact preflight. A stale
    // client retries the bounded authoritative preflight instead of trusting old
    // rows or accepting a different subset.
    if store.accepted_preflight(&proposal_id, &allowed_row_ids).is_none() {
        let preflight = tokio::time::timeout(
            BULK_RENAME_APPLY_TIMEOUT,
            crate::agent::tools::propose::rename::preflight(&app, proposal_id.clone(), allowed_row_ids.clone()),
        )
        .await
        .map_err(|_| IpcError::timeout())?;
        if preflight.status != BulkRenamePreflightStatus::Ready {
            return Err(IpcError::from_err("Review the rename plan again before applying it."));
        }
    }

    let Some((proposal, accepted)) = store.take_accepted_preflight(&proposal_id, &allowed_row_ids) else {
        return Err(IpcError::from_err(
            "This rename review has expired. Ask Cmdr to prepare it again.",
        ));
    };
    let Some(volume_id) = proposal.rows.first().map(|row| row.volume_id.clone()) else {
        return Err(IpcError::from_err("This rename plan has no rows to apply."));
    };
    if proposal.rows.iter().any(|row| row.volume_id != volume_id) {
        return Err(IpcError::from_err("A rename plan must stay on one volume."));
    }

    let fingerprints: HashMap<_, _> = accepted
        .fingerprints
        .into_iter()
        .map(|fingerprint| (fingerprint_row_id(&fingerprint).to_string(), fingerprint))
        .collect();
    let mut rows = Vec::with_capacity(allowed_row_ids.len());
    let mut applied_rows = Vec::with_capacity(allowed_row_ids.len());
    for row_id in &allowed_row_ids {
        let Some(proposal_row) = proposal.rows.iter().find(|row| &row.row_id == row_id) else {
            return Err(IpcError::from_err("Review the rename plan again before applying it."));
        };
        let Some(fingerprint) = fingerprints.get(row_id) else {
            return Err(IpcError::from_err("Review the rename plan again before applying it."));
        };
        applied_rows.push(proposal_row);
        rows.push(crate::file_system::write_operations::BulkRenameRow {
            row_id: row_id.clone(),
            source: PathBuf::from(&proposal_row.source_path),
            destination: Path::new(&proposal_row.source_path)
                .parent()
                .unwrap_or_else(|| Path::new(""))
                .join(&proposal_row.destination_name),
            expected_fingerprint: map_bulk_rename_fingerprint(fingerprint),
        });
    }

    let initiator = bulk_rename_initiator(&applied_rows);
    crate::file_system::write_operations::start_bulk_rename(
        Arc::new(crate::file_system::write_operations::TauriEventSink::new(app)),
        volume_id,
        rows,
        initiator,
    )
    .map_err(IpcError::from_err)
}

/// Replaces one row's proposed name with the one the user typed in the review, and answers the
/// row as the dialog should now show it. The name is validated server-side; the row keeps no
/// evidence afterwards, and the edit invalidates the accepted preflight, so the new name is
/// rechecked before it can reach the filesystem.
#[tauri::command]
#[specta::specta]
pub fn revise_bulk_rename_row(
    app: AppHandle,
    proposal_id: String,
    row_id: String,
    destination_name: String,
) -> Result<crate::agent::tools::propose::rename::RenameProposalRowSnapshot, IpcError> {
    crate::agent::tools::propose::rename::revise_row(&app, &proposal_id, &row_id, &destination_name)
        .map_err(|error| IpcError::from_err(error.message))
}

/// Who the operation log credits for a batch. The agent proposed it, but a row the user
/// retyped in the review was the user's own choice, so a batch carrying one is mixed
/// provenance rather than the agent's work.
fn bulk_rename_initiator(
    rows: &[&crate::agent::tools::propose::rename::RenameProposalRow],
) -> crate::operation_log::types::Initiator {
    let user_edited = rows
        .iter()
        .any(|row| row.evidence.source == crate::agent::tools::propose::evidence::EvidenceSource::UserEdited);
    if user_edited {
        crate::operation_log::types::Initiator::AgentEdited
    } else {
        crate::operation_log::types::Initiator::Agent
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

/// Discards a staged proposal after the user closes its review. There is no
/// agent-controlled approval route: only this user action consumes the plan.
#[tauri::command]
#[specta::specta]
pub fn cancel_bulk_rename_proposal(app: AppHandle, proposal_id: String) {
    if let Some(store) = app.try_state::<RenameProposalStore>() {
        store.consume(&proposal_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::tools::propose::evidence::{EvidenceSource, RenameEvidence};
    use crate::agent::tools::propose::rename::RenameProposalRow;
    use crate::operation_log::types::Initiator;

    fn row_from(source: EvidenceSource) -> RenameProposalRow {
        RenameProposalRow {
            row_id: "row".into(),
            source_path: "/shots/one.png".into(),
            volume_id: "root".into(),
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
