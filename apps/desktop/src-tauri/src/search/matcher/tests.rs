//! Unit tests for the compiled query.
//!
//! The engine's own tests (`search/engine/tests/`) are the oracle for behavior
//! preservation: they run the whole `search_ranked` path and were written before this
//! module existed. These tests pin the extracted unit directly, so a rule that breaks
//! points at the rule rather than at a result count.

use std::path::PathBuf;

use super::*;

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
    let compiled = CompiledQuery::compile(&q, SMALL_ARENA).unwrap();

    assert!(compiled.matches(&file("notes.txt")));
    assert!(!compiled.matches(&file("report.pdf")));
}

#[test]
fn a_glob_with_a_wildcard_is_left_anchored_as_typed() {
    let mut q = query();
    q.name_pattern = Some("report*".to_string());
    let compiled = CompiledQuery::compile(&q, SMALL_ARENA).unwrap();

    assert!(compiled.matches(&file("report.pdf")));
    // Not auto-wrapped, so a name that merely CONTAINS "report" is out.
    assert!(!compiled.matches(&file("Q1-report.pdf")));
}

#[test]
fn a_question_mark_matches_exactly_one_character() {
    let mut q = query();
    q.name_pattern = Some("Q?-report.pdf".to_string());
    let compiled = CompiledQuery::compile(&q, SMALL_ARENA).unwrap();

    assert!(compiled.matches(&file("Q1-report.pdf")));
    assert!(!compiled.matches(&file("Q12-report.pdf")));
}

#[test]
fn a_regex_query_is_used_verbatim() {
    let mut q = query();
    q.name_pattern = Some(r"Q[1-4].*\.pdf".to_string());
    q.pattern_type = PatternType::Regex;
    let compiled = CompiledQuery::compile(&q, SMALL_ARENA).unwrap();

    assert!(compiled.matches(&file("Q1-report.pdf")));
    assert!(!compiled.matches(&file("Q5-report.pdf")));
}

#[test]
fn an_unbuildable_pattern_is_refused_with_the_reason() {
    let mut q = query();
    q.name_pattern = Some("[unclosed".to_string());
    q.pattern_type = PatternType::Regex;

    let err = CompiledQuery::compile(&q, SMALL_ARENA).unwrap_err();
    assert!(matches!(err, CompileError::InvalidPattern(_)));
    // The sentence the IPC boundary carries; the frontend shows it as typed.
    assert!(err.to_string().starts_with("Invalid pattern: "));
}

#[test]
fn an_empty_pattern_is_no_pattern_at_all() {
    // Not "a pattern that happens to match nearly everything". An empty glob would
    // compile to `^.*.*$`, and `.` doesn't cross a newline, so a filename containing
    // one — legal on Unix — would drop out of a query that filters by nothing.
    let mut q = query();
    q.name_pattern = Some(String::new());
    let compiled = CompiledQuery::compile(&q, SMALL_ARENA).unwrap();

    assert!(compiled.matches(&file("anything-at-all")));
    assert!(compiled.matches(&file("we\nird.txt")));
}

// ── Case folding ─────────────────────────────────────────────────────

#[test]
fn the_platform_default_decides_case_folding() {
    let compiled = CompiledQuery::compile(&query(), SMALL_ARENA).unwrap();
    assert_eq!(compiled.case_insensitive(), cfg!(target_os = "macos"));
}

#[test]
fn a_query_can_override_the_platform_default_both_ways() {
    let mut insensitive = query();
    insensitive.case_sensitive = Some(false);
    insensitive.name_pattern = Some("NOTES.*".to_string());
    let compiled = CompiledQuery::compile(&insensitive, SMALL_ARENA).unwrap();
    assert!(compiled.case_insensitive());
    assert!(compiled.matches(&file("notes.txt")));

    let mut sensitive = query();
    sensitive.case_sensitive = Some(true);
    sensitive.name_pattern = Some("NOTES.*".to_string());
    let compiled = CompiledQuery::compile(&sensitive, SMALL_ARENA).unwrap();
    assert!(!compiled.case_insensitive());
    assert!(!compiled.matches(&file("notes.txt")));
}

