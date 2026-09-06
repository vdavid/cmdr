// The lint set this crate is held to lives in the workspace root's
// `[workspace.lints]`, opted into by `Cargo.toml`'s `lints.workspace = true`.
// These two can't go with them: `unused_crate_dependencies` is judged per
// compilation unit (as a package-wide flag every test target would report unused
// externs for deps only the lib uses), and `missing_docs` is this crate's own
// contract — its API is a deliverable rather than a side effect.
#![warn(unused_crate_dependencies)]
#![deny(missing_docs)]

//! Everything Cmdr says to an Android phone or a camera over MTP.
//!
//! The device backend with the flakiest hardware under it, and the reason this
//! is a crate: `cargo check -p cmdr-mtp --all-targets` verifies the whole
//! session layer with no app in the graph, so a sideways reach into the app is a
//! compile error rather than a convention. Nothing here names `tauri`, and
//! nothing here holds a user-facing word — the host renders every sentence from
//! the typed values this crate returns.
//!
//! ## The shape of the thing
//!
//! A phone is not a filesystem. MTP exposes an object tree addressed by opaque
//! 32-bit handles over a single-threaded PTP session, and that session is the
//! scarce resource everything here is arranged around:
//!
//! - [`list_mtp_devices`] enumerates without opening anything, and
//!   [`watch_devices`] is the USB hotplug stream the app's watcher task drives.
//! - `connection` is the session layer: one [`MtpConnectionManager`] owning a
//!   PTP session per device, the path↔handle caches, the per-device priority
//!   gate, the interrupt-endpoint event loop, and the session-reset recovery.
//! - [`volume`] is the `Volume` impl over one storage area, assembled from that.
//! - [`virtual_device`] registers a fixture-backed fake phone, which is how any
//!   of the above is testable at all.
//!
//! ## Two things a caller has to know
//!
//! **The manager is a value, not a singleton.** [`MtpConnectionManager::new`]
//! takes the host, the event sink, and the registrar; the app parks the one it
//! built, and a test builds its own with fakes. Every [`MtpVolume`] carries the
//! manager that attached it.
//!
//! **Dropping a future here has a physical consequence.** An abandoned PTP
//! transaction leaves the phone waiting for bytes nobody will send, and it stays
//! wedged until the user unplugs it. `mtp-rs` bounds each USB transfer itself
//! and fails cleanly, so there is never an outer wall-clock timeout to add; the
//! `mtp-dropping-timeout` check enforces that over this tree.
//!
//! See `CLAUDE.md` for the must-knows and `DETAILS.md` for the boundary's
//! rationale and the capped surface.

//noinspection RsUnusedImport
// We dev-depend on ourselves so the `testing` feature is on for dev targets and
// off for the lib (see `Cargo.toml`). That makes `cmdr_mtp` an extern crate of
// its own test target, which `unused_crate_dependencies` reports.
#[cfg(test)]
use cmdr_mtp as _;

//noinspection RsUnusedImport
// `testing` is what pulls `tempfile` in, but its only user (`VirtualDeviceFixture`)
// also needs `virtual-device`, so `testing` alone links a crate nothing names and
// `unused_crate_dependencies` reports it. Cargo can't express "this feature needs
// that dep only alongside another one", so the reference is stated here instead.
#[cfg(feature = "testing")]
use tempfile as _;

mod connection;
mod discovery;
/// What a test drives a virtual device with, on either side of the boundary.
///
/// Behind `testing` (and `virtual-device`, since there's nothing to connect to
/// without it), so a shipped build carries none of it. ❌ Never enable either in
/// production.
#[cfg(all(feature = "virtual-device", any(test, feature = "testing")))]
pub mod testing;
mod types;
pub mod volume;

/// A fixture-backed fake phone, registered in-process.
///
/// Behind the `virtual-device` feature, which forwards to `mtp-rs`'s own. The
/// app turns it on as `virtual-mtp` for the Playwright lane and for a
/// `CMDR_VIRTUAL_MTP=1` dev session. ❌ Never in a production build.
#[cfg(feature = "virtual-device")]
pub mod virtual_device;

pub use connection::{
    ConnectedDeviceInfo, DeviceWatch, MtpConnectionError, MtpConnectionManager, MtpDeleteScope, MtpDeviceEvent,
    MtpDeviceEvents, MtpDisconnectReason, MtpObjectInfo, MtpVolumeRegistrar, ResolvedMtpObject, no_device_events,
};
pub use discovery::{HotplugEvent, list_mtp_devices, watch_devices};
pub use types::{MtpDeviceInfo, MtpStorageInfo};
pub use volume::MtpVolume;

/// An [`MtpDeviceEvents`] that remembers what it was told, so a test can assert
/// on the lifecycle sequence a user would have seen.
///
/// ❌ Not `cfg(test)` alone: that's set only while a crate compiles its OWN test
/// target, so a consumer's test build would see it vanish.
#[cfg(any(test, feature = "testing"))]
pub use connection::RecordingMtpDeviceEvents;
