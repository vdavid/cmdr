use super::*;
use crate::indexing::enrichment::get_read_pool_for;

/// Every `IndexVolumeKind`, so a new variant can't be added without deciding
/// its capabilities here.
const ALL_KINDS: [IndexVolumeKind; 4] = [
    IndexVolumeKind::Local,
    IndexVolumeKind::LocalExternal,
    IndexVolumeKind::Smb,
    IndexVolumeKind::Mtp,
];

/// The five capability axes must match the plan's table exactly. Each tuple is
/// `(uses_local_scanner, is_trait_scanned, has_event_journal, mount_rooted,
/// feeds_search)`.
#[test]
fn capability_axes_match_the_table() {
    let expected = |kind: IndexVolumeKind| -> (bool, bool, bool, bool, bool) {
        (
            kind.uses_local_scanner(),
            kind.is_trait_scanned(),
            kind.has_event_journal(),
            kind.mount_rooted(),
            kind.feeds_search(),
        )
    };

    // (local_scanner, trait_scanned, event_journal, mount_rooted, feeds_search)
    assert_eq!(expected(IndexVolumeKind::Local), (true, false, true, false, true));
    assert_eq!(
        expected(IndexVolumeKind::LocalExternal),
        (true, false, false, true, false)
    );
    assert_eq!(expected(IndexVolumeKind::Smb), (false, true, false, true, false));
    assert_eq!(expected(IndexVolumeKind::Mtp), (false, true, false, true, false));
}

/// `uses_local_scanner` and `is_trait_scanned` are exact complements: every
/// kind is scanned by exactly one of the two pipelines, so they can't silently
/// drift (a new variant landing in neither, or both, fails here).
#[test]
fn scanner_axes_partition_the_enum() {
    for kind in ALL_KINDS {
        assert_ne!(
            kind.uses_local_scanner(),
            kind.is_trait_scanned(),
            "{kind:?} must be scanned by exactly one pipeline"
        );
    }
}

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
            try_reserve_initializing_phase(name, IndexVolumeKind::Local, store, pool, pending, fresh(None)).is_ok(),
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

    assert!(try_reserve_initializing_phase("vol-a", IndexVolumeKind::Local, s1, p1, pe1, fresh(None)).is_ok());
    assert!(try_reserve_initializing_phase("vol-b", IndexVolumeKind::Local, s2, p2, pe2, fresh(None)).is_ok());
    assert!(is_active("vol-a"));
    assert!(is_active("vol-b"));
    // Each volume routes to ITS OWN pool, never the other's (no cross-talk).
    assert!(get_read_pool_for("vol-a").is_some() && get_read_pool_for("vol-b").is_some());

    // A second reservation for vol-a must fail (would spawn a second writer
    // on the same DB) while vol-b is untouched.
    let (s1b, p1b, pe1b) = mk("vol-a");
    assert!(
        try_reserve_initializing_phase("vol-a", IndexVolumeKind::Local, s1b, p1b, pe1b, fresh(None)).is_err(),
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

/// REGRESSION (QA-frozen-app self-deadlock): the scan-start freshness firing
/// must NOT re-lock `INDEX_REGISTRY`, so a caller that already holds the
/// registry lock (the real `force_scan` → `mgr.start_scan` → fire-`ScanStarted`
/// chain) can fire it without self-deadlocking on the non-recursive mutex.
///
/// We reproduce the cycle's exact shape WITHOUT standing up a full
/// `IndexManager`: acquire the global `INDEX_REGISTRY` lock (as `force_scan`
/// does), then — still holding it — fire the scan-start transition through the
/// `Arc`-direct seam (`apply_freshness_event_on`), exactly as the manager now
/// does via `self.freshness`. The whole thing runs on a watchdog thread; if
/// the firing re-locked the registry (the pre-fix `apply_freshness_event`
/// path), this would hang forever and the watchdog would fire. It returns
/// promptly, and the transition still lands (Stale → Scanning).
///
/// Pre-fix, swapping the body to `apply_freshness_event(vid, ScanStarted)`
/// under the held lock deadlocks (the watchdog trips) — a genuine red→green.
#[test]
fn scan_start_freshness_firing_does_not_relock_the_registry() {
    use std::sync::mpsc;

    let _guard = INDEX_REGISTRY_TEST_GUARD.lock().unwrap_or_else(|e| e.into_inner());
    clear_registry_and_pools();
    INDEX_REGISTRY.lock().unwrap().remove("deadlock-test");

    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = dir.path().join("index-deadlock-test.db");
    let store = IndexStore::open(&db_path).expect("open store");
    let pool = Arc::new(ReadPool::new(db_path.clone()).expect("pool"));
    let pending = Arc::new(PendingSizes::new());
    let freshness = fresh(Some(Freshness::Stale));
    assert!(
        try_reserve_initializing_phase(
            "deadlock-test",
            IndexVolumeKind::Local,
            store,
            pool,
            pending,
            Arc::clone(&freshness)
        )
        .is_ok(),
        "reserve must succeed"
    );

    // Run the held-lock firing on a watchdog thread so a deadlock can't wedge
    // the test runner forever — it surfaces as a timeout instead.
    let (done_tx, done_rx) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        // Hold the registry lock, exactly as `force_scan` does across
        // `mgr.start_scan`.
        let _reg = INDEX_REGISTRY.lock().expect("registry lock");
        // Fire the scan-start transition through the Arc-direct seam — the
        // manager's `self.freshness` path. This must NOT touch the registry.
        apply_freshness_event_on(&freshness, "deadlock-test", FreshnessEvent::ScanStarted);
        let _ = done_tx.send(());
        // Drop `_reg` here, after signalling: the assertion below proves we
        // got this far without blocking on the lock we already hold.
    });

    // Before the fix, the firing re-locks `INDEX_REGISTRY` and hangs forever;
    // the watchdog would never receive the signal. 5 s is generous for a pure
    // in-memory transition.
    assert!(
        done_rx.recv_timeout(Duration::from_secs(5)).is_ok(),
        "scan-start freshness firing deadlocked while the registry lock was held \
         (it must NOT re-lock INDEX_REGISTRY)"
    );
    worker.join().expect("watchdog thread must not panic");

    // The transition still landed: Stale → Scanning.
    assert_eq!(
        get_freshness("deadlock-test"),
        Some(Freshness::Scanning),
        "the scan-start firing must still flip Stale → Scanning"
    );

    INDEX_REGISTRY.lock().unwrap().remove("deadlock-test");
    clear_registry_and_pools();
}

