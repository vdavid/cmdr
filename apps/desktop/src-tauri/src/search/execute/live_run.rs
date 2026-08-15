//! A live search: the index half, then the walk.
//!
//! One run, three shapes of caller. [`start_live`] hands the dialog a run id and
//! reports over events; [`run_live_collected`] folds the same run into one reply
//! for a transport that can't carry them (`live/collect.rs`); both drive
//! [`run_live_blocking`], which IS the run.
//!
//! The order inside it is the whole design. Ask the coverage question, load the
//! arena the answer is honored against, run the covered half over it, ask for the
//! walk, and only THEN emit — so a run that would have said nothing at all can
//! wait for whoever holds its ground and work the answer out again, having said
//! nothing twice. The coverage model itself is `coverage.rs`; the covered half is
//! the parent module's `search_covered_half`, the same pass a plain
//! `run_blocking` makes.

use crate::index_host::index;
use cmdr_index::CoverageDimension;

use super::coverage::{
    AfterAnotherWalk, CoverageQuestion, UnreadableGround, arena_for_coverage, coverage_kind, coverage_of,
    coverage_scopes, every_frontier_root_is_another_walks,
};
use super::{CoveredHalf, Target, resolve_target, search_covered_half};
use crate::search::excludes::ExcludeRules;
use crate::search::live::{
    self, CollectingSink, LiveAnswer, LiveRun, ResultStream, RunOrigin, SearchEventSink, SearchPhase,
    SearchRunCoverage, SearchRunError, WalkEnding, WalkJudge,
};
use crate::search::matcher::{CompiledQuery, Evaluator};
use crate::search::query;
use crate::search::types::SearchQuery;
use crate::search::volumes::{self, VolumeLoad};

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
pub(super) fn run_live_blocking(query: SearchQuery, target: Target, run: &LiveRun, sink: &dyn SearchEventSink) {
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
        // Ground a walk gave up on reaches this two ways and BOTH have to count.
        // The flag is what THIS walk gave up on; `unreadable.abandoned` is what any
        // walk gave up on durably, which is precisely why the frontier stopped
        // offering it and this run never went there. Without the second half a
        // search over a wedged mount would report itself exhaustive.
        abandoned_ground: abandoned_ground || !unreadable.abandoned.is_empty(),
        // How much of the drive that is, in places rather than folders. Only the
        // durable half has paths to count; a walk that gave up on ground it never
        // recorded leaves this 0, and the note says the honest thing over it.
        abandoned_locations: cmdr_fs::path_locations::location_count(&unreadable.abandoned),
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

/// Everything a live run works out before it emits anything.
///
/// Grouped because that is exactly the repeatable part: no row has gone out, so a
/// run that finds it has nothing to say can do all of it again a moment later
/// without saying anything twice (`another_walk_owns_the_whole_answer`).
struct Groundwork {
    /// What the index can't answer for, and which state of it that describes.
    question: CoverageQuestion,
    /// What the index CAN answer, ready to emit. `None` when the volume has no
    /// index at all, or when the run was stopped before the arena landed.
    half: Option<CoveredHalf>,
    /// Where the volume is mounted, for the walk's own path work.
    mount_root: Option<String>,
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

/// How often a run waiting on somebody else's walk asks whether it's done.
///
/// Short enough that a quick walk isn't followed by an idle pause, long enough
/// that a walk of a whole NAS doesn't pay for a coverage query every frame. The
/// query itself is a row lookup on a scope nothing has listed.
const OTHER_WALK_POLL: std::time::Duration = std::time::Duration::from_millis(200);

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
