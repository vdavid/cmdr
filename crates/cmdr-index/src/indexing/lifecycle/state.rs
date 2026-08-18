//! Indexing state machine and the per-volume registry.
//!
//! Holds the `INDEX_REGISTRY` (one `IndexInstance` per volume id) and the
//! `IndexPhase` enum that gates every public operation for a volume. The jobs that
//! act ON the registry live in submodules, and this file re-exports them, so
//! `state::<anything>` stays the one path callers use:
//!
//! - [`auto_start`]: the pure launch-policy predicates.
//! - [`reservation`]: the lock-first `(absent) -> Initializing` check-and-set.
//! - [`startup`]: `start_indexing_for`, the choke point every transport starts
//!   through, plus the per-transport entry points. [`walk_database`]: what a
//!   `WriterOnly` start does to the database a search walk will fill.
//! - [`teardown`]: stop / forget / reset, and the sweep over every volume.
//! - [`scan_control`]: force a rescan, stop one, trigger verification.
//! - [`queries`]: the read-only question surface.
//! - [`freshness_bridge`]: registry ↔ `lifecycle/freshness.rs` wiring + epoch bumps.
//! - [`supervisor`]: the fatal-storage-error watch that flips a volume to `Failed`.
//!
//! ## Registry shape
//!
//! Each indexed volume has one `IndexInstance` bundling its `{phase, kind,
//! signals}`. The registry is the authority for *which* volumes are indexed and
//! for their lifecycle transitions. Every invariant the single-volume design
//! held — single-writer per DB, lock-first reservation, drop-guard-before-drain,
//! reads via `ReadPool` never under the lifecycle lock — now holds *per volume
//! id*, keyed independently in the map so two volumes can't corrupt each other.
//!
//! A volume's read handles (`ReadPool`, `PendingSizes`) are NOT in its instance.
//! `reservation` builds them, then PUSHES them into the volume-keyed tables in
//! `read/handles.rs` as it reserves the slot, and every teardown path withdraws
//! them. That's what keeps the read path — the hottest path in the subsystem —
//! from having to reach back into this registry. See `read/handles.rs` and the
//! `DETAILS.md` registry section.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::LazyLock;
use std::sync::atomic::Ordering;
use tokio_util::sync::CancellationToken;

use super::freshness::Freshness;
use super::manager::IndexManager;
use crate::indexing::store::{IndexFailure, IndexStore};
use crate::indexing::volume::{IndexVolumeKind, VolumeId};
use crate::indexing::watch::branches::{self, AfterWalk};

mod auto_start;
mod freshness_bridge;
mod queries;
mod reservation;
mod scan_control;
mod startup;
mod supervisor;
mod teardown;
mod walk_database;

pub use auto_start::{should_auto_start, should_auto_start_indexing};
pub(crate) use freshness_bridge::{
    apply_freshness_event, apply_freshness_event_on, bump_current_epoch_for, get_freshness,
};
#[cfg(test)]
pub(crate) use queries::is_watching_for_test;
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(crate) use queries::registered_mtp_volume_ids_for_device;
pub(crate) use queries::{
    all_registered_volume_ids, awaits_its_first_scan, index_failure, ready_volumes_with_kind, volume_kind,
};
pub use queries::{is_active, is_failed};
#[cfg(any(test, feature = "testing"))]
pub use reservation::reserve_initializing_index_for_test;
pub(crate) use reservation::{is_initializing_phase, try_reserve_initializing_phase};
pub(in crate::indexing::lifecycle) use scan_control::{Handover, off_the_registry, resume_the_phases};
pub use scan_control::{force_scan, stop_scan, trigger_verification};
#[cfg(test)]
pub(crate) use scan_control::{rescan_with_phases_owed_for_test, set_scanning_for_test, while_detached_for_test};
pub(crate) use startup::record_drive_index_enabled;
pub use startup::start_indexing;
pub(in crate::indexing::lifecycle) use startup::{Activation, start_indexing_for};
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(crate) use startup::{
    start_indexing_for_local_external_inner, start_indexing_for_mtp_inner, start_indexing_for_smb_inner,
};
#[cfg(test)]
pub(crate) use supervisor::fail_index_for_test;
pub(crate) use supervisor::spawn_failure_supervisor;
pub(crate) use teardown::reset_to_not_indexed;
pub(crate) use teardown::stop_all_indexing;
pub use teardown::{clear_every_index, clear_index, disable_drive_index_persist_intent, stop_indexing};

