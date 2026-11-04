//! This machine's calendar, the one zone every date a person or the model reads is in.
//!
//! `cmdr-search` reads no clock and knows no zone (`cmdr_search::translate::LocalDays`);
//! this is the host's answer, through `chrono::Local`. The formatters here are the
//! app's only date renderings for the model: `format_local_date`
//! (`YYYY-MM-DD`, for the `*Human` and `asOf` fields) and `format_local_datetime`
//! (RFC 3339 with the local offset, for a field that shows a time of day).

use chrono::{LocalResult, NaiveDate, Offset, TimeZone};
use time::{Date, Month};

use cmdr_search::translate::LocalDays;
use cmdr_search::{SearchQuery, query};

/// The machine's own zone, through `chrono::Local` (which resolves the offset per
/// instant, so a boundary or a stamp across a DST change takes that day's offset).
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

    fn offset_at(&self, at: u64) -> i64 {
        let utc = chrono::DateTime::from_timestamp(at.min(i64::MAX as u64) as i64, 0).unwrap_or_default();
        i64::from(utc.with_timezone(&chrono::Local).offset().fix().local_minus_utc())
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

/// `YYYY-MM-DD` on this machine's calendar (`LocalDays::format_date`).
pub(crate) fn format_local_date(ts: u64) -> String {
    SystemZone.format_date(ts)
}

/// This machine's wall clock with its offset, `2026-03-29T14:00:00+02:00`
/// (`LocalDays::format_datetime`).
pub(crate) fn format_local_datetime(ts: u64) -> String {
    SystemZone.format_datetime(ts)
}

/// A query's summary, its dates on this machine's calendar (MCP's `interpretedQuery`).
pub(crate) fn summarize_query(q: &SearchQuery) -> String {
    query::summarize_query(q, &SystemZone)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A day's first instant reads back as that day here, so a chip shows the date its
    /// boundary was computed from. Bites on any machine east of UTC.
    #[test]
    fn a_local_midnight_reads_back_as_its_own_date() {
        let today = SystemZone.today();
        assert_eq!(SystemZone.date_of(SystemZone.midnight(today)), today);
        let next = today.next_day().expect("not the last day");
        assert_eq!(SystemZone.date_of(SystemZone.midnight(next) - 1), today);
    }

    /// The offset is this machine's at that instant, the one `chrono::Local` stamps.
    /// Bites on any machine whose zone isn't UTC (the crate's fixed-zone tests pin the
    /// format and the west, east, and summer-time cases everywhere).
    #[test]
    fn a_stamp_carries_this_machines_offset_then() {
        for at in [1_768_447_800_u64, 1_784_000_000] {
            let expected = chrono::DateTime::from_timestamp(at as i64, 0)
                .expect("in range")
                .with_timezone(&chrono::Local)
                .to_rfc3339_opts(chrono::SecondsFormat::Secs, false);
            // chrono writes UTC's offset as `+00:00` too with `use_z: false`.
            assert_eq!(format_local_datetime(at), expected);
            assert_eq!(format_local_date(at), expected[..10]);
        }
    }
}
