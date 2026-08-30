//! Walking a coverage frontier: the write half of the coverage concept.
//!
//! `read/coverage.rs` answers what a scope still needs walked; this drives the
//! walk that fills it in, and hands the entries it finds to whoever asked while
//! it's still running. Every row it writes goes through the volume's normal
//! writer into the normal index (Decision 2), so the work survives the search
//! that paid for it and the next search over the same ground walks less.
//!
//! ## Two kinds of ground, and the one branch between them
//!
//! [`Ground`] is the ONLY thing here that asks what kind of volume this is: a
//! local filesystem is read by the guarded walker, and everything the index
//! reaches only through a `Volume` — a share, a phone, whatever backend comes
//! next — by `network_scanner`'s scoped walk. Downstream of a discovered entry
//! the two are the same code: one writer, one set of epochs, one frontier query,
//! one descent rule.
//!
//! ## Two primitives on the local half, and which one runs
//!
//! A frontier node is virgin ground by definition, so the workload is a bulk add
//! and the PARALLEL walker wins it outright — measured on a real frontier in
//! `docs/notes/cover-walk-primitive-2026-08-05.md`. It runs by default.
//!
//! The serial reconcile is the repair path, for the one case the parallel walker
//! can't take: a frontier node that ISN'T virgin. Those exist (an FSEvents
//! verification pass writes children under a directory without marking that
//! directory listed), and the parallel walker allocates fresh ids for every name
//! it finds, so over pre-existing rows `INSERT OR IGNORE` would drop its rows
//! silently and orphan everything below them. `reconcile_subtree` compares by
//! name and writes only differences, which is exactly the shape that case needs.
//! The trait walk needs no such split: it compares names per directory as it
//! goes, so it takes that case itself. ❌ No path ever deletes: covering is
//! add-only work.

use std::path::Path;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};
use std::thread::JoinHandle;

use cmdr_fs::volume::Volume;
use tokio_util::sync::CancellationToken;

use crate::indexing::IndexPathSpace;
use crate::indexing::host::runtime;
use crate::indexing::metadata::{MetadataSnapshot, extract_metadata};
use crate::indexing::network_scanner::scan_pace::ScanPacer;
use crate::indexing::network_scanner::{VolumeScanError, cover_volume_subtree, stat_one_directory};
use crate::indexing::read::coverage::CoverageDimension;
use crate::indexing::scanner::{CoveredEntry, ScanError, ScanSummary, WalkHeartbeat, cover_subtree};
use crate::indexing::store::IndexStore;
use crate::indexing::volume::IndexVolumeKind;
use crate::indexing::writer::IndexWriter;
use cmdr_fs::pluralize::{pluralize, pluralize_with};

/// How many batches may sit between the walk and its consumer.
///
/// Bounded on purpose (Decision 3): a consumer that falls behind slows the walk
/// down rather than letting a queue grow to the size of the subtree. Small,
/// because each batch already holds up to 2 000 entries.
const BATCH_QUEUE_DEPTH: usize = 8;

/// What a walk over a frontier covered.
///
/// `cancelled` is the field that matters to a caller: it separates "the index now
/// answers for this scope" from "someone stopped us partway", and the two are
/// different terminal states in the UI. It is NOT a failure either way — a
/// cancelled walk still left every directory it read marked, so the next search
/// over the same ground starts from where this one stopped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoverOutcome {
    /// Entries the walk discovered and wrote.
    pub entries_found: u64,
    /// Directories among them.
    pub dirs_found: u64,
    /// Frontier roots it finished. Anything it didn't reach stays frontier, and
    /// a fresh `coverage` call names it.
    pub roots_covered: usize,
    /// Whether it stopped early because someone cancelled it.
    pub cancelled: bool,
    /// Whether it gave up on ground it started: a directory abandoned after it
    /// stopped responding, or a subtree pruned by the consecutive-failure budget.
    ///
    /// Independent of every field above: a walk can cover every frontier root it
    /// took, uncancelled, and still have read less than the tree holds. So a
    /// caller that reports completeness has to consult this too, or a short
    /// answer reads as an exhaustive one. Those directories are never marked
    /// listed, so the next search offers them again.
    pub abandoned_ground: bool,
}

