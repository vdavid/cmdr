//! The `important_folders` and `folder_importance` agent tools.
//!
//! Both REUSE the shipped `cmdr://importance` snapshot core
//! (`tools::resources::importance::snapshot_*`), which already handles the headline
//! behaviors: answering OFFLINE for an unmounted-but-scored drive (the importance
//! stores outlive the mount) and the floored-vs-unscored derivation. This file
//! places each call on volumes through `crate::location`'s resolver, translates
//! between app paths and each store's own path space through
//! `crate::location::store_path` (shared with the visit recorder), shapes the typed snapshots into the
//! agent's JSON, and adds the staleness comparison (`asOfGeneration` vs the
//! volume's current `recomputeGeneration`, spec §2.4).

use std::path::Path;

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Runtime};

use crate::location::Location;
use crate::location::miss::LocationMissResult;
use crate::location::resolve::{LocationInput, LocationMiss, VolumeFacts, resolve};
use crate::location::same_share::Via;
use crate::location::store_path::{from_store_path, to_store_path};
use crate::tools::resources::importance::{
    PathImportance, RankedFolder, RankedScope, VolumeOverview, snapshot_overview, snapshot_path_on, snapshot_threshold,
    snapshot_top,
};
use crate::tools::resources::volumes::VolumeKind;
use crate::tools::{ToolError, ToolResult};
use cmdr_index::IndexVolumeKind;
use cmdr_index::importance::{Explanation, FloorReason, ImportanceIndex, SignalSet, is_background_scored};

const DEFAULT_TOP_N: usize = 20;

/// A store row's place on the wire: its path joined onto the volume's root (from
/// the one root rule, so an offline share answers under its last root).
fn app_location(volume_id: &str, store_path: &str, root: &str) -> Option<Location> {
    from_store_path(store_path, root).map(|path| Location {
        volume_id: volume_id.to_string(),
        path,
    })
}

fn to_value(result: &impl Serialize) -> ToolResult {
    serde_json::to_value(result).map_err(|e| ToolError::internal(e.to_string()))
}

// ── important_folders ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RankedFolderOut {
    pub location: Location,
    /// Rounded to three decimals: more digits are noise and cost tokens.
    pub score: f64,
    /// The row's strongest signals, like `projectRoot, changed today, 6 visits`.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub why: String,
    /// Near-duplicate descendants folded into this row; pass its location as
    /// `under` + `volumeId` to list them.
    #[serde(skip_serializing_if = "is_zero")]
    pub folded: usize,
    /// The volume whose store scored this row, when `location` re-addresses it
    /// onto the member of the same share a read prefers (a live one).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub via: Option<Via>,
}

fn is_zero(n: &usize) -> bool {
    *n == 0
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VolumeOverviewOut {
    pub volume_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// The volume's current recompute generation (its scored-ness age marker).
    pub generation: u64,
    pub folder_count: u64,
    /// Scored under an id no volume the app knows carries (a share once reached by
    /// another address), so its rows have no root to stand on and are left out.
    #[serde(skip_serializing_if = "crate::tools::read::is_false")]
    pub unplaced: bool,
    /// Volume ids proven to be the same share as this one; its rows are answered
    /// on whichever member a read prefers.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub same_share_as: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportantFoldersResult {
    /// The `under` folder the ranking was scoped to, as resolved.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub under: Option<Location>,
    pub folders: Vec<RankedFolderOut>,
    /// `true` when more folders matched than were returned (threshold mode).
    #[serde(skip_serializing_if = "crate::tools::read::is_false")]
    pub truncated: bool,
    /// The scored volumes and their current generations, so staleness is visible.
    pub volumes: Vec<VolumeOverviewOut>,
    /// Why `folders` is empty; absent when it isn't.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub empty: Option<EmptyReason>,
}

/// Why a ranking came back with no rows, so the model can tell "nothing matters
/// here" from "not scored yet" and act on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum EmptyReason {
    /// No volume in scope has an importance store, and none was named.
    NoScoredVolume,
    /// The named volume's kind is never scored (a phone, SFTP, WebDAV, S3, ...).
    KindNotScored,
    /// The named volume would be scored, but it has no drive index: indexing is
    /// off for it or never ran.
    NotIndexed,
    /// The store is still on its first pass: no rows yet at generation 0. Keyed on
    /// the row count, since an incremental-only store holds rows at generation 0.
    StillScoring,
    /// `under` floors by design (a cache, system, hidden, or build folder), and so
    /// does everything below it.
    UnderFloored,
    /// Folders are scored here, but none reach `threshold`.
    BelowThreshold,
    /// Scored, and nothing at or below `under` (or on the volume) earned a row.
    NothingScored,
}

