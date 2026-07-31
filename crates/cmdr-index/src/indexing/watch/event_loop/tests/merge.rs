//! `merge_fs_events` dedup/flag-priority, `EventReconciler` buffer
//! overflow/mode, and replay-dedup tests.

use super::*;
use std::collections::HashMap;

use crate::indexing::reconcile::reconciler::EventReconciler;

/// Merging created+removed: `item_removed` wins (priority-based merge),
/// dropping `item_created`. The reconciler's stat-before-delete in
/// `handle_removal` compensates: if the file still exists on disk, it
/// upserts instead of deleting. Regression coverage for f0c225f.
#[test]
fn merge_created_then_removed_prioritizes_removed() {
    let created = make_event(
        "/test/file.txt",
        100,
        watcher::FsEventFlags {
            item_created: true,
            item_is_file: true,
            ..Default::default()
        },
    );
    let removed = make_event(
        "/test/file.txt",
        200,
        watcher::FsEventFlags {
            item_removed: true,
            item_is_file: true,
            ..Default::default()
        },
    );

    let merged = merge_fs_events(&created, &removed);

    assert!(merged.flags.item_removed, "item_removed should be set");
    assert!(!merged.flags.item_created, "item_created is dropped; removed wins");
    assert!(merged.flags.item_is_file, "item_is_file should be preserved");
    assert_eq!(merged.event_id, 200, "higher event_id should be kept");
}

/// Same as above but with events in reverse order: removed first, then
/// created. `item_removed` still wins.
#[test]
fn merge_removed_then_created_prioritizes_removed() {
    let removed = make_event(
        "/test/file.txt",
        100,
        watcher::FsEventFlags {
            item_removed: true,
            item_is_file: true,
            ..Default::default()
        },
    );
    let created = make_event(
        "/test/file.txt",
        200,
        watcher::FsEventFlags {
            item_created: true,
            item_is_file: true,
            ..Default::default()
        },
    );

    let merged = merge_fs_events(&removed, &created);

    assert!(merged.flags.item_removed, "item_removed should be set");
    assert!(!merged.flags.item_created, "item_created is dropped; removed wins");
    assert!(merged.flags.item_is_file, "item_is_file should be preserved");
    assert_eq!(merged.event_id, 200, "higher event_id should be kept");
}

/// When merging, the higher event_id should always win regardless of
/// which event is "existing" vs "incoming".
#[test]
fn merge_keeps_higher_event_id() {
    let older = make_event(
        "/test/file.txt",
        300,
        watcher::FsEventFlags {
            item_modified: true,
            item_is_file: true,
            ..Default::default()
        },
    );
    let newer = make_event(
        "/test/file.txt",
        100,
        watcher::FsEventFlags {
            item_modified: true,
            item_is_file: true,
            ..Default::default()
        },
    );

    // existing=older (300), incoming=newer (100)
    let merged = merge_fs_events(&older, &newer);
    assert_eq!(merged.event_id, 300, "higher event_id should be kept");

    // existing=newer (100), incoming=older (300)
    let merged = merge_fs_events(&newer, &older);
    assert_eq!(merged.event_id, 300, "higher event_id should be kept");
}

// ── merge_fs_events dedup/flag tests ─────────────────────────────

/// Three events for the same path merge into one with the highest
/// priority flag and the highest event_id.
#[test]
fn merge_three_events_same_path_keeps_highest_priority() {
    let modified = make_event(
        "/test/file.txt",
        10,
        watcher::FsEventFlags {
            item_modified: true,
            item_is_file: true,
            ..Default::default()
        },
    );
    let created = make_event(
        "/test/file.txt",
        20,
        watcher::FsEventFlags {
            item_created: true,
            item_is_file: true,
            ..Default::default()
        },
    );
    let modified2 = make_event(
        "/test/file.txt",
        30,
        watcher::FsEventFlags {
            item_modified: true,
            item_is_file: true,
            ..Default::default()
        },
    );

    // Simulate HashMap-style dedup: fold sequentially
    let merged = merge_fs_events(&modified, &created);
    let merged = merge_fs_events(&merged, &modified2);

    assert!(
        merged.flags.item_created,
        "item_created should survive (higher priority than modified)"
    );
    assert!(!merged.flags.item_modified, "item_modified is subsumed by item_created");
    assert_eq!(merged.event_id, 30, "highest event_id should be kept");
}

