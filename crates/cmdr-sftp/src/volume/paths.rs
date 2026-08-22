//! Turning the paths the app addresses this volume with into remote paths.
//!
//! Every session-touching method starts here, so a wrong answer sends a real
//! request to a real, wrong place. The rules that guard against that are on
//! [`SftpVolume::to_remote_path`].

use std::path::{Component, Path, PathBuf};

use cmdr_fs::volume::VolumeError;

use super::SftpVolume;

impl SftpVolume {
    /// The absolute server-side path for `path`, or `NotFound` when `path` isn't
    /// on this volume.
    ///
    /// The app addresses this volume with paths relative to its root, and also
    /// with the absolute paths this backend itself handed out — which ARE remote
    /// paths, since there's no mount and no second spelling of the tree. Empty,
    /// `.`, and a bare `/` all mean the root.
    ///
    /// # Why it refuses instead of anchoring
    ///
    /// ❌ Never reach for `cmdr_fs::volume::root_anchored` here. That helper
    /// ANCHORS: on a volume rooted at `/srv/data` it turns `/etc/passwd` into
    /// `/srv/data/etc/passwd`, which is a real path on a real server and quietly
    /// the wrong one. Refusing is what says so. (`SmbVolume::to_smb_path` makes
    /// the same call for the same reason.)
    ///
    /// # Two ways of guessing that would each send a request somewhere wrong
    ///
    /// - **The root is matched by whole COMPONENTS.** A string prefix compare
    ///   strips `/srv/data` off a sibling `/srv/data-1/photos` and asks the
    ///   server for `-1/photos`, which is a legal name.
    /// - **`..` is resolved before the check, never after.** `photos/../../etc`
    ///   is the same escape spelled relatively, and the server would resolve it
    ///   happily.
    pub(super) fn to_remote_path(&self, path: &Path) -> Result<String, VolumeError> {
        let root = normalize(&self.root);
        // A bare `/` is how the app spells "this volume's root", the same way it
        // does for a share. Everything else absolute is a real server path.
        if path == Path::new("/") {
            return Ok(root.to_string_lossy().into_owned());
        }
        let joined = if path.is_absolute() {
            normalize(path)
        } else {
            // `.` and the empty path both mean the root, and `join` on either
            // leaves the root alone, so they need no special case.
            normalize(&root.join(path))
        };

        if !joined.starts_with(&root) {
            return Err(VolumeError::NotFound(path.to_string_lossy().into_owned()));
        }
        Ok(joined.to_string_lossy().into_owned())
    }

}

/// Resolves `.` and `..` lexically, with no round trip and no symlink following.
///
/// Lexical on purpose: asking the server would be a round trip per path AND a
/// TOCTOU window, and the question here is "did the caller address something
/// outside this volume", which is about the path they wrote rather than about
/// what it resolves to.
///
/// `..` at the root is absorbed rather than escaping, matching what a POSIX
/// server does with `/..`.
fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::RootDir => out.push(Component::RootDir),
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            Component::Normal(part) => out.push(part),
            // Windows-only, and this backend is reached over a network from
            // wherever Cmdr runs, so there is no drive letter to carry.
            Component::Prefix(_) => {}
        }
    }
    out
}

#[cfg(test)]
#[path = "paths_test.rs"]
mod paths_test;
