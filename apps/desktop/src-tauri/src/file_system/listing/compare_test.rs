//! Compare directories (⇧F2): which rows each pane marks against the other.

use super::caching_test_support::{TestListing, TestListingGuard};
use super::compare::{CompareDirectoriesMode, compare_directories};
use super::diff_emitter::flush_now_for_test;
use super::metadata::FileEntry;
use super::sorting::{DirectorySortMode, SortColumn, SortOrder, sort_entries};

fn file(dir: &str, name: &str, size: u64, modified: u64) -> FileEntry {
    FileEntry {
        size: Some(size),
        modified_at: Some(modified),
        ..FileEntry::new(name.to_string(), format!("{dir}/{name}"), false, false)
    }
}

fn folder(dir: &str, name: &str) -> FileEntry {
    FileEntry::new(name.to_string(), format!("{dir}/{name}"), true, false)
}

fn pane(tag: &str, dir: &str, entries: Vec<FileEntry>) -> TestListingGuard {
    let mut entries = entries;
    sort_entries(
        &mut entries,
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
    );
    TestListing::new()
        .path(dir)
        .include_hidden(false)
        .entries(entries)
        .insert(tag)
}

fn marked(
    left: &TestListingGuard,
    right: &TestListingGuard,
    mode: CompareDirectoriesMode,
) -> (Vec<String>, Vec<String>) {
    let result = compare_directories(left.id(), false, right.id(), false, mode).expect("both listings are cached");
    let names = |listing: &TestListingGuard, rows: &[usize]| -> Vec<String> {
        let entries = listing.entries();
        let shown: Vec<&FileEntry> = entries.iter().filter(|e| !e.is_hidden).collect();
        rows.iter().map(|&row| shown[row].name.clone()).collect()
    };
    (names(left, &result.left), names(right, &result.right))
}

const L: &str = "/test/compare/left";
const R: &str = "/test/compare/right";

#[test]
fn safety_revision_changes_on_resort_without_a_watcher_event() {
    let left = pane("safety-sort-l", L, vec![file(L, "a", 1, 1)]);
    let right = pane("safety-sort-r", R, vec![]);
    super::operations::resort_listing(
        left.id(),
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
        None,
        false,
        None,
        false,
        None,
    )
    .unwrap();
    let result = compare_directories(left.id(), false, right.id(), false, CompareDirectoriesMode::Missing).unwrap();
    assert_eq!(result.left_sequence, 1);
}

#[test]
fn safety_revision_changes_inside_the_low_level_mutation() {
    let left = pane("safety-mutation-l", L, vec![]);
    let right = pane("safety-mutation-r", R, vec![]);
    super::diff_emitter::hold_for_test(left.id());
    super::caching::insert_entry_sorted(left.id(), file(L, "a", 1, 1)).unwrap();
    let result = compare_directories(left.id(), false, right.id(), false, CompareDirectoriesMode::Missing).unwrap();
    assert_eq!(result.left_sequence, 1);
}

#[test]
fn safety_successive_removes_publish_before_returning() {
    let left = pane("safety-removes", L, vec![file(L, "a", 1, 1), file(L, "b", 1, 1)]);
    super::diff_emitter::hold_for_test(left.id());
    super::caching::remove_entry_by_name(left.id(), std::ffi::OsStr::new("a")).unwrap();
    super::caching::remove_entry_by_name(left.id(), std::ffi::OsStr::new("b")).unwrap();
    assert_eq!(super::diff_emitter::pending_count(left.id()), 2);
    assert_eq!(
        left.with_listing(|l| l.sequence.load(std::sync::atomic::Ordering::Acquire)),
        2
    );
    let batches = super::diff_emitter::take_batches_for_test(left.id());
    assert_eq!(
        batches
            .iter()
            .map(|b| (b.from_sequence, b.sequence, b.total_count))
            .collect::<Vec<_>>(),
        vec![(0, 1, 1), (1, 2, 0)]
    );
    assert_eq!(
        batches.iter().map(|b| b.changes[0].index).collect::<Vec<_>>(),
        vec![0, 0]
    );
}

