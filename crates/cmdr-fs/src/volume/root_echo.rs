//! Spotting a typed path that repeats its volume's own root folder.
//!
//! [`super::root_anchored`] reads the transfer dialog's box as volume-relative,
//! always. On a place rooted at a server folder (`sftp://ada@nas:22/srv/data`),
//! a user who types the full server path they know (`/srv/data/photos`) gets it
//! anchored under the root: `/srv/data/srv/data/photos`. Guessing the other
//! reading would be wrong whenever that doubled folder really exists, so the
//! anchoring stays and the dialog warns instead, naming both readings.

use std::path::{Path, PathBuf};

/// A typed path that starts with its volume's root folder, in both readings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RootEcho {
    /// The server-side folder the volume is rooted at (`/srv/data`).
    pub root_folder: PathBuf,
    /// Where the path goes as typed, spelled server-side
    /// (`/srv/data/srv/data/photos`).
    pub resolved: PathBuf,
    /// The path with the repeated root folder taken off, in the box's own
    /// volume-relative dialect (`/photos`).
    pub stripped: PathBuf,
}

/// The two readings of `path` when it starts with `root`'s own folder, or `None`
/// when it can't be read two ways.
///
/// Only a scheme-shaped root (`sftp://`, `webdav://`, `adb://`, `mtp://`) has a
/// folder separate from its app spelling; a mounted root (`/Volumes/naspi`) is
/// matched by [`super::root_anchored`] itself and passes through untouched. A
/// root at the server's `/` reads both ways to the same place, so it never
/// echoes. Folders match by whole COMPONENTS, so `/srv/data-1` isn't a repeat of
/// `/srv/data`.
pub fn root_echo(root: &Path, path: &Path) -> Option<RootEcho> {
    let root_folder = scheme_root_folder(root)?;
    if path.as_os_str().is_empty() || path == Path::new(".") || path == Path::new("/") || path.starts_with(root) {
        return None;
    }
    let relative = path.strip_prefix("/").unwrap_or(path);
    let server_spelling = Path::new("/").join(relative);
    let rest = server_spelling.strip_prefix(&root_folder).ok()?;
    Some(RootEcho {
        resolved: root_folder.join(relative),
        stripped: Path::new("/").join(rest),
        root_folder,
    })
}

/// The folder part of a `<scheme>://<authority>/<folder>` root, or `None` for a
/// mounted root and for one at the server's `/`.
fn scheme_root_folder(root: &Path) -> Option<PathBuf> {
    let (_scheme, rest) = root.to_str()?.split_once("://")?;
    let folder = Path::new(&rest[rest.find('/')?..]);
    (folder != Path::new("/")).then(|| folder.to_path_buf())
}
