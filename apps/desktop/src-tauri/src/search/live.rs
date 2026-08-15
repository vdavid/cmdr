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
//!
//! Superseding is scoped to the DIALOG, which is the one asker that retypes; see
//! [`RunOrigin`]. An MCP call gets the same run, the same walk, and the same
//! answer, folded into one reply by [`collect`] because its transport can't
//! carry a stream.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::AtomicU64;
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

pub(crate) mod collect;
pub(crate) mod events;

pub(crate) use collect::{AnswerEnding, CollectingSink, LiveAnswer};
pub(crate) use events::{
    CoverageKind, SearchCancelledEvent, SearchCompleteEvent, SearchErrorEvent, SearchEventSink, SearchPhase,
    SearchProgressEvent, SearchRunCoverage, SearchRunError, TauriSearchEventSink, WalkEnding,
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

/// Who asked, which is what decides a new run's effect on the ones already in
/// flight.
///
/// ❌ Not a policy switch: both origins take the same path, walk the same
/// ground, and get the same answer (`docs/specs/unindexed-search-plan.md`
/// Decision 10). What differs is only who else is in the room.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RunOrigin {
    /// The search dialog. One dialog asks one question at a time, so a new run
    /// supersedes its previous one, and closing the dialog stops what it left
    /// behind.
    Dialog,
    /// An MCP tool call. Its own asker with its own caller waiting, so it
    /// neither supersedes nor is superseded, and a dialog closing is none of its
    /// business. Only the app quitting stops it.
    Agent,
}