impl CoverOutcome {
    /// What a walk that read nothing reports. `cancelled` says whether anyone
    /// stopped it, which is the one thing the three no-work paths disagree on: a
    /// walk that took no ground was never stopped, a walk whose thread wouldn't
    /// spawn or wouldn't join never got the chance to finish.
    fn nothing(cancelled: bool) -> Self {
        Self {
            entries_found: 0,
            dirs_found: 0,
            roots_covered: 0,
            cancelled,
            abandoned_ground: false,
        }
    }
}

/// A running walk over a frontier.
///
/// Take batches off it until [`next_batch`](Self::next_batch) reports `None`,
/// then [`finish`](Self::finish) for the totals. Dropping it does NOT stop the
/// walk (Decision 11: a superseded query keeps its walk running, because walking
/// is coverage work and matching is query work) — the walk simply stops emitting
/// and runs to completion filling the index.
///
/// ❌ It carries no `cancel` of its own: the handle holds a `Receiver`, so it
/// can't be shared with a second thread, and stopping a walk is nearly always
/// someone else's decision. Cancelling is the token [`start`] took, which the
/// caller keeps a clone of.
pub struct CoverWalk {
    batches: Receiver<Vec<CoveredEntry>>,
    /// `None` for a walk that took no ground: there was nothing to spawn a thread
    /// for. See [`took_no_ground`](CoverWalk::took_no_ground).
    thread: Option<JoinHandle<CoverOutcome>>,
    deferred: Vec<String>,
    heartbeat: WalkHeartbeat,
}

impl CoverWalk {
    /// The handle for a request whose every frontier root belongs to a walk
    /// already running (and for the degenerate empty frontier): no thread, no
    /// batches, and [`finish`](Self::finish) answers on the spot.
    ///
    /// ⚠️ Its promptness is the point, not an optimization. A spawned thread runs
    /// `walk_frontier` whatever its frontier holds, and that function's tail commits
    /// the writer — which parks behind everything already queued. Behind a drive's
    /// first index that is seconds of waiting to commit nothing, and the search that
    /// asked stays silent for all of it instead of saying whose walk it's behind.
    fn took_no_ground(deferred: Vec<String>) -> Self {
        let (sender, batches) = sync_channel(1);
        drop(sender);
        Self {
            batches,
            thread: None,
            deferred,
            heartbeat: WalkHeartbeat::new(),
        }
    }

    /// The next batch of entries, blocking until one arrives. `None` once the
    /// walk has ended, for whatever reason.
    pub fn next_batch(&self) -> Option<Vec<CoveredEntry>> {
        self.batches.recv().ok()
    }

    /// How many directories the walk has STARTED reading, as a counter it keeps
    /// writing to.
    ///
    /// A counter rather than a number, because the two threads a caller needs are
    /// not the same one: this handle goes wherever `next_batch` is blocked, and
    /// whoever reports progress is elsewhere. A batch-derived count reads zero for
    /// as long as a batch takes to fill (2 000 entries, or forever on a directory
    /// that hangs), so this is what "still working" has to be measured from.
    pub fn dirs_scanned_counter(&self) -> Arc<std::sync::atomic::AtomicU64> {
        self.heartbeat.dirs_scanned_counter()
    }

    /// The directory the walk started reading most recently, same reasoning.
    /// Indicative, not a cursor: the local walker reads several at once.
    pub fn current_dir_slot(&self) -> Arc<Mutex<Option<String>>> {
        self.heartbeat.current_dir_slot()
    }

    /// Frontier roots this walk is NOT covering, because another walk on the same
    /// volume already is.
    ///
    /// Their rows land in the same index either way, and a query re-run once the
    /// other walk gets there picks them up — so this is "you'll get these a bit
    /// later", never "these are lost". Normally empty; it fills when a refined
    /// query asks for ground its predecessor's walk is still covering, which
    /// Decision 11 keeps running.
    pub fn covered_by_another_walk(&self) -> &[String] {
        &self.deferred
    }

