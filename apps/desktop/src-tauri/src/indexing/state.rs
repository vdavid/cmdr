//! Indexing state machine and the per-volume registry.
//!
//! Holds the `INDEX_REGISTRY` (one `IndexInstance` per volume id) and the
//! `IndexPhase` enum that gates every public operation for a volume. Also owns
//! the bootstrap logic that spins up the `IndexManager`, the `ReadPool`, and the
//! incremental-vacuum timer.
//!
//! ## Registry shape (M1)
//!
//! Each indexed volume has one `IndexInstance` bundling its `{phase, read_pool,
//! pending_sizes}`. The registry is the authority for *which* volumes are
//! indexed and for their lifecycle transitions. Every invariant the
//! single-volume design held — single-writer per DB, lock-first reservation,
//! drop-guard-before-drain, reads via `ReadPool` never under the lifecycle
//! lock — now holds *per volume id*, keyed independently in the map so two
//! volumes can't corrupt each other.
//!
//! The root volume's `ReadPool` and `PendingSizes` are *also* reachable through
//! the standalone `READ_POOL` / `PENDING_SIZES` module globals (the read-path
//! fast handles used by enrichment, search, and IPC dir-stats). The root
//! `IndexInstance` shares the very same `Arc`s, so there is exactly one
//! allocation per volume and the two can't drift. See `enrichment.rs` /
//! `pending_sizes.rs` and the `DETAILS.md` registry section.
//!
//! `mod.rs` is a thin facade that re-exports the public functions defined
//! here; module-internal callers (e.g. `manager.rs`) can use the items
//! directly via `super::state`.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::LazyLock;
use std::sync::atomic::Ordering;
use std::time::Duration;

use tauri::AppHandle;

use super::enrichment::{ReadPool, get_read_pool_for, install_read_pool, uninstall_read_pool};
use super::events::{DEBUG_STATS, IndexDebugStatusResponse, IndexStatusResponse, VolumeIndexStatus};
use super::firmlinks;
use super::freshness::{Freshness, FreshnessEvent};
use super::manager::IndexManager;
use super::pending_sizes::{PendingSizes, get_pending_sizes_for, install_pending_sizes, uninstall_pending_sizes};
use super::store::{self, DirStats, IndexStore};
use super::verifier;
use super::writer::WriteMessage;

use crate::settings::FullDiskAccessChoice;

/// A volume's identity in the index registry (e.g. `"root"` for the local disk).
pub(crate) type VolumeId = String;

/// The local-disk volume id. The only volume ever registered in M1.
pub(crate) const ROOT_VOLUME_ID: &str = "root";

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
    /// Shutdown in progress (transitional, cleanup running). The instance is
    /// removed from the registry once the drain completes.
    ShuttingDown,
}

