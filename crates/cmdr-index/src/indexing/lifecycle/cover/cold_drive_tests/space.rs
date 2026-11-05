//! A drive whose index's disk runs low: it pauses, keeps its database, says nothing
//! to the error pipeline, and comes back by itself once there's room
//! (cmdr-reports#100, `state/space_pause.rs`).

use super::toggles::an_indexed_drive;
use super::*;
use crate::indexing::host::policy::{FakeHostPolicy, HostPolicy, install_for_test};
use crate::indexing::lifecycle::freshness::Freshness;
use crate::indexing::lifecycle::state;
use crate::indexing::writer::WriteMessage;

/// Drive the writer until it pauses for disk space. The writer asks the host as it
/// handles messages, at most once a second, so keep handing it some.
fn pause_for_low_disk(drive: &ColdDrive, host: &FakeHostPolicy) {
    host.note_index_storage_low(true);
    cmdr_fs::testing::wait_until(
        std::time::Duration::from_secs(10),
        "the drive to pause while the disk is low",
        || {
            if let Some((writer, _)) = state::get_writer_and_flux_for(drive.volume_id) {
                let _ = writer.send(WriteMessage::UpdateMeta {
                    key: "space-pause-test".to_string(),
                    value: "x".to_string(),
                });
            }
            state::get_freshness(drive.volume_id) == Some(Freshness::Paused)
        },
    );
}

/// The headline of #1002: while paused, the drive's database keeps answering (its
/// sizes serve as they were, stale), the badge reads `paused` with its last-scan
/// facts intact, and once there's room it comes back. A `LocalExternal` drive has
/// its own `.fseventsd`, but Cmdr doesn't replay it (`watch/DETAILS.md`, #808: a
/// same-UUID journal can hide a hole), so it does what it does at every mount:
/// walks itself whole, reconciling in place while its sizes keep serving, and ends
/// Fresh. ❌ Not Stale: David's order is replay, else rescan, and Stale only for a
/// drive with no event history at all (a share, a phone).
#[test]
fn a_paused_external_drive_keeps_serving_its_sizes_and_rescans_itself_when_room_returns() {
    let drive = an_indexed_drive("cover-space-pause-reads-test");
    assert_eq!(
        state::volume_kind(drive.volume_id),
        Some(IndexVolumeKind::LocalExternal)
    );
    let host = FakeHostPolicy::shared();
    let _host = install_for_test(Arc::clone(&host) as Arc<dyn HostPolicy>);
    let scans_before = drive.scans_started();
    pause_for_low_disk(&drive, &host);

    assert!(!state::is_active(drive.volume_id), "paused is not active");
    let status = drive.index.volume_status(drive.volume_id);
    assert_eq!(status.freshness, Some(Freshness::Paused), "the badge says paused");
    assert!(status.scan_completed_at.is_some(), "with its last-scan facts intact");
    assert!(
        !status.holds_index_storage,
        "an external drive isn't where the index lives"
    );
    assert!(
        drive.index.read_pool(drive.volume_id).is_some(),
        "the read pool stays installed while nothing writes"
    );
    assert!(
        drive
            .index
            .dir_stats(drive.volume_id, &drive.path("scope"))
            .expect("a read")
            .is_some(),
        "and listings keep their last-known sizes"
    );
    assert!(
        drive.events.events().iter().any(|event| matches!(
            event,
            crate::IndexEvent::FreshnessChanged {
                freshness: Freshness::Paused,
                ..
            }
        )),
        "the badge was told"
    );

    host.note_index_storage_low(false);
    cmdr_fs::testing::wait_until(
        std::time::Duration::from_secs(20),
        "the drive to come back and rescan itself once there's room",
        || {
            state::is_active(drive.volume_id)
                && drive.scans_started() > scans_before
                && state::get_freshness(drive.volume_id) == Some(Freshness::Fresh)
        },
    );
    assert!(
        drive
            .index
            .dir_stats(drive.volume_id, &drive.path("scope"))
            .expect("a read")
            .is_some(),
        "the restart serves the same sizes"
    );
    assert!(
        drive.events.events().iter().any(|event| matches!(
            event,
            crate::IndexEvent::FreshnessChanged {
                freshness: Freshness::Fresh,
                ..
            }
        )),
        "the badge was told it's back, Fresh"
    );
}

