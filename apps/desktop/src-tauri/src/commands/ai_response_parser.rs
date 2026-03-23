//! Key-value line parser for LLM classification responses.
//!
//! Parses the simple `key: value` format returned by the classification prompt
//! into a `ParsedLlmResponse`. Unknown keys are silently ignored, and enum fields
//! are validated against known values (unknown values become `None`).

/// Parsed output from the LLM classification prompt.
///
/// Each field corresponds to one line in the LLM response. Missing or invalid
/// lines produce `None` for that field — no filter is applied for that dimension.
#[derive(Debug, Default, PartialEq)]
pub struct ParsedLlmResponse {
    pub keywords: Option<String>,
    pub type_field: Option<String>,
    pub time: Option<String>,
    pub size: Option<String>,
    pub scope: Option<String>,
    pub exclude: Option<String>,
    pub folders: Option<String>,
    pub note: Option<String>,
}

impl ParsedLlmResponse {
    /// Returns `true` if all fields are `None` (LLM returned nothing useful).
    pub fn is_empty(&self) -> bool {
        self.keywords.is_none()
            && self.type_field.is_none()
            && self.time.is_none()
            && self.size.is_none()
            && self.scope.is_none()
            && self.exclude.is_none()
            && self.folders.is_none()
            && self.note.is_none()
    }
}

/// Parse a key-value line LLM response into a `ParsedLlmResponse`.
///
/// Each line is split on the first `:` only — values may contain colons
/// (for example, scope paths). Empty values and unknown keys are skipped.
/// Enum fields (`type`, `time`, `size`, `folders`) are validated; unknown
/// values are discarded.
pub fn parse_llm_response(response: &str) -> ParsedLlmResponse {
    let mut parsed = ParsedLlmResponse::default();
    for line in response.lines() {
        let line = line.trim();
        if let Some((key, value)) = line.split_once(':') {
            let key = key.trim().to_lowercase();
            let value = value.trim().to_string();
            if value.is_empty() {
                continue;
            }
            match key.as_str() {
                "keywords" => parsed.keywords = Some(value),
                "type" => parsed.type_field = validate_type(&value),
                "time" => parsed.time = validate_time(&value),
                "size" => parsed.size = validate_size(&value),
                "scope" => parsed.scope = Some(value),
                "exclude" => parsed.exclude = Some(value),
                "folders" => parsed.folders = validate_folders(&value),
                "note" => parsed.note = Some(value),
                _ => {} // unknown field, skip
            }
        }
    }
    parsed
}

/// Validate `type` against known enum values. Returns `None` for unknown values.
pub fn validate_type(value: &str) -> Option<String> {
    let v = value.trim().to_lowercase();
    match v.as_str() {
        "photos" | "screenshots" | "videos" | "documents" | "presentations" | "archives" | "music" | "code"
        | "rust" | "python" | "javascript" | "typescript" | "go" | "java" | "config" | "logs" | "fonts"
        | "databases" | "xcode" | "shell-scripts" | "ssh-keys" | "docker-compose" | "env-files" | "none" => Some(v),
        _ => None,
    }
}

/// Validate `time` against known enum values or date patterns (YYYY, YYYY..YYYY).
/// Returns `None` for unrecognized values.
pub fn validate_time(value: &str) -> Option<String> {
    let v = value.trim().to_lowercase();
    match v.as_str() {
        "today" | "yesterday" | "this_week" | "last_week" | "this_month" | "last_month" | "this_quarter"
        | "last_quarter" | "this_year" | "last_year" | "recent" | "last_3_months" | "last_6_months" | "old" => Some(v),
        _ => {
            // Accept YYYY or YYYY..YYYY / YYYY-YYYY / YYYY to YYYY / YYYY–YYYY
            let trimmed = value.trim();
            if is_year(trimmed) || is_range(trimmed) {
                Some(trimmed.to_string())
            } else {
                None
            }
        }
    }
}

/// Validate `size` against known enum values or size expressions (>Nmb, <Ngb).
/// Returns `None` for unrecognized values.
pub fn validate_size(value: &str) -> Option<String> {
    let v = value.trim().to_lowercase();
    match v.as_str() {
        "empty" | "tiny" | "small" | "large" | "huge" => Some(v),
        _ => {
            let trimmed = value.trim().to_lowercase();
            // Accept >NUMBERmb, >NUMBERgb, <NUMBERmb, <NUMBERgb, etc.
            if (trimmed.starts_with('>') || trimmed.starts_with('<')) && trimmed.len() > 1 {
                Some(trimmed)
            } else {
                None
            }
        }
    }
}

