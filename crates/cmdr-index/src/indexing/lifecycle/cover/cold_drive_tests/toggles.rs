//! Turning a drive's indexing off and straight back on, faster than a teardown
//! can finish. Whatever order the toggles arrive in, the drive ends up in the
//! state the user last asked for.

use super::*;
use crate::indexing::host::runtime;
use crate::indexing::lifecycle::state;

/// Start this drive's indexing the way the per-drive switch does.
fn turn_on(drive: &ColdDrive) {
    runtime::block_on(drive.index.start_volume(drive.volume_id)).expect("turning drive indexing on is not an error");
}

/// Turn it off the way the per-drive switch does, sticky veto and all.
fn turn_off(drive: &ColdDrive) {
    drive.index.disable_volume(drive.volume_id).expect("the drive stops");
}

/// The one file every test here looks for, so "did this drive actually index"
/// is one question with one answer.
const PROOF: &str = "scope/found.txt";

/// A drive with one file on it, already indexed and SETTLED.
///
/// ⚠️ The settle is load-bearing, not tidiness: the first index announces itself
/// asynchronously, so a `scans_started()` baseline read before it lands counts
/// that walk against whatever the test does next.
fn an_indexed_drive(volume_id: &'static str) -> ColdDrive {
    let drive = ColdDrive::new(volume_id);
    std::fs::create_dir_all(drive.tree.path().join("scope")).expect("dirs");
    std::fs::write(drive.tree.path().join(PROOF), "x").expect("file");
    turn_on(&drive);
    wait_until_indexed(&drive, "the first index to land");
    drive
}

/// Wait for this drive's index to hold the proof file. ❗ What "the drive is
/// indexing" has to mean in every assertion here: a call that returned `Ok` proves
/// nothing about whether any row was ever written.
fn wait_until_indexed(drive: &ColdDrive, what: &str) {
    cmdr_fs::testing::wait_until(std::time::Duration::from_secs(20), what, || {
        drive.is_indexed(&drive.path(PROOF))
    });
}

/// **The headline case.** Turning a drive's indexing off and back on inside the
/// teardown's drain window leaves it INDEXING, because that is what the user last
/// asked for.
///
/// The drain blocks for up to five seconds (`mgr.shutdown()` waits on the
/// live-event task), and the instance stays registered as `ShuttingDown` for all
/// of it. A start that bounced off that phase was reported as done and then lost:
/// indexing never came back for the rest of the session, and the disable's veto
/// landed on the far side of the drain and outlived the launch too. On CI it
/// wedged every later index-dependent spec.
#[test]
fn turning_indexing_off_then_on_inside_the_drain_window_leaves_the_drive_indexing() {
    let drive = an_indexed_drive("cover-toggle-inside-the-drain-test");
    assert!(
        state::is_active(drive.volume_id),
        "precondition: the drive is indexing before we toggle it"
    );

    // The stop opens the real window; the start lands inside it, exactly as a
    // second click does.
    state::while_stopping_for_test(drive.volume_id, || turn_on(&drive));

    assert!(
        state::is_active(drive.volume_id),
        "the last thing the user asked for was ON, so the drive has to be indexing"
    );
    // ❗ Not merely "the call returned Ok": the volume has to be doing the work.
    cmdr_fs::testing::wait_until(
        std::time::Duration::from_secs(20),
        "the restarted index to cover the drive",
        || drive.is_indexed(&drive.path("scope/found.txt")),
    );
    assert!(
        !IndexStore::user_disabled(&drive.db_path()),
        "and the disable's veto must not outlive the start that superseded it, or the drive \
         comes back off at the next launch"
    );
}