    /// Wait for the walk to end and report what it covered.
    ///
    /// Drops the batch channel first, so a caller that stopped reading batches
    /// doesn't deadlock against a walk parked on a full one. ❌ Cancel the token
    /// first if you want it to stop — on its own this waits for the whole
    /// frontier.
    pub fn finish(self) -> CoverOutcome {
        let CoverWalk { batches, thread, .. } = self;
        drop(batches);
        let Some(thread) = thread else {
            // Nothing ran and nothing stopped it: the ground was already somebody
            // else's when this walk asked for it.
            return CoverOutcome::nothing(false);
        };
        thread.join().unwrap_or_else(|_| CoverOutcome::nothing(true))
    }
}

/// Everything one walk needs, resolved on the caller's thread so a bad request
/// fails before a thread is spawned.
pub(crate) struct CoverContext {
    pub volume_id: String,
    pub writer: IndexWriter,
    pub space: IndexPathSpace,
    /// Which half of [`Ground`] this volume's rows come from. The ONE thing the
    /// walk branches on per kind; everything downstream of a discovered entry is
    /// identical.
    pub kind: IndexVolumeKind,
    /// Who commits what the walk wrote before anyone reads it.
    pub flush: FlushOnFinish,
}

/// Who waits for the writer once a walk ends.
///
/// The default is the walk itself, which is what a caller that asks a coverage
/// question the moment its walk returns needs — a search, above all: the marks
/// matter more than the rows, because a directory the walk gave up on carries its
/// cause in the same batch, and an uncommitted batch reads as "still uncovered".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum FlushOnFinish {
    /// Block until the writer has everything, before reporting.
    #[default]
    BeforeReporting,
    /// Report immediately, leaving the drain to the caller.
    ///
    /// For a caller running MANY walks in a row: a blocking flush per walk means
    /// the walker and the writer never overlap, which measured 37.5 s of standing
    /// still over ~1,500 frontier roots
    /// (`docs/notes/phased-vs-bulk-index-2026-08-14.md`). ⚠️ Taking this OWES the
    /// drain before reading coverage back or reporting anything as complete.
    LeftToTheCaller,
}

impl CoverContext {
    /// Hand the post-walk drain to the caller. See [`FlushOnFinish::LeftToTheCaller`]
    /// for what taking it owes.
    pub(crate) fn leaving_the_flush_to_the_caller(mut self) -> Self {
        self.flush = FlushOnFinish::LeftToTheCaller;
        self
    }
}

/// How long a walk somebody is waiting on gives a background walk to hand its
/// ground over before taking what it can and reporting the rest.
///
/// A bound rather than a promise: the walker checks the token between
/// directories, so the wait is one directory's read plus the parallel walker's
/// own drain. Measured locally, that is 89 ms median over 2,400-directory roots
/// and 151 ms median (214 ms worst) over 40,000-directory ones
/// (`docs/notes/preemption-2026-08-18.md`). This budget is 3.5× the worst of
/// those, which leaves room for a share's listing round trip, and it is short
/// enough that a walk which never stops costs a search a fraction of a second
/// rather than the tens of seconds it used to wait for a whole frontier root.
const YIELD_WAIT: std::time::Duration = std::time::Duration::from_millis(750);