/// Shape a ranked list plus the volume overview, placing each row on a volume's
/// root. A row whose volume `facts` can't place is left out (the read already
/// skips those volumes; this keeps an invented root unrepresentable). Pure.
///
/// A row scored on one member of a proven same-share, same-account group lands on
/// the member a read prefers (`VolumeFacts::preferred_member`: a live one before
/// an offline index), with `via` naming the store it came from. Two members'
/// stores scoring one folder make one row, the higher score kept.
pub(crate) fn build_important_folders(
    folders: Vec<RankedFolder>,
    truncated: bool,
    overview: Vec<VolumeOverview>,
    facts: &VolumeFacts,
    under: Option<Location>,
) -> ImportantFoldersResult {
    let mut placed: Vec<RankedFolderOut> = Vec::new();
    for f in folders {
        let Some(target) = facts.preferred_member(&f.volume_id) else {
            continue;
        };
        let Some(location) = app_location(&target.id, &f.path, &target.root) else {
            continue;
        };
        let row = RankedFolderOut {
            location,
            score: (f.score * 1000.0).round() / 1000.0,
            why: f.why,
            folded: f.folded,
            via: (target.id != f.volume_id).then(|| Via {
                volume_id: f.volume_id.clone(),
            }),
        };
        match placed.iter_mut().find(|kept| kept.location == row.location) {
            Some(kept) if kept.score < row.score => *kept = row,
            Some(_) => {}
            None => placed.push(row),
        }
    }
    ImportantFoldersResult {
        under,
        empty: None,
        folders: placed,
        truncated,
        volumes: overview
            .into_iter()
            .map(|v| VolumeOverviewOut {
                unplaced: facts.get(&v.volume_id).is_none(),
                same_share_as: facts.same_share_as(&v.volume_id),
                volume_id: v.volume_id,
                kind: v.kind.map(|k| k.to_string()),
                generation: v.generation,
                folder_count: v.folder_count,
            })
            .collect(),
    }
}

/// A ranked read's scope, placed: one volume (and maybe a folder on it), or every
/// volume the facts can place.
struct PlacedScope {
    /// The volume the call asked about, by `volumeId` or by where `under` resolved.
    named: Option<String>,
    volume_id: Option<String>,
    /// The `under` folder as an app location, echoed in the result.
    under: Option<Location>,
    /// The same folder in the store's path space, what the read filters by.
    under_in_store: Option<String>,
    placed: Vec<String>,
}

impl PlacedScope {
    /// Whether the read covers `volume_id`'s store.
    fn reads(&self, volume_id: &str) -> bool {
        match &self.volume_id {
            Some(id) => id == volume_id,
            None => self.placed.iter().any(|id| id == volume_id),
        }
    }

    fn ranked(&self) -> RankedScope<'_> {
        RankedScope {
            volume: self.volume_id.as_deref(),
            under: self.under_in_store.as_deref(),
            among: Some(&self.placed),
        }
    }
}

/// Place `volumeId` + `under` through the resolver. A bare `under` scopes the read
/// to the one live volume that holds it.
fn place_scope(facts: &VolumeFacts, volume_id: Option<&str>, under: Option<&str>) -> Result<PlacedScope, LocationMiss> {
    let placed = facts.volumes.iter().map(|v| v.id.clone()).collect();
    let Some(under) = under else {
        if let Some(id) = volume_id
            && facts.get(id).is_none()
        {
            // No folder was asked about, so the miss echoes an empty path.
            return Err(LocationMiss::UnknownVolume {
                requested: LocationInput {
                    volume_id: Some(id.to_string()),
                    path: String::new(),
                },
            });
        }
        return Ok(scope_on(facts, volume_id, None, None, placed));
    };
    let resolved = resolve(
        facts,
        &LocationInput {
            volume_id: volume_id.map(str::to_string),
            path: under.to_string(),
        },
    )?;
    let under_in_store = to_store_path(&resolved.location.path, &resolved.root);
    let volume_id = resolved.location.volume_id.clone();
    Ok(scope_on(
        facts,
        Some(&volume_id),
        Some(resolved.location),
        under_in_store,
        placed,
    ))
}

/// A scope on `volume_id` spans every member of its share that may answer for it:
/// one NAS scored under two ids is one ranking. A share's store paths are
/// share-relative, so `under` reads the same in each member's store.
fn scope_on(
    facts: &VolumeFacts,
    volume_id: Option<&str>,
    under: Option<Location>,
    under_in_store: Option<String>,
    placed: Vec<String>,
) -> PlacedScope {
    let group = volume_id.map(|id| facts.redirect_group_ids(id)).unwrap_or_default();
    let named = volume_id.map(str::to_string);
    match (volume_id, group.len() > 1) {
        (Some(_), true) => PlacedScope {
            named,
            volume_id: None,
            under,
            under_in_store,
            placed: group,
        },
        _ => PlacedScope {
            volume_id: named.clone(),
            named,
            under,
            under_in_store,
            placed,
        },
    }
}

