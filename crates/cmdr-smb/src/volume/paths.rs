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
    /// smb2 expects paths relative to the SHARE root with `/` separators, and the
    /// two are the same place only when this mount sits at the share root: an
    /// anchored mount (a DFS sub-mount, a subdirectory mount) has the volume root
    /// a directory or more inside the share, so its
    /// [`share_root`](SmbVolume::share_root) is joined on here.
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

        // Empty, `.`, and `/` all mean the volume root, which is where this mount
        // is anchored inside the share.
        if path_str.is_empty() || path_str == "/" || path_str == "." {
            return Ok(self.share_root.clone());
        }

        // Relative paths are what the trait contract asks for, and they're relative
        // to the VOLUME root, so they get the anchor too.
        if !path.is_absolute() {
            return Ok(self.under_share_root(&path_str.nfc().collect::<String>()));
        }

        // Absolute (the frontend does send these): must be inside the mount.
        match path.strip_prefix(&self.mount_path) {
            Ok(relative) => Ok(self.under_share_root(&relative.to_string_lossy().nfc().collect::<String>())),
            Err(_) => Err(VolumeError::NotFound(path_str.into_owned())),
        }
    }

    /// Joins a mount-relative path onto this mount's anchor inside the share.
    ///
    /// Free when the mount sits at the share root, which is every ordinary mount.
    fn under_share_root(&self, mount_relative: &str) -> String {
        match (self.share_root.as_str(), mount_relative) {
            ("", _) => mount_relative.to_string(),
            (anchor, "") => anchor.to_string(),
            (anchor, rest) => format!("{anchor}/{rest}"),
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

    /// Returns the full absolute path for a share-relative SMB path (under the
    /// mount point).
    ///
    /// The inverse of [`to_smb_path`](Self::to_smb_path): this mount's anchor
    /// inside the share comes back off, by whole COMPONENTS, so a sibling that
    /// merely shares a name prefix (`domain.old` beside `domain`) can't be
    /// mis-stripped into a path on a directory nobody mounted.
    ///
    /// Callers pass a path this instance's `to_smb_path` produced, so it's always
    /// under the anchor. One that isn't stays under the mount root unchanged,
    /// which is what an unanchored mount does with every path anyway.
    pub(super) fn to_display_path(&self, smb_path: &str) -> String {
        let mount_relative = self.below_share_root(smb_path);
        if mount_relative.is_empty() {
            self.mount_path.to_string_lossy().to_string()
        } else {
            format!("{}/{}", self.mount_path.display(), mount_relative)
        }
    }

    /// A share-relative path re-expressed relative to this mount's anchor.
    fn below_share_root<'a>(&self, smb_path: &'a str) -> &'a str {
        if self.share_root.is_empty() {
            return smb_path;
        }
        match smb_path.strip_prefix(&self.share_root) {
            // The anchor itself, or a whole component below it. A shared name
            // prefix (`domain.old`) fails this and falls through untouched.
            Some("") => "",
            Some(rest) => rest.strip_prefix('/').unwrap_or(smb_path),
            None => smb_path,
        }
    }
}

#[cfg(test)]
#[path = "paths_test.rs"]
mod paths_test;
