//! The `important_folders` and `folder_importance` agent tools.
//!
//! Both REUSE the shipped `cmdr://importance` snapshot core
//! (`mcp::resources::importance::snapshot_*`), which already handles the headline
//! behaviors: reading across every scored volume, answering OFFLINE for an
//! unmounted-but-scored drive (the importance stores outlive the mount), tilde
//! expansion, and the floored-vs-unscored derivation. This file only shapes those
//! typed snapshots into the agent's JSON and adds the staleness comparison
//! (`asOfGeneration` vs the volume's current `recomputeGeneration`, spec §2.4).

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Runtime};

use super::expand_tilde;
use crate::importance::{Explanation, FloorReason, ImportanceIndex, SignalSet};
use crate::mcp::resources::importance::{
    PathImportance, RankedFolder, VolumeOverview, snapshot_overview, snapshot_path, snapshot_threshold, snapshot_top,
};
use crate::mcp::{ToolError, ToolResult};

const DEFAULT_TOP_N: usize = 20;

// ── important_folders ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RankedFolderOut {
    pub volume: String,
    pub path: String,
    pub score: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VolumeOverviewOut {
    pub volume: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// The volume's current recompute generation (its scored-ness age marker).
    pub generation: u64,
    pub folder_count: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportantFoldersResult {
    pub folders: Vec<RankedFolderOut>,
    /// `true` when more folders matched than were returned (threshold mode).
    #[serde(skip_serializing_if = "crate::agent::tools::read::is_false")]
    pub truncated: bool,
    /// The scored volumes and their current generations, so staleness is visible.
    pub volumes: Vec<VolumeOverviewOut>,
}

/// Shape a ranked list plus the volume overview. Pure over the snapshot outputs.
pub(crate) fn build_important_folders(
    folders: Vec<RankedFolder>,
    truncated: bool,
    overview: Vec<VolumeOverview>,
) -> ImportantFoldersResult {
    ImportantFoldersResult {
        folders: folders
            .into_iter()
            .map(|f| RankedFolderOut {
                volume: f.volume_id,
                path: f.path,
                score: f.score,
            })
            .collect(),
        truncated,
        volumes: overview
            .into_iter()
            .map(|v| VolumeOverviewOut {
                volume: v.volume_id,
                kind: v.kind.map(|k| k.to_string()),
                generation: v.generation,
                folder_count: v.folder_count,
            })
            .collect(),
    }
}

pub fn important_folders_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "top": { "type": "integer", "description": "How many to return, highest score first (default 20). Ignored when threshold is set." },
            "threshold": { "type": "number", "description": "Return every folder scoring at or above this (0.0-1.0) instead of a top-N." },
            "volume": { "type": "string", "description": "Restrict to one volume id (see list_volumes); omit to span all scored volumes." }
        },
        "additionalProperties": false
    })
}

pub async fn execute_important_folders<R: Runtime>(app: &AppHandle<R>, params: &Value) -> ToolResult {
    let data_dir = crate::config::resolved_app_data_dir(app).map_err(ToolError::internal)?;
    let volume = params.get("volume").and_then(|v| v.as_str());
    let overview = snapshot_overview(&data_dir);

    let result = if let Some(threshold) = params.get("threshold").and_then(|v| v.as_f64()) {
        let (folders, truncated) =
            snapshot_threshold(&data_dir, threshold, volume).map_err(ToolError::invalid_params)?;
        build_important_folders(folders, truncated, overview)
    } else {
        let n = params
            .get("top")
            .and_then(|v| v.as_u64())
            .map(|n| n as usize)
            .unwrap_or(DEFAULT_TOP_N);
        let folders = snapshot_top(&data_dir, n, volume).map_err(ToolError::invalid_params)?;
        build_important_folders(folders, false, overview)
    };
    serde_json::to_value(&result).map_err(|e| ToolError::internal(e.to_string()))
}

// ── folder_importance ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum FolderImportanceResult {
    /// The folder has a stored weight, with its breakdown and staleness.
    Scored {
        path: String,
        volume: String,
        score: f64,
        as_of_generation: u64,
        recompute_generation: u64,
        /// `true` when the stored score predates the volume's latest recompute.
        stale: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        signals: Option<Explanation>,
    },
    /// The folder floors to zero by design, with why.
    Floored { path: String, reason: String, note: String },
    /// The folder isn't scored and doesn't floor.
    Unscored { path: String, note: String },
}

