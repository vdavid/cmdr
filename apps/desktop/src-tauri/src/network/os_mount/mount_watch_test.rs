use std::time::Duration;

use cmdr_fs::log_private::LogPrivate;

use super::MountWatch;
use crate::test_support::{CapturedLog, capture_logs};

const THRESHOLD: Duration = Duration::from_secs(15);

/// Runs `body` on a paused-clock, current-thread runtime, so the watch's task logs on THIS
/// thread (what `capture_logs` sees) and its waits elapse without real time passing.
fn logs_of(body: impl Future<Output = ()>) -> Vec<CapturedLog> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .start_paused(true)
        .build()
        .expect("a current-thread runtime builds");
    capture_logs(|| runtime.block_on(body)).1
}

fn warnings(logs: &[CapturedLog]) -> Vec<&str> {
    logs.iter()
        .filter(|log| log.level == log::Level::Warn)
        .map(|log| log.message.as_str())
        .collect()
}

#[test]
fn a_mount_outliving_the_threshold_warns_while_it_waits_and_when_it_returns() {
    let logs = logs_of(async {
        let watch = MountWatch::start("nas.local", "media", THRESHOLD);
        // allowed-test-sleep: the mount's duration IS the subject, on a paused clock
        tokio::time::sleep(Duration::from_secs(31)).await;
        watch.finish(false);
        tokio::task::yield_now().await;
    });

    let share = format!("{:?}", LogPrivate::volume("media"));
    let server = format!("{:?}", LogPrivate::host("nas.local"));
    assert_eq!(
        warnings(&logs),
        [
            format!(
                "Mounting share={share} on server={server} is still waiting on the system's mount service after 15 s"
            ),
            format!(
                "Mounting share={share} on server={server} is still waiting on the system's mount service after 30 s"
            ),
            format!(
                "Mounting share={share} on server={server} came back from the system's mount service after 31 s (failed)"
            ),
        ]
    );
}

#[test]
fn a_quick_mount_logs_nothing() {
    let logs = logs_of(async {
        let watch = MountWatch::start("nas.local", "media", THRESHOLD);
        // allowed-test-sleep: the mount's duration IS the subject, on a paused clock
        tokio::time::sleep(Duration::from_secs(2)).await;
        watch.finish(true);
        // Past the threshold, so a watch that kept running after `finish` would have spoken.
        // allowed-test-sleep: the quiet window IS the subject, on a paused clock
        tokio::time::sleep(Duration::from_secs(60)).await;
    });

    assert!(warnings(&logs).is_empty(), "got {:?}", warnings(&logs));
}
