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
//!
//! ## Two ways to run one
//!
//! - [`run_blocking`] answers from the index and returns. Everything it knows, it
//!   knows already; a scope the index doesn't cover comes back as an honest gap.
//!   It lives here, with the routing and the covered half it shares.
//! - [`start_live`] answers from the index AND walks what the index can't answer
//!   for, reporting over events until it's done. The covered half is the same
//!   engine pass; the difference is that the frontier gets read live rather than
//!   reported as missing. The run itself is `live_run.rs`, over the coverage model
//!   in `coverage.rs`.
//!
//! [`run_live_collected`] is the second of those over a transport that can't
//! carry events: same run, same walk, folded into one reply
//! (`live/collect.rs`). The MCP tools take it, which is all Decision 10's "a
//! thin wrapper on the same path" amounts to in code.
//!
//! The two halves of a live run are complementary by construction: the frontier
//! (`Index::coverage`) is exactly the ground the arena has nothing to say about,
//! so the engine's unfiltered pass over the scope IS the covered half. That's why
//! nothing enumerates covered subtrees, and why the deduplication in
//! `live::ResultStream` is insurance against a race rather than the mechanism.

use crate::index_host::index;
use cmdr_index::store::IndexStore;
use cmdr_index::{ROOT_VOLUME_ID, ReadPool};

use super::engine;
use super::query;
use super::types::{SearchQuery, SearchResult, SearchResultEntry, SearchSort};
use super::volumes::{self, LoadedVolume, VolumeLoad};

mod coverage;
mod live_run;

pub(crate) use live_run::{AGENT_WAIT_DEFAULT, AGENT_WAIT_MAX, LiveSearchStart, run_live_collected, start_live};

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
/// so callers branch on the variant, never on the message.
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

    let loaded = match volumes::ensure_volume(&target.volume_id) {
        VolumeLoad::Loaded(v) => v,
        VolumeLoad::NotIndexed => return Ok(uncovered_result(target)),
        VolumeLoad::Failed(e) => {
            log::warn!("search: volume '{}' isn't searchable: {e}", target.volume_id);
            return Ok(uncovered_result(target));
        }
    };

    let half = search_covered_half(&query, &target, &loaded)?;

    Ok(SearchResult {
        entries: half.entries,
        total_count: half.total,
        uncovered_scopes: Vec::new(),
        unresolved_scopes: half.unresolved_scopes,
        target_volume_id: target.volume_id,
        hidden_by_excludes: half.hidden_by_excludes,
    })
}

/// What one volume's index can answer for a query, on its own.
///
/// The whole of a [`run_blocking`] result, and the covered half of a live run —
/// the same pass either way, which is what keeps "indexed or not" a speed
/// difference rather than a behavioral one.
struct CoveredHalf {
    entries: Vec<SearchResultEntry>,
    total: u32,
    unresolved_scopes: Vec<String>,
    /// Matches the exclusion rules kept out of `total` (`engine::Ranked`).
    hidden_by_excludes: u32,
}

/// Run the engine over `loaded` and finish the result: resolve the scope to entry
/// ids, fill directory sizes from `dir_stats`, apply the size post-filter, and cut
/// the over-fetch back to `limit`.
fn search_covered_half(query: &SearchQuery, target: &Target, loaded: &LoadedVolume) -> Result<CoveredHalf, String> {
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
    let dir_sizes = dir_sizes_for(&vq, &loaded.pool)?;
    let engine::Ranked {
        mut entries,
        total_count: total,
        hidden_by_excludes,
    } = engine::search_ranked(&loaded.index, &vq, &weights, prefix, dir_sizes.as_ref())?;

    if query.count_only {
        // The engine's total is already exact — directory size filters included, since
        // `dir_sizes` applied them inside the scan — and count-only returns no rows.
        entries.clear();
    } else {
        // The rows are the right rows; they just don't carry a directory's recursive
        // size yet, because that isn't in the entries table.
        fill_dir_sizes(&mut entries, &loaded.pool);
    }

    Ok(CoveredHalf {
        entries,
        total,
        unresolved_scopes,
        hidden_by_excludes,
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
        hidden_by_excludes: 0,
    }
}

/// Read the directory sizes this query needs BEFORE the engine ranks anything, or
/// `None` when it needs none.
///
/// A directory's size lives in `dir_stats`, so the arena scan can't judge it. Doing
/// it afterwards, over the ranked top-k, is what made `sizeMin: 50 GB` miss a 1.7 TB
/// `~/Library`: it lost a recency-weighted ranking against hundreds of thousands of
/// freshly-touched folders long before anything looked at its size. Handing the
/// passing set in makes both the filter and `total_count` exact.
///
/// Built only for a query that filters or sorts directories by size, because it's a
/// full scan of `dir_stats` (deliberately unindexed on size — see
/// `IndexStore::dir_sizes_in_range`).
fn dir_sizes_for(query: &SearchQuery, pool: &ReadPool) -> Result<Option<engine::DirSizes>, String> {
    let dirs_included = query.is_directory != Some(false);
    let has_size_filter = query.min_size.is_some() || query.max_size.is_some();
    let sorts_by_size = query.sort_by == Some(SearchSort::Size);
    if !dirs_included || !(has_size_filter || sorts_by_size) {
        return Ok(None);
    }
    // Without a size filter the range is unbounded, so this is every directory:
    // the map is then a SORT KEY, and a directory missing from it is unknown-sized
    // rather than filtered out.
    let (min, max) = (query.min_size, query.max_size);
    // ❌ Never fall back to `None` here. The engine reads a missing map as "no
    // directory size filter to apply", so a failed read would answer with every
    // matching directory regardless of size — a wrong answer wearing a right one's
    // clothes. Failing is the honest outcome.
    let rows = pool
        .with_conn(|conn| IndexStore::dir_sizes_in_range(conn, min, max))
        .map_err(|e| format!("Couldn't read directory sizes: {e}"))?
        .map_err(|e| format!("Couldn't read directory sizes: {e}"))?;
    Ok(Some(engine::DirSizes::new(rows.into_iter().collect(), has_size_filter)))
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

#[cfg(test)]
mod tests;