/// Freshness rides the registry instance and transitions through the pure
/// state machine via `apply_freshness_event`. This pins the registry-level
/// wiring (the path the live watcher uses): a volume reserved Stale (the
/// load-as-Stale-on-launch case) goes Stale → Scanning → Fresh, and the
/// watcher-died event flips Fresh → Stale. The pure transitions
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
        try_reserve_initializing_phase(
            "smb-fresh-test",
            IndexVolumeKind::Smb,
            store,
            pool,
            pending,
            fresh(Some(Freshness::Stale))
        )
        .is_ok(),
        "reserve must succeed"
    );
    assert_eq!(get_freshness("smb-fresh-test"), Some(Freshness::Stale), "loads Stale");

    // A rescan begins ⇒ Scanning.
    apply_freshness_event("smb-fresh-test", FreshnessEvent::ScanStarted);
    assert_eq!(get_freshness("smb-fresh-test"), Some(Freshness::Scanning));

    // Clean completion ⇒ Fresh.
    apply_freshness_event("smb-fresh-test", FreshnessEvent::ScanCompleted);
    assert_eq!(get_freshness("smb-fresh-test"), Some(Freshness::Fresh));

    // Live-watch path: a watcher death flips Fresh ⇒ Stale.
    apply_freshness_event("smb-fresh-test", FreshnessEvent::WatcherDied);
    assert_eq!(get_freshness("smb-fresh-test"), Some(Freshness::Stale));

    // An absent volume has no freshness, and events on it are no-ops.
    assert_eq!(get_freshness("never-registered"), None);
    apply_freshness_event("never-registered", FreshnessEvent::ScanCompleted);
    assert_eq!(get_freshness("never-registered"), None);

    INDEX_REGISTRY.lock().unwrap().remove("smb-fresh-test");
    clear_registry_and_pools();
}

