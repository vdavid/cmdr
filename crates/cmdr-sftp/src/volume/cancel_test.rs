//! Calling a connect off: that it stops promptly, that it leaves nothing behind,
//! and that the one phase which can't be dropped survives being abandoned.
//!
//! ❗ **The hello window is why this file exists.** A cancel in the other two
//! phases drops a `russh` future, which is ordinary; a cancel in the hello has to
//! walk away from a future that panics its task when dropped
//! (`transport.rs` § hazard 1), so the cell for it drives real containers.
//!
//! The servers, the ports, and what each one is for:
//! `apps/desktop/test/sftp-servers/README.md`.

use std::path::Path;
use std::time::{Duration, Instant};

use cmdr_fs::volume::Volume;
use tokio_util::sync::CancellationToken;

use super::testing::*;
use super::{SftpConnectOutcome, connect_sftp_volume};
use crate::errors::SftpConnectError;
use crate::transport;

const FIXTURE: &str = "sftp-servers/start.sh (sftp-fixture)";

/// What "promptly" has to mean. Two orders of magnitude under the 10 s a phase
/// gets, so a cell that measures this can't pass by accident on a slow runner.
const PROMPTLY: Duration = Duration::from_secs(2);

/// Cancels `token` after `delay`, the way a user's click reaches a running dial.
fn cancel_after(token: &CancellationToken, delay: Duration) {
    let token = token.clone();
    tokio::spawn(async move {
        // allowed-test-sleep: the delay IS the subject, sweeping where in the
        // handshake the user's click lands
        tokio::time::sleep(delay).await;
        token.cancel();
    });
}

/// A cancel landing anywhere in a live handshake ends the connect at once, and
/// ❗ leaves no volume behind.
///
/// The delays sweep the whole connect rather than aiming at one phase, because
/// nothing outside the dial can see where a phase ends. A warm connect against
/// `sftp-fixture-openssh` is about 20 ms end to end (2026-08-23), so 0–24 ms
/// lands in the TCP connect, the key exchange, the auth ladder, and the hello in
/// turn. A delay that arrives after the session is up is an ordinary race and
/// answers `connected` — which is its own assertion, since ❗ the token is
/// re-read after the dial lands. What must never happen is a wait that runs on
/// into the phase budget.
#[tokio::test]
#[ignore = "needs the SFTP fixture stack: sftp-servers/start.sh (sftp-fixture)"]
async fn a_cancel_during_the_handshake_stops_the_dial_promptly_and_registers_nothing() {
    let params = fixture_params("OPENSSH", 12480);
    let host = fixture_host(&params, Some(FIXTURE_PASSWORD));
    // Approve first, so every dial below runs the whole handshake rather than
    // stopping at a host-key prompt.
    drop(connect_fixture(&host, params.clone()).await);

    let mut cancelled = 0;
    for delay_ms in [0, 3, 6, 9, 12, 15, 18, 21, 24] {
        let cancel = CancellationToken::new();
        cancel_after(&cancel, Duration::from_millis(delay_ms));

        let started = Instant::now();
        let outcome = connect_sftp_volume(
            "fixture",
            "sftp-cancelled-handshake",
            params.clone(),
            host.clone(),
            cancel,
        )
        .await;
        let took = started.elapsed();

        assert!(
            took < PROMPTLY,
            "a cancel at {delay_ms} ms has to end the dial on the spot, and this one took {took:?}"
        );
        match outcome {
            Err(SftpConnectError::Cancelled) => cancelled += 1,
            // The cancel lost the race with a connect that was already done. ❗ A
            // live session, not a half-built one, and dropping it here is the
            // shutdown.
            Ok(SftpConnectOutcome::Connected(volume)) => volume.disconnect().await,
            Ok(SftpConnectOutcome::NeedsHostKeyApproval(_)) => {
                panic!("this server's key was approved before the sweep started")
            }
            Err(_) => panic!("a cancelled dial answers `Cancelled`, never a transport failure"),
        }
    }
    assert!(
        cancelled > 0,
        "none of the nine delays landed inside a connect, so this cell tested nothing"
    );

    // The server and this process both survived being walked away from nine
    // times, which is the other half of what the sweep is for.
    let volume = connect_fixture(&host, params).await;
    assert!(volume.exists(Path::new("hello.txt")).await, "{FIXTURE}");
}

/// ❗ **The hazardous one.** A cancel inside the SFTP hello must not drop the
/// engine's `Sftp::new` future, and must not leave the session it builds alive.
///
/// The window is about a millisecond wide against a local server, so a cell can't
/// reach it by timing. `transport::dial_cancelling_inside_the_hello` runs
/// everything up to the engine's start on a live token and hands `await_hello` an
/// already-cancelled one, which is the code path a real cancel takes. What comes
/// back is the task that finishes the abandoned engine, so awaiting it proves the
/// engine ran to completion (dropped or aborted, its task would have died on the
/// `tasks.rs` `unwrap` instead) and that both it and the session were then let
/// go.
#[tokio::test]
#[ignore = "needs the SFTP fixture stack: sftp-servers/start.sh (sftp-fixture)"]
async fn a_cancel_inside_the_hello_window_leaves_no_live_session_and_no_panic() {
    let params = fixture_params("OPENSSH", 12480);
    let host = fixture_host(&params, Some(FIXTURE_PASSWORD));
    drop(connect_fixture(&host, params.clone()).await);

    for _ in 0..5 {
        let started = Instant::now();
        let finishing = transport::dial_cancelling_inside_the_hello(params.clone(), host.clone())
            .await
            .unwrap_or_else(|e| panic!("the hello window must answer a cancel, got {e:?}"));
        assert!(
            started.elapsed() < PROMPTLY,
            "❗ the user's wait ends at the cancel; the engine finishing is the detached task's problem"
        );

        let finished = tokio::time::timeout(PROMPTLY, finishing)
            .await
            .expect("the abandoned engine has to finish and let its session go, not sit on it")
            .expect("the task doing the finishing must not die either");
        assert!(
            finished.is_ok(),
            "❗ the engine's own task died instead of finishing, which is the `tasks.rs` `unwrap` going off: something dropped or aborted `Sftp::new`'s future"
        );
    }

    // A panic in a spawned task doesn't fail the task that spawned it, so the
    // proof it never happened is that the server and this binary are both still
    // usable afterwards.
    let volume = connect_fixture(&host, params).await;
    assert!(volume.exists(Path::new("hello.txt")).await, "{FIXTURE}");
}
