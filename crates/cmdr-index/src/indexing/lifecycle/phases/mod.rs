//! Covering a volume in the order its owner cares about, a few frontier roots at
//! a time.
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
//!    in groups, checking the visit queue between them.
//! 3. After each drain, ask the database whether anything is complete. Completion
//!    is derived, never remembered: "the frontier under this root is empty".
//!
//! ## Four rules that are easy to get wrong
//!
//! - **One `cover()` call per GROUP of frontier roots**, joined before the next
//!   starts, with the group sized from what the last one cost (`grouping.rs`).
//!   Measured, the join costs nothing (41 s of real walking against a whole-volume
//!   walk's 38.1 s), and the gap between calls is where the visit queue gets
//!   consulted. ❌ Never one call for a whole phase's frontier: the check inside
//!   `cover` is not a point the machine can consult a queue at, and a group that
//!   runs for minutes is deaf to where the user is looking.
//! - **Take stock after a DRAIN, ❌ not after every root.** Completion is read off
//!   the database, and until the drain the roots just walked are still in the
//!   writer's queue — so a stock-take per root asks an expensive question (a
//!   coverage descent over the whole volume, which grows with how much is already
//!   covered) about a database that hasn't moved. Over a resumed run's thousands
//!   of small roots that was three quarters of the wall clock
//!   (`docs/notes/phased-vs-bulk-index-2026-08-14.md`).
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
use crate::indexing::events::{ActivityPhase, CoveragePhase, EventSink, IndexEvent, ScanRunKind, set_phase_for};
use crate::indexing::lifecycle::freshness::Freshness;
use crate::indexing::lifecycle::progress_reporter::ScanProgressReporter;
use crate::indexing::lifecycle::{cover, master};
use crate::indexing::read::coverage::{CoverageDimension, coverage_for_scope};
use crate::indexing::scanner::ScanProgress;
use crate::indexing::store::IndexStore;
use crate::indexing::writer::{AggSource, IndexWriter};

mod completion;
mod grouping;
mod queue;
mod stitch;
mod visits;

pub(crate) use completion::HOME_COVERED_AT_KEY;
pub(crate) use visits::VisitLog;

#[cfg(test)]
mod tests;

use grouping::Grouping;
use queue::{Phase, PhaseQueue};

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
/// `~/Library` is 27.7% of a real boot index and 48% of home's coverage wall
/// clock, so deferring it moves the early media kick 39 s earlier (measured by
/// `tests::home_bench::how_long_home_takes`; the numbers and their conditions live in
/// `../DETAILS.md` § "The early home signal", ❌ not restated at each use). It
/// stays in scope — search over it is occasionally what someone wants — but it
/// goes last inside its phase, and the early home signal doesn't wait for it.
/// Linux has no single equivalent pile, so it has none.
#[cfg(target_os = "macos")]
const DEFERRED_HOME_FOLDER: Option<&str> = Some("Library");
#[cfg(not(target_os = "macos"))]
const DEFERRED_HOME_FOLDER: Option<&str> = None;

/// Whether a drive's first index is covered in phases, the way this module does
/// it, or built by one bulk scan the way it was before.
///
/// **The escape hatch.** Covering in phases changes how every never-completed
/// volume is launched, and that lands in an open beta. Off, `launch_route` sends
/// each of those volumes back through `start_scan`, so a bad week costs a relaunch
/// rather than a rollback. Who flips it and how: `../DETAILS.md` § "The escape
/// hatch".
///
/// Read at startup and never live-applied: the app answers it once, from the
/// product's own settings, and hands the answer over with the rest of
/// `IndexConfig`. Defaults ON so a host that configures nothing (a unit test, a
/// bench, a tool) gets the shipping behavior.
static PHASED_FIRST_INDEX: AtomicBool = AtomicBool::new(true);

/// Mirror the product's phased-first-index switch into the process. Called from
/// the config seam; nothing else writes it.
pub(crate) fn set_phased_first_index(enabled: bool) {
    PHASED_FIRST_INDEX.store(enabled, Ordering::Relaxed);
    if !enabled {
        log::info!(
            target: "indexing::phases",
            "Phased first index is OFF: a drive with no completed scan is built by one bulk scan",
        );
    }
}

