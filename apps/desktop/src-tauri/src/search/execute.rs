//! Multi-volume search orchestration.
//!
//! A search runs across one or more volumes: the volume(s) a scope points at, or —
//! unscoped — every volume with a persisted index. This module resolves the target
//! set, loads each volume lazily, runs the pure per-volume engine, and k-way-merges
//! the ranked slices into one global result. It's the one place both the search
//! dialog (`commands/search.rs`) and the MCP `search`/`ai_search` tools funnel
//! through, so routing, the honesty signal, and merge live once.

use crate::indexing::store::IndexStore;
use crate::indexing::{ReadPool, volume_id_for_local_path};

use super::engine::{self, RankedEntry};
use super::query;
use super::types::{SearchQuery, SearchResult};
use super::volumes::{self, VolumeLoad};

/// One target volume for a search: the volume id plus the scope include paths that
/// belong to it (empty for an unscoped, whole-volume search). `from_scope` marks a
/// target the user explicitly scoped to, so an unindexed one becomes an honest
/// coverage gap rather than a silent skip.
struct Target {
    volume_id: String,
    include_paths: Vec<String>,
    from_scope: bool,
}

/// Resolve a query's scope into the set of volumes to search.
///
/// - **Scoped** (`include_paths` non-empty): each path routes to its owning volume
///   (`volume_id_for_local_path`), grouping paths by volume, preserving order and
///   deduping volumes. Every target is `from_scope`.
/// - **Unscoped**: every volume with a persisted index, whole-volume each.
fn resolve_targets(query: &SearchQuery) -> Vec<Target> {
    match query.include_paths.as_ref().filter(|p| !p.is_empty()) {
        Some(paths) => {
            let mut targets: Vec<Target> = Vec::new();
            for path in paths {
                let volume_id = volume_id_for_local_path(path);
                if let Some(t) = targets.iter_mut().find(|t| t.volume_id == volume_id) {
                    t.include_paths.push(path.clone());
                } else {
                    targets.push(Target {
                        volume_id,
                        include_paths: vec![path.clone()],
                        from_scope: true,
                    });
                }
            }
            targets
        }
        None => volumes::all_indexed_volume_ids()
            .into_iter()
            .map(|volume_id| Target {
                volume_id,
                include_paths: Vec::new(),
                from_scope: false,
            })
            .collect(),
    }
}

/// Run a search across every target volume and merge. Synchronous (opens DBs, reads
/// arenas, scans with rayon) — call inside `spawn_blocking`.
///
/// Returns `Err` only for a query the engine rejects outright (invalid regex, too
/// broad). Coverage gaps (a scope on an unindexed volume) are NOT errors: they ride
/// back in `SearchResult::uncovered_scopes` alongside whatever the covered volumes
/// matched.
pub(crate) fn run_blocking(query: SearchQuery) -> Result<SearchResult, String> {
    // Record activity so the backstop timer doesn't evict a warm arena mid-use;
    // this covers the MCP path too (it has no dialog to touch activity for it).
    volumes::touch_activity();

    let targets = resolve_targets(&query);
    let base_limit = query.limit.min(1000) as usize;

    let mut merged: Vec<RankedEntry> = Vec::new();
    let mut total: u64 = 0;
    let mut uncovered_scopes: Vec<String> = Vec::new();

    for target in targets {
        let loaded = match volumes::ensure_volume(&target.volume_id) {
            VolumeLoad::Loaded(v) => v,
            VolumeLoad::NotIndexed => {
                // A scope pointing here can't be searched — surface it honestly. An
                // unscoped target simply has no index yet, so skip it silently.
                if target.from_scope {
                    uncovered_scopes.extend(target.include_paths);
                }
                continue;
            }
            VolumeLoad::Failed(e) => {
                log::warn!("search: skipping volume '{}': {e}", target.volume_id);
                if target.from_scope {
                    uncovered_scopes.extend(target.include_paths);
                }
                continue;
            }
        };

        let mut vq = query.clone();
        vq.include_paths = (!target.include_paths.is_empty()).then(|| target.include_paths.clone());
        vq.include_path_ids = vq
            .include_paths
            .as_ref()
            .map(|paths| query::resolve_include_path_ids(paths, &loaded.pool, loaded.mount_root.as_deref()));

        let weights = volumes::weights_for(&target.volume_id);
        let prefix = loaded.mount_root.as_deref().unwrap_or("");
        let (mut ranked, vtotal) = engine::search_ranked(&loaded.index, &vq, &weights, prefix)?;

        // Directory sizes live in `dir_stats`, not the entries table, so fill them
        // from this volume's pool, then drop dirs outside the size filter (the
        // engine over-fetched dir candidates to absorb this — see its limit bump).
        fill_ranked_dir_sizes(&mut ranked, &loaded.pool);
        let vtotal = filter_ranked_dirs_by_size(&mut ranked, &vq, vtotal);

        total += vtotal as u64;
        merged.extend(ranked);
    }

    // K-way merge: every per-volume slice is already ranked best-first, and the
    // keys compare across volumes (band + boosted recency are volume-independent),
    // so one global sort produces the merged order. Truncate to the caller's limit.
    merged.sort_by(|a, b| a.key.cmp_best_first(&b.key));
    merged.truncate(base_limit);

    Ok(SearchResult {
        entries: merged.into_iter().map(|r| r.entry).collect(),
        total_count: total.min(u32::MAX as u64) as u32,
        uncovered_scopes,
    })
}

/// Fill directory entries' sizes from a volume's `dir_stats` (batch lookup by entry
/// id). Files already carry their size from the entries table; only directories
/// reach here sizeless.
fn fill_ranked_dir_sizes(ranked: &mut [RankedEntry], pool: &ReadPool) {
    let dir_indices: Vec<usize> = ranked
        .iter()
        .enumerate()
        .filter(|(_, r)| r.entry.is_directory)
        .map(|(i, _)| i)
        .collect();
    if dir_indices.is_empty() {
        return;
    }
    let entry_ids: Vec<i64> = dir_indices.iter().map(|&i| ranked[i].entry.entry_id).collect();
    let _ = pool.with_conn(|conn| {
        if let Ok(stats_batch) = IndexStore::get_dir_stats_batch_by_ids(conn, &entry_ids) {
            for (i, &idx) in dir_indices.iter().enumerate() {
                if let Some(Some(stats)) = stats_batch.get(i) {
                    ranked[idx].entry.size = Some(stats.recursive_logical_size);
                }
            }
        }
    });
}

/// Drop directories whose (dir_stats) size falls outside the query's size filter,
/// and return the adjusted match total. Files are already size-filtered by the
/// engine, so they pass through. A no-op (returns `vtotal` unchanged) when the query
/// has no size filter; otherwise `total` becomes the retained length (approximate,
/// as the exact count would need `dir_stats` for every matching directory).
fn filter_ranked_dirs_by_size(ranked: &mut Vec<RankedEntry>, query: &SearchQuery, vtotal: u32) -> u32 {
    if query.min_size.is_none() && query.max_size.is_none() {
        return vtotal;
    }
    ranked.retain(|r| {
        if !r.entry.is_directory {
            return true;
        }
        if let Some(min) = query.min_size {
            match r.entry.size {
                Some(s) if s >= min => {}
                _ => return false,
            }
        }
        if let Some(max) = query.max_size {
            match r.entry.size {
                Some(s) if s <= max => {}
                _ => return false,
            }
        }
        true
    });
    ranked.len() as u32
}

#[cfg(test)]
mod tests;
