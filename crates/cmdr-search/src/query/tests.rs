use super::*;
use crate::local_days::FixedZone;
use time::macros::date;

// ── glob_to_regex ────────────────────────────────────────────────
//
// The `(?s)` prefix is part of the contract, not decoration: `*` and `?` have to
// reach a filename containing a newline. See `glob_to_regex`'s own docs.

#[test]
fn glob_to_regex_star() {
    assert_eq!(glob_to_regex("*.pdf"), r"(?s)^.*\.pdf$");
}

#[test]
fn glob_to_regex_question() {
    assert_eq!(glob_to_regex("file?.txt"), r"(?s)^file.\.txt$");
}

#[test]
fn glob_to_regex_escapes_metacharacters() {
    assert_eq!(glob_to_regex("a+b(c)"), r"(?s)^a\+b\(c\)$");
}

#[test]
fn glob_to_regex_literal() {
    assert_eq!(glob_to_regex("readme"), "(?s)^readme$");
}

// ── glob_to_regex (property-based) ───────────────────────────────
//
// The output of `glob_to_regex` is fed directly into `regex::Regex::new`
// by the search engine. A glob that escapes incorrectly would either
// panic the regex parser or silently match more than the user intended.
// These properties pin (a) the output is always a syntactically valid
// regex and (b) it matches the user's literal intent when no glob
// metacharacters are present.

mod glob_proptests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        /// For any glob string, the produced regex compiles successfully
        /// and is anchored end-to-end, behind the dot-matches-newline flag.
        #[test]
        fn output_is_valid_anchored_regex(glob in ".*") {
            let pattern = glob_to_regex(&glob);
            prop_assert!(pattern.starts_with("(?s)^"), "regex must start with (?s)^: {}", pattern);
            prop_assert!(pattern.ends_with('$'), "regex must end with $: {}", pattern);
            let compiled = regex::Regex::new(&pattern);
            prop_assert!(
                compiled.is_ok(),
                "regex must compile, got error for glob {:?}: {:?}",
                glob,
                compiled.err()
            );
        }

        /// For globs with no `*` or `?` (after the regex metachar set is
        /// taken into account), the compiled regex matches the original
        /// string literally and nothing else of different content.
        #[test]
        fn literal_globs_match_themselves(
            glob in "[A-Za-z0-9 ._+(){}\\[\\]^$|\\\\]{0,30}"
                .prop_filter("no glob metacharacters", |s: &String| {
                    !s.contains('*') && !s.contains('?')
                })
        ) {
            let pattern = glob_to_regex(&glob);
            let compiled = regex::Regex::new(&pattern).expect("must compile");
            prop_assert!(
                compiled.is_match(&glob),
                "regex {:?} must match its own literal glob {:?}",
                pattern, glob
            );
            // It must not match a string with a different last character.
            // Skip strings ending in `]` or other edge codepoints because
            // appending arbitrary content might collide with grapheme
            // clusters in surprising ways, so prepend instead.
            let modified = format!("X{glob}Y");
            prop_assert!(
                !compiled.is_match(&modified) || modified == glob,
                "regex for literal glob {:?} must not match longer {:?}",
                glob, modified
            );
        }

        /// For globs containing only `*` wildcards interleaved with
        /// literal segments, the compiled regex matches any string
        /// produced by replacing each `*` with the empty string OR an
        /// arbitrary literal segment — a newline included, since filenames
        /// may contain one and `*` means "any characters".
        #[test]
        fn star_matches_arbitrary_content(
            prefix in "[A-Za-z0-9_]{0,5}",
            middle in "[A-Za-z0-9_\n]{0,10}",
            suffix in "[A-Za-z0-9_]{0,5}"
        ) {
            let glob = format!("{prefix}*{suffix}");
            let pattern = glob_to_regex(&glob);
            let compiled = regex::Regex::new(&pattern).expect("must compile");
            let candidate = format!("{prefix}{middle}{suffix}");
            prop_assert!(
                compiled.is_match(&candidate),
                "regex {:?} for glob {:?} must match {:?}",
                pattern, glob, candidate
            );
        }
    }
}

// ── summarize_query ──────────────────────────────────────────────

