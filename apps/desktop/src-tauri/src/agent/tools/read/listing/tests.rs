//! `list_dir` unit tests: the coverage flags, the size contract, and the ordering
//! and paging rules. All pure over resolved inputs — no index, no filesystem.

use super::*;
use crate::file_system::volume::SpaceInfo;
use crate::search::format_size;

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
    ChildEntry::new(
        format!("some-reasonably-long-file-name-{index}.jpeg"),
        false,
        false,
        Some(1_234_567),
        false,
        Some(1_700_000_000),
    )
}

/// A named row with a size, for the ordering tests.
fn row(name: &str, is_directory: bool, size: Option<u64>) -> ChildEntry {
    ChildEntry::new(name.to_string(), is_directory, false, size, false, size)
}

/// A named folder row whose size is only a lower bound.
fn lower_bound_row(name: &str, size: u64) -> ChildEntry {
    ChildEntry::new(name.to_string(), true, false, Some(size), true, None)
}

/// A volume block with no space known: the default for tests that aren't about space.
fn no_space() -> VolumeBlock {
    VolumeBlock::new("root".to_string(), None)
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
        VolumeBlock::new(
            "root".to_string(),
            Some(SpaceInfo::bounded(2_000_000_000_000, 214_300_000_000)),
        ),
        &ListOptions::default(),
    );
    assert_eq!(result.volume.id, "root");
    assert_eq!(result.volume.available_bytes, Some(214_300_000_000));
    // The raw pair is what arithmetic needs; the human pair is what a sentence needs,
    // and the agent can't compute one from the other.
    assert_eq!(
        result.volume.total_human.as_deref(),
        Some(format_size(2_000_000_000_000)).as_deref()
    );
    assert_eq!(
        result.volume.available_human.as_deref(),
        Some(format_size(214_300_000_000)).as_deref()
    );
}

#[test]
fn an_unwatched_volume_has_no_human_space_either() {
    // Present exactly when the byte counterparts are: a "0 B" free would read as a
    // full disk.
    let result = build_list_dir(
        "/Users/x",
        Some(page_of(vec![])),
        None,
        true,
        Some(Freshness::Fresh),
        no_space(),
        &ListOptions::default(),
    );
    assert_eq!(result.volume.total_human, None);
    assert_eq!(result.volume.available_human, None);
}

// ── Human-readable forms ──────────────────────────────────────────────────

#[test]
fn a_lower_bound_size_carries_the_symbol_inside_the_string() {
    // The anti-lying property: the `≥` lives INSIDE `sizeHuman`, so a model that
    // quotes the string cannot present a lower bound as an exact total, however it
    // treats the separate `sizeIsLowerBound` flag.
    let stats = dir_stats(2_000_000_000_000, false, false, false);
    let result = build_list_dir(
        "/Users/x",
        Some(page_of(vec![lower_bound_row("archive", 1_000_000_000_000)])),
        Some(&stats),
        true,
        Some(Freshness::Fresh),
        no_space(),
        &ListOptions::default(),
    );
    let rows = result.children.as_ref().expect("an indexed listing");
    let human = rows[0].size_human.as_deref().expect("a known size");
    assert!(human.starts_with("≥ "), "a lower-bound child reads '{human}'");
    assert!(result.size.as_ref().unwrap().recursive_size_human.starts_with("≥ "));
}

#[test]
fn an_exact_size_carries_no_symbol() {
    let stats = dir_stats(1_024, true, false, false);
    let result = build_list_dir(
        "/Users/x",
        Some(page_of(vec![row("a-file", false, Some(1_024))])),
        Some(&stats),
        true,
        Some(Freshness::Fresh),
        no_space(),
        &ListOptions::default(),
    );
    let rows = result.children.as_ref().expect("an indexed listing");
    assert_eq!(rows[0].size_human.as_deref(), Some("1 KB"));
    assert_eq!(result.size.unwrap().recursive_size_human, "1 KB");
}

#[test]
fn an_unknown_size_has_no_human_form_rather_than_zero_bytes() {
    // A folder with no `dir_stats` row is unknown, not empty. A rendered "0 B"
    // would be a number the index can't back.
    let result = build_list_dir(
        "/Users/x",
        Some(page_of(vec![row("mystery", true, None)])),
        None,
        true,
        Some(Freshness::Fresh),
        no_space(),
        &ListOptions::default(),
    );
    let rows = result.children.as_ref().expect("an indexed listing");
    assert_eq!(rows[0].size, None);
    assert_eq!(rows[0].size_human, None);
}

