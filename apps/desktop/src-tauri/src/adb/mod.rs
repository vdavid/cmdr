//! Android over ADB: the app side of `cmdr_adb`.
//!
//! The crate owns the wire and the `Volume`; this module owns what only the
//! app can: the cached device list the ADB server pushes
//! (`host:track-devices`), the connected volumes and their registration, the
//! `DeviceVolumeProvider` the volume list folds over, and the IPC commands.
//! Same split as `mtp/` and `network/sftp_volume_wiring.rs`.
//!
//! Module map: `device_provider.rs` (the provider and the cached state),
//! `volume_wiring.rs` (connect, register, and the tracker), `commands.rs`
//! (IPC pass-throughs). `DETAILS.md` has the flows.

pub mod commands;
pub mod device_provider;
pub mod volume_wiring;

pub use volume_wiring::start_adb_tracker;
