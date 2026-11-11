//! The half of a copy's metadata that goes on BEFORE the landing rename: the
//! xattrs, then the mode, then the dates in one call.
//!
//! The order is forced. A path xattr write on a `0444` item is `EACCES`, so
//! xattrs go before the mode; a resource-fork write bumps mtime and APFS drags
//! birth down to an earlier mtime, so the dates go last and together (one
//! `fsetattrlist` / `setattrlist`). None of these refuses a rename, so all of
//! them may sit on a staged temp, and the rename carries them.

use std::fs::File;
use std::path::Path;
use std::time::SystemTime;

use super::{CarriedMetadata, ClassOutcome, MetadataReport};

/// What [`apply_staged`] writes to.
#[derive(Debug, Clone, Copy)]
pub enum StagedItem<'a> {
    /// A file the copy holds open: every step goes through its fd, so there's
    /// no path lookup, and an fd write on a `0444` file isn't refused the way a
    /// path write is.
    File(&'a File),
    /// An item with no fd to hand (a folder). Follows nothing: a folder isn't a
    /// link.
    Path(&'a Path),
    /// A symlink's OWN metadata (`XATTR_NOFOLLOW`, `FSOPT_NOFOLLOW`), never its
    /// target's. Takes no mode. Its dates are macOS-only: Linux would need
    /// `utimensat(AT_SYMLINK_NOFOLLOW)`, and they answer `NotApplicable` there.
    Link(&'a Path),
}

/// The dates [`apply_staged`] sets beside [`CarriedMetadata::created`], in the
/// same call. `None` leaves that date as the filesystem stamped it.
// DEFAULT-OK: "set neither", which leaves the item's own dates alone.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StagedTimes {
    /// The modification date: a stream's `modified_at`, a local source's mtime.
    pub modified: Option<SystemTime>,
    /// The access date, carried only where it costs nothing extra (a local
    /// source's stat). Never reported.
    pub accessed: Option<SystemTime>,
}

/// Puts `carried` and `times` on the staged `item`: each xattr, then the mode,
/// then birth, mtime, and atime in ONE call. Each class is independent, so a
/// refused xattr never skips the mode and a refused time-set never undoes it.
///
/// The flags are NOT applied: they go on after the landing
/// ([`super::apply_landed`]), and this report reads `Absent` for them and for
/// the ACL.
pub fn apply_staged(item: StagedItem<'_>, carried: &CarriedMetadata, times: StagedTimes) -> MetadataReport {
    super::note_helper_call();
    let xattrs = write_xattrs(item, carried);
    let mode = match (carried.mode, item) {
        (None, _) => ClassOutcome::Absent,
        (Some(_), StagedItem::Link(_)) => ClassOutcome::NotApplicable,
        (Some(mode), _) => write_mode(item, mode),
    };
    let (created, modified) = write_dates(item, carried.created, times);
    MetadataReport {
        xattrs,
        mode,
        created,
        modified,
        ..MetadataReport::ABSENT
    }
}

// The browser build has no host filesystem to put xattrs or a mode on
// (`DETAILS.md` § "The browser build").
#[cfg(target_family = "wasm")]
fn write_xattrs(_item: StagedItem<'_>, carried: &CarriedMetadata) -> ClassOutcome {
    if carried.xattrs.is_empty() {
        ClassOutcome::Absent
    } else {
        ClassOutcome::NotApplicable
    }
}

#[cfg(target_family = "wasm")]
fn write_mode(_item: StagedItem<'_>, _mode: u32) -> ClassOutcome {
    ClassOutcome::NotApplicable
}

#[cfg(unix)]
fn write_xattrs(item: StagedItem<'_>, carried: &CarriedMetadata) -> ClassOutcome {
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
            outcome = worse(outcome, refusal(&e));
        }
    }
    outcome
}

#[cfg(unix)]
fn write_mode(item: StagedItem<'_>, mode: u32) -> ClassOutcome {
    use std::os::unix::fs::PermissionsExt as _;
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
            refusal(&e)
        }
    }
}