/// Whether a never-completed volume is the phase machine's to cover. Every launch
/// and every full-walk entry point asks this before it routes.
pub(crate) fn phased_first_index() -> bool {
    PHASED_FIRST_INDEX.load(Ordering::Relaxed)
}

/// Set the switch for one test and put it back on drop, so a test that turns it
/// off doesn't leak that into whichever test runs next in the same binary.
///
/// Process-wide, like every other seam a test handle installs: hold
/// `handle::test_lock()` first.
#[cfg(any(test, feature = "testing"))]
#[must_use = "the switch is restored when the guard drops"]
pub(crate) fn install_for_test(enabled: bool) -> PhasedFirstIndexGuard {
    PhasedFirstIndexGuard {
        previous: PHASED_FIRST_INDEX.swap(enabled, Ordering::Relaxed),
    }
}

/// Restores the phased-first-index switch on drop.
#[cfg(any(test, feature = "testing"))]
pub(crate) struct PhasedFirstIndexGuard {
    previous: bool,
}

#[cfg(any(test, feature = "testing"))]
impl Drop for PhasedFirstIndexGuard {
    fn drop(&mut self) {
        PHASED_FIRST_INDEX.store(self.previous, Ordering::Relaxed);
    }
}

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
    /// The frontier roots under the walker right now, for the status response a
    /// mid-run window reload reads. Empty between walks, which is the honest
    /// answer: nothing is moving.
    walked_roots: Arc<std::sync::Mutex<Vec<String>>>,
    /// The phase the machine is on, for that same status response. ⚠️ Unlike the
    /// ground above, it does NOT empty between walks: the phase a machine is part
    /// way through is what is running, whether or not a walk is reading the disk
    /// this millisecond.
    coverage_phase: Arc<std::sync::Mutex<Option<CoveragePhase>>>,
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

    /// The ground under the walker right now, for `get_status`.
    pub(crate) fn walked_roots(&self) -> Vec<String> {
        use cmdr_fs::ignore_poison::IgnorePoison;
        self.walked_roots.lock_ignore_poison().clone()
    }

    /// Which phase the machine is on, for `get_status`. `None` before the first
    /// one is announced.
    pub(crate) fn coverage_phase(&self) -> Option<CoveragePhase> {
        use cmdr_fs::ignore_poison::IgnorePoison;
        *self.coverage_phase.lock_ignore_poison()
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
        walked_roots: Arc::new(std::sync::Mutex::new(Vec::new())),
        coverage_phase: Arc::new(std::sync::Mutex::new(None)),
        visits: Arc::new(VisitLog::new()),
        cancel: context.cancel.clone(),
        done: Arc::new(AtomicBool::new(false)),
    };

    let machine = Machine {
        started_at: Instant::now(),
        walking: Arc::clone(&handle.walking),
        working: Arc::clone(&handle.working),
        progress: Arc::clone(&handle.progress),
        walked_roots: Arc::clone(&handle.walked_roots),
        coverage_phase: Arc::clone(&handle.coverage_phase),
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

/// How one group of frontier roots ended.
///
/// The two are independent: a group can cover a dozen roots and still be stopped
/// on the thirteenth, and a group can be stopped having covered nothing. The pass
/// loop needs both, because a machine that STOPPED has ground left on purpose and
/// a machine that covered nothing has run out of it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct GroupOutcome {
    /// Whether the walk finished any frontier root.
    covered: bool,
    /// Whether the machine stopped it to cover somewhere the user opened.
    preempted: bool,
}