// ── NFD normalization (macOS) ────────────────────────────────────────

/// "café" composed (NFC): the form a keyboard produces.
const CAFE_NFC: &str = "caf\u{e9}.txt";
/// "café" decomposed (NFD): the form APFS stores, and the form both a walk and the
/// index read back off the filesystem.
const CAFE_NFD: &str = "cafe\u{301}.txt";

#[cfg(target_os = "macos")]
#[test]
fn a_composed_pattern_matches_the_decomposed_name_apfs_stores() {
    let mut q = query();
    q.name_pattern = Some(CAFE_NFC.to_string());
    let compiled = CompiledQuery::compile(&q, SMALL_ARENA).unwrap();

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
    let compiled = CompiledQuery::compile(&q, SMALL_ARENA).unwrap();

    assert!(!compiled.matches(&file(CAFE_NFC)));
}

// ── Type, size, and date predicates ──────────────────────────────────

#[test]
fn a_type_filter_keeps_only_its_own_kind() {
    let mut dirs_only = query();
    dirs_only.is_directory = Some(true);
    let compiled = CompiledQuery::compile(&dirs_only, SMALL_ARENA).unwrap();
    assert!(compiled.matches(&dir("Documents")));
    assert!(!compiled.matches(&file("notes.txt")));

    let mut files_only = query();
    files_only.is_directory = Some(false);
    let compiled = CompiledQuery::compile(&files_only, SMALL_ARENA).unwrap();
    assert!(!compiled.matches(&dir("Documents")));
    assert!(compiled.matches(&file("notes.txt")));
}

#[test]
fn size_bounds_are_inclusive_at_both_ends() {
    let mut q = query();
    q.min_size = Some(500);
    q.max_size = Some(1_500);
    let compiled = CompiledQuery::compile(&q, SMALL_ARENA).unwrap();

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
    assert!(
        !CompiledQuery::compile(&min_only, SMALL_ARENA)
            .unwrap()
            .matches(&sizeless)
    );

    let mut max_only = query();
    max_only.max_size = Some(1);
    assert!(
        !CompiledQuery::compile(&max_only, SMALL_ARENA)
            .unwrap()
            .matches(&sizeless)
    );
}

#[test]
fn a_directory_ignores_size_bounds_entirely() {
    // Directory sizes come from `dir_stats` AFTER ranking (`execute.rs`), so the
    // matcher must pass every directory through a size filter untouched. Dropping
    // them here would drop them before the only place that knows their size.
    let mut q = query();
    q.min_size = Some(1_000_000);
    let compiled = CompiledQuery::compile(&q, SMALL_ARENA).unwrap();

    assert!(compiled.matches(&dir("Documents")));
}

#[test]
fn date_bounds_are_inclusive_at_both_ends() {
    let mut q = query();
    q.modified_after = Some(3_000);
    q.modified_before = Some(5_000);
    let compiled = CompiledQuery::compile(&q, SMALL_ARENA).unwrap();

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
    assert!(
        !CompiledQuery::compile(&after_only, SMALL_ARENA)
            .unwrap()
            .matches(&undated)
    );

    let mut before_only = query();
    before_only.modified_before = Some(1);
    assert!(
        !CompiledQuery::compile(&before_only, SMALL_ARENA)
            .unwrap()
            .matches(&undated)
    );

    // A directory is exempt from SIZE bounds, not from date bounds.
    let mut dated = query();
    dated.modified_after = Some(1);
    let compiled = CompiledQuery::compile(&dated, SMALL_ARENA).unwrap();
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
    let compiled = CompiledQuery::compile(&q, SMALL_ARENA).unwrap();

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
    let compiled = CompiledQuery::compile(&query(), Evaluator::Arena { entries: 10 });
    assert!(compiled.is_ok());
}

