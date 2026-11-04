//! What a copy carries from its source beyond the bytes and the mtime: the
//! creation date, the mode, the user flags, and the extended attributes.
//!
//! ❗ No mtime and no atime here. A stream's `modified_at` is the ONE source of
//! a copy's mtime, so two getters can never disagree; a local copy hands both
//! dates to [`super::apply_staged`] beside this set ([`super::StagedTimes`]).

use std::ffi::OsString;
use std::fs::{File, Metadata};
use std::path::Path;
use std::time::SystemTime;

use super::{MetadataClasses, UserFlags};
#[cfg(not(target_family = "wasm"))]
use crate::log_private::LogPrivate;

/// The xattr macOS stamps on every file a process writes. The kernel ignores a
/// write of it (verified on macOS 27.0, `xattr -w` then `-p` reads back the
/// writer's own value, 2026-10-11), so copying it costs a read and a write for
/// nothing: a copy never carries it.
const PROVENANCE_XATTR: &str = "com.apple.provenance";

/// Gatekeeper's mark on a download. A copy carries it like any xattr, but a
/// destination that can't hold it loses nothing a person would miss.
const QUARANTINE_XATTR: &str = "com.apple.quarantine";

/// One extended attribute: its name and its whole value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Xattr {
    /// The full name, namespace included (`com.apple.metadata:_kMDItemUserTags`,
    /// `user.app`).
    pub name: OsString,
    /// The value, opaque bytes.
    pub value: Vec<u8>,
}

/// The source's metadata a copy puts on what it writes, apart from the dates
/// the stream already owns.
///
/// The flags travel here but go on only after the landing rename
/// ([`super::LandedMetadata`]): Locked refuses that rename, and the whole set
/// lands together so Hidden can't race ahead of it.
// DEFAULT-OK: the empty set is "the source carried nothing beyond its bytes",
// which is what a source that can't read any of it honestly reports.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CarriedMetadata {
    /// The creation (birth) date.
    pub created: Option<SystemTime>,
    /// The permission bits, setuid/setgid/sticky included (`0o7777`).
    pub mode: Option<u32>,
    /// The user flags Finder shows.
    pub flags: UserFlags,
    /// The extended attributes, provenance excluded; on Linux only `user.*`.
    pub xattrs: Vec<Xattr>,
}

impl CarriedMetadata {
    /// What the local file `file` carries, read off the handle the copy already
    /// holds and the `meta` its `fstat` returned: no path lookup, and no call at
    /// all for a class the stat already answers. A file with no xattrs costs one
    /// `flistxattr`.
    pub fn of_local_file(file: &File, meta: &Metadata) -> Self {
        Self::of_local_file_wanting(file, meta, MetadataClasses::ALL)
    }

    /// [`Self::of_local_file`], restricted to `wanted`: the xattrs (the only
    /// class that costs a call) are read only when wanted.
    pub fn of_local_file_wanting(file: &File, meta: &Metadata, wanted: MetadataClasses) -> Self {
        let xattrs = if wanted.contains(MetadataClasses::XATTRS) {
            local_xattrs(file)
        } else {
            Vec::new()
        };
        Self {
            xattrs,
            ..Self::of_local_stat(meta)
        }
        .restricted_to(wanted)
    }

    /// [`Self::of_local_file_wanting`] with each xattr's value left empty: one
    /// name-only `flistxattr`, never a `fgetxattr`, so asking which classes a
    /// file has costs nothing like reading them (a resource fork can be large).
    pub fn outline_of_local_file(file: &File, meta: &Metadata, wanted: MetadataClasses) -> Self {
        let xattrs = if wanted.contains(MetadataClasses::XATTRS) {
            local_xattr_names(file)
        } else {
            Vec::new()
        };
        Self {
            xattrs,
            ..Self::of_local_stat(meta)
        }
        .restricted_to(wanted)
    }

