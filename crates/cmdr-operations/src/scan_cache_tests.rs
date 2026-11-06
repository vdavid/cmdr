//! Unit tests for the three things that make the scan-preview cache
//! trustworthy: the request binding (`take_cached_scan_result` refuses an entry
//! describing a different selection), the coherence canary (`insert_scan_result`
//! refuses to believe a completed walk that carries no per-source results), and
//! the two named constructors that give the two production shapes names.
//!
//! `super::` here is `scan_cache`, so the private fields and statics are in
//! reach; that's deliberate, so these tests can pin the module wall itself.

use super::*;
use crate::testing::TestOperations;

/// A per-source result shaped like a top-level file, so the entries these tests
/// build are coherent by the canary's rule.
fn one_file(bytes: u64) -> CopyScanResult {
    CopyScanResult {
        file_count: 1,
        dir_count: 0,
        total_bytes: bytes,
        dedup_bytes: bytes,
        top_level_is_directory: false,
        top_level_modified_at: None,
        top_level_created_at: None,
    }
}

/// A completed volume-batch entry over `sources`, one top-level file each.
fn cached_for(sources: &[&str]) -> CachedScanResult {
    let bytes = 10 * sources.len() as u64;
    CachedScanResult::from_volume_batch(
        ScanPreviewPurpose::Transfer,
        paths(sources),
        sources.len(),
        bytes,
        bytes,
        sources.iter().map(|s| (PathBuf::from(s), one_file(10))).collect(),
    )
}

fn paths(list: &[&str]) -> Vec<PathBuf> {
    list.iter().map(PathBuf::from).collect()
}

fn unique_preview(tag: &str) -> String {
    format!("scan-cache-binding-{tag}-{}", uuid::Uuid::new_v4())
}

#[test]
fn cache_hit_when_the_operation_asks_for_exactly_the_previewed_sources() {
    let engine = TestOperations::new();
    let preview_id = unique_preview("exact");
    engine
        .stores()
        .previews()
        .insert_scan_result(preview_id.clone(), cached_for(&["/a", "/b"]));

    let hit = engine
        .stores()
        .previews()
        .take_cached_scan_result(&preview_id, &paths(&["/a", "/b"]));

    assert!(hit.is_some(), "the same selection must be a cache hit");
}

/// A preview that left out Mac-only files sized a different transfer from one
/// that writes them (#314), so the person flipping the toggle at the last
/// second costs a rescan, never totals that lie.
#[test]
fn a_preview_that_left_out_something_else_is_no_hit() {
    use cmdr_fs::mac_only_files::MacOnlyFiles;

    let engine = TestOperations::new();
    let previews = engine.stores().previews();
    for (walked, wanted, hit) in [
        (MacOnlyFiles::Skip, MacOnlyFiles::Skip, true),
        (MacOnlyFiles::Skip, MacOnlyFiles::Include, false),
        (MacOnlyFiles::Include, MacOnlyFiles::Skip, false),
    ] {
        let preview_id = unique_preview("mac-only");
        previews.insert_scan_result(preview_id.clone(), cached_for(&["/a"]).leaving_out(walked));
        assert_eq!(
            previews
                .take_cached_transfer_scan(&preview_id, &paths(&["/a"]), wanted)
                .is_some(),
            hit,
            "walked {walked:?}, wanted {wanted:?}"
        );
    }
}

#[test]
fn cache_hit_ignores_the_order_the_frontend_happened_to_send() {
    let engine = TestOperations::new();
    // Order is a frontend detail (pane sort), and `per_path` is order-rebuilt
    // downstream anyway, so the comparison is set-wise.
    let preview_id = unique_preview("order");
    engine
        .stores()
        .previews()
        .insert_scan_result(preview_id.clone(), cached_for(&["/a", "/b", "/c"]));

    let hit = engine
        .stores()
        .previews()
        .take_cached_scan_result(&preview_id, &paths(&["/c", "/a", "/b"]));

    assert!(hit.is_some(), "a reordered selection is the same selection");
}

#[test]
fn a_preview_of_a_different_selection_is_a_cache_miss() {
    let engine = TestOperations::new();
    let preview_id = unique_preview("foreign");
    engine
        .stores()
        .previews()
        .insert_scan_result(preview_id.clone(), cached_for(&["/a"]));

    let hit = engine
        .stores()
        .previews()
        .take_cached_scan_result(&preview_id, &paths(&["/b"]));

    assert!(hit.is_none(), "a preview of /a must not authorize acting on /b");
}

#[test]
fn an_operation_asking_for_more_than_the_preview_walked_is_a_cache_miss() {
    let engine = TestOperations::new();
    let preview_id = unique_preview("extra");
    engine
        .stores()
        .previews()
        .insert_scan_result(preview_id.clone(), cached_for(&["/a"]));

    let hit = engine
        .stores()
        .previews()
        .take_cached_scan_result(&preview_id, &paths(&["/a", "/b"]));

    assert!(hit.is_none(), "an unwalked extra source must not ride a stale preview");
}

