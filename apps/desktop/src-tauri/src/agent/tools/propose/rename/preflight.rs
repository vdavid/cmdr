//! Review-time revalidation of the subset the user currently allows.
//!
//! The caller sends opaque row ids only; paths and destination names stay in the proposal
//! store. Each allowed row is re-checked against the live source (locally by
//! `symlink_metadata`, remotely through the volume backend), duplicate destinations and
//! closed rename cycles are marked, and an acceptance is recorded only when every allowed row
//! is safe to apply.
//!
//! A Ready preflight is what records the user's answer: the ops the review left out become
//! `excluded` rows and the spine writes a server-owned acceptance record over what's left.
//! The fingerprints that pair with it stay in this process ([`AcceptedRenamePreflights`]), so
//! a restart forces a fresh preflight rather than reviving an approval given before it.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::{AppHandle, Manager, Runtime};

use super::store::{
    AcceptedPreflight, AcceptedRenamePreflights, RenameProposal, RenameProposalRow, RenameSourceFingerprint,
};
use crate::agent::AgentDb;
use crate::agent::store::proposals::AcceptanceOutcome;
use crate::file_system::write_operations::same_local_file;

/// A row's user-action-time validation result. It deliberately contains no
/// path or destination authority: the frontend retains only opaque row ids.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct BulkRenamePreflight {
    pub status: BulkRenamePreflightStatus,
    pub rows: Vec<BulkRenamePreflightRow>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum BulkRenamePreflightStatus {
    Ready,
    Blocked,
    Expired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct BulkRenamePreflightRow {
    pub row_id: String,
    pub status: BulkRenameRowStatus,
    pub reason: Option<BulkRenameBlockReason>,
    pub warnings: Vec<BulkRenameWarning>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum BulkRenameRowStatus {
    Ready,
    Blocked,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
#[derive(PartialEq, Eq)]
pub enum BulkRenameBlockReason {
    UnknownRow,
    DuplicateDestination,
    SourceMissing,
    SourceChanged,
    TargetExists,
    VolumeUnavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum BulkRenameWarning {
    ExtensionChanged,
    Cycle,
}

/// Revalidates the server-owned subset a user currently allows. The caller may
/// send opaque row ids only; paths and destination names remain in the proposal
/// store. This performs no mutation and records fingerprints only when every
/// allowed row is safe to apply.
pub async fn preflight<R: Runtime>(
    app: &AppHandle<R>,
    proposal_id: String,
    allowed_row_ids: Vec<String>,
) -> BulkRenamePreflight {
    let (Some(db), Some(accepted_preflights)) =
        (app.try_state::<AgentDb>(), app.try_state::<AcceptedRenamePreflights>())
    else {
        return expired_preflight();
    };
    let Ok(conn) = db.open_write_connection() else {
        return expired_preflight();
    };
    let Ok(Some(proposal)) = super::store::load(&conn, &proposal_id) else {
        return expired_preflight();
    };
    let outcome = if volume_uses_local_paths(&proposal.volume_id) {
        let blocking_proposal = proposal.clone();
        let blocking_allowed_row_ids = allowed_row_ids.clone();
        match tokio::task::spawn_blocking(move || preflight_local(&blocking_proposal, &blocking_allowed_row_ids)).await
        {
            Ok(outcome) => outcome,
            Err(_) => unavailable_preflight(&proposal, &allowed_row_ids),
        }
    } else {
        preflight_remote(&proposal, &allowed_row_ids).await
    };
    if outcome.status != BulkRenamePreflightStatus::Ready {
        return outcome.response;
    }
    match record_user_acceptance(&conn, &proposal, &allowed_row_ids) {
        Ok(true) => {}
        Ok(false) | Err(_) => return expired_preflight(),
    }
    accepted_preflights.record(
        &proposal_id,
        AcceptedPreflight {
            allowed_row_ids,
            fingerprints: outcome.fingerprints,
        },
    );
    outcome.response
}

/// Record what the user allowed, as the spine's server-owned acceptance record: every op the
/// review left out becomes `excluded`, and the digest over what's left is what the claim
/// transaction binds against at apply time.
///
/// The client presented ids, never values — so the digest (each live op's id, source path, and
/// destination NAME) is the half of the binding that says which names were checked. A revise
/// changes it, and the claim then refuses rather than applying a name preflight never saw.
fn record_user_acceptance(
    conn: &rusqlite::Connection,
    proposal: &RenameProposal,
    allowed_row_ids: &[String],
) -> Result<bool, crate::agent::store::AgentStoreError> {
    let Some(group_id) = super::store::numeric_id(&proposal.proposal_id) else {
        return Ok(false);
    };
    let deselected: Vec<i64> = proposal
        .rows
        .iter()
        .filter(|row| !allowed_row_ids.contains(&row.row_id))
        .filter_map(|row| super::store::numeric_id(&row.row_id))
        .collect();
    Ok(matches!(
        crate::agent::store::proposals::record_acceptance(conn, group_id, &deselected, now_secs())?,
        AcceptanceOutcome::Accepted { .. }
    ))
}

fn now_secs() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

pub(super) struct PreflightOutcome {
    pub response: BulkRenamePreflight,
    pub fingerprints: Vec<RenameSourceFingerprint>,
    pub status: BulkRenamePreflightStatus,
}

fn expired_preflight() -> BulkRenamePreflight {
    BulkRenamePreflight {
        status: BulkRenamePreflightStatus::Expired,
        rows: Vec::new(),
    }
}

pub(super) fn volume_uses_local_paths(volume_id: &str) -> bool {
    volume_id == "root"
}

pub(super) fn preflight_local(proposal: &RenameProposal, allowed_row_ids: &[String]) -> PreflightOutcome {
    let mut rows = initial_rows(proposal, allowed_row_ids);
    let allowed = allowed_rows(proposal, allowed_row_ids, &mut rows);
    mark_duplicate_destinations(&allowed, &mut rows);
    let allowed_sources: HashSet<&str> = allowed.iter().map(|row| row.source_path.as_str()).collect();
    let mut fingerprints = Vec::new();

    for row in &allowed {
        let Some(status) = rows.get_mut(&row.row_id) else {
            continue;
        };
        if status.status == BulkRenameRowStatus::Blocked {
            continue;
        }
        let source = PathBuf::from(&row.source_path);
        let source_meta = match std::fs::symlink_metadata(&source) {
            Ok(metadata) if !metadata.file_type().is_dir() => metadata,
            _ => {
                block(status, BulkRenameBlockReason::SourceMissing);
                continue;
            }
        };
        let destination = source.parent().unwrap_or(Path::new("")).join(&row.destination_name);
        if !allowed_sources.contains(destination.to_string_lossy().as_ref()) {
            match std::fs::symlink_metadata(&destination) {
                Ok(destination_meta) if !same_local_file(&source_meta, &destination_meta) => {
                    block(status, BulkRenameBlockReason::TargetExists);
                    continue;
                }
                Ok(_) | Err(_) => {}
            }
        }
        fingerprints.push(local_fingerprint(&row.row_id, &source_meta));
    }
    mark_cycle_warnings(&allowed, &mut rows);
    finish_preflight(rows, fingerprints)
}

async fn preflight_remote(proposal: &RenameProposal, allowed_row_ids: &[String]) -> PreflightOutcome {
    let mut rows = initial_rows(proposal, allowed_row_ids);
    let allowed = allowed_rows(proposal, allowed_row_ids, &mut rows);
    mark_duplicate_destinations(&allowed, &mut rows);
    let allowed_sources: HashSet<&str> = allowed.iter().map(|row| row.source_path.as_str()).collect();
    let mut fingerprints = Vec::new();
    let Some(volume) = crate::file_system::volume::manager::get_volume_manager().get(&proposal.volume_id) else {
        for status in rows.values_mut() {
            if status.status == BulkRenameRowStatus::Ready {
                block(status, BulkRenameBlockReason::VolumeUnavailable);
            }
        }
        return finish_preflight(rows, fingerprints);
    };

    for row in &allowed {
        let Some(status) = rows.get_mut(&row.row_id) else {
            continue;
        };
        if status.status == BulkRenameRowStatus::Blocked {
            continue;
        }
        let source = Path::new(&row.source_path);
        let source_meta = match volume.get_metadata(source).await {
            Ok(metadata) if !metadata.is_directory => metadata,
            _ => {
                block(status, BulkRenameBlockReason::SourceMissing);
                continue;
            }
        };
        let destination = source.parent().unwrap_or(Path::new("")).join(&row.destination_name);
        if !allowed_sources.contains(destination.to_string_lossy().as_ref())
            && volume.get_metadata(&destination).await.is_ok()
        {
            block(status, BulkRenameBlockReason::TargetExists);
            continue;
        }
        fingerprints.push(RenameSourceFingerprint::Remote {
            row_id: row.row_id.clone(),
            normalized_path: cmdr_index::store::normalize_for_comparison(&row.source_path),
            size: source_meta.size,
            modified: source_meta.modified_at.map(|modified| modified as i64),
        });
    }
    mark_cycle_warnings(&allowed, &mut rows);
    finish_preflight(rows, fingerprints)
}

pub(super) fn initial_rows(
    proposal: &RenameProposal,
    allowed_row_ids: &[String],
) -> HashMap<String, BulkRenamePreflightRow> {
    let known: HashSet<&str> = proposal.rows.iter().map(|row| row.row_id.as_str()).collect();
    allowed_row_ids
        .iter()
        .map(|row_id| {
            let row = BulkRenamePreflightRow {
                row_id: row_id.clone(),
                status: if known.contains(row_id.as_str()) {
                    BulkRenameRowStatus::Ready
                } else {
                    BulkRenameRowStatus::Blocked
                },
                reason: (!known.contains(row_id.as_str())).then_some(BulkRenameBlockReason::UnknownRow),
                warnings: proposal
                    .rows
                    .iter()
                    .find(|row| row.row_id == *row_id)
                    .map_or_else(Vec::new, |row| rename_warnings(&row.source_path, &row.destination_name)),
            };
            (row_id.clone(), row)
        })
        .collect()
}

pub(super) fn rename_warnings(source_path: &str, destination_name: &str) -> Vec<BulkRenameWarning> {
    let source_name = Path::new(source_path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(source_path);
    if extensions_match(source_name, destination_name) {
        Vec::new()
    } else {
        vec![BulkRenameWarning::ExtensionChanged]
    }
}

fn extensions_match(source_name: &str, destination_name: &str) -> bool {
    match (
        Path::new(source_name).extension(),
        Path::new(destination_name).extension(),
    ) {
        (Some(source), Some(destination)) => {
            let (Some(source), Some(destination)) = (source.to_str(), destination.to_str()) else {
                return source == destination;
            };
            source.eq_ignore_ascii_case(destination)
        }
        (None, None) => true,
        (Some(_), None) | (None, Some(_)) => false,
    }
}

pub(super) fn allowed_rows<'a>(
    proposal: &'a RenameProposal,
    allowed_row_ids: &[String],
    statuses: &mut HashMap<String, BulkRenamePreflightRow>,
) -> Vec<&'a RenameProposalRow> {
    let mut seen = HashSet::new();
    allowed_row_ids
        .iter()
        .filter_map(|row_id| {
            if !seen.insert(row_id.as_str()) {
                if let Some(status) = statuses.get_mut(row_id) {
                    block(status, BulkRenameBlockReason::UnknownRow);
                }
                return None;
            }
            proposal.rows.iter().find(|row| row.row_id == *row_id)
        })
        .collect()
}

pub(super) fn mark_duplicate_destinations(
    rows: &[&RenameProposalRow],
    statuses: &mut HashMap<String, BulkRenamePreflightRow>,
) {
    let mut grouped: HashMap<String, Vec<&str>> = HashMap::new();
    for row in rows {
        let destination = Path::new(&row.source_path)
            .parent()
            .unwrap_or(Path::new(""))
            .join(&row.destination_name);
        grouped
            .entry(cmdr_index::store::normalize_for_comparison(
                &destination.to_string_lossy(),
            ))
            .or_default()
            .push(&row.row_id);
    }
    for row_ids in grouped.values().filter(|row_ids| row_ids.len() > 1) {
        for row_id in row_ids {
            if let Some(status) = statuses.get_mut(*row_id) {
                block(status, BulkRenameBlockReason::DuplicateDestination);
            }
        }
    }
}

/// Marks the rows left after repeatedly peeling free destinations. Preflight
/// has already rejected duplicate destinations, so every remaining component
/// is a closed rename cycle. Case-only self-edges are staging requirements, not
/// multi-file cycles, and get no cycle warning.
pub(super) fn mark_cycle_warnings(rows: &[&RenameProposalRow], statuses: &mut HashMap<String, BulkRenamePreflightRow>) {
    let mut remaining: HashSet<&str> = rows
        .iter()
        .filter(|row| {
            statuses
                .get(&row.row_id)
                .is_some_and(|status| status.status == BulkRenameRowStatus::Ready)
        })
        .map(|row| row.row_id.as_str())
        .collect();
    loop {
        let source_keys: HashSet<String> = rows
            .iter()
            .filter(|row| remaining.contains(row.row_id.as_str()))
            .map(|row| cmdr_index::store::normalize_for_comparison(&row.source_path))
            .collect();
        let free: Vec<&str> = rows
            .iter()
            .filter(|row| remaining.contains(row.row_id.as_str()))
            .filter(|row| {
                let source = cmdr_index::store::normalize_for_comparison(&row.source_path);
                let destination = Path::new(&row.source_path)
                    .parent()
                    .unwrap_or(Path::new(""))
                    .join(&row.destination_name);
                let destination = cmdr_index::store::normalize_for_comparison(&destination.to_string_lossy());
                source == destination || !source_keys.contains(&destination)
            })
            .map(|row| row.row_id.as_str())
            .collect();
        if free.is_empty() {
            break;
        }
        for row_id in free {
            remaining.remove(row_id);
        }
    }
    for row_id in remaining {
        if let Some(status) = statuses.get_mut(row_id)
            && !status.warnings.contains(&BulkRenameWarning::Cycle)
        {
            status.warnings.push(BulkRenameWarning::Cycle);
        }
    }
}

fn block(row: &mut BulkRenamePreflightRow, reason: BulkRenameBlockReason) {
    row.status = BulkRenameRowStatus::Blocked;
    row.reason = Some(reason);
}

fn finish_preflight(
    rows: HashMap<String, BulkRenamePreflightRow>,
    fingerprints: Vec<RenameSourceFingerprint>,
) -> PreflightOutcome {
    let mut rows: Vec<_> = rows.into_values().collect();
    rows.sort_unstable_by(|a, b| a.row_id.cmp(&b.row_id));
    let status = if rows.iter().any(|row| row.status == BulkRenameRowStatus::Blocked) {
        BulkRenamePreflightStatus::Blocked
    } else {
        BulkRenamePreflightStatus::Ready
    };
    PreflightOutcome {
        response: BulkRenamePreflight {
            status: status.clone(),
            rows,
        },
        fingerprints,
        status,
    }
}

fn unavailable_preflight(proposal: &RenameProposal, allowed_row_ids: &[String]) -> PreflightOutcome {
    let mut rows = initial_rows(proposal, allowed_row_ids);
    for row in rows.values_mut() {
        if row.status == BulkRenameRowStatus::Ready {
            block(row, BulkRenameBlockReason::VolumeUnavailable);
        }
    }
    finish_preflight(rows, Vec::new())
}

#[cfg(unix)]
fn local_fingerprint(row_id: &str, metadata: &std::fs::Metadata) -> RenameSourceFingerprint {
    use std::os::unix::fs::MetadataExt;
    RenameSourceFingerprint::Local {
        row_id: row_id.to_string(),
        device: metadata.dev(),
        inode: metadata.ino(),
        size: metadata.len(),
        modified_nanos: metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|time| time.as_nanos()),
    }
}

#[cfg(not(unix))]
fn local_fingerprint(row_id: &str, metadata: &std::fs::Metadata) -> RenameSourceFingerprint {
    RenameSourceFingerprint::Local {
        row_id: row_id.to_string(),
        device: 0,
        inode: 0,
        size: metadata.len(),
        modified_nanos: metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|time| time.as_nanos()),
    }
}
