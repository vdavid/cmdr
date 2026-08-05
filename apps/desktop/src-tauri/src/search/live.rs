//! A search that keeps going: the runs in flight, and the walk feeding one.
//!
//! `execute.rs` orchestrates a live search (coverage → the index half → the
//! walk); this module owns the parts that only exist because the answer arrives
//! over time:
//!
//! - **The run registry.** A dialog supersedes its own query as the user refines
//!   it, and every run carries the token that stops its walk, so a closing dialog
//!   or a quitting app can stop every one of them from wherever it notices.
//! - **[`ResultStream`]**, which turns a trickle of matches into events at a rate
//!   a UI can absorb: at most [`BATCH_ROWS`] rows per event, at most
//!   [`BATCH_INTERVAL`] between events.
//! - **[`drive_walk`]**, the pump between the walk's thread and the run's.
//!
//! ## Superseding is not cancelling
//!
//! Refining a query starts a new run and marks the old one superseded: its
//! batches stop being emitted, and its WALK keeps running to completion filling
//! the index (Decision 11). Its driver keeps draining, for two reasons that both
//! matter — the bounded channel would otherwise stall the walk, and the arena
//! mark ([`volumes::mark_walked_behind`]) has to keep pace with the rows the walk
//! is still writing, or the next query prunes them as covered and shows FEWER
//! results than the one before it (Decision 12).

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, SyncSender, sync_channel};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

use tokio_util::sync::CancellationToken;

use cmdr_index::{CoverOutcome, CoverWalk, CoveredEntry};

use crate::ignore_poison::IgnorePoison;

use super::engine::{derive_icon_id, home_relative_parent};
use super::excludes::ExcludeRules;
use super::matcher::{CompiledQuery, covered_name};
use super::ranking::hash_path;
use super::types::{SearchQuery, SearchResultEntry};
use super::volumes;

pub(crate) mod events;

pub(crate) use events::{
    SearchCancelledEvent, SearchCompleteEvent, SearchErrorEvent, SearchEventSink, SearchPhase, SearchProgressEvent,
    SearchRunCoverage, SearchRunError, TauriSearchEventSink, WalkEnding,
};

/// The most rows one event carries.
const BATCH_ROWS: usize = 100;

/// The longest a found row waits for company before it's sent anyway. Also the
/// longest the run takes to notice it was cancelled.
const BATCH_INTERVAL: Duration = Duration::from_millis(100);

/// How many walk messages may queue between the walk's thread and the run's.
/// Small: each carries a whole batch, and the walk's own channel is already
/// bounded behind it.
const WALK_QUEUE_DEPTH: usize = 4;

// ── The runs in flight ───────────────────────────────────────────────

/// One live search run, from the query that started it to its terminal event.
pub(crate) struct LiveRun {
    /// What every event this run emits is stamped with.
    pub(crate) run_id: String,
    /// The one volume it covers.
    pub(crate) volume_id: String,
    /// Stops the walk AND the run. It's the token handed to `Index::cover`, so
    /// cancelling reaches the walk wherever it is rather than at the next batch
    /// boundary.
    cancel: CancellationToken,
    /// A newer run started. This one stops emitting; its walk carries on.
    superseded: AtomicBool,
}

impl LiveRun {
    /// Whether results from this run are still wanted. A cancelled run stops
    /// emitting rows too — its terminal event is what it has left to say.
    pub(crate) fn wants_results(&self) -> bool {
        !self.superseded.load(Ordering::Relaxed) && !self.cancel.is_cancelled()
    }

    /// Whether this run is allowed to emit anything at all. A superseded run
    /// isn't: the frontend has moved on, and a terminal event naming a run it
    /// dropped would be noise at best.
    pub(crate) fn wants_events(&self) -> bool {
        !self.superseded.load(Ordering::Relaxed)
    }

    pub(crate) fn is_cancelled(&self) -> bool {
        self.cancel.is_cancelled()
    }

    /// The token to hand `Index::cover`.
    pub(crate) fn cancel_token(&self) -> CancellationToken {
        self.cancel.clone()
    }
}