#[test]
fn a_modified_epoch_comes_with_the_date_it_means() {
    let result = build_list_dir(
        "/Users/x",
        Some(page_of(vec![child(0)])),
        None,
        true,
        Some(Freshness::Fresh),
        no_space(),
        &ListOptions::default(),
    );
    let rows = result.children.as_ref().expect("an indexed listing");
    assert_eq!(rows[0].modified, Some(1_700_000_000));
    assert_eq!(rows[0].modified_human.as_deref(), Some("2023-11-14"));
    // No timestamp, no date: nothing invented for a row the index has no mtime for.
    let no_mtime = row("x", false, None);
    assert_eq!(no_mtime.modified_human, None);
}

// ── The remainder ─────────────────────────────────────────────────────────

/// A page of `limit` rows out of five known-size children totalling 660 bytes,
/// inside a folder whose own recursive total is `folder_total`.
fn five_child_page(limit: usize, folder_stats: &DirStats) -> ListDirResult {
    let opts = ListOptions {
        limit,
        ..Default::default()
    };
    let page = sort_and_page(
        vec![
            row("a", false, Some(100)),
            row("b", false, Some(200)),
            row("c", false, Some(300)),
            row("d", false, Some(50)),
            row("e", false, Some(10)),
        ],
        &opts,
    );
    build_list_dir(
        "/p",
        Some(page),
        Some(folder_stats),
        true,
        Some(Freshness::Fresh),
        no_space(),
        &opts,
    )
}

#[test]
fn the_remainder_says_what_the_rows_it_did_not_show_add_up_to() {
    // Two of five children returned. The model can't subtract reliably, so the
    // answer carries what the other three come to.
    let stats = dir_stats(1_000, true, false, false);
    let result = five_child_page(2, &stats);
    let rem = result.remainder.expect("three children weren't shown");
    assert_eq!(rem.count, 3);
    assert_eq!(rem.bytes, 700, "1,000 total minus the 300 on this page");
    assert_eq!(rem.human, format_size(700));
    assert!(!rem.is_approximate);
}

#[test]
fn the_remainder_is_omitted_when_this_page_is_the_whole_folder() {
    // Nothing beyond the page ⇒ no remainder object at all, rather than a zero the
    // model would have to interpret.
    let stats = dir_stats(1_000, true, false, false);
    let result = five_child_page(5, &stats);
    assert_eq!(result.returned, Some(5));
    assert!(result.remainder.is_none());
}

#[test]
fn the_remainder_is_omitted_when_a_returned_child_size_is_unknown() {
    // With an unknown in the subtraction we can't speak: a wrong remainder is worse
    // than none.
    let opts = ListOptions {
        limit: 2,
        ..Default::default()
    };
    let stats = dir_stats(1_000, true, false, false);
    let page = sort_and_page(
        vec![
            row("a", true, None),
            row("b", false, Some(200)),
            row("c", false, Some(300)),
        ],
        &opts,
    );
    let result = build_list_dir(
        "/p",
        Some(page),
        Some(&stats),
        true,
        Some(Freshness::Fresh),
        no_space(),
        &opts,
    );
    assert_eq!(result.returned, Some(2));
    assert!(result.remainder.is_none(), "an unknown size silences the remainder");
}

#[test]
fn the_remainder_is_omitted_without_a_folder_total_to_subtract_from() {
    let opts = ListOptions {
        limit: 2,
        ..Default::default()
    };
    let page = sort_and_page(
        vec![
            row("a", false, Some(1)),
            row("b", false, Some(2)),
            row("c", false, Some(3)),
        ],
        &opts,
    );
    let result = build_list_dir("/p", Some(page), None, true, Some(Freshness::Fresh), no_space(), &opts);
    assert!(result.remainder.is_none());
}

#[test]
fn the_remainder_is_approximate_when_the_folder_total_is_a_lower_bound() {
    // Bounds run in BOTH directions here (an understated folder total pulls the
    // remainder down, an understated child pushes it up), so the flag claims no
    // direction and the string wears a `~`, not a `≥`.
    let stats = dir_stats(1_000, false, false, false);
    let result = five_child_page(2, &stats);
    let rem = result.remainder.expect("three children weren't shown");
    assert!(rem.is_approximate);
    assert!(
        rem.human.starts_with("~ "),
        "an approximate remainder reads '{}'",
        rem.human
    );
    assert!(!rem.human.contains('≥'), "no direction is claimed");
}

