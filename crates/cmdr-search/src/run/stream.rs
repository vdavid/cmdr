//! A run's matches, batched for a UI, and the facts they go out as.

use std::collections::HashSet;
use std::time::Duration;

use cmdr_fs::path_hash::hash_path;
use cmdr_fs::rt::{self, Instant};

use super::registry::LiveRun;
use super::report::CoverageFacts;
use super::state::{SearchPhase, SearchRunCoverage, SearchRunError, WalkEnding};
use crate::types::{SearchQuery, SearchResultEntry};

/// The most rows one progress fact carries.
pub const BATCH_ROWS: usize = 100;

/// The longest a found row waits for company before it's sent anyway. Also the
/// longest a host's pump should take to notice its run was cancelled.
pub const BATCH_INTERVAL: Duration = Duration::from_millis(100);

/// A batch of results, plus where the run has got to. The host's
/// `search-progress` payload, field for field.
#[derive(Debug, Clone)]
pub struct RunProgress {
    pub run_id: String,
    pub phase: SearchPhase,
    /// The ONE volume every row here lives on.
    pub target_volume_id: String,
    /// Rows found since the last fact, in arrival order. Empty on a progress-only
    /// fact.
    pub entries: Vec<SearchResultEntry>,
    /// Matches so far, counting the ones past the result cap.
    pub match_count: u32,
    /// Directories the walk has turned up so far.
    pub dirs_found: u64,
    /// Where the walk was as of this batch. Indicative, not a cursor.
    pub current_path: Option<String>,
    /// Whether the cap is reached, so no further rows will arrive.
    pub capped: bool,
}

/// A run's last word when it ended on its own terms or was stopped.
#[derive(Debug, Clone)]
pub struct RunEnded {
    pub run_id: String,
    /// The exact total for everything this run covered.
    pub match_count: u32,
    pub coverage: SearchRunCoverage,
}

/// A run that couldn't run at all.
#[derive(Debug, Clone)]
pub struct RunFailed {
    pub run_id: String,
    /// Typed, word-free classification. Branch on this.
    pub error: SearchRunError,
    /// The sentence to show.
    pub message: String,
}

/// Where a run's facts go. The host maps each onto its wire: desktop emits Tauri
/// events and folds them for an agent's one-shot reply.
pub trait RunSink: Send + Sync {
    fn progress(&self, fact: RunProgress);
    /// The run finished on its own terms, which isn't "the answer is complete":
    /// [`SearchRunCoverage::walk`] says which.
    fn complete(&self, fact: RunEnded);
    /// Somebody stopped the run.
    fn cancelled(&self, fact: RunEnded);
    fn failed(&self, fact: RunFailed);
}

/// Collects a run's matches and hands them out at a rate a UI can absorb.
///
/// One place counts, one place caps, one place emits, so "N so far" can't drift
/// from the rows on screen, and the cap can't quietly stop the walk.
pub struct ResultStream<'a> {
    run: &'a LiveRun,
    sink: &'a dyn RunSink,
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
    /// keeps the two halves apart: the tree partition is.
    seen: HashSet<u64>,
    dirs_found: u64,
    /// Matches an exclusion rule dropped, across BOTH halves: the arena scan's
    /// count arrives with the covered half, the walk's is counted here as batches
    /// are judged. Stamped onto the coverage in [`finish`](Self::finish), the one
    /// place that has seen both.
    hidden_by_excludes: u32,
    current_path: Option<String>,
    last_emit: Instant,
    /// The phase of the last fact that went out, so the terminal fact can say the
    /// phase the run was actually IN rather than guess one from how the walk ended.
    /// A run that never walked must not sign off as "walking".
    last_phase: SearchPhase,
}

