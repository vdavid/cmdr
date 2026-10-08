//! What the drive menu's two actions do to a volume the machine is half way
//! through covering.
//!
//! The badge offers **Stop** and **Forget** while a drive is scanning, and they
//! sit either side of the one fact the launch table reads: the persisted branch
//! set. Stop has to leave it, so the next launch ADDS to what this session bought
//! (`LaunchRoute::CoverInPhases`); Forget has to take it with the database, so the
//! next launch starts clean. Get Stop wrong and a resume silently becomes a
//! rebuild — every folder covered so far walked again, and the user's own action
//! is what cost them.
//!
//! ⚠️ These drive the MENU's calls (`Index::disable_volume`, `Index::forget_volume`),
//! not the quit path (`stop_indexing`) the sibling routing tests use. They are
//! different entry points: disable also persists the user's intent, and it is the
//! one the badge invokes.

use super::*;

use crate::indexing::lifecycle::phases::walk_gate;
use crate::indexing::lifecycle::state;
use crate::indexing::watch::branches::COVERED_BRANCHES_KEY;

/// Stop, then relaunch. The ghost is what tells a resume from a rebuild: nothing
/// re-lists covered ground, so a row the last session wrote survives a resume and
/// cannot survive a rebuild.
#[test]
fn stopping_a_half_covered_drive_leaves_it_resumable() {
    let drive = Drive::new(
        "phased-menu-stop",
        |root| {
            std::fs::create_dir_all(root.join("covered/inner")).expect("dirs");
        },
        &[],
    );
    drive.start();
    drive.wait_for_the_machine();

    drive
        .index
        .disable_volume(drive.volume_id)
        .expect("the menu's Stop goes through");

    assert!(
        drive.meta(COVERED_BRANCHES_KEY).is_some(),
        "❌ Stop must leave the record of which ground is covered: it is the ONLY thing telling a phased \
         partial from an interrupted bulk scan, and without it the next launch rebuilds from scratch"
    );
    assert!(
        drive.db_path().exists(),
        "and the index itself stays on disk — Stop is not Forget"
    );

    drive.plant_a_ghost("covered/inner", "last-session.txt");
    drive.forget_the_completion_marker();
    drive.start();
    drive.wait_for_the_machine();

    assert!(
        drive.ghost_survived("covered/inner", "last-session.txt"),
        "❌ a drive the user stopped comes back to what it had covered; it does not start over"
    );
}

/// Forget is the other half of the pair, and its whole job is to reclaim the
/// disk. The branch set lives inside the database, so it goes with it, which is
/// what makes the next launch a clean first index rather than a resume into rows
/// nobody can account for.
#[test]
fn forgetting_a_half_covered_drive_takes_the_branch_set_with_the_database() {
    let drive = Drive::new(
        "phased-menu-forget",
        |root| {
            std::fs::create_dir_all(root.join("covered/inner")).expect("dirs");
        },
        &[],
    );
    drive.start();
    drive.wait_for_the_machine();
    assert!(drive.db_path().exists(), "the run built something to forget");

    drive
        .index
        .forget_volume(drive.volume_id)
        .expect("the menu's Forget goes through");

    assert!(
        !drive.db_path().exists(),
        "❌ Forget reclaims the disk; a database left behind is the whole complaint the action answers"
    );

    drive.start();
    drive.wait_for_the_machine();

    assert!(
        drive.frontier(&drive.path("")).is_empty(),
        "and the drive indexes again from nothing, converging the same way a fresh install does"
    );
    assert!(
        drive.meta("scan_completed_at").is_some(),
        "all the way to its completion marker"
    );
}