#[test]
fn an_operation_asking_for_less_than_the_preview_walked_is_a_cache_miss() {
    let engine = TestOperations::new();
    // The dangerous direction: the cached file list is a superset, so believing
    // it would act on paths the user didn't select.
    let preview_id = unique_preview("missing");
    engine
        .stores()
        .previews()
        .insert_scan_result(preview_id.clone(), cached_for(&["/a", "/b"]));

    let hit = engine
        .stores()
        .previews()
        .take_cached_scan_result(&preview_id, &paths(&["/a"]));

    assert!(hit.is_none(), "a preview covering more than was asked for is not a hit");
}

#[test]
fn a_refused_entry_is_dropped_rather_than_left_for_the_next_caller() {
    let engine = TestOperations::new();
    // The refusal path still consumes the entry: leaving it behind would let a
    // second operation with the matching selection pick up a preview the first
    // already invalidated by acting.
    let preview_id = unique_preview("dropped");
    engine
        .stores()
        .previews()
        .insert_scan_result(preview_id.clone(), cached_for(&["/a"]));

    assert!(
        engine
            .stores()
            .previews()
            .take_cached_scan_result(&preview_id, &paths(&["/b"]))
            .is_none()
    );
    assert!(
        engine
            .stores()
            .previews()
            .take_cached_scan_result(&preview_id, &paths(&["/a"]))
            .is_none(),
        "the refused entry must not still be sitting in the cache"
    );
}

/// A panic that interrupted an in-place edit leaves an entry nobody can trust,
/// and `take_cached_scan_result` is what binds a delete's path set. So a
/// poisoned store forgets everything: the next delete rescans its sources
/// rather than deleting the files a torn entry still lists.
#[test]
fn after_a_panic_mid_edit_the_next_delete_rescans_its_sources() {
    let previews = ScanPreviews::new();
    previews.insert_scan_result("torn".to_string(), cached_for(&["/a"]));
    previews.tear_mid_mutation_for_test("torn", paths(&["/b"]));

    assert!(
        previews.take_cached_scan_result("torn", &paths(&["/b"])).is_none(),
        "an entry a panic interrupted must never bind a delete"
    );
    assert!(
        previews.take_cached_scan_result("torn", &paths(&["/a"])).is_none(),
        "nor under the sources it walked"
    );
}

