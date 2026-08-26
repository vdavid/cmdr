//! Calling a connect off: that it stops promptly, that it leaves nothing behind,
//! and that the phase which can't be cancelled by dropping a future stops too.
//!
//! ❗ **The hello window is why this file exists.** A cancel in the other phases
//! drops a `russh` future, which is ordinary; the hello runs in a task, so a
//! cancel there has to abort the engine and disconnect the session by hand
//! (`transport.rs` § "Cancelling a connect"). The cells drive real containers,
//! and the one that matters most asserts on the SERVER's side of the socket.
//!
//! The servers, the ports, and what each one is for:
//! `apps/desktop/test/sftp-servers/README.md`.

use std::path::Path;
use std::time::{Duration, Instant};

use cmdr_fs::testing::wait_until_async;
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
/// turn.
///
/// A delay that arrives around the moment the session lands is an ordinary race,
/// and either answer is right: `connected` if the token was still clear when the
/// dial returned, `cancelled` if it wasn't, because ❗ the token is re-read at
/// exactly that point. What must never happen is a session that is both
/// registered and called off, or a wait that runs on into the phase budget.
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
            // live session, not a half-built one, so dropping it is the shutdown
            // and there is nothing else to undo.
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

/// ❗ **The hazardous one.** A cancel inside the SFTP hello has to stop the
/// engine where it stands and close the SERVER's session with it, rather than
/// leaving either running.
///
/// A real subsystem answers in about a millisecond, so the window it opens is one
/// no cell can aim at. [`transport::HelloPeer::Stalling`] puts a command on the
/// channel that swallows `SSH_FXP_INIT` and answers nothing, which holds the
/// window open until this cell cancels into it; everything from there on is
/// `await_hello`'s own code.
///
/// The far end is where the claim lives, so that is where this asserts: the
/// command carries a marker into the server's process table, and it ends when
/// sshd closes the session's pipes. Session up, cancel, session gone, all inside
/// [`PROMPTLY`].
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the SFTP fixture stack: sftp-servers/start.sh (sftp-fixture)"]
async fn a_cancel_inside_the_hello_window_closes_the_servers_session_at_once() {
    let params = fixture_params("OPENSSH", 12480);
    let host = fixture_host(&params, Some(FIXTURE_PASSWORD));
    drop(connect_fixture(&host, params.clone()).await);

    let port = params.port;
    let marker = format!("cmdr-cancelled-hello-{}", std::process::id());
    let cancel = CancellationToken::new();
    let (reached, in_the_window) = tokio::sync::oneshot::channel();
    let dialing = tokio::spawn(transport::dial_cancelling_inside_the_hello(
        params.clone(),
        host.clone(),
        transport::HelloPeer::Stalling(marker.clone()),
        cancel.clone(),
        reached,
    ));
    in_the_window
        .await
        .expect("the dial has to reach the hello window before this cell can cancel into it");

    // Without this the rest could pass on a session that never opened.
    let up = format!("the server to open the session the hello waits on ({FIXTURE})");
    wait_until_async(PROMPTLY, &up, || sessions_open(port, &marker) == 1).await;

    let started = Instant::now();
    cancel.cancel();
    let outcome = tokio::time::timeout(PROMPTLY, dialing)
        .await
        .expect("❗ the user's wait ends at the cancel, whatever the server is doing")
        .expect("the dial itself must not panic");
    match outcome {
        Err(SftpConnectError::Cancelled) => {}
        Err(e) => panic!("a cancelled hello answers `Cancelled`, got {e:?}"),
        Ok(_) => panic!("a peer that answers nothing can't have delivered a hello"),
    }

    // ❗ The point of the cell. The engine is aborted and the session
    // disconnected at the cancel, so the socket closes now rather than whenever
    // the engine gives up: the far end has no session left to hold.
    let gone = format!("the server's session to close at the cancel ({FIXTURE})");
    wait_until_async(PROMPTLY, &gone, || sessions_open(port, &marker) == 0).await;
    assert!(
        started.elapsed() < PROMPTLY * 2,
        "the cancel and the close together have to stay inside the budget"
    );

    // A panic in a spawned task doesn't fail the task that spawned it, so the
    // proof it never happened is that the server and this binary are both still
    // usable afterwards.
    let volume = connect_fixture(&host, params).await;
    assert!(volume.exists(Path::new("hello.txt")).await, "{FIXTURE}");
}

/// A cancel inside a REAL hello window, which is the shape production hits.
///
/// The window is about a millisecond wide against a local server, so the cell
/// hands `await_hello` an already-cancelled token rather than trying to time one.
/// What it guards is the hazardous moment: aborting `Sftp::new` while the engine
/// is genuinely mid-hello used to panic a task inside `openssh-sftp-client`.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the SFTP fixture stack: sftp-servers/start.sh (sftp-fixture)"]
async fn a_cancel_inside_a_real_hello_window_stops_the_engine_without_panicking_it() {
    let params = fixture_params("OPENSSH", 12480);
    let host = fixture_host(&params, Some(FIXTURE_PASSWORD));
    drop(connect_fixture(&host, params.clone()).await);

    for _ in 0..5 {
        let cancel = CancellationToken::new();
        cancel.cancel();
        let (reached, _unwatched) = tokio::sync::oneshot::channel();
        let started = Instant::now();
        let outcome = transport::dial_cancelling_inside_the_hello(
            params.clone(),
            host.clone(),
            transport::HelloPeer::Subsystem,
            cancel,
            reached,
        )
        .await;
        match outcome {
            Err(SftpConnectError::Cancelled) => {}
            Err(e) => panic!("a cancelled hello answers `Cancelled`, got {e:?}"),
            Ok(_) => panic!("an already-cancelled token still let the hello through"),
        }
        assert!(
            started.elapsed() < PROMPTLY,
            "the cancel ends the dial on the spot, engine and all"
        );
    }

    let volume = connect_fixture(&host, params).await;
    assert!(volume.exists(Path::new("hello.txt")).await, "{FIXTURE}");
}

/// How many sessions the fixture server behind `port` is running for `marker`.
///
/// ❗ Ours alone, ❌ never a count of `sshd-session` processes: the container
/// stack is machine-wide and other suites hold sessions on the same server, so a
/// shared count would read their comings and goings as this cell's.
/// `HelloPeer::Stalling` names its command after `marker`, and the shell running
/// it ends the moment sshd closes the session's pipes.
fn sessions_open(port: u16, marker: &str) -> usize {
    let container = docker(&["ps", "--filter", &format!("publish={port}"), "--format", "{{.Names}}"]);
    let container = container.lines().next().unwrap_or_default();
    assert!(!container.is_empty(), "no container publishes port {port}: {FIXTURE}");
    docker(&["exec", container, "ps", "-eo", "args"])
        .lines()
        .filter(|line| line.contains(marker))
        .count()
}

/// One `docker` invocation, or a panic naming what could not be read.
///
/// ❗ Blocking, on purpose: it runs inside a `wait_until_async` condition, which
/// takes a sync closure. The cell is `multi_thread` so the dial keeps running on
/// another worker while this waits on the CLI.
fn docker(args: &[&str]) -> String {
    let out = std::process::Command::new("docker")
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("this cell reads the server's own process table and needs the `docker` CLI: {e}"));
    assert!(
        out.status.success(),
        "`docker {}` did not run: {}",
        args.join(" "),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}