/// Every run that hasn't reached its terminal state, keyed by run id.
static RUNS: LazyLock<Mutex<HashMap<String, Arc<LiveRun>>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// Register a run and supersede every other one.
///
/// The dialog asks one question at a time, so an earlier run's results are by
/// definition for a query the user has moved on from. ❌ Superseding does not
/// cancel: their walks keep filling the index, which is what makes the refined
/// query cheaper than the first one.
pub(crate) fn register(run_id: &str, volume_id: &str) -> Arc<LiveRun> {
    let run = Arc::new(LiveRun {
        run_id: run_id.to_string(),
        volume_id: volume_id.to_string(),
        cancel: CancellationToken::new(),
        superseded: AtomicBool::new(false),
    });
    let mut runs = RUNS.lock_ignore_poison();
    for other in runs.values() {
        other.superseded.store(true, Ordering::Relaxed);
    }
    runs.insert(run_id.to_string(), Arc::clone(&run));
    run
}

/// Forget a run that has reached its terminal state.
pub(crate) fn deregister(run_id: &str) {
    RUNS.lock_ignore_poison().remove(run_id);
}

/// Stop one run and its walk. Returns whether there was one to stop.
pub(crate) fn cancel_live_run(run_id: &str) -> bool {
    let run = RUNS.lock_ignore_poison().get(run_id).cloned();
    match run {
        Some(run) => {
            run.cancel.cancel();
            true
        }
        None => false,
    }
}

/// Serializes the tests that register a run. Registering supersedes every OTHER
/// run in the process, so two tests running at once would silence each other.
#[cfg(test)]
pub(crate) fn test_registry_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock_ignore_poison()
}

/// Stop every run and every walk behind them.
///
/// The dialog closing calls this, and so does the app quitting: a walk outlives
/// its dialog only through "Open in pane", and nothing may leave a half-walked
/// scope looking complete. It can't — a directory is marked listed only once its
/// rows are written — but a walk still reading a disk nobody is waiting on is
/// exactly the resource waste the app promises not to be.
pub(crate) fn cancel_all_live_runs() {
    for run in RUNS.lock_ignore_poison().values() {
        run.cancel.cancel();
    }
}

// ── Results, batched for a UI ────────────────────────────────────────

/// Collects a run's matches and hands them out at a rate a UI can absorb.
///
/// One place counts, one place caps, one place emits — so "N so far" can't drift
/// from the rows on screen, and the cap can't quietly stop the walk.
pub(crate) struct ResultStream<'a> {
    run: &'a LiveRun,
    sink: &'a dyn SearchEventSink,
    /// The most rows this run will emit. The count keeps rising past it.
    limit: usize,
    /// A count-only run emits no rows at all, just totals.
    count_only: bool,
    pending: Vec<SearchResultEntry>,
    emitted: usize,
    match_count: u32,
    /// Path hashes of the rows already EMITTED, so a file the index handed back
    /// and the walk rediscovers is shown once. Bounded by [`limit`](Self::limit)
    /// rather than by the walk, which is what makes it affordable on a walk that
    /// matches a million entries. It insures a race (a file indexed between the
    /// frontier query and the walk reaching it); it is not the mechanism that
    /// keeps the two halves apart — the tree partition is.
    seen: HashSet<u64>,
    dirs_found: u64,
    current_path: Option<String>,
    last_emit: Instant,
}

impl<'a> ResultStream<'a> {
    pub(crate) fn new(run: &'a LiveRun, sink: &'a dyn SearchEventSink, query: &SearchQuery) -> Self {
        let limit = query.limit.min(1000) as usize;
        Self {
            run,
            sink,
            limit,
            count_only: query.count_only,
            pending: Vec::with_capacity(BATCH_ROWS.min(limit)),
            emitted: 0,
            match_count: 0,
            seen: HashSet::with_capacity(limit.min(BATCH_ROWS)),
            dirs_found: 0,
            current_path: None,
            last_emit: Instant::now(),
        }
    }

    /// Whether anything downstream still wants rows.
    pub(crate) fn wants_results(&self) -> bool {
        self.run.wants_results()
    }

