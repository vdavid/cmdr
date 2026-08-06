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
//! [`run_live_collected`] is the second of those over a transport that can't
//! carry events: same run, same walk, folded into one reply
//! (`live/collect.rs`). The MCP tools take it, which is all Decision 10's "a
//! thin wrapper on the same path" amounts to in code.
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
    self, CollectingSink, CoverageKind, LiveAnswer, LiveRun, ResultStream, RunOrigin, SearchEventSink, SearchPhase,
    SearchRunCoverage, SearchRunError, WalkEnding, WalkJudge,
};
use super::matcher::{CompiledQuery, Evaluator};
use super::query;
use super::types::{SearchQuery, SearchResult, SearchResultEntry, SearchSort};
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
    let run = live::register(&run_id, &target.volume_id, RunOrigin::Dialog);

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

/// How long an agent's search waits for its answer when it names no budget.
///
/// Long enough for a cold arena on a big drive (10.9 s measured on a 13.5 M-entry
/// NAS index) plus a short walk, short enough to stay inside a typical MCP client's
/// request timeout. Past it the walk keeps going and the reply says so.
pub(crate) const AGENT_WAIT_DEFAULT: std::time::Duration = std::time::Duration::from_secs(20);

/// The longest an agent may ask to wait. A tool call is somebody's turn: past
/// this the honest answer is "here's what I have, ask again".
pub(crate) const AGENT_WAIT_MAX: std::time::Duration = std::time::Duration::from_secs(120);

/// Run a live search for a caller that can't subscribe to its events, and hand
/// back one answer.
///
/// The same run [`start_live`] starts: the same coverage question, the same
/// arena, the same walk, the same matching. Only the reporting differs, because
/// an MCP tool call is one request and one reply (`live/collect.rs` says what
/// survives the flattening).
///
/// Blocks for up to `budget` — call it inside `spawn_blocking`. ❌ Returning
/// does not stop the walk: its rows land in the index either way, so the same
/// search run again continues from where this one left off.
pub(crate) fn run_live_collected(query: SearchQuery, budget: std::time::Duration) -> Result<LiveAnswer, String> {
    // Activity only, no `cancel_idle_timer`: that pairs with the dialog closing
    // to restart it, and an agent has no dialog. The backstop timer stays the
    // one thing that eventually drops the arena.
    volumes::touch_activity();

    let target = resolve_target(&query).map_err(|e| e.to_string())?;
    let volume_id = target.volume_id.clone();
    let run_id = format!("agent-{}", uuid::Uuid::new_v4());
    let run = live::register(&run_id, &volume_id, RunOrigin::Agent);

    let sink = std::sync::Arc::new(CollectingSink::default());
    let run_sink = std::sync::Arc::clone(&sink);
    let spawned = std::thread::Builder::new().name("search-agent".into()).spawn(move || {
        run_live_blocking(query, target, &run, run_sink.as_ref());
        live::deregister(&run.run_id);
    });
    if let Err(e) = spawned {
        live::deregister(&run_id);
        return Err(format!("Search couldn't start: {e}"));
    }

    Ok(sink.answer_within(budget.min(AGENT_WAIT_MAX), volume_id))
}