fn make_query(
    name_pattern: Option<&str>,
    pattern_type: PatternType,
    min_size: Option<u64>,
    max_size: Option<u64>,
    modified_after: Option<u64>,
    modified_before: Option<u64>,
    is_directory: Option<bool>,
) -> SearchQuery {
    SearchQuery {
        name_pattern: name_pattern.map(|s| s.to_string()),
        pattern_type,
        min_size,
        max_size,
        modified_after,
        modified_before,
        is_directory,
        include_paths: None,
        exclude_dir_names: None,
        include_path_ids: None,
        count_only: false,
        limit: 30,
        case_sensitive: None,
        exclude_system_dirs: Some(false),
        sort_by: None,
    }
}

#[test]
fn summarize_empty_query() {
    let q = make_query(None, PatternType::Glob, None, None, None, None, None);
    assert_eq!(summarize_query(&q, &utc()), "(all entries)");
}

#[test]
fn summarize_name_only() {
    let q = make_query(Some("tes"), PatternType::Glob, None, None, None, None, None);
    assert_eq!(summarize_query(&q, &utc()), "\"tes\"");
}

#[test]
fn summarize_glob_pattern() {
    let q = make_query(Some("*.pdf"), PatternType::Glob, None, None, None, None, None);
    assert_eq!(summarize_query(&q, &utc()), "\"*.pdf\"");
}

#[test]
fn summarize_regex_pattern() {
    let q = make_query(Some("Q[1-4].*"), PatternType::Regex, None, None, None, None, None);
    assert_eq!(summarize_query(&q, &utc()), "\"Q[1-4].*\" (regex)");
}

#[test]
fn summarize_size_min() {
    let q = make_query(None, PatternType::Glob, Some(2 * 1024 * 1024), None, None, None, None);
    assert_eq!(summarize_query(&q, &utc()), "size >= 2 MiB");
}

#[test]
fn summarize_size_max() {
    let q = make_query(None, PatternType::Glob, None, Some(500 * 1024), None, None, None);
    assert_eq!(summarize_query(&q, &utc()), "size <= 500 KiB");
}

#[test]
fn summarize_size_range() {
    let q = make_query(
        None,
        PatternType::Glob,
        Some(1024 * 1024),
        Some(5 * 1024 * 1024 * 1024),
        None,
        None,
        None,
    );
    assert_eq!(summarize_query(&q, &utc()), "size 1 MiB\u{2013}5 GiB");
}

#[test]
fn summarize_date_after() {
    // 2025-01-01 00:00:00 UTC = 1735689600
    let q = make_query(None, PatternType::Glob, None, None, Some(1_735_689_600), None, None);
    assert_eq!(summarize_query(&q, &utc()), "last mod after 2025-01-01");
}

#[test]
fn summarize_date_before() {
    // 2026-03-01 00:00:00 UTC = 1772265600
    let q = make_query(None, PatternType::Glob, None, None, None, Some(1_772_323_200), None);
    assert_eq!(summarize_query(&q, &utc()), "last mod before 2026-03-01");
}

#[test]
fn summarize_date_range() {
    let q = make_query(
        None,
        PatternType::Glob,
        None,
        None,
        Some(1_735_689_600),
        Some(1_772_323_200),
        None,
    );
    assert_eq!(summarize_query(&q, &utc()), "last mod 2025-01-01\u{2013}2026-03-01");
}

#[test]
fn summarize_dirs_only() {
    let q = make_query(Some("*.pdf"), PatternType::Glob, None, None, None, None, Some(true));
    assert_eq!(summarize_query(&q, &utc()), "\"*.pdf\", dirs only");
}

#[test]
fn summarize_files_only() {
    let q = make_query(None, PatternType::Glob, None, None, None, None, Some(false));
    assert_eq!(summarize_query(&q, &utc()), "files only");
}

#[test]
fn summarize_combined() {
    let q = make_query(
        Some("tes"),
        PatternType::Glob,
        Some(2 * 1024 * 1024),
        None,
        None,
        Some(1_772_323_200),
        None,
    );
    assert_eq!(
        summarize_query(&q, &utc()),
        "\"tes\", size >= 2 MiB, last mod before 2026-03-01"
    );
}

#[test]
fn summarize_size_bytes() {
    let q = make_query(None, PatternType::Glob, Some(500), None, None, None, None);
    assert_eq!(summarize_query(&q, &utc()), "size >= 500 B");
}

