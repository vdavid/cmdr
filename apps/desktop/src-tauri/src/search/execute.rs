//! Single-volume search orchestration.
//!
//! A search covers at most ONE volume: the volume its scope points at, or the boot
//! volume when it has no scope. This module resolves that target, loads its arena,
//! runs the pure engine over it, and hands back its ranked results. It's the one place
//! both the search dialog (`commands/search.rs`) and the MCP `search`/`ai_search`
//! tools funnel through, so routing and the honesty signal live once.
//!
//! The ceiling is enforced HERE, not in the dialog: [`resolve_target`] returns one
//! target or refuses. A scope spanning two volumes has no honest answer, so it's an
//! error rather than a quiet pick. Why one volume at all: `docs/specs/unindexed-search-plan.md`
//! Decision 4 — a fan-out is the only way a search can silently omit a drive.

use crate::index_host::index;
use cmdr_index::store::IndexStore;
use cmdr_index::{ROOT_VOLUME_ID, ReadPool};

use super::engine;
use super::query;
use super::types::{SearchQuery, SearchResult, SearchResultEntry};
use super::volumes::{self, VolumeLoad};

/// The one volume a search targets: the volume id plus the scope include paths that
/// belong to it (empty for a whole-volume search). `from_scope` marks a target the
/// user explicitly scoped to, so an unindexed one becomes an honest coverage gap
/// rather than a silent skip.
#[cfg_attr(test, derive(Debug))]
struct Target {
    volume_id: String,
    include_paths: Vec<String>,
    from_scope: bool,
}

/// Why a query's scope can't be reduced to the one volume a search may cover. Typed
/// so callers branch on the variant, never on the message
/// (`.claude/rules/no-string-matching.md`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ScopeError {
    /// The include paths live on more than one volume. Carries the volume ids in
    /// first-seen order.
    SpansMultipleVolumes { volume_ids: Vec<String> },
}

impl std::fmt::Display for ScopeError {
    /// The sentence the dialog toasts and MCP returns. Draft copy pending David's
    /// review; it lives in Rust because this IPC boundary carries a bare message
    /// (see `query-runner.svelte.ts`'s `describeRunFailure`), same as the engine's
    /// "Query too broad".
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SpansMultipleVolumes { .. } => f.write_str(
                "A search covers one volume at a time. Narrow the scope to a single volume, or search them one by one.",
            ),
        }
    }
}

/// Resolve a query's scope into the ONE volume to search.
///
/// - **Scoped** (`include_paths` non-empty): every path routes to its owning volume
///   (`volume_id_for_local_path`). They must agree, or there's nothing to search;
///   the target then carries all of them and is `from_scope`.
/// - **Unscoped**: the boot volume, whole-volume, not `from_scope` (nobody asked for
///   it, so an unindexed boot volume isn't a coverage gap to report).
fn resolve_target(query: &SearchQuery) -> Result<Target, ScopeError> {
    let Some(paths) = query.include_paths.as_ref().filter(|p| !p.is_empty()) else {
        return Ok(Target {
            volume_id: ROOT_VOLUME_ID.to_string(),
            include_paths: Vec::new(),
            from_scope: false,
        });
    };

    let mut volume_ids: Vec<String> = Vec::new();
    for path in paths {
        let volume_id = index().volume_id_for_path(path);
        if !volume_ids.contains(&volume_id) {
            volume_ids.push(volume_id);
        }
    }
    if volume_ids.len() > 1 {
        return Err(ScopeError::SpansMultipleVolumes { volume_ids });
    }
    Ok(Target {
        volume_id: volume_ids.remove(0),
        include_paths: paths.clone(),
        from_scope: true,
    })
}

