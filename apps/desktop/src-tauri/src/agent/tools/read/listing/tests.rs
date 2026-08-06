//! `list_dir` unit tests: the coverage flags, the size contract, and the ordering
//! and paging rules. All pure over resolved inputs — no index, no filesystem.

use super::*;

fn dir_stats(size: u64, complete: bool, stale: bool, pending: bool) -> DirStats {
    DirStats {
        path: "/x".to_string(),
        recursive_size: size,
        recursive_physical_size: size,
        recursive_file_count: 3,
        recursive_dir_count: 1,
        recursive_has_symlinks: false,
        recursive_size_pending: pending,
        recursive_size_complete: complete,
        recursive_size_stale: stale,
    }
}

#[test]
fn fresh_index_is_authoritative_with_no_note() {
    let cov = coverage(true, Some(Freshness::Fresh), true);
    assert_eq!(cov.index_status, "fresh");
    assert!(cov.authoritative);
    assert_eq!(cov.note, None);
}

#[test]
fn stale_index_reads_stale_and_says_so() {
    let cov = coverage(true, Some(Freshness::Stale), true);
    assert_eq!(cov.index_status, "stale");
    assert!(!cov.authoritative);
    assert!(cov.note.is_some());
}

fn child(index: usize) -> ChildEntry {
    ChildEntry {
        name: format!("some-reasonably-long-file-name-{index}.jpeg"),
        is_directory: false,
        is_symlink: false,
        size: Some(1_234_567),
        size_is_lower_bound: false,
        modified: Some(1_700_000_000),
    }
}

/// A named row with a size, for the ordering tests.
fn row(name: &str, is_directory: bool, size: Option<u64>) -> ChildEntry {
    ChildEntry {
        name: name.to_string(),
        is_directory,
        is_symlink: false,
        size,
        size_is_lower_bound: false,
        modified: size,
    }
}

/// A volume block with no space known: the default for tests that aren't about space.
fn no_space() -> VolumeBlock {
    VolumeBlock {
        id: "root".to_string(),
        total_bytes: None,
        available_bytes: None,
    }
}

fn page_of(children: Vec<ChildEntry>) -> Page {
    let total = children.len();
    Page { rows: children, total }
}

#[test]
fn a_huge_folder_listing_is_paged_not_shipped_whole() {
    use crate::agent::chat::budget::{MAX_TOOL_RESULT_TOKENS, estimate_serialized_tokens};

    // A Downloads folder with 20k entries would serialize to hundreds of thousands of
    // tokens. The answer must carry what fits plus honest counts, so the model can say
    // what it saw and ask for more. `limit` is the caller's cut; the budget is the
    // backstop under it.
    let opts = ListOptions {
        limit: MAX_LIMIT,
        ..Default::default()
    };
    let children: Vec<ChildEntry> = (0..20_000).map(child).collect();
    let page = sort_and_page(children, &opts);
    let result = build_list_dir(
        "/downloads",
        Some(page),
        None,
        true,
        Some(Freshness::Fresh),
        no_space(),
        &opts,
    );

    let rows = result.children.as_ref().expect("an indexed listing");
    assert_eq!(result.total, Some(20_000), "the honest denominator survives");
    assert_eq!(result.returned, Some(rows.len()));
    assert!(result.truncated, "the cut must be visible to the model");
    assert!(rows.len() < 20_000);
    let spent: usize = rows.iter().map(estimate_serialized_tokens).sum();
    assert!(
        spent <= MAX_TOOL_RESULT_TOKENS,
        "the listing must fit the tool-result ceiling (spent {spent})"
    );
}

#[test]
fn a_normal_folder_listing_is_returned_whole() {
    let opts = ListOptions::default();
    let page = sort_and_page((0..30).map(child).collect(), &opts);
    let result = build_list_dir(
        "/photos",
        Some(page),
        None,
        true,
        Some(Freshness::Fresh),
        no_space(),
        &opts,
    );
    assert_eq!(result.total, Some(30));
    assert_eq!(result.returned, Some(30));
    assert!(!result.truncated);
}