/// The Scored result, deriving staleness from the two generations. Pure.
pub(crate) fn scored_result(
    path: String,
    volume: String,
    score: f64,
    as_of_generation: u64,
    recompute_generation: u64,
    signals: Option<Explanation>,
) -> FolderImportanceResult {
    FolderImportanceResult::Scored {
        path,
        volume,
        score,
        as_of_generation,
        recompute_generation,
        stale: as_of_generation < recompute_generation,
        signals,
    }
}

/// The Floored result, labeling the reason (mirrors `cmdr://importance`'s labels).
pub(crate) fn floored_result(path: String, reason: FloorReason) -> FolderImportanceResult {
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
        path,
        reason: label.to_string(),
        note: note.to_string(),
    }
}

pub(crate) fn unscored_result(path: String) -> FolderImportanceResult {
    FolderImportanceResult::Unscored {
        path,
        note: "No stored weight — either this volume hasn't been scored yet, or the folder isn't in the index."
            .to_string(),
    }
}

pub fn folder_importance_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "path": { "type": "string", "description": "Absolute or ~-relative folder path to explain." }
        },
        "required": ["path"],
        "additionalProperties": false
    })
}

pub async fn execute_folder_importance<R: Runtime>(app: &AppHandle<R>, params: &Value) -> ToolResult {
    let raw = params
        .get("path")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ToolError::invalid_params("Missing 'path' parameter"))?;
    let path = expand_tilde(raw);
    let data_dir = crate::config::resolved_app_data_dir(app).map_err(ToolError::internal)?;

    let result = match snapshot_path(&data_dir, &path, now_secs()) {
        PathImportance::Scored {
            volume_id,
            weight,
            explanation,
        } => {
            // The volume's CURRENT generation, to flag a stale stored score. The
            // availability mask doesn't affect the generation read, so `all()` is fine.
            let recompute = ImportanceIndex::open(&data_dir, &volume_id, SignalSet::all())
                .recompute_generation()
                .unwrap_or(0);
            scored_result(
                path,
                volume_id,
                weight.score.value(),
                weight.as_of_generation,
                recompute,
                explanation,
            )
        }
        PathImportance::Floored { reason } => floored_result(path, reason),
        PathImportance::Unscored => unscored_result(path),
    };
    serde_json::to_value(&result).map_err(|e| ToolError::internal(e.to_string()))
}

/// Wall-clock unix seconds for the `explain` recency terms. M5 threads the
/// envelope's clock through the runtime so a tool call and its explanation agree
/// on "now"; until then a tool call reads the current time directly.
fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn important_folders_passes_ranking_through_and_notes_truncation() {
        let folders = vec![
            RankedFolder {
                volume_id: "root".to_string(),
                path: "/Users/x/Projects".to_string(),
                score: 0.9,
            },
            RankedFolder {
                volume_id: "root".to_string(),
                path: "/Users/x/Documents".to_string(),
                score: 0.7,
            },
        ];
        let overview = vec![VolumeOverview {
            volume_id: "root".to_string(),
            kind: Some("local"),
            generation: 5,
            folder_count: 1_200,
        }];
        let out = build_important_folders(folders, true, overview);
        assert_eq!(out.folders[0].path, "/Users/x/Projects");
        assert_eq!(out.folders[0].score, 0.9);
        assert!(out.truncated);
        assert_eq!(out.volumes[0].generation, 5);
    }

    #[test]
    fn scored_result_flags_stale_when_score_predates_recompute() {
        let stale = scored_result("/p".to_string(), "root".to_string(), 0.8, 3, 5, None);
        let FolderImportanceResult::Scored { stale: is_stale, .. } = stale else {
            panic!("expected Scored");
        };
        assert!(is_stale, "as_of 3 < recompute 5 ⇒ stale");

        let current = scored_result("/p".to_string(), "root".to_string(), 0.8, 5, 5, None);
        let FolderImportanceResult::Scored { stale: is_stale, .. } = current else {
            panic!("expected Scored");
        };
        assert!(!is_stale, "as_of == recompute ⇒ current");
    }

    #[test]
    fn floored_and_unscored_carry_honest_reasons() {
        let floored = floored_result("/x/node_modules".to_string(), FloorReason::NameDenylisted);
        let json = serde_json::to_value(&floored).unwrap();
        assert_eq!(json["status"], "floored");
        assert_eq!(json["reason"], "nameDenylisted");

        let unscored = serde_json::to_value(unscored_result("/x/new".to_string())).unwrap();
        assert_eq!(unscored["status"], "unscored");
        assert!(unscored["note"].as_str().unwrap().contains("No stored weight"));
    }
}
