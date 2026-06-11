//! Key-value line parser for the selection LLM classification response.
//!
//! Same line-format as `crate::search::ai::parser` (one `key: value` per line, unknown
//! keys ignored, blank values skipped). The fields are narrower: there's no
//! `keywords` / `type` / `scope` / `folders` because Selection always runs in the
//! current folder.
//!
//! Validation is intentionally loose. The frontend matcher re-validates the pattern
//! (a malformed regex throws at compile time and the dialog surfaces the caveat). The
//! parser's job is to turn key-value lines into a typed struct, not to enforce
//! semantic correctness.

/// Parsed output from the selection classification prompt.
///
/// Each field corresponds to one line in the LLM response. Missing or invalid lines
/// produce `None` (no filter applied for that dimension).
#[derive(Debug, Default, PartialEq)]
pub struct ParsedSelectionLlmResponse {
    /// The glob or regex string (raw, unescaped).
    pub pattern: Option<String>,
    /// `"glob"` or `"regex"`. Any other value is dropped to `None`.
    pub kind: Option<String>,
    /// `"file"` or `"folder"`. The file-vs-folder dimension, distinct from glob/regex `kind`.
    /// `None` (line omitted or invalid) means "no opinion": the frontend keeps the user's
    /// current Both/Files/Folders choice. Any other value drops to `None`.
    pub item_type: Option<String>,
    /// Minimum size in bytes.
    pub size_min: Option<u64>,
    /// Maximum size in bytes.
    pub size_max: Option<u64>,
    /// ISO date (`YYYY-MM-DD`) for "modified on or after".
    pub modified_after: Option<String>,
    /// ISO date (`YYYY-MM-DD`) for "modified strictly before".
    pub modified_before: Option<String>,
    /// Optional caveat for the AI transparency strip.
    pub note: Option<String>,
    /// Optional short label for breadcrumb / history UX.
    pub label: Option<String>,
}

impl ParsedSelectionLlmResponse {
    /// Returns `true` if every field is `None` (LLM emitted nothing useful).
    pub fn is_empty(&self) -> bool {
        self.pattern.is_none()
            && self.kind.is_none()
            && self.item_type.is_none()
            && self.size_min.is_none()
            && self.size_max.is_none()
            && self.modified_after.is_none()
            && self.modified_before.is_none()
            && self.note.is_none()
            && self.label.is_none()
    }
}

/// Parses a key-value LLM response into a `ParsedSelectionLlmResponse`.
///
/// Each line is split on the first `:` only (values may contain colons, for example
/// regex patterns or ISO timestamps). Empty values and unknown keys are skipped. The
/// `kind` field is validated against `"glob"` / `"regex"`; unknown values drop to
/// `None`. Size fields are parsed as `u64`; non-integer values drop to `None`.
pub fn parse_selection_response(response: &str) -> ParsedSelectionLlmResponse {
    let mut parsed = ParsedSelectionLlmResponse::default();
    for line in response.lines() {
        let line = line.trim();
        if let Some((key, value)) = line.split_once(':') {
            let key = key.trim().to_lowercase();
            let value = value.trim().to_string();
            if value.is_empty() {
                continue;
            }
            match key.as_str() {
                "pattern" => parsed.pattern = Some(value),
                "kind" => parsed.kind = validate_kind(&value),
                "type" => parsed.item_type = validate_item_type(&value),
                "size_min" | "sizemin" => parsed.size_min = parse_u64(&value),
                "size_max" | "sizemax" => parsed.size_max = parse_u64(&value),
                "modified_after" | "modifiedafter" => parsed.modified_after = validate_iso_date(&value),
                "modified_before" | "modifiedbefore" => parsed.modified_before = validate_iso_date(&value),
                "note" => parsed.note = Some(value),
                "label" => parsed.label = Some(value),
                _ => {} // unknown field, skip
            }
        }
    }
    parsed
}

fn validate_kind(value: &str) -> Option<String> {
    let v = value.trim().to_lowercase();
    match v.as_str() {
        "glob" | "regex" => Some(v),
        _ => None,
    }
}

/// Accept only `file` / `folder` for the type dimension. `both`, `any`, and anything else drop
/// to `None` so the frontend keeps the user's current Both/Files/Folders choice (leave-alone).
/// The model is told to OMIT the line for "both", but `both` is mapped to `None` defensively.
fn validate_item_type(value: &str) -> Option<String> {
    let v = value.trim().to_lowercase();
    match v.as_str() {
        "file" | "folder" => Some(v),
        _ => None,
    }
}

/// Parse a `u64` from a value the LLM may have decorated with underscores, commas, or
/// stray whitespace. Returns `None` for anything that isn't a non-negative integer.
fn parse_u64(value: &str) -> Option<u64> {
    let cleaned: String = value
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '_' && *c != ',')
        .collect();
    cleaned.parse::<u64>().ok()
}

