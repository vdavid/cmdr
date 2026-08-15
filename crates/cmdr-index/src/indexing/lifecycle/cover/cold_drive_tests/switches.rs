//! The two drive-indexing switches govern background work only, so a search
//! walks a drive with either of them off. Both keep their teeth: nothing
//! schedules a scan and nothing starts a watcher.

use super::*;

/// ⚠️ A search walks a drive with drive indexing turned OFF, and that is
/// DELIBERATE (Decision 13). ❌ Don't "fix" it back into a refusal.
///
/// The master switch means "don't index anything on your own": no launch
/// auto-start, no per-drive enable, no reconnect resume. Searching is none of
/// those — it's a read the person in front of the app just asked for, and a walk
/// is what reading a folder Cmdr hasn't indexed IS. Refusing here wouldn't save
/// the user any work; it would only make the search return a wrong answer
/// silently, which is the exact bug this whole effort removes.
///
/// The switch keeps its teeth: nothing schedules a scan, nothing starts a
/// watcher, and the walk covers only the folder it was pointed at.
#[test]
fn a_search_walks_a_drive_with_the_master_switch_off() {
    let drive = ColdDrive::with_indexing_disabled("cover-master-switch-off-test");
    std::fs::create_dir_all(drive.tree.path().join("scope/inner")).expect("dirs");
    std::fs::write(drive.tree.path().join("scope/inner/found.txt"), "x").expect("file");
    std::fs::create_dir_all(drive.tree.path().join("elsewhere")).expect("dirs");
    let scope = drive.path("scope");

    assert!(
        !crate::indexing::lifecycle::master::master_enabled(),
        "precondition: drive indexing is off in settings"
    );

    let outcome = drive.cover(&scope);
    assert!(!outcome.cancelled);
    assert_eq!(outcome.roots_covered, 1, "the search got the walk it asked for");
    assert_eq!(outcome.entries_found, 3, "scope/ itself, inner/, and inner/found.txt");

    assert_eq!(
        drive.scans_started(),
        0,
        "and the switch keeps its teeth: nothing indexed the drive uninvited"
    );
    assert!(
        !drive.coverage(&drive.path("")).frontier.is_empty(),
        "only the searched folder was walked, not the drive"
    );
}

/// The sticky per-drive veto reads the same way: it stops background indexing of
/// that drive, not a read someone asked for.
///
/// Its teeth are elsewhere — a vetoed drive gets no watcher (M11), so its walked
/// branches go stale and re-walk instead of staying live. Turning a search on
/// that drive into a wrong answer was never the point.
#[test]
fn a_search_walks_a_drive_the_user_turned_indexing_off_for() {
    let drive = ColdDrive::new("cover-user-disabled-test");
    std::fs::create_dir_all(drive.tree.path().join("scope")).expect("dirs");
    std::fs::write(drive.tree.path().join("scope/found.txt"), "x").expect("file");
    drop(IndexStore::open(&drive.db_path()).expect("open store"));
    IndexStore::set_drive_index_intent(&drive.db_path(), false).expect("record the disable");

    let outcome = drive.cover(&drive.path("scope"));
    assert_eq!(outcome.roots_covered, 1, "the search still got its answer");
    assert_eq!(outcome.entries_found, 2, "scope/ itself and found.txt");
    assert_eq!(drive.scans_started(), 0, "with no scan of the drive behind it");
}
