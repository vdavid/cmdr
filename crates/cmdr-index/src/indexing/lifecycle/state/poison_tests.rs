//! What the registry answers after a panic poisoned its lock.
//!
//! Every one of these drives a job against a LOCAL registry of its own, never
//! [`INDEX_REGISTRY`](super::INDEX_REGISTRY): poisoning a process-global static is
//! permanent for the process, so it would break every sibling test in the binary.
//! That's the reason the jobs take `&Registry` as a parameter at all.
//!
//! The policy they pin lives in `cmdr_fs::ignore_poison`: this map is a value
//! store, so a background-thread panic must not turn the next drive-start or the
//! next status sweep into a second, app-killing panic.

use std::collections::HashMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::sync::Mutex;

use cmdr_fs::ignore_poison::IgnorePoison;

use super::queries::ready_candidates_on;
use super::reservation::try_reserve_initializing_phase_on;
use super::teardown::remove_instance_and_handles_on;
use super::{IndexInstance, IndexPhase, Registry, VolumeSignals};
use crate::indexing::lifecycle::freshness::Freshness;
use crate::indexing::read::enrichment::{ReadPool, uninstall_read_pool};
use crate::indexing::read::pending_sizes::{PendingSizes, uninstall_pending_sizes};
use crate::indexing::store::IndexStore;
use crate::indexing::volume::IndexVolumeKind;

/// A registry of a test's own, holding one instance per `(id, kind, freshness)`.
///
/// `ShuttingDown` is the phase because it's the one variant that carries nothing:
/// these jobs read the map's keys, kinds, and freshness, never the phase.
fn registry_of(volumes: &[(&str, IndexVolumeKind, Option<Freshness>)]) -> Registry {
    let mut map = HashMap::new();
    for (volume_id, kind, freshness) in volumes {
        map.insert(
            (*volume_id).to_string(),
            IndexInstance {
                phase: IndexPhase::ShuttingDown,
                kind: *kind,
                signals: VolumeSignals::new(Arc::new(Mutex::new(*freshness)), crate::NoopEventSink::shared()),
            },
        );
    }
    Mutex::new(map)
}

/// Panic while holding `registry`, the way a background indexing thread does,
/// leaving the lock poisoned for whoever acquires it next.
fn poison(registry: &Registry) {
    let panicked = catch_unwind(AssertUnwindSafe(|| {
        let _held = registry.lock_ignore_poison();
        panic!("simulated panic while holding the registry lock");
    }));
    assert!(panicked.is_err(), "the closure must have panicked");
    assert!(registry.is_poisoned(), "the panic must have poisoned the registry lock");
}

#[test]
fn a_poisoned_registry_still_reports_its_ready_volumes() {
    let registry = registry_of(&[
        ("root", IndexVolumeKind::Local, Some(Freshness::Fresh)),
        ("smb-share", IndexVolumeKind::Smb, Some(Freshness::Scanning)),
    ]);
    poison(&registry);

    let mut candidates = ready_candidates_on(&registry);
    candidates.sort_by(|a, b| a.0.cmp(&b.0));

    // The kind and the freshness both survive: neither can be torn by a panic, so
    // the schedulers' startup sweep must still find the volumes it has to score.
    assert_eq!(
        candidates,
        vec![
            ("root".to_string(), IndexVolumeKind::Local, true),
            ("smb-share".to_string(), IndexVolumeKind::Smb, false),
        ],
        "a poisoned registry must still answer with the real volumes and kinds"
    );
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn a_poisoned_registry_still_names_a_devices_mtp_volumes() {
    use super::queries::mtp_volume_ids_on;

    let registry = registry_of(&[
        ("mtp-AA:1", IndexVolumeKind::Mtp, None),
        ("mtp-AA:2", IndexVolumeKind::Mtp, None),
        ("mtp-AAB:1", IndexVolumeKind::Mtp, None),
        ("root", IndexVolumeKind::Local, None),
    ]);
    poison(&registry);

    let mut ids = mtp_volume_ids_on(&registry, "mtp-AA");
    ids.sort();

    // Both storages of the disconnected device, and NOT the `mtp-AAB` one whose id
    // merely starts the same way. A disconnect that answered empty here would leave
    // the device's indexes reading Fresh over a phone that's gone.
    assert_eq!(ids, vec!["mtp-AA:1".to_string(), "mtp-AA:2".to_string()]);
}

#[test]
fn a_poisoned_registry_still_gives_up_a_volumes_slot() {
    let registry = registry_of(&[("root", IndexVolumeKind::Local, None)]);
    poison(&registry);

    remove_instance_and_handles_on(&registry, "root");

    assert!(
        !registry.lock_ignore_poison().contains_key("root"),
        "a start-up failure must free the slot through a poisoned lock; leaving it \
         behind is what would block every later start for the volume"
    );
}

/// The three handles a reservation needs, backed by a throwaway database the
/// caller keeps alive.
fn reservation_handles(dir: &std::path::Path) -> (IndexStore, Arc<ReadPool>, Arc<PendingSizes>) {
    let db_path = dir.join("test-index.db");
    let store = IndexStore::open(&db_path).expect("open test store");
    let pool = Arc::new(ReadPool::new(db_path).expect("test read pool"));
    (store, pool, Arc::new(PendingSizes::new()))
}

fn test_signals() -> VolumeSignals {
    VolumeSignals::new(Arc::new(Mutex::new(None)), crate::NoopEventSink::shared())
}

#[test]
fn a_reservation_lands_through_a_poisoned_registry() {
    let volume_id = "poison-test-reserve-free";
    let dir = tempfile::tempdir().expect("temp dir for test index");
    let (store, pool, pending) = reservation_handles(dir.path());

    let registry = registry_of(&[]);
    poison(&registry);

    try_reserve_initializing_phase_on(
        &registry,
        volume_id,
        IndexVolumeKind::Local,
        store,
        pool,
        pending,
        test_signals(),
    )
    .unwrap_or_else(|_| panic!("reserving {volume_id} from absent must succeed"));

    assert!(
        registry.lock_ignore_poison().contains_key(volume_id),
        "the volume must be registered through the poison, or its index never starts"
    );

    // The read handles go into process-global tables, so give them back.
    uninstall_read_pool(volume_id);
    uninstall_pending_sizes(volume_id);
}

#[test]
fn a_poisoned_registry_still_refuses_a_second_reservation() {
    let volume_id = "poison-test-reserve-taken";
    let dir = tempfile::tempdir().expect("temp dir for test index");
    let (store, pool, pending) = reservation_handles(dir.path());

    let registry = registry_of(&[(volume_id, IndexVolumeKind::Local, None)]);
    poison(&registry);

    // The one-writer-per-DB gate is the reason recovering here is safe to begin
    // with: two writer threads on one database share neither their id counter nor
    // their accumulator maps, so they collide on the primary key. A poisoned lock
    // can't make a key wrongly absent (one insert site, every remove final), so the
    // check-and-set answers exactly as it does on a healthy lock.
    let refused = try_reserve_initializing_phase_on(
        &registry,
        volume_id,
        IndexVolumeKind::Local,
        store,
        pool,
        pending,
        test_signals(),
    );

    assert!(
        refused.is_err(),
        "an already-registered volume must be refused through the poison too"
    );
}
