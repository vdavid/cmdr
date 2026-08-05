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
//! - [`start_live`] answers from the index AND walks what the index can't answer
//!   for, reporting over events until it's done. The covered half is the same
//!   engine pass; the difference is that the frontier gets read live rather than
//!   reported as missing.
//!
//! The two halves of a live run are complementary by construction: the frontier
//! (`Index::coverage`) is exactly the ground the arena has nothing to say about,
//! so the engine's unfiltered pass over the scope IS the covered half. That's why
//! nothing enumerates covered subtrees, and why the deduplication in
//! [`live::ResultStream`] is insurance against a race rather than the mechanism.

use crate::index_host::index;
use cmdr_index::store::IndexStore;
use cmdr_index::{CoverageDimension, CoverageToken, ROOT_VOLUME_ID, ReadPool};

use super::engine;
use super::excludes::ExcludeRules;
use super::live::{
    self, LiveRun, ResultStream, SearchEventSink, SearchPhase, SearchRunCoverage, SearchRunError, WalkEnding, WalkJudge,
};
use super::matcher::{CompiledQuery, Evaluator};
use super::query;
use super::types::{SearchQuery, SearchResult, SearchResultEntry};
use super::volumes::{self, LoadedVolume, VolumeLoad};

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

    let half = search_covered_half(&query, &target, &loaded, limit)?;

    Ok(SearchResult {
        entries: half.entries,
        total_count: half.total,
        uncovered_scopes: Vec::new(),
        unresolved_scopes: half.unresolved_scopes,
        target_volume_id: target.volume_id,
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
}

/// Run the engine over `loaded` and finish the result: resolve the scope to entry
/// ids, fill directory sizes from `dir_stats`, apply the size post-filter, and cut
/// the over-fetch back to `limit`.
fn search_covered_half(
    query: &SearchQuery,
    target: &Target,
    loaded: &LoadedVolume,
    limit: usize,
) -> Result<CoveredHalf, String> {
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

    Ok(CoveredHalf {
        entries,
        total,
        unresolved_scopes,
    })
}

// ── The live path: the index half, then the walk ─────────────────────

/// What starting a live search hands back before any of it has happened.
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LiveSearchStart {
    /// Echoed from the request. Every event this run emits carries it.
    pub run_id: String,
    /// The ONE volume routing picked, known before the search has read anything,
    /// so the UI can name the drive it's about to search.
    pub target_volume_id: String,
}

/// Start a live search: register the run, and drive it on a thread of its own.
///
/// Returns as soon as routing has picked a volume, which is the only part that
/// can fail fast. Everything after that — a multi-second arena load, a walk that
/// may run for minutes — reports over events, because an IPC command may not sit
/// on the handler thread while it happens (`commands/CLAUDE.md`).
///
/// Starting a run SUPERSEDES every other one: the dialog asks one question at a
/// time. ❌ That is not a cancel — their walks keep going (Decision 11).
pub(crate) fn start_live(app: tauri::AppHandle, query: SearchQuery, run_id: String) -> Result<LiveSearchStart, String> {
    volumes::touch_activity();
    volumes::cancel_idle_timer();

    let target = resolve_target(&query).map_err(|e| e.to_string())?;
    let started = LiveSearchStart {
        run_id: run_id.clone(),
        target_volume_id: target.volume_id.clone(),
    };
    let run = live::register(&run_id, &target.volume_id);

    let spawned = std::thread::Builder::new().name("search-live".into()).spawn(move || {
        let sink = live::TauriSearchEventSink::new(app);
        run_live_blocking(query, target, &run, &sink);
        live::deregister(&run.run_id);
    });
    if let Err(e) = spawned {
        live::deregister(&run_id);
        return Err(format!("Search couldn't start: {e}"));
    }
    Ok(started)
}

/// A live search, start to terminal event. Long-lived and synchronous; call it
/// on a thread of its own.
fn run_live_blocking(query: SearchQuery, target: Target, run: &LiveRun, sink: &dyn SearchEventSink) {
    let mut stream = ResultStream::new(run, sink, &query);
    stream.announce(SearchPhase::ResolvingCoverage);

    // 1. What the index can't answer for, and which state of the index that
    //    answer describes. Asked BEFORE the arena is loaded, so an arena loaded
    //    after it holds every row it calls covered.
    let scopes = coverage_scopes(&target);
    let question = coverage_of(&target.volume_id, &scopes);

    // 2. The arena the answer is honored against (Decision 12).
    let loaded = match arena_for_coverage(&target.volume_id, &question.tokens) {
        VolumeLoad::Loaded(loaded) => Some(loaded),
        // Not an error and not a gap any more: a volume with no index is exactly
        // what the walk stands one up for. Nothing is covered, so the frontier
        // (the scope itself) is the whole answer.
        VolumeLoad::NotIndexed => None,
        VolumeLoad::Failed(e) => {
            log::warn!("Live search: volume '{}' isn't searchable: {e}", target.volume_id);
            stream.fail(
                SearchRunError::IndexUnreadable,
                "Cmdr can't read this drive's index. Re-indexing the drive fixes it.".to_string(),
            );
            return;
        }
    };

    // 3. The covered half, from the arena, exactly as a non-live search reads it.
    let mut unresolved_scopes = Vec::new();
    if let Some(loaded) = loaded.as_deref() {
        let limit = query.limit.min(1000) as usize;
        match search_covered_half(&query, &target, loaded, limit) {
            Ok(half) => {
                unresolved_scopes = half.unresolved_scopes;
                stream.add_indexed(half.entries, half.total);
            }
            Err(message) => {
                stream.fail(SearchRunError::Query, message);
                return;
            }
        }
    }

    // 4. The rest, walked live.
    let mount_root = loaded
        .as_deref()
        .and_then(|loaded| loaded.mount_root.clone())
        .or_else(|| volumes::registry_mount_root(&target.volume_id));
    let report =
        |walk: WalkEnding, unreadable: Vec<String>, still_covering: Vec<String>, capped: bool| SearchRunCoverage {
            walk,
            unreadable,
            still_covering,
            unresolved_scopes: unresolved_scopes.clone(),
            capped,
            target_volume_id: target.volume_id.clone(),
        };

    if question.frontier.is_empty() {
        let coverage = report(
            WalkEnding::NothingToWalk,
            question.unreadable,
            Vec::new(),
            stream.capped(),
        );
        stream.finish(coverage);
        return;
    }

    // A live walk refuses a query that narrows nothing, whatever the arena would
    // have allowed: the arena's cost is knowable and a filesystem's isn't
    // (`matcher.rs`). Refusing the RUN rather than answering from the index alone
    // is the honest half — a confident-looking list that silently skipped the
    // unindexed ground is what this whole effort exists to remove.
    let compiled = match CompiledQuery::compile(&query, Evaluator::LiveWalk) {
        Ok(compiled) => compiled,
        Err(e) => {
            stream.fail(SearchRunError::Query, e.to_string());
            return;
        }
    };
    let excludes = ExcludeRules::from_query(&query, compiled.case_insensitive());

    let walk = match index().cover(
        &target.volume_id,
        question.frontier.clone(),
        CoverageDimension::Listing,
        run.cancel_token(),
    ) {
        Ok(walk) => walk,
        Err(e) => {
            // Nothing to walk with: the drive isn't mounted, or it's mid-scan (in
            // which case the scan is covering that ground anyway). Either way this
            // run's answer is a lower bound and says so.
            log::warn!("Live search: can't walk '{}': {e}", target.volume_id);
            let coverage = report(
                WalkEnding::Interrupted,
                question.unreadable,
                Vec::new(),
                stream.capped(),
            );
            stream.finish(coverage);
            return;
        }
    };

    // The arena behind this search is out of date from here on. Marked at the
    // START, not on the first batch: a walk can write rows it never emits — the
    // local repair path for a frontier root that already holds rows writes
    // through the serial reconcile, which has no live consumer — and those rows
    // would otherwise be pruned as covered by the next query and served from an
    // arena that predates them.
    volumes::mark_walked_behind(&target.volume_id);

    let still_covering = walk.covered_by_another_walk().to_vec();
    let attempted_roots = question.frontier.len().saturating_sub(still_covering.len());
    let home_dir = dirs::home_dir().map(|home| home.to_string_lossy().into_owned());
    let judge = WalkJudge {
        compiled: &compiled,
        excludes: &excludes,
        volume_root: mount_root.as_deref(),
        home_dir: home_dir.as_deref(),
    };
    let ending = live::drive_walk(walk, attempted_roots, &judge, &mut stream);

    // What nothing is going to walk, re-read now the walk has stamped what it
    // found: a folder it was refused (no Full Disk Access) is `known_unreadable`
    // only once something has tried, so the answer from before the walk would be
    // silent on exactly the case the user can act on.
    let unreadable = match ending {
        WalkEnding::Cancelled => question.unreadable,
        _ => coverage_of(&target.volume_id, &scopes).unreadable,
    };
    let coverage = report(ending, unreadable, still_covering, stream.capped());
    stream.finish(coverage);
}

/// A coverage answer over a query's scopes, merged.
struct CoverageQuestion {
    /// Every frontier root, across every scope path.
    frontier: Vec<String>,
    /// Every directory nothing will walk, across every scope path.
    unreadable: Vec<String>,
    /// The token each answer carried. All of them have to match the arena's for
    /// the covered half to be trustworthy (Decision 12).
    tokens: Vec<CoverageToken>,
}

/// Ask the index what it can't answer for, over every scope path in turn.
fn coverage_of(volume_id: &str, scopes: &[String]) -> CoverageQuestion {
    let mut question = CoverageQuestion {
        frontier: Vec::new(),
        unreadable: Vec::new(),
        tokens: Vec::new(),
    };
    for scope in scopes {
        match index().coverage(volume_id, scope, CoverageDimension::Listing) {
            Ok(map) => {
                question.frontier.extend(map.frontier);
                question.unreadable.extend(map.unreadable);
                question.tokens.push(map.token);
            }
            Err(e) => {
                // An index that can't say what it covers can't be trusted to have
                // covered anything, so the scope goes to the walk whole — the same
                // conservative answer the coverage query gives itself when the
                // exclusion policy stamp doesn't match.
                log::warn!("Live search: no coverage answer for '{scope}': {e}");
                question.frontier.push(scope.clone());
            }
        }
    }
    question.frontier.sort_unstable();
    question.frontier.dedup();
    question.unreadable.sort_unstable();
    question.unreadable.dedup();
    question
}

/// The scope paths to ask about: the query's own include paths, canonicalized the
/// same way the index-side resolution canonicalizes them (a symlinked `/tmp` and
/// the index's `/private/tmp` have to be the same folder), or the whole volume
/// when the query has no scope.
fn coverage_scopes(target: &Target) -> Vec<String> {
    if target.include_paths.is_empty() {
        return vec![volumes::registry_mount_root(&target.volume_id).unwrap_or_else(|| "/".to_string())];
    }
    target
        .include_paths
        .iter()
        .map(|path| query::canonicalize_scope_path(path))
        .collect()
}

/// The arena a coverage answer may be honored against (Decision 12).
///
/// A coverage answer that calls a subtree covered is a promise the arena holds
/// its rows. A walk that wrote rows behind the arena breaks that promise, and the
/// symptom is silent: the same query, run again, prunes the ground it just walked
/// and returns FEWER results than the first time.
///
/// So: reload when the tokens disagree AND a walk is what put them out of step.
/// Both halves earn their keep. Without the token, every query after any walk
/// would pay a full arena rebuild. Without the walk mark, a boot disk — whose
/// background indexer moves the token several times a second — would rebuild in
/// front of nearly every search, which is the regression `volumes::get_loaded`
/// documents removing once already. What's left uncovered is ordinary index lag,
/// which search has always had.
fn arena_for_coverage(volume_id: &str, tokens: &[CoverageToken]) -> VolumeLoad {
    let load = volumes::ensure_volume(volume_id);
    let VolumeLoad::Loaded(ref loaded) = load else {
        return load;
    };
    if tokens.iter().all(|token| *token == loaded.coverage_token) {
        // Exactly the rows the answer was computed against.
        volumes::take_walked_behind(volume_id);
        return load;
    }
    if !volumes::take_walked_behind(volume_id) {
        return load;
    }
    // Loaded strictly after the coverage answer was taken, so it holds every row
    // that answer calls covered, whatever else landed meanwhile.
    log::debug!("Live search: reloading '{volume_id}'s arena, a walk wrote rows behind it");
    volumes::reload_volume(volume_id)
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
