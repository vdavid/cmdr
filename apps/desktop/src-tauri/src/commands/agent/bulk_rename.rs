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
    for row_id in &allowed_row_ids {
        let Some(proposal_row) = proposal.rows.iter().find(|row| &row.row_id == row_id) else {
            return Err(IpcError::from_err("Review the rename plan again before applying it."));
        };
        let Some(fingerprint) = fingerprints.get(row_id) else {
            return Err(IpcError::from_err("Review the rename plan again before applying it."));
        };
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

    crate::file_system::write_operations::start_bulk_rename(
        Arc::new(crate::file_system::write_operations::TauriEventSink::new(app)),
        volume_id,
        rows,
        crate::operation_log::types::Initiator::Agent,
    )
    .map_err(IpcError::from_err)
}

fn fingerprint_row_id(fingerprint: &RenameSourceFingerprint) -> &str {
    match fingerprint {
        RenameSourceFingerprint::Local { row_id, .. } | RenameSourceFingerprint::Remote { row_id, .. } => row_id,
    }
}

fn map_bulk_rename_fingerprint(
    fingerprint: &RenameSourceFingerprint,
) -> crate::file_system::write_operations::BulkRenameFingerprint {
    match fingerprint {
        RenameSourceFingerprint::Local {
            device,
            inode,
            size,
            modified_nanos,
            ..
        } => crate::file_system::write_operations::BulkRenameFingerprint::Local {
            device: *device,
            inode: *inode,
            size: *size,
            modified_nanos: *modified_nanos,
        },
        RenameSourceFingerprint::Remote {
            normalized_path,
            size,
            modified,
            ..
        } => crate::file_system::write_operations::BulkRenameFingerprint::Remote {
            normalized_path: normalized_path.clone(),
            size: *size,
            modified: *modified,
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
