//! The activity tap end to end: a synthetic batch through `process_live_batch`, asserting the
//! `FolderActivity` it emits.
//!
//! The unit-level folding is `watch/activity_monitor/tests.rs`. What these cover is the part
//! only a real batch can show: that the three places the corrected stream is UNREACHABLE from
//! its natural reading point are actually wired, so the tap counts what happened rather than
//! the leftovers.

use super::*;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use super::rename::{insert_path_chain, rename_test_setup, rename_test_tempdir};
use crate::indexing::events::{IndexEvent, RecordingSink};
use crate::indexing::reconcile::reconciler::EventReconciler;
use crate::indexing::store::ROOT_ID;
use crate::indexing::watch::activity_monitor::BatchObservers;
use crate::indexing::writer::IndexWriter;

/// Flags for a renamed entry that is a directory.
fn renamed_dir() -> watcher::FsEventFlags {
    watcher::FsEventFlags {
        item_renamed: true,
        item_is_dir: true,
        ..Default::default()
    }
}

/// Flags for a created file.
fn created_file() -> watcher::FsEventFlags {
    watcher::FsEventFlags {
        item_created: true,
        item_is_file: true,
        ..Default::default()
    }
}

/// Flags for a removed file that is gone from disk.
fn removed_file_flags() -> watcher::FsEventFlags {
    watcher::FsEventFlags {
        item_removed: true,
        item_is_file: true,
        ..Default::default()
    }
}

/// Run one batch through `process_live_batch` with a real tap, and return the rollups it
/// reported, folder-sorted, plus the batch instant they carried.
fn tapped_batch(
    pending_events: &mut HashMap<String, watcher::FsChangeEvent>,
    reconciler: &mut EventReconciler,
    writer: &IndexWriter,
    db_path: &Path,
) -> (u64, Vec<crate::indexing::events::FolderChangeRollup>) {
    let sink = Arc::new(RecordingSink::new());
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async {
        let conn = IndexStore::open_write_connection(db_path).unwrap();
        let mut pending_paths = HashSet::new();
        process_live_batch(
            pending_events,
            reconciler,
            &IndexPathSpace::root(),
            &conn,
            writer,
            &mut pending_paths,
            &mut BatchObservers::tapping("root", sink.clone()),
        );
    });
    writer.flush_blocking().unwrap();

    let mut observed_at = 0u64;
    let mut rollups = Vec::new();
    for event in sink.events() {
        if let IndexEvent::FolderActivity {
            volume_id,
            observed_at: at,
            folders,
        } = event
        {
            assert_eq!(volume_id, "root", "the rollups carry the loop's own volume");
            observed_at = at;
            rollups.extend(folders);
        }
    }
    rollups.sort_by(|a, b| a.folder.cmp(&b.folder));
    (observed_at, rollups)
}

/// A batch of creates and removals reports one rollup per folder, with the counts split by
/// kind and the batch's own instant on both time fields.
#[test]
fn a_live_batch_reports_its_folders_and_counts() {
    let (writer, db_path, _db_dir) = rename_test_setup();
    insert_path_chain(&db_path, Path::new("/Users/cmdrtap/Downloads"), &writer);

    let mut pending_events: HashMap<String, watcher::FsChangeEvent> = HashMap::new();
    for i in 0..3 {
        let p = format!("/Users/cmdrtap/Downloads/new{i}.txt");
        pending_events.insert(p.clone(), make_event(&p, 100 + i, created_file()));
    }
    let gone = "/Users/cmdrtap/Downloads/old.txt".to_string();
    pending_events.insert(gone.clone(), make_event(&gone, 110, removed_file_flags()));

    let mut reconciler = EventReconciler::new();
    reconciler.switch_to_live();
    let (observed_at, rollups) = tapped_batch(&mut pending_events, &mut reconciler, &writer, &db_path);

    assert_eq!(rollups.len(), 1, "every change landed in one folder");
    assert_eq!(rollups[0].folder, "/Users/cmdrtap/Downloads");
    assert_eq!(rollups[0].created, 3);
    assert_eq!(rollups[0].removed, 1);
    assert!(observed_at > 0, "the batch stamps its own instant");
    assert_eq!(
        rollups[0].last_event_at, observed_at,
        "a live batch spans milliseconds, so its newest change IS its instant"
    );

    writer.shutdown();
}