/// Birth, mtime, and atime in one call. Answers `(created, modified)`; atime
/// rides along and isn't reported.
fn write_dates(item: StagedItem<'_>, created: Option<SystemTime>, times: StagedTimes) -> (ClassOutcome, ClassOutcome) {
    let created_wanted = created.is_some();
    let created_settable = cfg!(target_os = "macos");
    let created = if created_settable { created } else { None };
    if created.is_none() && times.modified.is_none() && times.accessed.is_none() {
        let created_outcome = if created_wanted {
            ClassOutcome::NotApplicable
        } else {
            ClassOutcome::Absent
        };
        return (created_outcome, ClassOutcome::Absent);
    }
    let outcome = |written: std::io::Result<()>| match written {
        Ok(()) => ClassOutcome::Kept,
        Err(e) => {
            log::debug!(target: "metadata", "couldn't set the dates: {e}");
            refusal(&e)
        }
    };
    let both = outcome(set_dates(item, created, times));
    // A destination that refuses a birth date mustn't cost the mtime with it
    // (conflict logic and every listing read mtime), so a refused pair is
    // retried once without birth. Only a refusal pays the second call.
    let (created_outcome, dates_outcome) = match (created, both) {
        (Some(_), ClassOutcome::Refused | ClassOutcome::NotApplicable)
            if times.modified.is_some() || times.accessed.is_some() =>
        {
            (both, outcome(set_dates(item, None, times)))
        }
        _ => (both, both),
    };
    let created_outcome = match (created_wanted, created_settable) {
        (false, _) => ClassOutcome::Absent,
        (true, false) => ClassOutcome::NotApplicable,
        (true, true) => created_outcome,
    };
    let modified_outcome = if times.modified.is_some() {
        dates_outcome
    } else {
        ClassOutcome::Absent
    };
    (created_outcome, modified_outcome)
}

fn set_dates(item: StagedItem<'_>, created: Option<SystemTime>, times: StagedTimes) -> std::io::Result<()> {
    match item {
        StagedItem::File(file) => file.set_times(file_times(created, times)),
        #[cfg(target_os = "macos")]
        StagedItem::Path(path) => macos_dates::set_at_path(path, created, times, macos_dates::Follow::Yes),
        #[cfg(target_os = "macos")]
        StagedItem::Link(path) => macos_dates::set_at_path(path, created, times, macos_dates::Follow::No),
        // std's path setter is unstable, so a folder is opened for its fd.
        #[cfg(any(target_os = "linux", target_family = "wasm"))]
        StagedItem::Path(path) => File::open(path)?.set_times(file_times(created, times)),
        #[cfg(any(target_os = "linux", target_family = "wasm"))]
        StagedItem::Link(_) => Err(std::io::ErrorKind::Unsupported.into()),
    }
}

/// One `FileTimes`, which std issues as one `fsetattrlist` on macOS (birth
/// included) and one `futimens` on Linux.
fn file_times(created: Option<SystemTime>, times: StagedTimes) -> std::fs::FileTimes {
    let mut file_times = std::fs::FileTimes::new();
    if let Some(modified) = times.modified {
        file_times = file_times.set_modified(modified);
    }
    if let Some(accessed) = times.accessed {
        file_times = file_times.set_accessed(accessed);
    }
    #[cfg(target_os = "macos")]
    if let Some(created) = created {
        use std::os::darwin::fs::FileTimesExt as _;
        file_times = file_times.set_created(created);
    }
    #[cfg(any(target_os = "linux", target_family = "wasm"))]
    let _ = created;
    file_times
}

/// `Unsupported` (and `ENOTSUP` on macOS) means the destination can't hold the
/// class at all, which is a different answer from refusing a value.
fn refusal(e: &std::io::Error) -> ClassOutcome {
    #[cfg(target_os = "macos")]
    if e.raw_os_error() == Some(libc::ENOTSUP) {
        return ClassOutcome::NotApplicable;
    }
    if e.kind() == std::io::ErrorKind::Unsupported {
        ClassOutcome::NotApplicable
    } else {
        ClassOutcome::Refused
    }
}

/// The outcome that says more about what the copy lost: a refusal outranks a
/// class the destination can't hold, which outranks a kept one.
#[cfg_attr(
    target_family = "wasm",
    allow(dead_code, reason = "the browser build writes no xattrs")
)]
fn worse(a: ClassOutcome, b: ClassOutcome) -> ClassOutcome {
    use ClassOutcome::{Kept, NotApplicable, Refused};
    match (a, b) {
        (Refused, _) | (_, Refused) => Refused,
        (NotApplicable, _) | (_, NotApplicable) => NotApplicable,
        (Kept, other) | (other, Kept) => other,
        (a, _) => a,
    }
}

/// `setattrlist` by path: the only way to set a folder's or a symlink's birth
/// date, since std's path-taking setter is unstable and an fd can't be had for a
/// link.
#[cfg(target_os = "macos")]
mod macos_dates {
    use std::path::Path;
    use std::time::SystemTime;

    use super::StagedTimes;

    /// Whether the call follows a symlink.
    #[derive(Clone, Copy)]
    pub(super) enum Follow {
        Yes,
        No,
    }

    pub(super) fn set_at_path(
        path: &Path,
        created: Option<SystemTime>,
        times: StagedTimes,
        follow: Follow,
    ) -> std::io::Result<()> {
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
}
