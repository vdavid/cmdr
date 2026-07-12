//! The `list_dir` and `largest_dirs` agent tools, over the drive index.
//!
//! Both REUSE the shipped index query cores (`indexing::list_dir_children`,
//! `get_dir_stats`, `get_dir_stats_batch`) — they read the index and SQLite only,
//! never the disk, so they're safe on a dead mount. Neither re-derives listing
//! logic. `largest_dirs` is the one surface with no backing index query: it
//! batches `get_dir_stats` over the subdirectories and sorts them here.
//!
//! Every result carries a typed [`Coverage`] block so the model can voice the
//! index's honesty (spec §2.4): the freshness token (`fresh` / `scanning` /
//! `stale` / `off`, only `fresh` authoritative), a typed "no index" / "not in
//! index" state instead of a wrong empty listing, and each size's exact-vs-
//! lower-bound / stale / updating flags straight from `DirStats`.

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Runtime};

use super::{expand_tilde, join_child_path};
use crate::indexing::freshness::Freshness;
use crate::indexing::store::DirStats;
use crate::indexing::{get_dir_stats, get_dir_stats_batch, get_volume_index_status_for_path, list_dir_children};
use crate::mcp::resources::indexing::status_token;
use crate::mcp::{ToolError, ToolResult};

const DEFAULT_LARGEST_N: usize = 20;
const MAX_LARGEST_N: usize = 200;

/// The index's honesty for a read: its freshness token, whether reads are
/// authoritative (`fresh` only), and a plain-language caveat when they aren't (or
/// when the path isn't in the index).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Coverage {
    /// `fresh` / `scanning` / `stale` / `off`. The shared token every surface uses.
    pub index_status: String,
    /// Reads are authoritative only when the index is `fresh`.
    pub authoritative: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// Build the coverage block. `indexed` is whether the path actually resolved in a
/// live index (so `false` distinguishes "not in the index" from an empty folder).
/// Reuses `status_token` + `Freshness::is_authoritative` so it can't drift from the
/// rest of the app.
pub(crate) fn coverage(enabled: bool, freshness: Option<Freshness>, indexed: bool) -> Coverage {
    let authoritative = freshness.is_some_and(|f| f.is_authoritative());
    let note = if !indexed {
        Some(if enabled {
            "This folder isn't in the drive index — it may be new, hidden, or outside the indexed area.".to_string()
        } else {
            "This volume isn't indexed, so I can't read it from the index.".to_string()
        })
    } else if !authoritative {
        Some(match freshness {
            Some(Freshness::Scanning) => "The index is still scanning, so this may be incomplete.".to_string(),
            _ => "The index may have drifted since the last full scan, so treat this as best-effort.".to_string(),
        })
    } else {
        None
    };
    Coverage {
        index_status: status_token(enabled, freshness).to_string(),
        authoritative,
        note,
    }
}

/// One child row, shaped for the model.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChildEntry {
    pub name: String,
    pub is_directory: bool,
    #[serde(skip_serializing_if = "crate::agent::tools::read::is_false")]
    pub is_symlink: bool,
    /// The entry's own (non-recursive) size, if the index has it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified: Option<u64>,
}

/// A directory's recursive size totals plus the honest-size flags from `DirStats`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SizeStats {
    pub recursive_size: u64,
    pub recursive_file_count: u64,
    pub recursive_dir_count: u64,
    /// `true` when `recursive_size` is a lower bound, not an exact total (some
    /// subtree was never fully listed).
    pub size_is_lower_bound: bool,
    /// `true` when the exact size was computed at an older volume epoch (stale).
    pub size_is_stale: bool,
    /// `true` while the indexer is still applying writes affecting this subtree.
    pub size_is_updating: bool,
    /// `true` if a descendant is a symlink (so the size may omit linked content).
    pub has_symlinks: bool,
}

impl SizeStats {
    fn from_dir_stats(s: &DirStats) -> Self {
        Self {
            recursive_size: s.recursive_size,
            recursive_file_count: s.recursive_file_count,
            recursive_dir_count: s.recursive_dir_count,
            size_is_lower_bound: !s.recursive_size_complete,
            size_is_stale: s.recursive_size_stale,
            size_is_updating: s.recursive_size_pending,
            has_symlinks: s.recursive_has_symlinks,
        }
    }
}

// ── list_dir ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListDirResult {
    pub path: String,
    pub coverage: Coverage,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<SizeStats>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<ChildEntry>>,
}