/// One volume's index: its lifecycle phase plus the read-path handles
/// (`ReadPool` for lock-free enrichment/verification reads, `PendingSizes` for
/// the "size updating" hourglass). Bundling them per volume means a second
/// volume's pool can never be confused for this one's.
///
/// For the root volume, `read_pool` and `pending_sizes` are the same `Arc`s
/// installed into the `READ_POOL` / `PENDING_SIZES` module globals, so the
/// read-path fast handles and the registry never disagree.
pub(crate) struct IndexInstance {
    pub(crate) phase: IndexPhase,
    pub(crate) read_pool: Arc<ReadPool>,
    pub(crate) pending_sizes: Arc<PendingSizes>,
    /// This volume's freshness signal (gray = absent instance; blue/green/yellow
    /// = the `Freshness` variants). `Arc<Mutex<…>>` so scan-transition tasks and
    /// (M2-B) the watcher-lifetime layer can flip it without holding the registry
    /// lock. `None` means "not yet determined" (e.g. mid-initialization before
    /// the first scan transition); a `Running` volume always carries `Some`. The
    /// freshness state machine itself lives in `freshness.rs`. See DETAILS §
    /// "The freshness model".
    pub(crate) freshness: Arc<std::sync::Mutex<Option<Freshness>>>,
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
/// LazyLock is ready before any async tasks access it.
pub fn init() {
    drop(INDEX_REGISTRY.lock());
    log::debug!("Indexing registry initialized");
}

/// Whether indexing should auto-start on launch.
///
/// - If settings say disabled (`indexing_enabled == Some(false)`): never auto-start.
/// - Otherwise: auto-start by default (both dev and release builds).
pub fn should_auto_start(indexing_enabled: Option<bool>) -> bool {
    // User explicitly disabled indexing in settings
    if indexing_enabled == Some(false) {
        return false;
    }

    // Default true (setting not yet stored means first launch, enabled by default)
    true
}

/// Pure decision: should the indexer auto-start at app launch?
///
/// Combines the user's indexing-enabled setting with the FDA gate. The FDA gate
/// blocks the indexer from scanning `/` before the user has decided about Full
/// Disk Access, otherwise macOS native permission popups (iCloud, Photos, etc.)
/// stack on top of the in-app FDA modal at first launch.
///
/// Auto-start when ALL of the following hold:
/// - The user has not disabled indexing (`indexing_enabled != Some(false)`).
/// - The FDA gate isn't pending (see `crate::fda_gate::is_fda_pending`). The gate is pending only
///   when `fda_choice == NotAskedYet` AND the OS reports FDA isn't granted (i.e., we're still
///   showing the in-app onboarding modal. Once the user picks Deny (same session via
///   `start_indexing_after_fda_decision`) or Allow (which restarts the app), the indexer
///   auto-starts. After Deny, the scan triggers per-folder TCC prompts as it walks protected paths:
///   that's the "individual Allow/Deny prompts" contract the user opted into by denying FDA.
///
/// **FDA gates only the local (`root`) volume** (scanning `/` triggers TCC). SMB/MTP volumes are
/// not TCC-protected, so a future per-volume "Turn on indexing" for them must NOT route through
/// this gate (see the plan's rabbit hole #13). In M1 only `root` is ever started, so this is the
/// only auto-start path.
pub fn should_auto_start_indexing(
    indexing_enabled: Option<bool>,
    fda_choice: FullDiskAccessChoice,
    os_fda_granted: bool,
) -> bool {
    should_auto_start(indexing_enabled) && !crate::fda_gate::is_fda_pending(fda_choice, os_fda_granted)
}

// ── Registry helpers ─────────────────────────────────────────────────

/// Clone a non-root volume's read pool from its registry instance. Root's pool
/// lives in the `READ_POOL` global instead (see `enrichment::get_read_pool`).
pub(crate) fn get_instance_read_pool(volume_id: &str) -> Option<Arc<ReadPool>> {
    INDEX_REGISTRY
        .lock()
        .ok()?
        .get(volume_id)
        .map(|i| Arc::clone(&i.read_pool))
}

/// Clone a non-root volume's pending-size tracker from its registry instance.
/// Root's tracker lives in the `PENDING_SIZES` global instead.
pub(crate) fn get_instance_pending_sizes(volume_id: &str) -> Option<Arc<PendingSizes>> {
    INDEX_REGISTRY
        .lock()
        .ok()?
        .get(volume_id)
        .map(|i| Arc::clone(&i.pending_sizes))
}

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
pub(crate) fn get_writer_and_scanning_for(volume_id: &str) -> Option<(super::writer::IndexWriter, bool)> {
    let reg = INDEX_REGISTRY.lock().ok()?;
    match reg.get(volume_id).map(|i| &i.phase) {
        Some(IndexPhase::Running(mgr)) => Some((mgr.writer.clone(), mgr.scanning.load(Ordering::Relaxed))),
        _ => None,
    }
}

/// Trigger background verification of a directory against the volume's index DB.
/// Called after enrichment on each navigation. No-op if the volume's index is
/// not running. Fully fire-and-forget: the registry lock is acquired on a
/// spawned task, so it never blocks the caller (navigation thread).
pub fn trigger_verification(volume_id: &str, dir_path: &str) {
    let volume_id = volume_id.to_string();
    let dir_path = dir_path.to_string();
    tauri::async_runtime::spawn(async move {
        let reg = match INDEX_REGISTRY.lock() {
            Ok(g) => g,
            Err(_) => return,
        };
        if let Some(IndexInstance {
            phase: IndexPhase::Running(mgr),
            ..
        }) = reg.get(&volume_id)
        {
            let writer = mgr.writer.clone();
            let app = mgr.app.clone();
            let scanning = mgr.scanning.load(Ordering::Relaxed);
            drop(reg);
            verifier::maybe_verify(dir_path, writer, app, scanning);
        }
    });
}

/// Stop all scans and watcher for a volume without deleting its DB.
///
/// Called when the user disables indexing via settings. The index stays on disk
/// but no scanning or watching runs. Directory sizes revert to `<dir>`.
pub fn stop_indexing(volume_id: &str) -> Result<(), String> {
    verifier::invalidate();

    // Invalidate this volume's ReadPool/PendingSizes read-path handles before
    // shutdown so thread-local connections are discarded. For root these are the
    // module globals.
    if let Some(pool) = uninstall_read_pool(volume_id) {
        pool.invalidate();
    }
    uninstall_pending_sizes(volume_id);

    // Take the instance out under the lock, publish `ShuttingDown`, then release
    // the lock BEFORE the blocking drain. `mgr.shutdown()` blocks up to 5 s
    // draining the live-event task; holding the registry lock across it would
    // stall every concurrent `get_status`/`is_active`/`trigger_verification`
    // caller (for ANY volume) and park a tokio worker. The live event loop reads
    // via `ReadPool` and never reacquires the registry lock, so dropping the
    // guard while `ShuttingDown` is published is safe: concurrent callers see
    // `ShuttingDown` and proceed.
    let owned_mgr = {
        let mut reg = INDEX_REGISTRY
            .lock()
            .map_err(|e| format!("Failed to lock registry: {e}"))?;
        let instance = match reg.get_mut(volume_id) {
            Some(i) => i,
            None => return Ok(()), // not indexed
        };
        match std::mem::replace(&mut instance.phase, IndexPhase::ShuttingDown) {
            IndexPhase::Running(mgr) => mgr,
            IndexPhase::Initializing { .. } => {
                // An in-flight start observes the removal and shuts its half-built
                // manager down. Removing the whole instance is correct: it's
                // disabled now.
                reg.remove(volume_id);
                log::info!("Indexing stopped during initialization for '{volume_id}'");
                return Ok(());
            }
            other => {
                instance.phase = other; // put it back, wasn't running
                return Ok(());
            }
        }
    };

    // Guard released: run the blocking drain without holding the registry lock.
    let mut mgr = owned_mgr;
    mgr.shutdown();

    // Re-lock only to remove the now-disabled instance.
    {
        let mut reg = INDEX_REGISTRY
            .lock()
            .map_err(|e| format!("Failed to lock registry: {e}"))?;
        reg.remove(volume_id);
    }
    log::info!("Indexing stopped for '{volume_id}' (DB preserved on disk)");

    Ok(())
}

/// Phase classifier used by `start_indexing`'s post-`resume_or_scan` branch.
/// Returns true only while the phase carries the temporary init store. If
/// `stop_indexing` swapped the state out from under us during `resume_or_scan`,
/// the phase is `ShuttingDown` (or the instance was removed) and this returns
/// false. The caller treats that as "phase changed, shut the manager down".
///
/// Extracted as a pure helper so the state-machine race fragment is testable
/// without standing up an `AppHandle` / `IndexManager`.
pub(crate) fn is_initializing_phase(phase: &IndexPhase) -> bool {
    matches!(phase, IndexPhase::Initializing { .. })
}

/// Atomically reserve the `Initializing(store)` phase for `volume_id`. Returns
/// `Ok(())` when the volume had no registered instance (the only legitimate
/// start); returns `Err(store)` otherwise so the caller can drop the unused
/// store without constructing the heavy `IndexManager`.
///
/// This is the lock-first guard for `start_indexing`, now per volume id. Two
/// writer threads racing on the same DB share neither their `Arc<AtomicI64>` ID
/// counter nor their `AccumulatorMaps`, which produces PK collisions and
/// inflated `dir_stats`. The transition must be a single atomic check-and-set,
/// not "construct manager then maybe shut down" (which leaks a live writer
/// thread while `resume_or_scan` runs). Keyed per volume, two starts for the
/// *same* volume still can't race, while two *different* volumes start freely.
///
/// On success, installs the volume's `read_pool`/`pending_sizes` into the
/// registry instance (and, for root, the module globals) so enrichment works
/// during the `Initializing` phase.
pub(crate) fn try_reserve_initializing_phase(
    volume_id: &str,
    store: IndexStore,
    read_pool: Arc<ReadPool>,
    pending_sizes: Arc<PendingSizes>,
    initial_freshness: Option<Freshness>,
) -> Result<(), Box<IndexStore>> {
    let mut reg = INDEX_REGISTRY.lock().expect("INDEX_REGISTRY lock poisoned");
    if reg.contains_key(volume_id) {
        return Err(Box::new(store));
    }
    install_read_pool(volume_id, Arc::clone(&read_pool));
    install_pending_sizes(volume_id, Arc::clone(&pending_sizes));
    reg.insert(
        volume_id.to_string(),
        IndexInstance {
            phase: IndexPhase::Initializing { store },
            read_pool,
            pending_sizes,
            freshness: Arc::new(std::sync::Mutex::new(initial_freshness)),
        },
    );
    Ok(())
}

/// Apply a freshness transition for a volume via the pure state machine
/// (`freshness::Freshness::on`). No-op if the volume has no registered instance
/// or no current freshness value yet.
///
/// This is the seam M2-B uses to wire watcher-driven transitions
/// (`FreshnessEvent::WatcherDied` / `OverflowUnrecoverable`) into the index: it
/// just calls `apply_freshness_event(volume_id, FreshnessEvent::WatcherDied)`
/// from the watcher-lifetime layer. The scan paths call it with `ScanStarted` /
/// `ScanCompleted`.
pub(crate) fn apply_freshness_event(volume_id: &str, event: FreshnessEvent) {
    // `ScanStarted` is total even from "not yet determined": a scan can begin on
    // a volume that has no freshness yet (first ever scan). Seed it so the
    // transition is meaningful, then apply the event.
    if let Ok(reg) = INDEX_REGISTRY.lock()
        && let Some(instance) = reg.get(volume_id)
        && let Ok(mut f) = instance.freshness.lock()
    {
        let current = f.unwrap_or(Freshness::Scanning);
        *f = Some(current.on(event));
    }
}

/// Read a volume's current freshness, if it has a registered instance.
pub(crate) fn get_freshness(volume_id: &str) -> Option<Freshness> {
    INDEX_REGISTRY
        .lock()
        .ok()?
        .get(volume_id)
        .and_then(|i| i.freshness.lock().ok().and_then(|f| *f))
}

/// How a volume's index sources its freshness, which decides the scan strategy
/// and the launch-time freshness.
///
/// - [`Local`](IndexVolumeKind::Local): the boot disk. jwalk scan + FSEvents
///   journal, so a persisted index replays to **Fresh** on launch (continuity
///   self-heals). The only kind M1 ever started.
/// - [`Smb`](IndexVolumeKind::Smb): an SMB share scanned over the `Volume` trait
///   (no jwalk; `/Volumes/` is excluded from the local scanner). No event
///   journal, so a persisted index loads **Stale** on launch and the live
///   watcher (M2-B) is what keeps it Fresh while connected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum IndexVolumeKind {
    Local,
    Smb,
}

