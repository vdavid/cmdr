//! The delete gates: a reconcile may only reap the rows its listing didn't mention
//! when the observation behind it was WHOLE — the listing saw everything, and the
//! drive was still listed after the read.
//!
//! The control test is load-bearing. A gate that refused every delete would pass the
//! other tests here and quietly stop the index ever converging, so each "reaps
//! nothing" case is paired with a "still reaps" one on a healthy drive.

use super::*;
use crate::indexing::hold::VolumeWork;
use crate::indexing::host::volumes::{FakeVolumeProvider, MountIdentity, VolumeProvider, install_for_test};
use crate::indexing::metadata::MetadataSnapshot;
use std::sync::Arc;

/// The filesystem the drive in these tests is.
const DRIVE: MountIdentity = MountIdentity::from_raw(0x0DE1_E7E5);

/// A tree with two files, indexed, and work whose generation captured the drive's
/// identity — the shape a real `LocalExternal` start leaves behind.
///
/// Returns the provider (so a test can pull the drive), the work, and the installed
/// guard, which must stay alive for the test's duration.
struct Drive {
    provider: Arc<FakeVolumeProvider>,
    work: VolumeWork,
    _installed: crate::indexing::host::volumes::TestProviderGuard,
}

/// Where the drive sits in the fake mount table. The gates ask whether its IDENTITY
/// is still mounted, never where, so the table lists it where a real drive lives:
/// listed at the temp tree's own path, it would be a filesystem mounted inside the
/// boot tree, which the boot-space walks these tests run stop at
/// (`scanner::boot_tree_mounts`).
const DRIVE_MOUNT_POINT: &str = "/Volumes/cmdr-gate-test-drive";

fn mount_a_drive(volume_id: &str) -> Drive {
    let provider = FakeVolumeProvider::shared();
    provider.mount(DRIVE_MOUNT_POINT, DRIVE);
    let installed = install_for_test(Arc::clone(&provider) as Arc<dyn VolumeProvider>);
    Drive {
        provider,
        work: VolumeWork::for_test_on(volume_id, DRIVE),
        _installed: installed,
    }
}

/// The names the index holds under `dir`.
fn indexed_children(conn: &Connection, dir: &str) -> Vec<String> {
    let id = store::resolve_path(conn, dir)
        .expect("resolve the directory")
        .expect("the directory is indexed");
    let mut names: Vec<String> = IndexStore::list_children_on(id, conn)
        .expect("list the children")
        .into_iter()
        .map(|row| row.name)
        .collect();
    names.sort();
    names
}

/// Index `root`'s two files, then delete one on disk and reconcile again, reporting
/// what the index holds afterwards. `pull_the_drive` runs between the two passes.
fn reconcile_after_a_removal(volume_id: &str, pull_the_drive: bool) -> Vec<String> {
    let _serialized = crate::indexing::handle::test_lock();
    let dir = non_excluded_tempdir();
    let root = dir.path();
    std::fs::write(root.join("keep.txt"), b"keep").expect("write keep.txt");
    std::fs::write(root.join("gone.txt"), b"gone").expect("write gone.txt");

    let (writer, _db_dir, conn) = setup_test_writer();
    let root_str = root.to_string_lossy().to_string();
    ensure_path_in_db(&writer.db_path(), &root_str, &writer);
    let drive = mount_a_drive(volume_id);
    let space = IndexPathSpace::root();

    // First pass: a healthy drive fills the index.
    reconcile_subtree(root, &space, &conn, &writer, &drive.work, None).expect("the first reconcile walks");
    writer.flush_blocking().expect("flush the first pass");
    assert_eq!(
        indexed_children(&conn, &root_str),
        vec!["gone.txt".to_string(), "keep.txt".to_string()],
        "precondition: both files are indexed"
    );

    std::fs::remove_file(root.join("gone.txt")).expect("remove gone.txt");
    if pull_the_drive {
        // ⚠️ AFTER the removal and before the second pass: the directory still lists
        // fine (its mount-point folder is a real temp dir), so only the presence read
        // can tell that the drive is gone.
        drive.provider.mark_unmounted(DRIVE_MOUNT_POINT);
    }

    reconcile_subtree(root, &space, &conn, &writer, &drive.work, None).expect("the second reconcile walks");
    writer.flush_blocking().expect("flush the second pass");
    indexed_children(&conn, &root_str)
}

