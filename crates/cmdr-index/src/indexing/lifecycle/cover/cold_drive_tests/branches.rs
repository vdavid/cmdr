//! What a walk leaves behind for the live loop: the branch it registers, the
//! events that reach the index through it, and every path that releases or
//! retires one.

use super::*;

/// The walk's other half of Decision 9: it writes down the ground it covered, on
/// the volume's own database, so the branch survives the app.
///
/// Without this the plan needs the expiry it replaced — a walked branch nothing
/// watches is a snapshot of a folder taken once, and the next session would have
/// to either re-walk it or serve rows it can't vouch for.
#[test]
fn a_walk_leaves_the_ground_it_covered_watched_and_written_down() {
    let drive = ColdDrive::new("cover-branch-watch-test");
    std::fs::create_dir_all(drive.tree.path().join("scope/inner")).expect("dirs");
    std::fs::write(drive.tree.path().join("scope/inner/found.txt"), "x").expect("file");
    let scope = drive.path("scope");

    drive.cover(&scope);

    assert_eq!(
        crate::indexing::watch::branches::live_for(drive.volume_id).branch_paths(),
        [scope.as_str()],
        "the walked branch is the volume's to keep current now"
    );
    // `persist` hands the meta row to the async `IndexWriter` and `cover` only waits for
    // the COVERAGE to read as walked, so reading the database the instant the walk returns
    // races the writer thread. Wait for the row, then assert its exact contents.
    cmdr_fs::testing::wait_until(
        std::time::Duration::from_secs(10),
        "the walk's branch to reach the database",
        || drive.persisted_branches().is_some(),
    );
    assert_eq!(
        drive.persisted_branches(),
        Some("/scope".to_string()),
        "and it's on the drive's own database, index-relative so a remount still finds it"
    );
}

/// The whole promise, through the real watcher: change a file inside walked
/// ground and the index follows; change one beside it and the index doesn't move.
///
/// Every other test here drives the admission rule directly. This one starts a
/// real `DriveWatcher` on a real drive and touches real files, which is the only
/// way to prove `ensure_branch_watch` starts something that works — the failure it
/// catches is a walked branch that reads live and is quietly frozen.
#[cfg(any(target_os = "macos", target_os = "linux"))]
#[tokio::test(flavor = "multi_thread")]
#[allow(
    clippy::await_holding_lock,
    reason = "the fixture holds the process-wide seams for the whole test; holding it across the await IS the point"
)]
async fn a_change_inside_a_walked_branch_reaches_the_index_and_one_beside_it_does_not() {
    let drive = ColdDrive::new("cover-branch-live-test");
    std::fs::create_dir_all(drive.tree.path().join("walked")).expect("dirs");
    std::fs::create_dir_all(drive.tree.path().join("beside")).expect("dirs");
    std::fs::write(drive.tree.path().join("walked/already-there.txt"), "x").expect("file");
    let branch = drive.path("walked");

    drive.cover(&branch);
    assert!(
        !crate::indexing::watch::branches::live_for(drive.volume_id)
            .branch_paths()
            .is_empty(),
        "precondition: the walk left a branch to watch"
    );

    // Both drives created AFTER the walk, so neither is in the index yet. The
    // watcher is the only thing that can put either one there.
    std::fs::write(drive.tree.path().join("walked/appeared.txt"), "new").expect("file");
    std::fs::write(drive.tree.path().join("beside/appeared.txt"), "new").expect("file");

    let inside = drive.path("walked/appeared.txt");
    let outside = drive.path("beside/appeared.txt");
    cmdr_fs::testing::wait_until_async(
        std::time::Duration::from_secs(30),
        "the watcher to index the change inside the walked branch",
        || drive.is_indexed(&inside),
    )
    .await;
    assert!(
        !drive.is_indexed(&outside),
        "and the folder beside it stays this index's business only once a search walks it"
    );
}

/// A walk registers its ground when it starts and releases it when it ends, and
/// the release can't depend on what the registry is doing minutes later.
///
/// `force_scan` and `perform_registry_rescan` detach the manager for the whole of
/// a scan start, so a walk ending inside that window used to leave its branch
/// at `walks > 0` forever: `may_walk` false for that ground permanently, every
/// event for it buffered and never promoted, and the branch never absorbed.
#[test]
fn a_walk_that_finishes_while_the_manager_is_detached_still_releases_its_branch() {
    use crate::indexing::watch::branches::{self, WatchScope};
    use crate::indexing::watch::watcher::{FsChangeEvent, FsEventFlags};

    let drive = ColdDrive::new("cover-branch-shutdown-finish-test");
    std::fs::create_dir_all(drive.tree.path().join("scope")).expect("dirs");
    let scope = drive.path("scope");
    let branch = vec![scope.clone()];
    crate::indexing::lifecycle::state::start_indexing_for(
        drive.volume_id,
        drive.tree.path().to_path_buf(),
        IndexVolumeKind::LocalExternal,
        true,
        crate::indexing::lifecycle::state::Activation::WriterOnly,
    )
    .expect("stand the index up");

    crate::indexing::lifecycle::state::begin_branch_coverage(drive.volume_id, &branch);
    let watch = branches::live_for(drive.volume_id);
    assert!(
        watch.is_being_walked(Path::new(&scope)),
        "precondition: the ground is registered and its events wait for the walk"
    );
    let held = FsChangeEvent {
        path: drive.path("scope/arrived-mid-walk.txt"),
        event_id: 1,
        flags: FsEventFlags {
            item_created: true,
            item_is_file: true,
            ..Default::default()
        },
    };
    let _ = WatchScope::Branches(Arc::clone(&watch)).admit(held);

    crate::indexing::lifecycle::state::while_detached_for_test(drive.volume_id, || {
        crate::indexing::lifecycle::state::finish_branch_coverage(drive.volume_id, &branch);
    });

    assert!(
        !watch.is_being_walked(Path::new(&scope)),
        "the hold goes with the walk, whatever phase the registry was in"
    );
    assert_eq!(
        watch.take_promoted().events.len(),
        1,
        "and what it held is released, rather than buffering for the rest of the session"
    );
}