impl IndexVolumeKind {
    /// Whether this volume self-heals watch continuity from an event journal on
    /// launch. Only the local boot disk does (FSEvents replay). Feeds
    /// `freshness::initial_freshness_on_launch`.
    fn is_journaled(self) -> bool {
        matches!(self, IndexVolumeKind::Local)
    }
}

/// Create the IndexManager for the root volume and auto-start indexing
/// (resume from existing index or fresh scan).
///
/// Call after `init()`. On startup this checks for an existing index: if found,
/// it replays the FSEvents journal from the stored `last_event_id`; otherwise
/// it starts a fresh full scan.
///
/// `start_indexing` starts the local `root` volume; `start_indexing_for_smb`
/// starts an SMB share (M2). Both funnel through `start_indexing_for`.
pub fn start_indexing(app: &AppHandle) -> Result<(), String> {
    start_indexing_for(app, ROOT_VOLUME_ID, PathBuf::from("/"), IndexVolumeKind::Local)
}

/// Start indexing for a specific volume id and root path.
fn start_indexing_for(
    app: &AppHandle,
    volume_id: &str,
    volume_root: PathBuf,
    kind: IndexVolumeKind,
) -> Result<(), String> {
    log::info!("start_indexing: begin for '{volume_id}' ({kind:?})");
    super::memory_watchdog::start(app.clone());

    // Lock-first reservation, per volume id. We open the init store and the
    // read-path handles, then atomically claim the `(absent) -> Initializing`
    // transition BEFORE constructing the heavy `IndexManager`. If this volume is
    // already initializing or running, this call becomes a no-op — without the
    // per-volume guard, two writers race on the same DB (each owns its own
    // `Arc<AtomicI64>` ID counter and `AccumulatorMaps`), producing PK
    // collisions and inflated `dir_stats`.
    let data_dir = crate::config::resolved_app_data_dir(app)?;
    let db_path = data_dir.join(format!("index-{volume_id}.db"));
    let init_store = IndexStore::open(&db_path).map_err(|e| format!("Failed to open init store: {e}"))?;
    let pool = Arc::new(ReadPool::new(db_path.clone()).map_err(|e| format!("Failed to create read pool: {e}"))?);
    let pending = Arc::new(PendingSizes::new());

    // Seed the launch-time freshness from whether a scan ever completed on this
    // volume's persisted index, combined with the volume kind: a journaled local
    // index loads Fresh, a non-journaled SMB index loads Stale (we weren't
    // watching while off — the heart of the "admittedly stale" model). A fresh
    // start (no completed scan) seeds `None`; the scan transition flips it to
    // Scanning. Read the marker off the init store before reserving.
    let scan_completed = init_store
        .get_index_status()
        .map(|s| s.scan_completed_at.is_some())
        .unwrap_or(false);
    let initial_freshness = super::freshness::initial_freshness_on_launch(scan_completed, kind.is_journaled());

    if try_reserve_initializing_phase(
        volume_id,
        init_store,
        Arc::clone(&pool),
        Arc::clone(&pending),
        initial_freshness,
    )
    .is_err()
    {
        log::info!("start_indexing: '{volume_id}' already Initializing/Running/ShuttingDown, no-op");
        return Ok(());
    }

    let mut manager = match IndexManager::new_for_kind(volume_id.to_string(), volume_root, app.clone(), kind) {
        Ok(m) => m,
        Err(e) => {
            // Reservation succeeded but manager construction failed: remove the
            // instance so a subsequent call can retry cleanly, and drop the
            // installed read-path handles.
            remove_instance_and_handles(volume_id);
            return Err(e);
        }
    };

    let scan_result = manager.resume_or_scan();

    // Clone the writer before moving manager into the registry, so we can hand
    // it to the maintenance timer if startup succeeds.
    let writer_for_maintenance = manager.writer.clone();

    // Re-lock and check: if someone called stop_indexing() for this volume while
    // we were inside resume_or_scan(), the phase is no longer Initializing (or
    // the instance is gone). Respect that: shut the manager down instead of
    // overwriting.
    let mut reg = INDEX_REGISTRY
        .lock()
        .map_err(|e| format!("Failed to lock registry: {e}"))?;
    let still_initializing = reg.get(volume_id).is_some_and(|i| is_initializing_phase(&i.phase));
    match (still_initializing, scan_result) {
        (true, Ok(())) => {
            if let Some(instance) = reg.get_mut(volume_id) {
                instance.phase = IndexPhase::Running(Box::new(manager));
            }
            drop(reg);
            log::info!("start_indexing: done, '{volume_id}' IndexManager is Running");

            // Periodic DB maintenance every 30 s: reclaim free pages from
            // deletes/rescans (`IncrementalVacuum`) AND truncate the WAL file
            // so its high-water mark doesn't sit on disk (`WalCheckpoint`).
            // Both stop automatically when the writer channel closes.
            tauri::async_runtime::spawn(async move {
                loop {
                    tokio::time::sleep(Duration::from_secs(30)).await;
                    if writer_for_maintenance.send(WriteMessage::IncrementalVacuum).is_err() {
                        break;
                    }
                    if writer_for_maintenance.send(WriteMessage::WalCheckpoint).is_err() {
                        break;
                    }
                }
            });
        }
        (true, Err(e)) => {
            drop(reg);
            remove_instance_and_handles(volume_id);
            return Err(e);
        }
        (false, Ok(())) => {
            // Phase changed (e.g. stop_indexing removed the instance). Don't override.
            drop(reg);
            log::info!("start_indexing: '{volume_id}' phase changed during init, shutting down manager");
            manager.shutdown();
        }
        (false, Err(e)) => {
            drop(reg);
            log::warn!("start_indexing: resume_or_scan failed and phase changed for '{volume_id}': {e}");
            manager.shutdown();
        }
    }

    Ok(())
}

