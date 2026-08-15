//! Where a `MustScanSubDirs` anchor goes: the shallow/deep split between the
//! visible scanner and the throttled reconcile drain, escalation when the
//! parent chain is missing, and the queue that drains itself.

use super::*;
use tokio_util::sync::CancellationToken;

#[tokio::test]
async fn must_scan_sub_dirs_queued() {
    let mut reconciler = EventReconciler::new();
    reconciler.switch_to_live();

    let (writer, _dir, _conn) = setup_test_writer();
    reconciler.queue_must_scan_sub_dirs(PathBuf::from("/test/dir"), &writer);

    // Should not have any pending rescans after starting one
    // (it was popped from the set and started)
    assert!(reconciler.pending_rescans.lock().unwrap().is_empty());
    assert!(reconciler.rescan_active.load(Ordering::Relaxed));

    writer.shutdown();
}

#[tokio::test]
async fn must_scan_sub_dirs_deduplication() {
    let mut reconciler = EventReconciler::new();
    reconciler.switch_to_live();

    // Mark rescan as active so new ones get queued
    reconciler.rescan_active.store(true, Ordering::Relaxed);

    let (writer, _dir, _conn) = setup_test_writer();
    reconciler.queue_must_scan_sub_dirs(PathBuf::from("/test/dir"), &writer);
    reconciler.queue_must_scan_sub_dirs(PathBuf::from("/test/dir"), &writer);
    reconciler.queue_must_scan_sub_dirs(PathBuf::from("/test/other"), &writer);

    // Deduplication: only 2 unique paths should be queued
    assert_eq!(reconciler.pending_rescans.lock().unwrap().len(), 2);

    writer.shutdown();
}

// ── Depth-split MustScanSubDirs routing (churn resilience, Fix 1) ─

fn must_scan_dir_flags() -> FsEventFlags {
    FsEventFlags {
        must_scan_sub_dirs: true,
        item_is_dir: true,
        ..Default::default()
    }
}

/// A root-scale `MustScanSubDirs` must take the VISIBLE scanner path, not the
/// invisible reconcile hold. Pre-fix, `process_live_event` queued `/` onto the
/// reconcile drain, which holds the per-dir hourglass for the whole ~20-min walk
/// (`reconcile/reconciler/rescan.rs`) and — under continuous root churn — never releases it
/// (the stuck-hourglass bug). Post-fix, a shallow anchor routes to the scanner: no
/// hold, nothing queued on the drain.
#[tokio::test]
async fn root_scale_must_scan_routes_to_scanner_without_a_stuck_hold() {
    // A private per-volume instance (see `setup_private_writer`): the hold routes
    // to a PRIVATE tracker, so a fresh volume id also gives the shallow anchor a
    // clean sweep window without touching the process-global `SHALLOW_SWEEPS`.
    let volume_id = "smb://reconciler-test-root-scale";
    let (writer, _dir, conn, instance) = setup_private_writer(volume_id);
    let mut reconciler =
        EventReconciler::new_for(volume_id.to_string(), IndexPathSpace::root(), CancellationToken::new());
    reconciler.switch_to_live();
    let sink = Arc::new(Mutex::new(Vec::<String>::new()));
    reconciler.set_recording_scan_trigger(Arc::clone(&sink));

    // Drive a real root-scale MustScanSubDirs event through the live path.
    let mut pending = HashSet::new();
    reconciler.process_live_event(&make_event("/", 1, must_scan_dir_flags()), &conn, &writer, &mut pending);

    // Routed to the visible scanner...
    assert_eq!(
        sink.lock().unwrap().len(),
        1,
        "a shallow (root-scale) anchor routes to the scanner"
    );
    // ...and took NO reconcile hourglass hold, and queued nothing on the drain.
    assert!(
        !instance.tracker.is_pending("/"),
        "the scanner path must NOT hold the per-dir hourglass (the stuck-hold bug)"
    );
    assert!(
        reconciler.pending_rescans_snapshot().is_empty(),
        "nothing is queued on the reconcile drain"
    );
    assert!(!reconciler.is_rescan_active_for_test(), "no reconcile was spawned");

    writer.shutdown();
}

