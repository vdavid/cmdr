use time::{Date, Month};

use super::super::parser::{is_range, is_year};
use crate::local_days::LocalDays;

// ── Time mapping ─────────────────────────────────────────────────────

/// Map a `time` enum value to a (modified_after, modified_before) timestamp range,
/// on the user's calendar `days`. Every boundary is a LOCAL midnight
/// (`crate::local_days` says why).
///
/// Returns `(None, None)` for unrecognized values.
pub fn time_to_range(t: &str, days: &impl LocalDays) -> (Option<u64>, Option<u64>) {
    let today = days.today();
    let at = |date: Date| Some(days.midnight(date));
    match t {
        "today" => (at(today), None),
        "yesterday" => (at(today.previous_day().unwrap_or(today)), at(today)),
        "this_week" => (at(monday_of_week(today)), None),
        "last_week" => (at(days_before(monday_of_week(today), 7)), at(monday_of_week(today))),
        "this_month" => (at(first_of_month(today)), None),
        "last_month" => (at(first_of_last_month(today)), at(first_of_month(today))),
        "this_quarter" => (at(first_of_quarter(today)), None),
        "last_quarter" => (at(first_of_last_quarter(today)), at(first_of_quarter(today))),
        "this_year" => (at(jan1(today, 0)), None),
        "last_year" => (at(jan1(today, -1)), at(jan1(today, 0))),
        "recent" => (at(n_months_ago(today, 3)), None),
        "last_3_months" => (at(n_months_ago(today, 3)), None),
        "last_6_months" => (at(n_months_ago(today, 6)), None),
        "old" => (None, at(one_year_ago(today))),
        _ => {
            if is_year(t) {
                year_range(t, days)
            } else if is_range(t) {
                parse_date_range(t, days)
            } else {
                (None, None)
            }
        }
    }
}

fn days_before(date: Date, days: i64) -> Date {
    date.checked_sub(time::Duration::days(days)).unwrap_or(date)
}

fn monday_of_week(date: Date) -> Date {
    let days_since_monday = date.weekday().number_from_monday() - 1; // Monday=1, Sunday=7
    days_before(date, days_since_monday.into())
}

fn first_of_month(date: Date) -> Date {
    date.replace_day(1).unwrap_or(date)
}

fn first_of_last_month(date: Date) -> Date {
    let (year, month) = if date.month() == Month::January {
        (date.year() - 1, Month::December)
    } else {
        (date.year(), date.month().previous())
    };
    Date::from_calendar_date(year, month, 1).unwrap_or(date)
}

fn quarter_start_month(month: Month) -> Month {
    match month {
        Month::January | Month::February | Month::March => Month::January,
        Month::April | Month::May | Month::June => Month::April,
        Month::July | Month::August | Month::September => Month::July,
        Month::October | Month::November | Month::December => Month::October,
    }
}

fn first_of_quarter(date: Date) -> Date {
    Date::from_calendar_date(date.year(), quarter_start_month(date.month()), 1).unwrap_or(date)
}

fn first_of_last_quarter(date: Date) -> Date {
    let (year, month) = match quarter_start_month(date.month()) {
        Month::January => (date.year() - 1, Month::October),
        Month::April => (date.year(), Month::January),
        Month::July => (date.year(), Month::April),
        _ => (date.year(), Month::July),
    };
    Date::from_calendar_date(year, month, 1).unwrap_or(date)
}

fn jan1(date: Date, year_offset: i32) -> Date {
    Date::from_calendar_date(date.year() + year_offset, Month::January, 1).unwrap_or(date)
}

fn n_months_ago(date: Date, n: u8) -> Date {
    let mut year = date.year();
    let mut month_num = date.month() as u8;
    if month_num <= n {
        year -= 1;
        month_num += 12 - n;
    } else {
        month_num -= n;
    }
    let month = Month::try_from(month_num).unwrap_or(Month::January);
    let day = date.day().min(days_in_month(year, month));
    Date::from_calendar_date(year, month, day).unwrap_or(date)
}

fn one_year_ago(date: Date) -> Date {
    date.replace_year(date.year() - 1).unwrap_or(date)
}

