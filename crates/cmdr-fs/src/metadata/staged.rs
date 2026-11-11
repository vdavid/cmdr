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

use super::{CarriedMetadata, ClassOutcome, MetadataReport, platform};

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
    let xattrs = platform::write_xattrs(item, carried);
    let mode = match (carried.mode, item) {
        (None, _) => ClassOutcome::Absent,
        (Some(_), StagedItem::Link(_)) => ClassOutcome::NotApplicable,
        (Some(mode), _) => platform::write_mode_staged(item, mode),
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

/// Birth, mtime, and atime in one call. Answers `(created, modified)`; atime
/// rides along and isn't reported.
fn write_dates(item: StagedItem<'_>, created: Option<SystemTime>, times: StagedTimes) -> (ClassOutcome, ClassOutcome) {
    let created_wanted = created.is_some();
    let created_settable = platform::CREATED_SETTABLE;
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
    let both = outcome(platform::set_dates(item, created, times));
    // A destination that refuses a birth date mustn't cost the mtime with it
    // (conflict logic and every listing read mtime), so a refused pair is
    // retried once without birth. Only a refusal pays the second call.
    let (created_outcome, dates_outcome) = match (created, both) {
        (Some(_), ClassOutcome::Refused | ClassOutcome::NotApplicable)
            if times.modified.is_some() || times.accessed.is_some() =>
        {
            (both, outcome(platform::set_dates(item, None, times)))
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

/// The mtime and atime of `times` as one `FileTimes` (the birth date, where an OS can set
/// one, is its arm's to add).
pub(super) fn file_times(times: StagedTimes) -> std::fs::FileTimes {
    let mut file_times = std::fs::FileTimes::new();
    if let Some(modified) = times.modified {
        file_times = file_times.set_modified(modified);
    }
    if let Some(accessed) = times.accessed {
        file_times = file_times.set_accessed(accessed);
    }
    file_times
}

/// `Unsupported` (and `ENOTSUP` on macOS) means the destination can't hold the
/// class at all, which is a different answer from refusing a value.
pub(super) fn refusal(e: &std::io::Error) -> ClassOutcome {
    if platform::is_unsupported(e) || e.kind() == std::io::ErrorKind::Unsupported {
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
pub(super) fn worse(a: ClassOutcome, b: ClassOutcome) -> ClassOutcome {
    use ClassOutcome::{Kept, NotApplicable, Refused};
    match (a, b) {
        (Refused, _) | (_, Refused) => Refused,
        (NotApplicable, _) | (_, NotApplicable) => NotApplicable,
        (Kept, other) | (other, Kept) => other,
        (a, _) => a,
    }
}