#[test]
fn safety_stale_sort_hidden_and_consumption_refuse_without_mutating() {
    use super::operations::{ListingLookupError, get_selection_snapshot, resort_listing, set_listing_include_hidden};
    let left = pane("safety-stale", L, vec![file(L, "a", 1, 1), file(L, "b", 1, 1)]);
    let changed = resort_listing(
        left.id(),
        SortColumn::Name,
        SortOrder::Descending,
        DirectorySortMode::LikeFiles,
        Some("a"),
        false,
        Some(&[0]),
        false,
        Some(0),
    )
    .unwrap();
    assert_eq!(
        (changed.sequence, changed.total_count, changed.new_cursor_index),
        (1, 2, Some(1))
    );
    assert_eq!(changed.new_selected_indices, Some(vec![1]));
    let before = left.entry_names();
    assert!(matches!(
        resort_listing(
            left.id(),
            SortColumn::Size,
            SortOrder::Ascending,
            DirectorySortMode::LikeFiles,
            None,
            false,
            Some(&[0]),
            false,
            Some(0)
        ),
        Err(ListingLookupError::Changed { .. })
    ));
    assert!(matches!(
        set_listing_include_hidden(left.id(), true, Some(0), Some("a"), Some(&[0]), false),
        Err(ListingLookupError::Changed { .. })
    ));
    assert!(matches!(
        get_selection_snapshot(left.id(), false, &[0], 0),
        Err(ListingLookupError::Changed { .. })
    ));
    assert!(matches!(
        get_selection_snapshot(left.id(), true, &[0], 1),
        Err(ListingLookupError::Changed { .. })
    ));
    assert_eq!(left.entry_names(), before);
    left.with_listing(|l| {
        assert_eq!(l.sequence.load(std::sync::atomic::Ordering::Acquire), 1);
        assert!(!l.include_hidden());
        assert_eq!(l.sort_by, SortColumn::Name);
        assert_eq!(l.sort_order, SortOrder::Descending);
    });
    let consumed = get_selection_snapshot(left.id(), false, &[1], 1).unwrap();
    assert_eq!(consumed.paths, vec![format!("{L}/a")]);
    assert_eq!((consumed.file_count, consumed.folder_count), (1, 0));
}

#[test]
fn safety_one_remove_helper_is_one_transition_and_hidden_only_writes_are_silent() {
    let left = pane(
        "safety-remove-batch",
        L,
        vec![file(L, "a", 1, 1), file(L, "b", 1, 1), file(L, "c", 1, 1)],
    );
    super::diff_emitter::hold_for_test(left.id());
    super::caching::remove_entries_by_paths(left.id(), &[format!("{L}/a").into(), format!("{L}/c").into()]);
    let batches = super::diff_emitter::pending_batches_for_test(left.id());
    assert_eq!(batches.len(), 1);
    assert_eq!(
        (batches[0].from_sequence, batches[0].sequence, batches[0].total_count),
        (0, 1, 1)
    );
    assert_eq!(
        batches[0].changes.iter().map(|c| c.index).collect::<Vec<_>>(),
        vec![2, 0]
    );
    super::caching::insert_entry_sorted(left.id(), file(L, ".hidden", 1, 1)).unwrap();
    assert_eq!(
        left.with_listing(|l| l.sequence.load(std::sync::atomic::Ordering::Acquire)),
        1
    );
    assert_eq!(super::diff_emitter::pending_batches_for_test(left.id()).len(), 1);
}

