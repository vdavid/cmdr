//! Indexing state machine and the per-volume registry.
//!
//! Holds the `INDEX_REGISTRY` (one `IndexInstance` per volume id) and the
//! `IndexPhase` enum that gates every public operation for a volume. Also owns
//! the bootstrap logic that spins up the `IndexManager`, the `ReadPool`, and the
//! incremental-vacuum timer.
//!
//! ## Registry shape
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

use super::enrichment::{ReadPool, install_read_pool, uninstall_read_pool};
use super::freshness::{Freshness, FreshnessEvent};
use super::manager::IndexManager;
use super::pending_sizes::{PendingSizes, install_pending_sizes, uninstall_pending_sizes};
use super::store::IndexStore;
use super::verifier;
use super::writer::WriteMessage;

use crate::settings::FullDiskAccessChoice;

/// A volume's identity in the index registry (e.g. `"root"` for the local disk).
pub(crate) type VolumeId = String;

/// The local-disk volume id. The only volume registered when no network drive
/// is indexed.
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
    /// This volume's scan kind (Local / SMB / MTP). Retained so a consumer of the
    /// registry (the importance scheduler's startup sweep) can branch typed on the
    /// kind — score Local + SMB, exclude MTP — instead of re-deriving it from the
    /// volume-id string (which the `no-string-matching` rule forbids).
    pub(crate) kind: IndexVolumeKind,
    pub(crate) read_pool: Arc<ReadPool>,
    pub(crate) pending_sizes: Arc<PendingSizes>,
    /// This volume's freshness signal (gray = absent instance; blue/green/yellow
    /// = the `Freshness` variants). `Arc<Mutex<…>>` so scan-transition tasks and
    /// the live-watch layer can flip it without holding the registry
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

/// App handle for emitting freshness-change events from the (otherwise
/// handle-free) `apply_freshness_event` seam. Set once in `init`; absent only
/// before setup or in unit tests, where the emit is silently skipped.
static APP_HANDLE: std::sync::OnceLock<AppHandle> = std::sync::OnceLock::new();

// ── Initialization ───────────────────────────────────────────────────

/// Force-initialize the registry static and stash the app handle for freshness
/// event emission. Called during app setup so the LazyLock is ready before any
/// async tasks access it.
pub fn init(app: &AppHandle) {
    drop(INDEX_REGISTRY.lock());
    let _ = APP_HANDLE.set(app.clone());
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
/// this gate. When no network drive is indexed, only `root` is ever started, so this is the
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
///
/// The caller owns the `freshness` `Arc` (it shares a clone with the
/// `IndexManager`, which fires scan transitions through it WITHOUT re-locking
/// the registry); the instance stores the same `Arc`, so the manager and the
/// registry never disagree about freshness.
pub(crate) fn try_reserve_initializing_phase(
    volume_id: &str,
    kind: IndexVolumeKind,
    store: IndexStore,
    read_pool: Arc<ReadPool>,
    pending_sizes: Arc<PendingSizes>,
    freshness: Arc<std::sync::Mutex<Option<Freshness>>>,
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
            kind,
            read_pool,
            pending_sizes,
            freshness,
        },
    );
    Ok(())
}

/// Apply a freshness transition for a volume via the pure state machine
/// (`freshness::Freshness::on`). No-op if the volume has no registered instance
/// or no current freshness value yet.
///
/// EXTERNAL callers that only have a volume id (the live-watch layer:
/// `smb_index` / `mtp_index` firing `WatcherDied` / `OverflowUnrecoverable`)
/// use this entry point — it looks the instance's freshness `Arc` up UNDER the
/// registry lock, then delegates to `apply_freshness_event_on` (which does the
/// real transition + emit and never touches the registry).
///
/// ⚠️ Callers that ALREADY hold the registry lock (or can deadlock if it's
/// re-entered) must NOT use this. `IndexManager` fires its own scan-transition
/// events via `apply_freshness_event_on(&self.freshness, …)` using the `Arc` it
/// holds directly, so a `force_scan`/fallback caller can hold the registry lock
/// across `start_scan` without self-deadlocking on this re-lock.
pub(crate) fn apply_freshness_event(volume_id: &str, event: FreshnessEvent) {
    // Resolve the volume's freshness Arc UNDER the registry lock, clone it, then
    // DROP the lock before the transition + emit. The transition itself never
    // needs the registry, so holding it across the emit (a Tauri call) is both
    // unnecessary and a re-entrancy hazard for any caller already under the lock.
    let freshness = {
        let Ok(reg) = INDEX_REGISTRY.lock() else { return };
        let Some(instance) = reg.get(volume_id) else { return };
        Arc::clone(&instance.freshness)
    };
    apply_freshness_event_on(&freshness, volume_id, event);
}