#[test]
fn a_big_arena_refuses_a_query_that_narrows_nothing() {
    let at_ceiling = Evaluator::Arena {
        entries: ARENA_BROAD_QUERY_CEILING,
    };
    assert!(CompiledQuery::compile(&query(), at_ceiling).is_ok());

    let over_ceiling = Evaluator::Arena {
        entries: ARENA_BROAD_QUERY_CEILING + 1,
    };
    assert_eq!(
        CompiledQuery::compile(&query(), over_ceiling).unwrap_err(),
        CompileError::TooBroad
    );
}

#[test]
fn any_one_predicate_narrows_enough_for_a_big_arena() {
    let big = Evaluator::Arena {
        entries: ARENA_BROAD_QUERY_CEILING + 1,
    };
    for (label, narrow) in NARROWINGS {
        let mut q = query();
        narrow(&mut q);
        assert!(
            CompiledQuery::compile(&q, big).is_ok(),
            "a {label} predicate should narrow enough"
        );
    }
}

#[test]
fn an_empty_name_pattern_narrows_nothing() {
    let mut q = query();
    q.name_pattern = Some(String::new());
    let big = Evaluator::Arena {
        entries: ARENA_BROAD_QUERY_CEILING + 1,
    };
    assert_eq!(CompiledQuery::compile(&q, big).unwrap_err(), CompileError::TooBroad);
}

#[test]
fn a_live_walk_refuses_a_query_that_narrows_nothing() {
    // Unconditionally: there's no arena to weigh, and the ground a walk would read
    // is a filesystem of unknown size, over a network in the worst case.
    assert_eq!(
        CompiledQuery::compile(&query(), Evaluator::LiveWalk).unwrap_err(),
        CompileError::TooBroad
    );
}