#[test]
fn safety_replacement_diffs_current_rows_against_its_final_sorted_entries() {
    let left = pane("safety-replacement", L, vec![file(L, "a", 1, 1)]);
    super::operations::resort_listing(
        left.id(),
        SortColumn::Name,
        SortOrder::Descending,
        DirectorySortMode::LikeFiles,
        None,
        false,
        None,
        false,
        Some(0),
    )
    .unwrap();
    super::diff_emitter::hold_for_test(left.id());
    super::caching::insert_entry_sorted(left.id(), file(L, "b", 1, 1)).unwrap();
    // A reader returns ascending protocol order after a sort and another mutation committed.
    super::operations::update_listing_entries(
        left.id(),
        vec![file(L, "a", 1, 1), file(L, "c", 1, 1)],
        super::cached_listing::OverlayRows::Recounted(0),
    );
    let batches = super::diff_emitter::pending_batches_for_test(left.id());
    assert_eq!(batches.len(), 2);
    let batch = &batches[1];
    assert_eq!((batch.from_sequence, batch.sequence, batch.total_count), (2, 3, 2));
    assert_eq!(left.entry_names(), vec!["c", "a"]);
    assert!(
        batch
            .changes
            .iter()
            .any(|c| c.entry.name == "b" && c.index == 0 && c.change_type == super::diff::DiffChangeType::Remove)
    );
    assert!(
        batch
            .changes
            .iter()
            .any(|c| c.entry.name == "c" && c.index == 0 && c.change_type == super::diff::DiffChangeType::Add)
    );
}

#[test]
fn safety_hidden_toggle_preserves_exact_surviving_identities() {
    use super::operations::set_listing_include_hidden;
    let left = pane(
        "safety-hidden",
        L,
        vec![file(L, ".hidden", 1, 1), file(L, "a", 1, 1), file(L, "b", 1, 1)],
    );
    let shown = set_listing_include_hidden(left.id(), true, Some(0), Some("b"), Some(&[0, 1]), true).unwrap();
    assert_eq!(
        (shown.sequence, shown.total_count, shown.new_cursor_index),
        (1, 3, Some(2))
    );
    assert_eq!(
        shown.new_selected_indices,
        Some(vec![1, 2]),
        "newly shown hidden files were not selected"
    );
    let same = set_listing_include_hidden(left.id(), true, Some(1), Some("b"), Some(&[1, 2]), false).unwrap();
    assert_eq!(same.sequence, 1);
    let hidden = set_listing_include_hidden(left.id(), false, Some(1), Some("b"), Some(&[0, 2]), false).unwrap();
    assert_eq!(
        (hidden.sequence, hidden.total_count, hidden.new_cursor_index),
        (2, 2, Some(1))
    );
    assert_eq!(hidden.new_selected_indices, Some(vec![1]));
}

#[test]
fn safety_sort_discards_queued_batches_but_cannot_restamp_a_drained_batch() {
    let left = pane("safety-old-batches", L, vec![]);
    super::diff_emitter::hold_for_test(left.id());
    super::caching::insert_entry_sorted(left.id(), file(L, "a", 1, 1)).unwrap();
    let emitted = super::diff_emitter::take_batches_for_test(left.id());
    super::diff_emitter::hold_for_test(left.id());
    super::caching::insert_entry_sorted(left.id(), file(L, "b", 1, 1)).unwrap();
    let sorted = super::operations::resort_listing(
        left.id(),
        SortColumn::Name,
        SortOrder::Descending,
        DirectorySortMode::LikeFiles,
        None,
        false,
        None,
        false,
        Some(2),
    )
    .unwrap();
    assert_eq!(sorted.sequence, 3);
    assert_eq!(emitted[0].sequence, 1);
    assert_eq!(emitted[0].total_count, 1);
    assert!(super::diff_emitter::pending_batches_for_test(left.id()).is_empty());
    flush_now_for_test(left.id());
    assert_eq!(
        left.with_listing(|l| l.sequence.load(std::sync::atomic::Ordering::Acquire)),
        3
    );
}