fn days_in_month(year: i32, month: Month) -> u8 {
    // Use the first of the next month minus one day trick
    let next_month = month.next();
    let next_year = if next_month == Month::January { year + 1 } else { year };
    let next_first = Date::from_calendar_date(next_year, next_month, 1)
        .unwrap_or(Date::from_calendar_date(year, month, 28).expect("valid"));
    let last_day = next_first
        .previous_day()
        .unwrap_or(Date::from_calendar_date(year, month, 28).expect("valid"));
    last_day.day()
}

fn year_range(y: &str, days: &impl LocalDays) -> (Option<u64>, Option<u64>) {
    let year: i32 = match y.parse() {
        Ok(v) => v,
        Err(_) => return (None, None),
    };
    let start = match Date::from_calendar_date(year, Month::January, 1) {
        Ok(d) => d,
        Err(_) => return (None, None),
    };
    let end = match Date::from_calendar_date(year + 1, Month::January, 1) {
        Ok(d) => d,
        Err(_) => return (None, None),
    };
    (Some(days.midnight(start)), Some(days.midnight(end)))
}

fn parse_date_range(r: &str, days: &impl LocalDays) -> (Option<u64>, Option<u64>) {
    // Try separators: "..", " to ", "–" (en-dash), "-"
    let parts: Option<(&str, &str)> = ["..", " to ", "\u{2013}"]
        .iter()
        .find_map(|sep| r.split_once(sep))
        .or_else(|| {
            // Single hyphen: only for YYYY-YYYY
            r.split_once('-')
                .filter(|(l, r)| is_year(l.trim()) && is_year(r.trim()))
        });

    let (left, right) = match parts {
        Some((l, r)) => (l.trim(), r.trim()),
        None => return (None, None),
    };

    let start = parse_date_start(left).map(|date| days.midnight(date));
    let end = parse_date_end(right).map(|date| days.midnight(date));
    (start, end)
}

/// Parse a date string as the start of a range. "2024" → Jan 1 2024, "2024-06" → Jun 1 2024.
fn parse_date_start(s: &str) -> Option<Date> {
    if is_year(s) {
        let year: i32 = s.parse().ok()?;
        return Date::from_calendar_date(year, Month::January, 1).ok();
    }
    // YYYY-MM
    if let Some((y, m)) = s.split_once('-') {
        let year: i32 = y.parse().ok()?;
        let month: u8 = m.parse().ok()?;
        let month = Month::try_from(month).ok()?;
        return Date::from_calendar_date(year, month, 1).ok();
    }
    None
}

/// Parse a date string as the end of a range. "2025" → Jan 1 2026, "2024-06" → Jul 1 2024.
fn parse_date_end(s: &str) -> Option<Date> {
    if is_year(s) {
        let year: i32 = s.parse().ok()?;
        return Date::from_calendar_date(year + 1, Month::January, 1).ok();
    }
    // YYYY-MM → first of next month
    if let Some((y, m)) = s.split_once('-') {
        let year: i32 = y.parse().ok()?;
        let month_num: u8 = m.parse().ok()?;
        let (next_year, next_month) = if month_num >= 12 {
            (year + 1, Month::January)
        } else {
            (year, Month::try_from(month_num + 1).ok()?)
        };
        return Date::from_calendar_date(next_year, next_month, 1).ok();
    }
    None
}

#[cfg(test)]
mod tests {

    /// The mapping on a fixed calendar ([`stockholm_november`]), so every test reads
    /// the same on any machine.
    fn time_to_range(t: &str) -> (Option<u64>, Option<u64>) {
        super::time_to_range(t, &stockholm_november())
    }

    /// The fixed calendar's "now": noon on its today, in UTC.
    fn now_secs() -> u64 {
        ts(datetime!(2026-11-04 12:00 UTC))
    }

    #[test]
    fn time_today_has_start_no_end() {
        let (after, before) = time_to_range("today");
        assert!(after.is_some());
        assert!(before.is_none());
    }

    #[test]
    fn time_yesterday_has_both_bounds() {
        let (after, before) = time_to_range("yesterday");
        assert!(after.is_some());
        assert!(before.is_some());
        assert!(after.unwrap() < before.unwrap());
    }

    #[test]
    fn time_all_enum_values_produce_timestamps() {
        let enums = [
            "today",
            "yesterday",
            "this_week",
            "last_week",
            "this_month",
            "last_month",
            "this_quarter",
            "last_quarter",
            "this_year",
            "last_year",
            "recent",
            "last_3_months",
            "last_6_months",
            "old",
        ];
        for t in enums {
            let (after, before) = time_to_range(t);
            assert!(
                after.is_some() || before.is_some(),
                "time '{t}' should produce at least one bound"
            );
        }
    }

