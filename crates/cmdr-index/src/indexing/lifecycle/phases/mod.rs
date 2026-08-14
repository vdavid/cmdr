//! Covering a volume in the order its owner cares about, one frontier root at a
//! time.
//!
//! There is no first full scan. The whole drive is the LAST phase of the same
//! mechanism the user's own folders go through: `coverage` names what a scope
//! still needs walked, `cover` walks it, and the stitch next door is what makes an
//! ancestor scope's frontier shrink as the phases below it finish. Every walk is
//! add-only, durable, and resumable, so quitting mid-way keeps everything it
//! bought — which is the property today's truncate-and-rebuild first scan can't
//! have.
//!
//! ## The shape of a run
//!
//! 1. Ask the host which folders matter to this user (`HostPolicy::priority_roots`,
//!    an ORDER and nothing else), then `$HOME`, then the volume root.
//! 2. Stitch down to each phase root, ask for its frontier, and walk those roots
//!    one at a time, checking the visit queue in between.
//! 3. After each drain, ask the database whether anything is complete. Completion
//!    is derived, never remembered: "the frontier under this root is empty".
//!
//! ## Three rules that are easy to get wrong
//!
//! - **One `cover()` call per frontier root**, joined before the next starts.
//!   Measured, the join costs nothing (41 s of real walking against a whole-volume
//!   walk's 38.1 s), and it is what gives the queue its check points. ❌ Don't hand
//!   one call a whole phase's frontier to save the per-root bookkeeping: the check
//!   inside `cover` is not a point the machine can consult a queue at.
//! - **The writer drains once per phase, not once per root.** A blocking flush at
//!   the end of every walk was 37.5 s of the walker standing still over ~1,500
//!   roots (`docs/notes/phased-vs-bulk-index-2026-08-14.md`). Two sequences still
//!   need a real flush and ❌ must not be batched away: the stitch's
//!   upsert-then-mark, and the completion sequence's stamp-before-collapse.
//! - **The reporter's lifetime is the MACHINE's, not a walk's.** It carries the
//!   progress event stream, mid-scan partial aggregation, and the only legal home
//!   for the `open_listings` poll; one per walk would die and restart 50–150 times
//!   a phase.
//!
//! Depth, and why each of those is what it is: `../DETAILS.md`.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use tokio_util::sync::CancellationToken;

use crate::indexing::IndexPathSpace;
use crate::indexing::events::{ActivityPhase, EventSink, IndexEvent, ScanRunKind, set_phase_for};
use crate::indexing::lifecycle::freshness::Freshness;
use crate::indexing::lifecycle::progress_reporter::ScanProgressReporter;
use crate::indexing::lifecycle::{cover, master};
use crate::indexing::read::coverage::{CoverageDimension, coverage_for_scope};
use crate::indexing::scanner::ScanProgress;
use crate::indexing::store::IndexStore;
use crate::indexing::writer::{AggSource, IndexWriter};

mod completion;
mod queue;
mod stitch;
mod visits;

pub(crate) use completion::HOME_COVERED_AT_KEY;
pub(crate) use visits::VisitLog;

#[cfg(test)]
mod tests;

use queue::{Phase, PhaseQueue, Rank};

/// How many times a phase re-asks for its frontier after draining.
///
/// One pass covers what the frontier named; a second picks up what the first
/// exposed (a directory that appeared under a root while it was being walked) and
/// anything a concurrent search walk was holding. Past that, whatever is left is
/// ground this session can't cover, and the next launch asks again — which is
/// honest, and terminates. ❌ This is a PASS budget, never a completion rule:
/// completion stays "the frontier is empty".
const MAX_PASSES_PER_PHASE: usize = 2;

/// The folder inside home that every other home root would otherwise wait behind.
///
/// `~/Library` is 27.7% of a real boot index and 72.8 s of an 88.4 s home
/// coverage, against under six seconds for everything else in home
/// (`docs/notes/phased-vs-bulk-index-2026-08-14.md`). It stays in scope — search
/// over it is occasionally what someone wants — but it goes last inside its phase,
/// and the early home signal doesn't wait for it. Linux has no single equivalent
/// pile, so it has none.
#[cfg(target_os = "macos")]
const DEFERRED_HOME_FOLDER: Option<&str> = Some("Library");
#[cfg(not(target_os = "macos"))]
const DEFERRED_HOME_FOLDER: Option<&str> = None;

