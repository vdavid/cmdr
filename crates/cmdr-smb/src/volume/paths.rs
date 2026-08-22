//! Translating between the paths the app addresses this volume with and the
//! share-relative paths smb2 speaks.
//!
//! Every session-touching method starts here, so a wrong answer sends a real
//! request to a real, wrong place. The rules that guard against that are on
//! [`SmbVolume::to_smb_path`].

use super::SmbVolume;
use cmdr_fs::volume::VolumeError;
use std::path::{Path, PathBuf};

impl SmbVolume {
    /// Converts a volume-relative path to the SMB relative path string.
    ///
    /// The frontend sends paths relative to the volume root (which is the mount path).
    /// smb2 expects paths relative to the share root with `/` separators.
    /// NFC-normalizes the result because macOS sends NFD (decomposed) paths
    /// but SMB servers expect NFC (composed). Without this, paths with accented
    /// characters (like "ä") fail with STATUS_OBJECT_PATH_NOT_FOUND.
    ///
    /// An absolute path outside the mount root is `NotFound`, and the root is
    /// matched by whole COMPONENTS: every way of guessing an answer here put a
    /// real request at a real, wrong place. Both, and what each caller does with
    /// the error: `backends/DETAILS.md` § "Per-backend decisions".
    pub(super) fn to_smb_path(&self, path: &Path) -> Result<String, VolumeError> {
        use unicode_normalization::UnicodeNormalization;

        let path_str = path.to_string_lossy();

        // Empty, `.`, and `/` all mean the volume root.
        if path_str.is_empty() || path_str == "/" || path_str == "." {
            return Ok(String::new());
        }

        // Relative paths are what the trait contract asks for: use them as-is.
        if !path.is_absolute() {
            return Ok(path_str.nfc().collect());
        }

        // Absolute (the frontend does send these): must be inside the mount.
        match path.strip_prefix(&self.mount_path) {
            Ok(relative) => Ok(relative.to_string_lossy().nfc().collect()),
            Err(_) => Err(VolumeError::NotFound(path_str.into_owned())),
        }
    }

    /// The absolute display path for `path`'s own location on this share, or
    /// `None` when `path` isn't on this share at all.
    ///
    /// For the post-mutation listing-cache patches: the mutation has already
    /// succeeded by the time they run, so a path that doesn't convert must skip
    /// the notification rather than turn a done write into a reported failure.
    pub(super) fn display_path_for(&self, path: &Path) -> Option<PathBuf> {
        self.to_smb_path(path)
            .ok()
            .map(|smb_path| PathBuf::from(self.to_display_path(&smb_path)))
    }

    /// Returns the full absolute path for a relative SMB path (under mount point).
    pub(super) fn to_display_path(&self, smb_path: &str) -> String {
        if smb_path.is_empty() {
            self.mount_path.to_string_lossy().to_string()
        } else {
            format!("{}/{}", self.mount_path.display(), smb_path)
        }
    }
}

#[cfg(test)]
#[path = "paths_test.rs"]
mod paths_test;
