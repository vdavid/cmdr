//! Buffering events while the scan runs, and replaying them once it finishes.

use super::*;

// ── Reconciler buffer/replay tests ───────────────────────────────

#[test]
fn reconciler_starts_in_buffering_mode() {
    let reconciler = EventReconciler::new();
    assert!(reconciler.is_buffering());
    assert_eq!(reconciler.buffer_len(), 0);
}

#[test]
fn buffer_events_during_scan() {
    let mut reconciler = EventReconciler::new();

    reconciler.buffer_event(make_event("/test/a.txt", 10, created_file_flags()));
    reconciler.buffer_event(make_event("/test/b.txt", 20, modified_file_flags()));
    reconciler.buffer_event(make_event("/test/c.txt", 30, removed_file_flags()));

    assert_eq!(reconciler.buffer_len(), 3);
}

#[test]
fn switch_to_live_clears_buffer() {
    let mut reconciler = EventReconciler::new();

    reconciler.buffer_event(make_event("/test/a.txt", 10, created_file_flags()));
    reconciler.buffer_event(make_event("/test/b.txt", 20, created_file_flags()));

    reconciler.switch_to_live();

    assert!(!reconciler.is_buffering());
    assert_eq!(reconciler.buffer_len(), 0);
}

#[test]
fn events_not_buffered_in_live_mode() {
    let mut reconciler = EventReconciler::new();
    reconciler.switch_to_live();

    // In live mode, buffer_event is a no-op
    reconciler.buffer_event(make_event("/test/a.txt", 10, created_file_flags()));
    assert_eq!(reconciler.buffer_len(), 0);
}

// ── Replay tests ─────────────────────────────────────────────────

#[test]
fn replay_skips_events_at_or_before_scan_start() {
    let (writer, dir, conn) = setup_test_writer();
    let db_path = dir.path().join("test-reconciler.db");

    let test_dir = non_excluded_tempdir();
    let file_path = test_dir.path().join("old.txt");
    std::fs::write(&file_path, "old").unwrap();
    ensure_path_in_db(&db_path, &test_dir.path().to_string_lossy(), &writer);

    let mut reconciler = EventReconciler::new();
    reconciler.buffer_event(make_event(&file_path.to_string_lossy(), 5, created_file_flags()));
    reconciler.buffer_event(make_event(&file_path.to_string_lossy(), 10, created_file_flags()));

    let mut callback_called = false;
    let result = reconciler
        .replay(10, &conn, &writer, &mut |_| callback_called = true)
        .unwrap();

    writer.flush_blocking().unwrap();
    writer.shutdown();

    // All events at or before scan_start_event_id=10 are skipped
    assert_eq!(result, 10);
    assert!(!callback_called);

    // Nothing written to DB
    let store = IndexStore::open(&db_path).unwrap();
    let parent_id = store::resolve_path(store.read_conn(), &test_dir.path().to_string_lossy())
        .unwrap()
        .unwrap();
    let children = store.list_children(parent_id).unwrap();
    assert!(children.is_empty());
}

#[test]
fn replay_processes_events_after_scan_start() {
    let (writer, dir, conn) = setup_test_writer();
    let db_path = dir.path().join("test-reconciler.db");

    let test_dir = non_excluded_tempdir();
    let file_path = test_dir.path().join("new.txt");
    std::fs::write(&file_path, "new content").unwrap();
    ensure_path_in_db(&db_path, &test_dir.path().to_string_lossy(), &writer);

    let mut reconciler = EventReconciler::new();
    reconciler.buffer_event(make_event(&file_path.to_string_lossy(), 20, created_file_flags()));

    let result = reconciler.replay(10, &conn, &writer, &mut |_| {}).unwrap();

    writer.flush_blocking().unwrap();
    writer.shutdown();

    assert_eq!(result, 20);

    let store = IndexStore::open(&db_path).unwrap();
    let parent_id = store::resolve_path(store.read_conn(), &test_dir.path().to_string_lossy())
        .unwrap()
        .unwrap();
    let children = store.list_children(parent_id).unwrap();
    assert_eq!(children.len(), 1);
    assert_eq!(children[0].name, "new.txt");
}

