//! A server that REALLY goes away under a live volume, and comes back.
//!
//! ❗ The cut happens in a `cmdr_fs::testing::tcp_proxy::TcpProxy` this test
//! owns, between the client and VersityGW. ❌ Never pause or stop the container:
//! the stack is shared by lease with other test binaries, worktrees, and
//! sessions. The same shape as `crates/cmdr-webdav`'s twin.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use cmdr_fs::testing::tcp_proxy::TcpProxy;
use cmdr_fs::testing::wait_until_async;
use cmdr_fs::volume::host::VolumeHost;
use cmdr_fs::volume::host::credentials::InMemoryCredentials;
use cmdr_fs::volume::host::events::{RecordingVolumeEvents, VolumeConnection, VolumeEventSink};
use cmdr_fs::volume::{ConnectionState, Volume, VolumeError};
use tokio_util::sync::CancellationToken;
use url::Url;

use super::testing::*;
use super::{S3Volume, connect_s3_volume};
use crate::params::{S3ConnectionParams, S3Provider};

const FIXTURE: &str = "s3-servers/start.sh (s3-fixture)";

/// How long an operation on a REFUSED connection may take to answer: the real
/// answer is milliseconds; this keeps the failure message ours.
const ANSWERS_WITHIN: Duration = Duration::from_secs(3);

/// The backend's first backoff step (2 s) plus a probe.
const COMES_BACK_WITHIN: Duration = Duration::from_secs(6);

struct Proxied {
    proxy: TcpProxy,
    events: Arc<RecordingVolumeEvents>,
    volume: S3Volume,
}

/// Connects to the `cmdr-test` bucket on VersityGW THROUGH a fresh proxy,
/// secret remembered.
async fn through_a_proxy() -> Proxied {
    let target = SocketAddr::from(([127, 0, 0, 1], fixture_port(VERSITYGW)));
    let proxy = TcpProxy::start(target).await;
    let provider = S3Provider::Other {
        endpoint: Url::parse(&format!("http://127.0.0.1:{}", proxy.port())).expect("a valid URL"),
        region: None,
        path_style: true,
    };
    let params = S3ConnectionParams::new(provider, FIXTURE_ACCESS_KEY, Some(FIXTURE_BUCKET)).expect("valid");
    let events = Arc::new(RecordingVolumeEvents::new());
    let host = VolumeHost::builder()
        .runtime(tokio::runtime::Handle::current())
        .events(Arc::clone(&events) as Arc<dyn VolumeEventSink>)
        .credentials(Arc::new(InMemoryCredentials::new().with_entry(
            &params.credential_service(),
            Some(FIXTURE_ACCESS_KEY),
            FIXTURE_ACCESS_KEY,
            &fixture_secret(),
        )))
        .build();
    let volume = connect_s3_volume(
        "fixture",
        &format!("s3-test-proxied-{}", proxy.port()),
        params,
        host,
        CancellationToken::new(),
    )
    .await
    .unwrap_or_else(|e| panic!("the proxied fixture refused a connection ({e:?}); is the stack up? {FIXTURE}"));
    Proxied { proxy, events, volume }
}

fn reported(events: &RecordingVolumeEvents) -> Vec<VolumeConnection> {
    events.transitions().into_iter().map(|(_, state)| state).collect()
}

/// ❗ **A server that goes away is reported down at once, with a typed error,
/// and comes back on its own once it's reachable again.** There is no watcher,
/// so the first request after the drop is the detector.
#[tokio::test]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn a_server_that_goes_away_is_reported_down_and_comes_back_on_its_own() {
    let Proxied { proxy, events, volume } = through_a_proxy().await;
    volume.list_directory(volume.root(), None).await.expect(FIXTURE);

    proxy.refuse().await;

    let failed = tokio::time::timeout(ANSWERS_WITHIN, volume.list_directory(volume.root(), None))
        .await
        .expect("❗ a request to a refusing server has to answer, not hang");
    assert!(
        matches!(failed, Err(VolumeError::DeviceDisconnected(_))),
        "only DeviceDisconnected flips the state and starts the backoff, got {failed:?}"
    );
    assert_eq!(volume.connection_state(), Some(ConnectionState::Disconnected));
    assert_eq!(reported(&events), vec![VolumeConnection::Disconnected]);

    proxy.restore().await;

    wait_until_async(COMES_BACK_WITHIN, "the backoff loop to bring the volume back", || {
        reported(&events).contains(&VolumeConnection::Connected)
    })
    .await;
    assert_eq!(volume.connection_state(), Some(ConnectionState::Direct));
    volume.list_directory(volume.root(), None).await.expect(FIXTURE);
    assert_eq!(
        reported(&events),
        vec![VolumeConnection::Disconnected, VolumeConnection::Connected],
        "one drop, one recovery"
    );
}

/// ❗ **With "Reconnect automatically" off, a server that comes back is left
/// alone until someone asks.** The proxy counts every connection it accepts,
/// so "nothing probed" is a number.
#[tokio::test]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn with_the_switch_off_a_returning_server_stays_down_until_asked() {
    let Proxied { proxy, events, volume } = through_a_proxy().await;
    volume.set_auto_reconnect(false);

    proxy.refuse().await;
    let failed = tokio::time::timeout(ANSWERS_WITHIN, volume.list_directory(volume.root(), None))
        .await
        .expect("❗ a request to a refusing server has to answer, not hang");
    assert!(matches!(failed, Err(VolumeError::DeviceDisconnected(_))), "{failed:?}");
    proxy.restore().await;
    let connections_before = proxy.connections_accepted();

    // allowed-test-sleep: a negative assertion over a window. Nothing can signal "no probe happened"; the window runs
    // past the backoff's first 2 s step, which is when a loop that shouldn't exist would have probed.
    tokio::time::sleep(Duration::from_millis(2_500)).await;

    assert_eq!(
        proxy.connections_accepted(),
        connections_before,
        "❌ nothing may probe unattended with the switch off"
    );
    assert!(matches!(
        volume.attempt_reconnect().await,
        Err(VolumeError::NotSupported)
    ));

    volume
        .reconnect_with_credentials(FIXTURE_ACCESS_KEY.to_string(), fixture_secret())
        .await
        .expect(FIXTURE);
    volume.list_directory(volume.root(), None).await.expect(FIXTURE);
    assert_eq!(
        reported(&events),
        vec![VolumeConnection::Disconnected, VolumeConnection::Connected]
    );
}
