//! Orchestration tests: the one-volume ceiling (`resolve_target`), the ranked
//! slice `run_blocking` truncates, mount-path prefixing, the directory size
//! post-filter — and, at the bottom, a live search driven end to end over a real
//! index the walk builds as it goes.

use std::collections::HashMap;

use super::coverage::coverage_kind;
use super::*;
use crate::search::engine;
use crate::search::index::{OptU64, SearchEntry, SearchIndex};
use crate::search::live::CoverageKind;
use crate::search::ranking::ImportanceWeights;
use crate::search::types::{PatternType, SearchResultEntry};
use cmdr_index::ROOT_VOLUME_ID;

mod live_drive;
mod live_e2e;
mod stalled_walk;

// ── Synthetic index builder ──────────────────────────────────────────

fn arena_push(names: &mut String, name: &str) -> (u32, u16) {
    let offset = names.len() as u32;
    let len = name.len() as u16;
    names.push_str(name);
    (offset, len)
}

/// Build a tiny index holding one directory per `(dir, file, modified_at)` triple,
/// each with a single file inside it: `/dir/<file>`. Ids run root sentinel 1, then
/// dir/file pairs.
fn index_of(files: &[(&str, &str, u64)]) -> SearchIndex {
    let mut names = String::new();
    let (r_off, r_len) = arena_push(&mut names, "");
    let mut entries = vec![SearchEntry {
        id: 1,
        parent_id: 0,
        name_offset: r_off,
        name_len: r_len,
        is_directory: true,
        size: OptU64::NONE,
        modified_at: OptU64::NONE,
    }];
    for (dir, file, modified_at) in files {
        let (d_off, d_len) = arena_push(&mut names, dir);
        let (f_off, f_len) = arena_push(&mut names, file);
        let dir_id = entries.len() as i64 + 1;
        entries.push(SearchEntry {
            id: dir_id,
            parent_id: 1,
            name_offset: d_off,
            name_len: d_len,
            is_directory: true,
            size: OptU64::NONE,
            modified_at: OptU64::new(Some(1)),
        });
        entries.push(SearchEntry {
            id: dir_id + 1,
            parent_id: dir_id,
            name_offset: f_off,
            name_len: f_len,
            is_directory: false,
            size: OptU64::new(Some(10)),
            modified_at: OptU64::new(Some(*modified_at)),
        });
    }
    let mut id_to_index = HashMap::new();
    for (i, e) in entries.iter().enumerate() {
        id_to_index.insert(e.id, i);
    }
    SearchIndex {
        names,
        entries,
        id_to_index,
        generation: 1,
    }
}

/// The common single-file case: `/dir/<file>`.
fn one_file_index(dir: &str, file: &str, modified_at: u64) -> SearchIndex {
    index_of(&[(dir, file, modified_at)])
}

/// A plain substring query for `stem` (auto-wrapped `*stem*`), the case with a
/// match-quality gradient (exact vs prefix vs substring).
fn plain_query(stem: &str) -> SearchQuery {
    SearchQuery {
        name_pattern: Some(stem.to_string()),
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

fn ranked(index: &SearchIndex, query: &SearchQuery, prefix: &str) -> Vec<SearchResultEntry> {
    engine::search_ranked(index, query, &ImportanceWeights::empty(), prefix, None)
        .expect("search_ranked")
        .entries
}

// ── The ranked slice `run_blocking` hands back ───────────────────────

#[test]
fn the_engine_slice_is_already_best_first() {
    // `run_blocking` truncates the engine's slice and returns it as-is, with no sort
    // of its own, so the engine's best-first contract is what orders the results.
    // Here: a mid-string SUBSTRING match that's very new against an ancient EXACT
    // match. Match quality dominates, so the exact one leads.
    let vol = index_of(&[("a", "Q1-report.pdf", 9_999_999), ("b", "report", 1)]);
    let out = ranked(&vol, &plain_query("report"), "");

    let names: Vec<&str> = out.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(
        names,
        vec!["report", "Q1-report.pdf"],
        "the exact match ranks first despite the other's newer mtime"
    );
}

// ── Mount-path prefixing ─────────────────────────────────────────────

#[test]
fn non_root_paths_are_prefixed_with_the_mount_root() {
    // A non-root volume's index is mount-relative; the prefix restores the absolute
    // mount path so a NAS result opens in a pane.
    let vol = one_file_index("sub", "report.pdf", 100);
    let query = plain_query("report");

    let out = ranked(&vol, &query, "/Volumes/nas");
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].path, "/Volumes/nas/sub/report.pdf");
    assert_eq!(out[0].parent_path, "/Volumes/nas/sub");

    // Root (empty prefix) leaves the reconstructed absolute path untouched.
    let out_root = ranked(&vol, &query, "");
    assert_eq!(out_root[0].path, "/sub/report.pdf");
}

