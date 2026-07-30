//! Volume wiring: the backends, the registry, and teardown.
//!
//! The `Volume` trait itself, its sub-traits, the data types it exchanges, the
//! volume ID helpers, and the typed error classification all live in
//! `cmdr_fs::volume` — the index speaks in them and must not need the app. They
//! are re-exported here, so callers keep importing `volume::Volume`,
//! `volume::VolumeError`, `volume::smb_volume_id`, and so on unchanged.
//!
//! What can't move sits below: the real-storage backends (`backends`, with their
//! `smb2` / `mtp-rs` / git / mount-detection dependencies), the process-wide
//! `manager` registry, and macOS/Linux `eject`.

pub use cmdr_fs::volume::*;

// Per-backend `Volume` implementations live in `backends/`. The trait surface
// stays here; submodule names are re-exported below so external callers keep
// importing `volume::LocalPosixVolume`, `volume::MtpVolume`, etc. without
// caring about the `backends/` split.
pub mod backends;
// Volume teardown (USB/SD/DMG/SMB/MTP), used only by the macOS+Linux eject
// command. The macOS-vs-Linux difference (diskutil vs umount, NSURL vs
// `/sys/block`) lives inside via per-fn `#[cfg]`.
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub mod eject;
pub(crate) mod manager;

pub use backends::LocalPosixVolume;
pub(crate) use backends::rename_local_exclusive;
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub use backends::{MtpVolume, SmbVolume};

// `smb` is re-exported as a module path because callers reach into it for
// `SmbConnectionParams` / `connect_smb_volume` / `set_app_handle`.
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub use backends::smb;

#[cfg(test)]
mod inmemory_test;
#[cfg(all(test, any(target_os = "macos", target_os = "linux")))]
mod mtp_scan_oracle_tests;
#[cfg(all(test, any(target_os = "macos", target_os = "linux")))]
mod smb_scan_oracle_tests;
