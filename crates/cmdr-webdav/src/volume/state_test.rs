//! One transition, one event.

use std::sync::Arc;

use cmdr_fs::volume::Volume;
use cmdr_fs::volume::host::VolumeHost;
use cmdr_fs::volume::host::events::{RecordingVolumeEvents, VolumeConnection, VolumeEventSink};

use super::super::WebdavVolume;
use super::super::test_support::make_test_volume_with;
use super::ConnectionState;

fn watched() -> (Arc<RecordingVolumeEvents>, WebdavVolume) {
    let events = Arc::new(RecordingVolumeEvents::new());
    let host = VolumeHost::builder()
        .events(Arc::clone(&events) as Arc<dyn VolumeEventSink>)
        .build();
    (events, make_test_volume_with("/", host))
}

#[test]
fn a_state_that_did_not_move_reports_nothing() {
    let (events, volume) = watched();
    assert!(volume.inner.emit_if_changed(ConnectionState::Disconnected));
    assert!(!volume.inner.emit_if_changed(ConnectionState::Disconnected));
    assert!(volume.inner.emit_if_changed(ConnectionState::Connected));
    let seen: Vec<VolumeConnection> = events.transitions().into_iter().map(|(_, state)| state).collect();
    assert_eq!(seen, vec![VolumeConnection::Disconnected, VolumeConnection::Connected]);
}

#[test]
fn a_transition_names_the_volume_it_is_about() {
    let (events, volume) = watched();
    volume.inner.emit_if_changed(ConnectionState::NeedsCredentials);
    let transitions = events.transitions();
    assert_eq!(transitions.len(), 1);
    assert_eq!(transitions[0].0, volume.volume_id());
    assert_eq!(transitions[0].1, VolumeConnection::NeedsCredentials);
}

#[test]
fn a_retired_volume_reports_nothing_under_an_id_it_no_longer_owns() {
    let (events, volume) = watched();
    volume.on_superseded();
    assert!(volume.inner.emit_if_changed(ConnectionState::Disconnected));
    assert_eq!(volume.inner.connection_state(), ConnectionState::Disconnected);
    assert!(events.transitions().is_empty());
}

#[test]
fn a_volume_that_left_silently_reports_no_later_disconnect() {
    let (events, volume) = watched();
    volume.on_unmount();
    assert!(!volume.inner.emit_if_changed(ConnectionState::Disconnected));
    assert!(events.transitions().is_empty());
}
