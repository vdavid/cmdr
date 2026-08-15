//! Per-drive intent is written when the user ASKS, and a walk is not an ask.
//! What each door records, and what forgetting a drive takes with it.

use super::*;

/// A drive a search walked carries NO enable, so nothing resumes it later.
///
/// This is what makes the marker worth trusting. The walk stands up a database, a
/// writer, and covered rows on a drive the user never opted into, so if standing
/// any of that up counted as an enable, every searched drive would start indexing
/// itself at the next launch or master-switch toggle — which is exactly what the
/// per-drive opt-in exists to prevent. ❌ Don't record intent anywhere a walk
/// reaches.
#[test]
fn a_searched_drive_is_never_recorded_as_one_the_user_turned_on() {
    let drive = ColdDrive::new("cover-no-enable-from-a-walk-test");
    std::fs::create_dir_all(drive.tree.path().join("scope")).expect("dirs");
    std::fs::write(drive.tree.path().join("scope/found.txt"), "x").expect("file");

    drive.cover(&drive.path("scope"));

    assert!(
        !IndexStore::user_enabled(&drive.db_path()),
        "a search is a read, not a request to index the drive",
    );
    assert!(
        !crate::indexing::lifecycle::master::drive_index_should_run(true, &drive.db_path(), false),
        "so nothing resumes this drive on its own",
    );
}

/// Turning indexing on records the choice straight away, before the first index
/// has walked anything.
///
/// The durability that matters: a first index runs for minutes on a real drive,
/// and everything that can interrupt it (quit, unplug, a NAS drop, the master
/// switch) lands inside that window. The choice has to be on disk from the first
/// moment, so what comes back afterwards is the drive the user asked for.
#[tokio::test(flavor = "multi_thread")]
#[allow(
    clippy::await_holding_lock,
    reason = "the fixture holds the process-wide seams for the whole test; holding it across the await IS the point"
)]
async fn turning_indexing_on_records_the_choice_before_any_scan_finishes() {
    let drive = ColdDrive::new("cover-enable-records-intent-test");
    std::fs::create_dir_all(drive.tree.path().join("scope")).expect("dirs");

    drive
        .index
        .start_volume(drive.volume_id)
        .await
        .expect("the drive starts indexing");

    assert!(
        IndexStore::user_enabled(&drive.db_path()),
        "the enable is on the drive's own database from the moment it's asked for",
    );
    assert!(
        crate::indexing::lifecycle::master::drive_index_should_run(true, &drive.db_path(), false),
        "so a master-switch cycle or a reconnect brings this drive back",
    );

    // And turning it off again withdraws it, whatever the scan got to.
    drive.index.disable_volume(drive.volume_id).expect("the drive stops");
    assert!(
        !IndexStore::user_enabled(&drive.db_path()),
        "a disable withdraws the enable rather than leaving both markers on the DB",
    );
    assert!(
        !crate::indexing::lifecycle::master::drive_index_should_run(true, &drive.db_path(), false),
        "and the drive stays off",
    );
}

/// Forgetting a drive takes its intent with it: the marker lives in the database
/// forget deletes, so there's nothing left to resume from.
///
/// Worth pinning even though it's free. "Forget this drive" is the action that
/// reclaims the disk, and a marker that outlived the database it described would
/// bring the drive back at the next launch as a fresh full index.
#[tokio::test(flavor = "multi_thread")]
#[allow(
    clippy::await_holding_lock,
    reason = "the fixture holds the process-wide seams for the whole test; holding it across the await IS the point"
)]
async fn forgetting_a_drive_takes_the_users_choice_with_it() {
    let drive = ColdDrive::new("cover-forget-drops-intent-test");
    std::fs::create_dir_all(drive.tree.path().join("scope")).expect("dirs");

    drive
        .index
        .start_volume(drive.volume_id)
        .await
        .expect("the drive starts indexing");
    assert!(IndexStore::user_enabled(&drive.db_path()), "precondition: enabled");

    drive
        .index
        .forget_volume(drive.volume_id)
        .expect("the drive is forgotten");

    assert!(!drive.db_path().exists(), "forget reclaims the database");
    assert!(
        !crate::indexing::lifecycle::master::drive_index_should_run(true, &drive.db_path(), false),
        "so nothing brings the drive back on its own",
    );
}

/// Enabling a drive a search already walked records the choice too.
///
/// That call takes its own branch — the volume is already active, so it routes
/// straight to the scan the walk never ran — and it's the one branch that could
/// silently skip the record while looking like it worked.
#[tokio::test(flavor = "multi_thread")]
#[allow(
    clippy::await_holding_lock,
    reason = "the fixture holds the process-wide seams for the whole test; holding it across the await IS the point"
)]
async fn enabling_a_drive_a_search_walked_records_the_choice_too() {
    let drive = ColdDrive::new("cover-enable-after-walk-intent-test");
    std::fs::create_dir_all(drive.tree.path().join("walked")).expect("dirs");

    drive.cover(&drive.path("walked"));
    assert!(
        !IndexStore::user_enabled(&drive.db_path()),
        "precondition: the walk alone records nothing",
    );

    drive
        .index
        .start_volume(drive.volume_id)
        .await
        .expect("the drive starts indexing");

    assert!(
        IndexStore::user_enabled(&drive.db_path()),
        "the enable is recorded even though the volume was already active",
    );
}