/// End to end through the live path: the SECOND root-scale `MustScanSubDirs`
/// must not reach the scanner, and must not fall back onto the reconcile drain
/// either.
///
/// Scope, deliberately stated: this pins that the live path CONSULTS the window
/// and that a coalesced anchor goes nowhere. It does NOT pin the window's length
/// — it fires both events in the same second, which the old 45 s cooldown would
/// have coalesced too. The length is pinned by `rescan::route`'s unit tests, which
/// inject the clock; `route_shallow_to_scanner` reads the wall clock directly, so
/// a day-long window can't be exercised here without injecting a clock through
/// the live path.
#[tokio::test]
async fn a_second_root_scale_must_scan_does_not_reach_the_scanner() {
    // Private per-volume instance; a fresh volume id starts with a clean sweep
    // window, so the first `/` sweeps and the second coalesces.
    let volume_id = "smb://reconciler-test-second-root-scale";
    let (writer, _dir, conn, _instance) = setup_private_writer(volume_id);
    let mut reconciler =
        EventReconciler::new_for(volume_id.to_string(), IndexPathSpace::root(), CancellationToken::new());
    reconciler.switch_to_live();
    let sink = Arc::new(Mutex::new(Vec::<String>::new()));
    reconciler.set_recording_scan_trigger(Arc::clone(&sink));

    let mut pending = HashSet::new();
    // Two root-scale anchors, exactly the shape production sees: `/` twice.
    reconciler.process_live_event(&make_event("/", 1, must_scan_dir_flags()), &conn, &writer, &mut pending);
    reconciler.process_live_event(&make_event("/", 2, must_scan_dir_flags()), &conn, &writer, &mut pending);

    assert_eq!(
        sink.lock().unwrap().len(),
        1,
        "only the leading-edge anchor sweeps; the second is coalesced for the day"
    );
    // And the coalesced one still didn't fall back onto the reconcile drain.
    assert!(
        reconciler.pending_rescans_snapshot().is_empty(),
        "a coalesced shallow anchor must not queue an invisible reconcile instead"
    );

    writer.shutdown();
}

/// The counterpart: a deep/narrow `MustScanSubDirs` anchor keeps the throttled
/// reconcile drain (holds the hourglass, queues the anchor). Routing splits by
/// depth, so the deep path must NOT reach the scanner.
#[tokio::test]
async fn deep_must_scan_keeps_the_reconcile_drain() {
    // Private per-volume instance: the deep anchor's hourglass hold routes to this
    // volume's PRIVATE tracker, so the `is_pending` assertion below is immune to a
    // foreign root writer clearing the shared root tracker mid-assertion (this was
    // the root-cause flake — its panic poisoned `PENDING_SIZES_TEST_MUTEX` and
    // cascaded into every other holder).
    let volume_id = "smb://reconciler-test-deep";
    let (writer, _dir, conn, instance) = setup_private_writer(volume_id);
    let mut reconciler =
        EventReconciler::new_for(volume_id.to_string(), IndexPathSpace::root(), CancellationToken::new());
    reconciler.switch_to_live();
    // Keep the queued anchor visible (no spawn), so we assert on the queue directly.
    reconciler.rescan_active.store(true, Ordering::Relaxed);
    let sink = Arc::new(Mutex::new(Vec::<String>::new()));
    reconciler.set_recording_scan_trigger(Arc::clone(&sink));

    // Depth 5: well past the shallow threshold, so it reconciles in place.
    let deep = "/aaa/bbb/ccc/ddd/target";
    let mut pending = HashSet::new();
    reconciler.process_live_event(
        &make_event(deep, 1, must_scan_dir_flags()),
        &conn,
        &writer,
        &mut pending,
    );

    assert!(
        sink.lock().unwrap().is_empty(),
        "a deep anchor must NOT route to the scanner"
    );
    assert_eq!(
        reconciler.pending_rescans_snapshot(),
        vec![PathBuf::from(deep)],
        "the deep anchor is queued on the reconcile drain"
    );
    assert!(
        instance.tracker.is_pending(deep),
        "the reconcile drain holds the per-dir hourglass for a deep anchor"
    );

    writer.shutdown();
}

// ── Missing-parent escalation (Leak B) ──────────────────────────

