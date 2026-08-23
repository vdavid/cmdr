//! The tap's own guard (every live-batch driver builds a real activity observer), the
//! documented flags priority, and what one folded batch reports.

use std::sync::Arc;

use super::*;
use crate::indexing::events::{IndexEvent, RecordingSink};

/// Flags with everything off but what the test names.
fn flags(created: bool, removed: bool, renamed: bool, modified: bool) -> FsEventFlags {
    FsEventFlags {
        item_created: created,
        item_removed: removed,
        item_renamed: renamed,
        item_modified: modified,
        ..Default::default()
    }
}

/// An observer plus the sink it reports into.
fn observer() -> (ActivityObserver, Arc<RecordingSink>) {
    let sink = Arc::new(RecordingSink::new());
    (ActivityObserver::new("root", sink.clone()), sink)
}

/// The rollups of the single `FolderActivity` the sink recorded, folder-sorted.
fn rollups(sink: &RecordingSink) -> Vec<FolderChangeRollup> {
    let mut found: Vec<FolderChangeRollup> = sink
        .events()
        .into_iter()
        .flat_map(|event| match event {
            IndexEvent::FolderActivity { folders, .. } => folders,
            _ => Vec::new(),
        })
        .collect();
    found.sort_by(|a, b| a.folder.cmp(&b.folder));
    found
}

// ── The flags priority ───────────────────────────────────────────────

/// ⚠️ The flags are NOT one-hot: one coalesced event can carry created, removed, and renamed
/// at once. The documented order is renamed → created → removed → modified, and a different
/// order moves what a consumer reads out of the counts materially, so it is pinned here rather
/// than left to whichever branch the code happens to test first.
#[test]
fn a_multi_flag_event_counts_as_the_documented_winner() {
    assert_eq!(
        kind_of(&flags(true, true, true, true)),
        Some(ChangeKind::Renamed),
        "renamed outranks everything: it is the strongest statement of intent in the batch"
    );
    assert_eq!(
        kind_of(&flags(true, true, false, true)),
        Some(ChangeKind::Created),
        "created outranks removed, so a create-then-delete cycle reads as arrival"
    );
    assert_eq!(
        kind_of(&flags(false, true, false, true)),
        Some(ChangeKind::Removed),
        "removed outranks modified, the churn floor"
    );
    assert_eq!(kind_of(&flags(false, false, false, true)), Some(ChangeKind::Modified));
}

/// An event naming no change at all (a bare `must_scan_sub_dirs` anchor) counts as nothing.
/// Inventing a kind for it would turn every rescan into reported activity.
#[test]
fn an_event_with_no_change_flag_counts_as_nothing() {
    let anchor = FsEventFlags {
        must_scan_sub_dirs: true,
        item_is_dir: true,
        ..Default::default()
    };
    assert_eq!(kind_of(&anchor), None);
}

// ── Where a change is counted ────────────────────────────────────────

/// ⚠️ A directory's OWN event counts in its PARENT, exactly like a file's. A rollup describes
/// the folder a change happened IN, and `/a/b` appearing is a change in `/a`.
#[test]
fn a_directorys_own_event_counts_in_its_parent() {
    let (mut obs, sink) = observer();
    let mut dir = flags(true, false, false, false);
    dir.item_is_dir = true;
    obs.record_event("/Users/someone/Downloads/new-folder", &dir);
    obs.record_event("/Users/someone/Downloads/note.txt", &flags(true, false, false, false));
    obs.report(1_780_000_027);

    let found = rollups(&sink);
    assert_eq!(found.len(), 1, "both changes happened in the same folder");
    assert_eq!(found[0].folder, "/Users/someone/Downloads");
    assert_eq!(found[0].created, 2);
}

/// A removal storm is the one input credited to the named folder itself: the anchor is where
/// the storm HAPPENED, and every per-file event under it is about to be dropped in favour of
/// one subtree rescan. One removal, since one emptied folder is all the batch still knows.
#[test]
fn a_storm_anchor_counts_one_removal_inside_the_anchor() {
    let (mut obs, sink) = observer();
    obs.record_storm_anchor("/Users/someone/projects/app/target");
    obs.report(1_780_000_027);

    let found = rollups(&sink);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].folder, "/Users/someone/projects/app/target");
    assert_eq!(found[0].removed, 1);
}

