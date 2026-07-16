//! Orchestration tests that don't need the live registry: the cross-volume merge
//! (each volume's ranked slice combines by the shared key), mount-path prefixing,
//! the directory size post-filter, and scope→volume target grouping.

use std::collections::HashMap;

use super::*;
use crate::indexing::ROOT_VOLUME_ID;
use crate::search::engine::{self, RankedEntry};
use crate::search::index::{SearchEntry, SearchIndex};
use crate::search::ranking::ImportanceWeights;
use crate::search::types::PatternType;

// ── Synthetic index builder ──────────────────────────────────────────

fn arena_push(names: &mut String, name: &str) -> (u32, u16) {
    let offset = names.len() as u32;
    let len = name.len() as u16;
    names.push_str(name);
    (offset, len)
}

/// Build a tiny `/dir/<file>` index (root sentinel id 1, dir id 2, file id 3).
fn one_file_index(dir: &str, file: &str, modified_at: u64) -> SearchIndex {
    let mut names = String::new();
    let (r_off, r_len) = arena_push(&mut names, "");
    let (d_off, d_len) = arena_push(&mut names, dir);
    let (f_off, f_len) = arena_push(&mut names, file);
    let entries = vec![
        SearchEntry {
            id: 1,
            parent_id: 0,
            name_offset: r_off,
            name_len: r_len,
            is_directory: true,
            size: None,
            modified_at: None,
        },
        SearchEntry {
            id: 2,
            parent_id: 1,
            name_offset: d_off,
            name_len: d_len,
            is_directory: true,
            size: None,
            modified_at: Some(1),
        },
        SearchEntry {
            id: 3,
            parent_id: 2,
            name_offset: f_off,
            name_len: f_len,
            is_directory: false,
            size: Some(10),
            modified_at: Some(modified_at),
        },
    ];
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
    }
}

fn ranked(index: &SearchIndex, query: &SearchQuery, prefix: &str) -> Vec<RankedEntry> {
    engine::search_ranked(index, query, &ImportanceWeights::empty(), prefix)
        .expect("search_ranked")
        .0
}

// ── Cross-volume merge ───────────────────────────────────────────────