// ── The one-volume ceiling (`resolve_target`) ────────────────────────

#[test]
fn a_scope_spanning_two_volumes_cannot_be_expressed() {
    // The ceiling holds at the API, not just in the UI: a scope naming a local folder
    // AND a folder on a phone has no single volume to search, so it's refused outright
    // rather than quietly answering for one of them.
    let query = SearchQuery {
        include_paths: Some(vec!["/Users/me/a".to_string(), "mtp://device-1/65537/DCIM".to_string()]),
        ..plain_query("report")
    };
    let err = resolve_target(&query).expect_err("a two-volume scope must be refused");
    let ScopeError::SpansMultipleVolumes { volume_ids } = err;
    assert_eq!(
        volume_ids,
        vec![ROOT_VOLUME_ID.to_string(), "device-1:65537".to_string()]
    );
}

#[test]
fn scoped_local_paths_collapse_into_one_root_target() {
    // Two local include paths both belong to root, so they're one `root` target
    // carrying both, marked `from_scope`.
    let query = SearchQuery {
        include_paths: Some(vec!["/Users/me/a".to_string(), "/Users/me/b".to_string()]),
        ..plain_query("report")
    };
    let target = resolve_target(&query).expect("both local paths route to the one root volume");
    assert_eq!(target.volume_id, ROOT_VOLUME_ID);
    assert_eq!(target.include_paths.len(), 2);
    assert!(target.from_scope);
}

#[test]
fn an_unscoped_query_targets_the_boot_volume() {
    // With one volume as the ceiling, "no scope" means the boot volume rather than
    // every indexed volume, and it's never `from_scope` (nobody asked for it, so an
    // unindexed boot volume isn't a coverage gap to report).
    let target = resolve_target(&plain_query("report")).expect("an unscoped query always has a target");
    assert_eq!(target.volume_id, ROOT_VOLUME_ID);
    assert!(target.include_paths.is_empty());
    assert!(!target.from_scope);
}

// ── Count-only ───────────────────────────────────────────────────────

#[test]
fn count_only_returns_an_exact_total_and_no_rows() {
    // Count-only runs the same engine pass but returns just the total: no rows are
    // materialized, and the total is exact by construction — a directory size
    // filter was applied inside the scan (`DirSizes`), not subtracted afterwards.
    // Mirrors the count-only branch of `run_blocking`.
    let vol = one_file_index("a", "report.pdf", 100);
    let mut query = plain_query("report");
    query.count_only = true;

    let engine::Ranked {
        entries: ranked,
        total_count: vtotal,
        ..
    } = engine::search_ranked(&vol, &query, &ImportanceWeights::empty(), "", None).expect("search_ranked");
    assert!(ranked.is_empty(), "count-only returns no rows");
    assert_eq!(vtotal, 1, "and the total needs no correction pass");
}

// ── Which ground the answer came from ────────────────────────────────

/// Nothing to walk means the index answered the whole question.
#[test]
fn a_run_with_no_frontier_is_covered() {
    assert_eq!(coverage_kind(&[], &["/a".to_string()]), CoverageKind::Covered);
}

/// A scope root that is ITSELF a frontier root was covered by nothing, so the
/// whole answer came off the walk. The cold-drive case.
#[test]
fn a_run_whose_every_scope_is_frontier_is_live() {
    let scopes = vec!["/a".to_string(), "/b".to_string()];
    let frontier = vec!["/a".to_string(), "/b".to_string()];
    assert_eq!(coverage_kind(&frontier, &scopes), CoverageKind::Live);
}

/// Ground below a scope the index HAS listed is the mixed case, and so is one
/// scope of two being covered. Both are "part of this came from the index".
#[test]
fn a_run_walking_below_a_listed_scope_is_mixed() {
    let scopes = vec!["/a".to_string()];
    assert_eq!(
        coverage_kind(&["/a/new".to_string()], &scopes),
        CoverageKind::Mixed,
        "the scope was listed; something under it wasn't"
    );
    assert_eq!(
        coverage_kind(&["/b".to_string()], &["/a".to_string(), "/b".to_string()]),
        CoverageKind::Mixed,
        "one scope covered, one not"
    );
}
