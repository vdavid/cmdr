//! What "a day" means to the person searching: their LOCAL calendar.
//!
//! "Today", "last week", and "2024" are the user's days, so every boundary the time
//! mapping computes is a local midnight, and "today" is the local date. Reading them
//! in UTC put "today" one to two hours off in Stockholm, and handed the model
//! yesterday's date between local and UTC midnight.
//!
//! A trait rather than a clock read: the crate reads no clock and knows no zone, so
//! the host answers it (the desktop's `SystemZone`, through the machine's zone), and
//! a test pins a zone (and a DST change) on any machine.

use time::Date;

/// The user's calendar: today's date, the instant a given date starts, and the date
/// an instant falls on.
pub trait LocalDays {
    /// Today's date in the user's zone.
    fn today(&self) -> Date;
    /// Seconds since the Unix epoch at the first instant of `date` in the user's zone.
    fn midnight(&self, date: Date) -> u64;
    /// The date in the user's zone at `at` (seconds since the Unix epoch). What a
    /// boundary reads as to a person: `midnight(d)` falls on `d` in every zone.
    fn date_of(&self, at: u64) -> Date;
}

/// A fixed-offset calendar for tests, with an optional DST-style switch: dates on or
/// after `switch_on` use its offset.
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

    fn date_of(&self, at: u64) -> Date {
        let on_offset = |offset: i64| {
            time::OffsetDateTime::from_unix_timestamp(at as i64 + offset)
                .expect("a test instant is in range")
                .date()
        };
        match self.switch_on {
            Some((on, after)) if on_offset(after) >= on => on_offset(after),
            _ => on_offset(self.offset_secs),
        }
    }
}
