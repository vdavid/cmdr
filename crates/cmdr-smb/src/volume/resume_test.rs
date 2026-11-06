//! `Reattach` with no server in reach: when it hands a held handle a try, and
//! when it stops. What a try does with it (the reopen, the resume) runs against
//! Docker in `reattach_integration_test.rs`.

use super::*;
use crate::volume::state::ConnectionState;
use crate::volume::test_support::*;

#[tokio::test]
async fn a_share_that_is_up_gets_a_try_at_once() {
    let vol = make_test_volume_direct();
    let mut tries = Reattach::new(&vol.inner);
    assert!(tries.next().await.is_ok());
}

#[tokio::test]
async fn an_unmounted_share_stops_the_tries_for_good() {
    let vol = make_test_volume();
    vol.inner.unmounted.store(true, Ordering::Relaxed);
    let mut tries = Reattach::new(&vol.inner);
    assert!(matches!(tries.next().await, Err(GaveUp::Terminal)));
}

/// A server that stays down is dialed once per step (single-flight through
/// `do_attempt_reconnect`) and then given up on: the handle's owner reports the
/// loss, and the transfer engine's own outage wait takes it from there.
#[tokio::test(start_paused = true)]
async fn a_server_that_stays_down_is_given_up_on_after_the_last_step() {
    let vol = make_test_volume();
    // Nothing listens on port 1, so every dial is refused at once.
    {
        let mut params = vol.inner.params.write().await;
        params.server = "127.0.0.1".to_string();
        params.port = 1;
    }
    let started = tokio::time::Instant::now();
    let mut tries = Reattach::new(&vol.inner);
    assert!(matches!(tries.next().await, Err(GaveUp::OutOfTime)));
    assert_eq!(vol.inner.connection_state(), ConnectionState::Disconnected);
    let waited: Duration = REATTACH_BACKOFF.iter().sum();
    assert!(
        started.elapsed() >= waited,
        "every step is waited out before giving up: waited {:?}, steps sum to {waited:?}",
        started.elapsed()
    );
}
