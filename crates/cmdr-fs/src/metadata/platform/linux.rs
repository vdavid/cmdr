//! Linux: POSIX ACLs through `exacl`, no user flags (`chattr`'s need `CAP_LINUX_IMMUTABLE`,
//! and Hidden is a dot-name), no settable birth date, and nothing that blocks a removal the
//! way macOS's Locked does.

use std::ffi::OsString;
use std::fs::{File, Metadata};
use std::path::Path;
use std::time::SystemTime;

use super::super::{
    ClassOutcome, MetadataCaps, MetadataClasses, RemovalLock, StagedItem, StagedTimes, Unblock, UserFlags,
};
use crate::log_private::LogPrivate;

pub use super::unix::Acl;
pub(crate) use super::unix::{
    local_xattr_names, local_xattrs, mode_of, open_parent_folder, probe_umask, write_mode_at, write_mode_staged,
    write_xattrs, xattrs_at,
};

/// Only `user.*`: `system.posix_acl_*` is the ACL (`exacl` carries it), and `trusted.*` and
/// `security.*` need privileges a desktop app doesn't have.
pub(crate) fn carries_xattr(name: &OsString) -> bool {
    name.as_encoded_bytes().starts_with(b"user.")
}

pub(crate) fn flags_of(_meta: &Metadata) -> UserFlags {
    UserFlags::NONE
}

pub(crate) fn add_flags(_path: &Path, _flags: UserFlags) -> ClassOutcome {
    ClassOutcome::NotApplicable
}

/// A named user or group, or a default ACL, is what makes it more than the mode.
pub(crate) fn read_acl(path: &Path) -> Option<Acl> {
    match exacl::getfacl(path, None) {
        Ok(entries)
            if entries
                .iter()
                .any(|e| !e.name.is_empty() || e.flags.contains(exacl::Flag::DEFAULT)) =>
        {
            Some(Acl(entries))
        }
        Ok(_) => None,
        Err(e) => {
            log::debug!(target: "metadata", "couldn't read the ACL of {}: {e}", LogPrivate::path(path));
            None
        }
    }
}

pub(crate) fn write_acl(path: &Path, acl: &Acl) -> ClassOutcome {
    match exacl::setfacl(&[path], &acl.0, None) {
        Ok(()) => ClassOutcome::Kept,
        Err(e) => {
            log::debug!(target: "metadata", "couldn't set the ACL on {}: {e}", LogPrivate::path(path));
            ClassOutcome::Refused
        }
    }
}

pub(crate) fn clear_blockers(_path: &Path) -> Unblock {
    Unblock::NothingToClear
}

pub(crate) fn removal_lock(_path: &Path) -> RemovalLock {
    RemovalLock::Free
}

/// Linux can't set a creation date (`statx` reads it) and has no user flags.
pub const LOCAL_DISK_CAPS: MetadataCaps = MetadataCaps {
    stores: MetadataClasses::MODE.union(MetadataClasses::XATTRS),
    reports: MetadataClasses::CREATED
        .union(MetadataClasses::MODE)
        .union(MetadataClasses::XATTRS),
    folder_stores: MetadataClasses::MODE.union(MetadataClasses::XATTRS),
    folder_dates: true,
    staged_flags: UserFlags::NONE,
};

/// No settable birth date on Linux.
pub(crate) const CREATED_SETTABLE: bool = false;

/// One `futimens`. std's path setter is unstable, so a folder is opened for its fd; a link's
/// own dates would need `utimensat(AT_SYMLINK_NOFOLLOW)`.
pub(crate) fn set_dates(item: StagedItem<'_>, _created: Option<SystemTime>, times: StagedTimes) -> std::io::Result<()> {
    let file_times = times.file_times();
    match item {
        StagedItem::File(file) => file.set_times(file_times),
        StagedItem::Path(path) => File::open(path)?.set_times(file_times),
        StagedItem::Link(_) => Err(std::io::ErrorKind::Unsupported.into()),
    }
}

/// Only `Unsupported` says the destination can't hold the class.
pub(crate) fn is_unsupported(_e: &std::io::Error) -> bool {
    false
}