/// Accept only well-formed `YYYY-MM-DD` strings. Anything else (`2024`, `last_week`,
/// `2024-13-99`) drops to `None` so the matcher doesn't apply a broken filter.
fn validate_iso_date(value: &str) -> Option<String> {
    let v = value.trim();
    let format = time::macros::format_description!("[year]-[month]-[day]");
    if time::Date::parse(v, &format).is_ok() {
        Some(v.to_string())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_well_formed_response() {
        let response = "\
pattern: *.log
kind: glob
type: file
size_min: 1024
size_max: 1048576
modified_after: 2026-01-01
modified_before: 2026-02-01
note: can't filter by content
label: Recent log files";
        let parsed = parse_selection_response(response);
        assert_eq!(parsed.pattern.as_deref(), Some("*.log"));
        assert_eq!(parsed.kind.as_deref(), Some("glob"));
        assert_eq!(parsed.item_type.as_deref(), Some("file"));
        assert_eq!(parsed.size_min, Some(1024));
        assert_eq!(parsed.size_max, Some(1_048_576));
        assert_eq!(parsed.modified_after.as_deref(), Some("2026-01-01"));
        assert_eq!(parsed.modified_before.as_deref(), Some("2026-02-01"));
        assert_eq!(parsed.note.as_deref(), Some("can't filter by content"));
        assert_eq!(parsed.label.as_deref(), Some("Recent log files"));
    }

    #[test]
    fn parse_minimal_response() {
        let parsed = parse_selection_response("pattern: *.png\nkind: glob");
        assert_eq!(parsed.pattern.as_deref(), Some("*.png"));
        assert_eq!(parsed.kind.as_deref(), Some("glob"));
        assert!(parsed.size_min.is_none());
        assert!(parsed.modified_after.is_none());
    }

    #[test]
    fn parse_unknown_fields_are_skipped() {
        let parsed = parse_selection_response("pattern: *.png\nflavor: vanilla\nmood: cheerful");
        assert_eq!(parsed.pattern.as_deref(), Some("*.png"));
    }

    #[test]
    fn parse_garbage_response_yields_empty() {
        let parsed = parse_selection_response("nothing useful here\nat all\n!!!");
        assert!(parsed.is_empty());
    }

    #[test]
    fn parse_malformed_lines() {
        let parsed = parse_selection_response("no colon here\npattern:\nkind: glob\n: empty key");
        assert!(parsed.pattern.is_none(), "empty value skipped");
        assert_eq!(parsed.kind.as_deref(), Some("glob"));
    }

    #[test]
    fn parse_unknown_kind_dropped() {
        let parsed = parse_selection_response("pattern: x\nkind: fuzzy");
        assert_eq!(parsed.pattern.as_deref(), Some("x"));
        assert!(parsed.kind.is_none());
    }

    #[test]
    fn parse_item_type_file_and_folder() {
        let parsed = parse_selection_response("type: file");
        assert_eq!(parsed.item_type.as_deref(), Some("file"));
        let parsed = parse_selection_response("type: FOLDER");
        assert_eq!(parsed.item_type.as_deref(), Some("folder"));
    }

    #[test]
    fn parse_item_type_both_and_unknown_drop_to_none() {
        // "both" means "no opinion" — the frontend keeps the user's current type, so we map it
        // to None rather than carrying a third value across the wire.
        assert!(parse_selection_response("type: both").item_type.is_none());
        assert!(parse_selection_response("type: any").item_type.is_none());
        assert!(parse_selection_response("type: banana").item_type.is_none());
    }

    #[test]
    fn parse_invalid_iso_date_dropped() {
        let parsed = parse_selection_response("modified_after: last_week");
        assert!(parsed.modified_after.is_none());

        let parsed = parse_selection_response("modified_after: 2024-13-99");
        assert!(parsed.modified_after.is_none());
    }

    #[test]
    fn parse_size_accepts_underscores_and_commas() {
        let parsed = parse_selection_response("size_min: 1_048_576");
        assert_eq!(parsed.size_min, Some(1_048_576));
        let parsed = parse_selection_response("size_max: 5,242,880");
        assert_eq!(parsed.size_max, Some(5_242_880));
    }

    #[test]
    fn parse_size_rejects_units() {
        // The prompt asks for bytes; if the model emits "5mb" we'd rather drop it
        // than guess at the unit and silently mis-filter.
        let parsed = parse_selection_response("size_min: 5mb");
        assert!(parsed.size_min.is_none());
    }

    #[test]
    fn parse_alt_keys_accepted() {
        // The prompt always uses snake_case, but the model occasionally collapses to
        // lowercase camel. Accept the variants we know to be common.
        let parsed = parse_selection_response("sizemin: 100\nsizemax: 200\nmodifiedafter: 2026-01-01");
        assert_eq!(parsed.size_min, Some(100));
        assert_eq!(parsed.size_max, Some(200));
        assert_eq!(parsed.modified_after.as_deref(), Some("2026-01-01"));
    }

    #[test]
    fn parse_value_with_colons_preserved() {
        // Regex patterns may contain `:` (lookarounds, character classes). The split
        // is on the first `:` only.
        let parsed = parse_selection_response("pattern: ^[a-z]+(?:\\.log)$\nkind: regex");
        assert_eq!(parsed.pattern.as_deref(), Some("^[a-z]+(?:\\.log)$"));
        assert_eq!(parsed.kind.as_deref(), Some("regex"));
    }

    #[test]
    fn parse_extra_whitespace_and_mixed_case_keys() {
        let parsed = parse_selection_response("  Pattern :  *.PDF  \n  Kind : GLOB ");
        assert_eq!(parsed.pattern.as_deref(), Some("*.PDF"));
        assert_eq!(parsed.kind.as_deref(), Some("glob"));
    }

    #[test]
    fn empty_response_is_empty() {
        assert!(parse_selection_response("").is_empty());
    }
}