/// Internal SMB-start entry point, called by `smb_index::start_indexing_for_smb`
/// AFTER the direct-smb2 gate has passed. Funnels into the shared
/// `start_indexing_for` with the `Smb` kind so the lock-first reservation,
/// load-as-Stale freshness seeding, and `Volume`-trait scan path all apply.
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(crate) fn start_indexing_for_smb_inner(
    app: &AppHandle,
    volume_id: &str,
    mount_root: PathBuf,
) -> Result<(), String> {
    start_indexing_for(app, volume_id, mount_root, IndexVolumeKind::Smb)
}

/// Discard a volume's partial index and reset it to gray / not-indexed
/// (D-interrupted): an interrupted/disconnected network scan leaves data that's
/// worthless once the volume is gone, so we don't keep a half-snapshot live.
///
/// Removes the registry instance (so reads skip → gray), draining/shutting down
/// the writer first. The DB file stays on disk but carries no `scan_completed_at`
/// (the scan path cleared it at start), so a future enable does a clean fresh
/// scan. Equivalent to `stop_indexing` for this purpose, named for intent.
pub(crate) fn reset_to_not_indexed(volume_id: &str) {
    if let Err(e) = stop_indexing(volume_id) {
        log::warn!("reset_to_not_indexed('{volume_id}') failed: {e}");
    }
}

/// Remove a volume's instance from the registry and uninstall its read-path
/// handles (for root, the module globals). Used on start-up failure paths.
fn remove_instance_and_handles(volume_id: &str) {
    {
        let mut reg = INDEX_REGISTRY.lock().expect("INDEX_REGISTRY lock poisoned");
        reg.remove(volume_id);
    }
    if let Some(pool) = uninstall_read_pool(volume_id) {
        pool.invalidate();
    }
    uninstall_pending_sizes(volume_id);
}