/// Start walking `frontier` on the volume `context` describes.
///
/// The paths are the ones a [`coverage`](crate::Index::coverage) answer named,
/// each taken whole: nothing under a frontier node is covered, so there is no
/// pruning to do inside one. Ground another walk on this volume is already
/// covering is left to it and reported as
/// [`covered_by_another_walk`](CoverWalk::covered_by_another_walk) — unless
/// `for_whom` says somebody is waiting on this walk, in which case the background
/// walk holding that ground is asked to hand it over.
pub(crate) fn start(
    context: CoverContext,
    frontier: Vec<String>,
    dimension: CoverageDimension,
    cancel: CancellationToken,
    for_whom: WalkFor,
) -> CoverWalk {
    // Deliberately an irrefutable `let`: a second dimension has to become a
    // compile error here, not a silently-ignored parameter.
    let CoverageDimension::Listing = dimension;

    // ⚠️ A CHILD of whatever the caller passed, always. Stopping one walk so its
    // ground can change hands must not stop the volume, and a caller that handed
    // its own token straight in would have every yield cancel everything else
    // hanging off it. The caller's token still stops this walk, because that is
    // what a parent does.
    let walk_cancel = cancel.child_token();
    // ONE pulse for the whole frontier, not one per root: a consumer watching a
    // walk of eight roots wants a count that keeps climbing, not one that restarts.
    // Made before the claim so the holder carries it: a walk in the table nobody
    // can read progress from is a walk somebody may wait an hour on.
    let heartbeat = WalkHeartbeat::new();
    let holder = Holder::walking(walk_cancel.clone(), for_whom, heartbeat.dirs_scanned_counter());

    // Taken on the CALLER's thread, so the answer is already true by the time
    // this returns: a caller that starts two walks in a row can't have the second
    // one claim ground the first hasn't reached the registry with yet.
    //
    // A cover walk speaks only for the ground it names, so it composes with the
    // phase machine covering the same volume in pieces (Decision 13) and with any
    // other walk that stays off its frontier. ❌ Never `Holder::Rewriting` — that
    // is for a holder that blanks the whole database.
    let claim = match for_whom {
        // ❌ Never the waiting form for background work: a machine that queued
        // behind user walks would stop converging the moment somebody kept
        // searching (constraint 4).
        WalkFor::TheIndex => Claim::take(&context.volume_id, frontier, holder),
        WalkFor::TheUser => Claim::preempt(&context.volume_id, frontier, holder, YIELD_WAIT),
    };
    let deferred = claim.deferred().to_vec();

    // Every root belongs to a walk already running, so there is no walk to make.
    // ❌ Don't spawn one anyway: a thread with no ground still runs `walk_frontier`
    // to the end, which opens a backend session, commits the writer, and hands the
    // volume's rescan request on — all of it on behalf of nothing. The commit is
    // what hurts, because it parks behind every batch already queued, and the
    // caller's `finish` waits for it: 4.5-5.8 s behind a boot disk's first index in
    // the app, and 35 s on a cold one, spent to commit nothing
    // (`docs/notes/cover-no-ground-block-2026-08-15.md`).
    //
    // Nothing is owed on the way out either: the claim took no ground, so no ground
    // is freed, and whoever holds it runs the rescan when THEY let go.
    if claim.mine().is_empty() {
        return CoverWalk::took_no_ground(deferred);
    }

    // Tell the volume's watcher what this walk is about to cover, BEFORE it reads
    // anything. Two things follow from that order: a change landing in the ground
    // the walk has already passed waits rather than racing the walk's own ids, and
    // when the walk ends this ground is watched — which is what lets walk-written
    // coverage carry no expiry (Decision 9). Ground another walk claimed is
    // already registered by that walk, so only `mine` goes in.
    super::state::begin_branch_coverage(&context.volume_id, claim.mine());

    let (sender, batches) = sync_channel(BATCH_QUEUE_DEPTH);
    let walk_heartbeat = heartbeat.clone();
    let thread = std::thread::Builder::new()
        .name("index-cover".into())
        .spawn(move || {
            // Yield CPU to the UI, exactly as the full scan does: someone is
            // waiting on the results, but they're waiting on the UI more.
            cmdr_fs::thread_qos::set_current_thread_qos(cmdr_fs::thread_qos::QosClass::Utility);
            // The claim lives as long as the walk and no longer, so its ground
            // frees up on the completion path, the cancel path, and a panic alike.
            let outcome = walk_frontier(&context, claim.mine(), &sender, &walk_cancel, &walk_heartbeat);
            release_ground(&context.volume_id, claim);
            outcome
        })
        .unwrap_or_else(|e| {
            // A machine that can't spawn a thread has a bigger problem than this
            // walk; report nothing covered rather than pretending otherwise.
            log::warn!("Cover: couldn't spawn the walk thread: {e}");
            std::thread::spawn(|| CoverOutcome::nothing(true))
        });

    CoverWalk {
        batches,
        thread: Some(thread),
        deferred,
        heartbeat,
    }
}