pub fn important_folders_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "top": { "type": "integer", "description": "Default 20; ignored when threshold is set." },
            "threshold": { "type": "number", "description": "Every folder scoring at or above this (0-1) instead of a top-N." },
            "volumeId": { "type": "string", "description": "From list_volumes or a row's location; omit for every scored volume." },
            "under": { "type": "string", "description": "Only folders at or below this one (on volumeId when given); ~ ok." }
        },
        "additionalProperties": false
    })
}

pub async fn execute_important_folders<R: Runtime>(app: &AppHandle<R>, params: &Value) -> ToolResult {
    let data_dir = crate::config::resolved_app_data_dir(app).map_err(ToolError::internal)?;
    important_folders_on(&data_dir, &VolumeFacts::now().await, params, now_secs())
}

/// `important_folders` over a data dir and the volume facts, so the whole call is
/// testable without an app.
pub(crate) fn important_folders_on(data_dir: &Path, facts: &VolumeFacts, params: &Value, now: u64) -> ToolResult {
    let str_param = |key: &str| params.get(key).and_then(Value::as_str).filter(|s| !s.is_empty());
    let scope = match place_scope(facts, str_param("volumeId"), str_param("under")) {
        Ok(scope) => scope,
        Err(miss) => return to_value(&LocationMissResult::from(miss)),
    };
    let overview = snapshot_overview(data_dir);
    let stores: Vec<VolumeOverview> = overview.iter().filter(|v| scope.reads(&v.volume_id)).cloned().collect();
    let threshold = params.get("threshold").and_then(Value::as_f64);
    if stores.is_empty() {
        let mut result = build_important_folders(Vec::new(), false, overview, facts, scope.under);
        result.empty = Some(unscored_reason(facts, scope.named.as_deref()));
        return to_value(&result);
    }

    let (folders, truncated) = if let Some(threshold) = threshold {
        snapshot_threshold(data_dir, threshold, scope.ranked(), now).map_err(ToolError::invalid_params)?
    } else {
        let n = params
            .get("top")
            .and_then(Value::as_u64)
            .map(|n| n as usize)
            .unwrap_or(DEFAULT_TOP_N);
        let folders = snapshot_top(data_dir, n, scope.ranked(), now).map_err(ToolError::invalid_params)?;
        (folders, false)
    };
    let mut result = build_important_folders(folders, truncated, overview, facts, scope.under.clone());
    if result.folders.is_empty() {
        result.empty = Some(empty_reason(data_dir, &scope, &stores, threshold, now)?);
    }
    to_value(&result)
}

/// Why a scope with no store in it is empty: the named volume's kind is never
/// scored, it has no drive index, or it does and its first pass hasn't written yet.
fn unscored_reason(facts: &VolumeFacts, named: Option<&str>) -> EmptyReason {
    let Some(fact) = named.and_then(|id| facts.get(id)) else {
        return EmptyReason::NoScoredVolume;
    };
    if !index_kind(fact.kind).is_some_and(is_background_scored) {
        EmptyReason::KindNotScored
    } else if !(fact.index.stored || fact.index.live) {
        EmptyReason::NotIndexed
    } else {
        EmptyReason::StillScoring
    }
}

/// The drive-index kind a volume kind scans as, `None` for a kind with no drive
/// index at all. A local external drive scores like the boot disk, so `Local`
/// stands for both.
fn index_kind(kind: VolumeKind) -> Option<IndexVolumeKind> {
    match kind {
        VolumeKind::Local => Some(IndexVolumeKind::Local),
        VolumeKind::Smb => Some(IndexVolumeKind::Smb),
        VolumeKind::Mtp => Some(IndexVolumeKind::Mtp),
        VolumeKind::Adb => Some(IndexVolumeKind::Adb),
        VolumeKind::Sftp | VolumeKind::Webdav | VolumeKind::S3 | VolumeKind::Network | VolumeKind::Virtual => None,
    }
}

/// Why a scope whose `stores` exist ranked nothing.
fn empty_reason(
    data_dir: &Path,
    scope: &PlacedScope,
    stores: &[VolumeOverview],
    threshold: Option<f64>,
    now: u64,
) -> Result<EmptyReason, ToolError> {
    if let (Some(under), Some(store)) = (&scope.under_in_store, stores.first())
        && matches!(
            snapshot_path_on(data_dir, &store.volume_id, under, now),
            PathImportance::Floored { .. }
        )
    {
        return Ok(EmptyReason::UnderFloored);
    }
    // Generation 0 alone isn't "unscored": only a full pass stamps it.
    if stores.iter().all(|v| v.folder_count == 0 && v.generation == 0) {
        return Ok(EmptyReason::StillScoring);
    }
    if threshold.is_some()
        && !snapshot_top(data_dir, 1, scope.ranked(), now)
            .map_err(ToolError::invalid_params)?
            .is_empty()
    {
        return Ok(EmptyReason::BelowThreshold);
    }
    Ok(EmptyReason::NothingScored)
}

