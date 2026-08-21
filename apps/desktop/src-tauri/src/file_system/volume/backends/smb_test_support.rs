//! What the app-side SMB suites reach the fixture containers through.
//!
//! The fixtures themselves are `cmdr_smb::volume::testing`, shared with the
//! backend's own suites so both sides seed, clean up, and hash the same way. The
//! one thing that differs across the seam is the host: a backend test wants a
//! `VolumeHost` that answers nothing, and an app test wants the real wiring, so
//! the listing cache, the index, and the activity tracker see what the share
//! reports.

pub(super) use cmdr_smb::volume::testing::*;

// The vocabulary every SMB suite speaks, re-exported so one glob covers it. The
// backend's own `mod.rs` used to hold this prelude and reach the suites through
// `use super::*`; on this side of the seam `super` is a re-export of the crate,
// so the shared half lives here instead of being spelled out eight times.
pub(super) use std::path::{Path, PathBuf};
pub(super) use std::sync::Arc;
pub(super) use std::sync::atomic::Ordering;
pub(super) use std::time::Duration;

pub(super) use cmdr_fs::volume::{Volume, VolumeError, VolumeReadStream, WatchCoverage};

use super::SmbVolume;

/// Connects to the fixture container's `public` share, wired to THIS app.
///
/// Shadows the crate's detached-host builder of the same name, so a suite on
/// this side gets the app's wiring by writing what it always wrote.
pub(super) async fn make_docker_volume() -> SmbVolume {
    make_docker_volume_with_host(crate::volume_host::host()).await
}
