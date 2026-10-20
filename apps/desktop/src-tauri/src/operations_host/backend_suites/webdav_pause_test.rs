//! A paused upload onto a WebDAV server resumes and lands, however long the
//! pause outlasted the server's patience.
//!
//! The engine pauses a copy by parking the SOURCE between chunks, which stalls
//! a WebDAV upload's PUT body mid-request, and every real server drops a body
//! that goes silent long enough (nginx's `client_body_timeout` and Apache's
//! `Timeout`, a minute each). `webdav-fixture-impatient` drops one after 3 s,
//! so a pause outlasts it in seconds (GitHub #358).

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use cmdr_operations::VolumeCopyConfig;

use super::network_gated_source_test_support::{CANCEL_PAYLOAD_BYTES, gated_upload};
use super::network_transfer_test_support::{
    assert_no_staging_litter, budget, clean_deep, read_all, self_describing_bytes, sha256, start_copy,
};
use super::webdav_test_support::{WebdavFixture, fixture_on};
use crate::ignore_poison::IgnorePoison;

/// How long the impatient fixture lets a request body go silent
/// (`BODY_IDLE_TIMEOUT` in `apps/desktop/test/webdav-servers/docker-compose.yml`).
const SERVER_BODY_IDLE: Duration = Duration::from_secs(3);

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the WebDAV fixture stack: apps/desktop/test/webdav-servers/start.sh (webdav-fixture-impatient)"]
async fn webdav_integration_an_upload_paused_past_the_servers_patience_resumes_and_lands() {
    let (remote, dir) = fixture_on(WebdavFixture::Impatient).await;
    let bytes = self_describing_bytes(CANCEL_PAYLOAD_BYTES, "paused");
    let source = gated_upload(bytes.clone()).await;
    let running = start_copy(
        "webdav-long-pause",
        Arc::clone(&source.volume),
        vec![PathBuf::from("/big.bin")],
        Arc::clone(&remote),
        dir.clone(),
        VolumeCopyConfig::default(),
    )
    .await;

    // Two chunks into the server (the second hand-out proves the first was
    // taken), then the pause, with the source free to answer the moment the
    // pause lifts.
    source.gate.add_permits(2);
    crate::test_support::wait_until_async(budget(Duration::from_secs(6)), "two chunks into the PUT", || {
        source.handed_out.load(std::sync::atomic::Ordering::SeqCst) >= 2
    })
    .await;
    assert!(
        cmdr_operations::testing::pause_write_operation(&running.engine, &running.operation_id),
        "the upload is parked on the gate, so it's still running to pause"
    );
    source.gate.add_permits(100_000);
    // allowed-test-sleep: the server's own idle clock is what's under test, and it has no seam; the pause has to outlast it for real.
    tokio::time::sleep(SERVER_BODY_IDLE + Duration::from_millis(1_500)).await;
    let ended_while_paused = || {
        running
            .events
            .errors
            .lock_ignore_poison()
            .iter()
            .map(|e| format!("{:?}", e.error))
            .collect::<Vec<_>>()
    };
    assert!(
        running.events.complete.lock_ignore_poison().is_empty(),
        "a paused upload doesn't finish; it ended with {:?}",
        ended_while_paused()
    );
    assert!(
        cmdr_operations::testing::resume_write_operation(&running.engine, &running.operation_id),
        "the upload is still there to resume; it ended while paused with {:?}",
        ended_while_paused()
    );
    running.settle().await;
    running.assert_no_errors();

    assert_eq!(
        sha256(&read_all(remote.as_ref(), &dir.join("big.bin")).await),
        sha256(&bytes),
        "every byte lands once the pause lifts"
    );
    assert_no_staging_litter(remote.as_ref(), &dir, "an upload paused past the server's patience").await;
    clean_deep(remote.as_ref(), &dir).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the WebDAV fixture stack: apps/desktop/test/webdav-servers/start.sh (webdav-fixture-impatient)"]
async fn webdav_integration_a_cancel_after_the_server_cut_off_a_paused_upload_leaves_nothing() {
    let (remote, dir) = fixture_on(WebdavFixture::Impatient).await;
    let source = gated_upload(self_describing_bytes(CANCEL_PAYLOAD_BYTES, "paused-then-cancelled")).await;
    let running = start_copy(
        "webdav-long-pause-cancel",
        Arc::clone(&source.volume),
        vec![PathBuf::from("/big.bin")],
        Arc::clone(&remote),
        dir.clone(),
        VolumeCopyConfig::default(),
    )
    .await;
    source.gate.add_permits(2);
    crate::test_support::wait_until_async(budget(Duration::from_secs(6)), "two chunks into the PUT", || {
        source.handed_out.load(std::sync::atomic::Ordering::SeqCst) >= 2
    })
    .await;
    assert!(cmdr_operations::testing::pause_write_operation(
        &running.engine,
        &running.operation_id
    ));
    source.gate.add_permits(100_000);
    // allowed-test-sleep: the server's own idle clock is what's under test, and it has no seam; the pause has to outlast it for real.
    tokio::time::sleep(SERVER_BODY_IDLE + Duration::from_millis(1_500)).await;

    // The write is waiting out the pause with the cut-off upload already gone;
    // a cancel there ends it as a cancel.
    running.engine.cancel_write_operation(&running.operation_id, false);
    running.settle().await;

    assert!(
        running.events.complete.lock_ignore_poison().is_empty(),
        "a cancelled upload doesn't complete"
    );
    assert!(
        !running.events.cancelled.lock_ignore_poison().is_empty(),
        "a cancel while paused is reported as the cancel it is, never a failed transfer"
    );
    assert!(
        !remote.exists(&dir.join("big.bin")).await,
        "nothing lands at the user's name"
    );
    assert_no_staging_litter(remote.as_ref(), &dir, "a cancel after a pause the server outlasted").await;
    clean_deep(remote.as_ref(), &dir).await;
}