/// A live creation whose parent chain is absent must NOT be dropped: it escalates
/// to a rescan of the HIGHEST missing dir (the child of the deepest existing
/// dir), so `reconcile_subtree` anchors at an existing parent and discovers the
/// whole missing chain. Pre-fix this silently dropped the credit.
#[tokio::test(flavor = "multi_thread")]
async fn missing_parent_creation_escalates_to_highest_missing_dir() {
    let (writer, _dir, conn) = setup_test_writer();
    let db_path = writer.db_path();
    let space = IndexPathSpace::root();

    // On disk: <base>/mid/leaf/file.txt exists; only <base> and up is indexed.
    let base = non_excluded_tempdir();
    let deep = base.path().join("mid").join("leaf");
    std::fs::create_dir_all(&deep).expect("create deep dirs");
    let file = deep.join("file.txt");
    std::fs::write(&file, b"hi").expect("write file");

    let base_abs = space.absolute(&base.path().to_string_lossy());
    ensure_path_in_db(&db_path, &base_abs, &writer);

    let mut reconciler = EventReconciler::new();
    reconciler.switch_to_live();
    // Keep the queued anchor visible in the set (don't spawn a rescan).
    reconciler.rescan_active.store(true, Ordering::Relaxed);

    let file_abs = space.absolute(&file.to_string_lossy());
    let event = make_event(&file_abs, 1, created_file_flags());
    let mut pending = HashSet::new();
    reconciler.process_live_event(&event, &conn, &writer, &mut pending);

    let queued: Vec<PathBuf> = reconciler.pending_rescans.lock().unwrap().iter().cloned().collect();
    let expected = PathBuf::from(format!("{base_abs}/mid"));
    assert_eq!(
        queued,
        vec![expected],
        "escalates to the highest missing dir (child of the deepest existing dir)"
    );

    writer.shutdown();
}

/// `reconcile_subtree` on a root whose parent chain is missing escalates via its
/// `ReconcileSummary.escalation` (the caller re-queues an anchor closer to the
/// volume root), instead of dropping the whole subtree's credit.
#[test]
fn reconcile_subtree_missing_chain_escalates() {
    let (writer, _dir, conn) = setup_test_writer();
    let db_path = writer.db_path();
    let space = IndexPathSpace::root();

    let base = non_excluded_tempdir();
    let deep = base.path().join("mid").join("leaf");
    std::fs::create_dir_all(&deep).expect("create deep dirs");
    let base_abs = space.absolute(&base.path().to_string_lossy());
    ensure_path_in_db(&db_path, &base_abs, &writer);

    let cancelled = CancellationToken::new();
    let leaf_abs = space.absolute(&deep.to_string_lossy());
    let summary = reconcile_subtree(Path::new(&leaf_abs), &space, &conn, &writer, &cancelled).expect("reconcile ok");
    assert_eq!(
        summary.escalation,
        Some(PathBuf::from(format!("{base_abs}/mid"))),
        "escalates to the highest missing dir"
    );

    writer.shutdown();
}

/// A parent component that resolves to a FILE row (a stale file→dir type change)
/// counts as MISSING: the anchor re-lists the deepest existing DIR so the diff
/// deletes the stale file row and inserts the dir — never parenting under a file.
#[test]
fn escalation_anchor_stops_at_a_file_parent() {
    let (writer, _dir, conn) = setup_test_writer();
    let db_path = writer.db_path();
    let space = IndexPathSpace::root();

    let base = non_excluded_tempdir();
    let base_abs = space.absolute(&base.path().to_string_lossy());
    ensure_path_in_db(&db_path, &base_abs, &writer);
    // Insert "mid" as a FILE under base.
    {
        let wconn = IndexStore::open_write_connection(&db_path).unwrap();
        let base_id = store::resolve_path(&wconn, &base_abs).unwrap().unwrap();
        IndexStore::insert_entry_v2(&wconn, base_id, "mid", false, false, Some(1), Some(1), None, None).unwrap();
    }

    let target = format!("{base_abs}/mid/leaf/x.txt");
    let anchor = resolve_escalation_anchor(&space, &conn, &target);
    assert_eq!(
        anchor,
        Some(PathBuf::from(&base_abs)),
        "a file parent forces re-listing the deepest existing dir"
    );

    writer.shutdown();
}