/// What the rest of the lifecycle can ask of a machine that is running.
///
/// Held by the volume's `IndexManager`; every field is shared with the driver
/// thread, so a question asked through the manager is answered by what the driver
/// is doing right now.
pub(crate) struct PhaseHandle {
    /// A walk is reading the disk RIGHT NOW. What the per-navigation verifier is
    /// suppressed by, exactly as a full scan suppresses it.
    walking: Arc<AtomicBool>,
    /// The machine still has work: a phase queued, or one running.
    ///
    /// ⚠️ This, ❌ never [`walking`](Self::walking), is what a scan entry refuses
    /// against and what `get_status` reports as `scanning`. `walking` goes false
    /// between roots, and the stitch deliberately produces 50–150 of them per
    /// phase: a truncating rescan landing in one of those gaps would blank an index
    /// the machine is half way through building, and the search dialog's "building
    /// your index" state would flicker at root cadence.
    working: Arc<AtomicBool>,
    /// The counters the progress reporter emits and `get_status` reports.
    progress: Arc<ScanProgress>,
    /// Where the visit poll writes what the user opened.
    visits: Arc<VisitLog>,
    /// Stops the driver, and (through `done`) the reporter with it.
    cancel: CancellationToken,
    /// Ends the reporter's tick loop, the same way a scan's completion handler
    /// ends it.
    done: Arc<AtomicBool>,
}

impl PhaseHandle {
    /// Whether the machine still has work. See the field docs: this is the
    /// question every scan entry asks.
    pub(crate) fn has_work(&self) -> bool {
        self.working.load(Ordering::Relaxed)
    }

    /// Whether a walk is reading the disk right now.
    pub(crate) fn is_walking(&self) -> bool {
        self.walking.load(Ordering::Relaxed)
    }

    /// The live counters, for `get_status`.
    pub(crate) fn progress(&self) -> &Arc<ScanProgress> {
        &self.progress
    }

    /// Stop the machine: the running walk sees the token, the driver stops
    /// queueing, and the reporter's loop ends. Covered ground stays covered and
    /// watched.
    pub(crate) fn stop(&self) {
        self.cancel.cancel();
        self.working.store(false, Ordering::Relaxed);
        self.done.store(true, Ordering::Relaxed);
    }
}

/// Everything one run needs, resolved on the caller's thread.
pub(crate) struct MachineContext {
    pub volume_id: String,
    pub volume_root: PathBuf,
    pub space: IndexPathSpace,
    pub writer: IndexWriter,
    pub events: Arc<dyn EventSink>,
    pub freshness: Arc<std::sync::Mutex<Option<Freshness>>>,
    /// A child of the VOLUME's stop signal, so tearing the volume down stops the
    /// machine with everything else under it.
    pub cancel: CancellationToken,
}

/// Start covering a volume, and hand back what the manager holds on to.
///
/// The driver runs on a dedicated thread at `Utility` QoS rather than a runtime
/// worker: everything it does is blocking (walks, flushes, database reads), and
/// the whole point is to yield the machine to whoever the user is waiting on.
pub(crate) fn start(context: MachineContext) -> PhaseHandle {
    let handle = PhaseHandle {
        walking: Arc::new(AtomicBool::new(false)),
        // True from here, not from the first walk: the volume is already the
        // machine's, and a scan entry arriving before the thread starts must
        // refuse exactly as one arriving mid-phase does.
        working: Arc::new(AtomicBool::new(true)),
        progress: Arc::new(ScanProgress::new()),
        visits: Arc::new(VisitLog::new()),
        cancel: context.cancel.clone(),
        done: Arc::new(AtomicBool::new(false)),
    };

    let machine = Machine {
        started_at: Instant::now(),
        walking: Arc::clone(&handle.walking),
        working: Arc::clone(&handle.working),
        progress: Arc::clone(&handle.progress),
        visits: Arc::clone(&handle.visits),
        done: Arc::clone(&handle.done),
        cancel: context.cancel,
        volume_id: context.volume_id,
        volume_root: context.volume_root,
        space: context.space,
        writer: context.writer,
        events: context.events,
        freshness: context.freshness,
    };

    // Where the user is looking RIGHT NOW, before the first phase reads anything.
    // The reporter's first tick is half a second away, and the folder somebody has
    // open at the moment indexing starts is the single best guess there is.
    handle.visits.note(
        &crate::indexing::host::policy::current().open_listings(),
        &machine.volume_id,
    );

    // The machine's OWN 500 ms pump, for the machine's whole lifetime rather than
    // one walk's. Three things ride it and stop together without it: the
    // `index-scan-progress` event stream, mid-scan partial aggregation (which is
    // what makes sizes appear INSIDE a frontier root still being walked, and a
    // frontier root can be 1.58M entries), and the `open_listings` poll. `Sql`,
    // because a cover walk leaves the writer's accumulator maps empty.
    ScanProgressReporter::new(
        Arc::clone(&handle.progress),
        machine.writer.clone(),
        Arc::clone(&machine.events),
        machine.volume_id.clone(),
        AggSource::Sql,
    )
    .noting_visits(Arc::clone(&handle.visits))
    .spawn(Arc::clone(&handle.done));

    let spawned = std::thread::Builder::new().name("index-phases".into()).spawn(move || {
        cmdr_fs::thread_qos::set_current_thread_qos(cmdr_fs::thread_qos::QosClass::Utility);
        machine.run();
    });
    if let Err(e) = spawned {
        // A machine that can't spawn a thread has a bigger problem than this
        // volume's index; say so and report no work rather than pretending.
        log::warn!("Phases: couldn't spawn the driver thread: {e}");
        handle.working.store(false, Ordering::Relaxed);
        handle.done.store(true, Ordering::Relaxed);
    }
    handle
}