/// Stop all scans, shut down the writer, delete the DB file, and reset state
/// for a volume.
///
/// Call `start_indexing()` to create a fresh index afterward.
pub fn clear_index(volume_id: &str) -> Result<(), String> {
    verifier::invalidate();

    // Invalidate this volume's ReadPool/PendingSizes before deleting DB files so
    // thread-local connections are discarded.
    if let Some(pool) = uninstall_read_pool(volume_id) {
        pool.invalidate();
    }
    uninstall_pending_sizes(volume_id);

    // Take the instance out under the lock, publish `ShuttingDown`, then release
    // the lock BEFORE the blocking drain (same reasoning as `stop_indexing`: the
    // up-to-5 s `mgr.shutdown()` drain must not stall concurrent registry
    // readers or park a tokio worker). The live event loop reads via `ReadPool`
    // and never reacquires the registry lock, so dropping the guard while
    // `ShuttingDown` is published is safe.
    let owned_mgr = {
        let mut reg = INDEX_REGISTRY
            .lock()
            .map_err(|e| format!("Failed to lock registry: {e}"))?;
        let instance = match reg.get_mut(volume_id) {
            Some(i) => i,
            None => {
                log::info!("Drive index clear requested but '{volume_id}' was not indexed");
                return Ok(());
            }
        };
        match std::mem::replace(&mut instance.phase, IndexPhase::ShuttingDown) {
            IndexPhase::Running(mgr) => mgr,
            other => {
                instance.phase = other;
                log::info!("Drive index clear requested but '{volume_id}' was not active");
                return Ok(());
            }
        }
    };

    // Guard released: run the blocking drain without holding the registry lock.
    let mut mgr = owned_mgr;
    let db_path = mgr.db_path().to_path_buf();
    mgr.shutdown();

    // Re-lock only to remove the now-disabled instance.
    {
        let mut reg = INDEX_REGISTRY
            .lock()
            .map_err(|e| format!("Failed to lock registry: {e}"))?;
        reg.remove(volume_id);
    }

    // Delete DB file and WAL/SHM sidecars
    for path in [
        db_path.clone(),
        db_path.with_extension("db-wal"),
        db_path.with_extension("db-shm"),
    ] {
        if path.exists() {
            std::fs::remove_file(&path).map_err(|e| format!("Failed to delete {}: {e}", path.display()))?;
        }
    }
    log::info!("Drive index cleared for '{volume_id}' (DB deleted)");

    Ok(())
}

// ── Module-level public API (called by IPC commands) ─────────────────

/// Per-volume index status for the freshness UX (M3's per-drive badge).
///
/// Carries the volume's freshness color plus the last completed scan's facts
/// (`scan_completed_at`, `scan_duration_ms`) for the tooltip/menu footer. This
/// is the shape M3 consumes for EVERY drive (local included). A volume with no
/// registered instance is the gray / not-indexed state (`enabled: false`,
/// `freshness: None`); a registered one always carries a `freshness`.
///
/// Freshness is read from the registry; the scan facts come from the persisted
/// `meta` surfaced by `get_status`. The two can briefly disagree during a
/// transition, which is fine for a status badge.
pub fn get_volume_index_status(volume_id: &str) -> VolumeIndexStatus {
    let freshness = get_freshness(volume_id);
    let enabled = is_active(volume_id);

    // Pull the persisted last-scan facts from the status response (best-effort;
    // a not-indexed volume yields `None`s).
    let (scan_completed_at, scan_duration_ms) = get_status(volume_id)
        .ok()
        .and_then(|s| s.index_status)
        .map(|st| {
            (
                st.scan_completed_at.and_then(|v| v.parse::<u64>().ok()),
                st.scan_duration_ms.and_then(|v| v.parse::<u64>().ok()),
            )
        })
        .unwrap_or((None, None));

    VolumeIndexStatus {
        volume_id: volume_id.to_string(),
        enabled,
        freshness,
        scan_completed_at,
        scan_duration_ms,
    }
}

/// Per-volume index status, resolving the owning volume from a path (the IPC
/// stays path-based, like `get_dir_stats`). An SMB path resolves to its SMB
/// volume id; everything else to `root`.
pub fn get_volume_index_status_for_path(path: &str) -> VolumeIndexStatus {
    get_volume_index_status(&volume_id_for_local_path(path))
}

/// The empty/disabled status response (a volume with no running index).
fn disabled_status_response() -> IndexStatusResponse {
    IndexStatusResponse {
        initialized: false,
        scanning: false,
        entries_scanned: 0,
        dirs_found: 0,
        bytes_scanned: 0,
        index_status: None,
        db_file_size: None,
        volume_used_bytes: None,
    }
}

/// Get the current indexing status for a volume.
pub fn get_status(volume_id: &str) -> Result<IndexStatusResponse, String> {
    let reg = INDEX_REGISTRY.lock().map_err(|e| format!("Lock poisoned: {e}"))?;
    match reg.get(volume_id).map(|i| &i.phase) {
        None | Some(IndexPhase::ShuttingDown) => Ok(disabled_status_response()),
        Some(IndexPhase::Initializing { store, .. }) => {
            let db_file_size = store.db_file_size().ok();
            let index_status = store.get_index_status().ok();
            Ok(IndexStatusResponse {
                initialized: true,
                scanning: true,
                entries_scanned: 0,
                dirs_found: 0,
                bytes_scanned: 0,
                index_status,
                db_file_size,
                volume_used_bytes: None,
            })
        }
        Some(IndexPhase::Running(mgr)) => mgr.get_status(),
    }
}

