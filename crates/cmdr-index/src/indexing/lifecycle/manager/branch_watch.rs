//! Starting the watcher over the branches a walk covered, in three steps so the
//! slow one runs with no lock held.
//!
//! ⚠️ **The middle step blocks on `fseventsd`**: a stream start plus
//! `FSEventsGetCurrentEventId`, each a round trip to the one daemon every process
//! shares, and a read of the database. Under load one start took 5.8 s in the live
//! app (2026-10-06), and run under `INDEX_REGISTRY` (as `ensure_branch_watch` once
//! did, inside `with_running_manager`) it stalled `get_status` and every other
//! registry user, for every volume, for that long. So:
//!
//! 1. [`IndexManager::plan_branch_watch`] decides under the lock, from in-memory
//!    facts only, and marks the start as in flight;
//! 2. [`BranchWatchStart::start`] does the slow part with no lock held;
//! 3. [`IndexManager::install_branch_watch`] publishes the watcher under the lock,
//!    or hands it back when the volume moved on in between.
//!
//! The registry door is `state::ensure_branch_watch`; a caller that owns its
//! manager off the lock runs all three in one go ([`IndexManager::ensure_branch_watch`]).

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use super::IndexManager;
use crate::indexing::events::DEBUG_STATS;
use crate::indexing::hold::HoldKind;
use crate::indexing::lifecycle::master;
use crate::indexing::reconcile::reconciler::EventReconciler;
use crate::indexing::store::IndexStore;
use crate::indexing::watch::branches::{self, BranchWatch, WatchScope};
use crate::indexing::watch::event_loop::{JOURNAL_GAP_THRESHOLD, LiveConfig, run_live_event_loop};
use crate::indexing::watch::watcher::{self, DriveWatcher, FsChangeEvent};
use crate::indexing::writer::{IndexWriter, WriteMessage};
use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::pluralize::pluralize;

/// A manager's "a branch watch start is in flight" mark, cleared when the start
/// ends however it ends: installed, refused, failed, or unwound.
///
/// It also names WHICH manager the start is for. A stop and a fresh start can both
/// land while the stream starts, and the new manager carries a mark of its own, so
/// a watcher started for the old one can't be installed into it.
pub(super) struct InFlight(Arc<AtomicBool>);