/// A path with no parent names no folder, so it is ignored rather than panicking or crediting
/// an empty string.
#[test]
fn a_parentless_path_is_ignored() {
    let (mut obs, sink) = observer();
    obs.record_event("/", &flags(false, true, false, false));
    obs.record_event("", &flags(false, true, false, false));
    obs.record_event("bare", &flags(false, true, false, false));
    obs.report(1_780_000_027);

    assert!(rollups(&sink).is_empty());
    assert!(sink.events().is_empty(), "a batch with nothing to say emits nothing");
}

// ── What a folded batch reports ──────────────────────────────────────

/// The counts arrive per kind and per folder, and the batch's instant rides both fields, so a
/// host quantizing one of them still has the other exact.
#[test]
fn a_batch_reports_one_rollup_per_folder_with_its_counts() {
    let (mut obs, sink) = observer();
    obs.record_event("/a/one.txt", &flags(true, false, false, false));
    obs.record_event("/a/two.txt", &flags(false, false, false, true));
    obs.record("/a/three.txt", ChangeKind::Renamed);
    obs.record_event("/b/gone.txt", &flags(false, true, false, false));
    obs.report(1_780_000_027);

    let found = rollups(&sink);
    assert_eq!(found.len(), 2);
    assert_eq!(found[0].folder, "/a");
    assert_eq!((found[0].created, found[0].modified, found[0].renamed), (1, 1, 1));
    assert_eq!(found[0].last_event_at, 1_780_000_027);
    assert_eq!(found[1].folder, "/b");
    assert_eq!(found[1].removed, 1);
}

/// Nothing survives a batch. The map is drained on report, so memory is bounded by ONE batch's
/// folders rather than by everything the loop has ever seen.
#[test]
fn a_reported_batch_leaves_nothing_behind_for_the_next_one() {
    let (mut obs, sink) = observer();
    obs.record_event("/a/one.txt", &flags(true, false, false, false));
    obs.report(1_780_000_027);
    obs.record_event("/b/two.txt", &flags(true, false, false, false));
    obs.report(1_780_000_088);

    let found = rollups(&sink);
    assert_eq!(found.len(), 2, "one rollup from each batch, never a running total");
    assert_eq!(found[0].created, 1);
    assert_eq!(found[1].created, 1);
}

/// Nothing bounds a batch's distinct FOLDERS except the disk, so the tap caps them rather than
/// handing a host half a million rollups to loop over on the live-loop thread.
#[test]
fn a_batch_past_the_folder_cap_reports_the_cap_and_no_more() {
    let (mut obs, sink) = observer();
    for i in 0..MAX_FOLDERS_PER_BATCH + 50 {
        obs.record_event(&format!("/folder{i}/file.txt"), &flags(true, false, false, false));
    }
    obs.report(1_780_000_027);

    assert_eq!(rollups(&sink).len(), MAX_FOLDERS_PER_BATCH);
}

/// The sibling of `churn_monitor::tests::every_live_loop_owns_a_real_churn_observer`, and it
/// exists for the same reason that one does: the cold-start journal-replay path runs a SECOND
/// live loop, and an observer wired into only one of them measures nothing on a whole boot
/// route while every unit test passes. `live.rs`'s own history records that happening once.
///
/// `process_live_batch` takes the observers by `&mut`, so the compiler enforces that a batch
/// cannot be processed without one. This guards the hole the compiler can't see: a NEW live
/// loop in a third file, or an existing one quietly downgrading to a disabled bundle.
///
/// The scan is RECURSIVE and skips `tests` directories, since a test harness driving
/// `process_live_batch` with a disabled bundle is legitimate.
#[test]
fn every_live_loop_owns_a_real_activity_tap() {
    fn collect(dir: &std::path::Path, prefix: &str, out: &mut Vec<(String, std::path::PathBuf)>) {
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

    let event_loop = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/indexing/watch/event_loop");
    let mut sources: Vec<(String, std::path::PathBuf)> = Vec::new();
    collect(&event_loop, "", &mut sources);

    let mut drivers: Vec<String> = Vec::new();
    for (name, path) in sources {
        let src = std::fs::read_to_string(&path).expect("read source");
        if !src.contains("process_live_batch(") {
            continue;
        }
        assert!(
            src.contains("BatchObservers::from_env("),
            // allowed-pluralize-noun: `{name}` is a file name and `drives` is the verb, not a plural noun.
            "{name} drives live batches but never builds a real observer bundle, so the folder-activity \
             rollups would silently stop being produced on that route"
        );
        drivers.push(name);
    }
    drivers.sort();
    assert_eq!(
        drivers,
        vec!["live.rs".to_string(), "replay.rs".to_string()],
        "the set of live-batch drivers changed; wire the new one's BatchObservers, then update this list"
    );
}