// ── Indexing state machine ────────────────────────────────────────────

/// Lifecycle phases of one volume's index. Single source of truth for whether
/// that volume's index is active and what capabilities are available.
///
/// There is no `Disabled` variant: in the registry model, "disabled / not
/// indexed" is the *absence* of an `IndexInstance` for the volume id. An
/// instance only ever exists in one of these live-or-transitional phases, so
/// the read-path gate (`get_read_pool_for` returning `None`) and `get_status`
/// treat an absent key as disabled.
pub(crate) enum IndexPhase {
    /// IndexManager created, `resume_or_scan()` is running. A temporary read
    /// store is available for enrichment and status queries while initialization
    /// completes.
    Initializing { store: IndexStore },
    /// Fully operational: scanning, watching, enrichment, IPC all work.
    Running(Box<IndexManager>),
    /// The manager is momentarily OUT of the registry while a scan start runs its
    /// blocking prelude against it off the lock (`state::off_the_registry`). The
    /// volume is still fully alive: its writer thread, watcher, and read handles
    /// are all up, and the manager comes back within milliseconds.
    ///
    /// ⚠️ **Distinct from [`ShuttingDown`](IndexPhase::ShuttingDown), and that is
    /// the whole point.** One transient state served both, so a teardown landing
    /// in this window read it as "somebody is already tearing this volume down",
    /// reported success, and did nothing — including `fail_index`, which left a
    /// volume running over a dead writer for the rest of the session. A teardown
    /// that meets this phase CLAIMS it instead (`teardown`), and whoever hands the
    /// manager back carries the request out.
    ///
    /// Readers answer off it as what it is — a volume whose scan is starting — so
    /// nothing has to pretend the volume went away for the length of a rescan.
    Detached {
        /// This volume's writer, cloned off the manager as it left. Fixed for the
        /// manager's whole life (nothing ever reassigns `IndexManager::writer`),
        /// so this copy cannot drift from the one the manager holds. It's what
        /// lets an SMB or MTP change land in the buffer instead of the floor while
        /// the manager is away.
        writer: crate::indexing::writer::IndexWriter,
        /// What a teardown asked for while the manager was out, if anything.
        teardown: Option<TeardownClaim>,
    },
    /// Shutdown in progress (transitional, cleanup running). The instance is
    /// removed from the registry once the drain completes.
    ShuttingDown,
    /// The index DB died with a FATAL storage error (`SQLITE_IOERR`, corruption, a
    /// full or read-only disk, …), so indexing STOPPED for this volume. Unlike
    /// every other non-running state, a `Failed` instance STAYS registered — that
    /// is what lets the badge render a distinct "indexing stopped" state instead of
    /// the gray "disabled = no key" one. Its manager, writer thread, and watcher are
    /// torn down; its read-path handles are uninstalled (so reads skip cleanly, no
    /// per-navigation flood on a dead DB).
    ///
    /// Carries the typed [`IndexFailure`] reason (for the status surface and logs)
    /// and the DB path so recovery can reclaim the file without re-resolving it.
    /// Recovery is a rebuild-from-scratch: `clear_index` removes this instance and
    /// deletes the DB, then a fresh `start_indexing` scans. See `fail_index` and
    /// DETAILS § "The Failed state".
    Failed { reason: IndexFailure, db_path: PathBuf },
}

/// What a teardown asked for while a volume's manager was detached, for whoever
/// hands the manager back to carry out.
///
/// Recording the request is what makes it survive the window. ❌ Never a bare
/// bool: the three teardowns end the volume in three different places (removed,
/// removed with its database gone, registered as `Failed`), and a caller that
/// only knew "somebody asked" would have to guess which.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TeardownClaim {
    /// The writer reported a fatal storage error: register `Failed` and stop.
    Failed(IndexFailure),
    /// Stop indexing this volume, keeping its database on disk.
    Stopped,
    /// Stop indexing it and delete its database.
    Cleared,
}