impl Drop for InFlight {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

/// A branch watch [`IndexManager::plan_branch_watch`] decided to start, holding
/// everything the slow part needs so it never reaches back into the manager.
pub(in crate::indexing) struct BranchWatchStart {
    in_flight: InFlight,
    volume_id: String,
    volume_root: PathBuf,
    db_path: PathBuf,
    writer: IndexWriter,
    branches: Arc<BranchWatch>,
    paths: Vec<PathBuf>,
    resuming: bool,
}

/// A watcher [`BranchWatchStart::start`] got running, waiting to be installed.
pub(in crate::indexing) struct StartedBranchWatch {
    in_flight: InFlight,
    watcher: DriveWatcher,
    event_rx: tokio::sync::mpsc::UnboundedReceiver<FsChangeEvent>,
    since_event_id: u64,
    branches: Arc<BranchWatch>,
    paths: Vec<PathBuf>,
}

impl StartedBranchWatch {
    /// The running watcher, for a caller with no manager to install it into. It's
    /// still running: stop it, off the lock.
    #[must_use = "the watcher is still running until somebody stops it"]
    pub(in crate::indexing) fn into_watcher(self) -> DriveWatcher {
        self.watcher
    }
}

impl BranchWatchStart {
    /// Do everything slow about starting the watch: the per-drive veto (a database
    /// read), where to replay from (a database read and an `fseventsd` round trip),
    /// and the stream itself (another). ❌ Never under the registry lock.
    pub(in crate::indexing) fn start(self) -> Option<StartedBranchWatch> {
        if !master::branch_watch_allowed(master::master_enabled(), &self.db_path) {
            log::info!(
                "Branch watch: '{}' walked ground stays covered but unwatched; indexing is off for this drive",
                self.volume_id
            );
            return None;
        }
        // Replay from where this volume's stream last left off, so a branch covered
        // in an earlier session comes back current rather than as a snapshot of
        // whenever the app last ran. A gap too wide to be worth replaying takes the
        // same exit a cold start does: watch from now, and let the epoch bump say
        // the rows are stale rather than current.
        let since_event_id = replayable_event_id(&self.db_path, &self.volume_root);
        if self.resuming && since_event_id == 0 {
            // Nothing to replay, so this session can't know what happened to the
            // covered ground while the app was off. The rows stay trusted (Decision
            // 5: a covered-but-stale subtree is not re-walked) and the epoch bump is
            // what makes the read side RENDER them as stale rather than current.
            // Only on a resume: a bump right after a walk would mark rows stale that
            // were written a second ago.
            let _ = self.writer.send(WriteMessage::BumpCurrentEpoch);
        }
        let (event_tx, event_rx) = tokio::sync::mpsc::unbounded_channel();
        match DriveWatcher::start_branches(&self.volume_root, &self.paths, since_event_id, event_tx) {
            Ok(watcher) => Some(StartedBranchWatch {
                in_flight: self.in_flight,
                watcher,
                event_rx,
                since_event_id,
                branches: self.branches,
                paths: self.paths,
            }),
            Err(e) => {
                // Non-fatal, and honest: the coverage stays served, it just stops
                // being kept current.
                log::warn!("Branch watch: '{}' couldn't start a watcher: {e}", self.volume_id);
                None
            }
        }
    }
}

impl IndexManager {
    /// Decide whether to watch the branches a walk covered on this volume, and
    /// claim the start if so. Cheap and non-blocking, so it's safe under the
    /// registry lock; the start itself is [`BranchWatchStart::start`].
    ///
    /// This is the whole of what makes walk-written coverage keep its promise:
    /// without it a walked branch is a snapshot of a folder taken once, and the
    /// plan would need the expiry Decision 9 replaced. It runs on a volume whose
    /// index a SEARCH built (`Activation::WriterOnly`) — the one shape that has
    /// coverage and no watcher. A scanned volume already has one over everything,
    /// and starting a second would give one database two live loops.
    ///
    /// Four things have to be true, and each is a decision rather than a
    /// precaution:
    ///
    /// - the volume is read by the LOCAL walker, since this is a local-filesystem
    ///   watcher; a share or a phone is watched (or not) by its own transport, and
    ///   its walked branches are exactly as stale as its scanned index, which
    ///   loads Stale on every launch anyway;
    /// - nothing is watching it yet, and no start is already in flight;
    /// - something is actually covered;
    /// - both indexing switches allow it (`master::branch_watch_allowed`) — this
    ///   is where the per-drive veto gets its teeth. The master switch is asked
    ///   here and again by the start; the veto, a database read, only there.
    pub(in crate::indexing) fn plan_branch_watch(&mut self, resuming: bool) -> Option<BranchWatchStart> {
        if !self.kind.uses_local_scanner() || self.drive_watcher.is_some() || !master::master_enabled() {
            return None;
        }
        let branches = branches::live_for(&self.volume_id);
        if branches.is_empty() {
            return None;
        }
        if self.branch_watch_starting.swap(true, Ordering::AcqRel) {
            return None;
        }
        Some(BranchWatchStart {
            in_flight: InFlight(Arc::clone(&self.branch_watch_starting)),
            volume_id: self.volume_id.clone(),
            volume_root: self.volume_root.clone(),
            db_path: self.db_path().to_path_buf(),
            writer: self.writer.clone(),
            paths: branches.branch_paths().into_iter().map(PathBuf::from).collect(),
            branches,
            resuming,
        })
    }

