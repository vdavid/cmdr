//! Tests for the `Matcher` type that drives literal and regex search.

use std::ops::ControlFlow;

use proptest::prelude::*;

use super::search_matcher::{Matcher, MatcherBuildError, SearchMode};

fn literal_mode(case_sensitive: bool) -> SearchMode {
    SearchMode {
        use_regex: false,
        case_sensitive,
    }
}

fn regex_mode(case_sensitive: bool) -> SearchMode {
    SearchMode {
        use_regex: true,
        case_sensitive,
    }
}

fn collect_matches(matcher: &Matcher, line: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    matcher.find_matches(line, |start, end| {
        out.push((start, end));
        ControlFlow::Continue(())
    });
    out
}

#[test]
fn literal_case_sensitive_matches_exact_only() {
    let m = Matcher::build("Error", literal_mode(true)).unwrap();
    assert_eq!(collect_matches(&m, "Error here"), vec![(0, 5)]);
    assert_eq!(collect_matches(&m, "error here"), Vec::<(usize, usize)>::new());
    assert_eq!(collect_matches(&m, "ERROR here"), Vec::<(usize, usize)>::new());
}

#[test]
fn literal_case_insensitive_matches_any_casing() {
    let m = Matcher::build("error", literal_mode(false)).unwrap();
    assert_eq!(collect_matches(&m, "Error").len(), 1);
    assert_eq!(collect_matches(&m, "ERROR").len(), 1);
    assert_eq!(collect_matches(&m, "eRrOr").len(), 1);
}

#[test]
fn literal_finds_all_occurrences() {
    let m = Matcher::build("ab", literal_mode(true)).unwrap();
    let matches = collect_matches(&m, "ababab");
    assert_eq!(matches, vec![(0, 2), (2, 4), (4, 6)]);
}

#[test]
fn regex_case_sensitive_matches_digits() {
    let m = Matcher::build(r"\d+", regex_mode(true)).unwrap();
    let matches = collect_matches(&m, "a123b456c");
    assert_eq!(matches, vec![(1, 4), (5, 8)]);
}

#[test]
fn regex_case_insensitive_matches_any_casing() {
    let m = Matcher::build("error", regex_mode(false)).unwrap();
    assert_eq!(collect_matches(&m, "Error").len(), 1);
    assert_eq!(collect_matches(&m, "ERROR").len(), 1);
}

#[test]
fn empty_query_returns_no_matches() {
    let m = Matcher::build("", literal_mode(true)).unwrap();
    assert!(collect_matches(&m, "anything").is_empty());
}

#[test]
fn invalid_regex_returns_error() {
    let err = Matcher::build("(unclosed", regex_mode(true)).unwrap_err();
    assert!(matches!(err, MatcherBuildError::InvalidRegex(_)));
}

#[test]
fn regex_with_s_flag_is_rejected() {
    let err = Matcher::build("(?s).", regex_mode(true)).unwrap_err();
    assert!(matches!(err, MatcherBuildError::MultilineNotSupported));
}

#[test]
fn regex_with_literal_newline_is_rejected() {
    let err = Matcher::build("a\nb", regex_mode(true)).unwrap_err();
    assert!(matches!(err, MatcherBuildError::MultilineNotSupported));
}

#[test]
fn regex_with_escaped_n_is_rejected() {
    // `\n` in the regex pattern matches an actual newline byte; since we stream
    // line-by-line and a line never contains `\n`, this pattern can never match.
    // Treat it as a multiline pattern.
    let err = Matcher::build(r"a\nb", regex_mode(true)).unwrap_err();
    assert!(matches!(err, MatcherBuildError::MultilineNotSupported));
}

#[test]
fn regex_with_m_flag_is_accepted() {
    // (?m) only changes ^/$ semantics within the current slice; safe for our
    // streaming model.
    let m = Matcher::build(r"(?m)^foo", regex_mode(true)).unwrap();
    assert_eq!(collect_matches(&m, "foo"), vec![(0, 3)]);
    assert!(collect_matches(&m, "barfoo").is_empty());
}

#[test]
fn regex_too_complex_is_rejected() {
    // A pattern whose compiled NFA / DFA exceeds 8 MB; `(?:a|b){20}` style explosions
    // get rejected.
    let pattern = "a{1000}{1000}";
    let err = Matcher::build(pattern, regex_mode(true)).unwrap_err();
    assert!(matches!(err, MatcherBuildError::InvalidRegex(_)));
}

#[test]
fn literal_pattern_special_chars_treated_literally() {
    // In literal mode, regex metacharacters are not interpreted.
    let m = Matcher::build("[[", literal_mode(true)).unwrap();
    assert_eq!(collect_matches(&m, "a[[b"), vec![(1, 3)]);

    let m2 = Matcher::build(".*", literal_mode(true)).unwrap();
    assert_eq!(collect_matches(&m2, "x.*y.*z"), vec![(1, 3), (4, 6)]);
}