#[test]
fn unindexed_volume_returns_typed_no_index_not_a_wrong_zero() {
    // children None + not enabled ⇒ "off" + a "not indexed" note, never an
    // empty-but-authoritative listing.
    let result = build_list_dir(
        "/nas/share",
        None,
        None,
        false,
        None,
        no_space(),
        &ListOptions::default(),
    );
    assert_eq!(result.coverage.index_status, "off");
    assert!(!result.coverage.authoritative);
    assert!(result.coverage.note.as_deref().unwrap().contains("isn't indexed"));
    assert!(result.children.is_none());
    assert!(result.size.is_none());
    assert!(!result.truncated, "nothing was cut: there was nothing to cut");
}

#[test]
fn indexed_but_missing_path_is_a_distinct_not_in_index_note() {
    let result = build_list_dir(
        "/Users/x/new",
        None,
        None,
        true,
        Some(Freshness::Fresh),
        no_space(),
        &ListOptions::default(),
    );
    assert_eq!(result.coverage.index_status, "fresh");
    assert!(
        result
            .coverage
            .note
            .as_deref()
            .unwrap()
            .contains("isn't in the drive index")
    );
}

#[test]
fn list_dir_surfaces_lower_bound_and_updating_flags() {
    let stats = dir_stats(1_000, false, false, true);
    let result = build_list_dir(
        "/Users/x",
        Some(page_of(vec![row("sub", true, None)])),
        Some(&stats),
        true,
        Some(Freshness::Fresh),
        no_space(),
        &ListOptions::default(),
    );
    let size = result.size.unwrap();
    assert!(size.size_is_lower_bound);
    assert!(size.size_is_updating);
    assert_eq!(size.recursive_size, 1_000);
}

#[test]
fn a_listing_names_its_volume_and_how_full_it_is() {
    // A folder size is only actionable next to the drive's free space, so the
    // volume block rides along on every listing rather than costing a second call.
    let result = build_list_dir(
        "/Users/x",
        Some(page_of(vec![])),
        None,
        true,
        Some(Freshness::Fresh),
        VolumeBlock {
            id: "root".to_string(),
            total_bytes: Some(2_000_000_000_000),
            available_bytes: Some(214_300_000_000),
        },
        &ListOptions::default(),
    );
    assert_eq!(result.volume.id, "root");
    assert_eq!(result.volume.available_bytes, Some(214_300_000_000));
}

// ── Ordering and paging ───────────────────────────────────────────────────

#[test]
fn size_order_ranks_files_and_folders_together() {
    // The disk-usage case: one enormous file outweighs every folder around it,
    // and a folders-only ranking would hide exactly the row that answers the
    // question.
    let opts = ListOptions {
        sort_by: SortBy::Size,
        order: Order::Desc,
        ..Default::default()
    };
    let page = sort_and_page(
        vec![
            row("small-folder", true, Some(10)),
            row("huge-disk-image.raw", false, Some(900)),
            row("big-folder", true, Some(400)),
        ],
        &opts,
    );
    let names: Vec<&str> = page.rows.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, ["huge-disk-image.raw", "big-folder", "small-folder"]);
}

#[test]
fn an_unknown_size_never_leads_a_ranking_in_either_direction() {
    // A folder with no dir_stats row is unknown, not empty. Ranking it first in
    // either direction would read as "this is the biggest" or "this is the
    // smallest", and both are claims the index can't make.
    let rows = vec![
        row("unknown", true, None),
        row("known-small", true, Some(1)),
        row("known-big", true, Some(100)),
    ];
    for order in [Order::Desc, Order::Asc] {
        let opts = ListOptions {
            sort_by: SortBy::Size,
            order,
            ..Default::default()
        };
        let page = sort_and_page(rows.clone(), &opts);
        assert_eq!(
            page.rows.last().map(|r| r.name.as_str()),
            Some("unknown"),
            "unknown must sort last with order {order:?}"
        );
    }
}