#[test]
fn replay_sends_update_last_event_id() {
    let (writer, dir, conn) = setup_test_writer();
    let db_path = dir.path().join("test-reconciler.db");

    let test_dir = non_excluded_tempdir();
    let file_a = test_dir.path().join("a.txt");
    let file_b = test_dir.path().join("b.txt");
    std::fs::write(&file_a, "a").unwrap();
    std::fs::write(&file_b, "b").unwrap();
    ensure_path_in_db(&db_path, &test_dir.path().to_string_lossy(), &writer);

    let mut reconciler = EventReconciler::new();
    reconciler.buffer_event(make_event(&file_a.to_string_lossy(), 15, created_file_flags()));
    reconciler.buffer_event(make_event(&file_b.to_string_lossy(), 25, created_file_flags()));

    let result = reconciler.replay(10, &conn, &writer, &mut |_| {}).unwrap();

    writer.flush_blocking().unwrap();
    writer.shutdown();

    // Returns the highest event_id
    assert_eq!(result, 25);

    // Verify last_event_id was persisted to the DB
    let store = IndexStore::open(&db_path).unwrap();
    let stored_id = IndexStore::get_meta(store.read_conn(), "last_event_id").unwrap();
    assert_eq!(stored_id, Some("25".to_string()));
}

#[test]
fn replay_calls_callback_with_affected_paths() {
    let (writer, dir, conn) = setup_test_writer();
    let db_path = dir.path().join("test-reconciler.db");

    let test_dir = non_excluded_tempdir();
    let file_path = test_dir.path().join("notify.txt");
    std::fs::write(&file_path, "hi").unwrap();
    ensure_path_in_db(&db_path, &test_dir.path().to_string_lossy(), &writer);

    let mut reconciler = EventReconciler::new();
    reconciler.buffer_event(make_event(&file_path.to_string_lossy(), 20, created_file_flags()));

    let mut notified_paths: Vec<String> = Vec::new();
    reconciler
        .replay(10, &conn, &writer, &mut |paths| {
            notified_paths = paths;
        })
        .unwrap();

    writer.shutdown();

    assert!(!notified_paths.is_empty());
    // The parent directory should appear in affected paths
    let parent = test_dir.path().to_string_lossy().to_string();
    assert!(
        notified_paths.iter().any(|p| p == &parent),
        "expected parent dir in affected paths, got: {notified_paths:?}"
    );
}

#[test]
fn replay_empty_buffer_returns_scan_start_unchanged() {
    let (writer, _dir, conn) = setup_test_writer();

    let mut reconciler = EventReconciler::new();
    // No events buffered

    let mut callback_called = false;
    let result = reconciler
        .replay(42, &conn, &writer, &mut |_| callback_called = true)
        .unwrap();

    writer.shutdown();

    assert_eq!(result, 42);
    assert!(!callback_called);
}

#[test]
fn replay_all_events_before_scan_start_returns_unchanged() {
    let (writer, dir, conn) = setup_test_writer();
    let db_path = dir.path().join("test-reconciler.db");

    let test_dir = non_excluded_tempdir();
    let file_path = test_dir.path().join("stale.txt");
    std::fs::write(&file_path, "stale").unwrap();
    ensure_path_in_db(&db_path, &test_dir.path().to_string_lossy(), &writer);

    let mut reconciler = EventReconciler::new();
    reconciler.buffer_event(make_event(&file_path.to_string_lossy(), 3, created_file_flags()));
    reconciler.buffer_event(make_event(&file_path.to_string_lossy(), 7, modified_file_flags()));

    let mut callback_called = false;
    let result = reconciler
        .replay(100, &conn, &writer, &mut |_| callback_called = true)
        .unwrap();

    writer.shutdown();

    assert_eq!(result, 100);
    assert!(!callback_called);
}
