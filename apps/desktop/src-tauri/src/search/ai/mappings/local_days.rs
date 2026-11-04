//! What "a day" means to the person searching: their LOCAL calendar.
//!
//! "Today", "last week", and "2024" are the user's days, so every boundary the time
//! mapping computes is a local midnight, and "today" is the local date. Reading them
//! in UTC put "today" one to two hours off in Stockholm, and handed the model
//! yesterday's date between local and UTC midnight.
//!
//! A trait rather than a direct clock read, so a test can pin a zone (and a DST
//! change) on any machine, and so "now" arrives as a value the day the mapping
//! leaves the app crate.

use chrono::{LocalResult, NaiveDate, TimeZone};
use time::{Date, Month};

/// The user's calendar: today's date, and the instant a given date starts.
pub(crate) trait LocalDays {
    /// Today's date in the user's zone.
    fn today(&self) -> Date;
    /// Seconds since the Unix epoch at the first instant of `date` in the user's zone.
    fn midnight(&self, date: Date) -> u64;
}

/// The machine's own zone, through `chrono::Local` (which resolves the offset per
/// date, so a boundary across a DST change lands on that day's real midnight).
pub(crate) struct SystemZone;

impl LocalDays for SystemZone {
    fn today(&self) -> Date {
        from_naive(chrono::Local::now().date_naive())
    }

    fn midnight(&self, date: Date) -> u64 {
        let naive = to_naive(date);
        // A zone that skips midnight (a DST jump at 00:00) starts that day at the
        // first hour that exists; an ambiguous midnight starts at the earlier one.
        let start = (0..3).find_map(|hour| {
            let local = naive.and_hms_opt(hour, 0, 0)?;
            match chrono::Local.from_local_datetime(&local) {
                LocalResult::Single(t) => Some(t),
                LocalResult::Ambiguous(earliest, _) => Some(earliest),
                LocalResult::None => None,
            }
        });
        match start {
            Some(t) => t.timestamp().max(0) as u64,
            // No zone skips three hours; fall back to the date's UTC midnight.
            None => date.midnight().assume_utc().unix_timestamp().max(0) as u64,
        }
    }
}

fn to_naive(date: Date) -> NaiveDate {
    NaiveDate::from_ymd_opt(date.year(), u8::from(date.month()).into(), date.day().into())
        .expect("a `time::Date` is always a valid calendar date")
}

fn from_naive(date: NaiveDate) -> Date {
    use chrono::Datelike;
    let month = Month::try_from(date.month() as u8).expect("chrono months are 1-12");
    Date::from_calendar_date(date.year(), month, date.day() as u8).expect("chrono dates are valid calendar dates")
}

/// A fixed-offset calendar for tests, with an optional DST-style switch: dates on or
/// after `switch_on` use `offset_after_secs`.
#[cfg(test)]
pub(crate) struct FixedZone {
    pub today: Date,
    pub offset_secs: i64,
    pub switch_on: Option<(Date, i64)>,
}

#[cfg(test)]
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
}