#[test]
fn stable_scratch_allows_guarded_consumption_and_comparison() {
    let _settings = crate::file_system::staging::ShowTempsGuard::set(false);
    let owner = std::sync::Arc::new(());
    let temp = crate::file_system::staging::StagingTemp::mint(
        &std::path::Path::new(L).join("a"),
        Some(std::sync::Arc::downgrade(&owner)),
    );
    let owned_name = temp.path().file_name().unwrap().to_str().unwrap();
    let left = pane(
        "safety-scratch",
        L,
        vec![
            file(L, owned_name, 1, 1),
            file(L, "b.cmdr-tmp-leftover", 1, 1),
            file(L, "c.sb-shown", 1, 1),
        ],
    );
    let right = pane("safety-scratch-r", R, vec![]);
    let comparison = compare_directories(left.id(), false, right.id(), false, CompareDirectoriesMode::Missing).unwrap();
    assert!(comparison.settled);
    assert_eq!(comparison.left, vec![0, 1]);
    let snapshot = super::operations::get_selection_snapshot(left.id(), false, &[0, 1], 0).unwrap();
    assert_eq!(
        snapshot.paths,
        vec![format!("{L}/b.cmdr-tmp-leftover"), format!("{L}/c.sb-shown")]
    );
    let sorted = super::operations::resort_listing(
        left.id(),
        SortColumn::Name,
        SortOrder::Descending,
        DirectorySortMode::LikeFiles,
        None,
        false,
        Some(&[0]),
        false,
        Some(0),
    )
    .unwrap();
    assert_eq!(sorted.total_count, 2);
    assert_eq!(sorted.new_selected_indices, Some(vec![1]));
    let shown =
        super::operations::set_listing_include_hidden(left.id(), true, Some(sorted.sequence), None, Some(&[1]), false)
            .unwrap();
    assert_eq!(shown.total_count, 2);
    assert_eq!(
        super::operations::get_selection_snapshot(left.id(), true, &[1], shown.sequence)
            .unwrap()
            .paths,
        vec![format!("{L}/b.cmdr-tmp-leftover")]
    );
    assert!(
        !compare_directories(right.id(), true, right.id(), false, CompareDirectoriesMode::Missing)
            .unwrap()
            .settled
    );
}

#[test]
fn scratch_owner_expiry_refuses_each_stale_guard_then_allows_retry() {
    use super::operations::{ListingLookupError, get_selection_snapshot, resort_listing, set_listing_include_hidden};
    use crate::file_system::staging::{ShowTempsGuard, StagingTemp};
    let _settings = ShowTempsGuard::set(false);
    for consumer in 0..3 {
        let owner = std::sync::Arc::new(());
        let temp = StagingTemp::mint(
            &std::path::Path::new(L).join("a"),
            Some(std::sync::Arc::downgrade(&owner)),
        );
        let name = temp.path().file_name().unwrap().to_str().unwrap();
        let left = pane("scratch-expiry", L, vec![file(L, name, 1, 1), file(L, "b", 1, 1)]);
        super::diff_emitter::hold_for_test(left.id());
        assert_eq!(left.with_listing(|l| l.pane_rows().len()), 1);
        drop(owner); // The staging guard remains alive, but its operation does not.
        let consume = |revision| -> Result<Vec<String>, ListingLookupError> {
            match consumer {
                0 => get_selection_snapshot(left.id(), false, &[1], revision).map(|s| s.paths),
                1 => resort_listing(
                    left.id(),
                    SortColumn::Name,
                    SortOrder::Ascending,
                    DirectorySortMode::LikeFiles,
                    None,
                    false,
                    Some(&[1]),
                    false,
                    Some(revision),
                )
                .map(|s| {
                    assert_eq!(s.new_selected_indices, Some(vec![1]));
                    vec![format!("{L}/b")]
                }),
                _ => set_listing_include_hidden(left.id(), true, Some(revision), None, Some(&[1]), false).map(|s| {
                    assert_eq!(s.new_selected_indices, Some(vec![1]));
                    vec![format!("{L}/b")]
                }),
            }
        };
        assert!(matches!(consume(0), Err(ListingLookupError::Changed { .. })));
        let batches = super::diff_emitter::pending_batches_for_test(left.id());
        assert_eq!(batches.len(), 1);
        assert_eq!(
            (batches[0].from_sequence, batches[0].sequence, batches[0].total_count),
            (0, 1, 2)
        );
        assert_eq!(
            (batches[0].changes[0].change_type, batches[0].changes[0].index),
            (super::diff::DiffChangeType::Add, 0)
        );
        assert_eq!(consume(1).unwrap(), vec![format!("{L}/b")]);
    }
}

