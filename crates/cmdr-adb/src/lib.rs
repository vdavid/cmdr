#![warn(unused_crate_dependencies)]
#![deny(missing_docs)]

//! Everything Cmdr says to an Android device over ADB.
//!
//! The device-side twin of `cmdr-sftp`: one `Volume` per connected device,
//! rooted at the device's real filesystem (`/`) rather than at the curated
//! object tree MTP exposes. Nothing here knows about `tauri`, and nothing here
//! holds a user-facing word — the host renders every sentence from the typed
//! values this crate returns.
//!
//! ## The shape of the thing
//!
//! Cmdr talks to the **ADB server** (the `adb` daemon on `127.0.0.1:5037`), not
//! to the device directly: the server owns USB, multiplexes clients, and is
//! already running on any machine with the platform tools installed. Every
//! module below is one layer of that conversation:
//!
//! - `server` finds the endpoint and, once, starts an absent server.
//! - `transport` is the ONLY module that speaks the wire framing.
//! - `devices` lists attached devices and — the reason this backend can do what
//!   MTP can't — streams `host:track-devices` for hotplug.
//! - `features` records what a device's own ADB supports, read once per session.
//! - `sync` is the file-transfer service (`STAT`/`LIST`/`RECV`/`SEND`).
//! - `shell` is everything the sync service has no verb for: mkdir, rm, mv, df.
//! - `volume` is the `Volume` impl assembled from those.

#[cfg(test)]
use cmdr_adb as _;

pub(crate) mod devices;
pub(crate) mod errors;
pub(crate) mod features;
pub(crate) mod params;
pub mod server;
pub mod shell;
pub mod sync;
pub mod transport;
pub mod volume;

#[cfg(any(test, feature = "testing"))]
pub mod testing;

pub use devices::{AdbDevice, AdbDeviceState, DeviceTracker, list_devices, track_devices};
pub use errors::{AdbConnectError, AdbError, volume_error_from_adb, volume_error_from_errno};
pub use features::DeviceFeatures;
pub use params::AdbConnectionParams;
pub use server::AdbEndpoint;
pub use volume::{AdbVolume, connect_adb_volume};
