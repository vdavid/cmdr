//! Android over ADB: the app side of `cmdr_adb`.
//!
//! The crate owns the wire and the `Volume`; this module owns what only the
//! app can: the cached device list the ADB server pushes
//! (`host:track-devices`), the connected volumes and their registration, the
//! `DeviceVolumeProvider` the volume list folds over, and the IPC commands.
//! Same split as `mtp/` and `network/sftp_volume_wiring.rs`.
//!
//! Module map: `device_provider.rs` (the provider and the cached state),
//! `volume_wiring.rs` (connect, register, the tracker, and the two settings),
//! `commands.rs` (IPC pass-throughs). `DETAILS.md` has the flows.

pub mod commands;
pub mod device_provider;
pub mod volume_wiring;

pub use volume_wiring::{set_adb_binary_path, start_adb_tracker};

// The fixtures the app-side ADB suites share, the transfer suite in
// `write_operations` included.
#[cfg(test)]
pub(crate) mod test_support;
// A real tracker's pushes through the provider: retirement and row readiness.
#[cfg(test)]
mod tracker_test;
// Eject's round trip: `volume/eject/` to the provider, the row left behind, a fresh dial.
#[cfg(test)]
mod eject_test;
