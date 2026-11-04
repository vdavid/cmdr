//! The desktop's side of natural-language search: this machine's calendar and home
//! folder, handed to `cmdr_search::translate`, which owns the prompt, the reply's
//! parser, and the mappings (`crates/cmdr-search/src/translate/CLAUDE.md`).
//!
//! Picking a provider and calling it is `crate::ai`'s (`commands/search.rs` wires
//! the two), so nothing here touches a model, a key, or a setting.

use cmdr_search::SearchQuery;
use cmdr_search::translate::{self, ParsedLlmResponse, TranslateDisplay};

use super::zone::SystemZone;

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
    use chrono::TimeZone;
    use cmdr_search::translate::LocalDays;

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