    use crate::local_days::FixedZone;
    use time::macros::{date, datetime};

    fn ts(t: time::OffsetDateTime) -> u64 {
        t.unix_timestamp() as u64
    }

    /// Stockholm in November: UTC+1. "Today" starts at 00:00 local, 23:00 UTC the day before.
    fn stockholm_november() -> FixedZone {
        FixedZone {
            today: date!(2026 - 11 - 04),
            offset_secs: 3_600,
            switch_on: None,
        }
    }

    #[test]
    fn today_and_yesterday_start_at_local_midnight() {
        let zone = stockholm_november();
        assert_eq!(
            super::time_to_range("today", &zone),
            (Some(ts(datetime!(2026-11-03 23:00 UTC))), None)
        );
        assert_eq!(
            super::time_to_range("yesterday", &zone),
            (
                Some(ts(datetime!(2026-11-02 23:00 UTC))),
                Some(ts(datetime!(2026-11-03 23:00 UTC)))
            )
        );
    }

    /// A boundary on the far side of a DST change takes THAT day's offset: October 1 in
    /// Stockholm was summer time (UTC+2), so last month started at 22:00 UTC.
    #[test]
    fn a_boundary_across_a_dst_change_uses_that_days_offset() {
        let zone = FixedZone {
            switch_on: Some((date!(2026 - 10 - 25), 3_600)),
            offset_secs: 7_200,
            ..stockholm_november()
        };
        assert_eq!(
            super::time_to_range("last_month", &zone),
            (
                Some(ts(datetime!(2026-09-30 22:00 UTC))),
                Some(ts(datetime!(2026-10-31 23:00 UTC)))
            )
        );
    }

    #[test]
    fn a_year_range_is_local_too() {
        assert_eq!(
            super::time_to_range("2025", &stockholm_november()),
            (
                Some(ts(datetime!(2024-12-31 23:00 UTC))),
                Some(ts(datetime!(2025-12-31 23:00 UTC)))
            )
        );
    }

    #[test]
    fn time_year_range() {
        let (after, before) = time_to_range("2024");
        assert!(after.is_some());
        assert!(before.is_some());
        // 2024 → Jan 1 2024 to Jan 1 2025
        assert!(after.unwrap() < before.unwrap());
    }

    #[test]
    fn time_year_range_dotdot() {
        let (after, before) = time_to_range("2024..2025");
        assert!(after.is_some());
        assert!(before.is_some());
    }

    #[test]
    fn time_invalid_returns_none() {
        let (after, before) = time_to_range("next millennium");
        assert!(after.is_none());
        assert!(before.is_none());
    }

    #[test]
    fn time_recent_is_about_three_months_ago() {
        let (after, _) = time_to_range("recent");
        let ts = after.unwrap();
        // Should be roughly 90 days ago (±5 days for month length variation)
        let days_ago = (now_secs() - ts) / 86400;
        assert!(
            (85..=95).contains(&days_ago),
            "recent should be ~90 days ago, got {days_ago}"
        );
    }

    #[test]
    fn time_old_has_only_upper_bound() {
        let (after, before) = time_to_range("old");
        assert!(after.is_none());
        assert!(before.is_some());
    }

    #[test]
    fn time_last_6_months() {
        let (after, before) = time_to_range("last_6_months");
        assert!(after.is_some());
        assert!(before.is_none());
        let ts = after.unwrap();
        let days_ago = (now_secs() - ts) / 86400;
        assert!(
            (175..=190).contains(&days_ago),
            "last_6_months should be ~180 days ago, got {days_ago}"
        );
    }

    #[test]
    fn date_range_with_to() {
        let (after, before) = time_to_range("2024 to 2025");
        assert!(after.is_some());
        assert!(before.is_some());
    }

    #[test]
    fn date_range_with_en_dash() {
        let (after, before) = time_to_range("2024\u{2013}2025");
        assert!(after.is_some());
        assert!(before.is_some());
    }

    #[test]
    fn date_range_with_hyphen() {
        // YYYY-YYYY
        let (after, before) = time_to_range("2023-2024");
        assert!(after.is_some());
        assert!(before.is_some());
    }
}