/// Validate `folders` against known values (yes/no).
pub fn validate_folders(value: &str) -> Option<String> {
    let v = value.trim().to_lowercase();
    match v.as_str() {
        "yes" | "no" => Some(v),
        _ => None,
    }
}

/// Check if a string is a 4-digit year.
pub(crate) fn is_year(s: &str) -> bool {
    s.len() == 4 && s.chars().all(|c| c.is_ascii_digit())
}

/// Check if a string is a year range (YYYY..YYYY, YYYY-YYYY, YYYY to YYYY, YYYY–YYYY).
pub(crate) fn is_range(s: &str) -> bool {
    // Try each separator
    for sep in &["..", " to ", "\u{2013}"] {
        // \u{2013} = en-dash –
        if let Some((left, right)) = s.split_once(sep) {
            let left = left.trim();
            let right = right.trim();
            if is_year_or_month(left) && is_year_or_month(right) {
                return true;
            }
        }
    }
    // Also try single hyphen, but only for YYYY-YYYY (not YYYY-MM)
    if let Some((left, right)) = s.split_once('-') {
        let left = left.trim();
        let right = right.trim();
        if is_year(left) && is_year(right) {
            return true;
        }
    }
    false
}

/// Check if a string looks like YYYY or YYYY-MM.
fn is_year_or_month(s: &str) -> bool {
    if is_year(s) {
        return true;
    }
    // YYYY-MM
    if s.len() == 7
        && let Some((year, month)) = s.split_once('-')
    {
        return is_year(year) && month.len() == 2 && month.chars().all(|c| c.is_ascii_digit());
    }
    false
}

