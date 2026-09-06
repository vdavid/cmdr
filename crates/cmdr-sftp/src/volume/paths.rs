//! Turning the paths the app addresses this volume with into remote paths.
//!
//! Every session-touching method starts here, so a wrong answer sends a real
//! request to a real, wrong place. The translation itself is
//! `cmdr_fs::volume::remote_paths`, shared with WebDAV: the app root carries the
//! `sftp://<user>@<host>:<port>` prefix, the server's tree hangs under it, and a
//! bare server-absolute path is ❌ REFUSED rather than anchored. That module's
//! header has the reasoning and the two ways of guessing that would each send a
//! request somewhere wrong.

use std::path::{Path, PathBuf};

use cmdr_fs::volume::VolumeError;

use super::SftpVolume;

impl SftpVolume {
    /// The absolute server-side path for `path`, or `NotFound` when `path` isn't
    /// on this volume.
    pub(super) fn to_remote_path(&self, path: &Path) -> Result<String, VolumeError> {
        self.root
            .to_remote_path(path)
            .ok_or_else(|| VolumeError::NotFound(path.to_string_lossy().into_owned()))
    }

    /// The path the APP addresses `path` by: the prefixed spelling the panes
    /// hold, so the listing-cache patcher and a pane agree on what a file is
    /// called.
    ///
    /// A refusal becomes "no patch to make": a listing-cache patch is a
    /// courtesy, and ❌ must never fail a mutation that already landed.
    pub(super) fn display_path_for(&self, path: &Path) -> Option<PathBuf> {
        self.root
            .to_remote_path(path)
            .map(|remote| self.root.to_app_path(&remote))
    }
}

#[cfg(test)]
#[path = "paths_test.rs"]
mod paths_test;
