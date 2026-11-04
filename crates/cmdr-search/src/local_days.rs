//! What "a day" and "a time" mean to the person reading: their LOCAL calendar.
//!
//! "Today", "last week", and "2024" are the user's days, so every boundary the time
//! mapping computes is a local midnight, and "today" is the local date. Reading them
//! in UTC put "today" one to two hours off in Stockholm, and handed the model
//! yesterday's date between local and UTC midnight. The same goes for every date the
//! app shows a person or the model (`LocalDays::format_date` and `format_datetime`):
//! a file changed at 00:30 in Stockholm read as the day before in UTC.
//!
//! A trait rather than a clock read: the crate reads no clock and knows no zone, so
//! the host answers it (the desktop's `SystemZone`, through the machine's zone), and
//! a test pins a zone (and a DST change) on any machine.

use time::{Date, OffsetDateTime, UtcOffset};

/// The user's calendar: today's date, the instant a given date starts, and the
/// offset in force at a given instant.
pub trait LocalDays {
    /// Today's date in the user's zone.
    fn today(&self) -> Date;
    /// Seconds since the Unix epoch at the first instant of `date` in the user's zone.
    fn midnight(&self, date: Date) -> u64;
    /// Seconds east of UTC in the user's zone at `at` (seconds since the Unix epoch),
    /// with that instant's summer time applied.
    fn offset_at(&self, at: u64) -> i64;

    /// The local wall clock at `at`, carrying that instant's offset.
    fn local_at(&self, at: u64) -> OffsetDateTime {
        let utc =
            OffsetDateTime::from_unix_timestamp(at.min(i64::MAX as u64) as i64).unwrap_or(OffsetDateTime::UNIX_EPOCH);
        // A real zone's offset is under ±26 h, which `UtcOffset` always holds; an
        // out-of-range answer reads as UTC rather than panicking.
        let offset = i32::try_from(self.offset_at(at))
            .ok()
            .and_then(|secs| UtcOffset::from_whole_seconds(secs).ok())
            .unwrap_or(UtcOffset::UTC);
        utc.checked_to_offset(offset).unwrap_or(utc)
    }

    /// The date in the user's zone at `at` (seconds since the Unix epoch). What a
    /// boundary reads as to a person: `midnight(d)` falls on `d` in every zone.
    fn date_of(&self, at: u64) -> Date {
        self.local_at(at).date()
    }

    /// `at` as a person or the model reads a date: `YYYY-MM-DD` on this calendar, so
    /// a file changed at 00:30 in Stockholm reads as that day, not UTC's day before.
    /// No time of day, so no offset. ❌ Don't override: every surface shares this shape.
    fn format_date(&self, at: u64) -> String {
        let format = time::macros::format_description!("[year]-[month]-[day]");
        self.local_at(at).format(&format).unwrap_or_else(|_| at.to_string())
    }

    /// `at` as a person or the model reads a time of day: this calendar's wall clock
    /// with the offset in force then, RFC 3339 to the second
    /// (`2026-03-29T14:00:00+02:00`). The offset keeps it unambiguous across a
    /// summer-time change and comparable with another zone's stamp; `+00:00` rather
    /// than `Z` in UTC, so every stamp has one shape. ❌ Don't override.
    fn format_datetime(&self, at: u64) -> String {
        let format = time::macros::format_description!(
            "[year]-[month]-[day]T[hour]:[minute]:[second][offset_hour sign:mandatory]:[offset_minute]"
        );
        self.local_at(at).format(&format).unwrap_or_else(|_| at.to_string())
    }
}

/// A fixed-offset calendar for tests, with an optional DST-style switch: instants on
/// or after the start of `switch_on` (in its new offset) use that offset.
#[cfg(any(test, feature = "testing"))]
pub struct FixedZone {
    /// The calendar's "today".
    pub today: Date,
    /// Seconds east of UTC.
    pub offset_secs: i64,
    /// From this date on, this offset instead.
    pub switch_on: Option<(Date, i64)>,
}

#[cfg(any(test, feature = "testing"))]
impl LocalDays for FixedZone {
    fn today(&self) -> Date {
        self.today
    }

    fn midnight(&self, date: Date) -> u64 {
        let offset = match self.switch_on {
            Some((on, after)) if date >= on => after,
            _ => self.offset_secs,
        };
        (date.midnight().assume_utc().unix_timestamp() - offset).max(0) as u64
    }

    fn offset_at(&self, at: u64) -> i64 {
        match self.switch_on {
            Some((on, after)) if at >= self.midnight(on) => after,
            _ => self.offset_secs,
        }
    }
}