/// This machine's home directory. A test drives a synthetic one, because the home
/// phase and its early signal are the whole reason the machine has an order at all
/// and a real `$HOME` is never inside a temp tree.
fn home_dir() -> Option<PathBuf> {
    #[cfg(test)]
    if let Some(home) = tests::home_override() {
        return Some(home);
    }
    dirs::home_dir()
}

/// The driver, which owns its thread for the whole run.
struct Machine {
    started_at: Instant,
    walking: Arc<AtomicBool>,
    working: Arc<AtomicBool>,
    progress: Arc<ScanProgress>,
    visits: Arc<VisitLog>,
    done: Arc<AtomicBool>,
    cancel: CancellationToken,
    volume_id: String,
    volume_root: PathBuf,
    space: IndexPathSpace,
    writer: IndexWriter,
    events: Arc<dyn EventSink>,
    freshness: Arc<std::sync::Mutex<Option<Freshness>>>,
}

impl Machine {
    fn run(&self) {
        self.announce_the_start();
        let mut queue = self.initial_queue();
        while let Some(phase) = queue.take_next() {
            if !self.may_run() {
                break;
            }
            self.run_phase(&phase, &mut queue);
        }
        // ⚠️ Once more with nothing left to walk. A phase whose frontier is ALREADY
        // empty walks nothing and drains nothing, so a run that only had to confirm
        // what a previous session covered would never reach a stock-take — and a
        // volume killed between its last walk and its stamp would stay unmarked
        // forever, re-running the machine on every launch to discover the same
        // thing.
        self.take_stock();
        self.finish();
    }

    /// The phases, in the order they run: the folders this user cares about
    /// (whatever the host says, and the host owns the ranking), then `$HOME`, then
    /// the whole volume.
    ///
    /// `$HOME` is queued explicitly rather than trusted to be the last priority
    /// root: the host caps its list, so a user with two dozen favorites can push it
    /// off the end, and the phase between the priority roots and `/` is what makes
    /// the early media signal possible at all. The queue drops it as a duplicate
    /// when the host already named it.
    fn initial_queue(&self) -> PhaseQueue {
        let mut queue = PhaseQueue::new();
        for root in crate::indexing::host::policy::current().priority_roots(&self.volume_id) {
            queue.push(Rank::PriorityRoot, root);
        }
        if let Some(home) = self.home_on_this_volume() {
            queue.push(Rank::Home, home);
        }
        queue.push(Rank::WholeVolume, self.volume_root.clone());
        queue
    }

    /// This machine's home directory, when it is on the volume being covered.
    /// `None` for an external drive, and for a machine with no home at all.
    fn home_on_this_volume(&self) -> Option<PathBuf> {
        let home = home_dir()?;
        home.starts_with(&self.volume_root).then_some(home)
    }

    /// One phase: stitch down to its root, then walk what its frontier still
    /// names, one root at a time.
    fn run_phase(&self, phase: &Phase, queue: &mut PhaseQueue) {
        set_phase_for(
            self.events.as_ref(),
            &self.volume_id,
            ActivityPhase::Scanning,
            &format!("covering {}", phase.path.display()),
        );
        stitch::down_to(&self.space, &self.writer, &phase.path);

        for pass in 0..MAX_PASSES_PER_PHASE {
            let frontier = self.frontier_under(&phase.path);
            if frontier.is_empty() {
                break;
            }
            let (first, deferred) = self.order(frontier);
            let mut covered = self.walk_all(&first, phase.rank, queue);
            // The drain and the check that follows it are why the deferred roots
            // are split out at all: everything else in this phase is covered at
            // this moment, so a phase root whose only remaining ground is the
            // deferred pile can already say so.
            self.drain();
            self.take_stock();
            covered |= self.walk_all(&deferred, phase.rank, queue);
            self.drain();
            self.take_stock();
            if !covered {
                log::debug!(
                    "Phases: pass {} over {} covered nothing, so the rest is left to the next one",
                    pass + 1,
                    phase.path.display()
                );
                self.report_a_vanished_volume_if_that_is_what_happened();
                break;
            }
        }
    }