/// The disconnect-vs-cancel completion split, at the registry level (the
/// full `start_volume_scan` completion handler needs an `AppHandle`, so it
/// stays under integration; this pins the two state actions it dispatches):
///
/// - DISCONNECT keeps the instance and marks it Stale (so the honest partial
///   is still served), via `apply_freshness_event(WatcherDied)` — NOT a
///   reset. The instance stays active and routable.
/// - USER CANCEL discards via `reset_to_not_indexed`, which removes the
///   instance ⇒ gray.
///
/// `bump_current_epoch_for` is a safe no-op on a non-`Running` (here
/// `Initializing`) or absent volume — the scan-start funnel bumps via its own
/// flushed writer send, and the disconnect branch runs while `Running`.
#[test]
fn disconnect_keeps_instance_stale_user_cancel_resets_to_gray() {
    let _guard = INDEX_REGISTRY_TEST_GUARD.lock().unwrap_or_else(|e| e.into_inner());
    clear_registry_and_pools();
    INDEX_REGISTRY.lock().unwrap().remove("smb-disco-test");

    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = dir.path().join("index-smb-disco-test.db");
    let store = IndexStore::open(&db_path).expect("open store");
    let pool = Arc::new(ReadPool::new(db_path.clone()).expect("pool"));
    let pending = Arc::new(PendingSizes::new());

    // Reserve, then drive to Fresh as if a scan just completed.
    assert!(
        try_reserve_initializing_phase(
            "smb-disco-test",
            IndexVolumeKind::Smb,
            store,
            pool,
            pending,
            fresh(Some(Freshness::Stale))
        )
        .is_ok()
    );
    apply_freshness_event("smb-disco-test", FreshnessEvent::ScanStarted);
    apply_freshness_event("smb-disco-test", FreshnessEvent::ScanCompleted);
    assert_eq!(get_freshness("smb-disco-test"), Some(Freshness::Fresh));

    // A non-`Running` / absent volume's epoch bump must not panic.
    bump_current_epoch_for("smb-disco-test"); // Initializing ⇒ no-op
    bump_current_epoch_for("never-registered"); // absent ⇒ no-op

    // DISCONNECT branch: keep the instance, mark Stale.
    apply_freshness_event("smb-disco-test", FreshnessEvent::WatcherDied);
    assert_eq!(
        get_freshness("smb-disco-test"),
        Some(Freshness::Stale),
        "a disconnect keeps the instance and marks it Stale (honest partial still served)"
    );
    assert!(
        is_active("smb-disco-test"),
        "the disconnect branch must NOT remove the instance"
    );
    assert!(
        get_read_pool_for("smb-disco-test").is_some(),
        "the ReadPool stays installed so sizes are still served"
    );

    // USER CANCEL branch: reset to gray (instance gone).
    reset_to_not_indexed("smb-disco-test");
    assert_eq!(
        get_freshness("smb-disco-test"),
        None,
        "user cancel resets to gray (no instance ⇒ no freshness)"
    );
    assert!(
        !is_active("smb-disco-test"),
        "reset_to_not_indexed removes the instance"
    );

    clear_registry_and_pools();
}

/// Forgetting (`clear_index`) a Stale external index must transition the
/// volume to gray/disabled, not leave a dangling Stale
/// badge, AND delete the DB from disk. The badge goes gray because removal
/// drops the registry instance, so `get_freshness` returns `None` (the
/// absence-of-instance = gray model). Exercises the `Initializing`-phase
/// `clear_index` path (a re-enabled-but-still-scanning Stale index): pre-fix,
/// that path early-returned, leaving the instance AND the DB behind.
#[test]
fn forget_stale_index_transitions_to_gray_and_deletes_db() {
    let _guard = INDEX_REGISTRY_TEST_GUARD.lock().unwrap_or_else(|e| e.into_inner());
    clear_registry_and_pools();
    INDEX_REGISTRY.lock().unwrap().remove("smb-forget-test");

    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = dir.path().join("index-smb-forget-test.db");
    let store = IndexStore::open(&db_path).expect("open store");
    let pool = Arc::new(ReadPool::new(db_path.clone()).expect("pool"));
    let pending = Arc::new(PendingSizes::new());

    // Reserve as Stale (the load-as-Stale-on-launch case, then re-enabled so
    // it's mid-scan / Initializing).
    assert!(
        try_reserve_initializing_phase(
            "smb-forget-test",
            IndexVolumeKind::Smb,
            store,
            pool,
            pending,
            fresh(Some(Freshness::Stale))
        )
        .is_ok(),
        "reserve must succeed"
    );
    assert_eq!(get_freshness("smb-forget-test"), Some(Freshness::Stale), "loads Stale");
    assert!(db_path.exists(), "DB file exists before forget");

    // Forget it.
    clear_index("smb-forget-test").expect("clear_index must succeed");

    // Badge goes gray (no instance ⇒ no freshness), and the DB is gone.
    assert_eq!(
        get_freshness("smb-forget-test"),
        None,
        "forgetting a Stale index must transition it to gray, not a dangling Stale"
    );
    assert!(!is_active("smb-forget-test"), "the instance must be removed");
    assert!(!db_path.exists(), "forget must delete the index DB from disk");

    clear_registry_and_pools();
}

