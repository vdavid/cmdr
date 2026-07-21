//! The `propose_rename_plan` tool. It stages a bounded, cache-validated rename
//! proposal without touching the filesystem.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Manager, Runtime};
use uuid::Uuid;

use crate::agent::llm::types::AgentToolResult;
use crate::file_system::validation::validate_filename;
use crate::ignore_poison::IgnorePoison;
use crate::mcp::pane_state::{PaneFileEntry, PaneState, PaneStateStore};
use crate::mcp::{ToolError, ToolResult};

const MAX_RENAMES: usize = 200;
const PROPOSAL_TTL: Duration = Duration::from_secs(15 * 60);

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RenamePlanInput {
    renames: Vec<RenameInput>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RenameInput {
    source_path: String,
    volume_id: String,
    destination_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenameProposal {
    pub proposal_id: String,
    pub rows: Vec<RenameProposalRow>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenameProposalRow {
    pub row_id: String,
    pub source_path: String,
    pub volume_id: String,
    pub destination_name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameProposalSnapshot {
    pub proposal_id: String,
    pub rows: Vec<RenameProposalRowSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameProposalRowSnapshot {
    pub row_id: String,
    pub source_name: String,
    pub destination_name: String,
}

impl RenameProposal {
    pub fn snapshot(&self) -> RenameProposalSnapshot {
        RenameProposalSnapshot {
            proposal_id: self.proposal_id.clone(),
            rows: self
                .rows
                .iter()
                .map(|row| RenameProposalRowSnapshot {
                    row_id: row.row_id.clone(),
                    source_name: Path::new(&row.source_path)
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or(&row.source_path)
                        .to_string(),
                    destination_name: row.destination_name.clone(),
                })
                .collect(),
        }
    }
}

struct StoredProposal {
    proposal: RenameProposal,
    expires_at: Instant,
    accepted_preflight: Option<AcceptedPreflight>,
}

/// The exact user-approved subset that passed the latest preflight. The apply
/// command consumes this later; the frontend never receives fingerprints.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcceptedPreflight {
    pub allowed_row_ids: Vec<String>,
    pub fingerprints: Vec<RenameSourceFingerprint>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenameSourceFingerprint {
    Local {
        row_id: String,
        device: u64,
        inode: u64,
        size: u64,
        modified_nanos: Option<u128>,
    },
    Remote {
        row_id: String,
        normalized_path: String,
        size: Option<u64>,
        modified: Option<i64>,
    },
}

#[derive(Default)]
pub struct RenameProposalStore {
    proposals: Mutex<HashMap<String, StoredProposal>>,
}

impl RenameProposalStore {
    pub fn stage(&self, proposal: RenameProposal) -> RenameProposalSnapshot {
        let snapshot = proposal.snapshot();
        let mut proposals = self.proposals.lock_ignore_poison();
        proposals.retain(|_, stored| stored.expires_at > Instant::now());
        proposals.insert(
            proposal.proposal_id.clone(),
            StoredProposal {
                proposal,
                expires_at: Instant::now() + PROPOSAL_TTL,
                accepted_preflight: None,
            },
        );
        snapshot
    }

    /// Gets an immutable proposal for repeated review-time checks. Expired
    /// records are removed and indistinguishable from a missing id to callers.
    pub fn get(&self, proposal_id: &str) -> Option<RenameProposal> {
        let mut proposals = self.proposals.lock_ignore_poison();
        let is_live = proposals
            .get(proposal_id)
            .is_some_and(|stored| stored.expires_at > Instant::now());
        if !is_live {
            proposals.remove(proposal_id);
            return None;
        }
        proposals.get(proposal_id).map(|stored| stored.proposal.clone())
    }

    /// Discards a proposal after an explicit user cancellation or terminal apply.
    pub fn consume(&self, proposal_id: &str) -> Option<RenameProposal> {
        let stored = self.proposals.lock_ignore_poison().remove(proposal_id)?;
        (stored.expires_at > Instant::now()).then_some(stored.proposal)
    }

    pub fn record_accepted_preflight(&self, proposal_id: &str, accepted: AcceptedPreflight) -> bool {
        let mut proposals = self.proposals.lock_ignore_poison();
        let Some(stored) = proposals.get_mut(proposal_id) else {
            return false;
        };
        if stored.expires_at <= Instant::now() {
            proposals.remove(proposal_id);
            return false;
        }
        stored.accepted_preflight = Some(accepted);
        true
    }

    pub fn accepted_preflight(&self, proposal_id: &str, allowed_row_ids: &[String]) -> Option<AcceptedPreflight> {
        let proposal = self.get(proposal_id)?;
        let proposals = self.proposals.lock_ignore_poison();
        let stored = proposals.get(&proposal.proposal_id)?;
        let accepted = stored.accepted_preflight.clone()?;
        (accepted.allowed_row_ids == allowed_row_ids).then_some(accepted)
    }

    /// Atomically consumes the exact user-approved subset after a successful
    /// preflight. Once apply begins, the proposal cannot be replayed or altered.
    pub fn take_accepted_preflight(
        &self,
        proposal_id: &str,
        allowed_row_ids: &[String],
    ) -> Option<(RenameProposal, AcceptedPreflight)> {
        let mut proposals = self.proposals.lock_ignore_poison();
        let stored = proposals.get(proposal_id)?;
        if stored.expires_at <= Instant::now() {
            proposals.remove(proposal_id);
            return None;
        }
        if stored
            .accepted_preflight
            .as_ref()
            .is_none_or(|accepted| accepted.allowed_row_ids != allowed_row_ids)
        {
            return None;
        }
        let stored = proposals.remove(proposal_id)?;
        Some((stored.proposal, stored.accepted_preflight?))
    }
}

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

pub struct RenameDispatchOutcome {
    pub result: AgentToolResult,
    pub proposal: Option<RenameProposalSnapshot>,
}

pub fn propose_rename_plan_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": { "renames": { "type": "array", "items": { "type": "object", "properties": {
            "sourcePath": { "type": "string" }, "volumeId": { "type": "string" }, "destinationName": { "type": "string" }
        }, "required": ["sourcePath", "volumeId", "destinationName"], "additionalProperties": false }, "maxItems": MAX_RENAMES } },
        "required": ["renames"], "additionalProperties": false
    })
}

