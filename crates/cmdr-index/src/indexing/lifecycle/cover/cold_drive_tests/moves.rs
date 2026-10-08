//! Renaming a drive while it indexes. The mount point moves while the drive
//! stays mounted, and the index has to keep up with it there: listings, live
//! changes, and every row it already holds.

use std::path::PathBuf;

use super::toggles::{an_indexed_drive, indexed, turn_on};
use super::*;
use crate::indexing::lifecycle::state;

/// The file every drive here starts with (`toggles::an_indexed_drive`).
const PROOF: &str = "scope/found.txt";

/// An indexed drive on the REAL FSEvents journal, for the tests that prove the
/// index is live at the new mount point by a change the OS has to deliver.
///
/// ⚠️ Real from the start, ❌ not only at the new root: a fake journal's synthetic
/// event ids would be stored as the drive's `last_event_id` and then handed to the
/// real stream the move restarts, which watches from a point in time that never
/// existed and delivers nothing.
fn indexed_for_real(volume_id: &'static str) -> ColdDrive {
    indexed(ColdDrive::watched_for_real(volume_id))
}

/// Moves a drive's tree back where its `TempDir` will look for it, on any exit.
struct PutBack {
    from: PathBuf,
    to: PathBuf,
}

impl Drop for PutBack {
    fn drop(&mut self) {
        let _ = std::fs::rename(&self.from, &self.to);
    }
}

/// Rename the drive's mount point to a sibling, and have the host report it there,
/// the way the app's registry does once macOS says the volume was renamed.
fn rename_the_drive(drive: &ColdDrive) -> (PathBuf, PutBack) {
    let old_root = drive.tree.path().to_path_buf();
    let mut name = old_root.file_name().expect("a named tree").to_os_string();
    name.push("-renamed");
    let new_root = old_root.with_file_name(name);
    std::fs::rename(&old_root, &new_root).expect("rename the drive's mount point");
    drive.volumes.register(
        drive.volume_id,
        Arc::new(
            cmdr_fs::volume::InMemoryVolume::new("Renamed")
                .with_root(&new_root)
                .with_local_fs_access(),
        ),
    );
    let put_back = PutBack {
        from: new_root.clone(),
        to: old_root,
    };
    (new_root, put_back)
}

/// Whether the drive's index holds a row for `relative`. Rows are mount-relative,
/// so the answer doesn't depend on where the drive is mounted.
fn holds(drive: &ColdDrive, relative: &str) -> bool {
    let Ok(conn) = IndexStore::open_read_connection(&drive.db_path()) else {
        return false;
    };
    crate::indexing::store::resolve_path(&conn, &format!("/{relative}"))
        .ok()
        .flatten()
        .is_some()
}

/// **The headline case.** A drive renamed while it indexes keeps indexing at its
/// new mount point. A file that arrived while nothing listened at either path is
/// found by the listing the restart walks, a file created afterwards lands through
/// the watcher, and nothing the index held before the rename goes missing.
#[test]
fn a_drive_renamed_while_it_indexes_keeps_indexing_at_its_new_mount_point() {
    let drive = indexed_for_real("cover-move-renamed-drive-test");
    let (new_root, _put_back) = rename_the_drive(&drive);
    std::fs::write(new_root.join("scope/in-the-gap.txt"), "y").expect("write while nothing listens");

    let followed = drive.index.follow_volume_move(drive.volume_id);
    assert!(holds(&drive, PROOF), "following the move deletes nothing it held");

    // 30 s, and no redo possible: the gap file has to predate the move. A completed
    // drive's restart catches up through the JOURNAL when it can, so this rides a
    // real `fseventsd` replay, which took over 20 s about one run in 20 on a loaded
    // host (2026-10-07).
    cmdr_fs::testing::wait_until(
        std::time::Duration::from_secs(30),
        "the listing at the new mount point to find what arrived in the gap",
        || holds(&drive, "scope/in-the-gap.txt"),
    );
    assert_live_at(&drive, &new_root, "arrived");

    assert!(followed, "the index says it followed the drive");
    assert!(
        holds(&drive, PROOF),
        "and what it held before the rename is all still there"
    );
    assert!(state::is_active(drive.volume_id), "and the drive is still indexing");
}

/// Wait for a file created at `root` to land through the watcher: the proof the
/// index is live there, rather than an `Ok` from the call that put it there.
fn assert_live_at(drive: &ColdDrive, root: &Path, stem: &str) {
    until_the_watcher_delivers(&root.join("scope"), stem, std::time::Duration::from_secs(15), |name| {
        holds(drive, &format!("scope/{name}"))
    });
}

/// A drive the user turned off before its rename stays off: a move restarts an
/// index that's running, ❌ never one the user's last word stopped.
#[test]
fn a_drive_turned_off_before_its_rename_stays_off() {
    let drive = an_indexed_drive("cover-move-turned-off-test");
    drive.index.disable_volume(drive.volume_id).expect("the drive stops");
    let (_new_root, _put_back) = rename_the_drive(&drive);

    assert!(
        !drive.index.follow_volume_move(drive.volume_id),
        "nothing is indexing, so there's nothing to follow"
    );
    assert!(!state::is_active(drive.volume_id), "and the drive stays off");
}

/// A rename landing while a toggle's restart waits out a drain moves THAT restart:
/// the drive comes back at its new root, where the old one no longer exists.
#[test]
fn a_rename_inside_a_drain_window_moves_the_start_that_window_carries() {
    let drive = indexed_for_real("cover-move-inside-the-drain-test");
    let mut new_root = None;
    let mut put_back = None;

    state::while_stopping_for_test(drive.volume_id, || {
        // Recorded at the OLD root: the host still reports it there.
        turn_on(&drive);
        let (root, guard) = rename_the_drive(&drive);
        assert!(
            drive.index.follow_volume_move(drive.volume_id),
            "the start the drain carries follows the drive"
        );
        new_root = Some(root);
        put_back = Some(guard);
    });

    let new_root = new_root.expect("the drive was renamed");
    assert!(state::is_active(drive.volume_id), "the toggle's start ran");
    assert_live_at(&drive, &new_root, "after-the-drain");
    drop(put_back);
}

/// A rename landing while a scan start holds the manager out of the registry
/// takes effect at the handback: the manager it hands back is still at the old
/// root, so the handback drains it and the restart brings it up at the new one.
#[test]
fn a_rename_while_a_scan_starts_restarts_the_drive_at_the_handback() {
    let drive = indexed_for_real("cover-move-while-detached-test");
    let mut new_root = None;
    let mut put_back = None;

    state::while_detached_for_test(drive.volume_id, || {
        let (root, guard) = rename_the_drive(&drive);
        assert!(
            drive.index.follow_volume_move(drive.volume_id),
            "the detached drive records the move"
        );
        new_root = Some(root);
        put_back = Some(guard);
    });

    let new_root = new_root.expect("the drive was renamed");
    assert_live_at(&drive, &new_root, "after-the-handback");
    assert!(state::is_active(drive.volume_id), "and the drive is indexing");
    drop(put_back);
}