/// Disconnect-storm resilience: rapidly connect/scan/disconnect/forget
/// two external volumes many times must never crash, wedge the registry, or
/// leave a dangling instance/freshness. Mirrors `tests/stress_tests_lifecycle.rs`'s
/// repeated-cycle philosophy at the registry-lifecycle level (the seam where
/// SMB/MTP churn actually lives: reserve → ScanStarted → ScanCompleted →
/// WatcherDied(disconnect) → forget/disable).
///
/// Each round alternates the teardown between `clear_index` (forget: delete
/// DB) and `stop_indexing` (disable: keep DB), and alternates which of the
/// two volume ids leads, so an interleave can't hide. After every round both
/// volumes must be fully gray (no instance, no freshness); after the storm
/// the registry must be empty of these ids and re-reservable (not wedged).
#[test]
fn disconnect_storm_two_volumes_never_wedges_the_registry() {
    let _guard = INDEX_REGISTRY_TEST_GUARD.lock().unwrap_or_else(|e| e.into_inner());
    clear_registry_and_pools();
    for vid in ["smb-storm", "mtp-storm:65537"] {
        INDEX_REGISTRY.lock().unwrap().remove(vid);
    }

    let dir = tempfile::tempdir().expect("temp dir");
    // Reserve a volume freshly as Stale (the load-as-Stale-on-launch case),
    // re-opening its DB each round (forget deletes it between rounds).
    let reserve_stale = |vid: &str| {
        let db_path = dir.path().join(format!("index-{vid}.db"));
        let store = IndexStore::open(&db_path).expect("open store");
        let pool = Arc::new(ReadPool::new(db_path.clone()).expect("pool"));
        let pending = Arc::new(PendingSizes::new());
        assert!(
            try_reserve_initializing_phase(
                vid,
                IndexVolumeKind::Smb,
                store,
                pool,
                pending,
                fresh(Some(Freshness::Stale))
            )
            .is_ok(),
            "reserve {vid} must succeed (registry not wedged)"
        );
    };

    const ROUNDS: usize = 20;
    let vids = ["smb-storm", "mtp-storm:65537"];
    for round in 0..ROUNDS {
        // Alternate which volume leads, so connect/disconnect ordering varies.
        let ordered: Vec<&str> = if round % 2 == 0 {
            vids.to_vec()
        } else {
            vids.iter().rev().copied().collect()
        };

        for vid in &ordered {
            reserve_stale(vid);
            // A rescan begins and completes: Stale → Scanning → Fresh.
            apply_freshness_event(vid, FreshnessEvent::ScanStarted);
            assert_eq!(get_freshness(vid), Some(Freshness::Scanning), "round {round}: scanning");
            apply_freshness_event(vid, FreshnessEvent::ScanCompleted);
            assert_eq!(get_freshness(vid), Some(Freshness::Fresh), "round {round}: fresh");
            // The device disconnects / SMB session drops: Fresh → Stale.
            apply_freshness_event(vid, FreshnessEvent::WatcherDied);
            assert_eq!(
                get_freshness(vid),
                Some(Freshness::Stale),
                "round {round}: stale on disconnect"
            );
        }

        // Tear both down. Alternate forget (clear_index, deletes DB) vs.
        // disable (stop_indexing, keeps DB) so both teardown drains churn.
        for vid in &ordered {
            if round % 2 == 0 {
                clear_index(vid).expect("clear_index must not fail under churn");
            } else {
                stop_indexing(vid).expect("stop_indexing must not fail under churn");
            }
            // Either way the badge must be gray: no instance ⇒ no freshness.
            assert_eq!(
                get_freshness(vid),
                None,
                "round {round}: {vid} must be gray after teardown"
            );
            assert!(!is_active(vid), "round {round}: {vid} instance must be gone");
        }
    }

    // The registry isn't wedged: both ids are absent and re-reservable.
    {
        let reg = INDEX_REGISTRY.lock().unwrap();
        for vid in vids {
            assert!(!reg.contains_key(vid), "{vid} must not linger in the registry");
        }
    }
    reserve_stale("smb-storm");
    assert!(
        is_active("smb-storm"),
        "registry still accepts a fresh reservation after the storm"
    );

    clear_registry_and_pools();
}

