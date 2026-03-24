//! Assembles a `SearchQuery` from parsed LLM output and generates display values.
//!
//! This is the orchestrator: it calls mapping functions from `mappings` to convert
//! individual fields, then assembles the results into a complete `SearchQuery`.

use crate::commands::search::{TranslateDisplay, TranslatedQuery};
use crate::search::{PatternType, SearchQuery, format_timestamp};

use super::mappings::{
    keywords_to_pattern, merge_keyword_and_type, parse_exclude_list, scope_to_paths, size_to_filter, time_to_range,
    type_to_filter,
};
use super::parser::ParsedLlmResponse;

// ── ISO date conversion ──────────────────────────────────────────────

/// Converts an ISO date string (YYYY-MM-DD) to a unix timestamp (seconds since epoch).
pub fn iso_date_to_timestamp(date_str: &str) -> Result<u64, String> {
    let format = time::macros::format_description!("[year]-[month]-[day]");
    let date = time::Date::parse(date_str, &format).map_err(|e| format!("Invalid date '{date_str}': {e}"))?;
    let datetime = date.with_hms(0, 0, 0).expect("midnight is always valid");
    let timestamp = datetime.assume_utc().unix_timestamp();
    if timestamp < 0 {
        return Err(format!("Date '{date_str}' is before unix epoch"));
    }
    Ok(timestamp as u64)
}

// ── Assembly ─────────────────────────────────────────────────────────

/// Build a `SearchQuery` from a parsed LLM response.
pub fn build_search_query(parsed: &ParsedLlmResponse) -> SearchQuery {
    let type_filter = parsed.type_field.as_deref().and_then(type_to_filter);
    let (time_after, time_before) = parsed.time.as_deref().map(time_to_range).unwrap_or_default();
    let (size_min, size_max) = parsed.size.as_deref().map(size_to_filter).unwrap_or_default();
    let scope = parsed.scope.as_deref().map(scope_to_paths);
    let exclude = parsed.exclude.as_deref().map(parse_exclude_list);
    let is_dir = parsed.folders.as_deref().map(|f| f == "yes");

    // Build keyword pattern
    let kw = parsed.keywords.as_deref().and_then(keywords_to_pattern);

    // Handle dotfiles scope name_prefix: if no keywords, add `.*` glob for dotfile prefix
    let kw = match (&kw, &scope) {
        (None, Some(s)) if s.name_prefix == Some(".") => Some((".*".to_string(), PatternType::Glob)),
        _ => kw,
    };

    // Merge keywords + type into a single name_pattern + pattern_type
    let (name_pattern, pattern_type) = merge_keyword_and_type(kw, type_filter.as_ref());

    let include_system_dirs = type_filter.as_ref().is_some_and(|f| f.include_system_dirs);

    SearchQuery {
        name_pattern,
        pattern_type,
        min_size: size_min,
        max_size: size_max,
        modified_after: time_after,
        modified_before: time_before,
        is_directory: is_dir,
        include_paths: scope.as_ref().map(|s| s.paths.clone()),
        exclude_dir_names: exclude,
        include_path_ids: None,
        limit: 30,
        case_sensitive: None,
        exclude_system_dirs: if include_system_dirs { Some(false) } else { None },
    }
}

// ── Caveat generation ────────────────────────────────────────────────

/// Generate a caveat string based on the parsed LLM response and built query.
///
/// Priority: LLM-provided note (truncated 200 chars) > Rust-inferred caveats.
pub fn generate_caveat(parsed: &ParsedLlmResponse, query: &SearchQuery) -> Option<String> {
    // LLM provided a note → use it (sanitized: max 200 chars, no HTML)
    if let Some(note) = &parsed.note {
        let sanitized: String = note.chars().filter(|c| *c != '<' && *c != '>').take(200).collect();
        return Some(sanitized);
    }
    // No name pattern → very broad search
    if query.name_pattern.is_none() {
        return Some(
            "No filename filter \u{2014} results may be very broad. Add a name or file type to narrow.".into(),
        );
    }
    None
}

// ── Display value generation ─────────────────────────────────────────

/// Build human-readable display values for the frontend filter UI.
pub fn build_translate_display(parsed: &ParsedLlmResponse, query: &SearchQuery) -> TranslateDisplay {
    TranslateDisplay {
        name_pattern: query.name_pattern.clone(),
        pattern_type: Some(match query.pattern_type {
            PatternType::Glob => "glob".to_string(),
            PatternType::Regex => "regex".to_string(),
        }),
        min_size: query.min_size,
        max_size: query.max_size,
        modified_after: query.modified_after.map(format_timestamp),
        modified_before: query.modified_before.map(format_timestamp),
        is_directory: query.is_directory,
        include_paths: query.include_paths.clone(),
        exclude_dir_names: query.exclude_dir_names.clone(),
        case_sensitive: parsed.type_field.as_deref().and(query.case_sensitive),
    }
}

