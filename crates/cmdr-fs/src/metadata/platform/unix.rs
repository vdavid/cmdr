//! What both Unixes answer the same way: POSIX modes, the `xattr` calls, the umask probe, and
//! opening a parent folder for one removal.

use std::fs::{File, Metadata};
use std::os::unix::fs::PermissionsExt as _;
use std::path::Path;

use super::super::{
    CarriedMetadata, ClassOutcome, LandedMetadata, OpenedFolder, ParentOpening, StagedItem, Unblock, Xattr,
};
use crate::log_private::LogPrivate;

/// An ACL as the helper carries it: opaque entries, read and written whole.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Acl(pub(in crate::metadata) Vec<exacl::AclEntry>);

pub(crate) fn mode_of(meta: &Metadata) -> Option<u32> {
    Some(meta.permissions().mode() & 0o7777)
}

pub(crate) fn write_mode_at(path: &Path, mode: u32) -> ClassOutcome {
    match std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)) {
        Ok(()) => ClassOutcome::Kept,
        Err(e) => {
            log::debug!(target: "metadata", "couldn't set mode {mode:o} on {}: {e}", LogPrivate::path(path));
            ClassOutcome::Refused
        }
    }
}

pub(crate) fn write_mode_staged(item: StagedItem<'_>, mode: u32) -> ClassOutcome {
    let permissions = std::fs::Permissions::from_mode(mode);
    let written = match item {
        StagedItem::File(file) => file.set_permissions(permissions),
        StagedItem::Path(path) => std::fs::set_permissions(path, permissions),
        StagedItem::Link(_) => return ClassOutcome::NotApplicable,
    };
    match written {
        Ok(()) => ClassOutcome::Kept,
        Err(e) => {
            log::debug!(target: "metadata", "couldn't set mode {mode:o}: {e}");
            super::super::staged::refusal(&e)
        }
    }
}

pub(crate) fn probe_umask() -> u32 {
    use std::os::unix::fs::DirBuilderExt as _;

    let nanos = crate::rt::wall_now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    let probe = std::env::temp_dir().join(format!(".cmdr-umask-probe-{}-{nanos}", std::process::id()));
    let made = std::fs::DirBuilder::new()
        .mode(0o777)
        .create(&probe)
        .and_then(|()| std::fs::metadata(&probe));
    let _ = std::fs::remove_dir(&probe);
    match made {
        Ok(meta) => !meta.permissions().mode() & 0o777,
        Err(e) => {
            let fallback = super::super::mode::FALLBACK_UMASK;
            log::debug!(target: "metadata", "couldn't probe the umask ({e}); assuming {fallback:03o}");
            fallback
        }
    }
}

pub(crate) fn local_xattrs(file: &File) -> Vec<Xattr> {
    use xattr::FileExt as _;
    match file.list_xattr() {
        Ok(names) => names
            .filter(super::carries_xattr)
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

pub(crate) fn local_xattr_names(file: &File) -> Vec<Xattr> {
    use xattr::FileExt as _;
    match file.list_xattr() {
        Ok(names) => names
            .filter(super::carries_xattr)
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

pub(crate) fn xattrs_at(path: &Path) -> Vec<Xattr> {
    match xattr::list(path) {
        Ok(names) => names
            .filter(super::carries_xattr)
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

pub(crate) fn write_xattrs(item: StagedItem<'_>, carried: &CarriedMetadata) -> ClassOutcome {
    use xattr::FileExt as _;
    if carried.xattrs.is_empty() {
        return ClassOutcome::Absent;
    }
    let mut outcome = ClassOutcome::Kept;
    for xattr in &carried.xattrs {
        let written = match item {
            StagedItem::File(file) => file.set_xattr(&xattr.name, &xattr.value),
            // `xattr::set` never follows a link, which is what both arms want.
            StagedItem::Path(path) | StagedItem::Link(path) => xattr::set(path, &xattr.name, &xattr.value),
        };
        if let Err(e) = written {
            // allowed-log-private: an xattr key (`com.apple.…`), the triage clue, not a file's name
            log::debug!(target: "metadata", "couldn't set xattr {:?}: {e}", xattr.name);
            outcome = super::super::staged::worse(outcome, super::super::staged::refusal(&e));
        }
    }
    outcome
}

pub(crate) fn open_parent_folder(child: &Path) -> ParentOpening {
    let Some(folder) = child.parent().filter(|p| !p.as_os_str().is_empty()) else {
        return ParentOpening::NothingToOpen;
    };
    let mode = match std::fs::symlink_metadata(folder) {
        Ok(meta) if meta.is_dir() => meta.permissions().mode() & 0o7777,
        _ => return ParentOpening::NothingToOpen,
    };
    let mut restore = match super::super::clear_blockers(folder) {
        Unblock::Cleared(cleared) => cleared,
        Unblock::NothingToClear => LandedMetadata::default(),
        Unblock::Refused => return ParentOpening::Refused,
    };
    const OWNER_WRITE_SEARCH: u32 = 0o300;
    if mode & OWNER_WRITE_SEARCH != OWNER_WRITE_SEARCH {
        super::super::note_helper_call();
        if let Err(e) = std::fs::set_permissions(folder, std::fs::Permissions::from_mode(mode | OWNER_WRITE_SEARCH)) {
            log::debug!(target: "metadata", "couldn't open {} for a removal: {e}", LogPrivate::dir_path(folder));
            super::super::apply_landed(folder, &restore);
            return ParentOpening::Refused;
        }
        restore = restore.with_mode(mode);
    }
    if restore.is_empty() {
        ParentOpening::NothingToOpen
    } else {
        ParentOpening::Opened(OpenedFolder::new(folder.to_path_buf(), restore))
    }
}