/// Everything a finished walk owes the volume, in the one order that works.
///
/// 1. **The branch set first.** Whatever the outcome, a cancelled walk still
///    marked every directory it read, so that ground needs watching exactly as
///    much as a completed walk's does. It runs after the walk's own flush, so the
///    rows the released events land on are the rows the walk wrote.
/// 2. **Then the claim**, which is what frees the ground for anything else.
/// 3. **Then the rescan this walk made someone wait for**, which is why the order
///    matters at all: fired before the claim went, the scan's own claim would find
///    this very walk still holding the ground, and defer itself again.
///
/// ❌ Not folded into `state::finish_branch_coverage` (which several tests and the
/// shutdown-window path call on their own) and ❌ not hung off `Claim`'s `Drop`:
/// the claim is a lock-level primitive that tests take and release freely, and a
/// scan spawning out of a destructor is a side effect nobody reading `Claim` would
/// expect.
fn release_ground(volume_id: &str, claim: Claim) {
    super::state::finish_branch_coverage(volume_id, claim.mine());
    drop(claim);
    super::rescan_request::run_if_owed(volume_id);
}

/// Walk every frontier root in turn, on the walk thread.
///
/// The backend's scan session brackets the WHOLE frontier, not each root: over SMB
/// that's a pool of extra connections, and opening one per frontier root would pay
/// the setup repeatedly for the same walk. ❌ Nothing between the two calls may
/// return early — the pairing is what keeps a cancelled walk from leaving the pool
/// standing.
fn walk_frontier(
    context: &CoverContext,
    frontier: &[String],
    sender: &SyncSender<Vec<CoveredEntry>>,
    cancel: &CancellationToken,
    heartbeat: &WalkHeartbeat,
) -> CoverOutcome {
    let started = std::time::Instant::now();
    // Arm the writer-wait probe, so the line at the end can say how much of this
    // walk was the queue rather than the disk (`writer/wait_probe.rs`).
    crate::indexing::writer::wait_probe::take();

    let Some(ground) = Ground::under(context) else {
        log::warn!(
            "Cover: '{}' isn't reachable right now, so nothing is walked",
            context.volume_id
        );
        return CoverOutcome::nothing(true);
    };

    ground.open_session();
    let mut outcome = walk_roots(context, &ground, frontier, sender, cancel, heartbeat);
    ground.close_session();
    // Read after the walk, not during it: the local half only knows what it gave
    // up on once its engine reports, and a give-up on the last root counts as much
    // as one on the first.
    outcome.abandoned_ground = heartbeat.abandoned_count() > 0;

    // Everything this walk learned is COMMITTED before it reports, on the
    // cancelled path too. The rows are the smaller half of that: the marks
    // matter more — a directory the walk gave up on carries `known_unreadable`,
    // and a caller that asks what's still uncovered the moment the walk ends
    // would otherwise be told "nothing", one search too early. It's the last
    // thing the walk does, so nothing waits on the queue that didn't have to.
    //
    // A caller running many walks in a row takes that drain over (a flush per walk
    // stops the walker and the writer overlapping at all) — except in two cases.
    //
    // One: this walk's ground BUFFERED live events while it ran. Those are
    // released the moment the branch is finished, a few lines below, and the loop
    // that replays them resolves paths through a read connection: against rows
    // still sitting in the writer's batch, every one of them would look like a
    // change under a missing parent.
    //
    // Two: ⚠️ this walk was STOPPED, so its ground can change hands the moment it
    // lets go — and a preemption is exactly that, immediately. The next holder
    // decides what is virgin ground by reading the DATABASE, so rows still in the
    // queue read as directories nobody has written, and it would allocate fresh
    // ids for names this walk already named: the `INSERT OR IGNORE` collision the
    // claim table exists to prevent. The ground comes back when nothing is
    // WALKING it; it changes hands when nothing is WRITING it.
    let owed = context.flush == FlushOnFinish::BeforeReporting
        || cancel.is_cancelled()
        || super::state::branch_coverage_buffered_events(&context.volume_id, frontier);
    if owed && let Err(e) = context.writer.flush_blocking() {
        log::warn!("Cover: the walk's last rows may not have landed: {e}");
    }

    // Both numbers, always: a walk parked on a saturated writer queue and a walk
    // reading a slow disk take the same wall time and want different fixes, and
    // without the split the line blames the walker for the queue.
    let waited = crate::indexing::writer::wait_probe::take();
    log::debug!(
        "Cover: {} over {}{} in {:.1?} ({:.1?} of it waiting on the writer)",
        pluralize_with(outcome.entries_found, "entry", "entries"),
        pluralize(outcome.roots_covered as u64, "frontier root"),
        if outcome.cancelled { " (cancelled)" } else { "" },
        started.elapsed(),
        waited,
    );
    outcome
}

