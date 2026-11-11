//! The browser build has no host filesystem: no mode, xattrs, ACL, flags, or dates to read or
//! write (`DETAILS.md` § "The browser build"), so every class answers "the platform can't hold
//! this", and `Acl` wraps `Infallible`, so no code path can hold one.

use std::ffi::OsString;
use std::fs::{File, Metadata};
use std::path::Path;
use std::time::SystemTime;

use super::super::{
    ClassOutcome, MetadataCaps, MetadataClasses, ParentOpening, RemovalLock, StagedItem, StagedTimes, Unblock,
    UserFlags, Xattr,
};

/// The browser build reads no ACLs, so it can never hold one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Acl(std::convert::Infallible);

pub(crate) fn mode_of(_meta: &Metadata) -> Option<u32> {
    None
}

pub(crate) fn write_mode_at(_path: &Path, _mode: u32) -> ClassOutcome {
    ClassOutcome::NotApplicable
}

pub(crate) fn write_mode_staged(_item: StagedItem<'_>, _mode: u32) -> ClassOutcome {
    ClassOutcome::NotApplicable
}

pub(crate) fn probe_umask() -> u32 {
    super::super::mode::FALLBACK_UMASK
}

pub(crate) fn carries_xattr(name: &OsString) -> bool {
    name.as_encoded_bytes().starts_with(b"user.")
}

pub(crate) fn local_xattrs(_file: &File) -> Vec<Xattr> {
    Vec::new()
}

pub(crate) fn local_xattr_names(_file: &File) -> Vec<Xattr> {
    Vec::new()
}

pub(crate) fn xattrs_at(_path: &Path) -> Vec<Xattr> {
    Vec::new()
}

pub(crate) fn write_xattrs(_item: StagedItem<'_>, xattrs: &[Xattr]) -> ClassOutcome {
    if xattrs.is_empty() {
        ClassOutcome::Absent
    } else {
        ClassOutcome::NotApplicable
    }
}

pub(crate) fn read_acl(_path: &Path) -> Option<Acl> {
    None
}

pub(crate) fn write_acl(_path: &Path, acl: &Acl) -> ClassOutcome {
    match acl.0 {}
}

pub(crate) fn flags_of(_meta: &Metadata) -> UserFlags {
    UserFlags::NONE
}

pub(crate) fn add_flags(_path: &Path, _flags: UserFlags) -> ClassOutcome {
    ClassOutcome::NotApplicable
}

pub(crate) fn clear_blockers(_path: &Path) -> Unblock {
    Unblock::NothingToClear
}

pub(crate) fn removal_lock(_path: &Path) -> RemovalLock {
    RemovalLock::Free
}

pub(crate) fn open_parent_folder(_child: &Path) -> ParentOpening {
    ParentOpening::NothingToOpen
}

/// The browser has no local disk to keep anything on.
pub const LOCAL_DISK_CAPS: MetadataCaps = MetadataCaps {
    stores: MetadataClasses::NONE,
    reports: MetadataClasses::NONE,
    folder_stores: MetadataClasses::NONE,
    folder_dates: false,
    staged_flags: UserFlags::NONE,
};

pub(crate) const CREATED_SETTABLE: bool = false;

pub(crate) fn set_dates(item: StagedItem<'_>, _created: Option<SystemTime>, times: StagedTimes) -> std::io::Result<()> {
    let file_times = times.file_times();
    match item {
        StagedItem::File(file) => file.set_times(file_times),
        StagedItem::Path(path) => File::open(path)?.set_times(file_times),
        StagedItem::Link(_) => Err(std::io::ErrorKind::Unsupported.into()),
    }
}

pub(crate) fn is_unsupported(_e: &std::io::Error) -> bool {
    false
}
