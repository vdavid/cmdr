//! Session lifecycle with no server in reach: the reconnect early-exits, the
//! state transitions and the events they suppress, the watch-coverage gate, and
//! the two ways a share retires.

use super::*;
use crate::volume::SmbVolume;
use crate::volume::test_support::*;
use cmdr_fs::volume::host::events::{RecordingVolumeEvents, VolumeConnection};
use cmdr_fs::volume::{Volume, WatchCoverage};
use std::path::Path;

/// The volume id every recorded case reports under.
const RECORDED_ID: &str = "volumestestshare";

/// A disconnected volume whose event seam remembers what the frontend was told.
///
/// The transitions below are the only thing a user ever learns about a session
/// dropping, so "did it emit, and how many times" is the assertion — the state
/// the backend kept for itself is the cheap half.
fn volume_with_recorded_events() -> (SmbVolume, Arc<RecordingVolumeEvents>) {
    let events = Arc::new(RecordingVolumeEvents::new());
    let host = VolumeHost::builder().events(events.clone()).build();
    (make_test_volume_with(RECORDED_ID, host), events)
}

#[tokio::test]
async fn attempt_reconnect_noop_when_already_direct() {
    // If state is Direct, the helper bails early without building a session.
    // This is the path concurrent callers hit after the winner finishes.
    let vol = make_test_volume_direct();
    let result = vol.inner.do_attempt_reconnect().await;
    assert!(result.is_ok(), "expected Ok when already Direct, got {:?}", result);
    assert_eq!(vol.connection_state(), ConnectionState::Direct);
}

#[tokio::test]
async fn attempt_reconnect_bails_when_unmounted() {
    // After `on_unmount` runs, reconnect must not try to build a new session
    // (otherwise we'd leak a watcher + smb2 session into an orphaned volume).
    let vol = make_test_volume();
    vol.inner.unmounted.store(true, Ordering::Relaxed);
    let result = vol.inner.do_attempt_reconnect().await;
    assert!(
        matches!(result, Err(VolumeError::DeviceDisconnected(_))),
        "expected DeviceDisconnected when unmounted, got {:?}",
        result
    );
}

#[tokio::test]
async fn single_flight_concurrent_callers_serialize() {
    // Two parallel `do_attempt_reconnect` calls must serialize on
    // `reconnect_lock`. With the volume already Direct, both should return
    // Ok cheaply: the second one observes Direct after the first releases
    // the guard. Mutex contention itself is the assertion that single-flight
    // is wired up; if it wasn't, both calls would race past the early-exit
    // check.
    let vol = Arc::new(make_test_volume_direct());
    let v2 = Arc::clone(&vol);
    let v3 = Arc::clone(&vol);
    let (r1, r2) = tokio::join!(async move { v2.inner.do_attempt_reconnect().await }, async move {
        v3.inner.do_attempt_reconnect().await
    });
    assert!(r1.is_ok());
    assert!(r2.is_ok());
    assert_eq!(vol.connection_state(), ConnectionState::Direct);
}

/// Several in-flight ops all meet the same broken session, and each one runs the
/// transition. The user hears about it ONCE: a second banner for a share that is
/// already showing as unreachable is noise, and the frontend's backoff cycle
/// restarts on every event it sees.
#[tokio::test]
async fn transition_to_disconnected_idempotent() {
    let (vol, events) = volume_with_recorded_events();
    vol.inner.state.store(ConnectionState::Direct as u8, Ordering::Relaxed);

    vol.inner.transition_to_disconnected();
    vol.inner.transition_to_disconnected();

    assert_eq!(vol.connection_state(), ConnectionState::Disconnected);
    assert_eq!(
        events.transitions(),
        vec![(RECORDED_ID.to_string(), VolumeConnection::Disconnected)],
        "a second op meeting the same dead session must not re-announce it"
    );
}

/// The same on the way back up: a reconnect that raced another one and lost
/// still runs the transition, and a second "connected" would restart the
/// frontend's recovery for a share that is already serving.
#[tokio::test]
async fn transition_to_direct_idempotent() {
    let (vol, events) = volume_with_recorded_events();

    vol.inner.transition_to_direct();
    vol.inner.transition_to_direct();

    assert_eq!(vol.connection_state(), ConnectionState::Direct);
    assert_eq!(
        events.transitions(),
        vec![(RECORDED_ID.to_string(), VolumeConnection::Connected)],
        "the losing side of a reconnect race must not re-announce the session"
    );
}