    /// Put a started branch watch to work, or hand its watcher back for the caller
    /// to stop, off the lock, when this manager isn't the one it was started for
    /// or a watcher came up here in the meantime (a scan start, say). Non-blocking:
    /// a spawn and some bookkeeping.
    #[must_use = "a watcher handed back is still running until somebody stops it"]
    pub(in crate::indexing) fn install_branch_watch(&mut self, started: StartedBranchWatch) -> Option<DriveWatcher> {
        if !Arc::ptr_eq(&started.in_flight.0, &self.branch_watch_starting) || self.drive_watcher.is_some() {
            return Some(started.watcher);
        }
        let StartedBranchWatch {
            in_flight,
            watcher,
            event_rx,
            since_event_id,
            branches,
            paths,
        } = started;

        let watcher_overflow = Some(watcher.overflow_flag());
        // A branch a walk began while the stream was starting isn't in `paths`. A
        // watcher that watches branch by branch (inotify) has to be told about it;
        // one that watches the volume root already carries it.
        for path in branches.branch_paths() {
            if !paths.iter().any(|started_on| *started_on == Path::new(&path)) {
                watcher.watch_branch(Path::new(&path));
            }
        }
        self.drive_watcher = Some(watcher);
        self.branch_watched = true;
        drop(in_flight);
        DEBUG_STATS.watcher_active.store(true, Ordering::Relaxed);
        log::info!(
            "Branch watch: '{}' is watching {} (since_event_id={since_event_id})",
            self.volume_id,
            pluralize(paths.len() as u64, "walked branch"),
        );

        let space = self.path_space();
        let scope = WatchScope::Branches(Arc::clone(&branches));
        let mut reconciler = EventReconciler::new_for(
            self.volume_id.clone(),
            space.clone(),
            self.work.child(HoldKind::LiveLoop),
        );
        reconciler.within(scope.clone());
        // Live from the first event: there is no scan to wait for, and the branches
        // that ARE being walked buffer on their own (`WatchScope`).
        reconciler.switch_to_live();

        let writer = self.writer.clone();
        let events = Arc::clone(&self.events);
        let volume_id = self.volume_id.clone();
        let handle = crate::indexing::host::runtime::spawn(async move {
            run_live_event_loop(
                event_rx,
                reconciler,
                writer,
                events,
                LiveConfig {
                    volume_id,
                    space,
                    watcher_overflow,
                    scope,
                },
            )
            .await;
        });
        let mut guard = self.live_event_task.lock_ignore_poison();
        if let Some(previous) = guard.replace(handle) {
            previous.abort();
        }
        None
    }

    /// Plan, start, and install a branch watch in one go, for a caller that holds
    /// this manager OFF the registry (a detached scan start, a test). ⚠️ Blocks on
    /// `fseventsd`: ❌ never call it with the registry held, which means never
    /// from a `with_running_manager` closure. Use `state::ensure_branch_watch`.
    pub(in crate::indexing) fn ensure_branch_watch(&mut self, resuming: bool) {
        let Some(started) = self.plan_branch_watch(resuming).and_then(BranchWatchStart::start) else {
            return;
        };
        if let Some(mut stray) = self.install_branch_watch(started) {
            stray.stop();
        }
    }

    /// A search walk is about to cover `paths` on this volume.
    ///
    /// Registering them BEFORE the walk reads anything is the whole point: from
    /// here their events wait for the walk instead of racing it. It runs on every
    /// volume with a live loop, not only a branch-watched one — a walk over a hole
    /// in an indexed drive races that drive's loop identically.
    ///
    /// Answers the branch watch to start, if the volume has none yet; the caller
    /// starts it off the lock.
    pub(in crate::indexing) fn begin_branch_coverage(&mut self, paths: &[String]) -> Option<BranchWatchStart> {
        let branches = branches::live_for(&self.volume_id);
        let added = branches.begin_covering(paths);
        // A watcher that watches branch by branch (inotify) has to be told about
        // each new one; one that watches the volume root already carries them.
        if self.branch_watched
            && let Some(watcher) = self.drive_watcher.as_ref()
        {
            for path in &added {
                watcher.watch_branch(Path::new(path));
            }
        }
        self.plan_branch_watch(false)
    }
}

/// The event id a (re)started watcher may replay from: the stored one, unless the
/// journal has moved so far past it that replaying costs more than it's worth. `0`
/// means "from now".
///
/// Same threshold the cold-start replay uses, for the same reason, and the same
/// consolation: nothing is lost that the index claimed to know, because a gap this
/// wide leaves the covered rows stale-but-trusted (Decision 5) rather than
/// wrong-and-confident.
fn replayable_event_id(db_path: &Path, volume_root: &Path) -> u64 {
    if !watcher::supports_event_replay() {
        return 0;
    }
    let stored = IndexStore::open_read_connection(db_path)
        .ok()
        .and_then(|conn| IndexStore::get_meta(&conn, "last_event_id").ok().flatten())
        .and_then(|id| id.parse::<u64>().ok())
        .unwrap_or(0);
    let current = watcher::current_event_id(volume_root);
    if stored == 0 || (current > 0 && current > stored + JOURNAL_GAP_THRESHOLD) {
        return 0;
    }
    stored
}
