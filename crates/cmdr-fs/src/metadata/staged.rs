//! The half of a copy's metadata that goes on BEFORE the landing rename: the
//! xattrs, then the mode, then the dates in one call.
//!
//! The order is forced. A path xattr write on a `0444` item is `EACCES`, so
//! xattrs go before the mode; a resource-fork write bumps mtime and APFS drags
//! birth down to an earlier mtime, so the dates go last and together (one
//! `fsetattrlist` / `setattrlist`). None of these refuses a rename, so all of
//! them may sit on a staged temp, and the rename carries them.

use std::time::SystemTime;

use super::{CarriedMetadata, ClassOutcome, MetadataReport, StagedItem, StagedTimes, platform};

/// Puts `carried` and `times` on the staged `item`: each xattr, then the mode,
/// then birth, mtime, and atime in ONE call. Each class is independent, so a
/// refused xattr never skips the mode and a refused time-set never undoes it.
///
/// The flags are NOT applied: they go on after the landing
/// ([`super::apply_landed`]), and this report reads `Absent` for them and for
/// the ACL.
pub fn apply_staged(item: StagedItem<'_>, carried: &CarriedMetadata, times: StagedTimes) -> MetadataReport {
    super::note_helper_call();
    let xattrs = platform::write_xattrs(item, &carried.xattrs);
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
            platform::refusal(&e)
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
