//! The paths the app addresses a place with, down to the bucket and key a
//! request names.
//!
//! The translation is `cmdr_fs::volume::remote_paths`, shared with SFTP and
//! WebDAV: the app spelling carries the account's `s3://<key id>@<host>:<port>`
//! prefix, the server-side tree under it is `/<bucket>/<key…>`, and a place is
//! rooted at `/` (the account, listing buckets) or `/<bucket>`. A bare
//! server-absolute path is ❌ REFUSED rather than anchored; that module's header
//! has why.

use std::collections::HashSet;
use std::path::Path;

use cmdr_fs::volume::VolumeError;

use super::S3Volume;
use super::listing::{Child, FILE_SUFFIX};

/// What a server-side path names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Target<'a> {
    /// `/`: the account, whose children are its buckets.
    Account,
    /// `/<bucket>`: a bucket's top.
    Bucket(&'a str),
    /// `/<bucket>/<key>`: an object, or a folder (a prefix) under the bucket.
    Key { bucket: &'a str, key: &'a str },
}

/// Splits a normalized server-side path (`RemoteRoot::to_remote_path`'s
/// answer: absolute, no trailing slash, no `.` or `..`).
pub(super) fn target_of(remote: &str) -> Target<'_> {
    let rest = remote.trim_start_matches('/');
    match rest.split_once('/') {
        _ if rest.is_empty() => Target::Account,
        Some((bucket, key)) if !key.is_empty() => Target::Key { bucket, key },
        Some((bucket, _)) => Target::Bucket(bucket),
        None => Target::Bucket(rest),
    }
}

/// Which of a name's two holders a path means.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Holder {
    /// Whatever holds it, the folder first (the usual case: no file was
    /// listed beside a folder of its name).
    Either,
    /// The file shown as `<name> (file)`.
    File,
    /// The folder a file of its name was listed beside: ❗ never that file.
    Folder,
}

/// A path as a request names it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Resolved {
    /// The server-side path the request names: for a `<name> (file)` row, the
    /// file's real key.
    pub remote: String,
    pub holder: Holder,
}

// ── A file beside a folder of its name ────────────────────────────────
//
// S3 lets the key `photos` sit beside keys under `photos/`. A pane row is one
// path, so the listing shows the file as `photos (file)` (`listing::settle`)
// and remembers its real path in `beside_folders`. Every path then goes
// through `resolve`: `…/photos (file)` names the key `photos`, and `…/photos`
// names ONLY the folder, so no operation on the folder row (delete, rename,
// stat) can reach the file. Each listing of the folder refreshes what it holds,
// and deleting or renaming the file forgets it.

impl S3Volume {
    /// The server-side path for `path`, or `NotFound` when `path` isn't on this
    /// place. A `<name> (file)` row's path gives its real key.
    pub(super) fn to_remote_path(&self, path: &Path) -> Result<String, VolumeError> {
        Ok(self.resolve(path)?.remote)
    }

    /// The server-side path for `path` and which holder of its name it means.
    pub(super) fn resolve(&self, path: &Path) -> Result<Resolved, VolumeError> {
        let shown = self
            .root
            .to_remote_path(path)
            .ok_or_else(|| VolumeError::NotFound(path.to_string_lossy().into_owned()))?;
        let beside = self.beside_folders();
        if let Some(real) = shown.strip_suffix(FILE_SUFFIX)
            && beside.contains(real)
        {
            return Ok(Resolved {
                remote: real.to_string(),
                holder: Holder::File,
            });
        }
        let holder = if beside.contains(&shown) {
            Holder::Folder
        } else {
            Holder::Either
        };
        Ok(Resolved { remote: shown, holder })
    }

    /// The server-side path a pane shows the object at `remote` under:
    /// `<remote> (file)` when it was listed beside a folder of its name.
    pub(super) fn shown_remote(&self, remote: &str) -> String {
        if self.beside_folders().contains(remote) {
            format!("{remote}{FILE_SUFFIX}")
        } else {
            remote.to_string()
        }
    }

    /// Records which files a listing of `parent` showed beside a folder of
    /// their name, replacing what the folder's last listing said.
    pub(super) fn note_beside_folders(&self, parent: &str, children: &[Child]) {
        let mut beside = self.beside_folders();
        beside.retain(|remote| parent_of(remote) != parent);
        for child in children {
            if let Child::Object {
                name,
                beside_folder: true,
                ..
            } = child
                && let Some(real) = name.strip_suffix(FILE_SUFFIX)
            {
                beside.insert(child_of(parent, real));
            }
        }
    }

    /// The file at `remote` is gone (deleted, renamed away): its folder's path
    /// means whatever holds it again.
    pub(super) fn forget_beside_folder(&self, remote: &str) {
        self.beside_folders().remove(remote);
    }

    /// The set, whatever a panicking holder left: each entry is a whole path.
    fn beside_folders(&self) -> std::sync::MutexGuard<'_, HashSet<String>> {
        self.inner
            .beside_folders
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

/// The server-side path of `remote`'s parent folder.
fn parent_of(remote: &str) -> &str {
    match remote.rsplit_once('/') {
        Some(("", _)) => "/",
        Some((parent, _)) => parent,
        None => "",
    }
}

/// `parent/name` in server-side spelling.
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