/// Build a `TranslatedQuery` from a `SearchQuery` (for IPC serialization).
pub fn build_translated_query(query: &SearchQuery) -> TranslatedQuery {
    TranslatedQuery {
        name_pattern: query.name_pattern.clone(),
        pattern_type: match query.pattern_type {
            PatternType::Glob => "glob".to_string(),
            PatternType::Regex => "regex".to_string(),
        },
        min_size: query.min_size,
        max_size: query.max_size,
        modified_after: query.modified_after,
        modified_before: query.modified_before,
        is_directory: query.is_directory,
        include_paths: query.include_paths.clone(),
        exclude_dir_names: query.exclude_dir_names.clone(),
        case_sensitive: query.case_sensitive,
        exclude_system_dirs: query.exclude_system_dirs,
    }
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Assembly ─────────────────────────────────────────────────────

    #[test]
    fn build_full_query() {
        let parsed = ParsedLlmResponse {
            keywords: Some("rymd".to_string()),
            type_field: Some("documents".to_string()),
            time: Some("recent".to_string()),
            size: Some("large".to_string()),
            scope: Some("downloads".to_string()),
            exclude: Some("node_modules .git".to_string()),
            folders: Some("no".to_string()),
            note: None,
        };
        let query = build_search_query(&parsed);
        assert!(query.name_pattern.is_some());
        assert_eq!(query.pattern_type, PatternType::Regex);
        assert!(query.min_size.is_some());
        assert!(query.modified_after.is_some());
        assert_eq!(query.is_directory, Some(false));
        assert!(query.include_paths.is_some());
        assert!(query.exclude_dir_names.is_some());
        assert_eq!(query.exclude_dir_names.as_ref().unwrap().len(), 2);
    }

    #[test]
    fn build_logs_type_includes_system_dirs() {
        let parsed = ParsedLlmResponse {
            type_field: Some("logs".to_string()),
            ..Default::default()
        };
        let query = build_search_query(&parsed);
        assert_eq!(query.exclude_system_dirs, Some(false));
    }

    #[test]
    fn build_dotfiles_scope() {
        let parsed = ParsedLlmResponse {
            scope: Some("dotfiles".to_string()),
            ..Default::default()
        };
        let query = build_search_query(&parsed);
        assert!(query.include_paths.is_some());
        // Should have a name pattern for dotfiles (starts with ".")
        assert!(query.name_pattern.is_some());
    }

    #[test]
    fn build_folders_yes() {
        let parsed = ParsedLlmResponse {
            keywords: Some("node_modules".to_string()),
            folders: Some("yes".to_string()),
            ..Default::default()
        };
        let query = build_search_query(&parsed);
        assert_eq!(query.is_directory, Some(true));
    }

    // ── Caveat generation ────────────────────────────────────────────

    #[test]
    fn caveat_from_llm_note() {
        let parsed = ParsedLlmResponse {
            note: Some("can't filter by photo content".to_string()),
            ..Default::default()
        };
        let query = build_search_query(&parsed);
        let caveat = generate_caveat(&parsed, &query);
        assert_eq!(caveat.as_deref(), Some("can't filter by photo content"));
    }

    #[test]
    fn caveat_truncated_at_200_chars() {
        let long_note = "a".repeat(300);
        let parsed = ParsedLlmResponse {
            note: Some(long_note),
            ..Default::default()
        };
        let query = build_search_query(&parsed);
        let caveat = generate_caveat(&parsed, &query).unwrap();
        assert_eq!(caveat.len(), 200);
    }

    #[test]
    fn caveat_no_name_pattern() {
        let parsed = ParsedLlmResponse {
            size: Some("large".to_string()),
            ..Default::default()
        };
        let query = build_search_query(&parsed);
        let caveat = generate_caveat(&parsed, &query);
        assert!(caveat.is_some());
        assert!(caveat.unwrap().contains("No filename filter"));
    }

    #[test]
    fn caveat_none_when_has_pattern() {
        let parsed = ParsedLlmResponse {
            keywords: Some("test".to_string()),
            ..Default::default()
        };
        let query = build_search_query(&parsed);
        let caveat = generate_caveat(&parsed, &query);
        assert!(caveat.is_none());
    }

    // ── Display values ───────────────────────────────────────────────

    #[test]
    fn display_values_populated() {
        let parsed = ParsedLlmResponse {
            keywords: Some("test".to_string()),
            type_field: Some("rust".to_string()),
            time: Some("today".to_string()),
            size: Some("large".to_string()),
            ..Default::default()
        };
        let query = build_search_query(&parsed);
        let display = build_translate_display(&parsed, &query);
        assert!(display.name_pattern.is_some());
        assert_eq!(display.pattern_type.as_deref(), Some("regex"));
        assert!(display.min_size.is_some());
        assert!(display.modified_after.is_some());
    }

    // ── Fallback integration ─────────────────────────────────────────

    #[test]
    fn empty_llm_response_produces_empty_parsed() {
        let parsed = ParsedLlmResponse::default();
        assert!(parsed.is_empty());
    }

    #[test]
    fn partial_llm_response_builds_partial_query() {
        let parsed = ParsedLlmResponse {
            time: Some("recent".to_string()),
            ..Default::default()
        };
        let query = build_search_query(&parsed);
        // Only time filter set
        assert!(query.modified_after.is_some());
        assert!(query.name_pattern.is_none());
        assert!(query.min_size.is_none());
    }

    // ── ISO date conversion ──────────────────────────────────────────

    #[test]
    fn test_iso_date_to_timestamp() {
        // 2025-01-01 00:00:00 UTC = 1735689600
        let ts = iso_date_to_timestamp("2025-01-01").unwrap();
        assert_eq!(ts, 1_735_689_600);
    }

    #[test]
    fn test_iso_date_to_timestamp_mid_year() {
        // 2026-06-15 00:00:00 UTC = 1781481600
        let ts = iso_date_to_timestamp("2026-06-15").unwrap();
        assert_eq!(ts, 1_781_481_600);
    }

    #[test]
    fn test_iso_date_to_timestamp_invalid() {
        assert!(iso_date_to_timestamp("not-a-date").is_err());
        assert!(iso_date_to_timestamp("2025-13-01").is_err());
        assert!(iso_date_to_timestamp("2025-01-32").is_err());
    }
}