/// ⚠️ **The regression anchor for the matched-rename change.** `detect_renames_by_inode`
/// `retain`s its matches OUT of the batch, so a tap reading only what Pass 2 still holds would
/// see nothing here at all — and a consumer scoring intent would read a rename-only batch as
/// no activity whatsoever.
#[test]
fn a_rename_only_batch_still_reports_a_rename() {
    let fs_root = rename_test_tempdir();
    let new_dir_path = fs_root.path().join("Renamed");
    std::fs::create_dir(&new_dir_path).expect("create renamed dir");
    let inode = std::os::unix::fs::MetadataExt::ino(&std::fs::symlink_metadata(&new_dir_path).unwrap());

    let (writer, db_path, _db_dir) = rename_test_setup();
    let parent_id = insert_path_chain(&db_path, fs_root.path(), &writer);
    {
        let conn = IndexStore::open_write_connection(&db_path).unwrap();
        IndexStore::insert_entry_v2(&conn, parent_id, "Original", true, false, None, None, None, Some(inode)).unwrap();
    }
    assert_ne!(parent_id, ROOT_ID, "the rename's parent is a real folder");

    let mut pending_events: HashMap<String, watcher::FsChangeEvent> = HashMap::new();
    let new_path = new_dir_path.to_string_lossy().to_string();
    pending_events.insert(new_path.clone(), make_event(&new_path, 300, renamed_dir()));

    let mut reconciler = EventReconciler::new();
    reconciler.switch_to_live();
    let (_observed_at, rollups) = tapped_batch(&mut pending_events, &mut reconciler, &writer, &db_path);

    assert_eq!(rollups.len(), 1, "the matched rename is still reported");
    assert_eq!(
        rollups[0].folder,
        fs_root.path().to_string_lossy(),
        "a directory's own event counts in its PARENT"
    );
    assert_eq!(rollups[0].renamed, 1);

    writer.shutdown();
}

/// ⚠️ A removal storm drops every strict-descendant removal in favour of one subtree rescan,
/// so without the anchor being surfaced a sixty-thousand-file delete inside a surviving folder
/// would report nothing. It reports ONE removal at the anchor, which is what the batch still
/// honestly knows.
#[test]
fn a_removal_storm_reports_one_removal_at_its_anchor() {
    let (writer, db_path, _db_dir) = rename_test_setup();
    let base = "/Users/cmdrtap/ws/p1/p2/p3/deep/bulk";
    let n = storm::REMOVAL_STORM_THRESHOLD + 1;
    insert_path_chain(&db_path, Path::new(base), &writer);

    let mut pending_events: HashMap<String, watcher::FsChangeEvent> = HashMap::new();
    for i in 0..n {
        let p = format!("{base}/item{i}.dat");
        pending_events.insert(p.clone(), make_event(&p, 400 + i as u64, removed_file_flags()));
    }

    let mut reconciler = EventReconciler::new();
    reconciler.switch_to_live();
    reconciler.set_rescan_active_for_test(true);
    let (_observed_at, rollups) = tapped_batch(&mut pending_events, &mut reconciler, &writer, &db_path);

    assert_eq!(rollups.len(), 1, "one anchor, not two hundred per-file removals");
    assert_eq!(rollups[0].folder, base, "credited to the folder the storm emptied");
    assert_eq!(rollups[0].removed, 1);

    writer.shutdown();
}

/// A batch that changed nothing countable says nothing, so a quiet loop stays quiet on the
/// event seam too.
#[test]
fn an_empty_batch_reports_nothing() {
    let (writer, db_path, _db_dir) = rename_test_setup();
    let mut pending_events: HashMap<String, watcher::FsChangeEvent> = HashMap::new();
    let mut reconciler = EventReconciler::new();
    reconciler.switch_to_live();

    let (_observed_at, rollups) = tapped_batch(&mut pending_events, &mut reconciler, &writer, &db_path);

    assert!(rollups.is_empty());
    writer.shutdown();
}