pub async fn dispatch<R: Runtime>(app: &AppHandle<R>, call_id: &str, params: &Value) -> RenameDispatchOutcome {
    match build_proposal(app, params) {
        Ok(proposal) => {
            let snapshot = app.state::<RenameProposalStore>().stage(proposal);
            RenameDispatchOutcome {
                result: AgentToolResult {
                    call_id: call_id.to_string(),
                    content: serde_json::json!({ "readyForReview": true, "count": snapshot.rows.len() }),
                    elided: false,
                },
                proposal: Some(snapshot),
            }
        }
        Err(error) => RenameDispatchOutcome {
            result: AgentToolResult {
                call_id: call_id.to_string(),
                content: serde_json::json!({ "problem": error.message }),
                elided: false,
            },
            proposal: None,
        },
    }
}

pub async fn execute_propose_rename_plan<R: Runtime>(app: &AppHandle<R>, params: &Value) -> ToolResult {
    let outcome = dispatch(app, "registry", params).await;
    Ok(outcome.result.content)
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
    let Some(store) = app.try_state::<RenameProposalStore>() else {
        return expired_preflight();
    };
    let Some(proposal) = store.get(&proposal_id) else {
        return expired_preflight();
    };
    let outcome = if proposal
        .rows
        .first()
        .is_some_and(|row| volume_uses_local_paths(&row.volume_id))
    {
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
    if outcome.status == BulkRenamePreflightStatus::Ready
        && !store.record_accepted_preflight(
            &proposal_id,
            AcceptedPreflight {
                allowed_row_ids,
                fingerprints: outcome.fingerprints,
            },
        )
    {
        return expired_preflight();
    }
    outcome.response
}

struct PreflightOutcome {
    response: BulkRenamePreflight,
    fingerprints: Vec<RenameSourceFingerprint>,
    status: BulkRenamePreflightStatus,
}

fn expired_preflight() -> BulkRenamePreflight {
    BulkRenamePreflight {
        status: BulkRenamePreflightStatus::Expired,
        rows: Vec::new(),
    }
}

fn volume_uses_local_paths(volume_id: &str) -> bool {
    volume_id == "root"
}

fn preflight_local(proposal: &RenameProposal, allowed_row_ids: &[String]) -> PreflightOutcome {
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
    let Some(volume_id) = proposal.rows.first().map(|row| row.volume_id.as_str()) else {
        return finish_preflight(rows, fingerprints);
    };
    let Some(volume) = crate::file_system::get_volume_manager().get(volume_id) else {
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
            normalized_path: crate::indexing::store::normalize_for_comparison(&row.source_path),
            size: source_meta.size,
            modified: source_meta.modified_at.map(|modified| modified as i64),
        });
    }
    mark_cycle_warnings(&allowed, &mut rows);
    finish_preflight(rows, fingerprints)
}

