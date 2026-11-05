//! A volume paused for disk space meeting a start: the reservation takes its slot
//! over, read pool and all, and a pause's own restart respects a stop that landed
//! after its last look (`reservation.rs`, `space_pause.rs`).

use std::collections::HashMap;
use std::sync::Arc;

use super::reservation::try_reserve_initializing_phase_on;
use super::*;
use crate::NoopEventSink;
use crate::indexing::lifecycle::freshness::Freshness;
use crate::indexing::read::enrichment::{ReadPool, get_read_pool_for, install_read_pool, uninstall_read_pool};
use crate::indexing::read::pending_sizes::{PendingSizes, uninstall_pending_sizes};

fn fresh(seed: Option<Freshness>) -> Arc<std::sync::Mutex<Option<Freshness>>> {
    Arc::new(std::sync::Mutex::new(seed))
}

/// A start taking a paused volume's slot over swaps its read pool for the new one
/// in the same critical section: the volume never routes no reads, and the old
/// pool is invalidated. A pause's own restart that finds the key gone refuses: a
/// stop or clear landed after the waiter's last look, and that word stands.
#[test]
fn a_start_takes_a_paused_slot_over_and_a_pause_restart_respects_a_stop() {
    let dir = tempfile::tempdir().expect("temp dir");
    let vid = "paused-takeover";
    let registry: Registry = std::sync::Mutex::new(HashMap::new());
    let old_db = dir.path().join("old.db");
    let _old_store = IndexStore::open(&old_db).expect("old store");
    let old_pool = Arc::new(ReadPool::new(old_db.clone()).expect("old pool"));
    install_read_pool(vid, Arc::clone(&old_pool));
    registry.lock_ignore_poison().insert(
        vid.to_string(),
        IndexInstance {
            phase: IndexPhase::Paused {
                db_path: old_db.clone(),
                why: SpaceStop::DiskFull,
            },
            started_as: StartRequest::for_test(IndexVolumeKind::Local),
            signals: VolumeSignals::new(fresh(Some(Freshness::Paused)), NoopEventSink::shared()),
            work: VolumeWork::for_test(vid),
        },
    );

    let new_db = dir.path().join("new.db");
    let new_store = IndexStore::open(&new_db).expect("new store");
    let new_pool = Arc::new(ReadPool::new(new_db.clone()).expect("new pool"));
    let reserved = try_reserve_initializing_phase_on(
        &registry,
        vid,
        StartRequest::for_test(IndexVolumeKind::Local).after_space_pause(),
        new_store,
        Arc::clone(&new_pool),
        Arc::new(PendingSizes::new()),
        VolumeSignals::new(fresh(Some(Freshness::Fresh)), NoopEventSink::shared()),
    );
    assert!(reserved.is_ok(), "the restart takes the paused slot over");
    assert!(
        registry
            .lock_ignore_poison()
            .get(vid)
            .is_some_and(|i| is_initializing_phase(&i.phase)),
        "the slot now holds the new start"
    );
    assert!(
        get_read_pool_for(vid).is_some_and(|pool| Arc::ptr_eq(&pool, &new_pool)),
        "the new pool routes the volume's reads"
    );
    drop(reserved);
    uninstall_read_pool(vid);
    uninstall_pending_sizes(vid);

    // The same restart, after a stop took the key away: refused.
    let gone: Registry = std::sync::Mutex::new(HashMap::new());
    let late_db = dir.path().join("late.db");
    let late_store = IndexStore::open(&late_db).expect("late store");
    assert!(
        try_reserve_initializing_phase_on(
            &gone,
            vid,
            StartRequest::for_test(IndexVolumeKind::Local).after_space_pause(),
            late_store,
            Arc::new(ReadPool::new(late_db.clone()).expect("late pool")),
            Arc::new(PendingSizes::new()),
            VolumeSignals::new(fresh(None), NoopEventSink::shared()),
        )
        .is_err(),
        "a pause's restart never starts a volume somebody stopped meanwhile"
    );
    assert!(get_read_pool_for(vid).is_none(), "and installs nothing");
}
