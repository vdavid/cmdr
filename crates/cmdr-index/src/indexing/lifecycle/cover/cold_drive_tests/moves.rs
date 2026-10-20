//! Renaming a drive while it indexes. The mount point moves while the drive
//! stays mounted, and the index has to keep up with it there: listings, live
//! changes, and every row it already holds.
//!
//! These run on the SCRIPTED journal (`watch/watcher/fake_journal.rs`): a change
//! reaches the index because the test records it, replayed from the drive's stored
//! event id or delivered live, so what they pin is the index's own move, restart,
//! and replay logic, in milliseconds. That FSEvents really replays a renamed path
//! from the old id is the OS's half, pinned on the real journal by
//! [`real_journal`], which runs in the opt-in macOS lane.

use std::path::PathBuf;

use super::toggles::{an_indexed_drive, turn_on};
use super::*;
use crate::indexing::lifecycle::state;

/// The file every drive here starts with (`toggles::an_indexed_drive`).
const PROOF: &str = "scope/found.txt";

/// Moves a drive's tree back where its `TempDir` will look for it, on any exit.
struct PutBack {
    from: PathBuf,
    to: PathBuf,
    /// The scripted journal over the new root, for as long as the drive lives there.
    #[cfg(target_os = "macos")]
    _journal: Option<crate::indexing::watch::watcher::fake_journal::Guard>,
}

impl Drop for PutBack {
    fn drop(&mut self) {
        let _ = std::fs::rename(&self.from, &self.to);
    }
}

/// Rename the drive's mount point to a sibling, and have the host report it there,
/// the way the app's registry does once macOS says the volume was renamed. A drive
/// on the scripted journal keeps it at the new root.
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
        #[cfg(target_os = "macos")]
        _journal: drive
            ._journal
            .as_ref()
            .map(|_| crate::indexing::watch::watcher::fake_journal::fake_for(&new_root)),
    };
    (new_root, put_back)
}

/// Create a file, and tell the scripted journal so, which is what makes it a
/// change the index can hear about. Off macOS the watcher is the real inotify one
/// and needs no telling.
fn create_a_file(drive: &ColdDrive, path: &Path) {
    std::fs::write(path, "w").expect("a change for the watcher to deliver");
    #[cfg(target_os = "macos")]
    if !on_a_real_watcher(drive) {
        crate::indexing::watch::watcher::fake_journal::record_created_file(path);
    }
    #[cfg(not(target_os = "macos"))]
    let _ = drive;
}

