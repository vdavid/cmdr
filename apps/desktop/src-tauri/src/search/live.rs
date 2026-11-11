//! A search that keeps going: the app's one run registry, and the walk feeding a
//! run.
//!
//! `execute.rs` orchestrates a live search (coverage → the index half → the
//! walk). The run registry, [`ResultStream`], the run-state data, and the one
//! terminal report ([`CoverageFacts`]) are `cmdr-search`'s (`cmdr_search::run`);
//! this module holds the app's ONE [`SearchRuns`] and owns what consumes
//! `cmdr-index`'s walk:
//!
//! - **[`drive_walk`]**, the pump between the walk's thread and the run's.
//! - **[`WalkJudge`]**, which matches a walked entry through the crate's
//!   `CompiledQuery` and `ExcludeRules`.
//!
//! ## Superseding is not cancelling
//!
//! A superseded run's driver keeps draining (Decision 11), for two reasons that
//! both matter: the bounded channel would otherwise stall the walk, and the arena
//! mark ([`volumes::mark_walked_behind`]) has to keep pace with the rows the walk
//! is still writing, or the next query prunes them as covered and shows FEWER
//! results than the one before it (Decision 12).
//!
//! An MCP call gets the same run, the same walk, and the same answer, folded into
//! one reply by [`collect`] because its transport can't carry a stream.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, SyncSender, sync_channel};
use std::sync::{Arc, LazyLock, Mutex};

use cmdr_index::{CoverOutcome, CoverWalk, CoveredEntry};

use crate::ignore_poison::IgnorePoison;

use super::engine::{derive_icon_id, home_relative_parent};
use super::excludes::ExcludeRules;
use super::matcher::CompiledQuery;
use super::types::SearchResultEntry;
use super::volumes;

pub(crate) mod collect;
pub(crate) mod events;
mod walked;

pub(crate) use cmdr_search::run::{
    BATCH_INTERVAL, CoverageFacts, CoverageKind, LiveRun, ResultStream, RunOrigin, RunSink, SearchPhase,
    SearchRunCoverage, SearchRunError, SearchRuns, UnreadableGround, UnresolvedScope, WalkEnding,
};
pub(crate) use collect::{AnswerEnding, CollectingSink, LiveAnswer};
pub(crate) use events::TauriSearchEventSink;

/// How many walk messages may queue between the walk's thread and the run's.
/// Small: each carries a whole batch, and the walk's own channel is already
/// bounded behind it.
const WALK_QUEUE_DEPTH: usize = 4;

// ── The app's runs ───────────────────────────────────────────────────

/// Every run in flight in this process, so the dialog closing or the app quitting
/// reaches each of them from wherever it notices.
static RUNS: LazyLock<SearchRuns> = LazyLock::new(SearchRuns::new);

/// Register a run, superseding the dialog runs it speaks over
/// ([`SearchRuns::register`]).
pub(crate) fn register(run_id: &str, volume_id: &str, origin: RunOrigin) -> Arc<LiveRun> {
    RUNS.register(run_id, volume_id, origin)
}

/// Forget a run that has reached its terminal state.
pub(crate) fn deregister(run_id: &str) {
    RUNS.deregister(run_id);
}

/// Stop one run and its walk. Returns whether there was one to stop.
pub(crate) fn cancel_live_run(run_id: &str) -> bool {
    RUNS.cancel(run_id)
}

/// Whether the run named will still report its ending ([`SearchRuns::is_live`]).
pub(crate) fn is_live_run(run_id: &str) -> bool {
    RUNS.is_live(run_id)
}

/// Stop every run and every walk behind them: the app quitting.
pub(crate) fn cancel_all_live_runs() {
    RUNS.cancel_all();
}

/// Stop every DIALOG run except the one named: the dialog closing
/// ([`SearchRuns::cancel_dialog_runs_except`]).
pub(crate) fn cancel_dialog_runs_except(keep_run_id: Option<&str>) {
    RUNS.cancel_dialog_runs_except(keep_run_id);
}

/// Serializes the app tests that register a run in the app's one registry.
/// Registering supersedes every OTHER dialog run in it, so two tests running at
/// once would silence each other. The crate's own tests take a fresh
/// [`SearchRuns`] each and need no lock.
#[cfg(test)]
pub(crate) fn test_registry_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock_ignore_poison()
}

// ── The walk, pumped into the stream ─────────────────────────────────

/// What the walk's own thread hands to the run's thread.
enum WalkMsg {
    /// Entries the walk discovered.
    Batch(Vec<CoveredEntry>),
    /// The walk ended, and what it covered.
    Ended(CoverOutcome),
}

