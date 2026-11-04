//! The seam over fakes: no database, so every outcome a source can give is one line.

use std::cell::Cell;
use std::collections::HashMap;

use super::*;
use crate::search::index::{ARENA_ROOT_ID, arena_of};
use crate::search::ranking::ImportanceWeights;
use crate::search::types::PatternType;

/// `/Volumes/fake/docs/report.pdf` and `/Volumes/fake/docs/old` (a folder), on a
/// mount-rooted volume.
fn arena() -> SearchIndex {
    arena_of(
        &[
            (ARENA_ROOT_ID, 0, "", true, None, None),
            (2, ARENA_ROOT_ID, "docs", true, None, Some(10)),
            (3, 2, "report.pdf", false, Some(500), Some(20)),
            (4, 2, "old", true, None, Some(30)),
        ],
        1,
    )
}

const MOUNT: &str = "/Volumes/fake";

/// A source whose answers are set per test, and which counts the scope lookups.
struct Fake {
    arena: SearchIndex,
    scope: Result<HashMap<&'static str, i64>, ()>,
    range: Result<HashMap<i64, u64>, ()>,
    fill: Result<HashMap<i64, u64>, ()>,
    scope_lookups: Cell<usize>,
}

impl Fake {
    fn new() -> Self {
        Self {
            arena: arena(),
            scope: Ok(HashMap::from([("/docs", 2)])),
            range: Ok(HashMap::from([(2, 5_000), (4, 50)])),
            fill: Ok(HashMap::from([(2, 5_000), (4, 50)])),
            scope_lookups: Cell::new(0),
        }
    }
}

fn unreadable(read: SourceRead) -> SourceError {
    SourceError {
        read,
        detail: "the fake says no".to_string(),
    }
}

impl SearchRows for Fake {
    fn arena(&self) -> &SearchIndex {
        &self.arena
    }

    fn path_prefix(&self) -> &str {
        MOUNT
    }

    fn resolve_scope(&self, index_paths: &[&str]) -> Result<Vec<Option<i64>>, SourceError> {
        self.scope_lookups.set(self.scope_lookups.get() + 1);
        let known = self.scope.as_ref().map_err(|()| unreadable(SourceRead::Scope))?;
        Ok(index_paths.iter().map(|path| known.get(path).copied()).collect())
    }
}

impl DirSizeSource for Fake {
    fn in_range(&self, min: Option<u64>, max: Option<u64>) -> Result<HashMap<i64, u64>, SourceError> {
        let sizes = self.range.as_ref().map_err(|()| unreadable(SourceRead::SizeRange))?;
        Ok(sizes
            .iter()
            .filter(|(_, size)| min.is_none_or(|m| **size >= m) && max.is_none_or(|m| **size <= m))
            .map(|(id, size)| (*id, *size))
            .collect())
    }

    fn of(&self, ids: &[i64]) -> Result<Vec<Option<u64>>, SourceError> {
        let sizes = self.fill.as_ref().map_err(|()| unreadable(SourceRead::SizeFill))?;
        Ok(ids.iter().map(|id| sizes.get(id).copied()).collect())
    }
}

fn query(pattern: &str) -> SearchQuery {
    SearchQuery {
        name_pattern: Some(pattern.to_string()),
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
        case_sensitive: Some(false),
        exclude_system_dirs: Some(false),
        sort_by: None,
    }
}

fn search(fake: &Fake, query: &SearchQuery, scope: &[&str]) -> Result<CoveredHalf, SearchError> {
    let pairs: Vec<(String, String)> = scope.iter().map(|p| (p.to_string(), p.to_string())).collect();
    let weights = ImportanceWeights::empty();
    search_covered(fake, fake, query, &pairs, &SearchContext::for_test(&weights))
}

fn names(half: &CoveredHalf) -> Vec<&str> {
    half.entries.iter().map(|e| e.name.as_str()).collect()
}

// ── A read that fails fails the search ───────────────────────────────

#[test]
fn a_scope_the_source_cant_read_fails_the_search() {
    let fake = Fake {
        scope: Err(()),
        ..Fake::new()
    };
    let error = search(&fake, &query("*"), &["/Volumes/fake/docs"]).expect_err("the scope read failed");
    assert!(matches!(
        error,
        SearchError::SourceUnreadable(SourceError {
            read: SourceRead::Scope,
            ..
        })
    ));
}