/// The actual freshness transition + FE emit, operating on a volume's freshness
/// `Arc` DIRECTLY — it NEVER locks `INDEX_REGISTRY`. This is the lock-discipline
/// seam: `IndexManager` holds a clone of its volume's freshness `Arc` and fires
/// scan transitions through here, so a scan-start firing can't re-enter the
/// registry (the self-deadlock a held-registry caller like `force_scan` hit).
///
/// `apply_freshness_event` is the registry-lookup wrapper for external callers
/// that only have a volume id.
pub(crate) fn apply_freshness_event_on(
    freshness: &std::sync::Mutex<Option<Freshness>>,
    volume_id: &str,
    event: FreshnessEvent,
) {
    // `ScanStarted` is total even from "not yet determined": a scan can begin on
    // a volume that has no freshness yet (first ever scan). Seed it so the
    // transition is meaningful, then apply the event.
    //
    // We compute the next value under the freshness lock, then emit the FE event
    // AFTER dropping it (emit never needs it, and holding a std Mutex across a
    // Tauri call risks contention). The event fires only on an actual value
    // change, so the FE's one-time stale dialog sees the exact Fresh→Stale
    // transition (subscribe-don't-poll).
    let changed_to = {
        let Ok(mut f) = freshness.lock() else { return };
        let previous = *f;
        let next = f.unwrap_or(Freshness::Scanning).on(event);
        *f = Some(next);
        (previous != Some(next)).then_some(next)
    };

    if let Some(next) = changed_to
        && let Some(app) = APP_HANDLE.get()
    {
        use tauri_specta::Event;
        let _ = super::events::IndexFreshnessChangedEvent {
            volume_id: volume_id.to_string(),
            freshness: next,
        }
        .emit(app);
    }

    // Publish scan completion on the neutral in-process lifecycle bus, alongside
    // the frontend `.emit` above. A backend subsystem (the importance scheduler)
    // drives its full-volume recompute off this, without `indexing/` depending on
    // it (plan Decision 4). We fire on the EVENT, not on a freshness change: a
    // Fresh→Fresh rescan completion still means new data to rescore, and it must
    // notify the bus even though the badge didn't move.
    if event == FreshnessEvent::ScanCompleted {
        super::lifecycle_bus::publish_scan_completed(volume_id);
    }
}