#[test]
fn summarize_size_gb() {
    let q = make_query(
        None,
        PatternType::Glob,
        Some(1024 * 1024 * 1024),
        None,
        None,
        None,
        None,
    );
    assert_eq!(summarize_query(&q, &utc()), "size >= 1 GiB");
}

#[test]
fn summarize_empty_name_pattern() {
    let q = make_query(Some(""), PatternType::Glob, None, None, None, None, None);
    assert_eq!(summarize_query(&q, &utc()), "(all entries)");
}

// ── Diagnostic-safe summary ──────────────────────────────────────

#[test]
fn diagnostic_summary_keeps_shape_and_never_the_pattern() {
    const PATTERN_SENTINEL: &str = "Q🔒.*private-pattern";
    const SCOPE_SENTINEL_A: &str = "/Users/private-person/Downloads/secret scope a";
    const SCOPE_SENTINEL_B: &str = "/Volumes/private-drive/secret-scope-b";
    const EXCLUDE_SENTINEL_A: &str = "private-exclusion-a";
    const EXCLUDE_SENTINEL_B: &str = "private-exclusion-b";
    let mut q = make_query(
        Some(PATTERN_SENTINEL),
        PatternType::Regex,
        Some(1_024),
        Some(8_192),
        Some(1_735_689_600),
        Some(1_772_323_200),
        Some(false),
    );
    q.include_paths = Some(vec![SCOPE_SENTINEL_A.to_string(), SCOPE_SENTINEL_B.to_string()]);
    q.exclude_dir_names = Some(vec![EXCLUDE_SENTINEL_A.to_string(), EXCLUDE_SENTINEL_B.to_string()]);
    q.count_only = true;
    q.case_sensitive = Some(false);
    q.exclude_system_dirs = Some(false);

    // The local log keeps the scope whole: a 0-match line means nothing without it.
    let summary = summarize_query_for_diagnostics(&q);
    assert!(
        !summary.contains(PATTERN_SENTINEL),
        "the pattern literal never logs: {summary}"
    );
    assert_eq!(
        summary,
        format!(
            "pattern=regex(19 chars/22 bytes), size=min+max, modified=after+before, type=files, case=insensitive, count-only=true, scope=roots(2) [path={SCOPE_SENTINEL_A:?}, path={SCOPE_SENTINEL_B:?}], exclusions=2 [dir={EXCLUDE_SENTINEL_A:?}, dir={EXCLUDE_SENTINEL_B:?}], system-exclusions=off"
        )
    );
}