impl TeardownClaim {
    /// How much of the volume this claim takes away, so two teardowns landing in
    /// one window resolve to the one that does more rather than to whichever
    /// arrived last.
    ///
    /// A user asking to stop or clear outranks a storage failure: they asked for
    /// the drive to go quiet, and a red "indexing stopped" badge on a drive
    /// somebody just turned off is a worse answer than a gray one. Clearing
    /// outranks stopping because it IS stopping, plus the database.
    fn reach(self) -> u8 {
        match self {
            TeardownClaim::Failed(_) => 0,
            TeardownClaim::Stopped => 1,
            TeardownClaim::Cleared => 2,
        }
    }
}

impl IndexPhase {
    /// Ask a phase to CARRY a teardown request instead of bouncing it, reporting
    /// whether it took it.
    ///
    /// Only [`Detached`](IndexPhase::Detached) can: it's the one phase whose
    /// manager is coming back, so it's the one phase with somebody to hand the
    /// request to. Every other phase answers `false` and the caller acts on it
    /// directly, exactly as it always did.
    pub(super) fn claim_the_teardown(&mut self, claim: TeardownClaim) -> bool {
        let IndexPhase::Detached { teardown, .. } = self else {
            return false;
        };
        if teardown.is_none_or(|held| held.reach() < claim.reach()) {
            *teardown = Some(claim);
        }
        true
    }
}

/// The three handles a volume's registry instance and its `IndexManager` both
/// hold: its freshness signal, where its reports go, and its stop signal. Passed
/// as ONE value so the two can never be built with different halves — a manager
/// firing freshness through a different `Arc`, or cancelling a token nothing
/// else watches, is exactly the class of bug this shape rules out.
#[derive(Clone)]
pub(crate) struct VolumeSignals {
    /// This volume's freshness signal (gray = absent instance; blue/green/yellow
    /// = the `Freshness` variants). `Arc<Mutex<…>>` so scan-transition tasks and
    /// the live-watch layer can flip it without holding the registry lock.
    /// `None` means "not yet determined" (e.g. mid-initialization before the
    /// first scan transition); a `Running` volume always carries `Some`. The
    /// state machine itself lives in `lifecycle/freshness.rs`. See DETAILS §
    /// "The freshness model".
    pub(crate) freshness: Arc<std::sync::Mutex<Option<Freshness>>>,
    /// Where this volume's reports go. Held per volume rather than in a
    /// process-wide slot so the handle-free seams (the freshness transition, the
    /// failure supervisor) stay per-volume, like every other invariant here.
    pub(crate) events: Arc<dyn crate::EventSink>,
    /// This volume's stop signal — the ROOT of every cancellation under it.
    /// Every long walk it starts (a full scan, a reconcile, a subtree rescan, a
    /// verification) runs on a `child_token()`, so tearing the volume down stops
    /// all of them at once.
    ///
    /// ❌ Nothing below `lifecycle` looks this up by volume id. Whoever starts the
    /// work is handed a child token by the layer that owns this one (the manager,
    /// or `trigger_verification` while it already holds the instance). A late
    /// lookup would answer `None` for a volume that just went away and hand the
    /// walk a token that never fires — precisely the walk that needs to stop.
    pub(crate) cancel: CancellationToken,
}

impl VolumeSignals {
    /// Build a volume's shared handles: a fresh stop signal plus the caller's
    /// freshness and sink.
    pub(crate) fn new(freshness: Arc<std::sync::Mutex<Option<Freshness>>>, events: Arc<dyn crate::EventSink>) -> Self {
        Self {
            freshness,
            events,
            cancel: CancellationToken::new(),
        }
    }
}

/// One volume's index: its lifecycle phase, its scan kind, and the handles it
/// shares with its `IndexManager`.
///
/// ❌ The read-path handles (`ReadPool`, `PendingSizes`) are deliberately NOT
/// here. They live in the volume-keyed tables in `read/handles.rs`, which
/// lifecycle pushes into; putting a copy back would let the two disagree and
/// would re-create the read-path-depends-on-lifecycle cycle. See `DETAILS.md`
/// § "Where a volume's read handles live".
pub(crate) struct IndexInstance {
    pub(crate) phase: IndexPhase,
    /// This volume's scan kind (Local / SMB / MTP). Retained so a consumer of the
    /// registry (the importance scheduler's startup sweep) can branch typed on the
    /// kind — score Local + SMB, exclude MTP — instead of re-deriving it from the
    /// volume-id string.
    pub(crate) kind: IndexVolumeKind,
    /// The handles this volume shares with its `IndexManager`.
    pub(crate) signals: VolumeSignals,
}