/// Everything matching a walked entry needs, resolved once per run.
pub(crate) struct WalkJudge<'a> {
    /// The same predicates the arena scan applied, by construction
    /// (`matcher.rs`).
    pub(crate) compiled: &'a CompiledQuery,
    /// The same exclusions the arena scan applied (`excludes.rs`), against a
    /// walked entry's own path rather than an ancestor id chain.
    pub(crate) excludes: &'a ExcludeRules,
    /// Where the ancestor walk stops: the volume's mount root, or `None` for the
    /// boot volume.
    pub(crate) volume_root: Option<&'a str>,
    /// The absolute home directory, for the `~` in a row's parent path.
    pub(crate) home_dir: Option<&'a str>,
}

impl WalkJudge<'_> {
    /// Judge one batch and hand what survives to the stream.
    ///
    /// ❌ It reports no progress: where the walk is comes from the walk's own
    /// heartbeat (see [`set_walk_progress`](ResultStream::set_walk_progress)), so
    /// a run that is receiving no batches still shows one.
    fn consume(&self, batch: Vec<CoveredEntry>, stream: &mut ResultStream<'_>) {
        if !stream.wants_results() {
            return;
        }
        for entry in batch {
            if !walked::matches_walked(self.compiled, &entry) {
                continue;
            }
            let path = entry.path.to_string_lossy();
            if self.excludes.excludes_walked(&path, self.volume_root) {
                stream.note_excluded();
                continue;
            }
            stream.add_walked(live_result_entry(&entry, &path, self.home_dir));
        }
    }
}

/// One walked entry as a result row.
///
/// `entry_id` is `0`: a walked entry has no arena id, and nothing downstream may
/// use one to look it up. Its size is its own (pre-hardlink-dedup, what a listing
/// shows), and a directory arrives without a recursive size — `dir_stats` doesn't
/// exist for ground that was walked a moment ago (Accepted difference 5).
fn live_result_entry(entry: &CoveredEntry, path: &str, home_dir: Option<&str>) -> SearchResultEntry {
    // The name the matcher judged it under, not a second derivation of it: a row
    // shown under a different name than it matched by is the same silent fork
    // `matcher.rs` exists to prevent.
    let name = walked::covered_name(&entry.path).into_owned();
    SearchResultEntry {
        icon_id: derive_icon_id(&name, entry.is_directory),
        name,
        parent_path: home_relative_parent(path, home_dir),
        path: path.to_string(),
        is_directory: entry.is_directory,
        size: entry.logical_size,
        modified_at: entry.modified_at,
        entry_id: 0,
    }
}

/// How a walk ended, and whether what it read was all there was.
///
/// Two answers rather than one, because they're independent: a walk can cover
/// every root it took, uncancelled, and still have abandoned directories inside
/// them (Accepted difference 9). Folding that into [`WalkEnding::Interrupted`]
/// would make the UI say the drive went away, which isn't what happened.
pub(crate) struct WalkResult {
    pub(crate) ending: WalkEnding,
    /// The walk gave up on ground it started, so its rows are a lower bound
    /// whatever `ending` says.
    pub(crate) abandoned_ground: bool,
    /// Frontier roots the walk couldn't cover while the volume stayed there
    /// (`CoverOutcome::failed_roots`). Each one is a folder nothing read.
    pub(crate) failed_roots: Vec<String>,
}

/// Drive a running walk into `stream` until it ends or somebody stops it.
///
/// `attempted_roots` is how many frontier roots this walk actually took (the ones
/// another walk already claimed are not its to cover), which is what decides
/// whether a finished walk covered its ground or stopped short of it.
pub(crate) fn drive_walk(
    walk: CoverWalk,
    attempted_roots: usize,
    judge: &WalkJudge<'_>,
    stream: &mut ResultStream<'_>,
) -> WalkResult {
    let (tx, rx) = sync_channel(WALK_QUEUE_DEPTH);
    // Taken BEFORE the handle moves: progress has to be readable from the thread
    // that reports it, and the handle is about to go to one that spends its life
    // blocked on a batch.
    let dirs_scanned = walk.dirs_scanned_counter();
    let current_dir = walk.current_dir_slot();
    // The walk handle can't leave this thread once it's blocked on a batch, and
    // the run's thread has to wake up on a timer (to flush, and to notice a
    // cancel). So the handle goes to a thread whose only job is blocking on it.
    let forwarder = std::thread::Builder::new()
        .name("search-walk".into())
        .spawn(move || forward(walk, &tx));
    if let Err(e) = &forwarder {
        log::warn!("Live search: couldn't spawn the walk reader: {e}");
        return WalkResult {
            ending: WalkEnding::Interrupted,
            abandoned_ground: false,
            failed_roots: Vec::new(),
        };
    }

    // What this walk is actually doing, said ONCE up front rather than waited for:
    // a run learns it has moved on from the arena at the moment it does, and the
    // stream's own record of the phase is right from the first turn.
    let phase = walk_phase(attempted_roots);
    stream.announce(phase);

    pump(
        &rx,
        attempted_roots,
        phase,
        judge,
        stream,
        &WalkPulse {
            dirs_scanned,
            current_dir,
        },
    )
}

