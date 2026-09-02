//! Which devices the server sees, and a push channel for when that changes.
//!
//! `host:devices-l` answers one line per device: `serial<spaces>state` then
//! optional `product:x model:y device:z transport_id:n` fields.
//! `host:track-devices` pushes the SHORT format (`serial\tstate`) on every
//! change for the life of the socket; [`track_devices`] refetches the long
//! format on each push so listeners always see the rich fields.

use std::sync::Arc;
use std::time::Duration;

use log::{debug, warn};
use serde::{Deserialize, Serialize};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::errors::AdbConnectError;
use crate::server::AdbEndpoint;

/// One device the server knows about.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AdbDevice {
    /// The serial (`host:devices` column one). Identity of the volume.
    pub serial: String,
    /// What the server can do with it. Only [`AdbDeviceState::Ready`] mounts.
    pub state: AdbDeviceState,
    /// `ro.product.name`, when the server reports it.
    pub product: Option<String>,
    /// `ro.product.model`, when the server reports it. Underscores for spaces.
    pub model: Option<String>,
    /// `ro.product.device`, when the server reports it.
    pub device: Option<String>,
    /// The server's transport id, for telling two identical serials apart.
    pub transport_id: Option<u32>,
}

/// The server's state word for a device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum AdbDeviceState {
    /// `device`: authorized and online. The only mountable state.
    Ready,
    /// `unauthorized`: waiting on the phone's "Allow USB debugging" prompt.
    Unauthorized,
    /// `offline`: attached but `adbd` isn't answering.
    Offline,
    /// `no permissions`: the host can't open the USB device (udev on Linux).
    NoPermissions,
    /// `connecting`: a TCP device mid-handshake.
    Connecting,
    /// `authorizing`: mid RSA handshake.
    Authorizing,
    /// `recovery`: booted to recovery.
    Recovery,
    /// `bootloader`: in fastboot.
    Bootloader,
    /// `sideload`: recovery's sideload mode.
    Sideload,
    /// A word this crate doesn't know.
    Unknown,
}

impl AdbDeviceState {
    /// Parses the server's state word. `no permissions` carries extra text on
    /// some servers (`no permissions (user in plugdev group?)`), so the match is
    /// on the prefix.
    pub fn parse(word: &str) -> Self {
        match word {
            "device" => Self::Ready,
            "unauthorized" => Self::Unauthorized,
            "offline" => Self::Offline,
            "connecting" => Self::Connecting,
            "authorizing" => Self::Authorizing,
            "recovery" => Self::Recovery,
            "bootloader" => Self::Bootloader,
            "sideload" => Self::Sideload,
            w if w.starts_with("no permissions") => Self::NoPermissions,
            _ => Self::Unknown,
        }
    }

    /// The server's word for this state, for the fake server and logs.
    pub fn as_word(self) -> &'static str {
        match self {
            Self::Ready => "device",
            Self::Unauthorized => "unauthorized",
            Self::Offline => "offline",
            Self::NoPermissions => "no permissions",
            Self::Connecting => "connecting",
            Self::Authorizing => "authorizing",
            Self::Recovery => "recovery",
            Self::Bootloader => "bootloader",
            Self::Sideload => "sideload",
            Self::Unknown => "unknown",
        }
    }
}

impl AdbDevice {
    /// What to call this device: the model with underscores as spaces, else
    /// the product, else the serial.
    pub fn display_name(&self) -> String {
        if let Some(model) = self.model.as_deref().filter(|m| !m.is_empty()) {
            return model.replace('_', " ");
        }
        if let Some(product) = self.product.as_deref().filter(|p| !p.is_empty()) {
            return product.to_string();
        }
        self.serial.clone()
    }

    /// Whether the device can be mounted right now.
    pub fn is_ready(&self) -> bool {
        self.state == AdbDeviceState::Ready
    }
}

/// Parses a `host:devices-l` (or `host:devices`) payload.
///
/// The long format pads the serial with spaces to a column, then the state,
/// then `key:value` fields; the short format is `serial\tstate`. `no
/// permissions` is two words, so the state is everything up to the first
/// `key:value` field rather than the second token.
pub(crate) fn parse_device_list(payload: &str) -> Vec<AdbDevice> {
    payload.lines().filter_map(parse_device_line).collect()
}

fn parse_device_line(line: &str) -> Option<AdbDevice> {
    let line = line.trim_end();
    if line.is_empty() {
        return None;
    }
    let (serial, rest) = line.split_once(|c: char| c.is_whitespace())?;
    let rest = rest.trim_start();
    let mut state_words = Vec::new();
    let mut device = AdbDevice {
        serial: serial.to_string(),
        state: AdbDeviceState::Unknown,
        product: None,
        model: None,
        device: None,
        transport_id: None,
    };
    for token in rest.split_whitespace() {
        match token.split_once(':') {
            Some(("product", v)) => device.product = Some(v.to_string()),
            Some(("model", v)) => device.model = Some(v.to_string()),
            Some(("device", v)) => device.device = Some(v.to_string()),
            Some(("transport_id", v)) => device.transport_id = v.parse().ok(),
            Some(_) => {}
            None => state_words.push(token),
        }
    }
    device.state = AdbDeviceState::parse(&state_words.join(" "));
    Some(device)
}