/// Bump a volume's `current_epoch` on a continuity break that does NOT rescan
/// (watcher death, change-notify overflow, MTP disconnect, or the disconnect
/// completion branch). Routes through the volume's running writer so the bump
/// honors the single-writer-per-DB invariant. No-op for an unindexed or
/// not-yet-`Running` volume (a scan-start funnel bumps via its own flushed send,
/// not this helper).
///
/// Fire-and-forget: the bump rides the writer channel in order behind any
/// in-flight writes, so a subsequent read may briefly see the old epoch. That's
/// benign — the freshness badge already flips Stale alongside this call, and the
/// per-dir stale derivation self-corrects once the bump commits.
pub(crate) fn bump_current_epoch_for(volume_id: &str) {
    if let Some((writer, _scanning)) = get_writer_and_scanning_for(volume_id)
        && let Err(e) = writer.send(WriteMessage::BumpCurrentEpoch)
    {
        log::warn!("bump_current_epoch_for('{volume_id}'): writer send failed: {e}");
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

/// How a volume's index is scanned, watched, rooted, and searched.
///
/// Four capabilities that move together for the three original kinds but pull
/// apart for [`LocalExternal`](IndexVolumeKind::LocalExternal), so each is an
/// explicit, orthogonal method rather than a single conflated predicate:
///
/// - [`uses_local_scanner`](Self::uses_local_scanner): the guarded walker + FSEvents pipeline
///   (`Local`, `LocalExternal`) vs the `Volume` trait scanner (`Smb`, `Mtp`).
///   Its exact complement is [`is_trait_scanned`](Self::is_trait_scanned).
/// - [`has_event_journal`](Self::has_event_journal): self-heals watch continuity
///   by replaying an FSEvents journal on launch. Only the boot disk (`Local`).
/// - [`mount_rooted`](Self::mount_rooted): the index `ROOT_ID` is the mount
///   (`/Volumes/X`), not `/`. True for `LocalExternal`, `Smb`, `Mtp`.
/// - [`feeds_search`](Self::feeds_search): the single volume whose writes back
///   the in-memory search index. Only the boot disk (`Local`).
///
/// The kinds:
///
/// - [`Local`](IndexVolumeKind::Local): the boot disk. The guarded walker's scan + FSEvents
///   journal, so a persisted index replays to **Fresh** on launch (continuity
///   self-heals). `/`-rooted and the sole search-feeding volume. The only kind
///   started when no network drive is indexed.
/// - [`LocalExternal`](IndexVolumeKind::LocalExternal): a plain local external
///   drive (USB stick, SD card, extra disk, mounted disk image). Uses the same
///   guarded walker + FSEvents pipeline as `Local`, but mount-rooted (`ROOT_ID` =
///   `/Volumes/X`). It has no FSEvents journal (external volumes carry no
///   `.fseventsd`), so a persisted index loads **Stale** on launch; live
///   FSEvents still fire while mounted, so a running watcher keeps it current.
///   Doesn't feed search.
/// - [`Smb`](IndexVolumeKind::Smb): an SMB share scanned over the `Volume` trait
///   (no guarded walker; `/Volumes/` is excluded from the local scanner). Mount-rooted.
///   No event journal, so a persisted index loads **Stale** on launch and the
///   live watcher is what keeps it Fresh while connected.
/// - [`Mtp`](IndexVolumeKind::Mtp): a phone/camera storage scanned over the same
///   `Volume` trait. Identical to `Smb` for indexing purposes (non-journaled,
///   mount-rooted, network/USB scan path, loads Stale on launch); the live PTP
///   event loop keeps it Fresh while the device is connected (D4). A distinct
///   variant only so the scan path and any future MTP-specific tuning have a
///   name to branch on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum IndexVolumeKind {
    Local,
    LocalExternal,
    Smb,
    Mtp,
}

impl IndexVolumeKind {
    /// Whether this volume is scanned and watched by the local guarded walker + FSEvents
    /// pipeline rather than the `Volume` trait scanner. True for the boot disk
    /// and local external drives. Exact complement of
    /// [`is_trait_scanned`](Self::is_trait_scanned).
    pub(crate) fn uses_local_scanner(self) -> bool {
        matches!(self, IndexVolumeKind::Local | IndexVolumeKind::LocalExternal)
    }

    /// Whether this volume scans over the `Volume` trait (network/USB) rather
    /// than the local guarded walker. SMB and MTP both do. Exact complement of
    /// [`uses_local_scanner`](Self::uses_local_scanner).
    pub(crate) fn is_trait_scanned(self) -> bool {
        matches!(self, IndexVolumeKind::Smb | IndexVolumeKind::Mtp)
    }

    /// Whether this volume self-heals watch continuity from an event journal on
    /// launch. Only the local boot disk does (FSEvents replay). Feeds
    /// `freshness::initial_freshness_on_launch`. Local external drives carry no
    /// `.fseventsd`, and SMB and MTP have no journal.
    pub(crate) fn has_event_journal(self) -> bool {
        matches!(self, IndexVolumeKind::Local)
    }

    /// Whether the index's `ROOT_ID` is the volume's mount point (`/Volumes/X`)
    /// rather than `/`. True for every volume except the boot disk: local
    /// external drives, SMB shares, and MTP devices all index relative to their
    /// mount.
    ///
    /// Consumed by [`IndexPathSpace`](crate::indexing::IndexPathSpace) to decide
    /// whether the local scan/reconcile/live pipeline strips a mount root before
    /// `store::resolve_path`, and to pick the [`ExclusionScope`].
    pub(crate) fn mount_rooted(self) -> bool {
        matches!(
            self,
            IndexVolumeKind::LocalExternal | IndexVolumeKind::Smb | IndexVolumeKind::Mtp
        )
    }

    /// Whether this volume's writes back the single in-memory search index.
    /// Search is single-volume by construction (D7): only the boot disk
    /// (`Local`) feeds it. See `writer::WRITER_GENERATION`.
    pub(crate) fn feeds_search(self) -> bool {
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
/// starts an SMB share. Both funnel through `start_indexing_for`.
pub fn start_indexing(app: &AppHandle) -> Result<(), String> {
    // The boot disk is APFS, so its inodes are trustworthy.
    start_indexing_for(app, ROOT_VOLUME_ID, PathBuf::from("/"), IndexVolumeKind::Local, true)
}

/// Start indexing for a specific volume id and root path.
///
/// `inodes_trustworthy` is the volume's filesystem inode-identity fact, resolved
/// once by the caller (from the volume's `FilesystemKind` for a local external
/// drive; `true` for the boot disk and trait-scanned volumes). It threads to the
/// per-scan `IndexPathSpace` so a FAT/exFAT drive stores `inode: None`.
fn start_indexing_for(
    app: &AppHandle,
    volume_id: &str,
    volume_root: PathBuf,
    kind: IndexVolumeKind,
    inodes_trustworthy: bool,
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
    let initial_freshness = super::freshness::initial_freshness_on_launch(scan_completed, kind.has_event_journal());

    // Launch-as-Stale ⇒ bump `current_epoch` at THIS call site (the pure
    // `initial_freshness_on_launch` has no DB handle and can't bump). A
    // non-journaled (SMB/MTP) index with a completed prior scan loads Stale —
    // we weren't watching while off, so its persisted dirs are stale-but-visible;
    // bumping the epoch makes the read side render them stale (not falsely
    // current) per the honest-sizes model. A journaled local index loads Fresh
    // and does NOT bump (continuity self-heals via FSEvents replay). No writer is
    // running for this volume yet (it spawns inside `resume_or_scan`), so we bump
    // directly on a short-lived write connection — safe, single-writer not yet
    // contended. A bump failure is non-fatal: the read side degrades a missing
    // epoch to "all current", so worst case the launch reads Fresh-looking until
    // the next continuity break.
    if initial_freshness == Some(Freshness::Stale) {
        match IndexStore::open_write_connection(&db_path) {
            Ok(conn) => {
                if let Err(e) = IndexStore::bump_current_epoch(&conn) {
                    log::warn!("start_indexing_for('{volume_id}'): launch epoch bump failed: {e}");
                }
            }
            Err(e) => log::warn!("start_indexing_for('{volume_id}'): launch epoch bump conn failed: {e}"),
        }
    }

    // One freshness `Arc` per volume, shared by the registry instance and the
    // `IndexManager`. The manager fires its scan transitions through this handle
    // directly (no registry re-lock), so a held-registry caller (`force_scan`,
    // the journal-gap fallback) can drive a scan without self-deadlocking.
    let freshness = Arc::new(std::sync::Mutex::new(initial_freshness));

    if try_reserve_initializing_phase(
        volume_id,
        kind,
        init_store,
        Arc::clone(&pool),
        Arc::clone(&pending),
        Arc::clone(&freshness),
    )
    .is_err()
    {
        log::info!("start_indexing: '{volume_id}' already Initializing/Running/ShuttingDown, no-op");
        return Ok(());
    }

    // Announce the registration on the lifecycle bus so a backend subsystem (the
    // importance scheduler) can wire up per-volume subscriptions for a volume that
    // registered AFTER it did its startup sweep — a share mounted mid-session (plan
    // M4 late-registering volumes). The kind rides along so the consumer branches
    // typed (score Local + SMB, exclude MTP), never on the id string. Published
    // once, right after the reservation wins, so an early scan completion still
    // arrives on the (already-subscribed) scan bus afterwards.
    super::lifecycle_bus::publish_volume_registered(volume_id, kind);

    let mut manager = match IndexManager::new_for_kind(
        volume_id.to_string(),
        volume_root,
        app.clone(),
        kind,
        inodes_trustworthy,
        Arc::clone(&freshness),
    ) {
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
    // SMB stores trait-provided inodes and doesn't run the local inode-keyed
    // rename pre-pass, so its inode identity is treated as trustworthy.
    start_indexing_for(app, volume_id, mount_root, IndexVolumeKind::Smb, true)
}

/// Internal MTP-start entry point, called by `mtp_index::start_indexing_for_mtp`
/// once the device is confirmed connected. Funnels into the shared
/// `start_indexing_for` with the `Mtp` kind so the lock-first reservation,
/// load-as-Stale freshness seeding, and `Volume`-trait scan path all apply.
/// `volume_root` is the MTP volume's `mtp://{device}/{storage}` root.
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(crate) fn start_indexing_for_mtp_inner(
    app: &AppHandle,
    volume_id: &str,
    volume_root: PathBuf,
) -> Result<(), String> {
    // MTP reuses the `inode` column for PTP object handles and doesn't run the
    // local rename pre-pass, so its inode identity is treated as trustworthy.
    start_indexing_for(app, volume_id, volume_root, IndexVolumeKind::Mtp, true)
}

/// Internal local-external-start entry point, called by
/// `local_external_index::start_indexing_for_local_external` after the volume is
/// classified as a plain local external drive. Funnels into the shared
/// `start_indexing_for` with the `LocalExternal` kind so the lock-first
/// reservation, load-as-Stale freshness seeding, and the LOCAL guarded-walker + FSEvents
/// scan path all apply. `mount_root` is the drive's mount point (`/Volumes/X`),
/// so the index is mount-rooted (unlike the boot disk's `/`).
///
/// `inodes_trustworthy` is the drive's filesystem inode-identity fact, resolved
/// once by `local_external_index::classify` (from its `FilesystemKind`): `false`
/// for FAT/exFAT so the scan/reconcile/live pipeline stores `inode: None` and the
/// rename pre-pass stays inert (an inode-reused delete+create must never become a
/// false move), `true` for every other local format.
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(crate) fn start_indexing_for_local_external_inner(
    app: &AppHandle,
    volume_id: &str,
    mount_root: PathBuf,
    inodes_trustworthy: bool,
) -> Result<(), String> {
    start_indexing_for(
        app,
        volume_id,
        mount_root,
        IndexVolumeKind::LocalExternal,
        inodes_trustworthy,
    )
}

/// All registered MTP volume ids belonging to `device_id` (one device hosts N
/// storages, each a separate index). Used by the disconnect hook to flip every
/// one of the device's indexes to Stale.
///
/// Matches by the volume id's device-id half (robust `rsplit` via
/// `mtp::identity`, so a `:` in a serial device id doesn't mis-key), NOT a raw
/// prefix — `mtp-AA` must not match `mtp-AAB:1`.
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(crate) fn registered_mtp_volume_ids_for_device(device_id: &str) -> Vec<String> {
    let reg = INDEX_REGISTRY.lock().expect("INDEX_REGISTRY lock poisoned");
    reg.keys()
        .filter(|vid| crate::mtp::identity::device_id_of_volume(vid) == Some(device_id))
        .cloned()
        .collect()
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
    // Take ownership of whatever the instance carries. `Running` hands back the
    // manager (needs a blocking drain before the files go); `Initializing` /
    // `ShuttingDown` carry no live writer to drain but MUST still be removed so
    // the badge goes gray (not a dangling Stale) and the DB is reclaimed —
    // forgetting a re-enabled-but-still-scanning Stale index has to work. Either
    // way we resolve the DB path before dropping the guard.
    enum ClearTarget {
        Running { mgr: Box<IndexManager> },
        NoWriter { db_path: PathBuf },
    }
    let target = {
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
            IndexPhase::Running(mgr) => ClearTarget::Running { mgr },
            IndexPhase::Initializing { store } => {
                // No live writer thread to drain (still in resume_or_scan), but
                // an in-flight start may be mid-`resume_or_scan`: publishing
                // `ShuttingDown` makes it observe the change and shut its
                // half-built manager down (same contract as `stop_indexing`).
                let db_path = store.db_path().to_path_buf();
                reg.remove(volume_id);
                ClearTarget::NoWriter { db_path }
            }
            IndexPhase::ShuttingDown => {
                // Another teardown is already draining this volume. It will
                // remove the instance and (for clear) delete the DB; don't race
                // a second delete. Put the marker back and bail.
                instance.phase = IndexPhase::ShuttingDown;
                log::info!("Drive index clear requested but '{volume_id}' is already shutting down");
                return Ok(());
            }
        }
    };

    // Guard released: run the blocking drain (Running only) without the lock.
    let db_path = match target {
        ClearTarget::Running { mgr } => {
            let mut mgr = mgr;
            let db_path = mgr.db_path().to_path_buf();
            mgr.shutdown();
            // Re-lock only to remove the now-disabled instance.
            {
                let mut reg = INDEX_REGISTRY
                    .lock()
                    .map_err(|e| format!("Failed to lock registry: {e}"))?;
                reg.remove(volume_id);
            }
            db_path
        }
        ClearTarget::NoWriter { db_path } => db_path,
    };

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

/// Force a fresh full scan for a volume (for debug/manual trigger).
///
/// Takes the `Running` manager OUT of the registry under the lock (publishing a
/// transient `ShuttingDown`), DROPS the guard, then runs `start_scan` — whose
/// prelude does blocking I/O (`block_in_place(flush_blocking)`, a space-info
/// query) — off the lock, and finally re-locks only to put the manager back as
/// `Running`. Same drop-the-guard-before-blocking discipline as
/// `stop_indexing`/`clear_index` (DETAILS § "Drop the registry guard before the
/// shutdown drain"): a blocking flush under the global registry lock would
/// freeze every concurrent registry user (the QA-observed UI freeze), on top of
/// the self-deadlock from the freshness firing (now fixed via the manager's own
/// freshness `Arc`). `start_scan`'s spawned tasks capture their own clones and
/// never re-resolve the manager in the registry, so it's safe to run detached.
pub fn force_scan(volume_id: &str) -> Result<(), String> {
    // Take the manager out under the lock (transient `ShuttingDown`), so the
    // blocking rescan prelude runs WITHOUT holding the registry lock.
    let mut mgr = {
        let mut reg = INDEX_REGISTRY.lock().map_err(|e| format!("Lock poisoned: {e}"))?;
        let instance = reg.get_mut(volume_id).ok_or("Indexing not initialized")?;
        match std::mem::replace(&mut instance.phase, IndexPhase::ShuttingDown) {
            IndexPhase::Running(mgr) => mgr,
            other => {
                // Not running (Initializing / ShuttingDown): nothing to force.
                // Put the phase back and report not-initialized, as before.
                instance.phase = other;
                return Err("Indexing not initialized".to_string());
            }
        }
    };

    // Guard released: run the (blocking-prelude) scan start off the lock.
    // `force_rescan` routes by the volume's TYPED kind: a `Local` volume runs the
    // guarded walker (`start_scan`), an SMB/MTP volume walks the `Volume` trait from its share
    // root (`start_volume_scan`). Calling `start_scan` unconditionally here ran
    // the local guarded walker over a network mount — walking nothing and falsely
    // marking the index complete — so a NAS "Rescan now" indexed zero entries.
    let result = mgr.force_rescan("manual start");

    // Re-lock to restore the manager as `Running`. If the instance vanished
    // while we were detached (a concurrent `stop_indexing`/`clear_index` swapped
    // it out), respect that and shut our now-orphaned manager down instead of
    // resurrecting a removed volume.
    let mut reg = INDEX_REGISTRY.lock().map_err(|e| format!("Lock poisoned: {e}"))?;
    match reg.get_mut(volume_id) {
        Some(instance) if matches!(instance.phase, IndexPhase::ShuttingDown) => {
            instance.phase = IndexPhase::Running(mgr);
            result
        }
        _ => {
            drop(reg);
            log::info!("force_scan: '{volume_id}' was torn down during scan start; shutting down the manager");
            mgr.shutdown();
            result
        }
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

/// Snapshot the ready-to-score volume ids WITH their typed kind. The importance and
/// media-index schedulers' startup sweeps use this to branch typed on the kind (score
/// Local + SMB, exclude MTP — plan M4) without re-deriving the kind from the volume-id
/// string (`no-string-matching`). Readiness filter: a registered instance whose
/// freshness is `Fresh` (an authoritative completed scan). `Scanning`/`Stale` volumes
/// are excluded (a `Scanning` one fires `ScanCompleted` on the bus when it finishes; a
/// `Stale` one has nothing to score yet).
///
/// A volume that loaded `Fresh` at launch from its persisted `scan_completed_at` never
/// re-fires a `ScanCompleted`, so a scheduler that only waited on the bus would never
/// act on it (its retained bus value stays `Pending`) — the common restart case. This
/// snapshot is how the sweeps find those volumes; wiring their subscriptions is NOT
/// enough on its own, so each scheduler pairs this with an explicit startup enqueue
/// (media's `kick_all_ready_passes`, importance's `enqueue_initial_full_pass_if_unscored`).
pub(crate) fn ready_volumes_with_kind() -> Vec<(VolumeId, IndexVolumeKind)> {
    let reg = INDEX_REGISTRY.lock().expect("INDEX_REGISTRY lock poisoned");
    reg.iter()
        .filter(|(_, instance)| {
            instance
                .freshness
                .lock()
                .ok()
                .and_then(|f| *f)
                .is_some_and(|f| f == Freshness::Fresh)
        })
        .map(|(vid, instance)| (vid.clone(), instance.kind))
        .collect()
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
///
/// macOS-only: its sole caller is the memory watchdog, which only monitors
/// resident memory on macOS (`mach_task_info`); the non-macOS watchdog is a
/// no-op stub, so this would be dead code there.
#[cfg(target_os = "macos")]
pub(crate) fn stop_all_indexing() {
    for volume_id in all_registered_volume_ids() {
        if let Err(e) = stop_indexing(&volume_id) {
            log::warn!("stop_all_indexing: stop_indexing('{volume_id}') failed: {e}");
        }
    }
    // Tell shared-resident-pool subsystems (media_index enrichment) to yield to the
    // SAME 16 GB ceiling, rather than a second independent budget over one pool.
    super::subsystem_stop::run_subsystem_stop_hooks();
}

/// The typed kind of a registered volume, or `None` if it has no index instance.
///
/// Lets a consumer (the `record_visit` command) branch on the kind — record a
/// visit for a Local/SMB volume, skip an MTP one — without inspecting the
/// volume-id string (`no-string-matching`).
pub(crate) fn volume_kind(volume_id: &str) -> Option<IndexVolumeKind> {
    INDEX_REGISTRY.lock().ok()?.get(volume_id).map(|i| i.kind)
}

/// Test-only: reserve a lightweight `Initializing` index instance for `volume_id`
/// of the given `kind`, backed by a throwaway temp DB (returned so the caller keeps
/// it alive). Stops short of building an `IndexManager` (which needs an
/// `AppHandle`), so `stop_indexing` on it takes the fast `Initializing`-removal arm.
/// Lets cross-module tests (the eject-stop ordering, the unmount cleanup) exercise
/// the REAL registry + `stop_indexing` without a Tauri runtime.
#[cfg(test)]
pub(crate) fn reserve_initializing_index_for_test(volume_id: &str, kind: IndexVolumeKind) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("temp dir for test index");
    let db_path = dir.path().join("test-index.db");
    let store = IndexStore::open(&db_path).expect("open test store");
    let pool = Arc::new(ReadPool::new(db_path.clone()).expect("test read pool"));
    let pending = Arc::new(PendingSizes::new());
    try_reserve_initializing_phase(
        volume_id,
        kind,
        store,
        pool,
        pending,
        Arc::new(std::sync::Mutex::new(Some(Freshness::Fresh))),
    )
    .unwrap_or_else(|_| panic!("reserve {volume_id} must succeed from absent"));
    dir
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
mod tests;
