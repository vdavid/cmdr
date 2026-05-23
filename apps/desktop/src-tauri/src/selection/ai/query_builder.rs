//! Assembles a `SelectionTranslateResult` from a parsed LLM response.
//!
//! Mirrors `crate::search::ai::query_builder` in spirit: pure functions over the
//! parsed response, no IPC concerns. The result is what crosses the wire to the
//! frontend selection dialog.

use serde::Serialize;

use super::parser::ParsedSelectionLlmResponse;

// ── Result type that crosses the IPC boundary ─────────────────────────────

/// The structured selection translation handed to the frontend. Mirrors Search's
/// `TranslateResult` minus the search-specific bits (scope, exclude_system_dirs,
/// is_directory, paths). The pattern always comes back ready to compile; the
/// frontend matcher decides whether to apply size and date predicates.
#[derive(Debug, Clone, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SelectionTranslateResult {
    /// The glob or regex string. `None` when the LLM didn't produce a usable pattern;
    /// the frontend then shows the caveat and doesn't apply any selection.
    pub pattern: Option<String>,
    /// `"glob"` or `"regex"`. `None` when `pattern` is `None`.
    pub kind: Option<String>,
    /// Minimum size in bytes (inclusive). `None` means no lower bound.
    pub size_min: Option<u64>,
    /// Maximum size in bytes (inclusive). `None` means no upper bound.
    pub size_max: Option<u64>,
    /// ISO date `YYYY-MM-DD`; matches files modified on or after this date.
    pub modified_after: Option<String>,
    /// ISO date `YYYY-MM-DD`; matches files modified strictly before this date.
    pub modified_before: Option<String>,
    /// Optional caveat the dialog renders in the AI transparency strip.
    pub caveat: Option<String>,
    /// Short label (≤40 chars) for breadcrumb / history UX.
    pub label: Option<String>,
}

// ── Assembly ──────────────────────────────────────────────────────────────

/// Builds a `SelectionTranslateResult` from a parsed LLM response.
///
/// Decision rules:
/// - If the LLM omitted `pattern` entirely, the result has `pattern: None`. The
///   frontend treats this as "didn't understand"; combined with the caveat below
///   the user sees a clear "couldn't translate" hint.
/// - If `kind` is missing but `pattern` is present, default to `"glob"`. The model
///   occasionally forgets the `kind:` line for obvious globs like `*.png`.
/// - Size and date filters pass through unchanged.
pub fn build_selection_translate_result(parsed: &ParsedSelectionLlmResponse) -> SelectionTranslateResult {
    let pattern = parsed
        .pattern
        .as_deref()
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(str::to_string);

    let kind = if pattern.is_some() {
        // Default to glob when the model omitted `kind:` but emitted a pattern.
        Some(parsed.kind.clone().unwrap_or_else(|| "glob".to_string()))
    } else {
        None
    };

    SelectionTranslateResult {
        pattern,
        kind,
        size_min: parsed.size_min,
        size_max: parsed.size_max,
        modified_after: parsed.modified_after.clone(),
        modified_before: parsed.modified_before.clone(),
        caveat: generate_caveat(parsed),
        label: build_label(parsed),
    }
}

// ── Caveat generation ─────────────────────────────────────────────────────

/// Generate a caveat string surfaced in the AI transparency strip.
///
/// Priority order:
/// 1. The LLM's `note:` field (sanitized to ≤200 chars, HTML angle brackets stripped).
/// 2. A built-in caveat when the response had no pattern AND no size/date filters
///    (so the user knows nothing got applied).
pub fn generate_caveat(parsed: &ParsedSelectionLlmResponse) -> Option<String> {
    if let Some(note) = &parsed.note {
        let sanitized: String = note.chars().filter(|c| *c != '<' && *c != '>').take(200).collect();
        let trimmed = sanitized.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }

    let has_pattern = parsed.pattern.as_deref().is_some_and(|p| !p.trim().is_empty());
    let has_size_filter = parsed.size_min.is_some() || parsed.size_max.is_some();
    let has_date_filter = parsed.modified_after.is_some() || parsed.modified_before.is_some();
    if !has_pattern && !has_size_filter && !has_date_filter {
        return Some("Couldn't translate the request. Try again or switch to Filename mode.".to_string());
    }

    None
}

// ── Label generation ──────────────────────────────────────────────────────

