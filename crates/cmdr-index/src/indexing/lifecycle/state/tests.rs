use super::reservation::try_reserve_initializing_phase_on;
use super::*;
use crate::NoopEventSink;
use crate::indexing::lifecycle::freshness::FreshnessEvent;
use crate::indexing::read::enrichment::{ReadPool, get_read_pool_for, uninstall_read_pool};
use crate::indexing::read::pending_sizes::{PendingSizes, uninstall_pending_sizes};
use crate::indexing::volume::ROOT_VOLUME_ID;
use std::time::Duration;

/// The read path's skip-vs-route gate is "does `get_read_pool_for` return a
/// pool?". An unregistered volume must return `None` (so its listings skip
/// before any DB work, exactly like the old `should_exclude` early-return); a
/// reserved one returns its own pool. Reserving installs the pool, so the gate
/// flips on; teardown withdraws it.
#[test]
fn read_pool_routing_tracks_registration() {
    let _guard = INDEX_REGISTRY_TEST_GUARD.lock().unwrap_or_else(|e| e.into_inner());
    clear_registry_and_pools();

    let indexed = |vid: &str| get_read_pool_for(vid).is_some();

    assert!(!indexed("root"), "no pool => not indexed");
    assert!(!indexed("smb-nas"), "absent key => not indexed");

    // Reserve root and a non-root volume; reservation installs each one's pool,
    // so both must then route.
    let dir = tempfile::tempdir().expect("temp dir");
    let reserve = |name: &str| {
        let db_path = dir.path().join(format!("{name}.db"));
        let store = IndexStore::open(&db_path).expect("open store");
        let pool = Arc::new(ReadPool::new(db_path.clone()).expect("pool"));
        let pending = Arc::new(PendingSizes::new());
        assert!(
            try_reserve_initializing_phase(
                name,
                StartRequest::for_test(IndexVolumeKind::Local),
                store,
                pool,
                pending,
                VolumeSignals::new(fresh(None), NoopEventSink::shared()),
            )
            .is_ok(),
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

    assert!(
        try_reserve_initializing_phase(
            "vol-a",
            StartRequest::for_test(IndexVolumeKind::Local),
            s1,
            p1,
            pe1,
            VolumeSignals::new(fresh(None), NoopEventSink::shared()),
        )
        .is_ok()
    );
    assert!(
        try_reserve_initializing_phase(
            "vol-b",
            StartRequest::for_test(IndexVolumeKind::Local),
            s2,
            p2,
            pe2,
            VolumeSignals::new(fresh(None), NoopEventSink::shared()),
        )
        .is_ok()
    );
    assert!(is_active("vol-a"));
    assert!(is_active("vol-b"));
    // Each volume routes to ITS OWN pool, never the other's (no cross-talk).
    assert!(get_read_pool_for("vol-a").is_some() && get_read_pool_for("vol-b").is_some());

    // A second reservation for vol-a must fail (would spawn a second writer
    // on the same DB) while vol-b is untouched.
    let (s1b, p1b, pe1b) = mk("vol-a");
    assert!(
        try_reserve_initializing_phase(
            "vol-a",
            StartRequest::for_test(IndexVolumeKind::Local),
            s1b,
            p1b,
            pe1b,
            VolumeSignals::new(fresh(None), NoopEventSink::shared()),
        )
        .is_err(),
        "double-start of the same volume must be rejected"
    );
    assert!(is_active("vol-b"), "vol-b unaffected by vol-a's rejected start");

    // Stop vol-a through the real teardown path; vol-b survives.
    stop_indexing("vol-a").expect("stop vol-a");
    assert!(!is_active("vol-a"));
    assert!(
        get_read_pool_for("vol-a").is_none(),
        "stopping vol-a withdrew its pool, so reads skip"
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
            StartRequest::for_test(IndexVolumeKind::Local),
            store,
            pool,
            pending,
            VolumeSignals::new(Arc::clone(&freshness), NoopEventSink::shared()),
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
        apply_freshness_event_on(&freshness, &NoopEventSink, "deadlock-test", FreshnessEvent::ScanStarted);
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
            StartRequest::for_test(IndexVolumeKind::Smb),
            store,
            pool,
            pending,
            VolumeSignals::new(fresh(Some(Freshness::Stale)), NoopEventSink::shared()),
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
            StartRequest::for_test(IndexVolumeKind::Smb),
            store,
            pool,
            pending,
            VolumeSignals::new(fresh(Some(Freshness::Stale)), NoopEventSink::shared()),
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
            StartRequest::for_test(IndexVolumeKind::Smb),
            store,
            pool,
            pending,
            VolumeSignals::new(fresh(Some(Freshness::Stale)), NoopEventSink::shared()),
        )
        .is_ok(),
        "reserve must succeed"
    );
    assert_eq!(get_freshness("smb-forget-test"), Some(Freshness::Stale), "loads Stale");
    assert!(db_path.exists(), "DB file exists before forget");
    assert!(
        get_read_pool_for("smb-forget-test").is_some(),
        "a reserved volume routes to its pool"
    );

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
    // The withdraw-before-delete ordering, observed from the read side: by the time
    // the file is gone the volume routes nothing, so no reader can still be opening
    // a connection to it.
    assert!(
        get_read_pool_for("smb-forget-test").is_none(),
        "clear_index must withdraw the read pool, not just delete the DB"
    );

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
                StartRequest::for_test(IndexVolumeKind::Smb),
                store,
                pool,
                pending,
                VolumeSignals::new(fresh(Some(Freshness::Stale)), NoopEventSink::shared()),
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
            try_reserve_initializing_phase(
                vid,
                StartRequest::for_test(IndexVolumeKind::Local),
                store,
                pool,
                pending,
                VolumeSignals::new(fresh(Some(initial)), NoopEventSink::shared()),
            )
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
    apply_freshness_event_on(
        &freshness,
        &NoopEventSink,
        "bus-chokepoint-test",
        FreshnessEvent::ScanCompleted,
    );

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

/// Reset every registry-backed test global: remove ONLY the volume ids these
/// tests reserve, from the registry AND from the read-handle tables (which are
/// keyed separately, so a registry removal alone leaves a routable pool behind).
///
/// ❌ Never `INDEX_REGISTRY.clear()` here. The registry is a process-global shared
/// with EVERY other test module, and under bare `cargo test` (threads in one
/// process) concurrent tests register private per-volume instances into it
/// (`stress_test_helpers::TestInstanceGuard`). A blanket clear wipes those
/// mid-assertion, an isolation flake (a routed `hold_rescan` then finds no tracker
/// and silently no-ops). So remove exactly the ids reserved in this file; keep
/// this list in sync with them.
fn clear_registry_and_pools() {
    const STATE_TEST_VIDS: &[&str] = &[
        ROOT_VOLUME_ID,
        "smb-nas",
        "vol-a",
        "vol-b",
        "deadlock-test",
        "smb-fresh-test",
        "smb-disco-test",
        "smb-forget-test",
        "smb-storm",
        "mtp-storm:65537",
        "sweep-fresh",
        "sweep-stale",
        "sweep-scanning",
        "awaits-first-scan",
    ];
    let mut reg = INDEX_REGISTRY.lock().unwrap();
    for vid in STATE_TEST_VIDS {
        reg.remove(*vid);
    }
    drop(reg);
    for vid in STATE_TEST_VIDS {
        uninstall_read_pool(vid);
        uninstall_pending_sizes(vid);
    }
}

/// "Has this volume ever actually been walked?" is not "is it active?", and
/// `Index::start_volume` branches on the difference — a search-driven walk
/// registers an instance nothing has ever scanned, and an enable that treated it
/// as indexed would leave the drive unindexed forever.
///
/// A volume nobody registered, and one whose own start is still in flight, both
/// answer NO: there is nothing to scan, and a start already running needs no
/// second one.
#[test]
fn awaiting_a_first_scan_is_not_the_same_as_being_active() {
    let _guard = INDEX_REGISTRY_TEST_GUARD.lock().unwrap_or_else(|e| e.into_inner());
    clear_registry_and_pools();

    assert!(
        !awaits_its_first_scan("awaits-first-scan"),
        "an unregistered volume has no index to scan into"
    );

    let _dir = reserve_initializing_index_for_test("awaits-first-scan", IndexVolumeKind::Local);
    assert!(is_active("awaits-first-scan"), "reserved ⇒ active");
    assert!(
        !awaits_its_first_scan("awaits-first-scan"),
        "a start already in flight owns the first scan"
    );

    clear_registry_and_pools();
}

/// Tests that mutate `INDEX_REGISTRY` serialize on this guard (mirrors
/// `tests/integration_tests.rs`'s `INDEXING_TEST_GUARD`).
static INDEX_REGISTRY_TEST_GUARD: LazyLock<std::sync::Mutex<()>> = LazyLock::new(|| std::sync::Mutex::new(()));

/// Clearing has to reach a database no volume has an instance for. That's the
/// ordinary shape once a search can walk: the walk stands a writer up, the app
/// restarts, and with drive indexing off nothing ever registers that volume
/// again — so the registry lookup that used to answer "not indexed, nothing to
/// do" was leaving real disk behind with no way for anyone to reclaim it.
#[test]
fn clearing_reaches_a_database_no_volume_is_registered_for() {
    let _lock = crate::indexing::handle::test_lock();
    let _guard = INDEX_REGISTRY_TEST_GUARD.lock().unwrap_or_else(|e| e.into_inner());
    clear_registry_and_pools();

    let dir = tempfile::tempdir().expect("temp dir");
    let _config = crate::indexing::host::config::install_data_dir_for_test(dir.path());
    let db_path = dir.path().join("index-smb-walked-only.db");
    std::fs::write(&db_path, vec![0u8; 64]).expect("write db");
    std::fs::write(dir.path().join("index-smb-walked-only.db-wal"), vec![0u8; 8]).expect("write wal");

    assert!(!is_active("smb-walked-only"), "test setup: nothing is registered");
    clear_index("smb-walked-only").expect("clearing an unregistered index must succeed");

    assert!(!db_path.exists(), "the database must be gone");
    assert!(
        !dir.path().join("index-smb-walked-only.db-wal").exists(),
        "its WAL sidecar must go with it"
    );
}

/// "Clear index" in settings means the whole index, not the boot disk's: a
/// search walks whichever drive it's pointed at, so the disk it accumulates can
/// belong to a share nobody ever turned indexing on for. The sweep therefore
/// takes the union of what's registered and what's on disk.
#[test]
fn clearing_everything_takes_the_registered_and_the_forgotten_alike() {
    let _lock = crate::indexing::handle::test_lock();
    let _guard = INDEX_REGISTRY_TEST_GUARD.lock().unwrap_or_else(|e| e.into_inner());
    clear_registry_and_pools();

    let dir = tempfile::tempdir().expect("temp dir");
    let _config = crate::indexing::host::config::install_data_dir_for_test(dir.path());

    // One volume with a live instance, one database with nobody behind it.
    let live_db = dir.path().join("index-smb-nas.db");
    let store = IndexStore::open(&live_db).expect("open store");
    let pool = Arc::new(ReadPool::new(live_db.clone()).expect("pool"));
    assert!(
        try_reserve_initializing_phase(
            "smb-nas",
            StartRequest::for_test(IndexVolumeKind::Smb),
            store,
            pool,
            Arc::new(PendingSizes::new()),
            VolumeSignals::new(fresh(Some(Freshness::Stale)), NoopEventSink::shared()),
        )
        .is_ok(),
        "reserve must succeed"
    );
    let orphan_db = dir.path().join("index-mtp-old-phone.db");
    std::fs::write(&orphan_db, vec![0u8; 64]).expect("write orphan db");

    clear_every_index().expect("the sweep must succeed");

    assert!(!live_db.exists(), "the registered volume's database must go");
    assert!(!orphan_db.exists(), "so must the one nothing registered");
    assert!(!is_active("smb-nas"), "and the instance with it");
    assert_eq!(
        crate::indexing::resources::retention::total_index_db_bytes(),
        0,
        "nothing is left to report"
    );
}

/// **Every phase a start can meet, and what it does about each.** The bug this
/// pins existed because one of the five was never considered: the reservation
/// asked `contains_key`, which conflates "already live, correctly refuse" with "on
/// its way out, must not refuse".
///
/// Driven against a registry of its own, so the phases can be placed rather than
/// raced. Three of the five need no manager and are placed here; the two that do
/// are pinned end-to-end next door in `cover::cold_drive_tests`:
/// - `Running` — `activation::turning_indexing_on_after_a_walk_covers_the_drive_without_truncating_it`
///   ("an indexed drive is left alone": a start on a live volume walks nothing again).
/// - `Detached`, both shapes — `toggles::a_start_on_a_detached_drive_lets_its_manager_come_back`
///   and `toggles::two_teardowns_and_a_start_in_one_window_end_with_a_rebuilt_index`.
#[test]
fn a_start_answers_every_phase_it_can_meet() {
    let dir = tempfile::tempdir().expect("temp dir");
    let mut nth = 0;
    // A registry holding one volume in `phase`, plus a reservation attempt against
    // it. Answers what the reservation returned and what the phase became.
    let mut reserve_over = |phase: IndexPhase| {
        nth += 1;
        let registry: Registry = std::sync::Mutex::new(HashMap::new());
        registry.lock_ignore_poison().insert(
            "phase-table".to_string(),
            IndexInstance {
                phase,
                kind: IndexVolumeKind::Local,
                signals: VolumeSignals::new(fresh(None), NoopEventSink::shared()),
            },
        );
        let db_path = dir.path().join(format!("phase-table-{nth}.db"));
        let store = IndexStore::open(&db_path).expect("store");
        let refused = try_reserve_initializing_phase_on(
            &registry,
            "phase-table",
            StartRequest::for_test(IndexVolumeKind::Local),
            store,
            Arc::new(ReadPool::new(db_path.clone()).expect("pool")),
            Arc::new(PendingSizes::new()),
            VolumeSignals::new(fresh(None), NoopEventSink::shared()),
        )
        .is_err();
        let recorded = matches!(
            registry.lock_ignore_poison().get("phase-table").map(|i| &i.phase),
            Some(IndexPhase::ShuttingDown { restart: Some(_) })
        );
        // The pool went into the process-wide table under a name no other test uses.
        uninstall_read_pool("phase-table");
        uninstall_pending_sizes("phase-table");
        (refused, recorded)
    };

    // ABSENT: the only legitimate start. Proven by the reservations above; here we
    // only need it to not be one of the refusals below.
    let absent: Registry = std::sync::Mutex::new(HashMap::new());
    let db_path = dir.path().join("phase-table-absent.db");
    assert!(
        try_reserve_initializing_phase_on(
            &absent,
            "phase-table-absent",
            StartRequest::for_test(IndexVolumeKind::Local),
            IndexStore::open(&db_path).expect("store"),
            Arc::new(ReadPool::new(db_path.clone()).expect("pool")),
            Arc::new(PendingSizes::new()),
            VolumeSignals::new(fresh(None), NoopEventSink::shared()),
        )
        .is_ok(),
        "an absent key is the one shape a start may take",
    );
    uninstall_read_pool("phase-table-absent");
    uninstall_pending_sizes("phase-table-absent");

    // INITIALIZING: a start for this volume is already in flight and will land
    // `Running`. Refusing is idempotence, and there is nothing to record — the
    // volume the caller wants is on its way.
    let init_store = IndexStore::open(&dir.path().join("phase-table-init.db")).expect("store");
    assert_eq!(
        reserve_over(IndexPhase::Initializing { store: init_store }),
        (true, false),
        "a start already in flight needs no second one",
    );

    // SHUTTING DOWN: the volume is on its way OUT, so the start is RECORDED and the
    // drain carries it out. ❗ This is the whole bug: refusing here silently drops
    // what the user asked for, and the disable's veto then lands on top of it.
    assert_eq!(
        reserve_over(IndexPhase::ShuttingDown { restart: None }),
        (true, true),
        "a start meeting a drain has to be recorded, never bounced",
    );

    // FAILED: refused here, and rightly — `start_indexing_for` clears a dead index
    // out of the way BEFORE it ever reserves, so a start only reaches this arm if
    // the volume re-failed in the window. Recording a restart on an instance
    // nothing is draining would strand it.
    assert_eq!(
        reserve_over(IndexPhase::Failed {
            reason: IndexFailure {
                code: 10,
                extended_code: 778,
            },
            db_path: dir.path().join("dead.db"),
        }),
        (true, false),
        "a dead index is cleared by the choke point, not recorded against",
    );
}
