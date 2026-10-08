//! Turning the paths the app addresses this volume with into the remote paths
//! [`crate::transport::WebdavClient::url_for`] encodes.
//!
//! The translation itself is `cmdr_fs::volume::remote_paths`, shared with SFTP:
//! the app root carries the `webdav://<user>@<host>:<port>` prefix, the server's
//! collection tree hangs under it, and a bare server-absolute path is ❌ REFUSED
//! rather than anchored. That module's header has the reasoning.

use std::path::{Path, PathBuf};

use cmdr_fs::volume::VolumeError;
use cmdr_fs::volume::remote_paths::normalize_remote_path;

use super::WebdavVolume;

/// The root as a remote path: `/`, or `/Photos` for a volume opened under a
/// sub-collection. Empty, `.`, and `/` spellings all normalize to `/`.
///
/// ❗ Needed BEFORE a volume exists: `connect_webdav_volume` probes the root
/// with one PROPFIND to prove the credential, and there is nothing to ask yet.
/// The spelling is `normalize_remote_path`'s, the one `RemoteRoot` gives the
/// root once the volume exists, so the probe and the volume agree.
pub(super) fn root_remote_path(remote_root: &Path) -> String {
    normalize_remote_path(remote_root).to_string_lossy().into_owned()
}

impl WebdavVolume {
    /// The remote path for `path`, or `NotFound` when `path` isn't on this
    /// volume.
    pub(super) fn to_remote_path(&self, path: &Path) -> Result<String, VolumeError> {
        self.root
            .to_remote_path(path)
            .ok_or_else(|| VolumeError::NotFound(path.to_string_lossy().into_owned()))
    }

    /// The path the APP addresses `path` by: the prefixed spelling the panes
    /// hold, so the listing-cache patcher and a pane agree on what a file is
    /// called.
    ///
    /// A refusal is "no patch to make": a listing-cache patch is a courtesy, and
    /// ❌ must never fail a mutation that already landed.
    pub(super) fn display_path_for(&self, path: &Path) -> Option<PathBuf> {
        self.root
            .to_remote_path(path)
            .and_then(|remote| self.root.to_app_path(&remote))
    }
}

/// `parent/name` in remote-path spelling.
pub(super) fn child_of(parent: &str, name: &str) -> String {
    if parent.ends_with('/') {
        format!("{parent}{name}")
    } else {
        format!("{parent}/{name}")
    }
}

#[cfg(test)]
#[path = "paths_test.rs"]
mod paths_test;
