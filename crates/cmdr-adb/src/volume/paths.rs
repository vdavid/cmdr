//! Turning the paths the app addresses this volume with into device paths, and
//! back.
//!
//! The volume is rooted at `adb://<serial>` (`cmdr_fs::volume::adb_app_root`),
//! and the device's own tree hangs under it: `adb://R58M1/sdcard/DCIM` is the
//! device's `/sdcard/DCIM`. The translation is `cmdr_fs::volume::remote_paths`,
//! shared with SFTP and WebDAV, so a bare device-absolute path is ❌ REFUSED
//! rather than anchored. That module's header has why: an un-hinted resolver
//! sends a scheme-free `/sdcard/x` to the Mac's boot disk.
//!
//! ❗ **One refusal is this backend's own: a `..` that climbs above the root.**
//! `RemoteRoot` resolves `..` lexically and absorbs it at `/`, which is right for
//! a server rooted deeper than `/` (the climb leaves the root and is refused by
//! the containment check) but silent here, where the root IS `/`:
//! `adb://R58M1/sdcard/../../etc` would quietly become `/etc`. So the climb is
//! counted first and refused, ❌ never anchored.

use std::path::{Component, Path, PathBuf};

use cmdr_fs::volume::VolumeError;

use super::AdbVolume;

impl AdbVolume {
    /// The absolute device-side path for `path`, or `NotFound` when `path` isn't
    /// on this volume.
    ///
    /// Accepts the root's three aliases (empty, `.`, `/`), this device's own
    /// `adb://<serial>/…` paths, and paths relative to the root, all resolved
    /// lexically. Refuses a bare device-absolute path, another device's prefix,
    /// and a `..` that climbs above the root.
    pub(super) fn to_device_path(&self, path: &Path) -> Result<String, VolumeError> {
        let refused = || VolumeError::NotFound(path.to_string_lossy().into_owned());
        if climbs_above_the_root(path.strip_prefix(&self.root).unwrap_or(path)) {
            return Err(refused());
        }
        self.paths.to_remote_path(path).ok_or_else(refused)
    }

    /// The app-facing spelling of a device path: the exact inverse of
    /// [`to_device_path`](Self::to_device_path).
    ///
    /// ❗ The device's `/` comes back as the bare root, `adb://<serial>`, with no
    /// trailing slash: that is what the device provider's row and the frontend's
    /// `constructAdbPath` spell, and a pane compares the root by string.
    pub(super) fn to_app_path(&self, device: &str) -> PathBuf {
        if device == "/" {
            return self.root.clone();
        }
        self.paths.to_app_path(device)
    }

    /// The path the APP addresses `path` by, for the listing-cache patcher. A
    /// refusal becomes "no patch to make": a listing-cache patch is a courtesy
    /// and ❌ must never fail a mutation that already landed.
    pub(super) fn display_path_for(&self, path: &Path) -> Option<PathBuf> {
        self.to_device_path(path).ok().map(|device| self.to_app_path(&device))
    }
}

/// Whether `path`'s `..` components would climb above the root it hangs off.
fn climbs_above_the_root(path: &Path) -> bool {
    let mut depth = 0usize;
    for component in path.components() {
        match component {
            Component::Normal(_) => depth += 1,
            Component::ParentDir => match depth.checked_sub(1) {
                Some(up) => depth = up,
                None => return true,
            },
            Component::RootDir | Component::CurDir | Component::Prefix(_) => {}
        }
    }
    false
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