/// The frontier loop itself, one root at a time, whatever kind of ground it is.
fn walk_roots(
    context: &CoverContext,
    ground: &Ground,
    frontier: &[String],
    sender: &SyncSender<Vec<CoveredEntry>>,
    cancel: &CancellationToken,
    heartbeat: &WalkHeartbeat,
) -> CoverOutcome {
    let mut outcome = CoverOutcome {
        entries_found: 0,
        dirs_found: 0,
        roots_covered: 0,
        cancelled: false,
        abandoned_ground: false,
    };

    for path in frontier {
        if cancel.is_cancelled() {
            outcome.cancelled = true;
            break;
        }
        let root = Path::new(path);
        // Ground the index has no row for can't be resolved to a scan root, and
        // that isn't only a cold volume's problem: a folder created since its
        // parent was last listed has no row on a fully indexed drive either. Give
        // the walk the chain it needs, without claiming a listing for any of it.
        match bootstrap::ensure_walkable(context, ground, root) {
            Err(e) => {
                log::warn!("Cover: can't walk {path}: {e}");
                continue;
            }
            // A root this walk had to create is a row no index reader has ever
            // seen, and a walk reports a directory's CONTENTS — so unless it goes
            // out here, the one entry a scoped search can never answer with is the
            // folder the user scoped to. A root the index already held is already
            // the reader's to report, and emitting it again would double it.
            Ok(bootstrap::RootRow::Created(snapshot)) => {
                emit_root(root, &snapshot, sender);
                // Counted with the rows the walk itself writes, so "what the
                // consumer saw" and "what this walk added" stay the same number.
                outcome.entries_found += 1;
                outcome.dirs_found += 1;
            }
            Ok(bootstrap::RootRow::Existing) => {}
        }
        // A partial walk's totals count exactly as much as a complete one's, so
        // both arms hand the same summary to the same accumulation and only the
        // VERDICT differs. Keeping one `+=` pair is what stops the cancel path
        // drifting from the completion path.
        let (summary, verdict) = ground.cover(context, root, sender, cancel, heartbeat);
        if let Some(summary) = summary {
            outcome.entries_found += summary.total_entries;
            outcome.dirs_found += summary.total_dirs;
        }
        match verdict {
            RootOutcome::Covered => outcome.roots_covered += 1,
            RootOutcome::Cancelled => {
                outcome.cancelled = true;
                break;
            }
            RootOutcome::Failed => {}
            // The roots behind it are on that same volume and that same session, so
            // asking each of them buys a round trip that can't come back: up to
            // `LIST_TIMEOUT` (120 s) apiece, times a frontier of thousands.
            RootOutcome::VolumeGone => break,
        }
    }
    outcome
}

/// Hand a frontier root itself to the consumer, in the shape every other
/// discovered entry arrives in.
///
/// A batch of one, because there is one of them per root and it goes out before
/// the root's listing does — the folder, then what's inside it, which is the order
/// a reader expects. ❌ Its ancestors don't come with it: they're above whatever
/// scope asked for the walk.
fn emit_root(root: &Path, snapshot: &MetadataSnapshot, sender: &SyncSender<Vec<CoveredEntry>>) {
    // A dropped receiver means the consumer stopped listening (a superseded
    // query), which is not this walk's business: it keeps filling the index.
    let _ = sender.send(vec![CoveredEntry {
        path: root.to_path_buf(),
        is_directory: true,
        // `stat_directory` answers `None` for a symlink rather than describing
        // one, so a root that got here is a real directory.
        is_symlink: false,
        logical_size: snapshot.logical_size,
        physical_size: snapshot.physical_size,
        modified_at: snapshot.modified_at,
    }]);
}