#[test]
fn both_scratch_settings_publish_drift_and_reject_old_selection() {
    use crate::file_system::staging::{ShowTempsGuard, StagingTemp, set_show_safe_save_files, set_show_staging_temps};
    let _settings = ShowTempsGuard::set_both(false, false);
    let owner = std::sync::Arc::new(());
    let temp = StagingTemp::mint(
        &std::path::Path::new(L).join("a"),
        Some(std::sync::Arc::downgrade(&owner)),
    );
    let name = temp.path().file_name().unwrap().to_str().unwrap();
    let left = pane(
        "scratch-settings",
        L,
        vec![file(L, name, 1, 1), file(L, "b.sb-temp", 1, 1), file(L, "c", 1, 1)],
    );
    let right = pane("scratch-settings-right", R, vec![]);
    super::diff_emitter::hold_for_test(left.id());
    for (revision, show_staging, show_safe, count) in [
        (0, true, false, 2),
        (1, true, true, 3),
        (2, false, true, 2),
        (3, false, false, 1),
    ] {
        set_show_staging_temps(show_staging);
        set_show_safe_save_files(show_safe);
        assert!(matches!(
            super::operations::get_selection_snapshot(left.id(), false, &[0], revision),
            Err(super::operations::ListingLookupError::Changed { .. })
        ));
        let batches = super::diff_emitter::pending_batches_for_test(left.id());
        assert_eq!(batches.last().unwrap().total_count, count);
        let snapshot = super::operations::get_selection_snapshot(left.id(), false, &[count - 1], revision + 1).unwrap();
        assert_eq!(snapshot.paths, vec![format!("{L}/c")]);
        let cmp = compare_directories(left.id(), false, right.id(), false, CompareDirectoriesMode::Missing).unwrap();
        assert!(cmp.settled);
        assert_eq!(cmp.left_sequence, revision + 1);
        assert_eq!(cmp.left.len(), count);
    }
}

#[test]
fn scratch_drift_precedes_entry_mutation_in_the_correct_row_spaces() {
    use crate::file_system::staging::{ShowTempsGuard, StagingTemp};
    let _settings = ShowTempsGuard::set(false);
    let owner = std::sync::Arc::new(());
    let temp = StagingTemp::mint(
        &std::path::Path::new(L).join("a"),
        Some(std::sync::Arc::downgrade(&owner)),
    );
    let name = temp.path().file_name().unwrap().to_str().unwrap();
    let left = pane(
        "scratch-drift-remove",
        L,
        vec![file(L, name, 1, 1), file(L, "b", 1, 1), file(L, "c", 1, 1)],
    );
    super::diff_emitter::hold_for_test(left.id());
    drop(owner);
    let (rows, _) = super::caching::remove_entry_by_name(left.id(), std::ffi::OsStr::new("b")).unwrap();
    assert_eq!(rows.before, Some(1));
    let batches = super::diff_emitter::pending_batches_for_test(left.id());
    assert_eq!(batches.len(), 2);
    assert_eq!(
        (batches[0].from_sequence, batches[0].sequence, batches[0].total_count),
        (0, 1, 3)
    );
    assert_eq!(
        (batches[1].from_sequence, batches[1].sequence, batches[1].total_count),
        (1, 2, 2)
    );
    assert_eq!(
        (batches[0].changes[0].change_type, batches[0].changes[0].index),
        (super::diff::DiffChangeType::Add, 0)
    );
    assert_eq!(
        (batches[1].changes[0].change_type, batches[1].changes[0].index),
        (super::diff::DiffChangeType::Remove, 1)
    );
    assert_eq!(
        super::operations::get_selection_snapshot(left.id(), false, &[1], 2)
            .unwrap()
            .paths,
        vec![format!("{L}/c")]
    );
}

