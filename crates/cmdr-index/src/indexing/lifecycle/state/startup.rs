//! Standing a volume's index up: the one path every transport starts through.
//!
//! `start_indexing_for` is the choke point — the master-switch gate, the init
//! store, the read handles, the lock-first reservation, the `IndexManager`, and
//! whatever [`Activation`] says happens next. The per-transport entry points below
//! it differ only in the kind they name.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use super::teardown::remove_instance_and_handles;
use super::walk_database::{evict_an_index_no_walk_can_trust, prepare_database_for_a_walk};
use super::{
    INDEX_REGISTRY, IndexPhase, VolumeSignals, is_initializing_phase, resolved_index_db_path, spawn_failure_supervisor,
    try_reserve_initializing_phase, with_running_manager,
};
use crate::indexing::lifecycle::freshness::Freshness;
use crate::indexing::lifecycle::manager::IndexManager;
use crate::indexing::lifecycle::{freshness, lifecycle_bus, master};
use crate::indexing::read::enrichment::ReadPool;
use crate::indexing::read::pending_sizes::PendingSizes;
use crate::indexing::store::IndexStore;
use crate::indexing::volume::{IndexVolumeKind, ROOT_VOLUME_ID};
use crate::indexing::writer::WriteMessage;

/// What a start does once the volume's writer is up.
///
/// Both arms build the same thing — a database, a writer thread, the read
/// handles, a registered instance — and differ only in what runs next, which is
/// exactly the difference between indexing a drive and being able to write to
/// its index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::indexing::lifecycle) enum Activation {
    /// Index the volume: replay its journal or walk it whole, then keep watching
    /// it. What turning indexing on for a drive means.
    IndexTheVolume,
    /// Stand the index up and stop there: no scan, no watcher. What a
    /// search-driven coverage walk needs — it writes the rows itself, and a full
    /// scan of the drive is precisely what someone searching one folder did not
    /// ask for.
    WriterOnly,
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
pub fn start_indexing() -> Result<(), String> {
    // The boot disk is APFS, so its inodes are trustworthy.
    start_indexing_for(
        ROOT_VOLUME_ID,
        PathBuf::from("/"),
        IndexVolumeKind::Local,
        true,
        Activation::IndexTheVolume,
    )
}