/// Events for different paths are preserved independently when stored
/// in a HashMap keyed by path (the live event loop's dedup strategy).
#[test]
fn distinct_paths_are_all_preserved() {
    let paths = ["/a.txt", "/b.txt", "/c.txt", "/d/e.txt", "/f/g/h.txt"];
    let mut map = HashMap::<String, watcher::FsChangeEvent>::new();

    for (i, path) in paths.iter().enumerate() {
        let event = make_event(
            path,
            (i + 1) as u64,
            watcher::FsEventFlags {
                item_modified: true,
                item_is_file: true,
                ..Default::default()
            },
        );
        map.entry(path.to_string())
            .and_modify(|existing| {
                *existing = merge_fs_events(existing, &event);
            })
            .or_insert(event);
    }

    assert_eq!(map.len(), paths.len(), "each distinct path should have its own entry");
    for path in &paths {
        assert!(map.contains_key(*path), "map should contain {path}");
    }
}

/// `must_scan_sub_dirs` always wins when merged with other flags.
#[test]
fn merge_must_scan_sub_dirs_wins_over_modified() {
    let modified = make_event(
        "/test/dir",
        10,
        watcher::FsEventFlags {
            item_modified: true,
            item_is_dir: true,
            ..Default::default()
        },
    );
    let must_scan = make_event(
        "/test/dir",
        20,
        watcher::FsEventFlags {
            must_scan_sub_dirs: true,
            item_is_dir: true,
            ..Default::default()
        },
    );

    // must_scan_sub_dirs incoming
    let merged = merge_fs_events(&modified, &must_scan);
    assert!(merged.flags.must_scan_sub_dirs, "must_scan_sub_dirs should win");
    assert_eq!(merged.event_id, 20);

    // must_scan_sub_dirs existing
    let merged = merge_fs_events(&must_scan, &modified);
    assert!(
        merged.flags.must_scan_sub_dirs,
        "must_scan_sub_dirs should win regardless of order"
    );
    assert_eq!(merged.event_id, 20);
}

/// `must_scan_sub_dirs` wins even when the other event has `item_removed`.
#[test]
fn merge_must_scan_sub_dirs_wins_over_removed() {
    let removed = make_event(
        "/test/dir",
        10,
        watcher::FsEventFlags {
            item_removed: true,
            item_is_dir: true,
            ..Default::default()
        },
    );
    let must_scan = make_event(
        "/test/dir",
        20,
        watcher::FsEventFlags {
            must_scan_sub_dirs: true,
            item_is_dir: true,
            ..Default::default()
        },
    );

    let merged = merge_fs_events(&removed, &must_scan);
    assert!(
        merged.flags.must_scan_sub_dirs,
        "must_scan_sub_dirs should win over item_removed"
    );
}

// ── EventReconciler buffer overflow tests ────────────────────────

/// Buffering exactly MAX_BUFFER_CAPACITY (500K) events does NOT
/// trigger overflow. Adding one more does.
#[test]
fn buffer_capacity_boundary() {
    // MAX_BUFFER_CAPACITY is 500_000 (private to reconciler.rs)
    let cap = 500_000usize;
    let mut reconciler = EventReconciler::new();

    for i in 0..cap {
        reconciler.buffer_event(make_event(
            "/test/file.txt",
            i as u64,
            watcher::FsEventFlags {
                item_modified: true,
                item_is_file: true,
                ..Default::default()
            },
        ));
    }

    assert_eq!(
        reconciler.buffer_len(),
        cap,
        "buffer should hold exactly MAX_BUFFER_CAPACITY events"
    );
    assert!(
        !reconciler.did_buffer_overflow(),
        "should not overflow at exactly MAX_BUFFER_CAPACITY"
    );

    // One more triggers overflow
    reconciler.buffer_event(make_event(
        "/test/overflow.txt",
        cap as u64,
        watcher::FsEventFlags {
            item_modified: true,
            item_is_file: true,
            ..Default::default()
        },
    ));

    assert!(
        reconciler.did_buffer_overflow(),
        "should overflow after exceeding MAX_BUFFER_CAPACITY"
    );
    assert_eq!(reconciler.buffer_len(), 0, "buffer should be cleared on overflow");
}

