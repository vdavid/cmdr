//! Claiming a volume's registry slot.
//!
//! The lock-first `(absent) -> Initializing` check-and-set every start funnels
//! through, plus the phase classifier that tells a start whether its reservation
//! still stands. Apart from `startup.rs` because the reservation carries the whole
//! one-writer-per-DB concurrency contract, and it reads better without 300 lines
//! of bootstrap around it.

use cmdr_fs::ignore_poison::IgnorePoison;
use std::sync::Arc;

use super::{INDEX_REGISTRY, IndexInstance, IndexPhase, Registry, StartRequest, VolumeSignals};
use crate::indexing::hold::VolumeWork;
#[cfg(any(test, feature = "testing"))]
use crate::indexing::lifecycle::freshness::Freshness;
use crate::indexing::lifecycle::manager::StartCause;
use crate::indexing::read::enrichment::{ReadPool, install_read_pool};
use crate::indexing::read::pending_sizes::{PendingSizes, install_pending_sizes};
use crate::indexing::store::IndexStore;
#[cfg(any(test, feature = "testing"))]
use crate::indexing::volume::IndexVolumeKind;

/// Phase classifier used by `start_indexing`'s post-`resume_or_scan` branch.
/// Returns true only while the phase carries the temporary init store. A
/// `stop_indexing` / `clear_index` that swapped the state out from under us during
/// `resume_or_scan` removed the instance, so this answers false and the caller
/// treats it as "phase changed, shut the manager down".
///
/// ⚠️ **It is HALF the check, and ❌ not enough on its own.** The slot a teardown
/// frees can be reserved by a fresh start before we re-lock, and that start's
/// instance is `Initializing` too — so the caller pairs this with "is MY
/// reservation's stop signal still uncancelled" (the teardown cancels it on the way
/// out). Without that half, an old start installs its manager over the newcomer's
/// and two writer threads end up on one database.
///
/// Extracted as a pure helper so the state-machine race fragment is testable
/// without standing up an `IndexManager`.
pub(crate) fn is_initializing_phase(phase: &IndexPhase) -> bool {
    matches!(phase, IndexPhase::Initializing { .. })
}

/// Atomically reserve the `Initializing(store)` phase for `volume_id`. Returns the
/// start's root [`VolumeWork`] when the volume had no registered instance (the only
/// legitimate start); returns `Err(store)` otherwise so the caller can drop the
/// unused store without constructing the heavy `IndexManager`.
///
/// ⚠️ **The work, and the new generation of the volume's hold in it, is minted in
/// the same critical section as the insert.** The instance keeps one share and the
/// start hands the other to the manager it builds. A teardown that meets
/// `Initializing` removes the instance, and its share with it, long before that
/// manager is gone, so the start's share is the only thing still saying a start is
/// working on the volume (`hold.rs`).
///
/// ⚠️ **A refusal is not always a no-op.** A volume that is on its way OUT of the
/// registry still holds its key, and `request` is RECORDED on the transient phase
/// so whoever ends that window starts the volume again
/// ([`IndexPhase::claim_the_restart`]). Bouncing off it instead is what made
/// "turn this drive's indexing off and straight back on" leave the drive dark for
/// the rest of the session. The two live phases (`Initializing`, `Running`) are
/// the real no-op: a start is already in flight, or already finished.
///
/// This is the lock-first guard for `start_indexing`, now per volume id. Two
/// writer threads racing on the same DB share neither their `Arc<AtomicI64>` ID
/// counter nor their `AccumulatorMaps`, which produces PK collisions and
/// inflated `dir_stats`. The transition must be a single atomic check-and-set,
/// not "construct manager then maybe shut down" (which leaks a live writer
/// thread while `resume_or_scan` runs). Keyed per volume, two starts for the
/// *same* volume still can't race, while two *different* volumes start freely.
///
/// A volume [`Paused`](IndexPhase::Paused) for disk space is the one held key a
/// start takes over rather than refuses: nothing runs on it, and swapping its read
/// handles for the new ones here, in one critical section, is what keeps its
/// listings serving sizes across the restart. A pause's own restart
/// ([`StartCause::SpacePauseEnded`]) that finds the key gone refuses instead: a
/// stop or clear landed after its last check.
///
/// On success, and STILL UNDER THE REGISTRY LOCK, publishes the volume's
/// `read_pool`/`pending_sizes` into the read-side tables, so a volume is never
/// visible in the registry without a routable read path and enrichment works from
/// the `Initializing` phase onward. Both `install_*` calls are leaf-lock
/// operations (a hash insert), so taking them under this lock adds no ordering
/// hazard: nothing ever acquires the registry while holding a read-handle table.
///
/// The caller owns the `freshness` `Arc` (it shares a clone with the
/// `IndexManager`, which fires scan transitions through it WITHOUT re-locking
/// the registry); the instance stores the same `Arc`, so the manager and the
/// registry never disagree about freshness.
pub(crate) fn try_reserve_initializing_phase(
    volume_id: &str,
    request: StartRequest,
    store: IndexStore,
    read_pool: Arc<ReadPool>,
    pending_sizes: Arc<PendingSizes>,
    signals: VolumeSignals,
) -> Result<VolumeWork, Box<IndexStore>> {
    try_reserve_initializing_phase_on(
        &INDEX_REGISTRY,
        volume_id,
        request,
        store,
        read_pool,
        pending_sizes,
        signals,
    )
}