/// Clearing a drive's index takes its branches with it, because they describe
/// coverage that no longer exists.
///
/// No code does this: the set lives on the database the clear deletes, which is
/// why it lives there. The test is here so a later change that moves it somewhere
/// else has to notice.
#[test]
fn clearing_a_drives_index_drops_the_branches_with_the_coverage() {
    let drive = ColdDrive::new("cover-branch-clear-test");
    std::fs::create_dir_all(drive.tree.path().join("scope")).expect("dirs");
    std::fs::write(drive.tree.path().join("scope/found.txt"), "x").expect("file");

    drive.cover(&drive.path("scope"));
    // `BranchWatch::persist` hands the meta row to the async `IndexWriter`, and
    // `cover` only waits for the COVERAGE to read as walked. Wait for the row itself:
    // reading the database the instant the walk returns races the writer thread, which
    // is a race only a slow host loses (it did, on the Docker Linux lane).
    cmdr_fs::testing::wait_until(
        std::time::Duration::from_secs(10),
        "the walk's branch to reach the database",
        || drive.persisted_branches().is_some(),
    );

    drive.index.forget_volume(drive.volume_id).expect("clear the index");

    assert_eq!(
        drive.persisted_branches(),
        None,
        "the database went, and the set with it"
    );
    assert!(
        crate::indexing::watch::branches::live_for(drive.volume_id)
            .branch_paths()
            .is_empty(),
        "and nothing in memory still claims to watch it"
    );
}

/// The per-drive veto's teeth, and the whole of them: a drive the user turned
/// indexing off for is still walked for a search (Decision 13), and is left
/// unwatched afterwards.
///
/// So its walked ground stays covered and served, and stops being kept current
/// the moment the app stops. ❌ Not "re-walked" — the walk marked those
/// directories listed, so the frontier never offers them again; they are
/// covered-but-stale, which Decision 5 trusts.
///
/// The veto takes the WATCHER, not the record. What a walk covered is a fact
/// about the index, and dropping it would leave the next session unable to tell
/// walked ground from ground nobody ever asked about.
#[test]
fn a_vetoed_drive_is_walked_and_left_unwatched() {
    let drive = ColdDrive::new("cover-branch-veto-test");
    std::fs::create_dir_all(drive.tree.path().join("scope")).expect("dirs");
    std::fs::write(drive.tree.path().join("scope/found.txt"), "x").expect("file");
    drop(IndexStore::open(&drive.db_path()).expect("open store"));
    IndexStore::set_drive_index_intent(&drive.db_path(), false).expect("record the disable");
    let scope = drive.path("scope");

    let outcome = drive.cover(&scope);
    assert_eq!(outcome.roots_covered, 1, "the search still got its answer");

    assert!(
        !crate::indexing::lifecycle::state::is_watching_for_test(drive.volume_id),
        "and nothing is watching it: the veto stops everything that runs uninvited"
    );
    assert_eq!(
        crate::indexing::watch::branches::live_for(drive.volume_id).branch_paths(),
        [scope],
        "while what the walk covered is still written down"
    );
    cmdr_fs::testing::wait_until(
        std::time::Duration::from_secs(10),
        "the walk's branch to reach the database",
        || drive.persisted_branches().is_some(),
    );
    assert_eq!(
        drive.persisted_branches(),
        Some("/scope".to_string()),
        "on the drive's own database, so a later session knows this ground was walked"
    );
}

/// A branch survives the volume going away and coming back, which is what
/// "persisted" has to mean.
///
/// Nothing at LAUNCH resumes it, deliberately: an unregistered volume answers
/// neither sizes nor coverage questions, so the first moment that coverage can be
/// read is the moment its index comes up — and that's the moment the watch
/// returns.
#[test]
fn a_branch_comes_back_when_the_volume_does() {
    let drive = ColdDrive::new("cover-branch-resume-test");
    std::fs::create_dir_all(drive.tree.path().join("scope")).expect("dirs");
    std::fs::write(drive.tree.path().join("scope/found.txt"), "x").expect("file");
    let scope = drive.path("scope");

    drive.cover(&scope);
    crate::indexing::lifecycle::state::stop_indexing(drive.volume_id).expect("stop the volume");
    assert!(
        crate::indexing::watch::branches::live_for(drive.volume_id)
            .branch_paths()
            .is_empty(),
        "precondition: the set goes with the instance that watched it"
    );

    crate::indexing::lifecycle::state::start_indexing_for(
        drive.volume_id,
        drive.tree.path().to_path_buf(),
        IndexVolumeKind::LocalExternal,
        true,
        crate::indexing::lifecycle::state::Activation::WriterOnly,
    )
    .expect("stand the index back up");

    assert_eq!(
        crate::indexing::watch::branches::live_for(drive.volume_id).branch_paths(),
        [scope],
        "and the volume comes back watching what it walked last time"
    );
}