    /// Whether the cap has been reached. The walk carries on regardless:
    /// convergence is the payoff, and a walk stopped at the cap would freeze
    /// "N so far" at a number that never becomes true.
    pub(crate) fn capped(&self) -> bool {
        self.emitted >= self.limit
    }

    /// Say which phase the run is in, without any results to show for it yet.
    pub(crate) fn announce(&mut self, phase: SearchPhase) {
        self.emit(phase, Vec::new());
    }

    /// Take the covered half: the engine's ranked rows and its exact total.
    pub(crate) fn add_indexed(&mut self, entries: Vec<SearchResultEntry>, total: u32) {
        self.match_count = total;
        for entry in entries {
            self.seen.insert(hash_path(&entry.path));
            if self.emitted < self.limit {
                self.emitted += 1;
                self.pending.push(entry);
            }
            if self.pending.len() >= BATCH_ROWS {
                self.flush(SearchPhase::ReadingIndex);
            }
        }
        self.flush(SearchPhase::ReadingIndex);
    }

    /// Take one match the walk found. Counted always; shown while there's room.
    pub(crate) fn add_walked(&mut self, entry: SearchResultEntry) {
        let path = hash_path(&entry.path);
        if self.seen.contains(&path) {
            // The index already answered for this one. Rare by construction, and
            // the count must not move either: it's the same file.
            return;
        }
        self.match_count = self.match_count.saturating_add(1);
        if self.count_only || self.emitted >= self.limit {
            // Nothing more to remember: the set exists to keep a ROW from
            // appearing twice, and no further rows are going out. What it costs is
            // that a duplicate arriving past the cap is counted twice — the same
            // bounded inaccuracy a count-only run has, and the price of a set that
            // can't grow with the walk.
            return;
        }
        self.seen.insert(path);
        self.emitted += 1;
        self.pending.push(entry);
        if self.pending.len() >= BATCH_ROWS {
            self.flush(SearchPhase::Walking);
        }
    }

    /// Record where the walk has got to.
    pub(crate) fn note_walk_progress(&mut self, dirs_found: u64, current_path: Option<String>) {
        self.dirs_found += dirs_found;
        if current_path.is_some() {
            self.current_path = current_path;
        }
    }

    /// Emit whatever is pending if the interval has elapsed, or if a full batch
    /// is waiting. Called on every walk batch and on every idle tick, so a run
    /// that finds nothing still reports progress.
    pub(crate) fn flush_if_due(&mut self, phase: SearchPhase) {
        if self.pending.len() >= BATCH_ROWS || self.last_emit.elapsed() >= BATCH_INTERVAL {
            self.flush(phase);
        }
    }

    /// Emit what's pending, and the progress that came with it.
    pub(crate) fn flush(&mut self, phase: SearchPhase) {
        let entries = std::mem::take(&mut self.pending);
        self.emit(phase, entries);
    }

    fn emit(&mut self, phase: SearchPhase, entries: Vec<SearchResultEntry>) {
        self.last_emit = Instant::now();
        if !self.run.wants_events() {
            return;
        }
        self.sink.emit_progress(SearchProgressEvent {
            run_id: self.run.run_id.clone(),
            phase,
            entries,
            match_count: self.match_count,
            dirs_found: self.dirs_found,
            current_path: self.current_path.clone(),
            capped: self.capped(),
        });
    }

    /// The run's last word. A superseded run says nothing: the frontend dropped
    /// its id when it asked the next question.
    ///
    /// A cancelled run ends as cancelled whatever the walk was doing when it
    /// stopped — including a run stopped before its walk ever started. Deciding
    /// that HERE is what keeps every terminal path from having to remember it.
    pub(crate) fn finish(mut self, mut coverage: SearchRunCoverage) {
        if self.run.is_cancelled() {
            coverage.walk = WalkEnding::Cancelled;
        }
        self.flush(match coverage.walk {
            WalkEnding::NothingToWalk => SearchPhase::ReadingIndex,
            _ => SearchPhase::Walking,
        });
        if !self.run.wants_events() {
            return;
        }
        match coverage.walk {
            WalkEnding::Cancelled => self.sink.emit_cancelled(SearchCancelledEvent {
                run_id: self.run.run_id.clone(),
                match_count: self.match_count,
                coverage,
            }),
            _ => self.sink.emit_complete(SearchCompleteEvent {
                run_id: self.run.run_id.clone(),
                match_count: self.match_count,
                coverage,
            }),
        }
    }