#[test]
fn diagnostic_scope_names_at_most_three_roots() {
    let mut q = make_query(None, PatternType::Glob, None, None, None, None, None);
    q.include_paths = Some((1..=5).map(|i| format!("/tmp/root-{i}")).collect());

    let summary = summarize_query_for_diagnostics(&q);

    assert!(
        summary.contains(r#"scope=roots(5) [path="/tmp/root-1", path="/tmp/root-2", path="/tmp/root-3", +2 more]"#),
        "{summary}"
    );
    assert!(summary.contains("scope=") && !summary.contains("root-4"), "{summary}");
}

#[test]
fn functional_summary_keeps_the_literal_pattern_for_mcp() {
    const FUNCTIONAL_SENTINEL: &str = "Q[private-functional].*";
    let q = make_query(
        Some(FUNCTIONAL_SENTINEL),
        PatternType::Regex,
        None,
        None,
        None,
        None,
        None,
    );

    assert_eq!(
        summarize_query(&q, &utc()),
        format!("\"{FUNCTIONAL_SENTINEL}\" (regex)")
    );
}

#[test]
fn diagnostic_summary_pluralizes_a_one_character_pattern() {
    let q = make_query(Some("x"), PatternType::Glob, None, None, None, None, None);

    assert!(summarize_query_for_diagnostics(&q).starts_with("pattern=glob(1 char),"));
}

// ── Local dates and times ────────────────────────────────────────
//
// Every date a person or the model reads is on the user's calendar. Three zones pin
// it: UTC-5 (New York in winter), UTC+9 (Tokyo), and Stockholm across its
// 2026-03-29 summer-time change (UTC+1 to UTC+2).

fn utc() -> FixedZone {
    FixedZone {
        today: date!(2026 - 11 - 04),
        offset_secs: 0,
        switch_on: None,
    }
}

fn zone(offset_secs: i64) -> FixedZone {
    FixedZone { offset_secs, ..utc() }
}

/// Stockholm around 2026-03-29. `FixedZone` switches at a local midnight rather than
/// at 02:00, which is close enough to pin "each instant takes its own offset".
fn stockholm_spring() -> FixedZone {
    FixedZone {
        offset_secs: 3_600,
        switch_on: Some((date!(2026 - 03 - 29), 7_200)),
        ..utc()
    }
}

/// 2026-01-15 03:30:00 UTC.
const EARLY_MORNING_UTC: u64 = 1_768_447_800;
/// 2026-01-15 20:30:00 UTC.
const EVENING_UTC: u64 = 1_768_509_000;

#[test]
fn a_date_west_of_utc_falls_on_the_local_day_before() {
    assert_eq!(zone(-5 * 3_600).format_date(EARLY_MORNING_UTC), "2026-01-14");
    assert_eq!(utc().format_date(EARLY_MORNING_UTC), "2026-01-15");
}

#[test]
fn a_date_east_of_utc_falls_on_the_local_day_after() {
    assert_eq!(zone(9 * 3_600).format_date(EVENING_UTC), "2026-01-16");
    assert_eq!(utc().format_date(EVENING_UTC), "2026-01-15");
}

#[test]
fn a_time_of_day_is_local_and_carries_its_offset() {
    assert_eq!(
        zone(-5 * 3_600).format_datetime(EARLY_MORNING_UTC),
        "2026-01-14T22:30:00-05:00"
    );
    assert_eq!(
        zone(9 * 3_600).format_datetime(EVENING_UTC),
        "2026-01-16T05:30:00+09:00"
    );
    assert_eq!(utc().format_datetime(EVENING_UTC), "2026-01-15T20:30:00+00:00");
}

#[test]
fn each_instant_takes_its_own_offset_across_a_summer_time_change() {
    let stockholm = stockholm_spring();
    // 2026-03-28 12:00 UTC: winter time.
    assert_eq!(stockholm.format_datetime(1_774_699_200), "2026-03-28T13:00:00+01:00");
    // 2026-03-29 12:00 UTC: summer time.
    assert_eq!(stockholm.format_datetime(1_774_785_600), "2026-03-29T14:00:00+02:00");
    // 2026-03-28 22:30 UTC: past the switch, so already the 29th here.
    assert_eq!(stockholm.format_date(1_774_737_000), "2026-03-29");
}

#[test]
fn a_summary_names_the_users_days() {
    // "After 2026-01-15" in Tokyo starts at 2026-01-14 15:00 UTC, which UTC reads as the 14th.
    let tokyo_midnight = 1_768_402_800;
    let q = make_query(None, PatternType::Glob, None, None, Some(tokyo_midnight), None, None);
    assert_eq!(summarize_query(&q, &zone(9 * 3_600)), "last mod after 2026-01-15");
    // "Before 2026-01-15" in New York ends at 2026-01-15 05:00 UTC.
    let new_york_midnight = 1_768_453_200;
    let q = make_query(None, PatternType::Glob, None, None, None, Some(new_york_midnight), None);
    assert_eq!(summarize_query(&q, &zone(-5 * 3_600)), "last mod before 2026-01-15");
}

// ── parse_scope ─────────────────────────────────────────────────

/// The home folder `~` expands to in these tests: a value, so they read the same on any machine.
const HOME: &str = "/Users/tester";

#[test]
fn parse_scope_basic_include() {
    let scope = parse_scope("~/projects", Some(HOME));
    let home = HOME;
    assert_eq!(scope.include_paths, vec![format!("{home}/projects")]);
    assert!(scope.exclude_patterns.is_empty());
}

#[test]
fn parse_scope_basic_exclude() {
    let scope = parse_scope("!node_modules", Some(HOME));
    assert!(scope.include_paths.is_empty());
    assert_eq!(scope.exclude_patterns, vec!["node_modules"]);
}

#[test]
fn parse_scope_mixed() {
    let scope = parse_scope("~/projects, !node_modules, !.git", Some(HOME));
    let home = HOME;
    assert_eq!(scope.include_paths, vec![format!("{home}/projects")]);
    assert_eq!(scope.exclude_patterns, vec!["node_modules", ".git"]);
}

#[test]
fn parse_scope_multiple_includes() {
    let scope = parse_scope("~/projects, ~/Documents", Some(HOME));
    let home = HOME;
    assert_eq!(
        scope.include_paths,
        vec![format!("{home}/projects"), format!("{home}/Documents")]
    );
}

#[test]
fn parse_scope_quoted_commas_double() {
    let scope = parse_scope("\"path,with,commas\"", Some(HOME));
    assert_eq!(scope.include_paths, vec!["path,with,commas"]);
}

#[test]
fn parse_scope_quoted_commas_single() {
    let scope = parse_scope("'path,with,commas'", Some(HOME));
    assert_eq!(scope.include_paths, vec!["path,with,commas"]);
}

#[test]
fn parse_scope_backslash_escaped_commas() {
    let scope = parse_scope("path\\,with\\,commas", Some(HOME));
    assert_eq!(scope.include_paths, vec!["path,with,commas"]);
}

#[test]
fn parse_scope_empty_segments() {
    let scope = parse_scope("~/projects, , !.git", Some(HOME));
    let home = HOME;
    assert_eq!(scope.include_paths, vec![format!("{home}/projects")]);
    assert_eq!(scope.exclude_patterns, vec![".git"]);
}

#[test]
fn parse_scope_bare_exclude_wildcard() {
    let scope = parse_scope("!.*", Some(HOME));
    assert_eq!(scope.exclude_patterns, vec![".*"]);
}

#[test]
fn parse_scope_absolute_exclude_path() {
    let scope = parse_scope("!/Users/test_user_4/Downloads", Some(HOME));
    assert_eq!(scope.exclude_patterns, vec!["/Users/test_user_4/Downloads"]);
}

#[test]
fn parse_scope_empty_input() {
    let scope = parse_scope("", Some(HOME));
    assert!(scope.include_paths.is_empty());
    assert!(scope.exclude_patterns.is_empty());
}

#[test]
fn parse_scope_whitespace_trimming() {
    let scope = parse_scope("  ~/projects  ,  !node_modules  ", Some(HOME));
    let home = HOME;
    assert_eq!(scope.include_paths, vec![format!("{home}/projects")]);
    assert_eq!(scope.exclude_patterns, vec!["node_modules"]);
}

// ── split_scope_segments + parse_scope (property-based) ──────────
//
// The scope parser has nested escape/quote rules. Property tests probe
// the round-trip and count invariants that don't require asserting a
// specific canonical form.

mod scope_proptests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        /// For inputs containing no special characters (no comma, no
        /// quotes, no backslash), the result is exactly `[input]` and
        /// the round-trip `segments.join(",") == input` holds.
        #[test]
        fn plain_input_round_trips(input in "[^,\"'\\\\]*") {
            let segments = split_scope_segments(&input);
            prop_assert_eq!(segments.len(), 1, "no commas → exactly 1 segment");
            prop_assert_eq!(&segments[0], &input, "the only segment must equal input");
            prop_assert_eq!(segments.join(","), input.clone(), "round-trip via join must match");
        }

        /// For inputs containing only safe characters and unquoted commas
        /// (no quotes, no backslashes), the segment count equals the
        /// comma count + 1.
        #[test]
        fn comma_count_matches_segment_count(input in "[^\"'\\\\]*") {
            let segments = split_scope_segments(&input);
            let comma_count = input.chars().filter(|&c| c == ',').count();
            prop_assert_eq!(
                segments.len(),
                comma_count + 1,
                "expected {} segments for input {:?}, got {:?}",
                comma_count + 1, input, segments
            );
            // And the join round-trips for this character class.
            prop_assert_eq!(segments.join(","), input);
        }

        /// `parse_scope` never panics, and the count of resolved
        /// include/exclude entries is bounded by the segment count.
        #[test]
        fn parse_scope_never_overcounts(input in ".*") {
            let scope = parse_scope(&input, Some(HOME));
            let segments = split_scope_segments(&input);
            prop_assert!(
                scope.include_paths.len() + scope.exclude_patterns.len() <= segments.len(),
                "parse_scope produced more entries than segments: input={:?}, scope={:?}, segments={:?}",
                input, scope, segments
            );
        }
    }
}
