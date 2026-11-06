//! Mac-only hidden files: the `.DS_Store` Finder keeps in every folder it shows,
//! and the `._name` AppleDouble files macOS writes beside items on volumes that
//! can't hold xattrs. Both mean something only to a Mac, so a person copying to
//! a NAS, a Linux box, or a zip they'll open elsewhere can ask a transfer to
//! leave them out (#314).
//!
//! The match is on the NAME, typed here once, so every walk (the local scan, each
//! backend's copy scan, the cross-volume merge, the compress plan) asks the same
//! question. Only a FILE matches: a folder named `._x` is the person's own and
//! always goes.
//!
//! Not the same rule as [`crate::apple_double`]'s, which leaves out only a `._`
//! sidecar the KERNEL serves as its sibling's xattrs (so a copy that carries the
//! xattrs would land it twice). That one always applies; this one is the
//! person's choice per operation.

use std::ffi::OsStr;

use serde::{Deserialize, Serialize};

/// Whether a transfer writes Mac-only hidden files to its destination.
// DEFAULT-OK: `Include` claims nothing about the disk; it's what every transfer did
// before the person could ask otherwise, and what a caller that never asks gets.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum MacOnlyFiles {
    /// Copy them like any other file. The default: leaving a file out is
    /// something a person asks for.
    #[default]
    Include,
    /// Leave every listed `.DS_Store` and `._*` file out of the destination.
    Skip,
}

impl MacOnlyFiles {
    /// Whether a walk under this choice leaves out the entry called `name`.
    /// `is_directory` is the listing's own word: a folder never matches.
    pub fn leaves_out(self, name: &OsStr, is_directory: bool) -> bool {
        self == Self::Skip && !is_directory && is_mac_only_file_name(name)
    }

    /// [`Self::leaves_out`] for a name a listing hands over as a `str`.
    pub fn leaves_out_str(self, name: &str, is_directory: bool) -> bool {
        self.leaves_out(OsStr::new(name), is_directory)
    }
}

/// Whether `name` is a Mac-only hidden file's: exactly `.DS_Store`, or `._`
/// followed by at least one more byte. Byte-exact: Finder writes `.DS_Store` in
/// this spelling everywhere, so a `.ds_store` is somebody's own file.
pub fn is_mac_only_file_name(name: &OsStr) -> bool {
    let bytes = name.as_encoded_bytes();
    bytes == b".DS_Store" || bytes.strip_prefix(b"._").is_some_and(|rest| !rest.is_empty())
}

#[cfg(test)]
#[path = "mac_only_files_test.rs"]
mod tests;