/// After overflow, subsequent buffer_event calls are no-ops.
#[test]
fn buffer_overflow_drops_further_events() {
    let cap = 500_000usize;
    let mut reconciler = EventReconciler::new();

    // Fill to capacity + 1 to trigger overflow
    for i in 0..=cap {
        reconciler.buffer_event(make_event(
            "/test/file.txt",
            i as u64,
            watcher::FsEventFlags {
                item_modified: true,
                item_is_file: true,
                ..Default::default()
            },
        ));
    }
    assert!(reconciler.did_buffer_overflow());

    // Further events are silently dropped
    reconciler.buffer_event(make_event(
        "/test/new.txt",
        999_999,
        watcher::FsEventFlags {
            item_created: true,
            item_is_file: true,
            ..Default::default()
        },
    ));
    assert_eq!(reconciler.buffer_len(), 0, "buffer should remain empty after overflow");
}

/// `did_buffer_overflow()` returns true after overflow, but
/// `switch_to_live()` resets it. This matches the production flow:
/// overflow is checked (and acted on) BEFORE `switch_to_live()`.
#[test]
fn overflow_flag_is_readable_before_switch_to_live() {
    let cap = 500_000usize;
    let mut reconciler = EventReconciler::new();

    for i in 0..=cap {
        reconciler.buffer_event(make_event(
            "/test/file.txt",
            i as u64,
            watcher::FsEventFlags {
                item_modified: true,
                item_is_file: true,
                ..Default::default()
            },
        ));
    }

    // Overflow is observable before switch_to_live
    assert!(reconciler.did_buffer_overflow(), "overflow flag should be set");

    // switch_to_live resets it (by design: the caller already consumed the flag)
    reconciler.switch_to_live();
    assert!(
        !reconciler.did_buffer_overflow(),
        "switch_to_live should reset overflow flag"
    );
    assert!(!reconciler.is_buffering(), "should be in live mode");
}

/// Buffering mode transitions: new -> buffering, switch_to_live ->
/// live, buffer_event is no-op in live mode.
#[test]
fn buffering_mode_transitions() {
    let mut reconciler = EventReconciler::new();

    // Starts in buffering mode
    assert!(reconciler.is_buffering());

    // Buffer works
    reconciler.buffer_event(make_event(
        "/a.txt",
        1,
        watcher::FsEventFlags {
            item_created: true,
            item_is_file: true,
            ..Default::default()
        },
    ));
    assert_eq!(reconciler.buffer_len(), 1);

    // Switch to live
    reconciler.switch_to_live();
    assert!(!reconciler.is_buffering());
    assert_eq!(reconciler.buffer_len(), 0, "buffer cleared on switch");

    // buffer_event is no-op in live mode
    reconciler.buffer_event(make_event(
        "/b.txt",
        2,
        watcher::FsEventFlags {
            item_created: true,
            item_is_file: true,
            ..Default::default()
        },
    ));
    assert_eq!(reconciler.buffer_len(), 0, "buffer_event should be no-op in live mode");
}

// ── Replay dedup tests ───────────────────────────────────────────

/// Replay dedup: 500 removal events for the same path (like a SQLite
/// journal file) collapse to a single merged event.
#[test]
fn replay_dedup_collapses_duplicate_events() {
    let mut pending = HashMap::<String, watcher::FsChangeEvent>::new();

    for i in 0..500 {
        let path = "/Users/test/Library/peewee-sqlite.db-journal".to_string();
        let event = make_event(
            &path,
            1000 + i,
            watcher::FsEventFlags {
                item_removed: true,
                item_is_file: true,
                ..Default::default()
            },
        );
        pending
            .entry(path)
            .and_modify(|existing| {
                *existing = merge_fs_events(existing, &event);
            })
            .or_insert(event);
    }

    assert_eq!(pending.len(), 1, "500 events for same path should collapse to 1");
    let merged = pending.values().next().unwrap();
    assert_eq!(merged.event_id, 1499, "highest event_id should be kept");
    assert!(merged.flags.item_removed, "item_removed flag should be preserved");
}