/// The startup-sweep source (the importance scheduler's `start` sweeps this):
/// a volume that loaded `Fresh` at launch — from its persisted
/// `scan_completed_at`, WITHOUT re-firing a `ScanCompleted` event — must still
/// be surfaced by `ready_volumes_with_kind`, or a bus-only scheduler would never
/// score it (the common restart case, plan Decision 4). A `Scanning`/`Stale`
/// volume is excluded (a `Scanning` one fires the bus when it finishes).
#[test]
fn ready_volumes_with_kind_surfaces_a_fresh_at_launch_volume() {
    let _guard = INDEX_REGISTRY_TEST_GUARD.lock().unwrap_or_else(|e| e.into_inner());
    clear_registry_and_pools();
    for vid in ["sweep-fresh", "sweep-stale", "sweep-scanning"] {
        INDEX_REGISTRY.lock().unwrap().remove(vid);
    }

    let dir = tempfile::tempdir().expect("temp dir");
    let reserve = |vid: &str, initial: Freshness| {
        let db_path = dir.path().join(format!("index-{vid}.db"));
        let store = IndexStore::open(&db_path).expect("open store");
        let pool = Arc::new(ReadPool::new(db_path.clone()).expect("pool"));
        let pending = Arc::new(PendingSizes::new());
        assert!(
            try_reserve_initializing_phase(vid, IndexVolumeKind::Local, store, pool, pending, fresh(Some(initial)))
                .is_ok()
        );
    };

    // A Fresh-at-launch volume (loaded from a persisted completed scan), plus a
    // Stale and a Scanning one that must NOT be swept.
    reserve("sweep-fresh", Freshness::Fresh);
    reserve("sweep-stale", Freshness::Stale);
    reserve("sweep-scanning", Freshness::Scanning);

    let ready: Vec<VolumeId> = ready_volumes_with_kind().into_iter().map(|(vid, _)| vid).collect();
    assert!(
        ready.iter().any(|v| v == "sweep-fresh"),
        "a Fresh-at-launch volume must be swept (it never re-fires ScanCompleted)"
    );
    assert!(
        !ready.iter().any(|v| v == "sweep-stale"),
        "a Stale volume has no authoritative scan to score yet"
    );
    assert!(
        !ready.iter().any(|v| v == "sweep-scanning"),
        "a Scanning volume will fire ScanCompleted on the bus when it finishes"
    );

    clear_registry_and_pools();
}

/// The scan-completion chokepoint publishes on the lifecycle bus: firing
/// `ScanCompleted` through `apply_freshness_event_on` (both the local and
/// network paths funnel here) must advance the bus so the importance scheduler
/// sees it — even for a late subscriber (the `watch` retains the last value).
#[test]
fn scan_completed_publishes_on_the_lifecycle_bus() {
    use super::super::lifecycle_bus::{ScanState, subscribe};

    let freshness = fresh(Some(Freshness::Scanning));
    // Fire completion through the neutral chokepoint (no registry needed — the
    // publish keys off the volume id directly).
    apply_freshness_event_on(&freshness, "bus-chokepoint-test", FreshnessEvent::ScanCompleted);

    // A subscriber created AFTER the publish still sees the completion (the
    // late-subscriber replay the scheduler relies on).
    let rx = subscribe("bus-chokepoint-test");
    assert!(
        matches!(*rx.borrow(), ScanState::Completed { .. }),
        "ScanCompleted through the chokepoint must publish on the bus"
    );
}

/// Wrap an initial freshness in the `Arc<Mutex<…>>` the reservation now
/// takes (the manager and the registry share this same handle in
/// production).
fn fresh(initial: Option<Freshness>) -> Arc<std::sync::Mutex<Option<Freshness>>> {
    Arc::new(std::sync::Mutex::new(initial))
}

/// Reset every registry-backed test global: the instance map plus the root
/// read-path globals (which live outside the map).
fn clear_registry_and_pools() {
    INDEX_REGISTRY.lock().unwrap().clear();
    uninstall_read_pool(ROOT_VOLUME_ID);
    uninstall_pending_sizes(ROOT_VOLUME_ID);
}

/// Tests that mutate `INDEX_REGISTRY` serialize on this guard (mirrors
/// `tests/integration_tests.rs`'s `INDEXING_TEST_GUARD`).
static INDEX_REGISTRY_TEST_GUARD: LazyLock<std::sync::Mutex<()>> = LazyLock::new(|| std::sync::Mutex::new(()));