/// Run a search over its one target volume. Synchronous (opens a DB, reads an arena,
/// scans with rayon) — call inside `spawn_blocking`.
///
/// Returns `Err` for a scope that spans volumes ([`ScopeError`]) or a query the
/// engine rejects outright (invalid regex, too broad). A coverage gap (the scope's
/// volume has no index) is NOT an error: it rides back in
/// `SearchResult::uncovered_scopes` with an empty result set.
pub(crate) fn run_blocking(query: SearchQuery) -> Result<SearchResult, String> {
    // Record activity so the backstop timer doesn't evict a warm arena mid-use;
    // this covers the MCP path too (it has no dialog to touch activity for it).
    volumes::touch_activity();

    let target = resolve_target(&query).map_err(|e| e.to_string())?;
    let limit = query.limit.min(1000) as usize;

    let loaded = match volumes::ensure_volume(&target.volume_id) {
        VolumeLoad::Loaded(v) => v,
        VolumeLoad::NotIndexed => return Ok(uncovered_result(target)),
        VolumeLoad::Failed(e) => {
            log::warn!("search: volume '{}' isn't searchable: {e}", target.volume_id);
            return Ok(uncovered_result(target));
        }
    };

    let mut vq = query.clone();
    let unresolved_scopes = if target.include_paths.is_empty() {
        vq.include_paths = None;
        vq.include_path_ids = None;
        Vec::new()
    } else {
        let resolution =
            query::resolve_include_scope(&target.include_paths, &loaded.pool, loaded.mount_root.as_deref());
        // Empty ids ⇒ a mount-root ("whole volume") scope: drop the restriction
        // entirely (routing already scoped to this volume). Otherwise apply it.
        if resolution.include_ids.is_empty() {
            vq.include_paths = None;
            vq.include_path_ids = None;
        } else {
            vq.include_paths = Some(target.include_paths.clone());
            vq.include_path_ids = Some(resolution.include_ids);
        }
        resolution.unresolved
    };

    let weights = volumes::weights_for(&target.volume_id);
    let prefix = loaded.mount_root.as_deref().unwrap_or("");
    let (mut entries, mut total) = engine::search_ranked(&loaded.index, &vq, &weights, prefix)?;

    if query.count_only {
        // Count-only: an exact total, no rows. `entries` holds the matching directories
        // only when a size filter applies to them (else it's empty and `total` is
        // already exact). Fill their dir_stats sizes and subtract the ones outside the
        // filter. Files are already size-filtered by the engine.
        if !entries.is_empty() {
            fill_dir_sizes(&mut entries, &loaded.pool);
        }
        total = count_only_volume_total(total, &entries, &vq);
        entries.clear();
    } else {
        // Directory sizes live in `dir_stats`, not the entries table, so fill them
        // from the volume's pool, then drop dirs outside the size filter (the engine
        // over-fetched dir candidates to absorb this — see its limit bump).
        fill_dir_sizes(&mut entries, &loaded.pool);
        total = filter_dirs_by_size(&mut entries, &vq, total);
        // The engine already returns best-first, so there's nothing to re-sort: only
        // the over-fetch needs cutting back to what the caller asked for.
        entries.truncate(limit);
    }

    Ok(SearchResult {
        entries,
        total_count: total,
        uncovered_scopes: Vec::new(),
        unresolved_scopes,
        target_volume_id: target.volume_id,
    })
}

/// An empty result for a volume with no searchable index. The scope paths ride back as
/// an honest coverage gap when the user named them; an unscoped search reports nothing,
/// because nobody asked for the boot volume by name.
fn uncovered_result(target: Target) -> SearchResult {
    SearchResult {
        entries: Vec::new(),
        total_count: 0,
        uncovered_scopes: if target.from_scope {
            target.include_paths
        } else {
            Vec::new()
        },
        unresolved_scopes: Vec::new(),
        target_volume_id: target.volume_id,
    }
}

/// Fill directory entries' sizes from a volume's `dir_stats` (batch lookup by entry
/// id). Files already carry their size from the entries table; only directories
/// reach here sizeless.
fn fill_dir_sizes(entries: &mut [SearchResultEntry], pool: &ReadPool) {
    let dir_indices: Vec<usize> = entries
        .iter()
        .enumerate()
        .filter(|(_, e)| e.is_directory)
        .map(|(i, _)| i)
        .collect();
    if dir_indices.is_empty() {
        return;
    }
    let entry_ids: Vec<i64> = dir_indices.iter().map(|&i| entries[i].entry_id).collect();
    let _ = pool.with_conn(|conn| {
        if let Ok(stats_batch) = IndexStore::get_dir_stats_batch_by_ids(conn, &entry_ids) {
            for (i, &idx) in dir_indices.iter().enumerate() {
                if let Some(Some(stats)) = stats_batch.get(i) {
                    entries[idx].size = Some(stats.recursive_logical_size);
                }
            }
        }
    });
}

/// Drop directories whose (dir_stats) size falls outside the query's size filter,
/// and return the adjusted match total. Files are already size-filtered by the
/// engine, so they pass through. A no-op (returns `total` unchanged) when the query
/// has no size filter; otherwise `total` becomes the retained length (approximate,
/// as the exact count would need `dir_stats` for every matching directory).
fn filter_dirs_by_size(entries: &mut Vec<SearchResultEntry>, query: &SearchQuery, total: u32) -> u32 {
    if query.min_size.is_none() && query.max_size.is_none() {
        return total;
    }
    entries.retain(|e| !e.is_directory || size_in_range(e.size, query.min_size, query.max_size));
    entries.len() as u32
}

/// Whether a size (bytes) satisfies the query's min/max bounds. `None` (a directory
/// whose `dir_stats` row is missing) fails any active bound.
fn size_in_range(size: Option<u64>, min: Option<u64>, max: Option<u64>) -> bool {
    if let Some(min) = min {
        match size {
            Some(s) if s >= min => {}
            _ => return false,
        }
    }
    if let Some(max) = max {
        match size {
            Some(s) if s <= max => {}
            _ => return false,
        }
    }
    true
}

/// Adjust a count-only volume total by subtracting the directories whose `dir_stats`
/// size falls outside the query's size filter. `dirs` holds the matching directories
/// the engine handed back for this check (empty when no size filter applies to them,
/// in which case `total` is already exact). Files are already size-filtered upstream.
fn count_only_volume_total(total: u32, dirs: &[SearchResultEntry], query: &SearchQuery) -> u32 {
    let out_of_range = dirs
        .iter()
        .filter(|e| !size_in_range(e.size, query.min_size, query.max_size))
        .count() as u32;
    total.saturating_sub(out_of_range)
}

#[cfg(test)]
mod tests;