/// A live search, start to terminal event. Long-lived and synchronous; call it
/// on a thread of its own.
fn run_live_blocking(query: SearchQuery, target: Target, run: &LiveRun, sink: &dyn SearchEventSink) {
    let mut stream = ResultStream::new(run, sink, &query);
    stream.announce(SearchPhase::ResolvingCoverage);

    // 1-3. What the index can't answer for, what it CAN, and where the volume
    //      lives — the whole of what a run works out before it emits anything, so
    //      a run that would have emitted nothing can do it again after waiting.
    let scopes = coverage_scopes(&target);
    let mut ground = match groundwork(&query, &target, &scopes, run, AfterAnotherWalk::No) {
        Ok(ground) => ground,
        Err((error, message)) => {
            stream.fail(error, message);
            return;
        }
    };
    // 4. Ask for the walk — which is also the only trustworthy answer to "is this
    //    ground somebody else's?", because a claim is taken inside `cover` and
    //    nothing read earlier can see it. Two searches started a moment apart come
    //    out of one arena load together and arrive here microseconds apart.
    //
    //    A run that gets none of the ground AND had nothing from the index would
    //    answer with nothing at all, so instead it waits for the walk that holds
    //    the ground and works the whole thing out again. Nothing has gone out yet,
    //    so that costs the answer nothing and says nothing twice.
    let mut started = None;
    loop {
        if ground.question.frontier.is_empty() || run.is_cancelled() {
            break;
        }
        // A live walk refuses a query that narrows nothing, whatever the arena
        // would have allowed: the arena's cost is knowable and a filesystem's
        // isn't (`matcher.rs`). Refusing the RUN rather than answering from the
        // index alone is the honest half — a confident-looking list that silently
        // skipped the unindexed ground is what this effort exists to remove.
        let compiled = match CompiledQuery::compile(&query, Evaluator::LiveWalk) {
            Ok(compiled) => compiled,
            Err(e) => {
                stream.fail(SearchRunError::Query, e.to_string());
                return;
            }
        };
        match index().cover(
            &target.volume_id,
            ground.question.frontier.clone(),
            CoverageDimension::Listing,
            run.cancel_token(),
        ) {
            Ok(walk) => {
                let deferred = walk.covered_by_another_walk().to_vec();
                if deferred.len() < ground.question.frontier.len() || !index_gave_nothing(&ground) {
                    started = Some((walk, deferred, compiled));
                    break;
                }
                // It took no ground at all, and there's nothing to show meanwhile.
                // `finish` rather than a drop: it claimed nothing, so this only
                // closes the session it opened and joins its thread.
                let _ = walk.finish();
                wait_for_the_other_walk(&target.volume_id, &scopes, run, &mut stream);
                if run.is_cancelled() {
                    break;
                }
                match groundwork(&query, &target, &scopes, run, AfterAnotherWalk::Yes) {
                    Ok(next) => ground = next,
                    Err((error, message)) => {
                        stream.fail(error, message);
                        return;
                    }
                }
            }
            Err(e) => {
                // Nothing to walk with: the drive isn't mounted, or it's mid-scan
                // (in which case the scan is covering that ground anyway). Either
                // way this run's answer is a lower bound and says so.
                log::warn!("Live search: can't walk '{}': {e}", target.volume_id);
                break;
            }
        }
    }
    let unwalkable = started.is_none() && !ground.question.frontier.is_empty() && !run.is_cancelled();

    let Groundwork {
        question,
        half,
        mount_root,
    } = ground;

    // 5. The covered half goes out now: it was computed against the arena the
    //    coverage answer describes (Decision 12), and asking for the walk first is
    //    what makes the wait above possible without ever emitting twice.
    let mut unresolved_scopes = Vec::new();
    if let Some(half) = half {
        unresolved_scopes = half.unresolved_scopes;
        stream.add_indexed(half.entries, half.total, half.hidden_by_excludes);
    }

    // 6. The rest, walked live.
    // The scope paths the walk is about to answer for itself, in the canonical
    // form both halves speak.
    let walked_scopes: std::collections::HashSet<&String> =
        question.frontier.iter().filter(|root| scopes.contains(root)).collect();
    let kind = coverage_kind(&question.frontier, &scopes);
    let report = |walk: WalkEnding,
                  unreadable: UnreadableGround,
                  still_covering: Vec<String>,
                  capped: bool,
                  abandoned_ground: bool| SearchRunCoverage {
        // A scope the INDEX couldn't resolve isn't a gap once the walk has been
        // to it: the walk is the probe, and it just answered. Only a walk that
        // ran to the end proves it, so anything short leaves the signal
        // standing. Without this, the very case this milestone exists for — a
        // folder too new to be indexed — would show "Cmdr doesn't cover this
        // folder" over a complete list of its files.
        unresolved_scopes: match walk {
            WalkEnding::Completed => unresolved_scopes
                .iter()
                .filter(|scope| !walked_scopes.contains(&query::canonicalize_scope_path(scope)))
                .cloned()
                .collect(),
            _ => unresolved_scopes.clone(),
        },
        walk,
        kind,
        permission_denied: unreadable.permission_denied,
        declined: unreadable.declined,
        still_covering,
        abandoned_ground,
        capped,
        target_volume_id: target.volume_id.clone(),
        // Stamped by the stream on the way out (`ResultStream::finish`), which is
        // the only place that has seen BOTH halves' exclusion drops.
        hidden_by_excludes: 0,
    };

    // Nothing left to walk, or nobody left waiting for it, or nothing to walk it
    // with. The stopped case ends here rather than starting a walk that would be
    // cancelled on its first check — and, on a drive with no index, would have
    // stood one up on the way. `finish` relabels the ending when the run was
    // stopped.
    let Some((walk, still_covering, compiled)) = started else {
        let coverage = report(
            if unwalkable {
                WalkEnding::Interrupted
            } else {
                WalkEnding::NothingToWalk
            },
            question.unreadable,
            Vec::new(),
            stream.capped(),
            false,
        );
        stream.finish(coverage);
        return;
    };
    let excludes = ExcludeRules::from_query(&query, compiled.case_insensitive());

    // The arena behind this search is out of date from here on. Marked at the
    // START, not on the first batch: a walk can write rows it never emits — the
    // local repair path for a frontier root that already holds rows writes
    // through the serial reconcile, which has no live consumer — and those rows
    // would otherwise be pruned as covered by the next query and served from an
    // arena that predates them.
    volumes::mark_walked_behind(&target.volume_id);

    let attempted_roots = question.frontier.len().saturating_sub(still_covering.len());
    let home_dir = dirs::home_dir().map(|home| home.to_string_lossy().into_owned());
    let judge = WalkJudge {
        compiled: &compiled,
        excludes: &excludes,
        volume_root: mount_root.as_deref(),
        home_dir: home_dir.as_deref(),
    };
    let walked = live::drive_walk(walk, attempted_roots, &judge, &mut stream);

    // What nothing is going to walk, re-read now the walk has stamped what it
    // found: a folder it was refused (no Full Disk Access) carries its cause only
    // once something has tried, so the answer from before the walk would be silent
    // on exactly the case the user can act on.
    let unreadable = match walked.ending {
        WalkEnding::Cancelled => question.unreadable,
        _ => coverage_of(&target.volume_id, &scopes).unreadable,
    };
    let coverage = report(
        walked.ending,
        unreadable,
        still_covering,
        stream.capped(),
        walked.abandoned_ground,
    );
    stream.finish(coverage);
}

