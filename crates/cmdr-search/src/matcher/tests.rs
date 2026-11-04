//! Unit tests for the compiled query.
//!
//! The engine's own tests (`search/engine/tests/`) are the oracle for behavior
//! preservation: they run the whole `search_ranked` path and were written before this
//! module existed. These tests pin the extracted unit directly, so a rule that breaks
//! points at the rule rather than at a result count.

use super::*;

/// Compile under this machine's own name rules, as the desktop does.
fn compile(query: &SearchQuery, evaluator: Evaluator) -> Result<CompiledQuery, CompileError> {
    CompiledQuery::compile(query, evaluator, &NameRules::host())
}

/// A query with every field at its default. Tests set only what they're about.
fn query() -> SearchQuery {
    SearchQuery {
        name_pattern: None,
        pattern_type: PatternType::Glob,
        min_size: None,
        max_size: None,
        modified_after: None,
        modified_before: None,
        is_directory: None,
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

/// One edit to a query, labelled for the failure message it would produce.
type QueryEdit = (&'static str, fn(&mut SearchQuery));

/// An evaluator that never refuses a broad query, for the tests that aren't about
/// the guard.
const SMALL_ARENA: Evaluator = Evaluator::Arena { entries: 10 };

fn file(name: &str) -> Candidate<'_> {
    Candidate {
        name,
        is_directory: false,
        size: Some(1_000),
        modified_at: Some(1_000),
    }
}

fn dir(name: &str) -> Candidate<'_> {
    Candidate {
        name,
        is_directory: true,
        size: None,
        modified_at: Some(1_000),
    }
}

// ── Name pattern ─────────────────────────────────────────────────────

#[test]
fn a_wildcard_free_glob_matches_a_substring() {
    let mut q = query();
    q.name_pattern = Some("ote".to_string());
    let compiled = compile(&q, SMALL_ARENA).unwrap();

    assert!(compiled.matches(&file("notes.txt")));
    assert!(!compiled.matches(&file("report.pdf")));
}

#[test]
fn a_glob_with_a_wildcard_is_left_anchored_as_typed() {
    let mut q = query();
    q.name_pattern = Some("report*".to_string());
    let compiled = compile(&q, SMALL_ARENA).unwrap();

    assert!(compiled.matches(&file("report.pdf")));
    // Not auto-wrapped, so a name that merely CONTAINS "report" is out.
    assert!(!compiled.matches(&file("Q1-report.pdf")));
}

#[test]
fn a_question_mark_matches_exactly_one_character() {
    let mut q = query();
    q.name_pattern = Some("Q?-report.pdf".to_string());
    let compiled = compile(&q, SMALL_ARENA).unwrap();

    assert!(compiled.matches(&file("Q1-report.pdf")));
    assert!(!compiled.matches(&file("Q12-report.pdf")));
}

#[test]
fn a_regex_query_is_used_verbatim() {
    let mut q = query();
    q.name_pattern = Some(r"Q[1-4].*\.pdf".to_string());
    q.pattern_type = PatternType::Regex;
    let compiled = compile(&q, SMALL_ARENA).unwrap();

    assert!(compiled.matches(&file("Q1-report.pdf")));
    assert!(!compiled.matches(&file("Q5-report.pdf")));
}

#[test]
fn an_unbuildable_pattern_is_refused_with_the_reason() {
    let mut q = query();
    q.name_pattern = Some("[unclosed".to_string());
    q.pattern_type = PatternType::Regex;

    let err = compile(&q, SMALL_ARENA).unwrap_err();
    assert!(matches!(err, CompileError::InvalidPattern(_)));
    // The sentence the IPC boundary carries; the frontend shows it as typed.
    assert!(err.to_string().starts_with("Invalid pattern: "));
}

#[test]
fn an_empty_pattern_is_no_pattern_at_all() {
    // No regex is compiled and none runs per entry, which is why `narrows()` treats
    // an empty pattern as no pattern too.
    let mut q = query();
    q.name_pattern = Some(String::new());
    let compiled = compile(&q, SMALL_ARENA).unwrap();

    assert!(compiled.matches(&file("anything-at-all")));
    assert!(compiled.matches(&file(NEWLINE_NAME)));
}

// ── Newlines in filenames ────────────────────────────────────────────

/// A filename with a newline in it. Legal on every platform Cmdr runs on, and the
/// kind of thing a script or a bad download leaves behind.
const NEWLINE_NAME: &str = "we\nird.txt";

#[test]
fn a_newline_in_a_name_doesnt_hide_it_from_a_glob() {
    // `*` means "any characters", so it means that one too.
    let mut wildcard = query();
    wildcard.name_pattern = Some("*.txt".to_string());
    assert!(compile(&wildcard, SMALL_ARENA).unwrap().matches(&file(NEWLINE_NAME)));

    // And the plain-text query, which is a glob wrapped in `*…*`.
    let mut plain = query();
    plain.name_pattern = Some("ird".to_string());
    assert!(compile(&plain, SMALL_ARENA).unwrap().matches(&file(NEWLINE_NAME)));

    // `?` is one character, and a newline is a character.
    let mut single = query();
    single.name_pattern = Some("we?ird.txt".to_string());
    assert!(compile(&single, SMALL_ARENA).unwrap().matches(&file(NEWLINE_NAME)));
}

#[test]
fn a_case_insensitive_accented_glob_still_crosses_a_newline() {
    // Three rules at once, because they're carried by three different mechanisms:
    // the dot-matches-newline flag `glob_to_regex` emits inline, the case-insensitivity
    // the builder sets, and the decomposed form the candidate is matched raw against.
    // Inline flags and builder flags interacting is the kind of thing that looks
    // obviously fine and isn't, so this pins all three at once.
    let mut q = query();
    q.case_sensitive = Some(false); // explicit, so Linux and macOS agree
    q.name_pattern = Some("cafe\u{301}*".to_string()); // already NFD, likewise
    let compiled = compile(&q, SMALL_ARENA).unwrap();

    assert!(compiled.matches(&file("CAFE\u{301}\nX.TXT")));
    // Still anchored at the start: case folding and the newline flag don't loosen it.
    assert!(!compiled.matches(&file("XCAFE\u{301}\nX.TXT")));
}

#[test]
fn a_regex_query_keeps_standard_dot_semantics() {
    // The opposite call from the glob one, deliberately: someone writing a regex
    // expects `.` not to cross a newline, and `(?s)` when they want it to. Changing
    // that under them would make Cmdr's regex mode a dialect.
    let mut strict = query();
    strict.name_pattern = Some(r"we.ird\.txt".to_string());
    strict.pattern_type = PatternType::Regex;
    assert!(!compile(&strict, SMALL_ARENA).unwrap().matches(&file(NEWLINE_NAME)));

    let mut opted_in = query();
    opted_in.name_pattern = Some(r"(?s)we.ird\.txt".to_string());
    opted_in.pattern_type = PatternType::Regex;
    assert!(compile(&opted_in, SMALL_ARENA).unwrap().matches(&file(NEWLINE_NAME)));
}

// ── Case folding ─────────────────────────────────────────────────────

#[test]
fn the_platform_default_decides_case_folding() {
    let compiled = compile(&query(), SMALL_ARENA).unwrap();
    assert_eq!(compiled.case_insensitive(), cfg!(target_os = "macos"));
}

#[test]
fn a_query_can_override_the_platform_default_both_ways() {
    let mut insensitive = query();
    insensitive.case_sensitive = Some(false);
    insensitive.name_pattern = Some("NOTES.*".to_string());
    let compiled = compile(&insensitive, SMALL_ARENA).unwrap();
    assert!(compiled.case_insensitive());
    assert!(compiled.matches(&file("notes.txt")));

    let mut sensitive = query();
    sensitive.case_sensitive = Some(true);
    sensitive.name_pattern = Some("NOTES.*".to_string());
    let compiled = compile(&sensitive, SMALL_ARENA).unwrap();
    assert!(!compiled.case_insensitive());
    assert!(!compiled.matches(&file("notes.txt")));
}

// ── NFD normalization (macOS) ────────────────────────────────────────

// The two forms and both tests over them are macOS-only, and the CONSTANTS have to
// carry the gate too: Linux builds with `-D unused`, so a constant whose only
// readers are gated out is a hard error there rather than a warning.

/// "café" composed (NFC): the form a keyboard produces.
#[cfg(target_os = "macos")]
const CAFE_NFC: &str = "caf\u{e9}.txt";
/// "café" decomposed (NFD): the form APFS stores, and the form both a walk and the
/// index read back off the filesystem.
#[cfg(target_os = "macos")]
const CAFE_NFD: &str = "cafe\u{301}.txt";

#[cfg(target_os = "macos")]
#[test]
fn a_composed_pattern_matches_the_decomposed_name_apfs_stores() {
    let mut q = query();
    q.name_pattern = Some(CAFE_NFC.to_string());
    let compiled = compile(&q, SMALL_ARENA).unwrap();

    assert!(compiled.matches(&file(CAFE_NFD)));
}

#[cfg(target_os = "macos")]
#[test]
fn a_candidate_name_is_matched_raw_never_normalized() {
    // The PATTERN is normalized, the candidate is not: normalizing per entry would
    // cost an allocation per row and would be a second rule to keep in sync. The
    // consequence is that a composed name doesn't match — which is fine precisely
    // because it holds identically for an arena row and for a walked entry.
    let mut q = query();
    q.name_pattern = Some(CAFE_NFC.to_string());
    let compiled = compile(&q, SMALL_ARENA).unwrap();

    assert!(!compiled.matches(&file(CAFE_NFC)));
}

// ── Type, size, and date predicates ──────────────────────────────────

#[test]
fn a_type_filter_keeps_only_its_own_kind() {
    let mut dirs_only = query();
    dirs_only.is_directory = Some(true);
    let compiled = compile(&dirs_only, SMALL_ARENA).unwrap();
    assert!(compiled.matches(&dir("Documents")));
    assert!(!compiled.matches(&file("notes.txt")));

    let mut files_only = query();
    files_only.is_directory = Some(false);
    let compiled = compile(&files_only, SMALL_ARENA).unwrap();
    assert!(!compiled.matches(&dir("Documents")));
    assert!(compiled.matches(&file("notes.txt")));
}

#[test]
fn size_bounds_are_inclusive_at_both_ends() {
    let mut q = query();
    q.min_size = Some(500);
    q.max_size = Some(1_500);
    let compiled = compile(&q, SMALL_ARENA).unwrap();

    let sized = |bytes| Candidate {
        name: "f",
        is_directory: false,
        size: Some(bytes),
        modified_at: None,
    };
    assert!(!compiled.matches(&sized(499)));
    assert!(compiled.matches(&sized(500)));
    assert!(compiled.matches(&sized(1_500)));
    assert!(!compiled.matches(&sized(1_501)));
}

#[test]
fn a_file_with_no_size_fails_any_size_bound() {
    let sizeless = Candidate {
        name: "f",
        is_directory: false,
        size: None,
        modified_at: None,
    };

    let mut min_only = query();
    min_only.min_size = Some(1);
    assert!(!compile(&min_only, SMALL_ARENA).unwrap().matches(&sizeless));

    let mut max_only = query();
    max_only.max_size = Some(1);
    assert!(!compile(&max_only, SMALL_ARENA).unwrap().matches(&sizeless));
}

#[test]
fn a_directory_ignores_size_bounds_entirely() {
    // Directory sizes come from `dir_stats` AFTER ranking (`execute.rs`), so the
    // matcher must pass every directory through a size filter untouched. Dropping
    // them here would drop them before the only place that knows their size.
    let mut q = query();
    q.min_size = Some(1_000_000);
    let compiled = compile(&q, SMALL_ARENA).unwrap();

    assert!(compiled.matches(&dir("Documents")));
}

#[test]
fn date_bounds_are_inclusive_at_both_ends() {
    let mut q = query();
    q.modified_after = Some(3_000);
    q.modified_before = Some(5_000);
    let compiled = compile(&q, SMALL_ARENA).unwrap();

    let at = |ts| Candidate {
        name: "f",
        is_directory: false,
        size: None,
        modified_at: Some(ts),
    };
    assert!(!compiled.matches(&at(2_999)));
    assert!(compiled.matches(&at(3_000)));
    assert!(compiled.matches(&at(5_000)));
    assert!(!compiled.matches(&at(5_001)));
}

#[test]
fn an_entry_with_no_modified_time_fails_any_date_bound() {
    let undated = Candidate {
        name: "f",
        is_directory: false,
        size: None,
        modified_at: None,
    };

    let mut after_only = query();
    after_only.modified_after = Some(1);
    assert!(!compile(&after_only, SMALL_ARENA).unwrap().matches(&undated));

    let mut before_only = query();
    before_only.modified_before = Some(1);
    assert!(!compile(&before_only, SMALL_ARENA).unwrap().matches(&undated));

    // A directory is exempt from SIZE bounds, not from date bounds.
    let mut dated = query();
    dated.modified_after = Some(1);
    let compiled = compile(&dated, SMALL_ARENA).unwrap();
    assert!(!compiled.matches(&Candidate {
        modified_at: None,
        ..dir("Documents")
    }));
}

#[test]
fn every_predicate_has_to_hold_at_once() {
    let mut q = query();
    q.name_pattern = Some("*.pdf".to_string());
    q.min_size = Some(1_500_000);
    let compiled = compile(&q, SMALL_ARENA).unwrap();

    let pdf = |name, size| Candidate {
        name,
        is_directory: false,
        size: Some(size),
        modified_at: None,
    };
    assert!(compiled.matches(&pdf("big.pdf", 2_000_000)));
    assert!(!compiled.matches(&pdf("small.pdf", 1_000)));
    assert!(!compiled.matches(&pdf("big.txt", 2_000_000)));
}

// ── The broad-query guard ────────────────────────────────────────────

/// Every predicate that counts as narrowing. Both evaluators are held to the same
/// list, so a new predicate that one of them forgets shows up as a failure.
const NARROWINGS: [QueryEdit; 6] = [
    ("name", |q| q.name_pattern = Some("x".to_string())),
    ("min size", |q| q.min_size = Some(1)),
    ("max size", |q| q.max_size = Some(1)),
    ("modified after", |q| q.modified_after = Some(1)),
    ("modified before", |q| q.modified_before = Some(1)),
    ("type", |q| q.is_directory = Some(true)),
];

#[test]
fn a_small_arena_serves_a_query_that_narrows_nothing() {
    let compiled = compile(&query(), Evaluator::Arena { entries: 10 });
    assert!(compiled.is_ok());
}

#[test]
fn a_big_arena_refuses_a_query_that_narrows_nothing() {
    let at_ceiling = Evaluator::Arena {
        entries: ARENA_BROAD_QUERY_CEILING,
    };
    assert!(compile(&query(), at_ceiling).is_ok());

    let over_ceiling = Evaluator::Arena {
        entries: ARENA_BROAD_QUERY_CEILING + 1,
    };
    assert_eq!(compile(&query(), over_ceiling).unwrap_err(), CompileError::TooBroad);
}

#[test]
fn any_one_predicate_narrows_enough_for_a_big_arena() {
    let big = Evaluator::Arena {
        entries: ARENA_BROAD_QUERY_CEILING + 1,
    };
    for (label, narrow) in NARROWINGS {
        let mut q = query();
        narrow(&mut q);
        assert!(compile(&q, big).is_ok(), "a {label} predicate should narrow enough");
    }
}

#[test]
fn an_empty_name_pattern_narrows_nothing() {
    let mut q = query();
    q.name_pattern = Some(String::new());
    let big = Evaluator::Arena {
        entries: ARENA_BROAD_QUERY_CEILING + 1,
    };
    assert_eq!(compile(&q, big).unwrap_err(), CompileError::TooBroad);
}

#[test]
fn a_live_walk_refuses_a_query_that_narrows_nothing() {
    // Unconditionally: there's no arena to weigh, and the ground a walk would read
    // is a filesystem of unknown size, over a network in the worst case.
    assert_eq!(
        compile(&query(), Evaluator::LiveWalk).unwrap_err(),
        CompileError::TooBroad
    );
}

#[test]
fn a_live_walk_serves_a_query_that_narrows_anything() {
    for (label, narrow) in NARROWINGS {
        let mut q = query();
        narrow(&mut q);
        assert!(
            compile(&q, Evaluator::LiveWalk).is_ok(),
            "a {label} predicate should narrow enough"
        );
    }
}

#[test]
fn the_too_broad_sentence_names_what_would_fix_it() {
    assert_eq!(
        CompileError::TooBroad.to_string(),
        "Query too broad. Add a filename pattern, size, date, or type filter to narrow results."
    );
}

#[test]
fn an_unusable_exclude_reads_as_a_fix_and_quotes_a_short_pattern() {
    let err = CompileError::InvalidExclude {
        pattern: "?".repeat(200_000),
        reason: "size limit".to_string(),
    };
    // The quote stops at 40 characters, so a pathological glob can't flood the dialog.
    assert_eq!(
        err.to_string(),
        format!(
            "Cmdr can’t use the exclusion “{}…”. Try a shorter or simpler pattern, or remove it.",
            "?".repeat(40)
        )
    );

    let short = CompileError::InvalidExclude {
        pattern: "*.tmp".to_string(),
        reason: String::new(),
    };
    assert!(short.to_string().contains("“*.tmp”."));
}