/// The control: on a drive that is still there, a file that really was removed is
/// still reaped. Without this the gate could refuse every delete and the rest of
/// this file would still pass.
#[test]
fn a_reconcile_on_a_listed_drive_still_reaps_a_removed_row() {
    assert_eq!(
        reconcile_after_a_removal("gate-test-listed", false),
        vec!["keep.txt".to_string()],
        "a real removal on a healthy drive is still a delete"
    );
}

/// The case the gate exists for: the drive left between the two passes, so the
/// second listing is not evidence of anything and no row may be reaped from it.
#[test]
fn a_reconcile_whose_drive_left_reaps_nothing() {
    let mut names = reconcile_after_a_removal("gate-test-left", true);
    names.sort();
    assert_eq!(
        names,
        vec!["gone.txt".to_string(), "keep.txt".to_string()],
        "a drive that stopped being listed authorizes no deletes, so both rows survive"
    );
}

/// A listing that came back SHORT reaps nothing, and still writes everything it did
/// see: the upserts are driven by what was observed, only the reaping needs a whole
/// observation.
#[test]
fn an_incomplete_listing_reaps_nothing_and_still_upserts() {
    let (writer, db_dir, conn) = setup_test_writer();
    let db_path = db_dir.path().join("test-reconciler.db");
    let parent_id = IndexStore::insert_entry_v2(&conn, ROOT_ID, "short", true, false, None, None, None, None)
        .expect("insert the parent");
    let stale_id = IndexStore::insert_entry_v2(&conn, parent_id, "stale.txt", false, false, Some(1), None, None, None)
        .expect("insert the row the listing won't mention");
    let next_id = IndexStore::get_next_id(&conn).expect("next id");
    writer.next_id().fetch_max(next_id, Ordering::Relaxed);

    let seen = LiveChild {
        name: "fresh.txt".to_string(),
        is_directory: false,
        is_symlink: false,
        snap: MetadataSnapshot {
            logical_size: Some(7),
            physical_size: Some(7),
            modified_at: Some(1),
            inode: None,
            nlink: Some(1),
        },
    };
    let db_children = IndexStore::list_children_on(parent_id, &conn).expect("list the parent");

    // Bound first: `DirDiff` borrows the listing it was diffed against.
    let live = [seen];
    let diff = diff_dir_against_db(parent_id, &live, &db_children, MissingRows::Keep, &writer);
    writer.flush_blocking().expect("flush the diff");

    assert_eq!(diff.removed, 0, "a short listing reaps nothing");
    let conn = IndexStore::open_read_connection(&db_path).expect("read connection");
    let mut names: Vec<String> = IndexStore::list_children_on(parent_id, &conn)
        .expect("list the parent again")
        .into_iter()
        .map(|row| row.name)
        .collect();
    names.sort();
    assert_eq!(
        names,
        vec!["fresh.txt".to_string(), "stale.txt".to_string()],
        "the row it DID see is written, and the one it couldn't vouch for is kept"
    );
    assert!(
        IndexStore::get_entry_by_id(&conn, stale_id)
            .expect("read the stale row")
            .is_some(),
        "the unmentioned row is still there by id"
    );
}