#[test]
fn merge_ranks_by_band_across_volumes() {
    // Volume A holds a mid-string SUBSTRING match (very new); volume B holds an
    // EXACT match (ancient). The exact match must win the merged order no matter
    // which volume it came from — match-quality dominates, and the keys compare
    // across volumes.
    let vol_a = one_file_index("a", "Q1-report.pdf", 9_999_999);
    let vol_b = one_file_index("b", "report", 1);
    let query = plain_query("report");

    let mut merged: Vec<RankedEntry> = Vec::new();
    merged.extend(ranked(&vol_a, &query, ""));
    merged.extend(ranked(&vol_b, &query, ""));
    merged.sort_by(|x, y| x.key.cmp_best_first(&y.key));

    let names: Vec<&str> = merged.iter().map(|r| r.entry.name.as_str()).collect();
    assert_eq!(
        names,
        vec!["report", "Q1-report.pdf"],
        "the exact match ranks first across volumes despite the other's newer mtime"
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
    assert_eq!(out[0].entry.path, "/Volumes/nas/sub/report.pdf");
    assert_eq!(out[0].entry.parent_path, "/Volumes/nas/sub");

    // Root (empty prefix) leaves the reconstructed absolute path untouched.
    let out_root = ranked(&vol, &query, "");
    assert_eq!(out_root[0].entry.path, "/sub/report.pdf");
}

// ── Directory size post-filter ───────────────────────────────────────

#[test]
fn filter_ranked_dirs_by_size_trims_dirs_keeps_files() {
    let vol = one_file_index("sub", "report.pdf", 100);
    let query = plain_query("report");
    let mut out = ranked(&vol, &query, "");
    // Force a directory entry alongside the file to exercise the dir filter.
    out.push(RankedEntry {
        key: out[0].key,
        entry: crate::search::SearchResultEntry {
            name: "bigdir".to_string(),
            path: "/bigdir".to_string(),
            parent_path: "/".to_string(),
            is_directory: true,
            size: Some(50),
            modified_at: Some(1),
            icon_id: "dir".to_string(),
            entry_id: 99,
        },
    });

    // min_size 100: the file (already engine-filtered) passes; the 50-byte dir drops.
    let mut q = query.clone();
    q.min_size = Some(100);
    let before = out.len() as u32;
    let total = filter_ranked_dirs_by_size(&mut out, &q, before);
    let names: Vec<&str> = out.iter().map(|r| r.entry.name.as_str()).collect();
    assert_eq!(names, vec!["report.pdf"]);
    assert_eq!(total, 1, "total reflects the retained length under a size filter");
}

#[test]
fn filter_ranked_dirs_by_size_no_filter_is_noop() {
    let vol = one_file_index("sub", "report.pdf", 100);
    let query = plain_query("report");
    let mut out = ranked(&vol, &query, "");
    let before = out.len() as u32;
    let total = filter_ranked_dirs_by_size(&mut out, &query, before);
    assert_eq!(total, before, "no size filter ⇒ total unchanged");
    assert_eq!(out.len() as u32, before);
}

// ── Scope → volume target grouping ───────────────────────────────────

#[test]
fn scoped_local_paths_group_into_one_root_target() {
    // Two local include paths both belong to root, so they collapse to a single
    // `root` target carrying both, marked `from_scope`.
    let query = SearchQuery {
        include_paths: Some(vec!["/Users/me/a".to_string(), "/Users/me/b".to_string()]),
        ..plain_query("report")
    };
    let targets = resolve_targets(&query);
    assert_eq!(targets.len(), 1, "both local paths route to the one root volume");
    assert_eq!(targets[0].volume_id, ROOT_VOLUME_ID);
    assert_eq!(targets[0].include_paths.len(), 2);
    assert!(targets[0].from_scope);
}

#[test]
fn unscoped_targets_are_not_from_scope() {
    let query = plain_query("report");
    let targets = resolve_targets(&query);
    assert!(targets.iter().any(|t| t.volume_id == ROOT_VOLUME_ID));
    assert!(
        targets.iter().all(|t| !t.from_scope),
        "unscoped targets never count as coverage gaps"
    );
}

// ── Count-only across volumes ────────────────────────────────────────

#[test]
fn count_only_sums_per_volume_totals_with_no_rows() {
    // Count-only fans out the same as a normal search but returns just the total:
    // each volume's engine pass yields no rows (no size filter on dirs) and its match
    // count, and the orchestrator sums them with no k-way merge. Mirrors the count-only
    // branch of `run_blocking` for two volumes.
    let vol_a = one_file_index("a", "report.pdf", 100);
    let vol_b = one_file_index("b", "report.txt", 200);
    let mut query = plain_query("report");
    query.count_only = true;

    let mut total: u64 = 0;
    let mut rows = 0usize;
    for (vol, prefix) in [(&vol_a, ""), (&vol_b, "/Volumes/nas")] {
        let (ranked, vtotal) =
            engine::search_ranked(vol, &query, &ImportanceWeights::empty(), prefix).expect("search_ranked");
        // No size filter ⇒ the engine materializes no rows and the total is exact.
        assert!(
            ranked.is_empty(),
            "count-only returns no rows without a directory size filter"
        );
        rows += ranked.len();
        total += count_only_volume_total(vtotal, &ranked, &query) as u64;
    }
    // Each volume matches its one `report*` file ⇒ summed total 2, nothing to merge.
    assert_eq!(total, 2);
    assert_eq!(rows, 0);
}

#[test]
fn count_only_volume_total_subtracts_out_of_range_dirs() {
    // Grab a valid RankKey from a real ranked pass, then hand-build two matching
    // directories with filled sizes. The engine's volume total counts every match
    // (say 3 files that passed the size filter + these 2 dirs = 5); after the size
    // check, the 100-byte dir falls under the floor and drops, so the exact total is 4.
    let vol = one_file_index("sub", "report.pdf", 100);
    let key = ranked(&vol, &plain_query("report"), "")[0].key;
    let dirs = vec![
        RankedEntry {
            key,
            entry: crate::search::SearchResultEntry {
                name: "big".to_string(),
                path: "/big".to_string(),
                parent_path: "/".to_string(),
                is_directory: true,
                size: Some(10_000),
                modified_at: Some(1),
                icon_id: "dir".to_string(),
                entry_id: 98,
            },
        },
        RankedEntry {
            key,
            entry: crate::search::SearchResultEntry {
                name: "small".to_string(),
                path: "/small".to_string(),
                parent_path: "/".to_string(),
                is_directory: true,
                size: Some(100),
                modified_at: Some(1),
                icon_id: "dir".to_string(),
                entry_id: 99,
            },
        },
    ];
    let mut q = plain_query("report");
    q.count_only = true;
    q.min_size = Some(1_000);
    assert_eq!(count_only_volume_total(5, &dirs, &q), 4);
    // No size filter ⇒ the volume total is already exact, nothing subtracted.
    assert_eq!(count_only_volume_total(5, &dirs, &plain_query("report")), 5);
}