/// Replay dedup: events for different paths are all preserved while
/// duplicates within each path are merged.
#[test]
fn replay_dedup_preserves_distinct_paths_merges_duplicates() {
    let mut pending = HashMap::<String, watcher::FsChangeEvent>::new();

    // 100 events: 10 paths x 10 events each
    for path_idx in 0..10u64 {
        for event_idx in 0..10u64 {
            let path = format!("/path/{path_idx}/file.txt");
            let event = make_event(
                &path,
                path_idx * 10 + event_idx,
                watcher::FsEventFlags {
                    item_modified: true,
                    item_is_file: true,
                    ..Default::default()
                },
            );
            pending
                .entry(path)
                .and_modify(|existing| {
                    *existing = merge_fs_events(existing, &event);
                })
                .or_insert(event);
        }
    }

    assert_eq!(pending.len(), 10, "10 unique paths should be preserved");
    for path_idx in 0..10u64 {
        let path = format!("/path/{path_idx}/file.txt");
        let event = &pending[&path];
        assert_eq!(
            event.event_id,
            path_idx * 10 + 9,
            "each path should keep its highest event_id"
        );
    }
}

/// Replay dedup: mixed create/modify/remove events for the same path
/// merge with correct flag priority (removed wins).
#[test]
fn replay_dedup_mixed_events_merge_correctly() {
    let mut pending = HashMap::<String, watcher::FsChangeEvent>::new();
    let path = "/test/file.txt".to_string();

    let events = [
        (
            1,
            watcher::FsEventFlags {
                item_created: true,
                item_is_file: true,
                ..Default::default()
            },
        ),
        (
            2,
            watcher::FsEventFlags {
                item_modified: true,
                item_is_file: true,
                ..Default::default()
            },
        ),
        (
            3,
            watcher::FsEventFlags {
                item_modified: true,
                item_is_file: true,
                ..Default::default()
            },
        ),
        (
            4,
            watcher::FsEventFlags {
                item_removed: true,
                item_is_file: true,
                ..Default::default()
            },
        ),
    ];

    for (id, flags) in events {
        let event = make_event(&path, id, flags);
        pending
            .entry(path.clone())
            .and_modify(|existing| {
                *existing = merge_fs_events(existing, &event);
            })
            .or_insert(event);
    }

    assert_eq!(pending.len(), 1);
    let merged = &pending[&path];
    assert!(merged.flags.item_removed, "removed should win over created+modified");
    assert!(
        !merged.flags.item_created,
        "created should be dropped when removed wins"
    );
    assert_eq!(merged.event_id, 4, "highest event_id should be kept");
}

/// Replay dedup: simulates realistic event storm with a mix of high-churn
/// paths (SQLite journals, Chrome cache) and unique paths. Verifies the
/// dedup ratio matches expectations.
#[test]
fn replay_dedup_realistic_event_storm() {
    let mut pending = HashMap::<String, watcher::FsChangeEvent>::new();
    let mut raw_count = 0u64;

    // 500 events for a SQLite journal (same path, rapid create/delete)
    for i in 0..500 {
        let path = "/Users/test/Library/aw-server/peewee-sqlite.db-journal".to_string();
        let event = make_event(
            &path,
            i,
            watcher::FsEventFlags {
                item_removed: true,
                item_is_file: true,
                ..Default::default()
            },
        );
        pending
            .entry(path)
            .and_modify(|e| *e = merge_fs_events(e, &event))
            .or_insert(event);
        raw_count += 1;
    }

    // 200 events for Chrome cache (20 different todelete_ files, 10 events each)
    for file_idx in 0..20 {
        for event_idx in 0..10 {
            let path = format!("/Users/test/Library/Chrome/todelete_{file_idx:04x}");
            let event = make_event(
                &path,
                500 + file_idx * 10 + event_idx,
                watcher::FsEventFlags {
                    item_removed: true,
                    item_is_file: true,
                    ..Default::default()
                },
            );
            pending
                .entry(path)
                .and_modify(|e| *e = merge_fs_events(e, &event))
                .or_insert(event);
            raw_count += 1;
        }
    }

    // 50 unique file modifications (no duplicates)
    for i in 0..50 {
        let path = format!("/Users/test/projects/file_{i}.rs");
        let event = make_event(
            &path,
            700 + i,
            watcher::FsEventFlags {
                item_modified: true,
                item_is_file: true,
                ..Default::default()
            },
        );
        pending
            .entry(path)
            .and_modify(|e| *e = merge_fs_events(e, &event))
            .or_insert(event);
        raw_count += 1;
    }

    assert_eq!(raw_count, 750, "should have 750 raw events");
    // 1 (journal) + 20 (chrome) + 50 (unique) = 71 unique paths
    assert_eq!(pending.len(), 71, "should deduplicate to 71 unique paths");
}
