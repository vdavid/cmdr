//! The ADB backend: a `Volume` over one Android device, reached through the ADB
//! server's sync service and its shell.
//!
//! There is no OS mount under this and never will be: every listing, read, and
//! write rides a socket to the ADB server, which relays it to the device's
//! `adbd`. The volume is rooted at the device's own `/`, so the paths it hands
//! out ARE device paths and nothing has to be translated between two spellings
//! of the same tree.
//!
//! Nothing here names the application. What the backend needs from it arrives
//! through the [`VolumeHost`] seams handed to [`connect_adb_volume`].
//!
//! ## One socket per operation, no shared session
//!
//! Every sync operation opens its OWN `sync:` socket and closes it when done,
//! rather than queueing on one shared session behind a mutex. Two reasons:
//!
//! - A shared session serialized behind a lock deadlocks a same-volume copy: the
//!   source stream holds the session while the destination write waits for it.
//! - A paused transfer parks its read stream mid-file; on a shared session that
//!   parks every listing on the volume with it, and the pane can't navigate.
//!
//! The ADB server multiplexes sockets to the device, and `adbd` serializes the
//! I/O on its side anyway, so nothing is gained by holding one socket. What
//! bounds concurrent TRANSFERS is `max_concurrent_ops`, which the app's settings
//! table answers with 1 for this backend.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8};
use std::time::Duration;

use cmdr_fs::volume::host::VolumeHost;
use cmdr_fs::volume::host::settings::BackendName;
use cmdr_fs::volume::{Retirement, VolumeError, adb_volume_id};
use tokio_util::sync::CancellationToken;

use crate::devices::{AdbDeviceState, list_devices};
use crate::errors::{AdbConnectError, AdbError};
use crate::features::DeviceFeatures;
use crate::params::AdbConnectionParams;
use crate::server::AdbEndpoint;
use crate::sync::SyncSession;

mod mapping;
mod mutation;
mod paths;
mod query;
mod scan;
mod state;
mod streams;
mod volume_impl;
mod writes;

pub use state::ConnectionState;

#[cfg(any(test, feature = "testing"))]
pub mod testing;

/// This backend's settings namespace, for everything it reads through
/// [`VolumeHost::settings`]. A namespace, not a classification: nothing branches
/// on it, and the app resolves it through a table.
const BACKEND: BackendName = "adb";

/// How long a connect may take, all phases together: the device list, the
/// feature probe, and the hello. A device that answers none of them inside this
/// is not going to.
const CONNECT_BUDGET: Duration = Duration::from_secs(10);

/// A volume backed by one Android device over ADB.
pub struct AdbVolume {
    /// Display name: the device's model as `list_devices` reported it.
    name: String,
    /// Always the device's `/`. A plain field so `root()` stays a borrow.
    root: PathBuf,
    inner: Arc<AdbVolumeInner>,
}

/// The device-scoped half: what every operation on this volume shares.
struct AdbVolumeInner {
    /// From `adb_volume_id(serial)`, the key every piece of durable per-volume
    /// state is filed under.
    volume_id: String,
    /// The device this volume is, as the ADB server names it.
    serial: String,
    /// The ADB server every socket is opened against.
    endpoint: AdbEndpoint,
    /// What the device's own ADB supports, read once at connect. ❌ Never
    /// re-probed at a call site: `adbd` doesn't change under a session.
    features: DeviceFeatures,
    /// The state the host was last told about, so a device that is gone doesn't
    /// produce one event per failing operation (`state.rs`).
    state: AtomicU8,
    /// Whether the registry still serves this volume under its id.
    retirement: Retirement,
    /// Set by `on_unmount`, so a reconnect in flight bails rather than reviving
    /// a volume the app has forgotten.
    unmounted: AtomicBool,
    /// Single-flight around a hello re-run.
    reconnect_lock: tokio::sync::Mutex<()>,
    /// Everything this backend asks the app around it.
    host: VolumeHost,
}

impl AdbVolume {
    /// The volume id every listing-cache lookup and connection event uses.
    pub fn volume_id(&self) -> &str {
        &self.inner.volume_id
    }

    /// The device's serial, as the ADB server names it.
    pub fn serial(&self) -> &str {
        &self.inner.serial
    }

    /// The device's display name, as the sidebar labels it.
    pub fn device_name(&self) -> &str {
        &self.name
    }

    /// What the device's own ADB supports.
    pub fn features(&self) -> DeviceFeatures {
        self.inner.features
    }

    /// Where the connection stands right now.
    pub fn connection_state(&self) -> ConnectionState {
        self.inner.connection_state()
    }