/// Extract fallback keywords from the raw query when the LLM fails entirely.
///
/// Splits on whitespace, keeps words > 2 characters, returns the 3 longest
/// (biased toward content words over grammar particles).
pub fn fallback_keywords(raw_query: &str) -> String {
    let mut words: Vec<&str> = raw_query.split_whitespace().filter(|w| w.len() > 2).collect();
    // Sort by length descending, take top 3
    words.sort_by_key(|w| std::cmp::Reverse(w.len()));
    words.truncate(3);
    words.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_well_formed_response() {
        let response = "\
keywords: rymd
type: documents
time: recent
size: large
scope: downloads
exclude: node_modules .git
folders: no
note: can't determine content";
        let parsed = parse_llm_response(response);
        assert_eq!(parsed.keywords.as_deref(), Some("rymd"));
        assert_eq!(parsed.type_field.as_deref(), Some("documents"));
        assert_eq!(parsed.time.as_deref(), Some("recent"));
        assert_eq!(parsed.size.as_deref(), Some("large"));
        assert_eq!(parsed.scope.as_deref(), Some("downloads"));
        assert_eq!(parsed.exclude.as_deref(), Some("node_modules .git"));
        assert_eq!(parsed.folders.as_deref(), Some("no"));
        assert_eq!(parsed.note.as_deref(), Some("can't determine content"));
    }

    #[test]
    fn parse_missing_fields() {
        let response = "keywords: websocket\ntype: rust";
        let parsed = parse_llm_response(response);
        assert_eq!(parsed.keywords.as_deref(), Some("websocket"));
        assert_eq!(parsed.type_field.as_deref(), Some("rust"));
        assert!(parsed.time.is_none());
        assert!(parsed.size.is_none());
        assert!(parsed.scope.is_none());
        assert!(parsed.exclude.is_none());
        assert!(parsed.folders.is_none());
        assert!(parsed.note.is_none());
    }

    #[test]
    fn parse_unknown_fields_ignored() {
        let response = "keywords: test\nflavor: chocolate\nmood: happy";
        let parsed = parse_llm_response(response);
        assert_eq!(parsed.keywords.as_deref(), Some("test"));
        assert!(parsed.type_field.is_none());
    }

    #[test]
    fn parse_garbage_input() {
        let response = "this is just garbage\nno colons here\n!!!";
        let parsed = parse_llm_response(response);
        assert!(parsed.is_empty());
    }

    #[test]
    fn parse_malformed_lines() {
        let response = "no colon here\nkeywords:\ntype: documents\n: empty key";
        let parsed = parse_llm_response(response);
        // "keywords:" has empty value → skipped
        assert!(parsed.keywords.is_none());
        assert_eq!(parsed.type_field.as_deref(), Some("documents"));
    }

    #[test]
    fn parse_extra_whitespace_and_mixed_case_keys() {
        let response = "  Keywords :  rymd stuff  \n  TYPE : photos  ";
        let parsed = parse_llm_response(response);
        assert_eq!(parsed.keywords.as_deref(), Some("rymd stuff"));
        assert_eq!(parsed.type_field.as_deref(), Some("photos"));
    }

    #[test]
    fn parse_multi_word_values_preserved() {
        let response = "keywords: contract agreement invoice";
        let parsed = parse_llm_response(response);
        assert_eq!(parsed.keywords.as_deref(), Some("contract agreement invoice"));
    }

    #[test]
    fn validate_unknown_type_discarded() {
        let response = "type: bananas";
        let parsed = parse_llm_response(response);
        assert!(parsed.type_field.is_none());
    }

    #[test]
    fn validate_unknown_time_discarded() {
        let response = "time: next millennium";
        let parsed = parse_llm_response(response);
        assert!(parsed.time.is_none());
    }

    #[test]
    fn validate_unknown_size_discarded() {
        let response = "size: ginormous";
        let parsed = parse_llm_response(response);
        assert!(parsed.size.is_none());
    }

    #[test]
    fn validate_unknown_folders_discarded() {
        let response = "folders: maybe";
        let parsed = parse_llm_response(response);
        assert!(parsed.folders.is_none());
    }

    #[test]
    fn validate_time_year() {
        assert_eq!(validate_time("2024"), Some("2024".to_string()));
    }

    #[test]
    fn validate_time_year_range() {
        assert_eq!(validate_time("2024..2025"), Some("2024..2025".to_string()));
        assert_eq!(validate_time("2024 to 2025"), Some("2024 to 2025".to_string()));
        assert_eq!(validate_time("2024\u{2013}2025"), Some("2024\u{2013}2025".to_string())); // en-dash
    }

    #[test]
    fn validate_size_expressions() {
        assert_eq!(validate_size(">50mb"), Some(">50mb".to_string()));
        assert_eq!(validate_size("<1gb"), Some("<1gb".to_string()));
        assert_eq!(validate_size(">100mb"), Some(">100mb".to_string()));
    }

    #[test]
    fn validate_all_type_enums() {
        for t in &[
            "photos",
            "screenshots",
            "videos",
            "documents",
            "presentations",
            "archives",
            "music",
            "code",
            "rust",
            "python",
            "javascript",
            "typescript",
            "go",
            "java",
            "config",
            "logs",
            "fonts",
            "databases",
            "xcode",
            "shell-scripts",
            "ssh-keys",
            "docker-compose",
            "env-files",
            "none",
        ] {
            assert!(validate_type(t).is_some(), "type '{t}' should be valid");
        }
    }

    #[test]
    fn validate_all_time_enums() {
        for t in &[
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
        ] {
            assert!(validate_time(t).is_some(), "time '{t}' should be valid");
        }
    }

    #[test]
    fn value_with_colons_preserved() {
        let response = "scope: /Users/foo/path:with:colons";
        let parsed = parse_llm_response(response);
        assert_eq!(parsed.scope.as_deref(), Some("/Users/foo/path:with:colons"));
    }

    #[test]
    fn fallback_keywords_basic() {
        let result = fallback_keywords("find my important documents please");
        // Longest 3 words > 2 chars: "important" (9), "documents" (9), "please" (6)
        assert_eq!(result, "important documents please");
    }

    #[test]
    fn fallback_keywords_short_words_filtered() {
        let result = fallback_keywords("a be cat dog");
        // "a" and "be" are ≤ 2 chars
        assert_eq!(result, "cat dog");
    }

    #[test]
    fn fallback_keywords_empty_input() {
        let result = fallback_keywords("");
        assert_eq!(result, "");
    }

    #[test]
    fn fallback_keywords_limits_to_three() {
        let result = fallback_keywords("alpha bravo charlie delta echo");
        // Sorted by length: "charlie" (7), "bravo" (5), "alpha"/"delta"/"echo" (5/5/4)
        // Top 3 by length
        let words: Vec<&str> = result.split_whitespace().collect();
        assert_eq!(words.len(), 3);
    }

    #[test]
    fn parse_type_none_is_valid() {
        let response = "type: none";
        let parsed = parse_llm_response(response);
        assert_eq!(parsed.type_field.as_deref(), Some("none"));
    }
}
