//! A drive whose index's disk runs low: it pauses, keeps its database, says nothing
//! to the error pipeline, and comes back by itself once there's room
//! (cmdr-reports#100, `state/space_pause.rs`).

use super::toggles::an_indexed_drive;
use super::*;
use crate::indexing::host::policy::{FakeHostPolicy, HostPolicy, install_for_test};
use crate::indexing::lifecycle::state;
use crate::indexing::writer::WriteMessage;

#[test]
fn a_drive_pauses_while_the_disk_is_low_and_resumes_when_there_is_room() {
    let drive = an_indexed_drive("cover-space-pause-test");
    let host = FakeHostPolicy::shared();
    let _host = install_for_test(Arc::clone(&host) as Arc<dyn HostPolicy>);

    host.note_index_storage_low(true);
    // The writer asks the host as it handles messages, at most once a second, so
    // keep handing it some until it stops.
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
            !state::is_active(drive.volume_id)
        },
    );
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