/// [`try_reserve_initializing_phase`] against a registry passed in, so a test can
/// drive the check-and-set through a poisoned lock of its own and prove the
/// one-writer-per-DB gate still refuses a volume that is already registered.
pub(super) fn try_reserve_initializing_phase_on(
    registry: &Registry,
    volume_id: &str,
    request: StartRequest,
    store: IndexStore,
    read_pool: Arc<ReadPool>,
    pending_sizes: Arc<PendingSizes>,
    signals: VolumeSignals,
) -> Result<VolumeWork, Box<IndexStore>> {
    let kind = request.kind();
    // Which filesystem this start's root is, read from the host's mount table BEFORE
    // the lock. The generation is asked about by this identity for as long as it
    // lives, ❌ never by its root path: a rename moves the root while the drive stays
    // mounted. Only a local-scanner volume's root is a mount point to name.
    let mount_identity = if kind.uses_local_scanner() {
        crate::indexing::host::volumes::current().mount_identity(request.volume_root())
    } else {
        None
    };
    let mut reg = registry.lock_ignore_poison();
    let took_over_a_pause = match reg.get_mut(volume_id) {
        // A volume paused for disk space holds its key and its read handles, and
        // nothing is running on it: this start takes the slot over, in this one
        // critical section, so the new read pool replaces the old one with no
        // moment where the volume routes no reads (and no competing start can take
        // a slot freed in between). The insert below displaces the paused instance.
        Some(instance) if matches!(instance.phase, IndexPhase::Paused { .. }) => true,
        Some(instance) => {
            if instance.phase.claim_the_restart(request) {
                log::info!("start_indexing: '{volume_id}' is on its way out; this start runs as that finishes");
            } else {
                log::info!("start_indexing: '{volume_id}' is already initializing or running, no-op");
            }
            return Err(Box::new(store));
        }
        // A pause's restart found the volume gone: somebody stopped or cleared it
        // meanwhile, and their word stands.
        None if request.cause() == StartCause::SpacePauseEnded => {
            log::info!("start_indexing: '{volume_id}' isn't paused any more; its pause's restart has nothing to do");
            return Err(Box::new(store));
        }
        None => false,
    };
    let seeded = *signals.freshness.lock_ignore_poison();
    let events = Arc::clone(&signals.events);
    // `install` displaces the paused volume's pool and tracker, if it had them.
    let displaced_pool = crate::indexing::read::enrichment::get_read_pool_for(volume_id);
    install_read_pool(volume_id, read_pool);
    install_pending_sizes(volume_id, pending_sizes);
    let work = VolumeWork::take(volume_id, mount_identity);
    reg.insert(
        volume_id.to_string(),
        IndexInstance {
            phase: IndexPhase::Initializing { store },
            started_as: request.into_recorded(),
            signals,
            work: work.clone(),
        },
    );
    drop(reg);
    if took_over_a_pause {
        if let Some(pool) = displaced_pool {
            pool.invalidate();
        }
        // The badge says paused, and nothing else would tell it otherwise: a seeded
        // value fires no transition. (A `None` seed announces `Scanning` itself,
        // `startup.rs`.)
        if let Some(freshness) = seeded {
            events.emit(crate::IndexEvent::FreshnessChanged {
                volume_id: volume_id.to_string(),
                freshness,
            });
        }
        log::info!("start_indexing: '{volume_id}' takes over its paused index");
    }
    Ok(work)
}

/// Test-only: reserve a lightweight `Initializing` index instance for `volume_id`
/// of the given `kind`, backed by a throwaway temp DB (returned so the caller keeps
/// it alive). Stops short of building an `IndexManager` (which needs an
/// `AppHandle`), so `stop_indexing` on it takes the fast `Initializing`-removal arm.
/// Lets cross-module tests (the eject-stop ordering, the unmount cleanup) exercise
/// the REAL registry + `stop_indexing` without a Tauri runtime.
#[cfg(any(test, feature = "testing"))]
pub fn reserve_initializing_index_for_test(volume_id: &str, kind: IndexVolumeKind) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("temp dir for test index");
    let db_path = dir.path().join("test-index.db");
    let store = IndexStore::open(&db_path).expect("open test store");
    let pool = Arc::new(ReadPool::new(db_path.clone()).expect("test read pool"));
    let pending = Arc::new(PendingSizes::new());
    let work = try_reserve_initializing_phase(
        volume_id,
        StartRequest::for_test(kind),
        store,
        pool,
        pending,
        VolumeSignals::new(
            Arc::new(std::sync::Mutex::new(Some(Freshness::Fresh))),
            crate::NoopEventSink::shared(),
        ),
    )
    .unwrap_or_else(|_| panic!("reserve {volume_id} must succeed from absent"));
    // No start stands behind this slot, so only the instance holds the volume: a
    // stop that frees it finds the volume already let go.
    drop(work);
    dir
}