/// Which ground a run's answer is drawn from, decided by the coverage question
/// and nothing downstream of it.
///
/// A scope root that is itself a frontier root was covered by NOTHING, so a run
/// where that holds for every scope answers entirely off the walk. One where it
/// holds for none of them still had ground to walk somewhere below, which is the
/// mixed case. Pure, because it's the measure of how often a search still needs
/// to walk at all and that number is worth being able to test.
fn coverage_kind(frontier: &[String], scopes: &[String]) -> CoverageKind {
    if frontier.is_empty() {
        return CoverageKind::Covered;
    }
    if scopes.iter().all(|scope| frontier.contains(scope)) {
        return CoverageKind::Live;
    }
    CoverageKind::Mixed
}

/// Directories nothing is going to walk, split by WHOSE refusal it was: the two
/// are different sentences on screen, and only the first is one the user can act
/// on (`crates/cmdr-index`'s `UnreadableCause`).
#[derive(Default)]
struct UnreadableGround {
    /// A walk tried and the OS refused.
    permission_denied: Vec<String>,
    /// No walk will read it: a NAS snapshot tree.
    declined: Vec<String>,
}

impl UnreadableGround {
    /// Fold one scope's answer in.
    fn extend(&mut self, map: &cmdr_index::CoverageMap) {
        self.permission_denied.extend(map.permission_denied.iter().cloned());
        self.declined.extend(map.declined.iter().cloned());
    }

