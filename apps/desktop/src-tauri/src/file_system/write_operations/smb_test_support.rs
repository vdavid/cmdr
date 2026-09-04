//! What the app-side SMB suites reach the fixture containers through.
//!
//! The fixtures themselves are `cmdr_smb::volume::testing`, shared with the
//! backend's own suites so both sides seed, clean up, and hash the same way. The
//! one thing that differs across the seam is the host: a backend test wants a
//! `VolumeHost` that answers nothing, and an app test wants the real wiring, so
//! the listing cache, the index, and the activity tracker see what the share
//! reports.

pub(crate) use cmdr_smb::volume::testing::*;

// The vocabulary every SMB suite speaks, re-exported so one glob covers it
// instead of the same eight lines at the top of nine files.
pub(crate) use std::path::{Path, PathBuf};
pub(crate) use std::sync::Arc;
pub(crate) use std::sync::atomic::Ordering;
pub(crate) use std::time::Duration;

pub(crate) use cmdr_fs::volume::{Volume, VolumeError, VolumeReadStream, WatchCoverage};

use cmdr_smb::volume::SmbVolume;

/// Connects to the fixture container's `public` share, wired to THIS app.
///
/// Shadows the crate's detached-host builder of the same name, so a suite on
/// this side gets the app's wiring by writing what it always wrote.
pub(crate) async fn make_docker_volume() -> SmbVolume {
    make_docker_volume_with_host(crate::volume_host::host()).await
}