#[test]
fn identical_raw_replacement_still_publishes_scratch_visibility_drift() {
    use crate::file_system::staging::{ShowTempsGuard, set_show_safe_save_files};
    let _settings = ShowTempsGuard::set_both(false, false);
    let left = pane(
        "scratch-identical-replacement",
        L,
        vec![file(L, "a.sb-temp", 1, 1), file(L, "b", 1, 1)],
    );
    super::diff_emitter::hold_for_test(left.id());
    let raw = left.entries();
    set_show_safe_save_files(true);
    super::caching::publish_replacement(left.id(), raw, 0);
    let batches = super::diff_emitter::pending_batches_for_test(left.id());
    assert_eq!(batches.len(), 1);
    assert_eq!(
        (batches[0].from_sequence, batches[0].sequence, batches[0].total_count),
        (0, 1, 2)
    );
    assert_eq!(batches[0].changes[0].index, 0);
    assert_eq!(
        super::operations::get_selection_snapshot(left.id(), false, &[1], 1)
            .unwrap()
            .paths,
        vec![format!("{L}/b")]
    );
}

#[test]
fn scratch_aba_never_exposes_different_rows_at_the_same_revision() {
    use crate::file_system::staging::{ShowTempsGuard, set_show_safe_save_files};
    let _settings = ShowTempsGuard::set_both(false, false);
    let left = pane("scratch-aba", L, vec![file(L, "a.sb-temp", 1, 1), file(L, "b", 1, 1)]);
    super::diff_emitter::hold_for_test(left.id());
    left.with_listing(|listing| {
        assert!(!listing.scratch_has_drift());
        set_show_safe_save_files(true); // Flip AFTER validation, BEFORE another row read.
        assert_eq!(listing.pane_rows().get(0).unwrap().path, format!("{L}/b"));
        assert_eq!(listing.sequence.load(std::sync::atomic::Ordering::Acquire), 0);
        set_show_safe_save_files(false);
        assert_eq!(listing.pane_rows().len(), 1);
    });
    assert_eq!(
        super::operations::get_selection_snapshot(left.id(), false, &[0], 0)
            .unwrap()
            .paths,
        vec![format!("{L}/b")]
    );
    set_show_safe_save_files(true);
    assert!(super::operations::get_selection_snapshot(left.id(), false, &[0], 0).is_err());
    assert_eq!(
        super::operations::get_selection_snapshot(left.id(), false, &[0], 1)
            .unwrap()
            .paths,
        vec![format!("{L}/a.sb-temp")]
    );
    set_show_safe_save_files(false);
    assert!(super::operations::get_selection_snapshot(left.id(), false, &[0], 1).is_err());
    assert_eq!(
        super::operations::get_selection_snapshot(left.id(), false, &[0], 2)
            .unwrap()
            .paths,
        vec![format!("{L}/b")]
    );
}

#[test]
fn scratch_drift_on_a_dotfile_publishes_the_revision_even_with_no_visible_changes() {
    use crate::file_system::staging::{ShowTempsGuard, set_show_safe_save_files};
    let _settings = ShowTempsGuard::set_both(false, false);
    let mut hidden = file(L, ".a.sb-temp", 1, 1);
    hidden.is_hidden = true;
    let left = pane("scratch-hidden-revision", L, vec![hidden, file(L, "b", 1, 1)]);
    super::diff_emitter::hold_for_test(left.id());
    set_show_safe_save_files(true);
    assert!(matches!(
        super::operations::get_selection_snapshot(left.id(), false, &[0], 0),
        Err(super::operations::ListingLookupError::Changed { .. })
    ));
    let batches = super::diff_emitter::pending_batches_for_test(left.id());
    assert_eq!(batches.len(), 1);
    assert_eq!(
        (batches[0].from_sequence, batches[0].sequence, batches[0].total_count),
        (0, 1, 1)
    );
    assert!(batches[0].changes.is_empty());
    assert_eq!(
        super::operations::get_selection_snapshot(left.id(), false, &[0], 1)
            .unwrap()
            .paths,
        vec![format!("{L}/b")]
    );
}

