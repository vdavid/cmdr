use std::collections::HashMap;

use super::*;
use crate::ranking::ImportanceWeights;
use crate::seam::search_covered;
use crate::testing::FolderSizes;
use crate::types::{PatternType, SearchQuery};
use crate::{SearchContext, SearchError};

fn dir(path: &str) -> MemoryRow {
    MemoryRow {
        path: path.to_string(),
        is_directory: true,
        size: None,
        modified_at: Some(1_700_000_000),
    }
}

fn file(path: &str, size: u64) -> MemoryRow {
    MemoryRow {
        path: path.to_string(),
        is_directory: false,
        size: Some(size),
        modified_at: Some(1_700_000_000),
    }
}

/// Every row's `(id, parent_id, name)`, in arena order.
fn rows_of(source: &MemorySource) -> Vec<(i64, i64, String)> {
    let arena = source.arena();
    (0..arena.len())
        .map(|i| (arena.id(i), arena.parent_id(i), arena.name(i).to_string()))
        .collect()
}

#[test]
fn ids_go_root_then_depth_first_with_children_in_byte_order() {
    // Listed out of order on purpose: the order the rows arrive in decides nothing.
    let rows = vec![
        file("/b.txt", 1),
        dir("/a/Z"),
        file("/a/Z/inner.txt", 2),
        dir("/a"),
        file("/a/y.txt", 3),
        file("/B.txt", 4),
    ];
    let source = MemorySource::new(rows, "", NameRules::LINUX).expect("a valid tree");
    assert_eq!(
        rows_of(&source),
        vec![
            (1, 0, String::new()),
            (2, 1, "B.txt".into()),
            (3, 1, "a".into()),
            (4, 3, "Z".into()),
            (5, 4, "inner.txt".into()),
            (6, 3, "y.txt".into()),
            (7, 1, "b.txt".into()),
        ]
    );
}

#[test]
fn rows_that_cant_form_a_tree_are_refused_by_name() {
    let refused = |rows: Vec<MemoryRow>| MemorySource::new(rows, "", NameRules::LINUX).map(|_| ());
    assert_eq!(
        refused(vec![file("relative.txt", 1)]),
        Err(MemorySourceError::MalformedPath("relative.txt".into()))
    );
    assert_eq!(
        refused(vec![dir("/")]),
        Err(MemorySourceError::MalformedPath("/".into()))
    );
    assert_eq!(
        refused(vec![dir("/a//b")]),
        Err(MemorySourceError::MalformedPath("/a//b".into()))
    );
    assert_eq!(
        refused(vec![file("/a", 1), file("/a", 2)]),
        Err(MemorySourceError::DuplicatePath("/a".into()))
    );
    assert_eq!(
        refused(vec![file("/nowhere/a", 1)]),
        Err(MemorySourceError::MissingParent("/nowhere/a".into()))
    );
    assert_eq!(
        refused(vec![file("/f", 1), file("/f/a", 1)]),
        Err(MemorySourceError::ParentIsAFile("/f/a".into()))
    );
}

#[test]
fn a_scope_lookup_folds_through_the_name_rules() {
    let rows = || {
        vec![
            dir("/Photos"),
            dir("/Photos/Caf\u{e9}"),
            file("/Photos/Caf\u{e9}/a.jpg", 1),
        ]
    };
    let mac = MemorySource::new(rows(), "", NameRules::MACOS).expect("a valid tree");
    let linux = MemorySource::new(rows(), "", NameRules::LINUX).expect("a valid tree");

    assert_eq!(mac.id_of("/"), Some(ARENA_ROOT_ID));
    assert_eq!(mac.id_of("/Photos/Caf\u{e9}/"), Some(3));
    // A Mac's drive is case- and form-insensitive: another spelling finds the folder.
    assert_eq!(mac.id_of("/photos/cafe\u{301}"), Some(3));
    // Linux keeps names as they are.
    assert_eq!(linux.id_of("/Photos/Caf\u{e9}"), Some(3));
    assert_eq!(linux.id_of("/photos/caf\u{e9}"), None);
    assert_eq!(linux.id_of("/Photos/cafe\u{301}"), None);
    assert_eq!(linux.id_of("/Photos/Caf\u{e9}/a.jpg/deeper"), None);
}

#[test]
fn a_deep_tree_builds_without_recursing() {
    let mut rows = Vec::new();
    let mut path = String::new();
    for _ in 0..2_000 {
        path.push_str("/d");
        rows.push(dir(&path));
    }
    let source = MemorySource::new(rows, "", NameRules::LINUX).expect("a valid tree");
    assert_eq!(source.arena().len(), 2_001);
    assert_eq!(source.id_of(&path), Some(2_001));
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
        case_sensitive: None,
        exclude_system_dirs: Some(false),
        sort_by: None,
    }
}

/// The core answers over memory exactly as over the index: scope, folder sizes, and
/// the mount prefix on every row.
#[test]
fn a_search_over_memory_scopes_sizes_and_prefixes_its_rows() {
    let rows = vec![
        dir("/docs"),
        file("/docs/report.pdf", 10),
        dir("/docs/old"),
        file("/docs/old/report-1999.pdf", 20),
        file("/report.txt", 30),
    ];
    let source = MemorySource::new(rows, "/Volumes/share", NameRules::LINUX).expect("a valid tree");
    let sizes = FolderSizes(HashMap::from([
        (source.id_of("/docs").expect("docs"), 30),
        (source.id_of("/docs/old").expect("old"), 20),
    ]));
    let weights = ImportanceWeights::empty();
    let ctx = SearchContext {
        rules: NameRules::LINUX,
        home_dir: None,
        weights: &weights,
    };
    let scope = |path: &str| vec![(path.to_string(), path.to_string())];

    let half =
        search_covered(&source, &sizes, &query("report"), &scope("/Volumes/share/docs"), &ctx).expect("a search");
    let mut paths: Vec<&str> = half.entries.iter().map(|e| e.path.as_str()).collect();
    paths.sort_unstable();
    assert_eq!(
        paths,
        [
            "/Volumes/share/docs/old/report-1999.pdf",
            "/Volumes/share/docs/report.pdf"
        ]
    );
    assert!(half.unresolved_scopes.is_empty());

    let folders = SearchQuery {
        name_pattern: None,
        is_directory: Some(true),
        min_size: Some(25),
        ..query("*")
    };
    let half = search_covered(&source, &sizes, &folders, &[], &ctx).expect("a search");
    let found: Vec<(&str, Option<u64>)> = half.entries.iter().map(|e| (e.path.as_str(), e.size)).collect();
    assert_eq!(found, [("/Volumes/share/docs", Some(30))]);

    let half =
        search_covered(&source, &sizes, &query("report"), &scope("/Volumes/share/nope"), &ctx).expect("a search");
    assert!(half.entries.is_empty());
    assert_eq!(half.unresolved_scopes, ["/Volumes/share/nope"]);

    let invalid = SearchQuery {
        pattern_type: PatternType::Regex,
        ..query("[unclosed")
    };
    assert!(matches!(
        search_covered(&source, &sizes, &invalid, &[], &ctx),
        Err(SearchError::Query(_))
    ));
}