/// Stop cancels the queue as well as the walk. The call blocks on the manager's
/// shutdown, so by the time it returns the volume must report no work left —
/// otherwise the badge sits on "scanning" for a run nobody is doing, and the next
/// scan entry refuses against a machine that has stopped.
///
/// It stops the drive with phases still queued, which is the interesting moment:
/// the branch set is written as each walk FINISHES, so a stop taken here has one
/// recorded and more to come. (Stopping earlier still, inside the very first
/// walk, leaves the stitch's rows with no branch recorded, and the next launch
/// rebuilds them — deliberate, and the same rule that throws away an interrupted
/// bulk scan: nothing can say what ground those rows cover.)
#[test]
fn stopping_a_drive_ends_the_run_before_the_call_returns() {
    // Wide enough that the machine has phases queued behind the one it is on, so
    // "the queue is cleared" is a claim with something to clear.
    let drive = Drive::new(
        "phased-menu-stop-queue",
        |root| {
            for i in 0..40 {
                std::fs::create_dir_all(root.join(format!("branch-{i}/inner/deeper"))).expect("dirs");
            }
        },
        &["branch-0", "branch-1", "branch-2"],
    );
    drive.start();
    cmdr_fs::testing::wait_until(
        std::time::Duration::from_secs(30),
        "the first covered branch to be written down",
        || drive.meta(COVERED_BRANCHES_KEY).is_some(),
    );

    drive
        .index
        .disable_volume(drive.volume_id)
        .expect("the menu's Stop goes through");

    let status = drive.index.status(drive.volume_id);
    assert!(
        !status.is_ok_and(|status| status.scanning),
        "❌ Stop returning while the volume still reports work leaves a badge spinning for a run that is over"
    );
    assert!(
        drive.meta(COVERED_BRANCHES_KEY).is_some(),
        "and the branch set survives the stop, so the next launch adds to this run instead of replacing it"
    );
}

/// The machine decides to walk, then a Stop lands before the walk has asked for
/// its write context. The walk must find the drive stopped, ❌ never stand a fresh
/// index up for it (issue #375).
///
/// That second index is what a SEARCH gets when it walks an unindexed drive, and
/// it is carved out of both indexing switches for exactly that reason. A walk the
/// machine owes nobody isn't a search: taking that door brought the drive the
/// user had just stopped back up behind their back, as a registered index, for
/// the rest of the session.
#[test]
fn a_walk_the_machine_was_about_to_take_cant_bring_a_stopped_drive_back() {
    let drive = Drive::new(
        "phased-menu-stop-late-walk",
        |root| {
            std::fs::create_dir_all(root.join("branch/inner")).expect("dirs");
        },
        &[],
    );
    let gate = walk_gate::arm(drive.volume_id);
    drive.start();
    gate.wait_until_parked();

    drive
        .index
        .disable_volume(drive.volume_id)
        .expect("the menu's Stop goes through");
    gate.release();
    gate.wait_until_resolved();

    assert!(
        !state::is_active(drive.volume_id),
        "❌ the late walk stood a new index up for a drive the user just stopped"
    );
    assert!(
        !drive.index.status(drive.volume_id).is_ok_and(|status| status.scanning),
        "and nothing reads as scanning once Stop has returned"
    );
    assert!(
        IndexStore::user_disabled(&drive.db_path()),
        "and the user's veto is still the last word on the drive"
    );
}

/// The same walk, landing INSIDE the stop's drain instead of after it. A start
/// that meets a draining volume is recorded and carried out once the drain ends,
/// skipping the veto on its way, so a late walk asking for a context there was
/// the drive coming back AND forgetting it had been turned off.
#[test]
fn a_walk_that_lands_in_the_stops_drain_cant_bring_the_drive_back_either() {
    let drive = Drive::new(
        "phased-menu-stop-walk-in-drain",
        |root| {
            std::fs::create_dir_all(root.join("branch/inner")).expect("dirs");
        },
        &[],
    );
    let gate = walk_gate::arm(drive.volume_id);
    drive.start();
    gate.wait_until_parked();

    state::while_stopping_for_test(drive.volume_id, || {
        gate.release();
        gate.wait_until_resolved();
    });

    assert!(
        !state::is_active(drive.volume_id),
        "❌ the walk's start rode the drain and brought the drive back"
    );
    assert!(
        IndexStore::user_disabled(&drive.db_path()),
        "and the stop's veto landed rather than being skipped for that start"
    );
}