/// A user's stop on a paused drive is the last word: the paused instance goes
/// (gray), its reads with it, and room coming back doesn't restart it.
#[test]
fn stopping_a_paused_drive_keeps_it_stopped_when_space_returns() {
    let drive = an_indexed_drive("cover-space-pause-stop-test");
    let host = FakeHostPolicy::shared();
    let _host = install_for_test(Arc::clone(&host) as Arc<dyn HostPolicy>);
    pause_for_low_disk(&drive, &host);

    drive.index.disable_volume(drive.volume_id).expect("the drive stops");
    assert_eq!(drive.index.volume_status(drive.volume_id).freshness, None, "gray");
    assert!(drive.index.read_pool(drive.volume_id).is_none(), "reads skip");

    host.note_index_storage_low(false);
    // allowed-test-sleep: proves an ABSENCE (no restart) across several of the test
    // build's 50 ms rechecks (`space_pause::RECHECK_EVERY`).
    std::thread::sleep(std::time::Duration::from_millis(300));
    assert!(
        !state::is_active(drive.volume_id),
        "the stop wins over the pause's restart"
    );
    assert_eq!(drive.index.volume_status(drive.volume_id).freshness, None);
}

/// The memory watchdog stops ALL indexing and nothing restarts it but the user. A
/// paused volume whose disk clears in the meantime must not come back on its own,
/// into the memory pressure that stopped every other volume.
#[test]
fn a_watchdog_stop_during_the_pause_keeps_the_drive_stopped_when_space_returns() {
    let drive = an_indexed_drive("cover-space-pause-watchdog-test");
    let host = FakeHostPolicy::shared();
    let _host = install_for_test(Arc::clone(&host) as Arc<dyn HostPolicy>);
    pause_for_low_disk(&drive, &host);

    crate::indexing::resources::memory_watchdog::note_stop();
    host.note_index_storage_low(false);
    // allowed-test-sleep: proves an ABSENCE (no restart) across several of the test
    // build's 50 ms rechecks (`space_pause::RECHECK_EVERY`).
    std::thread::sleep(std::time::Duration::from_millis(300));
    assert!(
        !state::is_active(drive.volume_id),
        "the watchdog's stop wins over the space pause's restart"
    );
}

#[test]
fn a_drive_pauses_while_the_disk_is_low_and_resumes_when_there_is_room() {
    let drive = an_indexed_drive("cover-space-pause-test");
    let host = FakeHostPolicy::shared();
    let _host = install_for_test(Arc::clone(&host) as Arc<dyn HostPolicy>);
    pause_for_low_disk(&drive, &host);
    assert!(!state::is_failed(drive.volume_id), "a pause, not a failure");
    assert!(
        !drive
            .events
            .events()
            .iter()
            .any(|event| matches!(event, crate::IndexEvent::Error { .. })),
        "a low disk is nothing to report"
    );
    // Still low: it stays paused across a few rechecks.
    // allowed-test-sleep: proves an ABSENCE (no restart while low) across several of the
    // test build's 50 ms rechecks (`space_pause::RECHECK_EVERY`).
    std::thread::sleep(std::time::Duration::from_millis(300));
    assert!(
        !state::is_active(drive.volume_id),
        "it waits while the disk is still low"
    );

    host.note_index_storage_low(false);
    cmdr_fs::testing::wait_until(
        std::time::Duration::from_secs(20),
        "the drive to resume once there's room",
        || state::is_active(drive.volume_id),
    );
    assert!(
        drive.is_indexed(&drive.path("scope/found.txt")),
        "the database it kept is the one it resumed on"
    );
    let conn = IndexStore::open_read_connection(&drive.db_path()).expect("read connection");
    assert!(
        IndexStore::get_meta(&conn, "space-pause-test")
            .expect("readable")
            .is_some(),
        "the same database file: a `Failed`-style rebuild would have deleted it"
    );
}

/// A trip that lands while a scan start has the manager out (`Detached`) is
/// CLAIMED, and the handback finishes the pause: the signal is one-shot, so a
/// bounced trip would leave the volume running over a writer that already exited.
#[test]
fn a_pause_landing_mid_scan_start_still_ends_paused() {
    let drive = an_indexed_drive("cover-space-pause-detached-test");
    let host = FakeHostPolicy::shared();
    let _host = install_for_test(Arc::clone(&host) as Arc<dyn HostPolicy>);
    host.note_index_storage_low(true);

    state::while_detached_for_test(drive.volume_id, || {
        state::pause_for_space(
            drive.volume_id,
            crate::indexing::lifecycle::failure::SpaceStop::DiskFull,
        );
    });
    assert_eq!(
        state::get_freshness(drive.volume_id),
        Some(Freshness::Paused),
        "the handback carried the pause out"
    );
    assert!(drive.index.read_pool(drive.volume_id).is_some(), "reads kept");

    host.note_index_storage_low(false);
    cmdr_fs::testing::wait_until(
        std::time::Duration::from_secs(20),
        "the drive to come back once there's room",
        || state::is_active(drive.volume_id),
    );
}