    /// One order, no duplicates, however many scopes contributed.
    fn settle(&mut self) {
        for list in [&mut self.permission_denied, &mut self.declined] {
            list.sort_unstable();
            list.dedup();
        }
    }
}

/// A coverage answer over a query's scopes, merged.
struct CoverageQuestion {
    /// Every frontier root, across every scope path.
    frontier: Vec<String>,
    /// Every directory nothing will walk, across every scope path.
    unreadable: UnreadableGround,
    /// The token each answer carried. All of them have to match the arena's for
    /// the covered half to be trustworthy (Decision 12).
    tokens: Vec<CoverageToken>,
    /// The frontier roots another walk is covering as this was read. This run
    /// can't have them: one walk per patch of ground, or the two orphan each
    /// other's subtrees.
    being_walked: Vec<String>,
}

/// How often a run waiting on somebody else's walk asks whether it's done.
///
/// Short enough that a quick walk isn't followed by an idle pause, long enough
/// that a walk of a whole NAS doesn't pay for a coverage query every frame. The
/// query itself is a row lookup on a scope nothing has listed.
const OTHER_WALK_POLL: std::time::Duration = std::time::Duration::from_millis(200);

/// Everything a live run works out before it emits anything.
///
/// Grouped because that is exactly the repeatable part: no row has gone out, so a
/// run that finds it has nothing to say can do all of it again a moment later
/// without saying anything twice ([`another_walk_owns_the_whole_answer`]).
struct Groundwork {
    /// What the index can't answer for, and which state of it that describes.
    question: CoverageQuestion,
    /// What the index CAN answer, ready to emit. `None` when the volume has no
    /// index at all, or when the run was stopped before the arena landed.
    half: Option<CoveredHalf>,
    /// Where the volume is mounted, for the walk's own path work.
    mount_root: Option<String>,
}

/// Whether this groundwork is being redone after watching somebody else's walk
/// end, which is its own reason to trust nothing the arena holds.
#[derive(Clone, Copy, PartialEq, Eq)]
enum AfterAnotherWalk {
    No,
    Yes,
}

/// Ask the coverage question, load the arena the answer is honored against
/// (Decision 12: in that order, so the arena holds every row the answer calls
/// covered), and run the covered half over it.
///
/// The covered half is skipped outright for a run somebody stopped while that
/// arena was loading: the wait is seconds on a big drive, and a scan nobody will
/// see is the cheapest work there is to not do.
fn groundwork(
    query: &SearchQuery,
    target: &Target,
    scopes: &[String],
    run: &LiveRun,
    after: AfterAnotherWalk,
) -> Result<Groundwork, (SearchRunError, String)> {
    let question = coverage_of(&target.volume_id, scopes);

    let loaded = match arena_for_coverage(&target.volume_id, &question.tokens, after) {
        VolumeLoad::Loaded(loaded) => Some(loaded),
        // Not an error and not a gap any more: a volume with no index is exactly
        // what the walk stands one up for. Nothing is covered, so the frontier
        // (the scope itself) is the whole answer.
        VolumeLoad::NotIndexed => None,
        VolumeLoad::Failed(e) => {
            log::warn!("Live search: volume '{}' isn't searchable: {e}", target.volume_id);
            return Err((
                SearchRunError::IndexUnreadable,
                "Cmdr can't read this drive's index. Re-indexing the drive fixes it.".to_string(),
            ));
        }
    };

    let half = match loaded.as_deref().filter(|_| !run.is_cancelled()) {
        Some(loaded) => {
            Some(search_covered_half(query, target, loaded).map_err(|message| (SearchRunError::Query, message))?)
        }
        None => None,
    };

    Ok(Groundwork {
        mount_root: loaded
            .as_deref()
            .and_then(|loaded| loaded.mount_root.clone())
            .or_else(|| volumes::registry_mount_root(&target.volume_id)),
        question,
        half,
    })
}