/// Toggling as fast as a person can click settles on the LAST request, and pays
/// for one teardown and one index rather than one of each per click.
///
/// Every toggle after the first lands in the same drain window, so they collapse
/// into the single `Option` that window carries: each start overwrites it, each
/// stop clears it, and the far side of the drain acts on whatever is left.
#[test]
fn rapid_alternating_toggles_settle_on_the_last_one() {
    let drive = an_indexed_drive("cover-toggle-alternating-test");
    let scans_before = drive.scans_started();

    // on / off / on / off / on, all inside the one window the first off opened.
    state::while_stopping_for_test(drive.volume_id, || {
        turn_on(&drive);
        turn_off(&drive);
        turn_on(&drive);
        turn_off(&drive);
        turn_on(&drive);
    });

    assert!(state::is_active(drive.volume_id), "the last request was ON");
    assert!(
        !IndexStore::user_disabled(&drive.db_path()),
        "and the drive's own database agrees, so it comes back at the next launch"
    );
    assert!(
        drive.scans_started() - scans_before <= 1,
        "five toggles must not queue five walks of the drive"
    );
}

/// Two teardowns and a start landing in one window resolve coherently: the
/// teardown that takes the most away wins, and the start runs on the far side of
/// it.
///
/// The window here is the millisecond a scan start holds the manager out of the
/// registry (`IndexPhase::Detached`). A stop and a clear both land in it; the
/// clear outranks (`TeardownClaim::reach`), so the database goes, and the start
/// that came after them rebuilds it rather than being dropped on the floor.
#[test]
fn two_teardowns_and_a_start_in_one_window_end_with_a_rebuilt_index() {
    let drive = an_indexed_drive("cover-toggle-two-teardowns-test");

    state::while_detached_for_test(drive.volume_id, || {
        turn_off(&drive);
        drive
            .index
            .forget_volume(drive.volume_id)
            .expect("the drive is forgotten");
        turn_on(&drive);
    });

    assert!(
        state::is_active(drive.volume_id),
        "the start that came last is the one that decides where the volume ends up"
    );
    wait_until_indexed(&drive, "the rebuilt index to cover the drive again");
}

/// A start that meets a drive whose index DIED rebuilds it, instead of bouncing
/// off the `Failed` instance still holding the volume's key.
///
/// A `Failed` volume stays registered so the badge can say "indexing stopped", and
/// the recovery for it is a rebuild — the database is dead, so there is nothing to
/// resume. Answering a start with silence there leaves the red badge sitting until
/// somebody relaunches the app. This drives the CHOKE POINT, which is where the
/// automatic starts arrive (a launch, a share reconnect, a start recorded while
/// the volume was still dying); the user's own flip clears it one call earlier, in
/// `Index::start_volume`.
#[test]
fn a_start_that_meets_a_dead_index_rebuilds_it() {
    let drive = an_indexed_drive("cover-toggle-dead-index-test");

    // SQLITE_IOERR / SQLITE_IOERR_WRITE: the shape a dying disk reports.
    state::fail_index_for_test(
        &crate::NoopEventSink,
        drive.volume_id,
        crate::indexing::store::IndexFailure {
            code: 10,
            extended_code: 778,
        },
    );
    assert!(
        state::is_failed(drive.volume_id),
        "precondition: the volume is registered as failed, holding its key"
    );

    state::start_indexing_for(
        drive.volume_id,
        drive.tree.path().to_path_buf(),
        IndexVolumeKind::LocalExternal,
        true,
        state::Activation::IndexTheVolume,
    )
    .expect("the start rebuilds rather than refusing");

    assert!(
        !state::is_failed(drive.volume_id),
        "the dead index was cleared out of the start's way"
    );
    assert!(state::is_active(drive.volume_id), "and the volume is indexing again");
}

/// A start meeting a drive whose manager is momentarily OUT of the registry, with
/// nothing tearing it down, lets that manager come back and walks nothing again.
///
/// The fifth phase a start can meet, and the one that needs no request written
/// down: `Detached` exists for the milliseconds a scan start holds the manager
/// off the lock, and the volume comes back `Running` — which is exactly what the
/// start wanted. ❌ Recording a restart here would rebuild a live drive.
#[test]
fn a_start_on_a_detached_drive_lets_its_manager_come_back() {
    let drive = an_indexed_drive("cover-toggle-detached-live-test");
    let scans_before = drive.scans_started();

    state::while_detached_for_test(drive.volume_id, || turn_on(&drive));

    assert!(state::is_active(drive.volume_id), "the volume never stopped");
    assert_eq!(
        drive.scans_started(),
        scans_before,
        "and a start on a live drive still walks nothing again"
    );
}
