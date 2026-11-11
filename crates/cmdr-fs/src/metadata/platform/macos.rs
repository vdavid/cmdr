//! macOS: BSD user flags (`st_flags`, `lchflags`), ACLs read and written without following a
//! link, birth dates (`setattrlist` by path, `FileTimesExt` by fd), and the removal blockers
//! those flags and ACLs make.

use std::ffi::OsString;
use std::fs::Metadata;
use std::path::Path;
use std::time::SystemTime;

use super::super::{
    ClassOutcome, LandedMetadata, MetadataCaps, MetadataClasses, RemovalLock, StagedItem, StagedTimes, Unblock,
    UserFlags,
};
use crate::log_private::LogPrivate;

pub use super::unix::Acl;
pub(crate) use super::unix::{
    local_xattr_names, local_xattrs, mode_of, open_parent_folder, probe_umask, write_mode_at, write_mode_staged,
    write_xattrs, xattrs_at,
};

/// The xattr macOS stamps on every file a process writes. The kernel ignores a
/// write of it (verified on macOS 27.0, `xattr -w` then `-p` reads back the
/// writer's own value, 2026-10-11), so copying it costs a read and a write for
/// nothing: a copy never carries it.
const PROVENANCE_XATTR: &str = "com.apple.provenance";

/// Everything but provenance.
pub(crate) fn carries_xattr(name: &OsString) -> bool {
    name != PROVENANCE_XATTR
}

impl UserFlags {
    const BSD_BITS: [(UserFlags, u32); 4] = [
        (UserFlags::HIDDEN, libc::UF_HIDDEN),
        (UserFlags::LOCKED, libc::UF_IMMUTABLE),
        (UserFlags::APPEND_ONLY, libc::UF_APPEND),
        (UserFlags::NO_DUMP, libc::UF_NODUMP),
    ];

    /// The set's bits out of a raw `st_flags` word; every other bit is ignored.
    pub(in crate::metadata) fn from_bsd(raw: u32) -> Self {
        Self::BSD_BITS
            .iter()
            .filter(|(_, bit)| raw & bit != 0)
            .fold(Self::NONE, |set, (flag, _)| set | *flag)
    }

    /// The raw `st_flags` bits of this set.
    pub(in crate::metadata) fn to_bsd(self) -> u32 {
        Self::BSD_BITS
            .iter()
            .filter(|(flag, _)| self.contains(*flag))
            .fold(0, |raw, (_, bit)| raw | bit)
    }
}

/// `path`'s own flags word (never a symlink's target's).
fn read_raw(path: &Path) -> std::io::Result<u32> {
    use std::os::macos::fs::MetadataExt as _;
    std::fs::symlink_metadata(path).map(|meta| meta.st_flags())
}

// `lchflags(2)` is in libSystem, but the `libc` this workspace locks doesn't
// declare it yet.
unsafe extern "C" {
    fn lchflags(path: *const libc::c_char, flags: libc::c_uint) -> libc::c_int;
}