/// Index one file under `root`, then send a removal event for it after deleting it
/// on disk. `pull_the_drive` runs between the two, before the event is processed.
/// Reports the names the index still holds.
fn live_removal_after_a_deletion(volume_id: &str, pull_the_drive: bool) -> Vec<String> {
    let _serialized = crate::indexing::handle::test_lock();
    let dir = non_excluded_tempdir();
    let root = dir.path();
    let doomed = root.join("doomed.txt");
    std::fs::write(&doomed, b"doomed").expect("write doomed.txt");

    let (writer, db_dir, conn) = setup_test_writer();
    let db_path = db_dir.path().join("test-reconciler.db");
    let root_str = root.to_string_lossy().to_string();
    ensure_path_in_db(&db_path, &root_str, &writer);
    {
        let wconn = IndexStore::open_write_connection(&db_path).expect("write connection");
        let parent_id = store::resolve_path(&wconn, &root_str)
            .expect("resolve")
            .expect("the root is indexed");
        IndexStore::insert_entry_v2(&wconn, parent_id, "doomed.txt", false, false, Some(6), None, None, None)
            .expect("index the file");
        let next_id = IndexStore::get_next_id(&wconn).expect("next id");
        writer.next_id().fetch_max(next_id, Ordering::Relaxed);
    }

    let drive = mount_a_drive(volume_id);
    let mut reconciler = EventReconciler::new_for(
        volume_id.to_string(),
        IndexPathSpace::root(),
        drive.work.child(crate::indexing::hold::HoldKind::LiveLoop),
    );
    reconciler.switch_to_live();

    // The file really is gone, so the removal is genuine as far as the event path can
    // tell — only the presence read can say whether the drive was there to see it.
    std::fs::remove_file(&doomed).expect("remove doomed.txt");
    if pull_the_drive {
        drive.provider.mark_unmounted(DRIVE_MOUNT_POINT);
    }

    let event = make_event(&doomed.to_string_lossy(), 7, removed_file_flags());
    let mut origins = HashSet::new();
    reconciler.process_live_event(&event, &conn, &writer, &mut origins);
    reconciler.flush_deletes(&writer);
    writer.flush_blocking().expect("flush the batch");

    let names = indexed_children(&conn, &root_str);
    writer.shutdown();
    names
}

/// The control: on a drive that is still listed, a real removal still deletes. A gate
/// that withheld every delete would satisfy the test below and stop the live loop
/// ever converging.
#[test]
fn a_removal_event_on_a_listed_drive_still_deletes() {
    assert!(
        live_removal_after_a_deletion("gate-test-live-listed", false).is_empty(),
        "a real removal on a healthy drive is still a delete"
    );
}

/// The case the batch gate exists for: the drive left before the event was processed,
/// so the stat that would "prove" the removal failed for a reason that says nothing
/// about the file.
#[test]
fn a_removal_event_whose_drive_left_deletes_nothing() {
    assert_eq!(
        live_removal_after_a_deletion("gate-test-live-left", true),
        vec!["doomed.txt".to_string()],
        "a drive that stopped being listed authorizes no deletes, so the row survives"
    );
}

/// What makes a listing whole, decided on the ERRNO and ❌ never on a message.
///
/// A child that vanished between `readdir` and `stat` is genuinely absent, so the
/// listing still saw everything there is to see and may be diffed for deletes.
/// Every other failure means we never got to look at that child, which must not read
/// as "it's gone".
#[test]
fn only_a_gone_child_keeps_a_listing_whole() {
    use std::io::{Error, ErrorKind};

    assert!(
        child_is_absent(&Error::from(ErrorKind::NotFound)),
        "removed between readdir and stat: really absent"
    );
    assert!(
        child_is_absent(&Error::from(ErrorKind::NotADirectory)),
        "a path component stopped being a directory: the child isn't there either"
    );
    for unobserved in [
        ErrorKind::PermissionDenied,
        ErrorKind::TimedOut,
        ErrorKind::Interrupted,
        ErrorKind::Other,
    ] {
        assert!(
            !child_is_absent(&Error::from(unobserved)),
            "{unobserved:?} means we never looked, so the listing is short rather than complete"
        );
    }
}