/// A finished rescan starts the next queued one itself: the completion handler
/// drains `pending_rescans`, so a path queued behind an active walk runs without
/// waiting for another `must_scan_sub_dirs` event to happen along.
///
/// Poll to quiescence, never sleep a fixed span: the completion handler clears
/// `rescan_active` BEFORE it drains, and a walk of a missing root escalates to
/// its ancestor, so the drain is several thread-spawn + open-connection rounds.
/// A loaded machine takes far longer over those than any fixed sleep would guess.
#[tokio::test]
async fn queued_rescans_start_after_active_completes() {
    let mut reconciler = EventReconciler::new();
    reconciler.switch_to_live();

    let (writer, _dir, _conn) = setup_test_writer();

    // Two rescans for nonexistent paths (each completes almost immediately
    // because reconcile_subtree returns early when root isn't in DB). The second
    // queues behind the first, or is picked up straight away if the first
    // already finished; either way the queue has to drain to empty.
    reconciler.queue_must_scan_sub_dirs(PathBuf::from("/nonexistent_cmdr_test/first"), &writer);
    reconciler.queue_must_scan_sub_dirs(PathBuf::from("/nonexistent_cmdr_test/second"), &writer);

    // Without the drain, the second path stays queued forever.
    // 30 s is a backstop, not a guess: the drain runs on detached OS threads that
    // each open a SQLite connection and ride the writer, and under a saturated CI
    // runner (thousands of tests in parallel) those threads can wait seconds just
    // to be scheduled. A 5 s ceiling sat too close to that worst case and tripped.
    cmdr_fs::testing::wait_until_async(
        Duration::from_secs(30),
        "the rescan queue to drain after each active rescan completes",
        || !reconciler.is_rescan_active_for_test() && reconciler.pending_rescans_snapshot().is_empty(),
    )
    .await;

    writer.shutdown();
}

/// A volume watched branch by branch never routes an anchor to the visible
/// scanner, whatever its depth.
///
/// That route rescans the WHOLE volume (`perform_registry_rescan`), which on a
/// search-built index is the full drive walk `Activation::WriterOnly` exists to
/// not do — and on a drive whose owner turned indexing off, work both switches
/// exist to stop. The throttled drain walks the anchor and nothing above it.
#[tokio::test]
async fn a_branch_watched_volume_never_routes_an_anchor_to_the_whole_volume_scanner() {
    let volume_id = "smb://reconciler-test-branch-confined";
    let (writer, _dir, conn, _instance) = setup_private_writer(volume_id);
    let mut reconciler =
        EventReconciler::new_for(volume_id.to_string(), IndexPathSpace::root(), CancellationToken::new());
    reconciler.switch_to_live();
    let sink = Arc::new(Mutex::new(Vec::<String>::new()));
    reconciler.set_recording_scan_trigger(Arc::clone(&sink));

    let watch = crate::indexing::watch::branches::live_for(volume_id);
    let covered = vec!["/covered".to_string()];
    watch.begin_covering(&covered);
    watch.finish_covering(&covered, crate::indexing::watch::branches::AfterWalk::Watch);
    reconciler.within(crate::indexing::watch::branches::WatchScope::Branches(watch));

    let mut pending = HashSet::new();
    // Shallow enough that a whole-volume loop would hand it to the scanner.
    reconciler.process_live_event(
        &make_event("/covered", 1, must_scan_dir_flags()),
        &conn,
        &writer,
        &mut pending,
    );

    assert!(
        sink.lock().unwrap().is_empty(),
        "no whole-volume rescan: the covered branch is all this volume answers for"
    );
    assert_eq!(
        reconciler.rescan_scopes(),
        vec![PathBuf::from("/covered")],
        "the anchor walks on the throttled drain instead"
    );

    // And an anchor outside every branch is left to the next search, which is
    // where growing coverage belongs.
    reconciler.queue_must_scan_sub_dirs(PathBuf::from("/elsewhere"), &writer);
    assert_eq!(
        reconciler.rescan_scopes(),
        vec![PathBuf::from("/covered")],
        "the watcher never indexes ground nobody asked it to walk"
    );

    crate::indexing::watch::branches::forget(volume_id);
    writer.shutdown();
}