/// Shape one directory's listing. Pure over the resolved inputs, so the coverage
/// flags are testable without a live index.
pub(crate) fn build_list_dir(
    path: &str,
    children: Option<Vec<ChildEntry>>,
    stats: Option<&DirStats>,
    enabled: bool,
    freshness: Option<Freshness>,
) -> ListDirResult {
    ListDirResult {
        path: path.to_string(),
        coverage: coverage(enabled, freshness, children.is_some()),
        size: stats.map(SizeStats::from_dir_stats),
        children,
    }
}

pub fn list_dir_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "path": { "type": "string", "description": "Absolute or ~-relative folder path to list." }
        },
        "required": ["path"],
        "additionalProperties": false
    })
}

pub async fn execute_list_dir<R: Runtime>(_app: &AppHandle<R>, params: &Value) -> ToolResult {
    let path = required_path(params)?;
    let children = list_dir_children(&path)
        .map_err(ToolError::internal)?
        .map(|rows| rows.iter().map(child_from_row).collect());
    let stats = get_dir_stats(&path).map_err(ToolError::internal)?;
    let status = get_volume_index_status_for_path(&path);
    let result = build_list_dir(&path, children, stats.as_ref(), status.enabled, status.freshness);
    serde_json::to_value(&result).map_err(|e| ToolError::internal(e.to_string()))
}

// ── largest_dirs ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LargestDir {
    pub path: String,
    pub name: String,
    #[serde(flatten)]
    pub size: SizeStats,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LargestDirsResult {
    pub path: String,
    pub coverage: Coverage,
    /// The subdirectories ranked by recursive size, largest first.
    pub directories: Vec<LargestDir>,
}

/// Rank the candidate subdirectories by recursive size (largest first), dropping
/// those with no stats, and cap to `n`. Pure over the resolved `(path, stats)`
/// pairs. `indexed` mirrors `list_dir`'s coverage meaning.
pub(crate) fn build_largest_dirs(
    path: &str,
    candidates: Vec<(String, String, Option<DirStats>)>,
    n: usize,
    enabled: bool,
    freshness: Option<Freshness>,
    indexed: bool,
) -> LargestDirsResult {
    let mut dirs: Vec<LargestDir> = candidates
        .into_iter()
        .filter_map(|(child_path, name, stats)| {
            stats.map(|s| LargestDir {
                path: child_path,
                name,
                size: SizeStats::from_dir_stats(&s),
            })
        })
        .collect();
    dirs.sort_by(|a, b| {
        b.size
            .recursive_size
            .cmp(&a.size.recursive_size)
            .then_with(|| a.path.cmp(&b.path))
    });
    dirs.truncate(n);
    LargestDirsResult {
        path: path.to_string(),
        coverage: coverage(enabled, freshness, indexed),
        directories: dirs,
    }
}

pub fn largest_dirs_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "path": { "type": "string", "description": "Absolute or ~-relative parent folder whose subdirectories to rank by size." },
            "limit": { "type": "integer", "description": "How many to return, largest first (default 20, max 200)." }
        },
        "required": ["path"],
        "additionalProperties": false
    })
}

pub async fn execute_largest_dirs<R: Runtime>(_app: &AppHandle<R>, params: &Value) -> ToolResult {
    let path = required_path(params)?;
    let n = params
        .get("limit")
        .and_then(|v| v.as_u64())
        .map(|n| (n as usize).clamp(1, MAX_LARGEST_N))
        .unwrap_or(DEFAULT_LARGEST_N);
    let status = get_volume_index_status_for_path(&path);

    let children = list_dir_children(&path).map_err(ToolError::internal)?;
    let result = match children {
        None => build_largest_dirs(&path, Vec::new(), n, status.enabled, status.freshness, false),
        Some(rows) => {
            // Only real subdirectories are size-rankable (skip files and symlinks).
            let subdirs: Vec<(String, String)> = rows
                .iter()
                .filter(|r| r.is_directory && !r.is_symlink)
                .map(|r| (join_child_path(&path, &r.name), r.name.clone()))
                .collect();
            let paths: Vec<String> = subdirs.iter().map(|(p, _)| p.clone()).collect();
            let stats = get_dir_stats_batch(&paths).map_err(ToolError::internal)?;
            let candidates = subdirs
                .into_iter()
                .zip(stats)
                .map(|((child_path, name), s)| (child_path, name, s))
                .collect();
            build_largest_dirs(&path, candidates, n, status.enabled, status.freshness, true)
        }
    };
    serde_json::to_value(&result).map_err(|e| ToolError::internal(e.to_string()))
}

// ── Shared helpers ────────────────────────────────────────────────────────────