fn initial_rows(proposal: &RenameProposal, allowed_row_ids: &[String]) -> HashMap<String, BulkRenamePreflightRow> {
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

fn rename_warnings(source_path: &str, destination_name: &str) -> Vec<BulkRenameWarning> {
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

fn allowed_rows<'a>(
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

fn mark_duplicate_destinations(rows: &[&RenameProposalRow], statuses: &mut HashMap<String, BulkRenamePreflightRow>) {
    let mut grouped: HashMap<String, Vec<&str>> = HashMap::new();
    for row in rows {
        let destination = Path::new(&row.source_path)
            .parent()
            .unwrap_or(Path::new(""))
            .join(&row.destination_name);
        grouped
            .entry(crate::indexing::store::normalize_for_comparison(
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
fn mark_cycle_warnings(rows: &[&RenameProposalRow], statuses: &mut HashMap<String, BulkRenamePreflightRow>) {
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
            .map(|row| crate::indexing::store::normalize_for_comparison(&row.source_path))
            .collect();
        let free: Vec<&str> = rows
            .iter()
            .filter(|row| remaining.contains(row.row_id.as_str()))
            .filter(|row| {
                let source = crate::indexing::store::normalize_for_comparison(&row.source_path);
                let destination = Path::new(&row.source_path)
                    .parent()
                    .unwrap_or(Path::new(""))
                    .join(&row.destination_name);
                let destination = crate::indexing::store::normalize_for_comparison(&destination.to_string_lossy());
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
fn same_local_file(left: &std::fs::Metadata, right: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    left.dev() == right.dev() && left.ino() == right.ino()
}

#[cfg(not(unix))]
fn same_local_file(_left: &std::fs::Metadata, _right: &std::fs::Metadata) -> bool {
    false
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

fn build_proposal<R: Runtime>(app: &AppHandle<R>, params: &Value) -> Result<RenameProposal, ToolError> {
    let input: RenamePlanInput = serde_json::from_value(params.clone()).map_err(|_| {
        ToolError::invalid_params("Provide a rename plan with sourcePath, volumeId, and destinationName for every row.")
    })?;
    if input.renames.is_empty() {
        return Err(ToolError::invalid_params("A rename plan needs at least one row."));
    }
    if input.renames.len() > MAX_RENAMES {
        return Err(ToolError::invalid_params(
            "A rename plan can contain up to 200 items.".to_string(),
        ));
    }
    let store = app
        .try_state::<PaneStateStore>()
        .ok_or_else(|| ToolError::internal("Pane state isn't available yet"))?;
    let state = focused_state(&store);
    let volume_id = state
        .volume_id
        .clone()
        .ok_or_else(|| ToolError::invalid_params("The focused pane has no volume id yet."))?;
    let scope = scoped_files(&state)?;
    let mut source_paths = HashSet::new();
    let mut destination_names = HashSet::new();
    let mut rows = Vec::with_capacity(input.renames.len());
    for rename in input.renames {
        if rename.volume_id != volume_id {
            return Err(ToolError::invalid_params(
                "Every rename must use the focused pane's volume id.",
            ));
        }
        if !source_paths.insert(rename.source_path.clone()) {
            return Err(ToolError::invalid_params(
                "A source file can appear only once in a rename plan.",
            ));
        }
        validate_destination_name(&rename.destination_name)?;
        let destination_key = crate::indexing::store::normalize_for_comparison(&rename.destination_name);
        if !destination_names.insert(destination_key) {
            return Err(ToolError::invalid_params(
                "Destination names must be unique on this volume.",
            ));
        }
        if let Some(entry) = scope.get(rename.source_path.as_str()) {
            if entry.is_directory {
                return Err(ToolError::invalid_params(
                    "Rename plans can contain files, not folders.",
                ));
            }
        } else if !missing_local_child(&state, &volume_id, &rename.source_path) {
            return Err(ToolError::invalid_params(
                "Every source must be in the focused pane's effective scope.",
            ));
        }
        if crate::file_system::volume::backends::archive::archive_boundary_candidate(Path::new(&rename.source_path))
            .is_some()
        {
            return Err(ToolError::invalid_params(
                "Rename plans can't include files inside an archive.",
            ));
        }
        rows.push(RenameProposalRow {
            row_id: Uuid::new_v4().to_string(),
            source_path: rename.source_path,
            volume_id: volume_id.clone(),
            destination_name: rename.destination_name,
        });
    }
    Ok(RenameProposal {
        proposal_id: Uuid::new_v4().to_string(),
        rows,
    })
}

/// A model may invent a filename that is not in the pane cache. Keep that row
/// reviewable only when it names a nonexistent direct child of the focused local
/// folder; preflight then reports `SourceMissing`. Existing out-of-scope files and
/// every remote path stay rejected at the proposal boundary.
fn missing_local_child(state: &PaneState, volume_id: &str, source_path: &str) -> bool {
    if !volume_uses_local_paths(volume_id) || std::fs::symlink_metadata(source_path).is_ok() {
        return false;
    }
    let source = Path::new(source_path);
    source.parent() == Some(Path::new(&state.path)) && source.file_name().is_some()
}

fn focused_state(store: &PaneStateStore) -> PaneState {
    if store.get_focused_pane() == "right" {
        store.get_right()
    } else {
        store.get_left()
    }
}

fn scoped_files(state: &PaneState) -> Result<HashMap<&str, &PaneFileEntry>, ToolError> {
    let indexes: Vec<usize> = if state.selected_indices.is_empty() {
        (0..state.files.len()).collect()
    } else {
        state.selected_indices.clone()
    };
    let mut files = HashMap::with_capacity(indexes.len());
    for index in indexes {
        let entry = state.files.get(index).ok_or_else(|| {
            ToolError::invalid_params(
                "The selected files are not fully loaded. Ask the user to narrow or reload the selection.",
            )
        })?;
        files.insert(entry.path.as_str(), entry);
    }
    Ok(files)
}

fn validate_destination_name(name: &str) -> Result<(), ToolError> {
    if name == "." || name == ".." || name.contains('/') || name.contains('\\') {
        return Err(ToolError::invalid_params(
            "Each destinationName must be one filename, not a path.",
        ));
    }
    validate_filename(name).map_err(|_| ToolError::invalid_params("Each destinationName must be a valid filename."))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cycle_warnings_mark_only_closed_dependency_components() {
        let proposal = RenameProposal {
            proposal_id: "proposal".into(),
            rows: vec![
                proposal_row("chain-a", "/x/a", "b"),
                proposal_row("chain-b", "/x/b", "free"),
                proposal_row("cycle-a", "/x/c", "d"),
                proposal_row("cycle-b", "/x/d", "c"),
            ],
        };
        let allowed_ids: Vec<String> = proposal.rows.iter().map(|row| row.row_id.clone()).collect();
        let mut statuses = initial_rows(&proposal, &allowed_ids);
        let allowed = allowed_rows(&proposal, &allowed_ids, &mut statuses);

        mark_cycle_warnings(&allowed, &mut statuses);

        assert!(statuses["chain-a"].warnings.is_empty());
        assert!(statuses["chain-b"].warnings.is_empty());
        assert_eq!(statuses["cycle-a"].warnings, vec![BulkRenameWarning::Cycle]);
        assert_eq!(statuses["cycle-b"].warnings, vec![BulkRenameWarning::Cycle]);
    }

    fn proposal_row(row_id: &str, source_path: &str, destination_name: &str) -> RenameProposalRow {
        RenameProposalRow {
            row_id: row_id.into(),
            source_path: source_path.into(),
            volume_id: "root".into(),
            destination_name: destination_name.into(),
        }
    }

    #[test]
    fn extension_warnings_cover_changes_additions_removals_and_filename_edges() {
        for (source, destination) in [
            ("photo.png", "photo.jpg"),
            ("photo.png", "photo"),
            ("README", "README.md"),
            (".env", ".env.txt"),
            ("archive.tar.gz", "archive.tar.zip"),
            ("trailing.", "trailing"),
        ] {
            assert_eq!(
                rename_warnings(source, destination),
                vec![BulkRenameWarning::ExtensionChanged],
                "expected an extension warning for {source:?} -> {destination:?}"
            );
        }

        for (source, destination) in [
            ("photo.png", "renamed.png"),
            ("photo.PNG", "renamed.png"),
            (".env", ".config"),
            ("archive.tar.gz", "renamed.gz"),
        ] {
            assert!(
                rename_warnings(source, destination).is_empty(),
                "did not expect an extension warning for {source:?} -> {destination:?}"
            );
        }
    }

    #[test]
    fn local_preflight_blocks_a_source_that_no_longer_exists() {
        let temp = tempfile::tempdir().expect("temp directory");
        let missing = temp.path().join("missing.png");
        let proposal = RenameProposal {
            proposal_id: "proposal".into(),
            rows: vec![proposal_row(
                "row",
                missing.to_str().expect("UTF-8 temp path"),
                "renamed.png",
            )],
        };

        let outcome = preflight_local(&proposal, &["row".into()]);

        assert_eq!(outcome.status, BulkRenamePreflightStatus::Blocked);
        assert_eq!(
            outcome.response.rows[0].reason,
            Some(BulkRenameBlockReason::SourceMissing)
        );
        assert!(outcome.fingerprints.is_empty());
    }

    #[test]
    fn only_a_missing_direct_child_can_enter_review_without_a_pane_entry() {
        let temp = tempfile::tempdir().expect("temp directory");
        let state = PaneState {
            path: temp.path().to_string_lossy().into_owned(),
            ..PaneState::default()
        };
        let missing = temp.path().join("imagined.png");
        let nested = temp.path().join("nested").join("imagined.png");
        let existing = temp.path().join("existing.png");
        std::fs::write(&existing, b"present").expect("write fixture");

        assert!(missing_local_child(
            &state,
            "root",
            missing.to_str().expect("UTF-8 path")
        ));
        assert!(!missing_local_child(
            &state,
            "root",
            nested.to_str().expect("UTF-8 path")
        ));
        assert!(!missing_local_child(
            &state,
            "root",
            existing.to_str().expect("UTF-8 path")
        ));
        assert!(!missing_local_child(
            &state,
            "mtp-device",
            missing.to_str().expect("UTF-8 path")
        ));
    }

    #[test]
    fn destination_names_reject_paths_and_dot_entries() {
        for name in ["", ".", "..", "folder/name.png", "folder\\name.png"] {
            assert!(validate_destination_name(name).is_err(), "{name}");
        }
        assert!(validate_destination_name("2026-07-20 - Receipt.png").is_ok());
    }
    #[test]
    fn store_returns_an_immutable_snapshot_and_consumes_once() {
        let store = RenameProposalStore::default();
        let proposal = RenameProposal {
            proposal_id: "proposal".into(),
            rows: vec![RenameProposalRow {
                row_id: "row".into(),
                source_path: "/x/a.png".into(),
                volume_id: "root".into(),
                destination_name: "b.png".into(),
            }],
        };
        let snapshot = store.stage(proposal);
        assert_eq!(snapshot.rows[0].source_name, "a.png");
        assert!(store.get("proposal").is_some());
        assert!(store.get("proposal").is_some());
        assert!(store.consume("proposal").is_some());
        assert!(store.get("proposal").is_none());
    }

    #[test]
    fn current_folder_entries_are_a_valid_rename_scope_while_listing_updates() {
        let state = PaneState {
            total_files: 2,
            files: vec![PaneFileEntry {
                name: "a.png".into(),
                path: "/ignored/a.png".into(),
                is_directory: false,
                size: None,
                recursive_size: None,
                modified: None,
                recursive_size_pending: None,
                tags: vec![],
            }],
            ..Default::default()
        };
        assert_eq!(scoped_files(&state).expect("current entries are usable").len(), 1);
    }

    #[test]
    fn duplicate_final_targets_block_every_row_in_the_group() {
        let first = RenameProposalRow {
            row_id: "first".into(),
            source_path: "/ignored/a.png".into(),
            volume_id: "root".into(),
            destination_name: "same.png".into(),
        };
        let second = RenameProposalRow {
            row_id: "second".into(),
            source_path: "/ignored/b.png".into(),
            volume_id: "root".into(),
            destination_name: "same.png".into(),
        };
        let mut statuses = initial_rows(
            &RenameProposal {
                proposal_id: "proposal".into(),
                rows: vec![first.clone(), second.clone()],
            },
            &["first".into(), "second".into()],
        );
        mark_duplicate_destinations(&[&first, &second], &mut statuses);
        assert!(
            statuses
                .values()
                .all(|row| row.reason == Some(BulkRenameBlockReason::DuplicateDestination))
        );
    }

    #[test]
    fn accepted_preflight_requires_the_exact_allowed_subset() {
        let store = RenameProposalStore::default();
        store.stage(RenameProposal {
            proposal_id: "proposal".into(),
            rows: vec![],
        });
        assert!(store.record_accepted_preflight(
            "proposal",
            AcceptedPreflight {
                allowed_row_ids: vec!["row".into()],
                fingerprints: vec![],
            },
        ));
        assert!(store.accepted_preflight("proposal", &["row".into()]).is_some());
        assert!(store.accepted_preflight("proposal", &["other".into()]).is_none());
    }
}