    /// Walk each root in turn, consulting the visit queue between them. Reports
    /// whether anything was covered.
    fn walk_all(&self, roots: &[String], rank: Rank, queue: &mut PhaseQueue) -> bool {
        let mut covered = false;
        for root in roots {
            if !self.may_run() {
                return covered;
            }
            // A root the user just opened outranks home and the whole volume, and
            // never the priority roots — those are already the best answer to the
            // same question.
            if rank > Rank::VisitedRoot {
                self.take_a_visit(queue);
            }
            covered |= self.walk_one(root);
            self.take_stock();
        }
        covered
    }

    /// Run one root the user opened while we were walking, as its own small phase.
    ///
    /// ❌ It doesn't check the visit queue itself: a nested check would let a
    /// browsing user push the phase that is actually running arbitrarily far down,
    /// and one root per boundary is already faster than anyone can browse.
    fn take_a_visit(&self, queue: &mut PhaseQueue) {
        let Some(visited) = self.visits.take() else {
            return;
        };
        if queue.already_done(&visited) {
            return;
        }
        let phase = Phase {
            rank: Rank::VisitedRoot,
            path: visited,
        };
        log::debug!(
            "Phases: taking {} next, the user is looking at it",
            phase.path.display()
        );
        self.run_phase(&phase, queue);
        queue.mark_done(&phase.path);
    }

    /// Cover one frontier root and say whether it actually ran.
    ///
    /// Ground another walk on this volume already holds (a live search) is left to
    /// it: its rows land in the same index, and the next pass asks again.
    fn walk_one(&self, root: &str) -> bool {
        let context = match cover::context_for_walk(&self.volume_id) {
            Ok(context) => context.leaving_the_flush_to_the_caller(),
            Err(e) => {
                log::info!("Phases: can't walk '{}' right now: {e}", self.volume_id);
                return false;
            }
        };
        self.walking.store(true, Ordering::Relaxed);
        let walk = cover::start(
            context,
            vec![root.to_string()],
            CoverageDimension::Listing,
            self.cancel.child_token(),
        );
        let mine = walk.covered_by_another_walk().is_empty();
        // Draining the batches is what keeps the live entry counter honest: the
        // walk's own heartbeat counts directories, and a phased run has no total to
        // measure itself against, so the counter IS the progress.
        while let Some(batch) = walk.next_batch() {
            self.count(&batch);
        }
        let outcome = walk.finish();
        self.walking.store(false, Ordering::Relaxed);
        mine && outcome.roots_covered > 0
    }

    /// Fold one batch of discovered entries into the live counters.
    fn count(&self, batch: &[crate::indexing::scanner::CoveredEntry]) {
        let dirs = batch.iter().filter(|entry| entry.is_directory).count() as u64;
        let bytes: u64 = batch.iter().filter_map(|entry| entry.physical_size).sum();
        self.progress
            .entries_scanned
            .fetch_add(batch.len() as u64, Ordering::Relaxed);
        self.progress.dirs_found.fetch_add(dirs, Ordering::Relaxed);
        self.progress.bytes_scanned.fetch_add(bytes, Ordering::Relaxed);
    }

    /// What a scope still needs walked, in this volume's own space.
    fn frontier_under(&self, path: &Path) -> Vec<String> {
        let absolute = self.space.absolute(&path.to_string_lossy());
        let Some(index_path) = self.space.index_relative(&absolute) else {
            return Vec::new();
        };
        let Ok(conn) = IndexStore::open_read_connection(&self.writer.db_path()) else {
            return Vec::new();
        };
        coverage_for_scope(&conn, &index_path, &absolute, CoverageDimension::Listing)
            .map(|map| map.frontier)
            .unwrap_or_default()
    }

    /// Split a phase's frontier into what runs now and what runs after it.
    ///
    /// A coverage answer is explicitly unordered, so the walk order is decided
    /// here: alphabetical (so a log or a test reads the same way twice), with the
    /// one folder everything else would wait behind moved to the end.
    fn order(&self, mut frontier: Vec<String>) -> (Vec<String>, Vec<String>) {
        frontier.sort();
        let Some(deferred_root) = self.deferred_home_folder() else {
            return (frontier, Vec::new());
        };
        let deferred_root = deferred_root.to_string_lossy().into_owned();
        frontier
            .into_iter()
            .partition(|root| !crate::indexing::paths::path_prefix::is_at_or_under(root, &deferred_root))
    }