#[test]
fn the_remainder_is_approximate_when_a_returned_child_is_a_lower_bound() {
    let opts = ListOptions {
        limit: 1,
        ..Default::default()
    };
    let stats = dir_stats(1_000, true, false, false);
    let page = sort_and_page(
        vec![lower_bound_row("a-archive", 100), row("b", false, Some(200))],
        &opts,
    );
    let result = build_list_dir(
        "/p",
        Some(page),
        Some(&stats),
        true,
        Some(Freshness::Fresh),
        no_space(),
        &opts,
    );
    let rem = result.remainder.expect("one child wasn't shown");
    assert_eq!(rem.count, 1);
    assert!(rem.is_approximate);
}

#[test]
fn the_remainder_never_goes_below_zero() {
    // A lower-bound folder total can be smaller than the children we already listed.
    let stats = dir_stats(100, false, false, false);
    let result = five_child_page(2, &stats);
    let rem = result.remainder.expect("three children weren't shown");
    assert_eq!(rem.bytes, 0);
    assert!(rem.is_approximate);
}

#[test]
fn a_filtered_listing_has_no_remainder_at_all() {
    // With `type` narrowing the rows, `count` would be "folders not shown" while the
    // folder's recursive total still counts every loose file: two different
    // populations in one sentence. Omit rather than mislead.
    let opts = ListOptions {
        limit: 1,
        type_filter: Some(TypeFilter::Dirs),
        ..Default::default()
    };
    let stats = dir_stats(1_000, true, false, false);
    let page = sort_and_page(
        vec![
            row("a-folder", true, Some(100)),
            row("b-folder", true, Some(200)),
            row("c-file", false, Some(700)),
        ],
        &opts,
    );
    let result = build_list_dir(
        "/p",
        Some(page),
        Some(&stats),
        true,
        Some(Freshness::Fresh),
        no_space(),
        &opts,
    );
    assert_eq!(result.total, Some(2));
    assert!(result.remainder.is_none());
}

#[test]
fn the_wire_shape_carries_every_spoken_field_in_camel_case() {
    // What the model actually reads. Params are camelCase across every tool, so a
    // snake_case slip here is a field an agent pattern-matches past.
    let opts = ListOptions {
        sort_by: SortBy::Size,
        order: Order::Desc,
        limit: 2,
        ..Default::default()
    };
    let stats = dir_stats(2_000_000_000_000, false, false, false);
    let page = sort_and_page(
        vec![
            lower_bound_row("Photos", 1_900_000_000_000),
            row("clip.mov", false, Some(4_500_000_000)),
            row("notes.txt", false, Some(2_048)),
        ],
        &opts,
    );
    let result = build_list_dir(
        "/Users/x/Media",
        Some(page),
        Some(&stats),
        true,
        Some(Freshness::Fresh),
        VolumeBlock::new(
            "root".to_string(),
            Some(SpaceInfo::bounded(2_000_000_000_000, 214_300_000_000)),
        ),
        &opts,
    );
    let json = serde_json::to_value(&result).unwrap();
    assert_eq!(json["size"]["recursiveSizeHuman"], "≥ 1.8 TB");
    assert_eq!(json["volume"]["totalHuman"], "1.8 TB");
    assert_eq!(json["volume"]["availableHuman"], "199.6 GB");
    assert_eq!(json["children"][0]["sizeHuman"], "≥ 1.7 TB");
    assert_eq!(json["children"][1]["sizeHuman"], "4.2 GB");
    assert_eq!(json["remainder"]["count"], 1);
    assert_eq!(json["remainder"]["isApproximate"], true);
    // A row with no mtime carries no `modifiedHuman` key at all.
    assert!(json["children"][0].get("modifiedHuman").is_none());
}

#[test]
fn an_unindexed_folder_has_no_remainder() {
    let result = build_list_dir(
        "/nas/share",
        None,
        None,
        false,
        None,
        no_space(),
        &ListOptions::default(),
    );
    assert!(result.remainder.is_none());
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