#[test]
fn a_live_walk_serves_a_query_that_narrows_anything() {
    for (label, narrow) in NARROWINGS {
        let mut q = query();
        narrow(&mut q);
        assert!(
            CompiledQuery::compile(&q, Evaluator::LiveWalk).is_ok(),
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

// ── The two evaluators agree ─────────────────────────────────────────
//
// The milestone in one section: whatever the arena says about a row, a walk must say
// about the same file. A disagreement here is a user searching the same drive twice
// and getting two answers depending on whether it happened to be indexed.

/// The walked shape of a file: what `Index::cover` emits for it.
fn covered(path: &str, is_directory: bool, size: Option<u64>, modified_at: Option<u64>) -> CoveredEntry {
    CoveredEntry {
        path: PathBuf::from(path),
        is_directory,
        is_symlink: false,
        logical_size: size,
        physical_size: size,
        modified_at,
    }
}

/// One file, ready to be described both ways.
struct BothWays {
    path: &'static str,
    is_directory: bool,
    size: Option<u64>,
    modified_at: Option<u64>,
}

impl BothWays {
    fn file(path: &'static str, size: Option<u64>, modified_at: Option<u64>) -> Self {
        Self {
            path,
            is_directory: false,
            size,
            modified_at,
        }
    }

    fn dir(path: &'static str, modified_at: Option<u64>) -> Self {
        Self {
            path,
            is_directory: true,
            size: None,
            modified_at,
        }
    }

    /// What a row in the index carries. The name is the path's last component,
    /// which is what the scanner stored.
    fn as_arena_row(&self) -> Candidate<'_> {
        Candidate {
            name: self.path.rsplit('/').next().expect("a path has a last component"),
            is_directory: self.is_directory,
            size: self.size,
            modified_at: self.modified_at,
        }
    }

    /// What `Index::cover` emits for it.
    fn as_walked_entry(&self) -> CoveredEntry {
        covered(self.path, self.is_directory, self.size, self.modified_at)
    }
}

#[test]
fn a_walked_entry_and_its_arena_row_get_the_same_verdict() {
    // One file, described both ways, against every kind of predicate.
    let cases: [QueryEdit; 7] = [
        ("plain glob", |q| q.name_pattern = Some("repo".to_string())),
        ("wildcard glob", |q| q.name_pattern = Some("*.pdf".to_string())),
        ("regex", |q| {
            q.name_pattern = Some(r"^Q\d".to_string());
            q.pattern_type = PatternType::Regex;
        }),
        ("dirs only", |q| q.is_directory = Some(true)),
        ("files only", |q| q.is_directory = Some(false)),
        ("size range", |q| {
            q.min_size = Some(1_000);
            q.max_size = Some(3_000_000);
        }),
        ("date range", |q| {
            q.modified_after = Some(1_000);
            q.modified_before = Some(9_000);
        }),
    ];
    let files = [
        BothWays::file("/Users/alice/Q1-report.pdf", Some(2_000_000), Some(6_000)),
        BothWays::file("/Users/alice/notes.txt", Some(500), Some(5_000)),
        BothWays::dir("/Users/alice/Documents", Some(1_500)),
        BothWays::file("/Volumes/naspi/photos/holiday.jpg", None, None),
        // The decomposed form APFS stores, which is what both walks read back.
        BothWays::file("/Users/alice/cafe\u{301}.txt", Some(10), Some(10_000)),
    ];

    for (label, narrow) in cases {
        let mut q = query();
        narrow(&mut q);
        let compiled = CompiledQuery::compile(&q, SMALL_ARENA).unwrap();
        // Two evaluators that both reject everything would agree vacuously, so each
        // predicate has to keep something as well as drop something.
        let mut kept = 0;
        let mut dropped = 0;
        for f in &files {
            let verdict = compiled.matches(&f.as_arena_row());
            assert_eq!(
                verdict,
                compiled.matches_covered(&f.as_walked_entry()),
                "{label} disagreed about {}",
                f.path
            );
            if verdict { kept += 1 } else { dropped += 1 }
        }
        assert!(kept > 0 && dropped > 0, "{label} matched {kept} and dropped {dropped}");
    }
}

#[cfg(target_os = "macos")]
#[test]
fn an_accented_name_matches_the_same_way_walked_or_indexed() {
    // The case the whole module is here for: the pattern arrives composed, the
    // filesystem hands back the decomposed name to walk and scanner alike.
    let mut q = query();
    q.name_pattern = Some(CAFE_NFC.to_string());
    let compiled = CompiledQuery::compile(&q, SMALL_ARENA).unwrap();

    assert!(compiled.matches(&file(CAFE_NFD)));
    assert!(compiled.matches_covered(&covered(&format!("/Users/alice/{CAFE_NFD}"), false, Some(10), Some(10))));
}

#[test]
fn a_walked_entry_matches_on_its_own_name_not_its_path() {
    let mut q = query();
    q.name_pattern = Some("alice".to_string());
    let compiled = CompiledQuery::compile(&q, SMALL_ARENA).unwrap();

    // "alice" is a directory on the way to the file, not part of its name.
    assert!(!compiled.matches_covered(&covered("/Users/alice/notes.txt", false, Some(1), Some(1))));
}

#[test]
fn a_path_with_no_last_component_yields_the_empty_name() {
    // What the index would store for it too (`insert_visitor`'s `unwrap_or_default`).
    assert_eq!(covered_name(Path::new("/")), "");
    assert_eq!(covered_name(Path::new("")), "");
    assert_eq!(covered_name(Path::new("/Users/alice/notes.txt")), "notes.txt");
    // A trailing separator doesn't hide the name.
    assert_eq!(covered_name(Path::new("/Users/alice/")), "alice");
}

#[test]
fn a_walked_directory_ignores_size_bounds_just_as_an_arena_row_does() {
    // A walk knows a directory's own size and the arena doesn't, so this is the one
    // place the two could plausibly have been written differently. They aren't:
    // directory sizes are `dir_stats`' business, after ranking (see the module doc).
    let mut q = query();
    q.min_size = Some(1_000_000);
    let compiled = CompiledQuery::compile(&q, SMALL_ARENA).unwrap();

    assert!(compiled.matches_covered(&covered("/Users/alice/Documents", true, Some(4_096), Some(1))));
}