/// Asks the server for every device it sees, with the long-format fields.
pub async fn list_devices(endpoint: &AdbEndpoint) -> Result<Vec<AdbDevice>, AdbConnectError> {
    let mut conn = endpoint.connect().await?;
    conn.request("host:devices-l").await?;
    let payload = conn.read_hex_message().await?;
    conn.shutdown().await;
    Ok(parse_device_list(&String::from_utf8_lossy(&payload)))
}

/// Tuning for the tracker's reconnect loop. Tests shorten it.
#[derive(Debug, Clone, Copy)]
pub(crate) struct TrackerBackoff {
    /// The first wait after a drop.
    pub initial: Duration,
    /// Doubling stops here.
    pub cap: Duration,
}

impl Default for TrackerBackoff {
    fn default() -> Self {
        Self {
            initial: Duration::from_secs(1),
            cap: Duration::from_secs(15),
        }
    }
}

/// A running `host:track-devices` subscription. Stops on [`DeviceTracker::stop`]
/// or drop.
pub struct DeviceTracker {
    task: JoinHandle<()>,
    cancel: CancellationToken,
}

impl std::fmt::Debug for DeviceTracker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeviceTracker").finish_non_exhaustive()
    }
}

impl DeviceTracker {
    /// Ends the subscription. Idempotent; the task is aborted, so no callback
    /// runs after this returns to a caller on another thread only if it wasn't
    /// already mid-call.
    pub fn stop(&self) {
        self.cancel.cancel();
        self.task.abort();
    }
}

impl Drop for DeviceTracker {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Subscribes to `host:track-devices` on `runtime`.
///
/// `on_change` gets the FULL parsed list (refetched through `host:devices-l`,
/// since a push carries only serial and state) on the first answer and on every
/// change after. When the socket drops, the tracker reconnects with backoff
/// (1 s, 2 s, … capped at 15 s), and each successful reconnect delivers the
/// list again so a listener that missed a change while the server was away
/// catches up.
pub fn track_devices(
    endpoint: AdbEndpoint,
    runtime: tokio::runtime::Handle,
    on_change: Arc<dyn Fn(Vec<AdbDevice>) + Send + Sync>,
) -> DeviceTracker {
    track_devices_with(endpoint, runtime, on_change, TrackerBackoff::default())
}

pub(crate) fn track_devices_with(
    endpoint: AdbEndpoint,
    runtime: tokio::runtime::Handle,
    on_change: Arc<dyn Fn(Vec<AdbDevice>) + Send + Sync>,
    backoff: TrackerBackoff,
) -> DeviceTracker {
    let cancel = CancellationToken::new();
    let token = cancel.clone();
    let task = runtime.spawn(async move {
        let mut wait = backoff.initial;
        loop {
            let session = token.run_until_cancelled(track_once(&endpoint, &on_change));
            match session.await {
                None => return,
                Some(Ok(())) => {
                    debug!("adb track-devices socket closed; reconnecting");
                    wait = backoff.initial;
                }
                Some(Err(err)) => {
                    warn!("adb track-devices dropped ({err}); retrying in {wait:?}");
                }
            }
            if token.run_until_cancelled(tokio::time::sleep(wait)).await.is_none() {
                return;
            }
            wait = (wait * 2).min(backoff.cap);
        }
    });
    DeviceTracker { task, cancel }
}

/// One `host:track-devices` session: `Ok(())` when the server closed the
/// socket cleanly, `Err` when anything else ended it.
async fn track_once(
    endpoint: &AdbEndpoint,
    on_change: &Arc<dyn Fn(Vec<AdbDevice>) + Send + Sync>,
) -> Result<(), AdbConnectError> {
    let mut conn = endpoint.connect().await?;
    conn.request("host:track-devices").await?;
    loop {
        let push = match conn.read_hex_message().await {
            Ok(push) => push,
            Err(crate::errors::AdbError::DeviceGone) => return Ok(()),
            Err(err) => return Err(err.into()),
        };
        let short = parse_device_list(&String::from_utf8_lossy(&push));
        let list = match list_devices(endpoint).await {
            Ok(list) => list,
            Err(err) => {
                debug!("adb devices-l after a push refused ({err}); using the short list");
                short
            }
        };
        on_change(list);
    }
}

#[cfg(test)]
#[path = "devices_test.rs"]
mod devices_test;