// ── folder_importance ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum FolderImportanceResult {
    /// The folder has a stored weight, with its breakdown and staleness.
    Scored {
        location: Location,
        score: f64,
        as_of_generation: u64,
        recompute_generation: u64,
        /// `true` when the stored score predates the volume's latest recompute.
        stale: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        signals: Option<Explanation>,
    },
    /// The folder floors to zero by design, with why.
    Floored {
        location: Location,
        reason: String,
        note: String,
    },
    /// The folder isn't scored and doesn't floor.
    Unscored { location: Location, note: String },
}

/// The Scored result, deriving staleness from the two generations. Pure.
pub(crate) fn scored_result(
    location: Location,
    score: f64,
    as_of_generation: u64,
    recompute_generation: u64,
    signals: Option<Explanation>,
) -> FolderImportanceResult {
    FolderImportanceResult::Scored {
        location,
        score,
        as_of_generation,
        recompute_generation,
        stale: as_of_generation < recompute_generation,
        signals,
    }
}

/// The Floored result, labeling the reason (mirrors `cmdr://importance`'s labels).
pub(crate) fn floored_result(location: Location, reason: FloorReason) -> FolderImportanceResult {
    let (label, note) = match reason {
        FloorReason::NameDenylisted => (
            "nameDenylisted",
            "Floored to zero by design: its name is denylisted (build output, cache, or VCS internals like node_modules or .git).",
        ),
        FloorReason::HiddenOrSystem => (
            "hiddenOrSystem",
            "Floored to zero by design: it's hidden (dot-prefixed) or system-owned.",
        ),
        FloorReason::UnderFlooredAncestor => (
            "underFlooredAncestor",
            "Floored to zero by design: a denylisted, hidden, or system ancestor floors its whole subtree.",
        ),
    };
    FolderImportanceResult::Floored {
        location,
        reason: label.to_string(),
        note: note.to_string(),
    }
}

pub(crate) fn unscored_result(location: Location) -> FolderImportanceResult {
    FolderImportanceResult::Unscored {
        location,
        note: "No stored weight — either this volume hasn't been scored yet, or the folder isn't in the index."
            .to_string(),
    }
}

pub fn folder_importance_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "path": { "type": "string", "description": "Absolute or ~-relative folder to explain." },
            "volumeId": { "type": "string", "description": "From list_volumes or a result's location; omit for a path on a connected volume." }
        },
        "required": ["path"],
        "additionalProperties": false
    })
}

pub async fn execute_folder_importance<R: Runtime>(app: &AppHandle<R>, params: &Value) -> ToolResult {
    let input =
        LocationInput::from_params(params).ok_or_else(|| ToolError::invalid_params("Missing 'path' parameter"))?;
    let data_dir = crate::config::resolved_app_data_dir(app).map_err(ToolError::internal)?;
    folder_importance_on(&data_dir, &VolumeFacts::now().await, &input, now_secs())
}

/// `folder_importance` over a data dir and the volume facts: the resolver picks ONE
/// volume, and only that volume's store answers.
pub(crate) fn folder_importance_on(
    data_dir: &Path,
    facts: &VolumeFacts,
    input: &LocationInput,
    now: u64,
) -> ToolResult {
    let resolved = match resolve(facts, input) {
        Ok(resolved) => resolved,
        Err(miss) => return to_value(&LocationMissResult::from(miss)),
    };
    let volume_id = resolved.location.volume_id.as_str();
    let in_store = to_store_path(&resolved.location.path, &resolved.root)
        .ok_or_else(|| ToolError::internal("the resolver placed a path outside its volume's root"))?;

    let result = match snapshot_path_on(data_dir, volume_id, &in_store, now) {
        PathImportance::Scored {
            weight, explanation, ..
        } => {
            // The volume's CURRENT generation, to flag a stale stored score. The
            // availability mask doesn't affect the generation read, so `all()` is fine.
            let recompute = ImportanceIndex::open(data_dir, volume_id, SignalSet::all())
                .recompute_generation()
                .unwrap_or(0);
            scored_result(
                resolved.location,
                weight.score.value(),
                weight.as_of_generation,
                recompute,
                explanation,
            )
        }
        PathImportance::Floored { reason } => floored_result(resolved.location, reason),
        PathImportance::Unscored => unscored_result(resolved.location),
    };
    to_value(&result)
}

/// Wall-clock unix seconds for the `explain` recency terms. A tool call reads the
/// current time directly; threading the envelope's clock through the runtime (so a
/// tool call and its explanation agree on "now") is a refinement, not yet wired.
fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
#[path = "importance_tests.rs"]
mod tests;