#[test]
fn marks_files_missing_on_the_other_side_and_the_newer_copy_of_the_rest() {
    let left = pane(
        "compare-newer-l",
        L,
        vec![
            file(L, "only-left.txt", 1, 100),
            file(L, "newer-left.txt", 1, 2_000),
            file(L, "same.txt", 1, 1_000),
            file(L, "older-left.txt", 1, 1_000),
        ],
    );
    let right = pane(
        "compare-newer-r",
        R,
        vec![
            file(R, "only-right.txt", 1, 100),
            file(R, "newer-left.txt", 1, 1_000),
            file(R, "same.txt", 1, 1_000),
            file(R, "older-left.txt", 1, 2_000),
        ],
    );

    let (l, r) = marked(&left, &right, CompareDirectoriesMode::NewerAndMissing);

    assert_eq!(l, vec!["newer-left.txt", "only-left.txt"]);
    assert_eq!(r, vec!["older-left.txt", "only-right.txt"]);
}

#[test]
fn a_two_second_difference_counts_as_the_same_time() {
    // FAT and some network shares store time in 2 s steps, so a copy can read a
    // second or two off its original.
    let left = pane("compare-tolerance-l", L, vec![file(L, "a.txt", 1, 1_002)]);
    let right = pane("compare-tolerance-r", R, vec![file(R, "a.txt", 1, 1_000)]);

    assert_eq!(
        marked(&left, &right, CompareDirectoriesMode::NewerAndMissing),
        (vec![], vec![])
    );
}

#[test]
fn folders_are_left_alone() {
    let left = pane(
        "compare-folders-l",
        L,
        vec![folder(L, "only-left-dir"), folder(L, "both")],
    );
    let right = pane("compare-folders-r", R, vec![folder(R, "both")]);

    assert_eq!(
        marked(&left, &right, CompareDirectoriesMode::NewerAndMissing),
        (vec![], vec![])
    );
}

#[test]
fn a_folder_with_the_files_name_is_not_its_counterpart() {
    let left = pane("compare-kind-l", L, vec![file(L, "report", 1, 100)]);
    let right = pane("compare-kind-r", R, vec![folder(R, "report")]);

    assert_eq!(
        marked(&left, &right, CompareDirectoriesMode::NewerAndMissing).0,
        vec!["report"]
    );
}

#[test]
fn names_match_across_case_and_unicode_form() {
    // Decomposed on one side, composed and upper-cased on the other: one file to a
    // case-insensitive Mac, so neither side counts it missing.
    let left = pane("compare-fold-l", L, vec![file(L, "Cafe\u{301}.txt", 1, 1_000)]);
    let right = pane("compare-fold-r", R, vec![file(R, "CAF\u{c9}.txt", 1, 1_000)]);

    assert_eq!(
        marked(&left, &right, CompareDirectoriesMode::NewerAndMissing),
        (vec![], vec![])
    );
}

#[test]
fn a_name_two_files_share_by_folding_matches_only_exactly_and_both_ways() {
    // A case-sensitive volume holding `Report` and `report`, against `REPORT`:
    // folding can't say which one `REPORT` is, so neither direction pairs them.
    let left = pane(
        "compare-collide-l",
        L,
        vec![file(L, "Report", 1, 1_000), file(L, "report", 1, 1_000)],
    );
    let right = pane("compare-collide-r", R, vec![file(R, "REPORT", 1, 1_000)]);

    let (l, r) = marked(&left, &right, CompareDirectoriesMode::NewerAndMissing);

    assert_eq!(l, vec!["report", "Report"], "both, in the pane's name order");
    assert_eq!(r, vec!["REPORT"]);
}

