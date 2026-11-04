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
use cmdr_index::ROOT_VOLUME_ID;

use super::context::{NameRules, SearchContext};
use super::index_source::{IndexSource, scope_pairs};
use super::ranking::ImportanceWeights;
use super::seam::{CoveredHalf, SearchError, search_covered};
use super::types::{SearchQuery, SearchResult};
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
    /// Whether any drive index can serve this volume (`BackendKind::can_be_indexed`).
    /// A server's can't, so a live run answers "not covered" without walking it: a
    /// walk stands an index up for the volume it walks.
    can_be_indexed: bool,
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
            can_be_indexed: true,
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
    let volume_id = volume_ids.remove(0);
    Ok(Target {
        can_be_indexed: volume_can_be_indexed(&volume_id, &paths[0]),
        volume_id,
        include_paths: paths.clone(),
        from_scope: true,
    })
}

/// The target for a query its caller already placed on one volume's index
/// (`tools/actions/search/placement.rs`), so its paths aren't routed again.
///
/// The one case path routing can't express: a share that isn't connected. Its
/// locations sit under its last root (`/Volumes/naspi`), which routes to the boot
/// disk once nothing is mounted there, so only the caller's resolution reaches the
/// share's index on disk. Empty include paths are the whole volume, as unscoped is.
fn target_on(query: &SearchQuery, volume_id: &str) -> Target {
    let include_paths = query.include_paths.clone().unwrap_or_default();
    Target {
        can_be_indexed: volume_can_be_indexed(volume_id, include_paths.first().map_or("", String::as_str)),
        volume_id: volume_id.to_string(),
        from_scope: !include_paths.is_empty(),
        include_paths,
    }
}

/// Whether a drive index can serve the volume a scope routed to.
///
/// A registered volume answers for itself (`Volume::capabilities`, the answer the
/// switcher's index badge and `Index::start_volume` read too). A saved server
/// nobody has connected isn't registered, but its path still names its kind, so
/// that answers instead. Any other unregistered volume (an unplugged phone, an
/// ejected drive) may well have an index on disk, so it gets its chance.
fn volume_can_be_indexed(volume_id: &str, scope_path: &str) -> bool {
    match crate::file_system::volume::manager::get_volume_manager().get(volume_id) {
        Some(volume) => volume.capabilities().can_be_indexed,
        None => cmdr_fs::volume::server_of_path(scope_path).is_none_or(|server| server.kind.can_be_indexed()),
    }
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

    let weights = volumes::weights_for(&target.volume_id);
    let home_dir = super::host_home_dir();
    let half = search_covered_half(&query, &target, &loaded, &host_context(&weights, home_dir.as_deref()))
        .map_err(|e| e.to_string())?;

    Ok(SearchResult {
        entries: half.entries,
        total_count: half.total,
        uncovered_scopes: Vec::new(),
        unresolved_scopes: half.unresolved_scopes,
        target_volume_id: target.volume_id,
        hidden_by_excludes: half.hidden_by_excludes,
    })
}

/// Run one volume's search through the seam: the loaded volume answers as an
/// [`IndexSource`], and the scope paths go in beside their canonical forms. The
/// covered half of a live run is this same pass, which is what keeps "indexed or
/// not" a speed difference rather than a behavioral one.
fn search_covered_half(
    query: &SearchQuery,
    target: &Target,
    loaded: &LoadedVolume,
    ctx: &SearchContext<'_>,
) -> Result<CoveredHalf, SearchError> {
    let source = IndexSource(loaded);
    let scope = scope_pairs(&target.include_paths, ctx.home_dir);
    search_covered(&source, &source, query, &scope, ctx)
}

/// What a desktop search runs under: this machine's name rules and home folder,
/// and the volume's importance weights.
fn host_context<'a>(weights: &'a ImportanceWeights, home_dir: Option<&'a str>) -> SearchContext<'a> {
    SearchContext {
        rules: NameRules::host(),
        home_dir,
        weights,
    }
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

#[cfg(test)]
mod tests;