    /// The one folder inside home that goes last, when this volume has one.
    fn deferred_home_folder(&self) -> Option<PathBuf> {
        Some(self.home_on_this_volume()?.join(DEFERRED_HOME_FOLDER?))
    }

    /// Wait for the writer to commit what the walks sent it. Once per phase, not
    /// once per root; see the module docs.
    fn drain(&self) {
        if let Err(e) = self.writer.flush_blocking() {
            log::warn!("Phases: the walks' rows may not have landed: {e}");
        }
    }

    /// Ask the database what is complete now. Cheap, and derived from rows alone,
    /// so it survives a relaunch and can't drift from what was actually covered.
    fn take_stock(&self) {
        completion::take_stock(self);
    }

    /// Whether the machine may keep going: nobody stopped it, and both indexing
    /// switches still say yes. Asked per phase and per root, so turning drive
    /// indexing off stops the walking rather than only the next launch.
    fn may_run(&self) -> bool {
        if self.cancel.is_cancelled() {
            return false;
        }
        master::background_walk_allowed(master::master_enabled(), &self.writer.db_path())
    }

    /// A phase that covered nothing on a volume whose root won't list is a drive
    /// that went away, and the UI has a row that stays stuck until something says
    /// so. A whole-volume scan reports this as `ScanError::RootUnlistable`; a cover
    /// walk reports "covered nothing", so the machine has to make the call itself.
    ///
    /// Only asked after a fruitless pass, so a healthy run never pays for it.
    fn report_a_vanished_volume_if_that_is_what_happened(&self) {
        if std::fs::read_dir(&self.volume_root).is_ok() {
            return;
        }
        log::warn!(
            "Phases: '{}' can't be listed any more, so its index stops where it is",
            self.volume_id
        );
        crate::indexing::lifecycle::state::apply_freshness_event_on(
            &self.freshness,
            self.events.as_ref(),
            &self.volume_id,
            crate::indexing::lifecycle::freshness::FreshnessEvent::ScanFailed,
        );
        set_phase_for(
            self.events.as_ref(),
            &self.volume_id,
            ActivityPhase::Idle,
            "coverage stopped (volume vanished)",
        );
        self.events.emit(IndexEvent::ScanAborted {
            volume_id: self.volume_id.clone(),
        });
        self.cancel.cancel();
    }

    /// Tell the host a run started, with the calibration a progress tier needs.
    /// The same event a full scan fires, because from out here it IS the volume's
    /// first index — it simply arrives in pieces.
    fn announce_the_start(&self) {
        let prior = IndexStore::open_read_connection(&self.writer.db_path())
            .ok()
            .and_then(|conn| IndexStore::read_scan_calibration_set(&conn).ok())
            .unwrap_or_default();
        let prior = prior.for_kind(ScanRunKind::FirstScan.calibration_kind());
        self.events.emit(IndexEvent::ScanStarted {
            volume_id: self.volume_id.clone(),
            run_kind: ScanRunKind::classify(false, prior.total_entries),
            prior_total_entries: prior.total_entries,
            prior_scan_duration_ms: prior.scan_duration_ms,
            // The denominator a full scan reads once at start. A phased run has no
            // knowable total until the volume-root phase, so the tier it feeds
            // stays "elapsed and a live count" until then, by design.
            volume_used_bytes: None,
        });
        crate::indexing::lifecycle::state::apply_freshness_event_on(
            &self.freshness,
            self.events.as_ref(),
            &self.volume_id,
            crate::indexing::lifecycle::freshness::FreshnessEvent::ScanStarted,
        );
    }

    /// The machine has nothing queued: stop reporting as busy, and let the
    /// reporter's tick loop end.
    fn finish(&self) {
        self.working.store(false, Ordering::Relaxed);
        self.done.store(true, Ordering::Relaxed);
        set_phase_for(
            self.events.as_ref(),
            &self.volume_id,
            ActivityPhase::Idle,
            "coverage finished",
        );
        log::info!(
            "Phases: '{}' covered {} in {:.1}s",
            self.volume_id,
            cmdr_fs::pluralize::pluralize(self.progress.entries_scanned.load(Ordering::Relaxed), "entry"),
            self.started_at.elapsed().as_secs_f64(),
        );
    }
}
