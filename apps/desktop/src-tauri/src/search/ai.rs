//! The desktop's side of natural-language search: this machine's calendar and home
//! folder, handed to `cmdr_search::translate`, which owns the prompt, the reply's
//! parser, and the mappings (`crates/cmdr-search/src/translate/CLAUDE.md`).
//!
//! Picking a provider and calling it is `crate::ai`'s (`commands/search.rs` wires
//! the two), so nothing here touches a model, a key, or a setting.

use chrono::{LocalResult, NaiveDate, TimeZone};
use time::{Date, Month};

use cmdr_search::SearchQuery;
use cmdr_search::translate::{self, LocalDays, ParsedLlmResponse, TranslateDisplay};

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

    fn date_of(&self, at: u64) -> Date {
        let utc = chrono::DateTime::from_timestamp(at as i64, 0).unwrap_or_default();
        from_naive(utc.with_timezone(&chrono::Local).date_naive())
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

/// The classification prompt, dated by this machine's calendar.
pub(crate) fn classification_prompt(current_type: Option<bool>) -> String {
    translate::build_classification_prompt(current_type, &SystemZone)
}

/// A parsed reply as a query, on this machine's calendar and home folder.
pub(crate) fn search_query(parsed: &ParsedLlmResponse) -> SearchQuery {
    let home = super::host_home_dir();
    translate::build_search_query(parsed, &SystemZone, home.as_deref())
}

/// A reply's display values (the AI chips), dated on this machine's calendar.
pub(crate) fn translate_display(parsed: &ParsedLlmResponse, query: &SearchQuery) -> TranslateDisplay {
    translate::build_translate_display(parsed, query, &SystemZone)
}

/// The instant an ISO date (YYYY-MM-DD) starts on this machine's calendar. The MCP
/// `search` tool reads its date arguments through it, so an agent's
/// `modifiedAfter: "2025-01-01"` means the user's day, as the dialog's does.
pub fn iso_date_to_timestamp(date_str: &str) -> Result<u64, String> {
    translate::iso_date_to_timestamp(date_str, &SystemZone)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// "Today" is the user's day, so it starts at LOCAL midnight. Bites on any machine
    /// whose zone isn't UTC (CI runs in UTC, where it passes either way; the crate's
    /// fixed-zone tests pin the rule everywhere).
    #[test]
    fn today_starts_at_this_machines_local_midnight() {
        let midnight = chrono::Local::now()
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .expect("valid time");
        let expected = chrono::Local
            .from_local_datetime(&midnight)
            .earliest()
            .expect("midnight exists locally")
            .timestamp() as u64;
        let parsed = ParsedLlmResponse {
            time: Some("today".to_string()),
            ..Default::default()
        };
        assert_eq!(search_query(&parsed).modified_after, Some(expected));
    }

    /// A day's first instant reads back as that day here, so a chip shows the date its
    /// boundary was computed from. Bites on any machine east of UTC.
    #[test]
    fn a_local_midnight_reads_back_as_its_own_date() {
        let today = SystemZone.today();
        assert_eq!(SystemZone.date_of(SystemZone.midnight(today)), today);
        let next = today.next_day().expect("not the last day");
        assert_eq!(SystemZone.date_of(SystemZone.midnight(next) - 1), today);
    }

    #[test]
    fn the_prompt_is_dated_today_here() {
        let format = time::macros::format_description!("[year]-[month]-[day]");
        let today = SystemZone.today().format(&format).expect("formats");
        assert!(classification_prompt(None).contains(&format!("Today: {today}.")));
    }

    #[test]
    fn a_named_folder_lands_in_this_machines_home() {
        let parsed = ParsedLlmResponse {
            scope: Some("downloads".to_string()),
            ..Default::default()
        };
        let home = crate::search::host_home_dir().expect("a test machine has a home folder");
        assert_eq!(
            search_query(&parsed).include_paths,
            Some(vec![format!("{home}/Downloads")])
        );
    }
}