/// How a volume's ground gets read.
///
/// Two halves, and every volume kind falls in one of them: the LOCAL guarded
/// walker reads the disk directly, and everything else is reached only through its
/// [`Volume`]. That's the whole per-kind branch in the coverage concept — the
/// frontier query, the descent rule, the epochs, and the writer are identical on
/// both sides, so a new backend needs no coverage code of its own.
pub(super) enum Ground {
    /// A local filesystem: the boot disk or a plain external mount.
    Local,
    /// A share, a phone, or whatever backend comes next.
    ViaTrait {
        volume: Arc<dyn Volume>,
        /// The same per-volume listing budget the background scan yields with, so
        /// browsing the share while it walks drops it to one listing in flight.
        pacer: ScanPacer,
    },
}

impl Ground {
    /// Which half this volume falls in, or `None` when a trait-scanned volume
    /// isn't registered any more (ejected, or a share that dropped between the
    /// coverage answer and the walk).
    fn under(context: &CoverContext) -> Option<Self> {
        if !context.kind.is_trait_scanned() {
            return Some(Ground::Local);
        }
        let volume = crate::indexing::host::volumes::current().get(&context.volume_id)?;
        Some(Ground::ViaTrait {
            volume,
            pacer: ScanPacer::for_volume(context.volume_id.clone()),
        })
    }

    /// Let the backend open whatever a walk's worth of listings needs (SMB spins up
    /// a small pool of extra connections). Default no-op everywhere else.
    fn open_session(&self) {
        if let Ground::ViaTrait { volume, .. } = self {
            runtime::block_on(volume.begin_scan_session());
        }
    }

    /// Tear it back down, on every outcome. Paired with
    /// [`open_session`](Self::open_session) by the shape of `walk_frontier`.
    fn close_session(&self) {
        if let Ground::ViaTrait { volume, .. } = self {
            runtime::block_on(volume.end_scan_session());
        }
    }

    /// Whether `path` is a directory this walk may descend into, and what to record
    /// for it. `None` for anything else: gone, unreadable, or a symlink.
    pub(super) fn stat_directory(&self, path: &Path) -> Option<MetadataSnapshot> {
        match self {
            Ground::Local => {
                let metadata = std::fs::symlink_metadata(path).ok()?;
                // A symlink reports `is_dir() == false` here, which is the answer we
                // want: the index stores symlinks without descending into them.
                metadata.is_dir().then(|| extract_metadata(&metadata, true, false))
            }
            Ground::ViaTrait { volume, .. } => {
                let entry = runtime::block_on(stat_one_directory(Arc::clone(volume), path.to_path_buf()))?;
                Some(MetadataSnapshot {
                    // A directory's own row carries no size, on every walk here.
                    logical_size: None,
                    physical_size: None,
                    modified_at: entry.modified_at,
                    inode: entry.inode,
                    nlink: None,
                })
            }
        }
    }

    /// Cover one frontier root, and say how it went.
    fn cover(
        &self,
        context: &CoverContext,
        root: &Path,
        sender: &SyncSender<Vec<CoveredEntry>>,
        cancel: &CancellationToken,
        heartbeat: &WalkHeartbeat,
    ) -> (Option<ScanSummary>, RootOutcome) {
        match self {
            Ground::Local => {
                match cover_subtree(
                    root,
                    &context.space,
                    &context.writer,
                    Some(sender.clone()),
                    cancel,
                    heartbeat,
                ) {
                    Ok(summary) => (Some(summary), RootOutcome::Covered),
                    Err(ScanError::Cancelled(summary)) => (Some(summary), RootOutcome::Cancelled),
                    Err(ScanError::NotVirgin) => (None, repair_non_virgin(context, root, cancel)),
                    Err(e) => {
                        // One unwalkable root doesn't stop the others: it simply stays
                        // frontier, and the next search asks for it again.
                        log::warn!("Cover: couldn't walk {}: {e}", root.display());
                        (None, RootOutcome::Failed)
                    }
                }
            }
            Ground::ViaTrait { volume, pacer } => {
                // ❌ There is no `NotVirgin` arm here, and no repair path to route
                // one to: the trait walk is add-only per directory (it keeps the
                // rows a name already has), so ground an earlier walk touched is
                // simply walked. Over a network round trip the per-directory name
                // check is free; over a local `readdir` it wouldn't be, which is why
                // the two halves differ here and nowhere else.
                let result = runtime::block_on(cover_volume_subtree(
                    Arc::clone(volume),
                    root.to_path_buf(),
                    &context.space,
                    &context.writer,
                    Some(sender.clone()),
                    cancel,
                    pacer,
                    heartbeat,
                ));
                match result {
                    Ok(summary) => (Some(summary), RootOutcome::Covered),
                    Err(VolumeScanError::Cancelled(summary)) => (Some(summary), RootOutcome::Cancelled),
                    // The one classification that says something about the VOLUME
                    // rather than about this root. ⚠️ Narrower than "the root failed"
                    // on purpose: a `Timeout` is one wedged directory on a share that
                    // is otherwise answering, and an `EmptyRoot` is no health claim
                    // at all.
                    Err(e) if e.is_terminal_disconnect() => {
                        log::warn!(
                            "Cover: '{}' went away while walking {}: {e}; leaving the rest of the frontier for the next search",
                            context.volume_id,
                            root.display(),
                        );
                        (None, RootOutcome::VolumeGone)
                    }
                    Err(e) => {
                        log::warn!("Cover: couldn't walk {}: {e}", root.display());
                        (None, RootOutcome::Failed)
                    }
                }
            }
        }
    }
}