/// Sets `path`'s own flags word to exactly `raw` (`lchflags`, never following a link).
fn write_raw(path: &Path, raw: u32) -> std::io::Result<()> {
    use std::os::unix::ffi::OsStrExt as _;
    let c_path = std::ffi::CString::new(path.as_os_str().as_bytes())
        .map_err(|_| std::io::Error::from(std::io::ErrorKind::InvalidInput))?;
    // SAFETY: `c_path` is a NUL-terminated path that outlives the call, `lchflags`
    // reads nothing else, and the declaration above matches its libSystem signature.
    if unsafe { lchflags(c_path.as_ptr(), raw) } == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

pub(crate) fn flags_of(meta: &Metadata) -> UserFlags {
    use std::os::macos::fs::MetadataExt as _;
    UserFlags::from_bsd(meta.st_flags())
}

pub(crate) fn add_flags(path: &Path, flags: UserFlags) -> ClassOutcome {
    match read_raw(path).and_then(|raw| write_raw(path, raw | flags.to_bsd())) {
        Ok(()) => ClassOutcome::Kept,
        Err(e) => {
            log::debug!(target: "metadata", "couldn't set the flags on {}: {e}", LogPrivate::path(path));
            ClassOutcome::Refused
        }
    }
}

pub(crate) fn read_acl(path: &Path) -> Option<Acl> {
    match exacl::getfacl(path, exacl::AclOption::SYMLINK_ACL) {
        Ok(entries) if !entries.is_empty() => Some(Acl(entries)),
        Ok(_) => None,
        Err(e) => {
            log::debug!(target: "metadata", "couldn't read the ACL of {}: {e}", LogPrivate::path(path));
            None
        }
    }
}

pub(crate) fn write_acl(path: &Path, acl: &Acl) -> ClassOutcome {
    match exacl::setfacl(&[path], &acl.0, exacl::AclOption::SYMLINK_ACL) {
        Ok(()) => ClassOutcome::Kept,
        Err(e) => {
            log::debug!(target: "metadata", "couldn't set the ACL on {}: {e}", LogPrivate::path(path));
            ClassOutcome::Refused
        }
    }
}

pub(crate) fn clear_blockers(path: &Path) -> Unblock {
    super::super::note_helper_call();
    let Ok(raw) = read_raw(path) else {
        return Unblock::NothingToClear;
    };
    if raw & (libc::SF_IMMUTABLE | libc::SF_APPEND) != 0 {
        return Unblock::Refused;
    }
    let mut cleared = LandedMetadata::default();
    let blocking = UserFlags::from_bsd(raw).intersection(UserFlags::RENAME_BLOCKERS);
    if !blocking.is_empty() {
        if let Err(e) = write_raw(path, raw & !blocking.to_bsd()) {
            log::debug!(target: "metadata", "couldn't unlock {}: {e}", LogPrivate::path(path));
            return Unblock::Refused;
        }
        cleared.flags = blocking;
    }
    match exacl::getfacl(path, exacl::AclOption::SYMLINK_ACL) {
        Ok(entries) if !entries.is_empty() => {
            if let Err(e) = exacl::setfacl(&[path], &[], exacl::AclOption::SYMLINK_ACL) {
                log::debug!(target: "metadata", "couldn't strip the ACL of {}: {e}", LogPrivate::path(path));
                return Unblock::Refused;
            }
            cleared.acl = Some(Acl(entries));
        }
        Ok(_) => {}
        Err(e) => log::debug!(target: "metadata", "couldn't read the ACL of {}: {e}", LogPrivate::path(path)),
    }
    if cleared.is_empty() {
        Unblock::NothingToClear
    } else {
        Unblock::Cleared(cleared)
    }
}

pub(crate) fn removal_lock(path: &Path) -> RemovalLock {
    let Ok(raw) = read_raw(path) else {
        return RemovalLock::Free;
    };
    lock_of(raw, || {
        exacl::getfacl(path, exacl::AclOption::SYMLINK_ACL)
            .map(|entries| entries.iter().any(denies_removal))
            .unwrap_or(false)
    })
}

/// A removal lock's verdict from a raw `st_flags` word, asking `acl_denies`
/// only when the flags alone don't decide it (it's a syscall).
pub(crate) fn lock_of(raw: u32, acl_denies: impl FnOnce() -> bool) -> RemovalLock {
    if raw & (libc::SF_IMMUTABLE | libc::SF_APPEND) != 0 {
        RemovalLock::SystemLocked
    } else if !UserFlags::from_bsd(raw)
        .intersection(UserFlags::RENAME_BLOCKERS)
        .is_empty()
        || acl_denies()
    {
        RemovalLock::Locked
    } else {
        RemovalLock::Free
    }
}

/// Whether one ACL entry refuses removing the item it's on, or anything out of
/// it. Any principal counts: telling whether `everyone` or a group reaches this
/// process would cost more than the rare extra question.
fn denies_removal(entry: &exacl::AclEntry) -> bool {
    !entry.allow
        && !entry.flags.contains(exacl::Flag::ONLY_INHERIT)
        && entry.perms.intersects(exacl::Perm::DELETE | exacl::Perm::DELETE_CHILD)
}

/// macOS keeps every class, on files and folders.
pub const LOCAL_DISK_CAPS: MetadataCaps = MetadataCaps {
    stores: MetadataClasses::ALL,
    reports: MetadataClasses::ALL,
    folder_stores: MetadataClasses::ALL,
    folder_dates: true,
    staged_flags: UserFlags::NONE,
};

/// macOS keeps a birth date a copy can set.
pub(crate) const CREATED_SETTABLE: bool = true;

pub(crate) fn set_dates(item: StagedItem<'_>, created: Option<SystemTime>, times: StagedTimes) -> std::io::Result<()> {
    match item {
        StagedItem::File(file) => file.set_times(file_times(created, times)),
        StagedItem::Path(path) => set_at_path(path, created, times, Follow::Yes),
        StagedItem::Link(path) => set_at_path(path, created, times, Follow::No),
    }
}

/// One `FileTimes`, which std issues as one `fsetattrlist`, birth included.
fn file_times(created: Option<SystemTime>, times: StagedTimes) -> std::fs::FileTimes {
    use std::os::darwin::fs::FileTimesExt as _;
    let mut file_times = super::super::staged::file_times(times);
    if let Some(created) = created {
        file_times = file_times.set_created(created);
    }
    file_times
}

/// `ENOTSUP` means the destination can't hold the class at all.
pub(crate) fn is_unsupported(e: &std::io::Error) -> bool {
    e.raw_os_error() == Some(libc::ENOTSUP)
}

/// Whether `setattrlist` follows a symlink.
#[derive(Clone, Copy)]
enum Follow {
    Yes,
    No,
}

/// `setattrlist` by path: the only way to set a folder's or a symlink's birth
/// date, since std's path-taking setter is unstable and an fd can't be had for a
/// link.
fn set_at_path(path: &Path, created: Option<SystemTime>, times: StagedTimes, follow: Follow) -> std::io::Result<()> {
    use std::os::unix::ffi::OsStrExt as _;

    // The buffer holds the requested times in attribute-bit order: CRTIME
    // (0x200), MODTIME (0x400), ACCTIME (0x1000).
    let mut commonattr = 0;
    let mut buffer = [libc::timespec { tv_sec: 0, tv_nsec: 0 }; 3];
    let mut count = 0;
    for (bit, time) in [
        (libc::ATTR_CMN_CRTIME, created),
        (libc::ATTR_CMN_MODTIME, times.modified),
        (libc::ATTR_CMN_ACCTIME, times.accessed),
    ] {
        if let Some(time) = time {
            commonattr |= bit;
            buffer[count] = timespec(time)?;
            count += 1;
        }
    }
    let mut attrs = libc::attrlist {
        bitmapcount: libc::ATTR_BIT_MAP_COUNT,
        reserved: 0,
        commonattr,
        volattr: 0,
        dirattr: 0,
        fileattr: 0,
        forkattr: 0,
    };
    let options = match follow {
        Follow::Yes => 0,
        Follow::No => libc::FSOPT_NOFOLLOW,
    };
    let c_path = std::ffi::CString::new(path.as_os_str().as_bytes())
        .map_err(|_| std::io::Error::from(std::io::ErrorKind::InvalidInput))?;
    // SAFETY: `c_path` is a NUL-terminated path that outlives the call;
    // `attrs` names exactly the `count` timespecs at the front of `buffer`,
    // in bit order, and the size passed covers exactly those.
    let rc = unsafe {
        libc::setattrlist(
            c_path.as_ptr(),
            (&raw mut attrs).cast(),
            buffer.as_mut_ptr().cast(),
            count * size_of::<libc::timespec>(),
            options,
        )
    };
    if rc == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

fn timespec(time: SystemTime) -> std::io::Result<libc::timespec> {
    let invalid = || std::io::Error::from(std::io::ErrorKind::InvalidInput);
    let (secs, nanos) = match time.duration_since(SystemTime::UNIX_EPOCH) {
        Ok(after) => (
            i64::try_from(after.as_secs()).map_err(|_| invalid())?,
            after.subsec_nanos(),
        ),
        Err(before) => {
            let before = before.duration();
            let secs = i64::try_from(before.as_secs()).map_err(|_| invalid())?;
            match before.subsec_nanos() {
                0 => (-secs, 0),
                nanos => (-secs - 1, 1_000_000_000 - nanos),
            }
        }
    };
    Ok(libc::timespec {
        tv_sec: secs,
        tv_nsec: i64::from(nanos),
    })
}