/// The driver, which owns its thread for the whole run.
struct Machine {
    started_at: Instant,
    walking: Arc<AtomicBool>,
    working: Arc<AtomicBool>,
    progress: Arc<ScanProgress>,
    walked_roots: Arc<std::sync::Mutex<Vec<String>>>,
    coverage_phase: Arc<std::sync::Mutex<Option<CoveragePhase>>>,
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
            queue.push(CoveragePhase::PriorityRoot, root);
        }
        if let Some(home) = self.home_on_this_volume() {
            queue.push(CoveragePhase::Home, home);
        }
        queue.push(CoveragePhase::WholeVolume, self.volume_root.clone());
        queue
    }

    /// This machine's home directory, when it is on the volume being covered.
    /// `None` for an external drive, and for a machine with no home at all.
    fn home_on_this_volume(&self) -> Option<PathBuf> {
        let home = home_dir()?;
        home.starts_with(&self.volume_root).then_some(home)
    }

    /// One phase: stitch down to its root, then walk what its frontier still
    /// names, in groups, taking stock either side of the drain.
    fn run_phase(&self, phase: &Phase, queue: &mut PhaseQueue) {
        // The ORDER is the whole feature, so a support bundle has to show it. A
        // dozen lines per first index, and none after that.
        log::info!("Phases: covering {} ({:?})", phase.path.display(), phase.kind);
        set_phase_for(
            self.events.as_ref(),
            &self.volume_id,
            ActivityPhase::Scanning,
            &format!("covering {}", phase.path.display()),
        );
        self.announce_the_phase(phase);
        stitch::down_to(&self.space, &self.writer, &phase.path);

        for pass in 0..MAX_PASSES_PER_PHASE {
            let frontier = self.frontier_under(&phase.path);
            if frontier.is_empty() {
                break;
            }
            let (first, deferred) = self.order(frontier);
            let mut covered = self.walk_all(&first, phase, queue);
            // The drain and the check that follows it are why the deferred roots
            // are split out at all: everything else in this phase is covered at
            // this moment, so a phase root whose only remaining ground is the
            // deferred pile can already say so.
            self.drain();
            self.take_stock();
            covered |= self.walk_all(&deferred, phase, queue);
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

    /// Say which phase is running, so a host can name it: to whoever is listening
    /// now through the event, and to whoever joins later through the handle.
    ///
    /// ⚠️ Called again after a visited-root interlude ends, ❌ not only when a
    /// phase starts: the interlude announces ITSELF (it is a phase, ranked and
    /// run like any other), and without this the header would name the folder the
    /// user opened for the rest of the outer phase — "Indexing the folders you use
    /// most" while the machine walks the whole drive. Idempotent: a host maps the
    /// phase to one label and re-announcing the same one changes nothing.
    fn announce_the_phase(&self, phase: &Phase) {
        use cmdr_fs::ignore_poison::IgnorePoison;
        *self.coverage_phase.lock_ignore_poison() = Some(phase.kind);
        self.events.emit(IndexEvent::CoveragePhaseStarted {
            volume_id: self.volume_id.clone(),
            phase: phase.kind,
            root: phase.path.to_string_lossy().into_owned(),
        });
    }

    /// Walk the frontier in groups, consulting the visit queue between them and
    /// stopping a group that is standing between the user and the folder they just
    /// opened. Reports whether the phase should get another pass.
    ///
    /// How big a group is comes from what the last one cost (`grouping.rs`): big
    /// roots keep it at one, and the tiny ones an interrupted run leaves behind let
    /// it grow, so the per-call cost stops being the whole cost.
    fn walk_all(&self, roots: &[String], phase: &Phase, queue: &mut PhaseQueue) -> bool {
        let mut another_pass = false;
        let mut grouping = Grouping::new();
        let mut rest = roots;
        loop {
            if !self.may_run() {
                return another_pass;
            }
            // A root the user just opened outranks home and the whole volume, and
            // never the priority roots — those are already the best answer to the
            // same question. An interlude that ran announced itself, so this phase
            // has to say what it is again on the way back.
            if phase.kind > CoveragePhase::VisitedRoot && self.take_a_visit(queue) {
                self.announce_the_phase(phase);
            }
            // ⚠️ Asked AFTER the visit check, ❌ never before it: the group a walk
            // was stopped for is routinely the last one, and testing emptiness
            // first would end the phase without ever running the folder the
            // machine stopped for.
            if rest.is_empty() {
                return another_pass;
            }
            let (group, remaining) = rest.split_at(grouping.roots().min(rest.len()));
            rest = remaining;
            let started = Instant::now();
            let outcome = self.walk_group(group, phase, queue);
            // A pass the machine stopped ON PURPOSE didn't run out of ground: the
            // roots it left are still frontier, and the next pass asks for them
            // again. Reading it as "covered nothing" would end the phase and leave
            // the volume to the retry ladder every time somebody browsed.
            another_pass |= outcome.covered || outcome.preempted;
            // ❌ Don't size the next group off one somebody cut short. It looks
            // cheap because it was stopped, and the sizing rule would answer by
            // handing the next call MORE roots — the opposite of what a machine
            // being interrupted wants.
            if !outcome.preempted {
                grouping.note(group.len(), started.elapsed());
            }
        }
    }

    /// Run one root the user opened while we were walking, as its own small phase.
    /// Reports whether one actually ran, which is what the caller owes a
    /// re-announcement for.
    ///
    /// ❌ It doesn't check the visit queue itself: a nested check would let a
    /// browsing user push the phase that is actually running arbitrarily far down,
    /// and one root per boundary is already faster than anyone can browse.
    /// ⚠️ Folders that have already had their turn are skipped rather than ending
    /// the check, so this asks the same question `a_visit_is_waiting_behind_this_walk`
    /// does. Both panes report every tick, so one parked on covered ground sits in
    /// front of the folder somebody just opened — and stopping a walk for a visit
    /// this then declined would stop the next walk too.
    fn take_a_visit(&self, queue: &mut PhaseQueue) -> bool {
        while let Some(visited) = self.visits.take() {
            if queue.already_done(&visited) {
                continue;
            }
            let phase = Phase {
                kind: CoveragePhase::VisitedRoot,
                path: visited,
            };
            log::debug!(
                "Phases: taking {} next, the user is looking at it",
                phase.path.display()
            );
            self.run_phase(&phase, queue);
            queue.mark_done(&phase.path);
            return true;
        }
        false
    }

    /// Cover a group of frontier roots in one walk, and say how it ended.
    ///
    /// The group is one claim, one branch bracket, one walk thread, and one
    /// backend session; inside it the walk takes the roots one at a time. Ground
    /// another walk on this volume already holds (a live search) is left to it:
    /// its rows land in the same index, and the next pass asks again.
    ///
    /// ⚠️ It also STOPS the walk when the folder somebody just opened is waiting
    /// behind it. A frontier root can be 1.58M entries and no stitch depth splits
    /// it (`../DETAILS.md` § "Why a visited root doesn't wait for a big sibling"),
    /// so the gap between groups is not a fine enough grain: without this, "what
    /// you open gets indexed next" means "in forty seconds".
    fn walk_group(&self, roots: &[String], phase: &Phase, queue: &PhaseQueue) -> GroupOutcome {
        let context = match cover::context_for_walk(&self.volume_id) {
            Ok(context) => context.leaving_the_flush_to_the_caller(),
            Err(e) => {
                log::info!("Phases: can't walk '{}' right now: {e}", self.volume_id);
                return GroupOutcome::default();
            }
        };
        self.walking.store(true, Ordering::Relaxed);
        self.note_walked_roots(roots.to_vec());
        // The ground is the walker's from here to `finish`, and the host is told
        // both ends: a folder's size can move under the user for exactly this
        // long, and the pair is what lets a listing say so per row instead of
        // marking the whole drive in flux for the whole run.
        self.events.emit(IndexEvent::CoverageBranchStarted {
            volume_id: self.volume_id.clone(),
            roots: roots.to_vec(),
        });
        // This walk's own stop signal, under the machine's: stopping it hands its
        // ground on without ending the run.
        let walk_cancel = self.cancel.child_token();
        let walk = cover::start(
            context,
            roots.to_vec(),
            CoverageDimension::Listing,
            walk_cancel.clone(),
            // Background coverage: it leaves ground a search holds to the search,
            // and hands its own over when one asks.
            cover::WalkFor::TheIndex,
        );
        // Draining the batches is what keeps the live entry counter honest: the
        // walk's own heartbeat counts directories, and a phased run has no total to
        // measure itself against, so the counter IS the progress. It is also the
        // one place inside a walk the machine gets a say, which is why the visit
        // check lives here rather than only between groups.
        // ⚠️ The check comes BEFORE the blocking receive, ❌ not after it: the
        // folder somebody opened routinely lands while the walk is spinning up,
        // and asking only once a batch has arrived would hold the machine on a
        // root it has already decided to leave.
        let mut preempted = false;
        loop {
            if !preempted && self.a_visit_is_waiting_behind_this_walk(phase, queue) {
                log::debug!(
                    "Phases: stopping the walk of {} to cover what the user just opened",
                    roots.first().map(String::as_str).unwrap_or("-")
                );
                walk_cancel.cancel();
                preempted = true;
            }
            let Some(batch) = walk.next_batch() else {
                break;
            };
            self.count(&batch);
        }
        let outcome = walk.finish();
        self.walking.store(false, Ordering::Relaxed);
        self.note_walked_roots(Vec::new());
        // ⚠️ Unconditional, on every exit path (covered, left to another walk,
        // cancelled). A missed end leaves a row wearing an hourglass for a walk
        // that stopped, and nothing later would take it off.
        self.events.emit(IndexEvent::CoverageBranchEnded {
            volume_id: self.volume_id.clone(),
            roots: roots.to_vec(),
        });
        // ⚠️ What the walk COVERED, ❌ not "no root was left to another walk": a
        // group can hold one root a live search is already walking and a dozen
        // nobody is, and reading the whole group as uncovered would end the phase's
        // pass with ground still on the frontier.
        GroupOutcome {
            covered: outcome.roots_covered > 0,
            preempted,
        }
    }

    /// Whether stopping the walk in flight would let the machine cover a folder
    /// somebody has open.
    ///
    /// A PEEK, ❌ never a take: the interlude can't run until the walk it would
    /// interrupt has ended, so deciding to stop and deciding what to run next are
    /// two moments, and taking here would drop the visit on the floor in between.
    /// A root that has already had its turn buys nothing, which is what keeps a
    /// pane sitting on one folder from stopping every walk of the run.
    fn a_visit_is_waiting_behind_this_walk(&self, phase: &Phase, queue: &PhaseQueue) -> bool {
        phase.kind > CoveragePhase::VisitedRoot && self.visits.any_waiting(|path| queue.already_done(path))
    }

    /// Record what is under the walker, for the status a mid-run reload reads.
    /// The EVENTS are what the UI follows; this is the same fact, pullable.
    fn note_walked_roots(&self, roots: Vec<String>) {
        use cmdr_fs::ignore_poison::IgnorePoison;
        *self.walked_roots.lock_ignore_poison() = roots;
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
            // Which is also what this says: follow the branch events for what is
            // under the walker, rather than reading the whole volume as in flux
            // for the run's whole length.
            covered_in_phases: true,
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
            cmdr_fs::pluralize::pluralize_with(
                self.progress.entries_scanned.load(Ordering::Relaxed),
                "entry",
                "entries"
            ),
            self.started_at.elapsed().as_secs_f64(),
        );
        self.pick_the_leftovers_up_later();
    }

    /// Say so when the machine stops with ground still on the frontier, and ask
    /// for another go at it.
    ///
    /// ⚠️ Without the line the two endings are indistinguishable in a log: a
    /// volume that finished and one that ran out of passes while somebody wrote to
    /// it both end on the line above and an idle phase. The second leaves the
    /// drive unmarked, and a support bundle has to be able to say which happened —
    /// a bench couldn't, and one unexplained run was the cost
    /// (`docs/notes/churn-against-completion-2026-08-15.md`).
    ///
    /// The retry is the in-session half of the same fact: the next launch would
    /// settle this drive in ~2 s, and `completion_retry` runs that resume on a
    /// backoff instead of making somebody quit the app for it. ❌ Nothing here
    /// marks anything complete: the frontier really isn't empty, and every surface
    /// saying so stays right until a walk empties it.
    ///
    /// Only asked once a run is over, so it costs one coverage query per first
    /// index.
    fn pick_the_leftovers_up_later(&self) {
        let left = self.frontier_under(&self.volume_root);
        if left.is_empty() {
            return;
        }
        log::info!(
            "Phases: '{}' stops with {} still to walk, so nothing marks it complete yet (first: {})",
            self.volume_id,
            cmdr_fs::pluralize::pluralize(left.len() as u64, "folder"),
            left.first().map(String::as_str).unwrap_or("-"),
        );
        // ⚠️ A machine somebody STOPPED didn't run out of passes: the volume is
        // being torn down, the master switch went off, or the drive vanished
        // (`report_a_vanished_volume_if_that_is_what_happened` cancels for exactly
        // this reason). Scheduling a retry there would wake a drive nothing is
        // indexing any more, every minute, until the app quits.
        if self.cancel.is_cancelled() {
            return;
        }
        crate::indexing::lifecycle::completion_retry::arm(&self.volume_id, crate::indexing::store::now_unix());
    }
}
