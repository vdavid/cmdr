//! Turning the paths the app addresses this volume with into root-relative
//! remote paths, the string [`crate::transport::WebdavClient::url_for`] encodes.
//!
//! The rules are `cmdr-sftp`'s (`crates/cmdr-sftp/src/volume/paths.rs`) and for
//! the same reasons: whole-component root matching, lexical `..` resolution
//! before the containment check, and a refusal (never anchoring) for anything
//! outside the root.

use std::path::{Component, Path, PathBuf};

use cmdr_fs::volume::VolumeError;

use super::WebdavVolume;

/// The root as a remote path: `/`, or `/Photos` for a volume opened under a
/// sub-collection. Empty, `.`, and `/` spellings all normalize to `/`.
pub(super) fn root_remote_path(remote_root: &Path) -> String {
    let normalized = normalize(&Path::new("/").join(remote_root));
    normalized.to_string_lossy().into_owned()
}

impl WebdavVolume {
    /// The root-relative remote path for `path`, or `NotFound` when `path`
    /// isn't on this volume. ❌ Never `root_anchored`: anchoring turns
    /// `/etc/passwd` into a real path under the root, and quietly the wrong one.
    pub(super) fn to_remote_path(&self, path: &Path) -> Result<String, VolumeError> {
        let root = normalize(&self.root);
        if path == Path::new("/") {
            return Ok(root.to_string_lossy().into_owned());
        }
        let joined = if path.is_absolute() {
            normalize(path)
        } else {
            normalize(&root.join(path))
        };
        if !joined.starts_with(&root) {
            return Err(VolumeError::NotFound(path.to_string_lossy().into_owned()));
        }
        Ok(joined.to_string_lossy().into_owned())
    }

    /// The path the APP addresses `path` by, which for this backend is the same
    /// string the server does. A refusal is "no patch to make": a listing-cache
    /// patch is a courtesy, and ❌ must never fail a mutation that already landed.
    pub(super) fn display_path_for(&self, path: &Path) -> Option<PathBuf> {
        self.to_remote_path(path).ok().map(PathBuf::from)
    }
}

/// Resolves `.` and `..` lexically. `..` at the root is absorbed.
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
            Component::Prefix(_) => {}
        }
    }
    if out.as_os_str().is_empty() {
        out.push(Component::RootDir);
    }
    out
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