    /// The run couldn't run at all.
    pub(crate) fn fail(self, error: SearchRunError, message: String) {
        if !self.run.wants_events() {
            return;
        }
        self.sink.emit_error(SearchErrorEvent {
            run_id: self.run.run_id.clone(),
            error,
            message,
        });
    }
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
    fn consume(&self, batch: Vec<CoveredEntry>, stream: &mut ResultStream<'_>) {
        let dirs = batch.iter().filter(|entry| entry.is_directory).count() as u64;
        // Where the walk is, as of this batch. The last entry is the freshest
        // thing it found; its parent is the directory it was reading.
        let current = batch
            .last()
            .and_then(|entry| entry.path.parent())
            .map(|parent| parent.to_string_lossy().into_owned());
        stream.note_walk_progress(dirs, current);

        if !stream.wants_results() {
            return;
        }
        for entry in batch {
            if !self.compiled.matches_covered(&entry) {
                continue;
            }
            let path = entry.path.to_string_lossy();
            if self.excludes.excludes_walked(&path, self.volume_root) {
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
    let name = covered_name(&entry.path).into_owned();
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
) -> WalkEnding {
    let (tx, rx) = sync_channel(WALK_QUEUE_DEPTH);
    // The walk handle can't leave this thread once it's blocked on a batch, and
    // the run's thread has to wake up on a timer (to flush, and to notice a
    // cancel). So the handle goes to a thread whose only job is blocking on it.
    let forwarder = std::thread::Builder::new()
        .name("search-walk".into())
        .spawn(move || forward(walk, &tx));
    if let Err(e) = &forwarder {
        log::warn!("Live search: couldn't spawn the walk reader: {e}");
        return WalkEnding::Interrupted;
    }

    pump(&rx, attempted_roots, judge, stream)
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
fn pump(
    rx: &Receiver<WalkMsg>,
    attempted_roots: usize,
    judge: &WalkJudge<'_>,
    stream: &mut ResultStream<'_>,
) -> WalkEnding {
    let mut outcome = None;
    loop {
        if stream.run.is_cancelled() {
            break;
        }
        match rx.recv_timeout(BATCH_INTERVAL) {
            Ok(WalkMsg::Batch(batch)) => {
                // Rows landed in the volume's index, so the arena behind this
                // search is now behind it (Decision 12). Marked even for a
                // superseded run, whose walk is still writing.
                volumes::mark_walked_behind(&stream.run.volume_id);
                judge.consume(batch, stream);
                stream.flush_if_due(SearchPhase::Walking);
            }
            Ok(WalkMsg::Ended(ended)) => {
                outcome = Some(ended);
                break;
            }
            // Nothing arrived, so nothing new to say — but a row found 99 ms ago
            // has waited long enough, and a cancel is checked at the top.
            Err(RecvTimeoutError::Timeout) => stream.flush_if_due(SearchPhase::Walking),
            // The reader thread went away without a verdict.
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
    ending_of(outcome.as_ref(), attempted_roots, stream.run.is_cancelled())
}

/// Which terminal state a walk reached.
///
/// The order matters. A cancelled walk reports `cancelled` whatever the reason,
/// and the reason we know it was OURS is that we asked: a walk that stopped
/// without being asked stopped because its ground went away (an ejected drive, a
/// share that dropped), which is a different sentence and a different state.
fn ending_of(outcome: Option<&CoverOutcome>, attempted_roots: usize, cancelled_by_us: bool) -> WalkEnding {
    if cancelled_by_us {
        return WalkEnding::Cancelled;
    }
    match outcome {
        // No verdict at all: the reader died, or we stopped listening. Either way
        // nothing may claim the frontier was covered.
        None => WalkEnding::Interrupted,
        Some(outcome) if outcome.cancelled || outcome.roots_covered < attempted_roots => WalkEnding::Interrupted,
        Some(_) => WalkEnding::Completed,
    }
}

#[cfg(test)]
mod tests;