/// A retired share still tracks its own state for the holders reading through
/// it, and says NOTHING under the volume id it no longer owns.
///
/// This is the whole point of the flag: the successor is serving that id on its
/// own healthy session, so a predecessor's dying watcher announcing a disconnect
/// would drop the frontend into "unreachable" and start a backoff cycle against
/// a share that never went away.
#[tokio::test]
async fn a_retired_share_announces_nothing_under_an_id_it_no_longer_owns() {
    let (vol, events) = volume_with_recorded_events();
    vol.inner.state.store(ConnectionState::Direct as u8, Ordering::Relaxed);
    vol.on_superseded();

    vol.inner.transition_to_disconnected();

    assert_eq!(
        vol.connection_state(),
        ConnectionState::Disconnected,
        "the share still tracks its own state for whoever still holds it"
    );
    assert!(
        events.transitions().is_empty(),
        "the id belongs to the successor, so this share must not speak for it: {:?}",
        events.transitions()
    );
}

#[test]
fn listing_watch_coverage_is_none_when_disconnected() {
    // No watcher_cancel set and state Disconnected: no coverage.
    let vol = make_test_volume();
    assert_eq!(vol.listing_watch_coverage(Path::new("/")), WatchCoverage::None);
}

#[test]
fn listing_watch_coverage_is_none_when_direct_but_no_watcher() {
    // State Direct but `watcher_cancel` empty: still `None` (we need both).
    let vol = make_test_volume_direct();
    assert_eq!(vol.listing_watch_coverage(Path::new("/")), WatchCoverage::None);
}

#[test]
fn listing_watch_coverage_is_none_when_watcher_set_but_disconnected() {
    // `watcher_cancel` populated but state Disconnected: `None`.
    let vol = make_test_volume();
    let (tx, _rx) = tokio::sync::oneshot::channel::<()>();
    *vol.inner.watcher_cancel.lock().unwrap() = Some(tx);
    assert_eq!(vol.listing_watch_coverage(Path::new("/")), WatchCoverage::None);
}

#[test]
fn listing_watch_coverage_is_every_writer_when_direct_and_watcher_set() {
    // Both conditions met. CHANGE_NOTIFY is raised by the server, so it sees
    // every client's writes, not only ours.
    let vol = make_test_volume_direct();
    let (tx, _rx) = tokio::sync::oneshot::channel::<()>();
    *vol.inner.watcher_cancel.lock().unwrap() = Some(tx);
    assert_eq!(vol.listing_watch_coverage(Path::new("/")), WatchCoverage::EveryWriter);
}

#[test]
fn on_unmount_marks_volume_dead() {
    // `on_unmount` is sync (called from FSEvents thread) and uses
    // `blocking_lock`, so this must be a `#[test]`, not a `#[tokio::test]`
    // (the latter panics inside a runtime when calling `blocking_lock`).
    let vol = make_test_volume_direct();
    assert!(!vol.inner.unmounted.load(Ordering::Relaxed));
    vol.on_unmount();
    assert!(vol.inner.unmounted.load(Ordering::Relaxed));
    assert_eq!(vol.connection_state(), ConnectionState::Disconnected);
}

/// Being superseded is not being unmounted. The successor took the volume id,
/// but this instance may still be serving a running copy, so its session and
/// connection state must survive untouched. Only the id-scoped parts retire.
#[test]
fn on_superseded_retires_the_id_but_keeps_the_session() {
    let vol = make_test_volume_direct();
    let (tx, _rx) = tokio::sync::oneshot::channel::<()>();
    *vol.inner.watcher_cancel.lock().unwrap() = Some(tx);

    vol.on_superseded();

    assert!(vol.inner.is_retired(), "the volume knows it's retired");
    assert!(
        !vol.inner.unmounted.load(Ordering::Relaxed),
        "supersede must not mark the volume dead: holders still use it"
    );
    assert_eq!(
        vol.inner.connection_state(),
        ConnectionState::Direct,
        "the session is still up, so ops on a held reference must not be gated off"
    );
    assert!(
        vol.inner.watcher_cancel.lock().unwrap().is_none(),
        "the watcher belongs to the volume id, which the successor now owns"
    );
}