/// The repair path for a frontier node the index already holds rows under.
///
/// Rare — it takes a verification pass writing children under a directory nothing
/// listed — and unsafe for the parallel walker, whose fresh ids would collide.
/// The serial reconcile compares by name and writes only differences, so it can
/// take the case without deleting anything.
fn repair_non_virgin(context: &CoverContext, root: &Path, cancel: &CancellationToken) -> RootOutcome {
    let db_path = context.writer.db_path();
    let conn = match IndexStore::open_read_connection(&db_path) {
        Ok(conn) => conn,
        Err(e) => {
            log::warn!("Cover: couldn't open a connection to repair {}: {e}", root.display());
            return RootOutcome::Failed;
        }
    };
    match crate::indexing::reconcile::reconciler::reconcile_subtree(
        root,
        &context.space,
        &conn,
        &context.writer,
        cancel,
    ) {
        Ok(summary) => {
            log::debug!(
                "Cover: repaired {} through the serial reconcile (+{} -{} ~{})",
                root.display(),
                summary.added,
                summary.removed,
                summary.updated,
            );
            if summary.cancelled {
                RootOutcome::Cancelled
            } else {
                RootOutcome::Covered
            }
        }
        Err(e) => {
            log::warn!("Cover: couldn't repair {}: {e}", root.display());
            RootOutcome::Failed
        }
    }
}

/// How one frontier root's walk ended, whichever primitive took it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RootOutcome {
    /// The node is covered now.
    Covered,
    /// Someone stopped the walk partway. Whatever it listed is still marked, and
    /// the rest of the frontier is left for the next search.
    Cancelled,
    /// It couldn't run. The node stays frontier and the next search asks again.
    Failed,
    /// The VOLUME is gone, and this root is only where the walk found out. Every
    /// root behind it is on the same volume and the same session, so the frontier
    /// loop stops rather than re-asking a question that can't be answered.
    ///
    /// ⚠️ [`Failed`](Self::Failed) for this root plus a verdict about the rest, and
    /// that is the whole of it: a root the loop skips is walked by nothing, so it is
    /// marked by nothing and stays frontier. ❌ Never write, mark, or count anything
    /// for a skipped root — that would turn "the NAS is asleep" into thousands of
    /// folders written out of search. `DETAILS.md` § "A dead volume is concluded
    /// once, not per root".
    VolumeGone,
}

mod bootstrap;
mod live;

pub(crate) use bootstrap::{NoCoverContext, context_for_walk};
/// Wider than the rest of the table's vocabulary because it is a parameter of
/// [`start`], which the handle calls.
pub(crate) use live::WalkFor;
#[cfg(test)]
pub(in crate::indexing) use live::somebody_is_asking_for_ground;
pub(in crate::indexing) use live::{
    Claim, Holder, Mode, a_rescan_can_start, forget_rescan, ground_being_walked, remember_rescan, take_rescan,
    walk_pulse,
};

#[cfg(test)]
mod bench;
#[cfg(test)]
mod cold_drive_tests;
#[cfg(test)]
mod network_give_up_tests;
#[cfg(test)]
mod network_tests;
#[cfg(test)]
mod test_support;
#[cfg(test)]
mod tests;