#[test]
fn offset_paging_covers_every_row_exactly_once() {
    // Stable order plus offset is what makes "resume with offset + returned"
    // safe: a repeated or skipped row would silently double-count or lose space.
    let all: Vec<ChildEntry> = (0..10).map(|i| row(&format!("f{i:02}"), false, Some(i))).collect();
    let mut seen: Vec<String> = Vec::new();
    let mut offset = 0;
    loop {
        let opts = ListOptions {
            sort_by: SortBy::Size,
            order: Order::Desc,
            limit: 3,
            offset,
            ..Default::default()
        };
        let page = sort_and_page(all.clone(), &opts);
        if page.rows.is_empty() {
            break;
        }
        offset += page.rows.len();
        seen.extend(page.rows.iter().map(|r| r.name.clone()));
    }
    let mut unique = seen.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(seen.len(), 10, "every row came back once");
    assert_eq!(unique.len(), 10, "and no row came back twice");
}

#[test]
fn a_type_filter_narrows_the_denominator_too() {
    // `total` has to count what matched the filter, or "3 of 100" reads as a
    // page of a much bigger folder than the caller asked about.
    let opts = ListOptions {
        type_filter: Some(TypeFilter::Dirs),
        ..Default::default()
    };
    let page = sort_and_page(
        vec![
            row("a-file", false, Some(1)),
            row("b-folder", true, Some(2)),
            row("c-folder", true, Some(3)),
        ],
        &opts,
    );
    assert_eq!(page.total, 2);
    assert!(page.rows.iter().all(|r| r.is_directory));
}

#[test]
fn a_last_page_is_not_flagged_truncated() {
    // `truncated` means "there is more"; on the final page there isn't, even
    // though `offset` is non-zero and `returned` is under `limit`.
    let opts = ListOptions {
        limit: 3,
        offset: 8,
        ..Default::default()
    };
    let page = sort_and_page((0..10).map(child).collect(), &opts);
    let result = build_list_dir("/p", Some(page), None, true, Some(Freshness::Fresh), no_space(), &opts);
    assert_eq!(result.returned, Some(2));
    assert_eq!(result.offset, 8);
    assert!(!result.truncated);
}

// ── Params ────────────────────────────────────────────────────────────────

#[test]
fn each_sort_key_carries_the_direction_a_caller_means_by_default() {
    use serde_json::json;
    let by_size = ListOptions::from_params(&json!({ "sortBy": "size" })).unwrap();
    assert_eq!(by_size.order, Order::Desc, "biggest first");
    let by_name = ListOptions::from_params(&json!({ "sortBy": "name" })).unwrap();
    assert_eq!(by_name.order, Order::Asc, "A→Z");
    let explicit = ListOptions::from_params(&json!({ "sortBy": "size", "order": "asc" })).unwrap();
    assert_eq!(explicit.order, Order::Asc, "an explicit direction still wins");
}

#[test]
fn an_unknown_sort_key_is_refused_rather_than_silently_reordered() {
    use serde_json::json;
    // Falling back to name order would hand back a page whose top row is not the
    // biggest, with nothing saying so.
    assert!(ListOptions::from_params(&json!({ "sortBy": "sixe" })).is_err());
    assert!(ListOptions::from_params(&json!({ "order": "descending" })).is_err());
    assert!(ListOptions::from_params(&json!({ "type": "folder" })).is_err());
}

#[test]
fn limit_is_clamped_to_something_a_result_can_carry() {
    use serde_json::json;
    assert_eq!(
        ListOptions::from_params(&json!({ "limit": 9_999 })).unwrap().limit,
        MAX_LIMIT
    );
    assert_eq!(ListOptions::from_params(&json!({ "limit": 0 })).unwrap().limit, 1);
    assert_eq!(ListOptions::from_params(&json!({})).unwrap().limit, DEFAULT_LIMIT);
}
