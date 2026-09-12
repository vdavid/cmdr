//! Stopping a removable drive's index for an eject. Whatever window the stop
//! lands in, it answers "released" only once nothing is working on the drive any
//! more: an unmount that meets a live watcher can wedge macOS FSKit.

use std::time::Duration;

use super::toggles::an_indexed_drive;
use super::*;
use crate::indexing::lifecycle::state::{self, RemovableStop};

/// A stop landing while ANOTHER teardown is draining the drive waits for that
/// drain. The drain's manager still holds the watcher until it shuts down, so
/// answering from the published `ShuttingDown` alone let an eject unmount under it.
#[test]
fn a_stop_that_meets_a_drain_in_flight_waits_for_it() {
    let drive = an_indexed_drive("cover-removal-mid-drain-test");
    assert_eq!(
        state::volume_kind(drive.volume_id),
        Some(IndexVolumeKind::LocalExternal),
        "precondition: a drive the removable stop is for"
    );

    state::while_stopping_for_test(drive.volume_id, || {
        assert_eq!(
            state::stop_removable_volume(drive.volume_id, Duration::ZERO),
            RemovableStop::StillReleasing,
            "the drain's manager hasn't shut down yet, so the drive isn't let go"
        );
    });

    // The window closed with the drain run to its end, so now nothing holds it.
    assert!(!state::is_active(drive.volume_id), "the drain retired the instance");
    assert_eq!(
        state::stop_removable_volume(drive.volume_id, Duration::ZERO),
        RemovableStop::NothingToStop,
        "and the manager that drained is gone with it"
    );
}

/// A stop landing while a scan start holds the drive's manager out of the
/// registry is only RECORDED, and the drain runs when the manager comes back. The
/// stop waits for that, rather than reporting a manager it never saw as let go.
#[test]
fn a_stop_that_lands_during_a_scan_start_waits_for_the_handback() {
    let drive = an_indexed_drive("cover-removal-detached-test");

    state::while_detached_for_test(drive.volume_id, || {
        assert_eq!(
            state::stop_removable_volume(drive.volume_id, Duration::ZERO),
            RemovableStop::StillReleasing,
            "the scan start still holds the manager; the stop is only claimed on it"
        );
    });

    // Handing the manager back carried the claimed stop out, drain and all.
    assert!(
        !state::is_active(drive.volume_id),
        "the claimed stop ran at the handback"
    );
    assert_eq!(
        state::stop_removable_volume(drive.volume_id, Duration::ZERO),
        RemovableStop::NothingToStop,
        "and the manager it drained is gone"
    );
}