/// Whether the drive's changes reach it through a real OS watcher: the real
/// FSEvents journal on macOS ([`real_journal`]), and inotify everywhere else,
/// which has no shared daemon to starve on.
fn on_a_real_watcher(drive: &ColdDrive) -> bool {
    #[cfg(target_os = "macos")]
    return drive._journal.is_none();
    #[cfg(not(target_os = "macos"))]
    {
        let _ = drive;
        true
    }
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

/// How long a change gets to reach the index. A scripted delivery (and inotify's,
/// off macOS) lands in milliseconds; this only bounds how long a broken path takes
/// to say so, under the 8 s nextest cap.
const LANDS_WITHIN: std::time::Duration = std::time::Duration::from_secs(5);

/// **The headline case.** A drive renamed while it indexes keeps indexing at its
/// new mount point. A file that arrived while nothing listened at either path is
/// found by the restart's replay, a file created afterwards lands through the
/// watcher, and nothing the index held before the rename goes missing.
#[test]
fn a_drive_renamed_while_it_indexes_keeps_indexing_at_its_new_mount_point() {
    let drive = an_indexed_drive("cover-move-renamed-drive-test");
    the_drive_keeps_indexing_at_its_new_mount_point(&drive, LANDS_WITHIN);
}

/// The headline case's body, for the scripted journal above and the real one in
/// [`real_journal`]. `patience` is how long each change gets to land.
fn the_drive_keeps_indexing_at_its_new_mount_point(drive: &ColdDrive, patience: std::time::Duration) {
    let (new_root, _put_back) = rename_the_drive(drive);
    create_a_file(drive, &new_root.join("scope/in-the-gap.txt"));

    let followed = drive.index.follow_volume_move(drive.volume_id);
    assert!(holds(drive, PROOF), "following the move deletes nothing it held");

    // No redo possible: the gap file has to predate the move. The test doesn't wait
    // for the first index to complete, so either restart can catch it: a completed
    // drive's reconciles by LISTING its new root, and one still in phases replays
    // the JOURNAL from the id it stored at the old root.
    cmdr_fs::testing::wait_until(
        patience,
        "the restart at the new mount point to replay what arrived in the gap",
        || holds(drive, "scope/in-the-gap.txt"),
    );
    assert_live_at(drive, &new_root, "arrived", patience);

    assert!(followed, "the index says it followed the drive");
    assert!(
        holds(drive, PROOF),
        "and what it held before the rename is all still there"
    );
    assert!(state::is_active(drive.volume_id), "and the drive is still indexing");
}

/// A rename landing after a walk wrote its rows but before it told the branch set
/// it ended keeps the index: the restart resumes it, ❌ never rebuilds it.
///
/// The walk's rows were on disk while the record of the ground they describe was
/// still waiting on the walk's release, which a volume in its drain window never
/// hears. The restart then read rows with no record behind them as a legacy
/// partial and threw the whole index away (#582).
#[test]
fn a_rename_between_a_walks_last_row_and_its_release_keeps_the_index() {
    let drive = ColdDrive::new("cover-move-before-the-release-test");
    std::fs::create_dir_all(drive.tree.path().join("scope")).expect("dirs");
    std::fs::write(drive.tree.path().join(PROOF), "x").expect("file");
    let unannounced = drive.tree.path().join("scope/gone-unannounced.txt");
    std::fs::write(&unannounced, "x").expect("file");
    let gate = finish_gate::arm(drive.volume_id);
    turn_on(&drive);
    gate.wait_until_parked();
    cmdr_fs::testing::wait_until(LANDS_WITHIN, "the walk's rows and marks to land", || {
        holds(&drive, PROOF) && drive.coverage(&drive.path("")).frontier.is_empty()
    });
    // Its row is one only a re-walk drops: nothing tells the journal it went.
    std::fs::remove_file(&unannounced).expect("remove it behind the index's back");

    let (new_root, _put_back) = rename_the_drive(&drive);
    create_a_file(&drive, &new_root.join("scope/in-the-gap.txt"));
    assert!(
        drive.index.follow_volume_move(drive.volume_id),
        "the index follows the drive"
    );
    gate.release();

    cmdr_fs::testing::wait_until(LANDS_WITHIN, "the restart to replay what arrived in the gap", || {
        holds(&drive, "scope/in-the-gap.txt")
    });
    assert!(
        holds(&drive, "scope/gone-unannounced.txt"),
        "the restart resumed the index the walk wrote rather than rebuilding it"
    );
    assert!(holds(&drive, PROOF), "and kept what it held");
}

/// Wait for a file created at `root` to land through the watcher: the proof the
/// index is live there, rather than an `Ok` from the call that put it there.
fn assert_live_at(drive: &ColdDrive, root: &Path, stem: &str, patience: std::time::Duration) {
    if on_a_real_watcher(drive) {
        // A real watcher can drop a change in its arming window, so it's redone.
        until_the_watcher_delivers(&root.join("scope"), stem, patience, |name| {
            holds(drive, &format!("scope/{name}"))
        });
        return;
    }
    let name = format!("{stem}.txt");
    create_a_file(drive, &root.join("scope").join(&name));
    cmdr_fs::testing::wait_until(patience, "a change at the new mount point to land", || {
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
    let drive = an_indexed_drive("cover-move-inside-the-drain-test");
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
    assert_live_at(&drive, &new_root, "after-the-drain", LANDS_WITHIN);
    drop(put_back);
}

/// A rename landing while a scan start holds the manager out of the registry
/// takes effect at the handback: the manager it hands back is still at the old
/// root, so the handback drains it and the restart brings it up at the new one.
#[test]
fn a_rename_while_a_scan_starts_restarts_the_drive_at_the_handback() {
    let drive = an_indexed_drive("cover-move-while-detached-test");
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
    assert_live_at(&drive, &new_root, "after-the-handback", LANDS_WITHIN);
    assert!(state::is_active(drive.volume_id), "and the drive is indexing");
    drop(put_back);
}

/// The OS half of the headline case, on the REAL FSEvents journal: a stream
/// started at a renamed directory with the id stored at the old path replays what
/// changed under the new one. Everything above takes that on trust.
///
/// `#[ignore]`d: it waits on a real `fseventsd` replay, which took over 20 s about
/// one run in 20 on a loaded host (2026-10-07) and over 30 s in a cluster of runs
/// on 2026-10-19. It runs in the opt-in macOS disk-image lane
/// (`pnpm check disk-images`, or `--include-slow` on a Mac).
#[cfg(target_os = "macos")]
mod real_journal {
    use super::super::toggles::indexed;
    use super::*;

    #[test]
    #[ignore = "real fseventsd replay; runs in the opt-in macOS disk-image lane"]
    fn fsevents_replays_a_renamed_drives_journal_from_the_id_stored_at_the_old_path() {
        let drive = indexed(ColdDrive::watched_for_real("cover-move-real-journal-test"));
        the_drive_keeps_indexing_at_its_new_mount_point(&drive, std::time::Duration::from_secs(60));
    }
}
