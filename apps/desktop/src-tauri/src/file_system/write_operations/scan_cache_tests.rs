//! Unit tests for the two things that make the scan-preview cache trustworthy:
//! the request binding (`take_cached_scan_result` refuses an entry describing a
//! different selection) and the coherence canary (`insert_scan_result` refuses
//! to believe a completed walk that carries no per-source results).
//!
//! `super::` here is `scan_cache`, so the private `SCAN_PREVIEW_RESULTS` is in
//! reach; that's deliberate, so these tests can pin the module wall itself.

use super::*;

/// A per-source result shaped like a top-level file, so the entries these tests
/// build are coherent by the canary's rule.
fn one_file(bytes: u64) -> CopyScanResult {
    CopyScanResult {
        file_count: 1,
        dir_count: 0,
        total_bytes: bytes,
        dedup_bytes: bytes,
        top_level_is_directory: false,
    }
}

/// Builds a completed local-walk-shaped entry over `sources`.
fn cached_for(sources: &[&str]) -> CachedScanResult {
    CachedScanResult {
        sources: sources.iter().map(PathBuf::from).collect(),
        files: Vec::new(),
        dirs: Vec::new(),
        file_count: sources.len(),
        total_bytes: 10 * sources.len() as u64,
        dedup_bytes: 10 * sources.len() as u64,
        per_path: sources.iter().map(|s| (PathBuf::from(s), one_file(10))).collect(),
        estimated_compressed_bytes: None,
        inserted_at: Instant::now(),
    }
}

fn paths(list: &[&str]) -> Vec<PathBuf> {
    list.iter().map(PathBuf::from).collect()
}

fn unique_preview(tag: &str) -> String {
    format!("scan-cache-binding-{tag}-{}", uuid::Uuid::new_v4())
}

#[test]
fn cache_hit_when_the_operation_asks_for_exactly_the_previewed_sources() {
    let preview_id = unique_preview("exact");
    insert_scan_result(preview_id.clone(), cached_for(&["/a", "/b"]));

    let hit = take_cached_scan_result(&preview_id, &paths(&["/a", "/b"]));

    assert!(hit.is_some(), "the same selection must be a cache hit");
}

#[test]
fn cache_hit_ignores_the_order_the_frontend_happened_to_send() {
    // Order is a frontend detail (pane sort), and `per_path` is order-rebuilt
    // downstream anyway, so the comparison is set-wise.
    let preview_id = unique_preview("order");
    insert_scan_result(preview_id.clone(), cached_for(&["/a", "/b", "/c"]));

    let hit = take_cached_scan_result(&preview_id, &paths(&["/c", "/a", "/b"]));

    assert!(hit.is_some(), "a reordered selection is the same selection");
}

#[test]
fn a_preview_of_a_different_selection_is_a_cache_miss() {
    let preview_id = unique_preview("foreign");
    insert_scan_result(preview_id.clone(), cached_for(&["/a"]));

    let hit = take_cached_scan_result(&preview_id, &paths(&["/b"]));

    assert!(hit.is_none(), "a preview of /a must not authorize acting on /b");
}

#[test]
fn an_operation_asking_for_more_than_the_preview_walked_is_a_cache_miss() {
    let preview_id = unique_preview("extra");
    insert_scan_result(preview_id.clone(), cached_for(&["/a"]));

    let hit = take_cached_scan_result(&preview_id, &paths(&["/a", "/b"]));

    assert!(hit.is_none(), "an unwalked extra source must not ride a stale preview");
}

#[test]
fn an_operation_asking_for_less_than_the_preview_walked_is_a_cache_miss() {
    // The dangerous direction: the cached file list is a superset, so believing
    // it would act on paths the user didn't select.
    let preview_id = unique_preview("missing");
    insert_scan_result(preview_id.clone(), cached_for(&["/a", "/b"]));

    let hit = take_cached_scan_result(&preview_id, &paths(&["/a"]));

    assert!(hit.is_none(), "a preview covering more than was asked for is not a hit");
}

#[test]
fn a_refused_entry_is_dropped_rather_than_left_for_the_next_caller() {
    // The refusal path still consumes the entry: leaving it behind would let a
    // second operation with the matching selection pick up a preview the first
    // already invalidated by acting.
    let preview_id = unique_preview("dropped");
    insert_scan_result(preview_id.clone(), cached_for(&["/a"]));

    assert!(take_cached_scan_result(&preview_id, &paths(&["/b"])).is_none());
    assert!(
        take_cached_scan_result(&preview_id, &paths(&["/a"])).is_none(),
        "the refused entry must not still be sitting in the cache"
    );
}

/// The canary: a completed walk that counted files but recorded no per-source
/// result is the exact shape the copy pipeline read as `is_directory: false`.
/// `debug_assert!` compiles in for `pnpm check rust-tests` (a debug build), so
/// this holds there; under `--release` the canary degrades to the warn and this
/// test would not panic.
#[test]
#[should_panic(expected = "must carry per_path entries")]
#[cfg(debug_assertions)]
fn inserting_a_completed_walk_with_no_per_source_results_trips_the_canary() {
    let preview_id = unique_preview("canary");
    insert_scan_result(
        preview_id,
        CachedScanResult {
            sources: paths(&["/a"]),
            files: Vec::new(),
            dirs: Vec::new(),
            file_count: 3,
            total_bytes: 30,
            dedup_bytes: 30,
            per_path: Vec::new(),
            estimated_compressed_bytes: None,
            inserted_at: Instant::now(),
        },
    );
}

/// The canary is one-directional. A volume batch scan legitimately caches an
/// empty `files` list with a populated `per_path`, and an empty selection
/// legitimately has neither.
#[test]
fn the_canary_leaves_the_legitimate_shapes_alone() {
    let volume_batch = unique_preview("volume-batch");
    insert_scan_result(volume_batch.clone(), cached_for(&["/a"]));
    assert!(take_cached_scan_result(&volume_batch, &paths(&["/a"])).is_some());

    let nothing_found = unique_preview("empty");
    insert_scan_result(
        nothing_found.clone(),
        CachedScanResult {
            sources: paths(&["/empty-dir"]),
            files: Vec::new(),
            dirs: vec![PathBuf::from("/empty-dir")],
            file_count: 0,
            total_bytes: 0,
            dedup_bytes: 0,
            per_path: Vec::new(),
            estimated_compressed_bytes: None,
            inserted_at: Instant::now(),
        },
    );
    assert!(take_cached_scan_result(&nothing_found, &paths(&["/empty-dir"])).is_some());
}