/// What a walk is doing, from the ground it actually took.
///
/// A walk that took NONE isn't walking. Every root it asked for was already
/// another walk's, so the run is queued behind that walk, reading what it
/// writes — and "0 folders scanned" under "looking through folders that aren't
/// indexed yet" is the shape of that lie. Pure, because which sentence a person
/// reads for minutes hangs on it.
fn walk_phase(attempted_roots: usize) -> SearchPhase {
    if attempted_roots == 0 {
        SearchPhase::WaitingForAnotherWalk
    } else {
        SearchPhase::Walking
    }
}

/// The two live readings the run takes off its walk between batches.
///
/// `Default` is a pulse attached to nothing, which is what a test driving the
/// pump over a hand-fed channel wants: no walk, so no progress to mirror.
#[derive(Default)]
struct WalkPulse {
    dirs_scanned: Arc<AtomicU64>,
    current_dir: Arc<Mutex<Option<String>>>,
}

impl WalkPulse {
    /// Copy the walk's own progress into the stream. Called every turn of the
    /// pump, so "still working" survives a walk that is emitting nothing.
    fn report_into(&self, stream: &mut ResultStream<'_>) {
        let current = self.current_dir.lock_ignore_poison().clone();
        stream.set_walk_progress(self.dirs_scanned.load(Ordering::Relaxed), current);
    }
}

/// Block on the walk, forwarding what it emits, and end with its verdict.
///
/// Dropping the receiver (the run gave up) ends this thread, and the walk itself
/// carries on filling the index: `finish` is still called, so the frontier claims
/// it holds are released when it genuinely stops.
fn forward(walk: CoverWalk, tx: &SyncSender<WalkMsg>) {
    while let Some(batch) = walk.next_batch() {
        if tx.send(WalkMsg::Batch(batch)).is_err() {
            break;
        }
    }
    let _ = tx.send(WalkMsg::Ended(walk.finish()));
}

/// The run's own loop: take batches, flush on the interval, stop when asked.
///
/// `phase` is what this walk is doing, decided by its caller from the ground it
/// took, so every event the loop emits says the same true thing.
fn pump(
    rx: &Receiver<WalkMsg>,
    attempted_roots: usize,
    phase: SearchPhase,
    judge: &WalkJudge<'_>,
    stream: &mut ResultStream<'_>,
    pulse: &WalkPulse,
) -> WalkResult {
    let mut outcome = None;
    loop {
        if stream.run().is_cancelled() {
            break;
        }
        pulse.report_into(stream);
        match rx.recv_timeout(BATCH_INTERVAL) {
            Ok(WalkMsg::Batch(batch)) => {
                // Rows landed in the volume's index, so the arena behind this
                // search is now behind it (Decision 12). Marked even for a
                // superseded run, whose walk is still writing.
                volumes::mark_walked_behind(&stream.run().volume_id);
                judge.consume(batch, stream);
                stream.flush_if_due(phase);
            }
            Ok(WalkMsg::Ended(ended)) => {
                outcome = Some(ended);
                break;
            }
            // Nothing arrived, so nothing new to say — but a row found 99 ms ago
            // has waited long enough, and a cancel is checked at the top.
            Err(RecvTimeoutError::Timeout) => stream.flush_if_due(phase),
            // The reader thread went away without a verdict.
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
    // One last reading, so the terminal event carries the directories the walk
    // got through after the pump's final tick rather than stopping short of them.
    pulse.report_into(stream);
    WalkResult {
        ending: ending_of(outcome.as_ref(), attempted_roots, stream.run().is_cancelled()),
        // A walk nobody heard from can't report what it abandoned; the ending
        // already says the list is a lower bound in that case.
        abandoned_ground: outcome.as_ref().is_some_and(|outcome| outcome.abandoned_ground),
        failed_roots: outcome.map(|outcome| outcome.failed_roots).unwrap_or_default(),
    }
}

/// Which terminal state a walk reached.
///
/// The order matters. A cancelled walk reports `cancelled` whatever the reason,
/// and the reason we know it was OURS is that we asked: a walk that stopped
/// without being asked stopped because its ground went away (an ejected drive, a
/// share that dropped), which is a different sentence and a different state.
///
/// A root that FAILED was still answered for: the volume was there and said no
/// about that one folder, which `failed_roots` reports in its own words. Counted
/// as unreached, it read as "the drive went away" over a NAS that was fine.
fn ending_of(outcome: Option<&CoverOutcome>, attempted_roots: usize, cancelled_by_us: bool) -> WalkEnding {
    if cancelled_by_us {
        return WalkEnding::Cancelled;
    }
    match outcome {
        // No verdict at all: the reader died, or we stopped listening. Either way
        // nothing may claim the frontier was covered.
        None => WalkEnding::Interrupted,
        Some(outcome) if outcome.cancelled || outcome.roots_covered + outcome.failed_roots.len() < attempted_roots => {
            WalkEnding::Interrupted
        }
        Some(_) => WalkEnding::Completed,
    }
}

#[cfg(test)]
mod tests;
