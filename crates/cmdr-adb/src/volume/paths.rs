//! Turning the paths the app addresses this volume with into device paths.
//!
//! The volume is rooted at the device's `/`, so anchoring is close to the
//! identity: a relative path hangs off `/`, an absolute one is already a device
//! path, and both spell the same file. What the function guards against is
//! `..`, which the device would resolve happily and which is the one way a path
//! can address something outside a volume rooted at `/`.

use std::path::{Component, Path, PathBuf};

use cmdr_fs::volume::VolumeError;

use super::AdbVolume;

impl AdbVolume {
    /// The absolute device-side path for `path`, or `NotFound` when `path`
    /// escapes the root.
    ///
    /// Idempotent: a pane's `/sdcard/DCIM` and a destination box's
    /// `sdcard/DCIM` land on the same file. Empty, `.`, and a bare `/` all mean
    /// the root. `..` is resolved lexically before anything is sent, and a path
    /// that would climb above `/` is refused the way `SftpVolume::to_remote_path`
    /// refuses one outside its export: ❌ never anchored, because anchoring
    /// silently addresses a real, wrong file.
    pub(super) fn to_device_path(&self, path: &Path) -> Result<String, VolumeError> {
        let mut out = PathBuf::from("/");
        for component in path.components() {
            match component {
                Component::RootDir | Component::CurDir => {}
                Component::ParentDir => {
                    if !out.pop() {
                        return Err(VolumeError::NotFound(path.to_string_lossy().into_owned()));
                    }
                }
                Component::Normal(part) => out.push(part),
                // No drive letters on a device reached over a socket.
                Component::Prefix(_) => {}
            }
        }
        Ok(out.to_string_lossy().into_owned())
    }

    /// The path the APP addresses `path` by, which for this backend is the same
    /// string the device does. A refusal becomes "no patch to make": a
    /// listing-cache patch is a courtesy and ❌ must never fail a mutation that
    /// already landed.
    pub(super) fn display_path_for(&self, path: &Path) -> Option<PathBuf> {
        self.to_device_path(path).ok().map(PathBuf::from)
    }
}

/// `parent/name` on the device, with the root's own slash not doubled.
pub(super) fn join_device_path(parent: &str, name: &str) -> String {
    if parent.ends_with('/') {
        format!("{parent}{name}")
    } else {
        format!("{parent}/{name}")
    }
}

#[cfg(test)]
#[path = "paths_test.rs"]
mod paths_test;