#[test]
fn a_size_range_the_source_cant_read_fails_the_search() {
    let fake = Fake {
        range: Err(()),
        ..Fake::new()
    };
    let sized = SearchQuery {
        min_size: Some(1_000),
        ..query("*")
    };
    let error = search(&fake, &sized, &[]).expect_err("the range read failed");
    assert!(matches!(
        error,
        SearchError::SourceUnreadable(SourceError {
            read: SourceRead::SizeRange,
            ..
        })
    ));
}

#[test]
fn a_size_fill_the_source_cant_read_fails_the_search() {
    let fake = Fake {
        fill: Err(()),
        ..Fake::new()
    };
    let error = search(&fake, &query("docs"), &[]).expect_err("the fill read failed");
    assert!(matches!(
        error,
        SearchError::SourceUnreadable(SourceError {
            read: SourceRead::SizeFill,
            ..
        })
    ));
}

#[test]
fn an_invalid_pattern_is_a_query_error_not_a_source_one() {
    let regex = SearchQuery {
        pattern_type: PatternType::Regex,
        ..query("[unclosed")
    };
    assert!(matches!(
        search(&Fake::new(), &regex, &[]),
        Err(SearchError::Query(CompileError::InvalidPattern(_)))
    ));
}

// ── The scope pipeline ───────────────────────────────────────────────

#[test]
fn the_mount_root_is_the_whole_volume_and_asks_the_source_nothing() {
    let fake = Fake::new();
    let half = search(&fake, &query("*"), &[MOUNT, "/Volumes/fake/nope"]).expect("the search answers");
    assert_eq!(names(&half), ["old", "report.pdf", "docs"], "every row, newest first");
    assert!(half.unresolved_scopes.is_empty());
    assert_eq!(fake.scope_lookups.get(), 0);
}

#[test]
fn a_subfolder_scope_keeps_its_rows_and_a_stray_path_is_reported() {
    let half = search(&Fake::new(), &query("*.pdf"), &["/Volumes/fake/docs", "/elsewhere/x"]).expect("answers");
    assert_eq!(names(&half), ["report.pdf"]);
    assert_eq!(half.unresolved_scopes, ["/elsewhere/x"]);
}

#[test]
fn a_scope_that_names_nothing_answers_nothing_and_says_so() {
    let half = search(&Fake::new(), &query("*"), &["/Volumes/fake/missing"]).expect("answers");
    assert!(half.entries.is_empty());
    assert_eq!(half.total, 0);
    assert_eq!(half.unresolved_scopes, ["/Volumes/fake/missing"]);
}

#[test]
fn the_caller_spelling_is_what_unresolved_reports() {
    // The canonical form is looked up; the original is what the user wrote, and what
    // comes back.
    let weights = ImportanceWeights::empty();
    let pairs = vec![("~/missing".to_string(), "/Volumes/fake/missing".to_string())];
    let half = search_covered(
        &Fake::new(),
        &Fake::new(),
        &query("*"),
        &pairs,
        &SearchContext::for_test(&weights),
    )
    .expect("answers");
    assert_eq!(half.unresolved_scopes, ["~/missing"]);
}

// ── Folder sizes ─────────────────────────────────────────────────────

#[test]
fn returned_folders_carry_the_sources_sizes() {
    let half = search(&Fake::new(), &query("*"), &[]).expect("answers");
    let size_of = |name: &str| half.entries.iter().find(|e| e.name == name).and_then(|e| e.size);
    assert_eq!(size_of("docs"), Some(5_000));
    assert_eq!(size_of("old"), Some(50));
    assert_eq!(size_of("report.pdf"), Some(500), "a file keeps its own size");
}

#[test]
fn a_folder_size_filter_reads_the_range_before_ranking() {
    let sized = SearchQuery {
        min_size: Some(1_000),
        ..query("*")
    };
    let half = search(&Fake::new(), &sized, &[]).expect("answers");
    assert_eq!(
        names(&half),
        ["docs"],
        "the 50-byte folder and the 500-byte file are under the bound"
    );
    assert_eq!(half.total, 1);
}
