//! Tests for the event loop, clustered onto the production seams:
//! - `activity`: the per-folder activity tap end to end through `process_live_batch`.
//! - `merge`: `merge_fs_events` dedup/flag-priority, buffer overflow/mode, and
//!   replay-dedup tests (the event-buffer behavior).
//! - `rename`: inode rename pre-pass, removal-storm coalescing, and the
//!   `process_live_batch` end-to-end rename, plus their shared fixtures.
//! - `split_parent`: the `split_parent_and_name` pure-helper tests.
//!
//! Production items resolve through `use super::*` (this module's `super` is
//! `event_loop`, so the root's re-exports and imports — `watcher`,
//! `merge_fs_events`, `process_live_batch`, `store`, `IndexStore`,
//! `IndexPathSpace`, `Path`, the `storm`/`live` submodules — are in scope and
//! chain into the cluster files via their own `use super::*`). Items that moved
//! into submodules (`detect_renames_by_inode`, `split_parent_and_name`) and
//! indexing-level types (`EventReconciler`, `IndexWriter`) are imported
//! explicitly where used.

use super::*;

mod activity;
mod ingestion;
mod merge;
mod rename;
mod split_parent;

/// Shared across the `merge` and `rename` clusters.
fn make_event(path: &str, event_id: u64, flags: watcher::FsEventFlags) -> watcher::FsChangeEvent {
    watcher::FsChangeEvent {
        path: path.to_string(),
        event_id,
        flags,
    }
}

/// Every live loop must publish its ORIGIN dirs on the dir-changed bus, not just
/// emit to the frontend.
///
/// The bug this pins: only the cold-start replay loop published, so a volume that
/// took the POST-SCAN route (`run_live_event_loop`) refreshed the UI but never woke
/// the importance rescore or the media live tick — their derived data went stale
/// until the next `ScanCompleted`, on a route every unit test passed. The scan is
/// the same shape as `every_live_loop_owns_a_real_churn_observer`, for the same
/// reason: a third live loop in a new file would otherwise slip past this guard.
#[test]
fn every_live_loop_publishes_its_changed_dirs() {
    fn collect(dir: &Path, prefix: &str, out: &mut Vec<(String, std::path::PathBuf)>) {
        for entry in std::fs::read_dir(dir).expect("event_loop dir") {
            let path = entry.expect("dir entry").path();
            let name = path.file_name().expect("file name").to_string_lossy().to_string();
            let rel = if prefix.is_empty() {
                name.clone()
            } else {
                format!("{prefix}/{name}")
            };
            if path.is_dir() {
                if name == "tests" {
                    continue;
                }
                collect(&path, &rel, out);
            } else if path.extension().is_some_and(|e| e == "rs") && !name.ends_with("tests.rs") {
                out.push((rel, path));
            }
        }
    }

    let event_loop = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/indexing/watch/event_loop");
    let mut sources: Vec<(String, std::path::PathBuf)> = Vec::new();
    collect(&event_loop, "", &mut sources);

    let mut publishers: Vec<String> = Vec::new();
    for (name, path) in sources {
        let src = std::fs::read_to_string(&path).expect("read source");
        if !src.contains("mark_pending_and_drain(") {
            continue;
        }
        assert!(
            src.contains("publish_dirs_changed("),
            // allowed-pluralize-noun: "drains" is a verb here (the file drains the set), not a plural noun after a count
            "{name} drains live changed dirs but never publishes them, so importance and media \
             would silently stop following the index on that route"
        );
        publishers.push(name);
    }
    publishers.sort();
    assert_eq!(
        publishers,
        vec!["live.rs".to_string(), "replay.rs".to_string()],
        "both live loops publish; a new one must too"
    );
}