/// Get extended debug status for the debug window.
pub fn get_debug_status(volume_id: &str) -> Result<IndexDebugStatusResponse, String> {
    let reg = INDEX_REGISTRY.lock().map_err(|e| format!("Lock poisoned: {e}"))?;
    match reg.get(volume_id).map(|i| &i.phase) {
        None | Some(IndexPhase::ShuttingDown) => {
            let base = disabled_status_response();
            let (activity_phase, phase_started_at, phase_duration_ms, phase_history) =
                IndexManager::read_phase_timeline();
            Ok(IndexDebugStatusResponse {
                base,
                watcher_active: false,
                live_event_count: 0,
                must_scan_count: 0,
                must_scan_rescans_completed: 0,
                live_entry_count: None,
                live_dir_count: None,
                dirs_with_stats: None,
                recent_must_scan_paths: Vec::new(),
                activity_phase,
                phase_started_at,
                phase_duration_ms,
                phase_history,
                verifying: false,
                db_main_size: None,
                db_wal_size: None,
                db_page_count: None,
                db_freelist_count: None,
            })
        }
        Some(IndexPhase::Initializing { store, .. }) => {
            let db_file_size = store.db_file_size().ok();
            let index_status = store.get_index_status().ok();
            let base = IndexStatusResponse {
                initialized: true,
                scanning: true,
                entries_scanned: 0,
                dirs_found: 0,
                bytes_scanned: 0,
                index_status,
                db_file_size,
                volume_used_bytes: None,
            };
            let (activity_phase, phase_started_at, phase_duration_ms, phase_history) =
                IndexManager::read_phase_timeline();
            let db_main_size = store.db_main_size().ok();
            let db_wal_size = store.db_wal_size().ok();
            let conn = store.read_conn();
            let (db_page_count, db_freelist_count) = IndexStore::db_page_stats(conn)
                .map(|(p, f)| (Some(p), Some(f)))
                .unwrap_or((None, None));
            Ok(IndexDebugStatusResponse {
                base,
                watcher_active: DEBUG_STATS.watcher_active.load(Ordering::Relaxed),
                live_event_count: 0,
                must_scan_count: 0,
                must_scan_rescans_completed: 0,
                live_entry_count: None,
                live_dir_count: None,
                dirs_with_stats: None,
                recent_must_scan_paths: Vec::new(),
                activity_phase,
                phase_started_at,
                phase_duration_ms,
                phase_history,
                verifying: DEBUG_STATS.verifying.load(Ordering::Relaxed),
                db_main_size,
                db_wal_size,
                db_page_count,
                db_freelist_count,
            })
        }
        Some(IndexPhase::Running(mgr)) => mgr.get_debug_status(),
    }
}

/// Look up recursive stats for a single directory in a volume's index.
pub fn get_dir_stats_on_volume(volume_id: &str, path: &str) -> Result<Option<DirStats>, String> {
    let pool = match get_read_pool_for(volume_id) {
        Some(p) => p,
        None => return Ok(None),
    };
    let normalized = firmlinks::normalize_path(path);

    pool.with_conn(|conn| {
        let entry_id =
            match store::resolve_path(conn, &normalized).map_err(|e| format!("Couldn't resolve path: {e}"))? {
                Some(id) => id,
                None => return Ok(None),
            };

        let stats =
            IndexStore::get_dir_stats_by_id(conn, entry_id).map_err(|e| format!("Couldn't get dir stats: {e}"))?;

        let pending = get_pending_sizes_for(volume_id).is_some_and(|t| t.is_pending(&normalized));
        Ok(stats.map(|s| DirStats {
            path: normalized.clone(),
            recursive_size: s.recursive_logical_size,
            recursive_physical_size: s.recursive_physical_size,
            recursive_file_count: s.recursive_file_count,
            recursive_dir_count: s.recursive_dir_count,
            recursive_has_symlinks: s.recursive_has_symlinks,
            recursive_size_pending: pending,
        }))
    })?
}

/// Look up recursive stats for a single directory, resolving the owning volume
/// from the path. IPC stays path-based (see `commands/indexing.rs`); the volume
/// is resolved internally. In M1 every absolute local path resolves to `root`.
pub fn get_dir_stats(path: &str) -> Result<Option<DirStats>, String> {
    get_dir_stats_on_volume(&volume_id_for_local_path(path), path)
}

/// Batch lookup of dir_stats for multiple paths on a volume.
pub fn get_dir_stats_batch_on_volume(volume_id: &str, paths: &[String]) -> Result<Vec<Option<DirStats>>, String> {
    let pool = match get_read_pool_for(volume_id) {
        Some(p) => p,
        None => return Ok(paths.iter().map(|_| None).collect()),
    };

    pool.with_conn(|conn| {
        let mut results = Vec::with_capacity(paths.len());
        let mut id_to_idx: Vec<(i64, usize, String)> = Vec::new();

        for (i, path) in paths.iter().enumerate() {
            let normalized = firmlinks::normalize_path(path);
            match store::resolve_path(conn, &normalized).map_err(|e| format!("Couldn't resolve path: {e}"))? {
                Some(id) => {
                    id_to_idx.push((id, i, normalized));
                    results.push(None);
                }
                None => results.push(None),
            }
        }

        if !id_to_idx.is_empty() {
            let ids: Vec<i64> = id_to_idx.iter().map(|(id, _, _)| *id).collect();
            let stats_batch = IndexStore::get_dir_stats_batch_by_ids(conn, &ids)
                .map_err(|e| format!("Couldn't get dir stats batch: {e}"))?;

            let tracker = get_pending_sizes_for(volume_id);
            for ((_, idx, normalized), stats_opt) in id_to_idx.into_iter().zip(stats_batch) {
                let pending = tracker.as_ref().is_some_and(|t| t.is_pending(&normalized));
                results[idx] = stats_opt.map(|s| DirStats {
                    path: normalized,
                    recursive_size: s.recursive_logical_size,
                    recursive_physical_size: s.recursive_physical_size,
                    recursive_file_count: s.recursive_file_count,
                    recursive_dir_count: s.recursive_dir_count,
                    recursive_has_symlinks: s.recursive_has_symlinks,
                    recursive_size_pending: pending,
                });
            }
        }

        Ok(results)
    })?
}