    /// This set with every class outside `wanted` dropped.
    #[must_use]
    pub fn restricted_to(mut self, wanted: MetadataClasses) -> Self {
        if !wanted.contains(MetadataClasses::CREATED) {
            self.created = None;
        }
        if !wanted.contains(MetadataClasses::MODE) {
            self.mode = None;
        }
        if !wanted.contains(MetadataClasses::FLAGS) {
            self.flags = UserFlags::NONE;
        }
        if !wanted.contains(MetadataClasses::XATTRS) {
            self.xattrs = Vec::new();
        }
        self
    }

    /// Whether the set carries nothing at all, the common case for a source
    /// that can't report a class.
    pub fn is_empty(&self) -> bool {
        self.created.is_none() && self.mode.is_none() && self.flags.is_empty() && self.xattrs.is_empty()
    }

    /// Which classes of this set hold something a person would miss if a copy
    /// dropped it, for an item last modified at `modified` on a volume where a
    /// fresh file gets `default_mode`: a creation date before the last edit (an
    /// equal one is what a dropped date looks like anyway), a mode a fresh file
    /// wouldn't get, any user flag, and any xattr but the quarantine. What the
    /// loss note counts (the app's `transfer/volume/DETAILS.md` § "Copies keep
    /// the source's metadata"); a class this set doesn't carry is never in it.
    pub fn worth_keeping(&self, modified: Option<SystemTime>, default_mode: u32) -> MetadataClasses {
        let whole_secs = |date: SystemTime| date.duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
        let mut worth = MetadataClasses::NONE;
        if let Some(created) = self.created
            && modified.is_none_or(|modified| whole_secs(created) < whole_secs(modified))
        {
            worth = worth | MetadataClasses::CREATED;
        }
        if self.mode.is_some_and(|mode| mode & 0o7777 != default_mode & 0o7777) {
            worth = worth | MetadataClasses::MODE;
        }
        if !self.flags.is_empty() {
            worth = worth | MetadataClasses::FLAGS;
        }
        if self.xattrs.iter().any(|xattr| xattr.name != QUARANTINE_XATTR) {
            worth = worth | MetadataClasses::XATTRS;
        }
        worth
    }

    /// The part of this set a copy puts on only AFTER the landing rename, onto
    /// a destination whose write keeps `staged` flags on the staged temp
    /// ([`super::MetadataCaps::staged_flags`]): the whole flag set when it holds
    /// any flag the write couldn't put on (Locked refuses the rename), so a
    /// destination that sets its flags as one word keeps the rest too; nothing
    /// otherwise, which is what lets a plain or Hidden-only file onto SMB cost no
    /// frame after its landing.
    pub fn after_landing(&self, staged: UserFlags) -> Self {
        if self.flags.without(staged).is_empty() {
            return Self::default();
        }
        Self {
            flags: self.flags,
            ..Self::default()
        }
    }

    /// What the symlink at `path` carries ITSELF (never its target's), given the
    /// `lstat` the caller took: its creation date and its own xattrs. No mode
    /// (a link's is meaningless) and no flags.
    pub fn of_local_link(path: &Path, meta: &Metadata) -> Self {
        Self {
            created: meta.created().ok(),
            mode: None,
            flags: UserFlags::NONE,
            xattrs: xattrs_at(path),
        }
    }

    /// What the local folder at `path` carries, given the `lstat` the caller
    /// took: its creation date, mode, flags, and xattrs (Finder tags, comments,
    /// the FinderInfo that marks a custom icon). A folder with no xattrs costs
    /// one `listxattr`.
    pub fn of_local_dir(path: &Path, meta: &Metadata) -> Self {
        Self {
            xattrs: xattrs_at(path),
            ..Self::of_local_stat(meta)
        }
    }

    /// The classes a local item's stat answers: creation date, mode, and
    /// (macOS) flags. No call at all.
    pub fn of_local_stat(meta: &Metadata) -> Self {
        #[cfg(not(target_family = "wasm"))]
        let mode = {
            use std::os::unix::fs::PermissionsExt as _;
            Some(meta.permissions().mode() & 0o7777)
        };
        // The browser build has no host filesystem, so no mode bits.
        #[cfg(target_family = "wasm")]
        let mode = None;
        Self {
            created: meta.created().ok(),
            mode,
            flags: flags_of(meta),
            xattrs: Vec::new(),
        }
    }
}

