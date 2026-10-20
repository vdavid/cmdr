//! How a mount Cmdr gave up on reads: a slow or absent server, or a wedged
//! `NetAuthSysAgent` (`DETAILS.md` § "A wedged `NetAuthSysAgent` hangs every NetFS mount").

use super::*;
use std::time::Duration;

/// A ledger of its own per test, so a stuck call one test files can't refuse the
/// mounts another test (or a live fixture test) makes.
fn ledger() -> &'static CallLedger {
    Box::leak(Box::new(CallLedger::new()))
}

const HELPER: Option<u32> = Some(4_242);

fn target(port: u16) -> GivenUpMount<'static> {
    GivenUpMount {
        server: "127.0.0.1",
        share: "photos",
        port,
    }
}

/// A port something answers on, held open for as long as the listener lives.
async fn answering_port() -> (tokio::net::TcpListener, u16) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind a loopback port");
    let port = listener.local_addr().expect("read the bound port").port();
    (listener, port)
}

/// A port nothing answers on: bound once, then let go, so a connect is refused.
async fn silent_port() -> u16 {
    let (listener, port) = answering_port().await;
    drop(listener);
    port
}

#[test]
fn a_call_still_running_blocks_nothing_until_it_is_found_stuck() {
    let ledger = ledger();
    assert!(ledger.stuck_call(HELPER).is_none());
    let _call = ledger.start();
    assert!(ledger.stuck_call(HELPER).is_none(), "a slow call isn't a stuck one");
}

#[test]
fn a_stuck_call_blocks_new_mounts_while_it_is_still_in_flight() {
    let ledger = ledger();
    let call = ledger.start();
    assert!(ledger.mark_stuck(call.id(), HELPER));
    assert!(ledger.stuck_call(HELPER).is_some());
    assert!(ledger.stuck_call(HELPER).is_some(), "asking doesn't clear it");
}

/// The daemon answered at last (or died), so NetFS let the thread go: nothing is
/// stuck any more, and the next mount goes out.
#[test]
fn a_stuck_call_that_comes_back_stops_blocking() {
    let ledger = ledger();
    let call = ledger.start();
    assert!(ledger.mark_stuck(call.id(), HELPER));
    drop(call);
    assert!(ledger.stuck_call(HELPER).is_none());
}

/// ❗ The user's fix is `killall NetAuthSysAgent`. Once the helper isn't the one
/// the call got stuck in, the old call says nothing about the new helper, even if
/// NetFS never lets its thread go.
#[test]
fn a_restarted_helper_clears_the_stuck_call() {
    let ledger = ledger();
    let call = ledger.start();
    assert!(ledger.mark_stuck(call.id(), HELPER));
    assert!(
        ledger.stuck_call(Some(9_999)).is_none(),
        "a new pid is a restarted helper"
    );
    assert!(
        ledger.stuck_call(HELPER).is_none(),
        "and the old call stays forgotten, even under the old pid"
    );

    let other = ledger.start();
    assert!(ledger.mark_stuck(other.id(), HELPER));
    assert!(
        ledger.stuck_call(None).is_none(),
        "no helper running is a restart too: launchd starts it on the next mount"
    );
}

/// Couldn't read the helper's pid when the call got stuck: nothing says it was
/// restarted since, so the call keeps blocking until it comes back.
#[test]
fn an_unknown_helper_keeps_blocking() {
    let ledger = ledger();
    let call = ledger.start();
    assert!(ledger.mark_stuck(call.id(), None));
    assert!(ledger.stuck_call(Some(1)).is_some());
    assert!(ledger.stuck_call(None).is_some());
}

#[test]
fn a_call_that_already_came_back_cant_be_marked_stuck() {
    let ledger = ledger();
    let call = ledger.start();
    let id = call.id();
    drop(call);
    assert!(!ledger.mark_stuck(id, HELPER));
    assert!(ledger.stuck_call(HELPER).is_none());
}

