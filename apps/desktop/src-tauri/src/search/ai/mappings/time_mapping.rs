use time::OffsetDateTime;

use super::super::parser::{is_range, is_year};

// ── Time mapping ─────────────────────────────────────────────────────

/// Map a `time` enum value to a (modified_after, modified_before) timestamp range.
///
/// Returns `(None, None)` for unrecognized values.
pub fn time_to_range(t: &str) -> (Option<u64>, Option<u64>) {
    let now = OffsetDateTime::now_utc();
    match t {
        "today" => (Some(start_of_today(now)), None),
        "yesterday" => (Some(start_of_yesterday(now)), Some(start_of_today(now))),
        "this_week" => (Some(monday_of_this_week(now)), None),
        "last_week" => (Some(monday_of_last_week(now)), Some(monday_of_this_week(now))),
        "this_month" => (Some(first_of_this_month(now)), None),
        "last_month" => (Some(first_of_last_month(now)), Some(first_of_this_month(now))),
        "this_quarter" => (Some(first_of_this_quarter(now)), None),
        "last_quarter" => (Some(first_of_last_quarter(now)), Some(first_of_this_quarter(now))),
        "this_year" => (Some(jan1(now, 0)), None),
        "last_year" => (Some(jan1(now, -1)), Some(jan1(now, 0))),
        "recent" => (Some(n_months_ago(now, 3)), None),
        "last_3_months" => (Some(n_months_ago(now, 3)), None),
        "last_6_months" => (Some(n_months_ago(now, 6)), None),
        "old" => (None, Some(one_year_ago(now))),
        _ => {
            if is_year(t) {
                year_range(t)
            } else if is_range(t) {
                parse_date_range(t)
            } else {
                (None, None)
            }
        }
    }
}

fn to_timestamp(dt: OffsetDateTime) -> u64 {
    dt.unix_timestamp().max(0) as u64
}

fn start_of_today(now: OffsetDateTime) -> u64 {
    let date = now.date();
    to_timestamp(date.with_hms(0, 0, 0).expect("valid").assume_utc())
}

fn start_of_yesterday(now: OffsetDateTime) -> u64 {
    let date = now.date().previous_day().unwrap_or(now.date());
    to_timestamp(date.with_hms(0, 0, 0).expect("valid").assume_utc())
}

fn monday_of_this_week(now: OffsetDateTime) -> u64 {
    let date = now.date();
    let weekday = date.weekday().number_from_monday(); // Monday=1, Sunday=7
    let days_since_monday = weekday - 1;
    let monday = date
        .checked_sub(time::Duration::days(days_since_monday as i64))
        .unwrap_or(date);
    to_timestamp(monday.with_hms(0, 0, 0).expect("valid").assume_utc())
}

fn monday_of_last_week(now: OffsetDateTime) -> u64 {
    let date = now.date();
    let weekday = date.weekday().number_from_monday();
    let days_since_monday = weekday - 1;
    let this_monday = date
        .checked_sub(time::Duration::days(days_since_monday as i64))
        .unwrap_or(date);
    let last_monday = this_monday.checked_sub(time::Duration::days(7)).unwrap_or(this_monday);
    to_timestamp(last_monday.with_hms(0, 0, 0).expect("valid").assume_utc())
}

fn first_of_this_month(now: OffsetDateTime) -> u64 {
    let date = now.date();
    let first = date.replace_day(1).unwrap_or(date);
    to_timestamp(first.with_hms(0, 0, 0).expect("valid").assume_utc())
}

fn first_of_last_month(now: OffsetDateTime) -> u64 {
    let date = now.date();
    let (year, month) = if date.month() == time::Month::January {
        (date.year() - 1, time::Month::December)
    } else {
        (date.year(), date.month().previous())
    };
    let first = time::Date::from_calendar_date(year, month, 1).unwrap_or(date);
    to_timestamp(first.with_hms(0, 0, 0).expect("valid").assume_utc())
}

fn quarter_start_month(month: time::Month) -> time::Month {
    match month {
        time::Month::January | time::Month::February | time::Month::March => time::Month::January,
        time::Month::April | time::Month::May | time::Month::June => time::Month::April,
        time::Month::July | time::Month::August | time::Month::September => time::Month::July,
        time::Month::October | time::Month::November | time::Month::December => time::Month::October,
    }
}

fn first_of_this_quarter(now: OffsetDateTime) -> u64 {
    let date = now.date();
    let qm = quarter_start_month(date.month());
    let first = time::Date::from_calendar_date(date.year(), qm, 1).unwrap_or(date);
    to_timestamp(first.with_hms(0, 0, 0).expect("valid").assume_utc())
}

fn first_of_last_quarter(now: OffsetDateTime) -> u64 {
    let date = now.date();
    let qm = quarter_start_month(date.month());
    let (year, month) = match qm {
        time::Month::January => (date.year() - 1, time::Month::October),
        time::Month::April => (date.year(), time::Month::January),
        time::Month::July => (date.year(), time::Month::April),
        time::Month::October => (date.year(), time::Month::July),
        _ => unreachable!(),
    };
    let first = time::Date::from_calendar_date(year, month, 1).unwrap_or(date);
    to_timestamp(first.with_hms(0, 0, 0).expect("valid").assume_utc())
}

