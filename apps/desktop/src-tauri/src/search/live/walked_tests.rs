//! The walk's half of "one matcher, two evaluators": whatever the arena says about a
//! row, a walk must say about the same file. A disagreement here is a user searching
//! the same drive twice and getting two answers depending on whether it happened to
//! be indexed.

use std::path::{Path, PathBuf};

use cmdr_index::CoveredEntry;

use super::{covered_name, matches_walked};
use crate::search::NameRules;
use crate::search::matcher::{Candidate, CompiledQuery, Evaluator};
use crate::search::types::{PatternType, SearchQuery};

/// `café` composed (one code point for the `é`), as a person types it.
const CAFE_NFC: &str = "caf\u{e9}.txt";
/// `café` decomposed (`e` plus a combining acute), as APFS stores it.
const CAFE_NFD: &str = "cafe\u{301}.txt";

/// An evaluator that never refuses a broad query.
const SMALL_ARENA: Evaluator = Evaluator::Arena { entries: 10 };

/// One edit to a query, labelled for the failure message it would produce.
type QueryEdit = (&'static str, fn(&mut SearchQuery));

fn compile(query: &SearchQuery) -> CompiledQuery {
    CompiledQuery::compile(query, SMALL_ARENA, &NameRules::host()).expect("the query compiles")
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

fn file(name: &str) -> Candidate<'_> {
    Candidate {
        name,
        is_directory: false,
        size: Some(1_000),
        modified_at: Some(1_000),
    }
}

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
        BothWays::file("/Users/test_user_4/Q1-report.pdf", Some(2_000_000), Some(6_000)),
        BothWays::file("/Users/test_user_4/notes.txt", Some(500), Some(5_000)),
        BothWays::dir("/Users/test_user_4/Documents", Some(1_500)),
        BothWays::file("/Volumes/testshare/photos/holiday.jpg", None, None),
        // The decomposed form APFS stores, which is what both walks read back.
        BothWays::file("/Users/test_user_4/cafe\u{301}.txt", Some(10), Some(10_000)),
    ];

    for (label, narrow) in cases {
        let mut q = query();
        narrow(&mut q);
        let compiled = compile(&q);
        // Two evaluators that both reject everything would agree vacuously, so each
        // predicate has to keep something as well as drop something.
        let mut kept = 0;
        let mut dropped = 0;
        for f in &files {
            let verdict = compiled.matches(&f.as_arena_row());
            assert_eq!(
                verdict,
                matches_walked(&compiled, &f.as_walked_entry()),
                "{label} disagreed about {}",
                f.path
            );
            if verdict { kept += 1 } else { dropped += 1 }
        }
        assert!(kept > 0 && dropped > 0, "{label} matched {kept} and dropped {dropped}");
    }
}

#[test]
fn an_accented_name_matches_the_same_way_walked_or_indexed() {
    // The case the whole module is here for: the pattern arrives composed, the
    // filesystem hands back the decomposed name (or the composed one, #955) to walk
    // and scanner alike. Under the Mac's rules, on any machine.
    let mut q = query();
    q.name_pattern = Some(CAFE_NFC.to_string());
    let compiled = CompiledQuery::compile(&q, SMALL_ARENA, &NameRules::MACOS).expect("the query compiles");

    for name in [CAFE_NFD, CAFE_NFC] {
        assert!(compiled.matches(&file(name)), "{name:?}");
        assert!(
            matches_walked(
                &compiled,
                &covered(&format!("/Users/test_user_4/{name}"), false, Some(10), Some(10))
            ),
            "{name:?}"
        );
    }
}

#[test]
fn a_walked_entry_matches_on_its_own_name_not_its_path() {
    let mut q = query();
    q.name_pattern = Some("test_user_4".to_string());
    let compiled = compile(&q);

    // "test_user_4" is a directory on the way to the file, not part of its name.
    assert!(!matches_walked(
        &compiled,
        &covered("/Users/test_user_4/notes.txt", false, Some(1), Some(1))
    ));
}

#[test]
fn a_path_with_no_last_component_yields_the_empty_name() {
    // What the index would store for it too (`insert_visitor`'s `unwrap_or_default`).
    assert_eq!(covered_name(Path::new("/")), "");
    assert_eq!(covered_name(Path::new("")), "");
    assert_eq!(covered_name(Path::new("/Users/test_user_4/notes.txt")), "notes.txt");
    // A trailing separator doesn't hide the name.
    assert_eq!(covered_name(Path::new("/Users/test_user_4/")), "test_user_4");
}

#[test]
fn a_walked_directory_ignores_size_bounds_just_as_an_arena_row_does() {
    // A walk knows a directory's own size and the arena doesn't, so this is the one
    // place the two could plausibly have been written differently. They aren't:
    // directory sizes are `dir_stats`' business, after ranking (see the module doc).
    let mut q = query();
    q.min_size = Some(1_000_000);
    let compiled = compile(&q);

    assert!(matches_walked(
        &compiled,
        &covered("/Users/test_user_4/Documents", true, Some(4_096), Some(1))
    ));
}
