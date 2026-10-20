//! Which generation a failure belongs to, against the fake server: a reconnect's
//! hello that brings the device back starts a new generation, one that finds it
//! already up doesn't, and a late error from before the way back can't mark the
//! device gone again.

use std::sync::Arc;

use cmdr_fs::volume::host::VolumeHost;
use cmdr_fs::volume::host::events::{RecordingVolumeEvents, VolumeConnection, VolumeEventSink};
use cmdr_fs::volume::{Volume, VolumeError};
use tokio_util::sync::CancellationToken;

use super::testing::FIXTURE_SERIAL;
use super::{AdbVolume, ConnectionState, connect_adb_volume};
use crate::params::AdbConnectionParams;
use crate::testing::{FakeAdbServer, FakeTree};

/// A volume on the fake server's device, reporting into `events`.
async fn recorded_volume(server: &FakeAdbServer) -> (Arc<RecordingVolumeEvents>, AdbVolume) {
    let events = Arc::new(RecordingVolumeEvents::new());
    let host = VolumeHost::builder()
        .events(Arc::clone(&events) as Arc<dyn VolumeEventSink>)
        .build();
    let volume = connect_adb_volume(
        AdbConnectionParams::at(FIXTURE_SERIAL, server.endpoint()),
        host,
        CancellationToken::new(),
    )
    .await
    .expect("connect");
    (events, volume)
}

/// The generation moves only when a reconnect's hello brings a device that was
/// down back: ❌ not on a hello that finds it already up, ❌ not on a loss, ❌ not
/// on a hello that didn't take.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn only_a_hello_that_brings_the_device_back_moves_the_generation() {
    let server = FakeAdbServer::start(FakeTree::new()).await;
    let (_events, volume) = recorded_volume(&server).await;
    let first = volume
        .connection_generation()
        .expect("an ADB volume reports its generation");

    volume.attempt_reconnect().await.expect("the device answers");
    assert_eq!(
        volume.connection_generation(),
        Some(first),
        "it was up: nothing came back"
    );

    volume.note_device_gone();
    assert_eq!(volume.connection_generation(), Some(first), "a loss is a state change");
    volume.attempt_reconnect().await.expect("the device answers");
    assert_eq!(volume.connection_generation(), Some(first + 1));

    volume.note_device_gone();
    server.stop();
    assert!(volume.attempt_reconnect().await.is_err(), "the server is gone");
    assert_eq!(volume.connection_generation(), Some(first + 1));
}

/// ❗ **A late loss from before the device came back leaves it connected.**
///
/// An operation that started before a reconnect brought the device back and
/// fails after it is old news. Marking the device gone on it would hand every
/// operation since an outage it never had. Driven through `noting`, the funnel
/// every delegator uses, so the generation is read where production reads it:
/// before the call runs.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_late_loss_from_before_the_device_came_back_leaves_it_connected() {
    let server = FakeAdbServer::start(FakeTree::new()).await;
    let (events, volume) = recorded_volume(&server).await;

    let outcome: Result<(), _> = volume
        .noting(async {
            // While the operation is out, the device drops and a reconnect
            // brings it back.
            volume.note_device_gone();
            volume.attempt_reconnect().await.expect("the device answers");
            Err(VolumeError::DeviceDisconnected("before the way back".into()))
        })
        .await;

    assert!(
        outcome.is_err(),
        "the operation's own answer is passed through untouched"
    );
    assert_eq!(volume.session_state(), ConnectionState::Connected);
    assert_eq!(
        events
            .transitions()
            .into_iter()
            .map(|(_, state)| state)
            .collect::<Vec<_>>(),
        vec![VolumeConnection::Disconnected, VolumeConnection::Connected],
        "the drop and the way back, and nothing from the late error"
    );

    // A loss on the current generation still counts.
    let outcome: Result<(), _> = volume
        .noting(async { Err(VolumeError::ConnectionTimeout("now".into())) })
        .await;
    assert!(outcome.is_err());
    assert_eq!(volume.session_state(), ConnectionState::Disconnected);
}