/// The required `path` param, tilde-expanded (agents send `~/…`).
fn required_path(params: &Value) -> Result<String, ToolError> {
    let raw = params
        .get("path")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ToolError::invalid_params("Missing 'path' parameter"))?;
    Ok(expand_tilde(raw))
}

/// Map an index `EntryRow` into the model-facing child shape. Typed against the
/// row's fields; `size` is the entry's own logical size where the index has it.
fn child_from_row(row: &crate::indexing::store::EntryRow) -> ChildEntry {
    ChildEntry {
        name: row.name.clone(),
        is_directory: row.is_directory,
        is_symlink: row.is_symlink,
        size: row.logical_size,
        modified: row.modified_at,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir_stats(size: u64, complete: bool, stale: bool, pending: bool) -> DirStats {
        DirStats {
            path: "/x".to_string(),
            recursive_size: size,
            recursive_physical_size: size,
            recursive_file_count: 3,
            recursive_dir_count: 1,
            recursive_has_symlinks: false,
            recursive_size_pending: pending,
            recursive_size_complete: complete,
            recursive_size_stale: stale,
        }
    }

    #[test]
    fn fresh_index_is_authoritative_with_no_note() {
        let cov = coverage(true, Some(Freshness::Fresh), true);
        assert_eq!(cov.index_status, "fresh");
        assert!(cov.authoritative);
        assert_eq!(cov.note, None);
    }

    #[test]
    fn stale_index_reads_stale_and_says_so() {
        let cov = coverage(true, Some(Freshness::Stale), true);
        assert_eq!(cov.index_status, "stale");
        assert!(!cov.authoritative);
        assert!(cov.note.is_some());
    }

    #[test]
    fn unindexed_volume_returns_typed_no_index_not_a_wrong_zero() {
        // children None + not enabled ⇒ "off" + a "not indexed" note, never an
        // empty-but-authoritative listing.
        let result = build_list_dir("/nas/share", None, None, false, None);
        assert_eq!(result.coverage.index_status, "off");
        assert!(!result.coverage.authoritative);
        assert!(result.coverage.note.as_deref().unwrap().contains("isn't indexed"));
        assert!(result.children.is_none());
        assert!(result.size.is_none());
    }

    #[test]
    fn indexed_but_missing_path_is_a_distinct_not_in_index_note() {
        let result = build_list_dir("/Users/x/new", None, None, true, Some(Freshness::Fresh));
        assert_eq!(result.coverage.index_status, "fresh");
        assert!(
            result
                .coverage
                .note
                .as_deref()
                .unwrap()
                .contains("isn't in the drive index")
        );
    }

    #[test]
    fn list_dir_surfaces_lower_bound_and_updating_flags() {
        let stats = dir_stats(1_000, false, false, true);
        let result = build_list_dir(
            "/Users/x",
            Some(vec![ChildEntry {
                name: "sub".to_string(),
                is_directory: true,
                is_symlink: false,
                size: None,
                modified: None,
            }]),
            Some(&stats),
            true,
            Some(Freshness::Fresh),
        );
        let size = result.size.unwrap();
        assert!(size.size_is_lower_bound);
        assert!(size.size_is_updating);
        assert_eq!(size.recursive_size, 1_000);
    }

    #[test]
    fn largest_dirs_sorts_by_size_desc_drops_statless_and_caps() {
        let candidates = vec![
            (
                "/p/a".to_string(),
                "a".to_string(),
                Some(dir_stats(10, true, false, false)),
            ),
            (
                "/p/b".to_string(),
                "b".to_string(),
                Some(dir_stats(300, true, false, false)),
            ),
            ("/p/c".to_string(), "c".to_string(), None), // no stats ⇒ dropped
            (
                "/p/d".to_string(),
                "d".to_string(),
                Some(dir_stats(200, true, false, false)),
            ),
        ];
        let result = build_largest_dirs("/p", candidates, 2, true, Some(Freshness::Fresh), true);
        assert_eq!(result.directories.len(), 2, "capped at 2");
        assert_eq!(result.directories[0].name, "b");
        assert_eq!(result.directories[0].size.recursive_size, 300);
        assert_eq!(result.directories[1].name, "d");
    }

    #[test]
    fn largest_dirs_on_unindexed_volume_is_empty_with_no_index_note() {
        let result = build_largest_dirs("/nas", Vec::new(), 20, false, None, false);
        assert!(result.directories.is_empty());
        assert_eq!(result.coverage.index_status, "off");
        assert!(result.coverage.note.is_some());
    }
}