#[test]
fn missing_mode_marks_only_what_the_other_side_lacks() {
    let left = pane(
        "compare-missing-l",
        L,
        vec![file(L, "only-left.txt", 1, 1), file(L, "newer.txt", 1, 9_000)],
    );
    let right = pane("compare-missing-r", R, vec![file(R, "newer.txt", 1, 1_000)]);

    assert_eq!(
        marked(&left, &right, CompareDirectoriesMode::Missing),
        (vec!["only-left.txt".to_string()], vec![])
    );
}

#[test]
fn size_mode_marks_both_copies_of_a_file_whose_size_differs() {
    let left = pane(
        "compare-size-l",
        L,
        vec![file(L, "grew.txt", 10, 1_000), file(L, "same.txt", 5, 1)],
    );
    let right = pane(
        "compare-size-r",
        R,
        vec![file(R, "grew.txt", 20, 9_000), file(R, "same.txt", 5, 9_000)],
    );

    assert_eq!(
        marked(&left, &right, CompareDirectoriesMode::SizeAndMissing),
        (vec!["grew.txt".to_string()], vec!["grew.txt".to_string()])
    );
}

#[test]
fn an_unknown_time_never_counts_as_newer() {
    let mut unknown = file(L, "a.txt", 1, 0);
    unknown.modified_at = None;
    let left = pane("compare-unknown-l", L, vec![unknown]);
    let right = pane("compare-unknown-r", R, vec![file(R, "a.txt", 1, 1)]);

    assert_eq!(
        marked(&left, &right, CompareDirectoriesMode::NewerAndMissing),
        (vec![], vec![])
    );
}

#[test]
fn compares_only_the_rows_the_panes_show() {
    // With hidden files off, a dotfile is nothing the pane shows, so it's never
    // marked, and its row numbers skip it.
    let left = pane(
        "compare-hidden-l",
        L,
        vec![file(L, ".hidden-only-left", 1, 1), file(L, "zz-only-left.txt", 1, 1)],
    );
    let right = pane("compare-hidden-r", R, vec![]);

    let result = compare_directories(
        left.id(),
        false,
        right.id(),
        false,
        CompareDirectoriesMode::NewerAndMissing,
    )
    .expect("both listings are cached");

    assert_eq!(result.left, vec![0], "row 0 of the pane is zz-only-left.txt");
}

#[test]
fn a_gone_listing_is_an_error_not_an_empty_answer() {
    let left = pane("compare-gone-l", L, vec![]);

    assert!(
        compare_directories(
            left.id(),
            false,
            "no-such-listing",
            false,
            CompareDirectoriesMode::Missing
        )
        .is_err()
    );
}

#[test]
fn the_answer_names_the_state_it_was_read_from() {
    let left = pane("cmp-seq-l", L, vec![file(L, "a.txt", 1, 100)]);
    let right = pane("cmp-seq-r", R, vec![]);
    let quiet = compare_directories(
        left.id(),
        false,
        right.id(),
        false,
        CompareDirectoriesMode::NewerAndMissing,
    )
    .expect("both listings are cached");
    assert!(quiet.settled);
    assert_eq!((quiet.left_sequence, quiet.right_sequence), (0, 0));

    // A change on its way to the left pane: the cache is ahead of what it shows.
    super::diff_emitter::hold_for_test(left.id());
    super::caching::insert_entry_sorted(left.id(), file(L, "b.txt", 1, 100)).unwrap();
    let busy = compare_directories(
        left.id(),
        false,
        right.id(),
        false,
        CompareDirectoriesMode::NewerAndMissing,
    )
    .expect("both listings are cached");
    assert!(
        busy.settled,
        "publication latency does not change the committed revision"
    );
    assert_eq!(busy.left_sequence, 1);

    // Once sent, the sequence moved on and nothing waits.
    flush_now_for_test(left.id());
    let after = compare_directories(
        left.id(),
        false,
        right.id(),
        false,
        CompareDirectoriesMode::NewerAndMissing,
    )
    .expect("both listings are cached");
    assert!(after.settled);
    assert_eq!(after.left_sequence, 1);
}