/// The xattrs worth carrying off the open `file`. A file with none costs one
/// `flistxattr`.
#[cfg(not(target_family = "wasm"))]
fn local_xattrs(file: &File) -> Vec<Xattr> {
    use xattr::FileExt as _;
    match file.list_xattr() {
        Ok(names) => names
            .filter(carries)
            .filter_map(|name| match file.get_xattr(&name) {
                Ok(Some(value)) => Some(Xattr { name, value }),
                Ok(None) => None,
                Err(e) => {
                    // allowed-log-private: an xattr key (`com.apple.…`), the triage clue, not a file's name
                    log::debug!(target: "metadata", "couldn't read xattr {name:?}: {e}");
                    None
                }
            })
            .collect(),
        Err(e) => {
            log::debug!(target: "metadata", "couldn't list a source's xattrs: {e}");
            Vec::new()
        }
    }
}

/// The names of the xattrs worth carrying off the open `file`, each with an
/// empty value.
#[cfg(not(target_family = "wasm"))]
fn local_xattr_names(file: &File) -> Vec<Xattr> {
    use xattr::FileExt as _;
    match file.list_xattr() {
        Ok(names) => names
            .filter(carries)
            .map(|name| Xattr {
                name,
                value: Vec::new(),
            })
            .collect(),
        Err(e) => {
            log::debug!(target: "metadata", "couldn't list a source's xattrs: {e}");
            Vec::new()
        }
    }
}

/// The xattrs a copy carries off the item at `path` itself. `xattr::list` and
/// `xattr::get` never follow a link (`XATTR_NOFOLLOW`).
#[cfg(not(target_family = "wasm"))]
fn xattrs_at(path: &Path) -> Vec<Xattr> {
    match xattr::list(path) {
        Ok(names) => names
            .filter(carries)
            .filter_map(|name| match xattr::get(path, &name) {
                Ok(Some(value)) => Some(Xattr { name, value }),
                Ok(None) => None,
                Err(e) => {
                    // allowed-log-private: an xattr key (`com.apple.…`), the triage clue, not a file's name
                    log::debug!(target: "metadata", "couldn't read xattr {name:?}: {e}");
                    None
                }
            })
            .collect(),
        Err(e) => {
            log::debug!(target: "metadata", "couldn't list the xattrs of {}: {e}", LogPrivate::path(path));
            Vec::new()
        }
    }
}

// The browser build has no host filesystem, so no xattrs to read
// (`DETAILS.md` § "The browser build").
#[cfg(target_family = "wasm")]
fn local_xattrs(_file: &File) -> Vec<Xattr> {
    Vec::new()
}

#[cfg(target_family = "wasm")]
fn local_xattr_names(_file: &File) -> Vec<Xattr> {
    Vec::new()
}

#[cfg(target_family = "wasm")]
fn xattrs_at(_path: &Path) -> Vec<Xattr> {
    Vec::new()
}

#[cfg(target_os = "macos")]
fn flags_of(meta: &Metadata) -> UserFlags {
    use std::os::macos::fs::MetadataExt as _;
    UserFlags::from_bsd(meta.st_flags())
}

#[cfg(not(target_os = "macos"))]
fn flags_of(_meta: &Metadata) -> UserFlags {
    UserFlags::NONE
}

/// Whether a copy carries the xattr `name`. On macOS everything but provenance;
/// on Linux only `user.*`: `system.posix_acl_*` is the ACL (`exacl` carries it),
/// and `trusted.*` and `security.*` need privileges a desktop app doesn't have.
#[cfg_attr(
    target_family = "wasm",
    allow(dead_code, reason = "the browser build reads no xattrs")
)]
fn carries(name: &OsString) -> bool {
    if cfg!(target_os = "macos") {
        name != PROVENANCE_XATTR
    } else {
        name.as_encoded_bytes().starts_with(b"user.")
    }
}