/// Batch lookup of dir_stats, resolving the owning volume from the paths. The
/// IPC `get_dir_stats_batch` sends one directory's children, which all live on
/// one volume; resolving from the first path is sufficient. In M1 every
/// absolute local path resolves to `root`.
pub fn get_dir_stats_batch(paths: &[String]) -> Result<Vec<Option<DirStats>>, String> {
    let volume_id = paths
        .first()
        .map(|p| volume_id_for_local_path(p))
        .unwrap_or_else(|| ROOT_VOLUME_ID.to_string());
    get_dir_stats_batch_on_volume(&volume_id, paths)
}

/// Resolve a filesystem path to its index volume id.
///
/// An SMB-mounted path (`/Volumes/<share>/…` on macOS, an `smbfs`/`cifs` mount
/// on Linux) maps to its `smb_volume_id(server, port, share)` — the SAME id the
/// `VolumeManager` and the SMB index register under — so a listing under that
/// share routes to the SMB volume's index, not `root`. Everything else (local
/// absolute paths) is `root`. The routed read paths still skip cleanly when the
/// resolved volume has no registered index (`get_read_pool_for` → `None`), so an
/// SMB share that isn't indexed costs zero DB work, exactly like before.
///
/// MTP virtual paths (`mtp-*`) still resolve to `root` here; M4 maps them.
fn volume_id_for_local_path(path: &str) -> VolumeId {
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    if let Some(smb_id) = super::smb_index::smb_volume_id_for_path(path) {
        return smb_id;
    }
    // TODO(M4): map mtp-* virtual paths to their volume ids.
    let _ = path;
    ROOT_VOLUME_ID.to_string()
}

/// Force a fresh full scan for a volume (for debug/manual trigger).
pub fn force_scan(volume_id: &str) -> Result<(), String> {
    let mut reg = INDEX_REGISTRY.lock().map_err(|e| format!("Lock poisoned: {e}"))?;
    match reg.get_mut(volume_id).map(|i| &mut i.phase) {
        Some(IndexPhase::Running(mgr)) => mgr.start_scan("manual start"),
        _ => Err("Indexing not initialized".to_string()),
    }
}

/// Stop the active scan for a volume without shutting down the manager.
pub fn stop_scan(volume_id: &str) -> Result<(), String> {
    let mut reg = INDEX_REGISTRY.lock().map_err(|e| format!("Lock poisoned: {e}"))?;
    match reg.get_mut(volume_id).map(|i| &mut i.phase) {
        Some(IndexPhase::Running(mgr)) => {
            mgr.stop_scan();
            Ok(())
        }
        _ => Err("Indexing not initialized".to_string()),
    }
}

/// Snapshot every registered volume id. Used by the global memory watchdog to
/// stop EVERY volume's index (not just `root`) when the global budget is hit.
pub(crate) fn all_registered_volume_ids() -> Vec<VolumeId> {
    INDEX_REGISTRY
        .lock()
        .map(|reg| reg.keys().cloned().collect())
        .unwrap_or_default()
}

/// Stop indexing for every registered volume (the global memory-budget action).
/// Each `stop_indexing` drains and removes one instance; we snapshot the ids
/// first so we're not iterating the map while `stop_indexing` mutates it.
pub(crate) fn stop_all_indexing() {
    for volume_id in all_registered_volume_ids() {
        if let Err(e) = stop_indexing(&volume_id) {
            log::warn!("stop_all_indexing: stop_indexing('{volume_id}') failed: {e}");
        }
    }
}

