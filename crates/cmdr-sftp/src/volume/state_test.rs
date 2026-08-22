//! One transition, one event.
//!
//! A server that goes down fails every operation aimed at it. If each failure
//! were reported, the frontend would take a banner it already drew and redraw it
//! per in-flight listing, which reads as a flicker and, on a busy volume, as a
//! flood.

use std::sync::Arc;

use cmdr_fs::volume::Volume;
use cmdr_fs::volume::host::VolumeHost;
use cmdr_fs::volume::host::events::{RecordingVolumeEvents, VolumeConnection, VolumeEventSink};

use super::super::test_support::{TEST_ROOT, make_test_volume_with};
use super::ConnectionState;
use crate::auth::AuthRungUsed;

/// A volume whose events land somewhere a test can read them.
fn watched() -> (Arc<RecordingVolumeEvents>, super::super::SftpVolume) {
    let events = Arc::new(RecordingVolumeEvents::new());
    let host = VolumeHost::builder()
        .events(Arc::clone(&events) as Arc<dyn VolumeEventSink>)
        .build();
    (events, make_test_volume_with(TEST_ROOT, AuthRungUsed::Agent, host))
}

/// The same state reported twice is one event, and a real change is always one
/// more.
#[test]
fn a_state_that_did_not_move_reports_nothing() {
    let (events, volume) = watched();

    assert!(volume.inner.emit_if_changed(ConnectionState::Disconnected));
    assert!(!volume.inner.emit_if_changed(ConnectionState::Disconnected));
    assert!(!volume.inner.emit_if_changed(ConnectionState::Disconnected));
    assert!(volume.inner.emit_if_changed(ConnectionState::Connected));

    let seen: Vec<VolumeConnection> = events.transitions().into_iter().map(|(_, state)| state).collect();
    assert_eq!(
        seen,
        vec![VolumeConnection::Disconnected, VolumeConnection::Connected],
        "three reports of the same state are one event; the return value is what tells the caller it won the edge"
    );
}

/// Every event names the id the listing cache and the reconnect manager key on.
#[test]
fn a_transition_names_the_volume_it_is_about() {
    let (events, volume) = watched();
    volume.inner.emit_if_changed(ConnectionState::NeedsCredentials);

    let transitions = events.transitions();
    assert_eq!(transitions.len(), 1);
    assert_eq!(transitions[0].0, volume.volume_id());
    assert_eq!(transitions[0].1, VolumeConnection::NeedsCredentials);
}

/// A retired volume still tracks its own state and says nothing about it.
///
/// Its id belongs to a newer instance now, so a `disconnected` under that id
/// would tell the frontend a healthy volume just dropped.
#[test]
fn a_retired_volume_reports_nothing_under_an_id_it_no_longer_owns() {
    let (events, volume) = watched();
    volume.on_superseded();

    assert!(
        volume.inner.emit_if_changed(ConnectionState::Disconnected),
        "the volume still knows where it stands, for whoever is still holding it"
    );
    assert_eq!(volume.inner.connection_state(), ConnectionState::Disconnected);
    assert!(
        events.transitions().is_empty(),
        "but it says nothing under an id somebody else owns"
    );
}

/// ❗ Superseding must not take the session away from the callers still using it.
///
/// A running transfer, an open viewer stream, and the indexer all hold an `Arc`
/// to this instance across a re-registration. Retiring is letting go of the ID,
/// not of the connection.
#[test]
fn superseding_retires_the_id_and_leaves_the_session_alone() {
    let (_events, volume) = watched();
    volume.on_superseded();

    assert!(volume.retirement().expect("a flag this backend publishes").is_retired());
    assert!(
        !volume.inner.unmounted.load(std::sync::atomic::Ordering::Relaxed),
        "a supersede is not an eject: nothing here may tear a live session out from under a holder"
    );
}