    /// Opens a fresh sync socket to the device.
    ///
    /// One per operation, and it closes with the operation: `mod.rs` § "One
    /// socket per operation" has why there is no shared session to reach for.
    pub(super) async fn open_sync(&self, path: &str) -> Result<SyncSession, VolumeError> {
        self.inner.open_sync(path).await
    }
}

impl AdbVolumeInner {
    async fn open_sync(&self, path: &str) -> Result<SyncSession, VolumeError> {
        if self.unmounted.load(std::sync::atomic::Ordering::Relaxed) {
            return Err(VolumeError::DeviceDisconnected(self.volume_id.clone()));
        }
        SyncSession::open(&self.endpoint, &self.serial, self.features)
            .await
            .map_err(|e| self.map_adb_error(e, path))
    }

    /// The crate's error mapper, with the one thing it can't know: which volume
    /// a `DeviceGone` describes.
    pub(super) fn map_adb_error(&self, error: AdbError, path: &str) -> VolumeError {
        match error {
            AdbError::DeviceGone => VolumeError::DeviceDisconnected(self.volume_id.clone()),
            other => crate::errors::volume_error_from_adb(other, path),
        }
    }

    /// The hello: one `stat` of the root, which proves the device answers sync
    /// requests at all. Shared by the connect and every reconnect.
    async fn hello(&self) -> Result<(), AdbError> {
        let mut session = SyncSession::open(&self.endpoint, &self.serial, self.features).await?;
        let stat = session.stat("/").await?;
        session.quit().await;
        if !stat.exists() {
            return Err(AdbError::Protocol(
                "the device answered the hello with a missing root".to_string(),
            ));
        }
        Ok(())
    }
}

/// Opens an ADB volume for the device with `params.serial`.
///
/// Four phases, all under one [`CONNECT_BUDGET`] and `cancel`: list the
/// devices (a serial that isn't there is `DeviceGone`; one the user hasn't
/// authorized yet is `Unauthorized`), read the device's features (no
/// `shell_v2` is a device too old to give exit codes, and every write verb
/// here depends on those), then the hello. A cancel ends the attempt where it
/// stands and leaves nothing behind: no volume, no socket.
pub async fn connect_adb_volume(
    params: AdbConnectionParams,
    host: VolumeHost,
    cancel: CancellationToken,
) -> Result<AdbVolume, AdbConnectError> {
    let attempt = async {
        let devices = list_devices(&params.endpoint).await?;
        let device = devices
            .into_iter()
            .find(|d| d.serial == params.serial)
            .ok_or_else(|| AdbConnectError::DeviceGone(params.serial.clone()))?;
        match device.state {
            AdbDeviceState::Ready => {}
            AdbDeviceState::Unauthorized => return Err(AdbConnectError::Unauthorized(params.serial.clone())),
            _ => return Err(AdbConnectError::DeviceGone(params.serial.clone())),
        }
        let features = DeviceFeatures::fetch(&params.endpoint, &params.serial)
            .await
            .map_err(|e| AdbConnectError::from(e).for_device(&params.serial))?;
        if !features.shell_v2 {
            return Err(AdbConnectError::DeviceTooOld {
                serial: params.serial.clone(),
            });
        }
        Ok((device.display_name(), features))
    };

    let (name, features) = tokio::select! {
        biased;
        () = cancel.cancelled() => return Err(AdbConnectError::Cancelled),
        outcome = tokio::time::timeout(CONNECT_BUDGET, attempt) => match outcome {
            Ok(outcome) => outcome?,
            Err(_) => return Err(AdbConnectError::TimedOut),
        },
    };

    let inner = Arc::new(AdbVolumeInner {
        volume_id: adb_volume_id(&params.serial),
        serial: params.serial.clone(),
        endpoint: params.endpoint.clone(),
        features,
        state: AtomicU8::new(ConnectionState::Connected as u8),
        retirement: Retirement::new(),
        unmounted: AtomicBool::new(false),
        reconnect_lock: tokio::sync::Mutex::new(()),
        host,
    });

    tokio::select! {
        biased;
        () = cancel.cancelled() => return Err(AdbConnectError::Cancelled),
        outcome = tokio::time::timeout(CONNECT_BUDGET, inner.hello()) => match outcome {
            Ok(Ok(())) => {}
            Ok(Err(e)) => return Err(AdbConnectError::from(e).for_device(&params.serial)),
            Err(_) => return Err(AdbConnectError::TimedOut),
        },
    }

    // PII-free: an ADB volume came up. ❌ No serial, model, or path crosses.
    inner.host.analytics().record("adb_connected", &[]);
    Ok(AdbVolume {
        name,
        root: PathBuf::from("/"),
        inner,
    })
}

#[cfg(test)]
mod conformance_test;
#[cfg(test)]
mod volume_impl_test;
