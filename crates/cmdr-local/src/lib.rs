// The lint set this crate is held to lives in the workspace root's
// `[workspace.lints]`, opted into by `Cargo.toml`'s `lints.workspace = true`.
// `unused_crate_dependencies` can't go with them: it's judged per compilation
// unit, so it belongs on the lib alone. `missing_docs` and `unreachable_pub` are
// this crate's own contract, as in every crate whose surface is capped.
#![warn(unused_crate_dependencies)]
#![warn(unreachable_pub)]
#![deny(missing_docs)]

//! The local disk: [`LocalPosixVolume`], the `Volume` over a `std::fs` root, and
//! the OS probes it and the app read local disks through.
//!
//! A storage backend as its own crate, like every other one. Everything it can't
//! answer itself (the pane listings it patches after a mutation, the watch
//! coverage it reports, the name order its listings come back in) arrives through
//! the [`VolumeHost`](cmdr_fs::volume::host::VolumeHost) it's constructed with. It
//! never registers itself and never names the app around it.
//!
//! Beside the backend:
//!
//! - [`reading`]: the local reader (`getattrlistbulk` on APFS and HFS+, one
//!   `read_dir` and an `lstat` per entry elsewhere), which the app's watcher and
//!   tag code read through too, and [`mutation`],
//!   what a local mutation means for the listing cache.
//! - The no-replace rename, [`rename_local_exclusive`]: the one in the app.
//! - [`space`] and `live_space`: how much room a local volume has.
//! - The mount probes: [`filesystem_kind`] (the filesystem behind a path), plus
//!   `mount_table` and `statfs` on macOS and `linux_mounts` on Linux.
//! - `nsurl` (macOS): reading one NSURL resource value.
//!
//! See `CLAUDE.md` for the must-knows and `DETAILS.md` for the reader, space, and
//! the rename's degradation.

//noinspection RsUnusedImport
// We dev-depend on ourselves so the `testing` feature is on for dev targets and
// off for the lib (see `Cargo.toml`). That makes `cmdr_local` an extern crate of
// its own test target, which `unused_crate_dependencies` reports.
#[cfg(test)]
use cmdr_local as _;
//noinspection RsUnusedImport
// Read by one space cell, which only the macOS test build compiles.
#[cfg(all(test, target_os = "linux"))]
use dirs as _;

/// Whether this build takes the test fast paths: space from `statvfs` rather than
/// NSURL (#357), and a 1 ms listing-progress sampler.
///
/// On for this crate's own tests and for a consumer's test build (through
/// `testing`), but never in an optimized build: Cargo compiles a consumer's
/// Criterion bench with its dev-dependencies, so `testing` is on there too, and the
/// `bench` profile (no `debug_assertions`) has to measure what ships. A cargo
/// feature can't make that cut, since one package's tests and benches share one
/// dev-dependency feature set. DETAILS § "A consumer's test build is a test build
/// here too".
const TEST_FAST_PATHS: bool = cfg!(any(test, all(feature = "testing", debug_assertions)));

pub mod filesystem_kind;
#[cfg(target_os = "linux")]
pub mod linux_mounts;
#[cfg(target_os = "macos")]
pub mod live_space;
#[cfg(target_os = "macos")]
pub mod mount_table;
pub mod mutation;
#[cfg(target_os = "macos")]
pub mod nsurl;
pub mod reading;
pub mod space;
#[cfg(target_os = "macos")]
pub mod statfs;

mod local_posix;

pub use local_posix::{LocalPosixVolume, rename_local_exclusive, rename_volume_error};

#[cfg(test)]
mod local_posix_conformance_test;
#[cfg(test)]
mod local_posix_link_test;
#[cfg(test)]
mod local_posix_resume_test;
#[cfg(test)]
mod local_posix_test;
#[cfg(test)]
mod reading_parity_test;
#[cfg(test)]
mod reading_test;