#[test]
fn callback_break_stops_iteration() {
    let m = Matcher::build("a", literal_mode(true)).unwrap();
    let mut count = 0;
    m.find_matches("aaaa", |_start, _end| {
        count += 1;
        if count == 2 {
            ControlFlow::Break(())
        } else {
            ControlFlow::Continue(())
        }
    });
    assert_eq!(count, 2);
}

#[test]
fn chunked_match_at_end_of_chunk() {
    // A 5 MB line with one match near the end.
    let mut line = String::with_capacity(5_000_000);
    line.push_str(&"x".repeat(4_999_990));
    line.push_str("MATCH!");

    let m = Matcher::build("MATCH!", literal_mode(true)).unwrap();
    let matches = collect_matches(&m, &line);
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0], (4_999_990, 4_999_996));
}

#[test]
fn chunked_match_spans_boundary() {
    // Construct a 2 MB line whose match straddles the 1 MB chunk boundary.
    // HUGE_LINE_THRESHOLD is 1 MB; overlap is 256 bytes. Place a 10-byte needle
    // around the boundary at offset 1_048_576 - 5.
    let needle = "NEEDLEHERE";
    let mut line = String::with_capacity(2_000_000);
    let prefix_len = 1_048_576 - 5;
    line.push_str(&"x".repeat(prefix_len));
    line.push_str(needle);
    line.push_str(&"y".repeat(2_000_000 - prefix_len - needle.len()));

    let m = Matcher::build(needle, literal_mode(true)).unwrap();
    let matches = collect_matches(&m, &line);
    assert_eq!(matches.len(), 1, "needle straddling boundary must be found once");
    assert_eq!(matches[0], (prefix_len, prefix_len + needle.len()));
}

#[test]
fn chunked_overlap_match_not_duplicated() {
    // A match landing exactly in the overlap region should be reported by the
    // chunk it starts in, not the previous chunk's overlap.
    let needle = "abcdef";
    let mut line = String::with_capacity(2_500_000);
    // Place the needle 50 bytes past the chunk boundary (within overlap from chunk N+1's POV,
    // but starts past the (threshold - overlap) cutoff for chunk N).
    let pos = 1_048_576 + 50;
    line.push_str(&"x".repeat(pos));
    line.push_str(needle);
    line.push_str(&"y".repeat(2_500_000 - pos - needle.len()));

    let m = Matcher::build(needle, literal_mode(true)).unwrap();
    let matches = collect_matches(&m, &line);
    assert_eq!(matches.len(), 1, "match in overlap must not be duplicated");
    assert_eq!(matches[0].0, pos);
}

#[test]
fn chunked_regex_finds_match_in_huge_line() {
    let mut line = String::with_capacity(5_000_000);
    line.push_str(&"a".repeat(2_000_000));
    line.push_str("123");
    line.push_str(&"a".repeat(3_000_000 - 3));

    let m = Matcher::build(r"\d+", regex_mode(true)).unwrap();
    let matches = collect_matches(&m, &line);
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0], (2_000_000, 2_000_003));
}

proptest! {
    /// Literal-mode `find_matches` results equal naive `str::find` loop for random inputs.
    #[test]
    fn prop_literal_matches_naive(
        needle in "[a-zA-Z0-9]{1,8}",
        haystack in "[a-zA-Z0-9]{0,200}",
    ) {
        let m = Matcher::build(&needle, literal_mode(true)).unwrap();
        let got = collect_matches(&m, &haystack);

        let mut expected = Vec::new();
        let mut start = 0;
        while let Some(rel) = haystack[start..].find(&needle) {
            let abs = start + rel;
            expected.push((abs, abs + needle.len()));
            start = abs + needle.len();
        }
        prop_assert_eq!(got, expected);
    }

    /// `regex::escape(needle)` in regex mode produces the same match positions as literal mode.
    #[test]
    fn prop_regex_escape_equivalent_to_literal(
        needle in "[a-zA-Z0-9.*+?\\[\\](){}|^$\\\\]{1,8}",
        haystack in "[a-zA-Z0-9.*+?\\[\\](){}|^$\\\\]{0,200}",
    ) {
        let literal = Matcher::build(&needle, literal_mode(true)).unwrap();
        let escaped = regex::escape(&needle);
        // Skip patterns that would be rejected (e.g. literal-newline escapes).
        if let Ok(regex_m) = Matcher::build(&escaped, regex_mode(true)) {
            prop_assert_eq!(
                collect_matches(&literal, &haystack),
                collect_matches(&regex_m, &haystack)
            );
        }
    }
}
