//! Fixtures for the ADB volume suites, on both sides of the crate boundary.
//!
//! Gated behind the `testing` feature, so it exists in dev targets and in no
//! shipped build. The fake server itself is `crate::testing::FakeAdbServer`.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8};

use cmdr_fs::volume::host::VolumeHost;
use cmdr_fs::volume::host::listings::RecordingListings;
use cmdr_fs::volume::{Retirement, adb_volume_id};
use tokio_util::sync::CancellationToken;

use super::{AdbVolume, AdbVolumeInner, ConnectionState, connect_adb_volume};
use crate::features::DeviceFeatures;
use crate::params::AdbConnectionParams;
use crate::server::AdbEndpoint;
use crate::testing::FakeAdbServer;

/// The serial every fixture device answers to: the fake server's own.
pub const FIXTURE_SERIAL: &str = crate::testing::FAKE_SERIAL;

/// A volume with no server behind it, for the pure paths (path translation,
/// capability answers). Every wire-touching call on it fails.
pub fn detached_volume() -> AdbVolume {
    AdbVolume {
        name: "Fixture phone".to_string(),
        root: PathBuf::from("/"),
        inner: Arc::new(AdbVolumeInner {
            volume_id: adb_volume_id(FIXTURE_SERIAL),
            serial: FIXTURE_SERIAL.to_string(),
            endpoint: AdbEndpoint::at(std::net::SocketAddr::from(([127, 0, 0, 1], 1))),
            features: DeviceFeatures::all(),
            state: AtomicU8::new(ConnectionState::Connected as u8),
            retirement: Retirement::new(),
            unmounted: AtomicBool::new(false),
            reconnect_lock: tokio::sync::Mutex::new(()),
            host: VolumeHost::detached(),
        }),
    }
}

/// Connects to the fake server's device `serial` through a host that records
/// what the volume tells its panes.
pub async fn connect_fake(server: &FakeAdbServer, serial: &str) -> (Arc<AdbVolume>, Arc<RecordingListings>) {
    let listings = Arc::new(RecordingListings::new());
    let host = VolumeHost::builder()
        .listings(Arc::clone(&listings) as Arc<dyn cmdr_fs::volume::host::listings::ListingHost>)
        .build();
    let volume = connect_adb_volume(
        AdbConnectionParams::at(serial, server.endpoint()),
        host,
        CancellationToken::new(),
    )
    .await
    .unwrap_or_else(|e| panic!("connecting to the fake server's {serial} must work, got {e:?}"));
    (Arc::new(volume), listings)
}