/// Maximum visible characters for a selection label. Mirrors Search's
/// `LABEL_MAX_CHARS` so both consumers truncate the same way.
const LABEL_MAX_CHARS: usize = 40;

/// Returns the LLM-produced label, trimmed and truncated. Returns `None` when the
/// LLM omitted the field or the value is blank after trimming.
pub fn build_label(parsed: &ParsedSelectionLlmResponse) -> Option<String> {
    let raw = parsed.label.as_deref()?.trim();
    if raw.is_empty() {
        return None;
    }
    let trimmed = raw.trim_end_matches(['.', '!', '?', ';', ':', ',']).trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(truncate_chars(trimmed, LABEL_MAX_CHARS))
}

fn truncate_chars(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let keep = max.saturating_sub(1).max(1);
    let kept: String = text.chars().take(keep).collect();
    let mut trimmed = kept.trim_end().to_string();
    trimmed.push('\u{2026}');
    trimmed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed_with_pattern(pattern: &str, kind: &str) -> ParsedSelectionLlmResponse {
        ParsedSelectionLlmResponse {
            pattern: Some(pattern.to_string()),
            kind: Some(kind.to_string()),
            ..Default::default()
        }
    }

    // ── Assembly ─────────────────────────────────────────────────────

    #[test]
    fn build_pattern_only() {
        let p = parsed_with_pattern("*.log", "glob");
        let r = build_selection_translate_result(&p);
        assert_eq!(r.pattern.as_deref(), Some("*.log"));
        assert_eq!(r.kind.as_deref(), Some("glob"));
        assert!(r.size_min.is_none());
        assert!(r.modified_after.is_none());
        assert!(r.caveat.is_none());
    }

    #[test]
    fn build_pattern_plus_size() {
        let p = ParsedSelectionLlmResponse {
            pattern: Some("*".to_string()),
            kind: Some("glob".to_string()),
            size_min: Some(5_242_880),
            ..Default::default()
        };
        let r = build_selection_translate_result(&p);
        assert_eq!(r.pattern.as_deref(), Some("*"));
        assert_eq!(r.size_min, Some(5_242_880));
        assert!(r.size_max.is_none());
    }

    #[test]
    fn build_pattern_plus_date() {
        let p = ParsedSelectionLlmResponse {
            pattern: Some("*backup*".to_string()),
            kind: Some("glob".to_string()),
            modified_after: Some("2026-05-15".to_string()),
            ..Default::default()
        };
        let r = build_selection_translate_result(&p);
        assert_eq!(r.modified_after.as_deref(), Some("2026-05-15"));
        assert!(r.modified_before.is_none());
    }

    #[test]
    fn build_defaults_kind_to_glob_when_missing() {
        let p = ParsedSelectionLlmResponse {
            pattern: Some("*.png".to_string()),
            kind: None,
            ..Default::default()
        };
        let r = build_selection_translate_result(&p);
        assert_eq!(r.kind.as_deref(), Some("glob"));
    }

    #[test]
    fn build_no_pattern_means_no_kind_either() {
        let p = ParsedSelectionLlmResponse {
            pattern: None,
            kind: Some("glob".to_string()),
            ..Default::default()
        };
        let r = build_selection_translate_result(&p);
        assert!(r.pattern.is_none());
        assert!(r.kind.is_none(), "kind must be cleared when pattern is absent");
    }

    #[test]
    fn build_empty_pattern_treated_as_missing() {
        let p = ParsedSelectionLlmResponse {
            pattern: Some("   ".to_string()),
            kind: Some("glob".to_string()),
            ..Default::default()
        };
        let r = build_selection_translate_result(&p);
        assert!(r.pattern.is_none());
        assert!(r.kind.is_none());
    }

    // ── Caveat generation ────────────────────────────────────────────

    #[test]
    fn caveat_from_llm_note() {
        let p = ParsedSelectionLlmResponse {
            pattern: Some("*.png".to_string()),
            kind: Some("glob".to_string()),
            note: Some("can't filter by tag".to_string()),
            ..Default::default()
        };
        assert_eq!(generate_caveat(&p).as_deref(), Some("can't filter by tag"));
    }

    #[test]
    fn caveat_truncated_at_200_chars() {
        let long = "a".repeat(300);
        let p = ParsedSelectionLlmResponse {
            note: Some(long),
            ..Default::default()
        };
        let caveat = generate_caveat(&p).unwrap();
        assert_eq!(caveat.len(), 200);
    }

    #[test]
    fn caveat_strips_html_brackets() {
        let p = ParsedSelectionLlmResponse {
            note: Some("avoid <script>tags</script> please".to_string()),
            ..Default::default()
        };
        let caveat = generate_caveat(&p).unwrap();
        assert!(!caveat.contains('<'));
        assert!(!caveat.contains('>'));
        assert!(caveat.contains("scripttags/script"));
    }

    #[test]
    fn caveat_falls_back_when_response_empty() {
        let p = ParsedSelectionLlmResponse::default();
        let caveat = generate_caveat(&p).unwrap();
        assert!(caveat.contains("Couldn't translate"));
    }

    #[test]
    fn caveat_silent_when_filter_only_response() {
        // Size-only or date-only responses are valid; no caveat needed.
        let p = ParsedSelectionLlmResponse {
            size_min: Some(1024),
            ..Default::default()
        };
        assert!(generate_caveat(&p).is_none());
    }

    #[test]
    fn caveat_silent_when_pattern_present() {
        let p = parsed_with_pattern("*.png", "glob");
        assert!(generate_caveat(&p).is_none());
    }

    #[test]
    fn caveat_blank_note_falls_back_when_response_otherwise_empty() {
        let p = ParsedSelectionLlmResponse {
            note: Some("   ".to_string()),
            ..Default::default()
        };
        let caveat = generate_caveat(&p).unwrap();
        assert!(caveat.contains("Couldn't translate"));
    }

    // ── Broken-LLM-response path ─────────────────────────────────────

    #[test]
    fn broken_response_returns_caveat_no_pattern() {
        // The LLM returned a kind but no pattern. We must NOT make up a pattern; we
        // must NOT compile a half-built query. We surface the caveat and let the user
        // try again.
        let p = ParsedSelectionLlmResponse {
            kind: Some("regex".to_string()),
            ..Default::default()
        };
        let r = build_selection_translate_result(&p);
        assert!(r.pattern.is_none());
        assert!(r.kind.is_none(), "kind must drop when pattern drops");
        assert!(r.caveat.is_some());
    }

    // ── Label generation ─────────────────────────────────────────────

    #[test]
    fn label_passes_short_value() {
        let p = ParsedSelectionLlmResponse {
            label: Some("Recent log files".to_string()),
            ..Default::default()
        };
        assert_eq!(build_label(&p).as_deref(), Some("Recent log files"));
    }

    #[test]
    fn label_truncates_long_value() {
        let p = ParsedSelectionLlmResponse {
            label: Some("a".repeat(80)),
            ..Default::default()
        };
        let label = build_label(&p).unwrap();
        assert_eq!(label.chars().count(), LABEL_MAX_CHARS);
        assert!(label.ends_with('\u{2026}'));
    }

    #[test]
    fn label_strips_trailing_punctuation() {
        let p = ParsedSelectionLlmResponse {
            label: Some("Recent log files.".to_string()),
            ..Default::default()
        };
        assert_eq!(build_label(&p).as_deref(), Some("Recent log files"));
    }

    #[test]
    fn label_returns_none_when_missing_or_blank() {
        assert!(build_label(&ParsedSelectionLlmResponse::default()).is_none());
        let p = ParsedSelectionLlmResponse {
            label: Some("   ".to_string()),
            ..Default::default()
        };
        assert!(build_label(&p).is_none());
    }

    // ── Result-level integration ─────────────────────────────────────

    #[test]
    fn full_result_serializes_to_camel_case() {
        let p = ParsedSelectionLlmResponse {
            pattern: Some("*.log".to_string()),
            kind: Some("glob".to_string()),
            size_min: Some(1024),
            modified_after: Some("2026-01-01".to_string()),
            label: Some("Log files".to_string()),
            ..Default::default()
        };
        let r = build_selection_translate_result(&p);
        let json = serde_json::to_string(&r).unwrap();
        assert!(json.contains("\"pattern\":\"*.log\""));
        assert!(json.contains("\"kind\":\"glob\""));
        assert!(json.contains("\"sizeMin\":1024"));
        assert!(json.contains("\"modifiedAfter\":\"2026-01-01\""));
        assert!(json.contains("\"label\":\"Log files\""));
    }
}