/// Start indexing for a specific volume id and root path.
///
/// `inodes_trustworthy` is the volume's filesystem inode-identity fact, resolved
/// once by the caller (from the volume's `FilesystemKind` for a local external
/// drive; `true` for the boot disk and trait-scanned volumes). It threads to the
/// per-scan `IndexPathSpace` so a FAT/exFAT drive stores `inode: None`.
///
/// `activation` picks what happens once the writer is up: a full index, or the
/// writer alone for a search-driven walk to fill in ([`Activation`]).
pub(in crate::indexing::lifecycle) fn start_indexing_for(
    volume_id: &str,
    volume_root: PathBuf,
    kind: IndexVolumeKind,
    inodes_trustworthy: bool,
    activation: Activation,
) -> Result<(), String> {
    // The master switch is a HARD gate on BACKGROUND indexing, at the one choke
    // point every transport funnels through, so no start or autonomous resume path
    // can slip past it (an SMB reconnect used to, and re-indexed a whole NAS the
    // user had opted out of). Callers that surface a refusal to the UI check
    // `master_enabled()` themselves and return a typed reason; here we no-op, since
    // nothing was promised.
    //
    // ⚠️ `WriterOnly` is carved OUT of it, deliberately (Decision 13): it stands up
    // a database and a writer for a search-driven walk and starts nothing that runs
    // on its own — no scan, no watcher, no resume. "Nothing indexes, anywhere"
    // means nothing indexes UNINVITED; a person searching a folder invited exactly
    // this read, and refusing it would only make their search quietly wrong. ❌
    // Don't collapse this back into a bare `master_enabled()` check; the test
    // `cover::cold_drive_tests::a_search_walks_a_drive_with_the_master_switch_off` is there to
    // catch it.
    if activation == Activation::IndexTheVolume && !master::master_enabled() {
        log::info!("start_indexing: refusing '{volume_id}' ({kind:?}), drive indexing is off in settings");
        return Ok(());
    }
    log::info!("start_indexing: begin for '{volume_id}' ({kind:?})");
    // The sink the host installed at startup. Everything below this line reports
    // through the trait, so no indexing code names Tauri to say something.
    let events = crate::indexing::host::events::current();
    crate::indexing::resources::memory_watchdog::start(Arc::clone(&events));

    // Lock-first reservation, per volume id. We open the init store and the
    // read-path handles, then atomically claim the `(absent) -> Initializing`
    // transition BEFORE constructing the heavy `IndexManager`. If this volume is
    // already initializing or running, this call becomes a no-op — without the
    // per-volume guard, two writers race on the same DB (each owns its own
    // `Arc<AtomicI64>` ID counter and `AccumulatorMaps`), producing PK
    // collisions and inflated `dir_stats`.
    let db_path = resolved_index_db_path(volume_id)?;
    if activation == Activation::WriterOnly {
        evict_an_index_no_walk_can_trust(volume_id, &db_path);
    }
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
    // A writer-only start never replays the journal, so it can't inherit the Fresh
    // a replay earns: an index it didn't verify loads Stale, exactly like a
    // non-journaled one. (A never-scanned volume loads `None` either way, which is
    // what a cold drive gets.)
    let journaled = activation == Activation::IndexTheVolume && kind.has_event_journal();
    let initial_freshness = freshness::initial_freshness_on_launch(scan_completed, journaled);

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

    // A writer-only start is the only thing that will ever touch this database
    // before rows are written into it, so it does what a scan start would
    // otherwise have done for it. Same short-lived write connection as the bump
    // above, for the same reason: no writer is running yet.
    if activation == Activation::WriterOnly {
        prepare_database_for_a_walk(volume_id, &db_path, &volume_root);
    }

    // One set of shared handles per volume, held by BOTH the registry instance
    // and the `IndexManager`: freshness, the sink, and the cancellation root. The
    // manager fires its scan transitions through the freshness handle directly
    // (no registry re-lock), so a held-registry caller (`force_scan`, the
    // journal-gap fallback) can drive a scan without self-deadlocking.
    let signals = VolumeSignals::new(Arc::new(std::sync::Mutex::new(initial_freshness)), Arc::clone(&events));

    if try_reserve_initializing_phase(
        volume_id,
        kind,
        init_store,
        Arc::clone(&pool),
        Arc::clone(&pending),
        signals.clone(),
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
    lifecycle_bus::publish_volume_registered(volume_id, kind);

    let mut manager = match IndexManager::new_for_kind(
        volume_id.to_string(),
        volume_root,
        db_path.clone(),
        kind,
        inodes_trustworthy,
        signals,
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

    let scan_result = match activation {
        Activation::IndexTheVolume => manager.resume_or_scan(),
        // Nothing to resume and nothing to scan: the walk that asked for this
        // writer is the only thing that will write through it.
        Activation::WriterOnly => Ok(()),
    };
    // Read before the manager moves into the registry: a volume the phase machine
    // is about to cover needs its branches back first, exactly like a search-built
    // one, and then the machine started on the far side of that.
    let phased = manager.awaits_its_phases();

    // Clone the writer before moving manager into the registry, so we can hand
    // it to the maintenance timer if startup succeeds.
    let writer_for_maintenance = manager.writer.clone();
    // Clone the writer's fatal-failure signal so the supervisor can watch it once
    // the volume is Running. Captured here (before `manager` moves into the
    // registry). The signal is one-shot, so a failure that already tripped during
    // the initial scan is still caught when the supervisor spawns below.
    let failure_signal = manager.writer.failure_signal();

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

            // A search-built index is the one shape that has coverage and no
            // watcher, so this is where its walked branches come back under one. A
            // phased volume is the second: it comes back partially covered, and
            // without this its covered ground would be unwatched for the rest of the
            // session with no epoch bump to admit it.
            if activation == Activation::WriterOnly || phased {
                resume_branch_watch(volume_id);
            }
            // ⚠️ ORDER. The machine's first walk starts a watcher of its own, and
            // `ensure_branch_watch` returns early when one is already running — so a
            // machine started before the line above would take the `resuming = true`
            // path with it, and the epoch bump for a gap too wide to replay would
            // never fire. Last session's covered rows would then render as CURRENT
            // when nothing verified them. ❌ Moving `branches::resumed_for` earlier is
            // not an equivalent fix: it restores the branch set but not the bump.
            with_running_manager(volume_id, |mgr| mgr.start_phases());

            // Watch for a fatal storage failure: if the writer trips its signal, the
            // supervisor fails this volume (stop + `Failed` phase) instead of letting
            // it log-and-retry forever. Spawned now that the volume is `Running` so
            // `fail_index` can tear the manager down out of the registry.
            spawn_failure_supervisor(Arc::clone(&events), volume_id.to_string(), failure_signal);

            // Periodic DB maintenance every 30 s: reclaim free pages from
            // deletes/rescans (`IncrementalVacuum`), truncate the WAL file so its
            // high-water mark doesn't sit on disk (`WalCheckpoint`), and offer back
            // any ground a walk gave up on whose retry window has elapsed
            // (`ClearAbandonedIfDue`). All stop automatically when the writer
            // channel closes.
            //
            // The last one is a 30 s TICK, not a 30 s policy: the window it consults
            // is hours long, per volume, and persisted, so a tick that isn't due
            // costs one `meta` read (`writer/abandoned_retry.rs`).
            crate::indexing::host::runtime::spawn(async move {
                loop {
                    tokio::time::sleep(Duration::from_secs(30)).await;
                    if writer_for_maintenance.send(WriteMessage::IncrementalVacuum).is_err() {
                        break;
                    }
                    if writer_for_maintenance.send(WriteMessage::WalCheckpoint).is_err() {
                        break;
                    }
                    if writer_for_maintenance.send(WriteMessage::ClearAbandonedIfDue).is_err() {
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

/// Bring a volume's walk-covered branches back under a watcher, on the instance
/// that just came up for them.
///
/// This is where "the branch set survives a restart" is cashed in: a search-built
/// index registers, its persisted branches load, and the watcher starts replaying
/// from where the last session's stream left off. Nothing at LAUNCH does this,
/// deliberately — an unregistered volume answers neither sizes nor coverage
/// questions, so the first moment that coverage can be read is the moment its
/// index comes up, and that's the moment to make it live again.
fn resume_branch_watch(volume_id: &str) {
    with_running_manager(volume_id, |mgr| {
        let conn = match IndexStore::open_read_connection(mgr.db_path()) {
            Ok(conn) => conn,
            Err(e) => {
                log::warn!("Branch watch: can't read '{volume_id}' branches back: {e}");
                return;
            }
        };
        crate::indexing::watch::branches::resumed_for(volume_id, &mgr.path_space(), &conn);
        mgr.ensure_branch_watch(true);
    });
}

/// Internal SMB-start entry point, called by `smb_index::start_indexing_for_smb`
/// AFTER the direct-smb2 gate has passed. Funnels into the shared
/// `start_indexing_for` with the `Smb` kind so the lock-first reservation,
/// load-as-Stale freshness seeding, and `Volume`-trait scan path all apply.
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(crate) fn start_indexing_for_smb_inner(volume_id: &str, mount_root: PathBuf) -> Result<(), String> {
    // SMB stores trait-provided inodes and doesn't run the local inode-keyed
    // rename pre-pass, so its inode identity is treated as trustworthy.
    start_indexing_for(
        volume_id,
        mount_root,
        IndexVolumeKind::Smb,
        true,
        Activation::IndexTheVolume,
    )
}

/// Internal MTP-start entry point, called by `mtp_index::start_indexing_for_mtp`
/// once the device is confirmed connected. Funnels into the shared
/// `start_indexing_for` with the `Mtp` kind so the lock-first reservation,
/// load-as-Stale freshness seeding, and `Volume`-trait scan path all apply.
/// `volume_root` is the MTP volume's `mtp://{device}/{storage}` root.
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(crate) fn start_indexing_for_mtp_inner(volume_id: &str, volume_root: PathBuf) -> Result<(), String> {
    // MTP reuses the `inode` column for PTP object handles and doesn't run the
    // local rename pre-pass, so its inode identity is treated as trustworthy.
    start_indexing_for(
        volume_id,
        volume_root,
        IndexVolumeKind::Mtp,
        true,
        Activation::IndexTheVolume,
    )
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
    volume_id: &str,
    mount_root: PathBuf,
    inodes_trustworthy: bool,
) -> Result<(), String> {
    start_indexing_for(
        volume_id,
        mount_root,
        IndexVolumeKind::LocalExternal,
        inodes_trustworthy,
        Activation::IndexTheVolume,
    )
}
