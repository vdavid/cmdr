//! What a machine's walk may hold while it waits on `fseventsd`: nothing anybody
//! else needs.
//!
//! A walk's first act is `begin_branch_coverage`, which starts the volume's branch
//! watch when none is up: a stream start plus `FSEventsGetCurrentEventId`, each a
//! round trip to the one `fseventsd` every process shares. Under load a single
//! start took 5.8 s, and doing it under `INDEX_REGISTRY` stalled `get_status` and
//! every other registry user, for every volume, for that long. These tests park
//! the fake stream start at a gate instead of timing a slow one.

use std::sync::mpsc;
use std::time::Duration;

use super::*;

use crate::indexing::lifecycle::state;
use crate::indexing::read::queries;
use crate::indexing::watch::watcher::fake_journal;

/// How long the status read gets before the test calls it stuck: a deadlock
/// guard, ❌ not a performance bound. A read that doesn't wait on the gate answers
/// in milliseconds; one that does can't answer until the gate opens, at all. Under
/// the 8 s per-test cap so the broken shape fails with its message.
const STUCK: Duration = Duration::from_secs(5);

/// Ask for the volume's status on another thread, and say whether it answered
/// while the stream start was still parked.
fn status_answers_while_parked(volume_id: &'static str) -> bool {
    let (answered, answer) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = answered.send(queries::get_status(volume_id).is_ok());
    });
    answer.recv_timeout(STUCK).is_ok()
}

#[test]
fn a_branch_watch_starting_inside_a_walk_leaves_the_registry_free() {
    let drive = Drive::new(
        "phased-lock-branch-watch",
        |root| {
            std::fs::create_dir_all(root.join("branch/inner")).expect("dirs");
        },
        &[],
    );
    let gate = fake_journal::park_stream_starts(drive.tree.path());
    drive.start();
    gate.wait_until_parked();

    let answered = status_answers_while_parked(drive.volume_id);
    gate.open();
    assert!(
        answered,
        "❌ the status read waited on a stream start, so the registry was held across an fseventsd round trip"
    );
    cmdr_fs::testing::wait_until(
        Duration::from_secs(30),
        "the branch watch to come up once its stream starts",
        || state::is_watching_for_test(drive.volume_id),
    );
}

/// Stopping a scan hands a branch-watched volume its watcher back, which is the
/// same stream start from a different door.
#[test]
fn a_branch_watch_restarting_after_a_scan_stop_leaves_the_registry_free() {
    let drive = Drive::new(
        "phased-lock-stop-scan",
        |root| {
            std::fs::create_dir_all(root.join("branch/inner")).expect("dirs");
        },
        &[],
    );
    drive.start();
    drive.wait_for_the_machine();
    assert!(
        state::is_watching_for_test(drive.volume_id),
        "a covered volume is branch-watched"
    );

    let gate = fake_journal::park_stream_starts(drive.tree.path());
    let stopping = std::thread::spawn(|| state::stop_scan("phased-lock-stop-scan"));
    gate.wait_until_parked();

    let answered = status_answers_while_parked(drive.volume_id);
    gate.open();
    stopping
        .join()
        .expect("the stop finishes")
        .expect("the volume is running");
    assert!(
        answered,
        "❌ the status read waited on the stop's stream start, so the registry was held across it"
    );
    assert!(
        state::is_watching_for_test(drive.volume_id),
        "and the stop gave the volume its branch watch back"
    );
}