/// The per-volume index registry: the authority for which volumes are indexed
/// and their lifecycle. Keyed by volume id so each volume's `(absent) ->
/// Initializing -> Running` machine is independent and two volumes can't race
/// on each other's state.
///
/// An *absent* key means "no index registered for this volume" — the read path
/// uses exactly that to decide skip-vs-route (`get_read_pool_for` returns
/// `None`, so enrichment skips before any DB work).
pub(crate) static INDEX_REGISTRY: LazyLock<std::sync::Mutex<HashMap<VolumeId, IndexInstance>>> =
    LazyLock::new(|| std::sync::Mutex::new(HashMap::new()));

// ── Initialization ───────────────────────────────────────────────────

/// Force-initialize the registry static. Called during app setup so the
/// `LazyLock` is ready before any async tasks access it.
///
/// The subsystem holds no `AppHandle`: what it needs from the host arrives
/// through the seams in `../host/`, so there is nothing to stash here.
pub fn init() {
    drop(INDEX_REGISTRY.lock());
    log::debug!("Indexing registry initialized");
}

/// The on-disk path of a volume's index DB (`index-<volume_id>.db` under the
/// resolved app data dir). Single-sources the filename format shared by the
/// indexer's open path and the on-connect resume probe.
pub(crate) fn resolved_index_db_path(volume_id: &str) -> Result<PathBuf, String> {
    let data_dir = crate::indexing::host::config::data_dir().map_err(|e| e.to_string())?;
    Ok(data_dir.join(format!("index-{volume_id}.db")))
}

// ── Registry helpers ─────────────────────────────────────────────────

/// Clone a volume's writer handle (and read whether a full scan is in progress)
/// if it has a `Running` index. Used by the SMB watch→index translator to
/// enqueue change messages (`UpsertEntryV2` / `DeleteEntryById` / …) onto the
/// single per-volume writer thread, preserving the single-writer-per-DB
/// invariant: the translator never writes directly. The `scanning` flag lets the
/// translator BUFFER changes during a full (re)scan and replay them after, so a
/// change to an already-walked directory isn't lost against the mid-scan
/// (truncated, rebuilding) index — the SMB equivalent of the local
/// arm-watcher-before-snapshot + reconcile flow.
///
/// `None` while the volume is `Initializing` (its scan owns the writer) or
/// absent.
///
/// A [`Detached`](IndexPhase::Detached) volume answers with its writer and
/// `scanning: true`, so the change is BUFFERED. Both halves matter: the phase
/// exists because a scan is being started, so `true` is the honest answer, and
/// buffering is what stops the change going on the floor the way it did when this
/// window read as no index at all. ❌ Don't "improve" it to the manager's real
/// `scanning` flag — that flag flips true partway through `start_scan`, so a live
/// apply in the gap would insert rows the `TruncateData` a few lines later blanks,
/// or worse, rows that land after it with ids the walk is about to allocate.
pub(crate) fn get_writer_and_scanning_for(volume_id: &str) -> Option<(crate::indexing::writer::IndexWriter, bool)> {
    let reg = INDEX_REGISTRY.lock().ok()?;
    match reg.get(volume_id).map(|i| &i.phase) {
        Some(IndexPhase::Running(mgr)) => Some((mgr.writer.clone(), mgr.scanning.load(Ordering::Relaxed))),
        Some(IndexPhase::Detached { writer, .. }) => Some((writer.clone(), true)),
        _ => None,
    }
}