fn jan1(now: OffsetDateTime, year_offset: i32) -> u64 {
    let first = time::Date::from_calendar_date(now.year() + year_offset, time::Month::January, 1).unwrap_or(now.date());
    to_timestamp(first.with_hms(0, 0, 0).expect("valid").assume_utc())
}

fn n_months_ago(now: OffsetDateTime, n: u8) -> u64 {
    let date = now.date();
    let mut year = date.year();
    let mut month_num = date.month() as u8;
    if month_num <= n {
        year -= 1;
        month_num += 12 - n;
    } else {
        month_num -= n;
    }
    let month = time::Month::try_from(month_num).unwrap_or(time::Month::January);
    let day = date.day().min(days_in_month(year, month));
    let target = time::Date::from_calendar_date(year, month, day).unwrap_or(date);
    to_timestamp(target.with_hms(0, 0, 0).expect("valid").assume_utc())
}

fn one_year_ago(now: OffsetDateTime) -> u64 {
    let date = now.date();
    let target = date.replace_year(date.year() - 1).unwrap_or(date);
    to_timestamp(target.with_hms(0, 0, 0).expect("valid").assume_utc())
}

fn days_in_month(year: i32, month: time::Month) -> u8 {
    // Use the first of the next month minus one day trick
    let next_month = month.next();
    let next_year = if next_month == time::Month::January {
        year + 1
    } else {
        year
    };
    let next_first = time::Date::from_calendar_date(next_year, next_month, 1)
        .unwrap_or(time::Date::from_calendar_date(year, month, 28).expect("valid"));
    let last_day = next_first
        .previous_day()
        .unwrap_or(time::Date::from_calendar_date(year, month, 28).expect("valid"));
    last_day.day()
}

fn year_range(y: &str) -> (Option<u64>, Option<u64>) {
    let year: i32 = match y.parse() {
        Ok(v) => v,
        Err(_) => return (None, None),
    };
    let start = match time::Date::from_calendar_date(year, time::Month::January, 1) {
        Ok(d) => d,
        Err(_) => return (None, None),
    };
    let end = match time::Date::from_calendar_date(year + 1, time::Month::January, 1) {
        Ok(d) => d,
        Err(_) => return (None, None),
    };
    (
        Some(to_timestamp(start.with_hms(0, 0, 0).expect("valid").assume_utc())),
        Some(to_timestamp(end.with_hms(0, 0, 0).expect("valid").assume_utc())),
    )
}

fn parse_date_range(r: &str) -> (Option<u64>, Option<u64>) {
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

    let start = parse_date_start(left);
    let end = parse_date_end(right);
    (start, end)
}

/// Parse a date string as the start of a range. "2024" → Jan 1 2024, "2024-06" → Jun 1 2024.
fn parse_date_start(s: &str) -> Option<u64> {
    if is_year(s) {
        let year: i32 = s.parse().ok()?;
        let date = time::Date::from_calendar_date(year, time::Month::January, 1).ok()?;
        return Some(to_timestamp(date.with_hms(0, 0, 0).expect("valid").assume_utc()));
    }
    // YYYY-MM
    if let Some((y, m)) = s.split_once('-') {
        let year: i32 = y.parse().ok()?;
        let month: u8 = m.parse().ok()?;
        let month = time::Month::try_from(month).ok()?;
        let date = time::Date::from_calendar_date(year, month, 1).ok()?;
        return Some(to_timestamp(date.with_hms(0, 0, 0).expect("valid").assume_utc()));
    }
    None
}

/// Parse a date string as the end of a range. "2025" → Jan 1 2026, "2024-06" → Jul 1 2024.
fn parse_date_end(s: &str) -> Option<u64> {
    if is_year(s) {
        let year: i32 = s.parse().ok()?;
        let date = time::Date::from_calendar_date(year + 1, time::Month::January, 1).ok()?;
        return Some(to_timestamp(date.with_hms(0, 0, 0).expect("valid").assume_utc()));
    }
    // YYYY-MM → first of next month
    if let Some((y, m)) = s.split_once('-') {
        let year: i32 = y.parse().ok()?;
        let month_num: u8 = m.parse().ok()?;
        let (next_year, next_month) = if month_num >= 12 {
            (year + 1, time::Month::January)
        } else {
            (year, time::Month::try_from(month_num + 1).ok()?)
        };
        let date = time::Date::from_calendar_date(next_year, next_month, 1).ok()?;
        return Some(to_timestamp(date.with_hms(0, 0, 0).expect("valid").assume_utc()));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let now = OffsetDateTime::now_utc();
        let ts = after.unwrap();
        // Should be roughly 90 days ago (±5 days for month length variation)
        let days_ago = (now.unix_timestamp() as u64 - ts) / 86400;
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
        let now = OffsetDateTime::now_utc();
        let ts = after.unwrap();
        let days_ago = (now.unix_timestamp() as u64 - ts) / 86400;
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