/// Whether the index gave this run nothing at all: no rows, and no count.
///
/// Half of the reason to wait for somebody else's walk (the other half is that
/// the walk request came back holding no ground). ❌ A run the index DID answer
/// for never waits: those rows are worth showing now, and holding them back for
/// somebody else's frontier would break Decision 11's promise that a refined
/// query keeps what its predecessor covered. That run reports `still_covering`
/// and says the rest arrives later, which is true for it.
fn index_gave_nothing(ground: &Groundwork) -> bool {
    ground
        .half
        .as_ref()
        .is_none_or(|half| half.entries.is_empty() && half.total == 0)
}

/// Whether there IS uncovered ground and every bit of it belongs to a walk
/// already running — the cheap question the wait loop re-asks.
fn every_frontier_root_is_another_walks(question: &CoverageQuestion) -> bool {
    !question.frontier.is_empty()
        && question
            .frontier
            .iter()
            .all(|root| question.being_walked.contains(root))
}

/// Wait until the ground this run needs stops being another walk's.
///
/// ❌ The coverage question only, never the arena: rebuilding a multi-second
/// snapshot of a big drive on every poll would cost more than the walk it's
/// waiting for. The full groundwork runs ONCE, after this returns.
///
/// It ends when the ground is free — the other walk finished, or stopped and left
/// a smaller frontier behind — or when somebody stops this run. ❌ No deadline:
/// the only alternative to waiting is the empty answer this exists to remove, and
/// every caller can stop it. Escape and the dialog closing cancel; an agent's wait
/// is its own transport budget, and past it the reply says the walk is still
/// going.
fn wait_for_the_other_walk(volume_id: &str, scopes: &[String], run: &LiveRun, stream: &mut ResultStream<'_>) {
    log::debug!("Live search: '{volume_id}' is being walked by another search; waiting for it");
    loop {
        // Say so every turn: the run is working, and this is the phase it's in.
        stream.announce(SearchPhase::ResolvingCoverage);
        std::thread::sleep(OTHER_WALK_POLL);
        if run.is_cancelled() || !every_frontier_root_is_another_walks(&coverage_of(volume_id, scopes)) {
            return;
        }
    }
}

/// Ask the index what it can't answer for, over every scope path in turn.
fn coverage_of(volume_id: &str, scopes: &[String]) -> CoverageQuestion {
    let mut question = CoverageQuestion {
        frontier: Vec::new(),
        unreadable: UnreadableGround::default(),
        tokens: Vec::new(),
        being_walked: Vec::new(),
    };
    for scope in scopes {
        match index().coverage(volume_id, scope, CoverageDimension::Listing) {
            Ok(map) => {
                question.unreadable.extend(&map);
                question.frontier.extend(map.frontier);
                question.being_walked.extend(map.being_walked);
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
    question.being_walked.sort_unstable();
    question.being_walked.dedup();
    question.unreadable.settle();
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
fn arena_for_coverage(volume_id: &str, tokens: &[CoverageToken], after: AfterAnotherWalk) -> VolumeLoad {
    let load = volumes::ensure_volume(volume_id);
    let VolumeLoad::Loaded(ref loaded) = load else {
        return load;
    };
    if tokens.iter().all(|token| *token == loaded.coverage_token) {
        // Exactly the rows the answer was computed against.
        volumes::take_walked_behind(volume_id);
        return load;
    }
    // A run that WATCHED another walk end doesn't need the mark to know a walk
    // wrote rows: it waited for that walk, and its own reason for waiting was
    // that the rows would be there afterwards. The mark is a global one-shot, so
    // whoever else consumed it must not cost this run the reload.
    if after == AfterAnotherWalk::No && !volumes::take_walked_behind(volume_id) {
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