/// One live search run, from the query that started it to its terminal event.
pub(crate) struct LiveRun {
    /// What every event this run emits is stamped with.
    pub(crate) run_id: String,
    /// The one volume it covers.
    pub(crate) volume_id: String,
    /// Who asked for it.
    origin: RunOrigin,
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

/// Register a run, superseding the ones it speaks over.
///
/// The dialog asks one question at a time, so an earlier DIALOG run's results
/// are by definition for a query the user has moved on from. ❌ Superseding does
/// not cancel: their walks keep filling the index, which is what makes the
/// refined query cheaper than the first one.
///
/// ❌ It reaches no [`RunOrigin::Agent`] run, in either direction: an MCP call
/// has its own caller waiting on its own answer, and a person typing has no
/// business emptying it (nor the other way round).
pub(crate) fn register(run_id: &str, volume_id: &str, origin: RunOrigin) -> Arc<LiveRun> {
    let run = Arc::new(LiveRun {
        run_id: run_id.to_string(),
        volume_id: volume_id.to_string(),
        origin,
        cancel: CancellationToken::new(),
        superseded: AtomicBool::new(false),
    });
    let mut runs = RUNS.lock_ignore_poison();
    if origin == RunOrigin::Dialog {
        for other in runs.values().filter(|other| other.origin == RunOrigin::Dialog) {
            other.superseded.store(true, Ordering::Relaxed);
        }
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
/// The app quitting calls this: nothing may leave a half-walked scope looking
/// complete. It can't — a directory is marked listed only once its rows are
/// written — but a walk still reading a disk nobody is waiting on is exactly the
/// resource waste the app promises not to be.
pub(crate) fn cancel_all_live_runs() {
    for run in RUNS.lock_ignore_poison().values() {
        run.cancel.cancel();
    }
}

/// Stop every DIALOG run except the one named, if any.
///
/// The dialog closing calls this with the run it deliberately outlived: "Open in
/// pane" promotes the results into a pane and leaves the walk filling it (the
/// handoff, `src/lib/search/walk-handoff.svelte.ts`), so
/// that ONE run has a consumer even though the dialog doesn't. Every other run of
/// the dialog's is a query nobody is reading.
///
/// ❌ An agent's run is untouched: closing the dialog says nothing about an MCP
/// call still waiting for its answer.
pub(crate) fn cancel_dialog_runs_except(keep_run_id: Option<&str>) {
    for run in RUNS.lock_ignore_poison().values() {
        if run.origin != RunOrigin::Dialog || keep_run_id.is_some_and(|keep| keep == run.run_id) {
            continue;
        }
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
    /// Matches an exclusion rule dropped, across BOTH halves: the arena scan's
    /// count arrives with the covered half, the walk's is counted here as batches
    /// are judged. Stamped onto the coverage in [`finish`](Self::finish), the one
    /// place that has seen both.
    hidden_by_excludes: u32,
    current_path: Option<String>,
    last_emit: Instant,
    /// The phase of the last event that went out, so the terminal event can say
    /// the phase the run was actually IN rather than guess one from how the walk
    /// ended. A run that never walked must not sign off as "walking".
    last_phase: SearchPhase,
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
            hidden_by_excludes: 0,
            current_path: None,
            last_emit: Instant::now(),
            // Every run starts here (`run_live_blocking` announces it first), so a
            // run that ends before saying anything else ends honestly.
            last_phase: SearchPhase::ResolvingCoverage,
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

    /// Take the covered half: the engine's ranked rows, its exact total, and how
    /// many matches its exclusion rules dropped.
    pub(crate) fn add_indexed(&mut self, entries: Vec<SearchResultEntry>, total: u32, hidden_by_excludes: u32) {
        self.match_count = total;
        self.hidden_by_excludes = self.hidden_by_excludes.saturating_add(hidden_by_excludes);
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

    /// Note one walked match an exclusion rule dropped, so the walked half of a
    /// live run reports its hidden matches the way the arena half does.
    pub(crate) fn note_excluded(&mut self) {
        self.hidden_by_excludes = self.hidden_by_excludes.saturating_add(1);
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

    /// Record where the walk has got to, as the WALK reports it.
    ///
    /// Absolute values, not deltas, and read off the walk's own heartbeat rather
    /// than derived from the batches it emits. A batch fills at 2 000 entries, so
    /// batch-derived progress sits at "0 folders scanned" and no path for as long
    /// as a batch takes to fill — which on a slow tree, or a directory that hangs,
    /// reads as frozen while the walk is very much alive.
    pub(crate) fn set_walk_progress(&mut self, dirs_scanned: u64, current_path: Option<String>) {
        self.dirs_found = dirs_scanned;
        if current_path.is_some() {
            self.current_path = current_path;
        }
    }

    /// Emit if it's been [`BATCH_INTERVAL`] since the last event. Called after
    /// every walk batch and on every idle tick, so a run that is finding nothing
    /// still reports where the walk has got to.
    ///
    /// ❌ No row check here: a full batch has already gone out from inside
    /// `add_indexed` / `add_walked`, which is where the count crosses
    /// [`BATCH_ROWS`]. What's left for the clock is the remainder.
    pub(crate) fn flush_if_due(&mut self, phase: SearchPhase) {
        if self.last_emit.elapsed() >= BATCH_INTERVAL {
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
        self.last_phase = phase;
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
        coverage.hidden_by_excludes = self.hidden_by_excludes;
        // The phase the run was in when it stopped. ❌ Never "it didn't end as
        // `NothingToWalk`, so call it walking": a run the drive refused, or one
        // stopped before its walk began, never walked, and its last word saying
        // otherwise is what put "0 folders scanned" under a walking sentence.
        self.flush(match coverage.walk {
            WalkEnding::NothingToWalk => SearchPhase::ReadingIndex,
            _ => self.last_phase,
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
    ///
    /// ❌ It reports no progress: where the walk is comes from the walk's own
    /// heartbeat (see [`set_walk_progress`](ResultStream::set_walk_progress)), so
    /// a run that is receiving no batches still shows one.
    fn consume(&self, batch: Vec<CoveredEntry>, stream: &mut ResultStream<'_>) {
        if !stream.wants_results() {
            return;
        }
        for entry in batch {
            if !self.compiled.matches_covered(&entry) {
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
        if stream.run.is_cancelled() {
            break;
        }
        pulse.report_into(stream);
        match rx.recv_timeout(BATCH_INTERVAL) {
            Ok(WalkMsg::Batch(batch)) => {
                // Rows landed in the volume's index, so the arena behind this
                // search is now behind it (Decision 12). Marked even for a
                // superseded run, whose walk is still writing.
                volumes::mark_walked_behind(&stream.run.volume_id);
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
        ending: ending_of(outcome.as_ref(), attempted_roots, stream.run.is_cancelled()),
        // A walk nobody heard from can't report what it abandoned; the ending
        // already says the list is a lower bound in that case.
        abandoned_ground: outcome.as_ref().is_some_and(|outcome| outcome.abandoned_ground),
    }
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