#[test]
fn a_new_mount_is_refused_with_the_explanation_while_a_call_is_stuck() {
    let ledger = ledger();
    assert_eq!(refusal_while_stuck(ledger, HELPER, "nas"), None);
    let call = ledger.start();
    assert!(ledger.mark_stuck(call.id(), HELPER));
    assert_eq!(
        refusal_while_stuck(ledger, HELPER, "nas"),
        if CAN_WEDGE {
            Some(MountError::SignInHelperStuck {
                server: "nas".to_string(),
            })
        } else {
            None
        }
    );
}

/// The wedge reading: a mount that waited past [`SUSPECT_AFTER`] on a server that
/// answers on its SMB port. macOS only, because only NetFS's helper wedges.
#[cfg(target_os = "macos")]
#[tokio::test]
async fn a_long_wait_on_a_server_that_answers_reads_as_a_stuck_helper() {
    let (_listener, port) = answering_port().await;
    for why in [GaveUpWhy::TimedOut, GaveUpWhy::Cancelled] {
        let ledger = ledger();
        let call = ledger.start();
        let verdict = read_given_up(ledger, call.id(), why, SUSPECT_AFTER, target(port), || HELPER).await;
        assert_eq!(
            verdict,
            MountError::SignInHelperStuck {
                server: "127.0.0.1".to_string()
            },
            "{why:?}"
        );
        assert!(
            ledger.stuck_call(HELPER).is_some(),
            "{why:?}: the next mount is refused right away"
        );
    }
}

#[tokio::test]
async fn a_long_wait_on_a_server_that_doesnt_answer_keeps_its_plain_answer() {
    let port = silent_port().await;
    let ledger = ledger();
    let call = ledger.start();
    let timed_out = read_given_up(
        ledger,
        call.id(),
        GaveUpWhy::TimedOut,
        SUSPECT_AFTER,
        target(port),
        || HELPER,
    )
    .await;
    assert_eq!(
        timed_out,
        MountError::Timeout {
            server: "127.0.0.1".to_string()
        }
    );
    let cancelled = read_given_up(
        ledger,
        call.id(),
        GaveUpWhy::Cancelled,
        SUSPECT_AFTER,
        target(port),
        || HELPER,
    )
    .await;
    assert_eq!(
        cancelled,
        MountError::Cancelled {
            share: "photos".to_string()
        }
    );
    assert!(ledger.stuck_call(HELPER).is_none());
}

/// A healthy NetFS mount takes a second or two. Cancelling sooner than
/// [`SUSPECT_AFTER`] is someone changing their mind, ❌ not evidence.
#[tokio::test]
async fn a_short_wait_is_never_a_stuck_helper() {
    let (_listener, port) = answering_port().await;
    let ledger = ledger();
    let call = ledger.start();
    let early = SUSPECT_AFTER - Duration::from_millis(1);
    let cancelled = read_given_up(ledger, call.id(), GaveUpWhy::Cancelled, early, target(port), || HELPER).await;
    assert_eq!(
        cancelled,
        MountError::Cancelled {
            share: "photos".to_string()
        }
    );
    assert!(ledger.stuck_call(HELPER).is_none());
}

/// A call that came back in the moment the mount gave up on it isn't stuck, however
/// the server answers.
#[tokio::test]
async fn a_call_that_came_back_meanwhile_keeps_its_plain_answer() {
    let (_listener, port) = answering_port().await;
    let ledger = ledger();
    let call = ledger.start();
    let id = call.id();
    drop(call);
    let verdict = read_given_up(ledger, id, GaveUpWhy::TimedOut, SUSPECT_AFTER, target(port), || HELPER).await;
    assert_eq!(
        verdict,
        MountError::Timeout {
            server: "127.0.0.1".to_string()
        }
    );
}

/// The pid lookup the restart check rides on finds a process by its name.
#[cfg(target_os = "macos")]
#[test]
fn a_running_process_is_found_by_name() {
    let own = std::process::id();
    let name = process_name(own).expect("this process has a name");
    assert!(pids_named(&name).contains(&own));
    assert!(pids_named("no-such-process-cmdr-test").is_empty());
}