/// Everything a coverage walk needs to run on a volume: its writer (the one
/// writer per DB) and its path space.
///
/// The walk REUSES the volume's running writer rather than standing a second one
/// up, which is the whole point: two writers on one database own separate id
/// counters and separate accumulator maps, so they collide on the primary key and
/// inflate `dir_stats`.
///
/// `None` in three cases, and a caller that gets it should walk nothing:
/// - the volume is `Initializing`, so its own start owns the writer;
/// - a full scan is RUNNING on it, which already covers everything a search would
///   have walked, and whose fresh ids would collide with the walk's over the same
///   names (`INSERT OR IGNORE` drops one and orphans its subtree);
/// - the volume has no index at all, which is the cold-bootstrap case
///   (`cover/bootstrap.rs` builds one).
///
/// A [`Detached`](IndexPhase::Detached) volume is the second case seen a
/// millisecond earlier — a scan is being started on it — so it answers `None`
/// too. The claim table would refuse the walk one line later anyway
/// (`IndexManager::claim_the_volume` takes the volume `Exclusive`ly before every
/// blocking call in `start_scan`), so this is belt over braces rather than the
/// protection itself.
pub(crate) fn cover_context_for(volume_id: &str) -> Option<crate::indexing::lifecycle::cover::CoverContext> {
    let reg = INDEX_REGISTRY.lock().ok()?;
    match reg.get(volume_id).map(|i| &i.phase) {
        Some(IndexPhase::Running(mgr)) if !mgr.scanning.load(Ordering::Relaxed) => {
            Some(crate::indexing::lifecycle::cover::CoverContext {
                volume_id: volume_id.to_string(),
                writer: mgr.writer.clone(),
                space: mgr.path_space(),
                // Taken off the SAME instance the writer came from: it decides
                // whether the walk reads a disk or a `Volume`, and a kind resolved
                // anywhere else could name a different volume than the writer does.
                kind: mgr.kind,
                flush: crate::indexing::lifecycle::cover::FlushOnFinish::default(),
            })
        }
        _ => None,
    }
}

/// Tell a volume's watcher that a search walk is about to cover `paths`.
///
/// Called on the thread that starts the walk, BEFORE it reads anything, so an
/// event that lands in the covered ground waits for the walk instead of racing
/// it. A volume with no live index (a vetoed drive, a share) has nothing to tell,
/// and the walk runs exactly as it did.
pub(crate) fn begin_branch_coverage(volume_id: &str, paths: &[String]) {
    with_running_manager(volume_id, |mgr| mgr.begin_branch_coverage(paths));
}

/// Tell it the walk ended, so what it held is released and what it covered
/// becomes ground the volume keeps current.
///
/// ❌ The release itself is NOT routed through the running manager. A walk ends
/// minutes after it began, and `force_scan` / `perform_registry_rescan` publish
/// `ShuttingDown` for the whole of a scan start — a finish that no-opped in that
/// window would leave the branch at `walks > 0` for the rest of the session:
/// `may_walk` false for that ground permanently, every event for it buffered and
/// never promoted, and the branch never absorbed. The set lives outside the
/// registry (`branches::live_for`) precisely so this can't depend on a phase.
pub(crate) fn finish_branch_coverage(volume_id: &str, paths: &[String]) {
    let branches = branches::live_for(volume_id);
    // What the volume wants remembered is still the manager's answer, when it has
    // one. A volume whose manager is momentarily out of the registry keeps nothing
    // written down: the rescan that took it out retires the branch set anyway.
    let mut after = AfterWalk::Forget;
    let mut record = None;
    with_running_manager(volume_id, |mgr| (after, record) = mgr.after_walk());
    branches.finish_covering(paths, after);
    // Off the registry lock: `persist` hands a meta row to the writer thread, and
    // nothing under this lock may block.
    if let Some((space, writer)) = record {
        branches.persist(&space, &writer);
    }
}

/// Whether any of the ground a walk is about to release is holding live events
/// for it. A walk that left its drain to the caller asks this before releasing:
/// the release replays them, and they resolve against committed rows or not at
/// all.
pub(crate) fn branch_coverage_buffered_events(volume_id: &str, paths: &[String]) -> bool {
    branches::live_for(volume_id).any_buffered(paths)
}

/// Run something against a volume's `Running` manager, or nothing if it has
/// none. Non-blocking work only — the registry lock is held throughout.
fn with_running_manager(volume_id: &str, f: impl FnOnce(&mut IndexManager)) {
    let Ok(mut reg) = INDEX_REGISTRY.lock() else {
        return;
    };
    if let Some(IndexPhase::Running(mgr)) = reg.get_mut(volume_id).map(|i| &mut i.phase) {
        f(mgr);
    }
}

#[cfg(test)]
mod tests;