impl<'a> ResultStream<'a> {
    pub fn new(run: &'a LiveRun, sink: &'a dyn RunSink, query: &SearchQuery) -> Self {
        let limit = query.effective_limit();
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
            last_emit: rt::now(),
            // Every run starts here (the host announces it first), so a run that
            // ends before saying anything else ends honestly.
            last_phase: SearchPhase::ResolvingCoverage,
        }
    }

    /// The run this stream reports for.
    pub fn run(&self) -> &LiveRun {
        self.run
    }

    /// Whether anything downstream still wants rows.
    pub fn wants_results(&self) -> bool {
        self.run.wants_results()
    }

    /// Whether the cap has been reached. The walk carries on regardless:
    /// convergence is the payoff, and a walk stopped at the cap would freeze
    /// "N so far" at a number that never becomes true.
    pub fn capped(&self) -> bool {
        self.emitted >= self.limit
    }

    /// Matches so far, the walk's past the cap included.
    #[cfg(any(test, feature = "testing"))]
    pub fn match_count(&self) -> u32 {
        self.match_count
    }

    /// Directories the walk has turned up, as last reported.
    #[cfg(any(test, feature = "testing"))]
    pub fn dirs_found(&self) -> u64 {
        self.dirs_found
    }

    /// Say which phase the run is in, without any results to show for it yet.
    pub fn announce(&mut self, phase: SearchPhase) {
        self.emit(phase, Vec::new());
    }

    /// Take the covered half: the engine's ranked rows, its exact total, and how
    /// many matches its exclusion rules dropped.
    pub fn add_indexed(&mut self, entries: Vec<SearchResultEntry>, total: u32, hidden_by_excludes: u32) {
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
    pub fn note_excluded(&mut self) {
        self.hidden_by_excludes = self.hidden_by_excludes.saturating_add(1);
    }

    /// Take one match the walk found. Counted always; shown while there's room.
    pub fn add_walked(&mut self, entry: SearchResultEntry) {
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
            // that a duplicate arriving past the cap is counted twice: the same
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
    /// than derived from the batches it emits. A batch fills at 2,000 entries, so
    /// batch-derived progress sits at "0 folders scanned" and no path for as long
    /// as a batch takes to fill, which on a slow tree, or a directory that hangs,
    /// reads as frozen while the walk is very much alive.
    pub fn set_walk_progress(&mut self, dirs_scanned: u64, current_path: Option<String>) {
        self.dirs_found = dirs_scanned;
        if current_path.is_some() {
            self.current_path = current_path;
        }
    }

    /// Emit if it's been [`BATCH_INTERVAL`] since the last fact. Called after
    /// every walk batch and on every idle tick, so a run that is finding nothing
    /// still reports where the walk has got to.
    ///
    /// ❌ No row check here: a full batch has already gone out from inside
    /// `add_indexed` / `add_walked`, which is where the count crosses
    /// `BATCH_ROWS`. What's left for the clock is the remainder.
    pub fn flush_if_due(&mut self, phase: SearchPhase) {
        if rt::since(self.last_emit) >= BATCH_INTERVAL {
            self.flush(phase);
        }
    }

    /// Emit what's pending, and the progress that came with it.
    pub fn flush(&mut self, phase: SearchPhase) {
        let entries = std::mem::take(&mut self.pending);
        self.emit(phase, entries);
    }

    fn emit(&mut self, phase: SearchPhase, entries: Vec<SearchResultEntry>) {
        self.last_emit = rt::now();
        // One line per CHANGE, never per fact: a run emits ten a second, and what
        // a reader of a bundle needs is the sentence the dialog was showing and
        // when it changed. This is the trail for "the app said it was walking and
        // nothing was walking".
        if phase != self.last_phase {
            log::debug!("Live search: run '{}' is now in phase {phase:?}", self.run.run_id);
        }
        self.last_phase = phase;
        if !self.run.wants_events() {
            return;
        }
        self.sink.progress(RunProgress {
            run_id: self.run.run_id.clone(),
            phase,
            target_volume_id: self.run.volume_id.clone(),
            entries,
            match_count: self.match_count,
            dirs_found: self.dirs_found,
            current_path: self.current_path.clone(),
            capped: self.capped(),
        });
    }

    /// The run's last word, built from `facts` by the one report every ending
    /// shares. A superseded run says nothing: the frontend dropped its id when it
    /// asked the next question.
    ///
    /// A cancelled run ends as cancelled whatever the walk was doing when it
    /// stopped, including a run stopped before its walk ever started. Deciding
    /// that HERE is what keeps every terminal path from having to remember it.
    pub fn finish(mut self, facts: CoverageFacts) {
        let mut coverage = facts.report(self.capped(), self.hidden_by_excludes);
        if self.run.is_cancelled() {
            coverage.walk = WalkEnding::Cancelled;
        }
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
        let ended = RunEnded {
            run_id: self.run.run_id.clone(),
            match_count: self.match_count,
            coverage,
        };
        match ended.coverage.walk {
            WalkEnding::Cancelled => self.sink.cancelled(ended),
            _ => self.sink.complete(ended),
        }
    }

    /// The run couldn't run at all.
    pub fn fail(self, error: SearchRunError, message: String) {
        if !self.run.wants_events() {
            return;
        }
        self.sink.failed(RunFailed {
            run_id: self.run.run_id.clone(),
            error,
            message,
        });
    }
}
