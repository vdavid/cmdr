//! MTP dates ↔ Unix seconds, both ways, over mtp-rs's `DateTime` calendar math.
//!
//! An upload sends UTC with a `Z` (Android reads it as UTC). A read honors a
//! date's own offset, and reads a zoneless one (what Android sends: the phone's
//! local wall clock) as the Mac's local time, through
//! [`convert_mtp_datetime_in`].

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use jiff::tz::TimeZone;
use mtp_rs::{DateTime, UtcOffset};

/// A device `DateTime` as Unix seconds, reading a zoneless one in the Mac's own
/// zone. `None` before 1970, which the `FileEntry` vocabulary can't hold, and
/// for fields that don't form a real date (mtp-rs already drops most of those
/// at parse time).
pub(super) fn convert_mtp_datetime(dt: DateTime) -> Option<u64> {
    // `system()` is jiff's cached copy of the Mac's zone, so a listing pays no
    // file read per entry, and a zone change reaches it within seconds.
    convert_mtp_datetime_in(dt, &TimeZone::system())
}

/// [`convert_mtp_datetime`] with the zone a zoneless date reads in passed in, so
/// a test needn't depend on the machine's.
///
/// A phone writes its own wall clock, and the phone and the Mac almost always
/// share a zone, so the Mac's zone is the best guess at what the phone meant.
/// The offset is the one THAT date had there, ❌ never today's: a summer photo
/// listed in winter keeps its summer offset. A wall-clock time the clocks
/// passed twice (the fall-back hour) reads as the earlier instant, and one they
/// skipped (spring forward) shifts forward by the gap, the way a person's clock
/// app would have shown it (jiff's "compatible" disambiguation). A date that
/// names its own offset keeps it.
fn convert_mtp_datetime_in(dt: DateTime, zone: &TimeZone) -> Option<u64> {
    let secs = if dt.offset.is_some() {
        dt.to_unix_seconds()?
    } else {
        // jiff validates the fields the way mtp-rs does (real month lengths,
        // leap years), so impossible ones answer `None` here too.
        let wall_clock = jiff::civil::DateTime::new(
            i16::try_from(dt.year).ok()?,
            i8::try_from(dt.month).ok()?,
            i8::try_from(dt.day).ok()?,
            i8::try_from(dt.hour).ok()?,
            i8::try_from(dt.minute).ok()?,
            i8::try_from(dt.second).ok()?,
            0,
        )
        .ok()?;
        zone.to_ambiguous_timestamp(wall_clock).compatible().ok()?.as_second()
    };
    u64::try_from(secs).ok()
}

/// A device `DateTime` as the instant a read stream reports.
pub(crate) fn system_time_from_mtp_datetime(dt: DateTime) -> Option<SystemTime> {
    convert_mtp_datetime(dt).map(|secs| UNIX_EPOCH + Duration::from_secs(secs))
}

/// The `DateModified` an upload sends for a file last changed at `date`, in
/// whole seconds of UTC. `None` for a date PTP can't spell (before 1970, or past
/// year 9999), so the device stamps its own.
pub(crate) fn mtp_datetime_from_system_time(date: SystemTime) -> Option<DateTime> {
    let secs = i64::try_from(date.duration_since(UNIX_EPOCH).ok()?.as_secs()).ok()?;
    DateTime::from_unix_seconds(secs, UtcOffset::UTC)
}

#[cfg(test)]
#[path = "dates_test.rs"]
mod dates_test;