/// Check whether a volume's index is active (initializing or running).
pub fn is_active(volume_id: &str) -> bool {
    INDEX_REGISTRY
        .lock()
        .map(|reg| {
            matches!(
                reg.get(volume_id).map(|i| &i.phase),
                Some(IndexPhase::Initializing { .. } | IndexPhase::Running(_))
            )
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The read path's skip-vs-route gate is "does `get_read_pool_for` return a
    /// pool?". An unregistered volume must return `None` (so its listings skip
    /// before any DB work, exactly like the old `should_exclude` early-return); a
    /// reserved one (root → global pool, non-root → instance pool) returns the
    /// pool. Reserving installs the pool, so the gate flips on; removing drops it.
    #[test]
    fn read_pool_routing_tracks_registration() {
        let _guard = INDEX_REGISTRY_TEST_GUARD.lock().unwrap_or_else(|e| e.into_inner());
        clear_registry_and_pools();

        let indexed = |vid: &str| get_read_pool_for(vid).is_some();

        assert!(!indexed("root"), "no pool => not indexed");
        assert!(!indexed("smb-nas"), "absent key => not indexed");

        // Reserve root (installs the global pool) and a non-root volume (installs
        // the instance pool). Both must then route to a pool.
        let dir = tempfile::tempdir().expect("temp dir");
        let reserve = |name: &str| {
            let db_path = dir.path().join(format!("{name}.db"));
            let store = IndexStore::open(&db_path).expect("open store");
            let pool = Arc::new(ReadPool::new(db_path.clone()).expect("pool"));
            let pending = Arc::new(PendingSizes::new());
            assert!(
                try_reserve_initializing_phase(name, store, pool, pending, None).is_ok(),
                "reserve {name} must succeed"
            );
        };
        reserve(ROOT_VOLUME_ID);
        reserve("smb-nas");

        assert!(indexed("root"), "reserved root => indexed");
        assert!(indexed("smb-nas"), "reserved non-root => indexed");
        assert!(!indexed("mtp-phone"), "unreserved volume still not indexed");
        // Routing is per-volume: root's pool and the non-root pool are distinct Arcs.
        assert!(
            !Arc::ptr_eq(
                &get_read_pool_for("root").unwrap(),
                &get_read_pool_for("smb-nas").unwrap()
            ),
            "each volume must route to its own pool, never another's"
        );

        clear_registry_and_pools();
        assert!(!indexed("root"), "cleared root => not indexed");
        assert!(!indexed("smb-nas"), "cleared non-root => not indexed");
    }

    /// Two distinct non-root volume ids reserve and release independently:
    /// reserving one must not block or affect the other, and removing one leaves
    /// the other intact. This is the per-volume isolation the registry buys — the
    /// `start/stop` two-volumes-don't-corrupt-each-other proof at the lock layer
    /// (the full lifecycle needs an `AppHandle`, kept under integration/E2E).
    #[test]
    fn reservations_are_independent_across_volumes() {
        let _guard = INDEX_REGISTRY_TEST_GUARD.lock().unwrap_or_else(|e| e.into_inner());
        clear_registry_and_pools();

        let dir = tempfile::tempdir().expect("temp dir");
        let mk = |name: &str| {
            let db_path = dir.path().join(format!("{name}.db"));
            let store = IndexStore::open(&db_path).expect("store");
            let pool = Arc::new(ReadPool::new(db_path.clone()).expect("pool"));
            let pending = Arc::new(PendingSizes::new());
            (store, pool, pending)
        };

        let (s1, p1, pe1) = mk("vol-a");
        let (s2, p2, pe2) = mk("vol-b");

        assert!(try_reserve_initializing_phase("vol-a", s1, p1, pe1, None).is_ok());
        assert!(try_reserve_initializing_phase("vol-b", s2, p2, pe2, None).is_ok());
        assert!(is_active("vol-a"));
        assert!(is_active("vol-b"));
        // Each volume routes to ITS OWN pool, never the other's (no cross-talk).
        assert!(get_read_pool_for("vol-a").is_some() && get_read_pool_for("vol-b").is_some());

        // A second reservation for vol-a must fail (would spawn a second writer
        // on the same DB) while vol-b is untouched.
        let (s1b, p1b, pe1b) = mk("vol-a");
        assert!(
            try_reserve_initializing_phase("vol-a", s1b, p1b, pe1b, None).is_err(),
            "double-start of the same volume must be rejected"
        );
        assert!(is_active("vol-b"), "vol-b unaffected by vol-a's rejected start");

        // Remove vol-a; vol-b survives.
        INDEX_REGISTRY.lock().unwrap().remove("vol-a");
        assert!(!is_active("vol-a"));
        assert!(
            get_read_pool_for("vol-a").is_none(),
            "vol-a's pool gone with its instance"
        );
        assert!(is_active("vol-b"), "removing vol-a must not disturb vol-b");
        assert!(get_read_pool_for("vol-b").is_some(), "vol-b still routable");

        clear_registry_and_pools();
    }

    /// Freshness rides the registry instance and transitions through the pure
    /// state machine via `apply_freshness_event`. This pins the registry-level
    /// wiring (the seam M2-B's watcher uses): a volume reserved Stale (the
    /// load-as-Stale-on-launch case) goes Stale → Scanning → Fresh, and the
    /// M2-B watcher-died event flips Fresh → Stale. The pure transitions
    /// themselves are pinned in `freshness::tests`; this proves the registry
    /// stores and threads them.
    #[test]
    fn freshness_transitions_through_the_registry() {
        let _guard = INDEX_REGISTRY_TEST_GUARD.lock().unwrap_or_else(|e| e.into_inner());
        clear_registry_and_pools();
        INDEX_REGISTRY.lock().unwrap().remove("smb-fresh-test");

        let dir = tempfile::tempdir().expect("temp dir");
        let db_path = dir.path().join("smb-fresh-test.db");
        let store = IndexStore::open(&db_path).expect("open store");
        let pool = Arc::new(ReadPool::new(db_path.clone()).expect("pool"));
        let pending = Arc::new(PendingSizes::new());

        // Reserve as Stale — the load-as-Stale-on-launch case for a persisted
        // SMB index.
        assert!(
            try_reserve_initializing_phase("smb-fresh-test", store, pool, pending, Some(Freshness::Stale)).is_ok(),
            "reserve must succeed"
        );
        assert_eq!(get_freshness("smb-fresh-test"), Some(Freshness::Stale), "loads Stale");

        // A rescan begins ⇒ Scanning.
        apply_freshness_event("smb-fresh-test", FreshnessEvent::ScanStarted);
        assert_eq!(get_freshness("smb-fresh-test"), Some(Freshness::Scanning));

        // Clean completion ⇒ Fresh.
        apply_freshness_event("smb-fresh-test", FreshnessEvent::ScanCompleted);
        assert_eq!(get_freshness("smb-fresh-test"), Some(Freshness::Fresh));

        // M2-B's seam: a watcher death flips Fresh ⇒ Stale.
        apply_freshness_event("smb-fresh-test", FreshnessEvent::WatcherDied);
        assert_eq!(get_freshness("smb-fresh-test"), Some(Freshness::Stale));

        // An absent volume has no freshness, and events on it are no-ops.
        assert_eq!(get_freshness("never-registered"), None);
        apply_freshness_event("never-registered", FreshnessEvent::ScanCompleted);
        assert_eq!(get_freshness("never-registered"), None);

        INDEX_REGISTRY.lock().unwrap().remove("smb-fresh-test");
        clear_registry_and_pools();
    }

    /// Reset every registry-backed test global: the instance map plus the root
    /// read-path globals (which live outside the map).
    fn clear_registry_and_pools() {
        INDEX_REGISTRY.lock().unwrap().clear();
        uninstall_read_pool(ROOT_VOLUME_ID);
        uninstall_pending_sizes(ROOT_VOLUME_ID);
    }

    /// Tests that mutate `INDEX_REGISTRY` serialize on this guard (mirrors
    /// `integration_tests.rs`'s `INDEXING_TEST_GUARD`).
    static INDEX_REGISTRY_TEST_GUARD: LazyLock<std::sync::Mutex<()>> = LazyLock::new(|| std::sync::Mutex::new(()));
}