/// The other half of forgetting on poison: the store recovers, so a preview
/// the person confirms after the panic still spares its operation a second
/// walk.
#[test]
fn a_preview_settled_after_a_panic_mid_edit_is_still_a_cache_hit() {
    let previews = ScanPreviews::new();
    previews.insert_scan_result("torn".to_string(), cached_for(&["/a"]));
    previews.tear_mid_mutation_for_test("torn", paths(&["/b"]));

    previews.insert_scan_result("after".to_string(), cached_for(&["/c"]));

    assert!(
        previews.take_cached_scan_result("after", &paths(&["/c"])).is_some(),
        "a poisoned store must recover, not skip every preview for the rest of the session"
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
    let engine = TestOperations::new();
    let preview_id = unique_preview("canary");
    engine.stores().previews().insert_scan_result(
        preview_id,
        CachedScanResult::from_volume_batch(ScanPreviewPurpose::Transfer, paths(&["/a"]), 3, 30, 30, Vec::new()),
    );
}

/// The canary is one-directional. A volume batch scan legitimately caches an
/// empty `files` list with a populated `per_path`, and an empty selection
/// legitimately has neither.
#[test]
fn the_canary_leaves_the_legitimate_shapes_alone() {
    let engine = TestOperations::new();
    let volume_batch = unique_preview("volume-batch");
    engine
        .stores()
        .previews()
        .insert_scan_result(volume_batch.clone(), cached_for(&["/a"]));
    assert!(
        engine
            .stores()
            .previews()
            .take_cached_scan_result(&volume_batch, &paths(&["/a"]))
            .is_some()
    );

    let nothing_found = unique_preview("empty");
    engine.stores().previews().insert_scan_result(
        nothing_found.clone(),
        CachedScanResult::from_local_walk(
            ScanPreviewPurpose::Transfer,
            paths(&["/empty-dir"]),
            Vec::new(),
            vec![PathBuf::from("/empty-dir")],
            0,
            0,
            Vec::new(),
            None,
        ),
    );
    assert!(
        engine
            .stores()
            .previews()
            .take_cached_scan_result(&nothing_found, &paths(&["/empty-dir"]))
            .is_some()
    );
}

// ---- the two named constructors ----

/// `from_local_walk` derives `file_count` from the file list it was handed, so
/// the count and the list can't drift apart at a call site.
#[test]
fn from_local_walk_derives_its_file_count_from_the_files_it_walked() {
    let file = FileInfo {
        path: PathBuf::from("/src/one.bin"),
        source_root: PathBuf::from("/src"),
        size: 10,
        progress_bytes: 10,
        is_symlink: false,
        tree_node: None,
    };
    let cached = CachedScanResult::from_local_walk(
        ScanPreviewPurpose::Transfer,
        paths(&["/src"]),
        vec![file],
        paths(&["/src"]),
        10,
        10,
        vec![(PathBuf::from("/src"), one_file(10))],
        None,
    );

    assert_eq!(cached.file_count, 1);
    assert_eq!(cached.files.len(), 1);
}

/// `from_local_walk` asserts the shape it claims: a walk that found files
/// recorded a per-source result. Stronger than the insert-time canary, which
/// can only speak about `file_count`. Debug builds only.
#[test]
#[should_panic(expected = "must record a per-source result")]
#[cfg(debug_assertions)]
fn from_local_walk_refuses_to_build_a_walk_that_found_files_but_no_sources() {
    let file = FileInfo {
        path: PathBuf::from("/src/one.bin"),
        source_root: PathBuf::from("/src"),
        size: 10,
        progress_bytes: 10,
        is_symlink: false,
        tree_node: None,
    };
    let _ = CachedScanResult::from_local_walk(
        ScanPreviewPurpose::Transfer,
        paths(&["/src"]),
        vec![file],
        Vec::new(),
        10,
        10,
        Vec::new(),
        None,
    );
}

/// `from_volume_batch` builds the remote shape: no per-file list (consumers
/// read `per_path`, and a delete that needs paths recurses itself) and never a
/// compress estimate, since remote sources don't sample.
#[test]
fn from_volume_batch_carries_no_file_list_and_no_estimate() {
    let cached = CachedScanResult::from_volume_batch(
        ScanPreviewPurpose::Transfer,
        paths(&["/a"]),
        4,
        40,
        40,
        vec![(PathBuf::from("/a"), one_file(40))],
    );

    assert!(cached.files.is_empty());
    assert!(cached.dirs.is_empty());
    assert!(cached.estimated_compressed_bytes.is_none());
    assert_eq!(cached.file_count, 4);
}

// ============================================================================
// What a cost estimate reads off a settled preview
// ============================================================================

fn folder(files: usize, dirs: usize, bytes: u64) -> CopyScanResult {
    CopyScanResult {
        file_count: files,
        dir_count: dirs,
        total_bytes: bytes,
        dedup_bytes: bytes,
        top_level_is_directory: true,
        top_level_modified_at: None,
        top_level_created_at: None,
    }
}

#[test]
fn cost_facts_carry_a_volume_scans_kept_files_and_its_folder_count() {
    let engine = TestOperations::new();
    let preview_id = unique_preview("cost-volume");
    let kept = vec![
        ScannedFile {
            size: 30,
            modified_at: Some(1_000),
        },
        ScannedFile {
            size: 10,
            modified_at: None,
        },
    ];
    let cached = CachedScanResult::from_volume_batch(
        ScanPreviewPurpose::Transfer,
        paths(&["/a"]),
        2,
        40,
        40,
        vec![(PathBuf::from("/a"), folder(2, 3, 40))],
    )
    .keeping_files(Some(kept.clone()));
    engine
        .stores()
        .previews()
        .insert_scan_result(preview_id.clone(), cached);

    let facts = engine.cached_cost_facts(&preview_id).expect("a settled preview");

    assert_eq!(facts.files, 2);
    assert_eq!(facts.dirs, 3);
    assert_eq!(facts.bytes, 40);
    assert_eq!(facts.per_file, Some(kept));
}

#[test]
fn cost_facts_of_a_volume_scan_that_kept_nothing_have_no_per_file_list() {
    let engine = TestOperations::new();
    let preview_id = unique_preview("cost-volume-bare");
    engine
        .stores()
        .previews()
        .insert_scan_result(preview_id.clone(), cached_for(&["/a"]));

    let facts = engine.cached_cost_facts(&preview_id).expect("a settled preview");

    assert_eq!(facts.files, 1);
    assert_eq!(facts.per_file, None);
}

#[test]
fn cost_facts_of_a_local_walk_list_every_files_size_undated() {
    let engine = TestOperations::new();
    let preview_id = unique_preview("cost-local");
    let file = |size: u64| FileInfo {
        path: PathBuf::from(format!("/src/{size}.bin")),
        source_root: PathBuf::from("/src"),
        size,
        progress_bytes: size,
        is_symlink: false,
        tree_node: None,
    };
    let cached = CachedScanResult::from_local_walk(
        ScanPreviewPurpose::Transfer,
        paths(&["/src"]),
        vec![file(5), file(7)],
        paths(&["/src"]),
        12,
        12,
        vec![(PathBuf::from("/src"), folder(2, 1, 12))],
        None,
    );
    engine
        .stores()
        .previews()
        .insert_scan_result(preview_id.clone(), cached);

    let facts = engine.cached_cost_facts(&preview_id).expect("a settled preview");

    let undated = |size: u64| ScannedFile {
        size,
        modified_at: None,
    };
    assert_eq!(facts.per_file, Some(vec![undated(5), undated(7)]));
    assert_eq!(facts.dirs, 1);
}

#[test]
fn cost_facts_of_an_unknown_preview_are_none() {
    let engine = TestOperations::new();
    assert!(engine.cached_cost_facts("no-such-preview").is_none());
}
