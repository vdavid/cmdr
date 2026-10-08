//! The quick filter: what a pattern matches, and that a filtered pane's row
//! space is the one every reader, every carried cursor and selection, and every
//! `directory-diff` speaks.

use super::cached_listing::OverlayRows;
use super::caching::notify_added;
use super::caching_test_support::{TestListing, TestListingGuard};
use super::diff::{DiffChangeType, compute_diff};
use super::diff_emitter::{hold_for_test, pending_changes_for_test, take_batches_for_test};
use super::metadata::FileEntry;
use super::name_filter::{NameFilter, set_listing_name_filter};
use super::operations::{find_file_index, get_file_at, get_total_count, update_listing_entries};
use super::sorting::{DirectorySortMode, SortColumn, SortOrder, sort_entries};

const DIR: &str = "/test/name-filter";

fn matches(pattern: &str, name: &str) -> bool {
    NameFilter::new(pattern)
        .expect("a pattern with something to filter by")
        .matches(name)
}

#[test]
fn a_plain_pattern_matches_anywhere_in_the_name_ignoring_case() {
    assert!(matches("rep", "Annual report.pdf"));
    assert!(matches("REP", "annual report.pdf"));
    assert!(matches("annual", "Annual report.pdf"));
    assert!(matches(".pdf", "Annual report.pdf"));
    assert!(!matches("xls", "Annual report.pdf"));
}

#[test]
fn a_pattern_ignores_unicode_form() {
    // Decomposed "é" (e + U+0301) in the name, composed in the pattern, and back.
    assert!(matches("caf\u{e9}", "Cafe\u{301} menu.txt"));
    assert!(matches("cafe\u{301}", "Caf\u{e9} menu.txt"));
    assert!(matches("ŽLUŤ", "žluťoučký kůň.txt"));
}

#[test]
fn wildcards_stand_for_any_run_and_any_one_character() {
    assert!(matches("*.pdf", "report.pdf"));
    assert!(matches("r*t", "Annual report.pdf"));
    assert!(matches("rep?rt", "report.pdf"));
    assert!(!matches("rep?rt", "repoort.pdf"));
    assert!(matches("a*b*c", "xxaYYbZZcxx"));
    assert!(!matches("a*b*c", "xxaYYcZZbxx"));
    assert!(matches("?", "x"));
    assert!(!matches("??", "x"));
}

#[test]
fn a_pattern_with_nothing_to_filter_by_is_no_filter() {
    assert_eq!(NameFilter::new(""), None);
    assert_eq!(NameFilter::new("*"), None);
    assert_eq!(NameFilter::new("***"), None);
}

fn entry(name: &str) -> FileEntry {
    FileEntry::new(name.to_string(), format!("{DIR}/{name}"), false, false)
}

fn sorted(mut entries: Vec<FileEntry>) -> Vec<FileEntry> {
    sort_entries(
        &mut entries,
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
    );
    entries
}

fn entries() -> Vec<FileEntry> {
    sorted(vec![
        entry(".zshrc"),
        entry("alpha.pdf"),
        entry("bravo.txt"),
        entry("charlie.pdf"),
        entry("delta.txt"),
    ])
}

fn pane(tag: &str) -> TestListingGuard {
    let listing = TestListing::new()
        .path(DIR)
        .include_hidden(false)
        .entries(entries())
        .insert(tag);
    hold_for_test(listing.id());
    listing
}

fn row_names(listing: &TestListingGuard) -> Vec<String> {
    let count = get_total_count(listing.id(), false).expect("listing is cached");
    (0..count)
        .map(|row| {
            get_file_at(listing.id(), row, false)
                .expect("listing is cached")
                .expect("row is within the count")
                .name
        })
        .collect()
}

#[test]
fn a_filtered_pane_shows_only_the_rows_that_match() {
    let listing = pane("name-filter-rows");

    let result =
        set_listing_name_filter(listing.id(), Some("pdf"), false, None, &[], false).expect("listing is cached");

    assert_eq!(result.total_count, 2);
    assert_eq!(row_names(&listing), vec!["alpha.pdf", "charlie.pdf"]);
    assert_eq!(
        find_file_index(listing.id(), "charlie.pdf", false).ok().flatten(),
        Some(1)
    );
    assert_eq!(find_file_index(listing.id(), "bravo.txt", false).ok().flatten(), None);
}

#[test]
fn clearing_the_filter_shows_every_row_again() {
    let listing = pane("name-filter-clear");
    set_listing_name_filter(listing.id(), Some("pdf"), false, None, &[], false).expect("listing is cached");

    let result = set_listing_name_filter(listing.id(), None, false, None, &[], false).expect("listing is cached");

    assert_eq!(result.total_count, 4, "hidden files stay hidden");
    assert_eq!(
        row_names(&listing),
        vec!["alpha.pdf", "bravo.txt", "charlie.pdf", "delta.txt"]
    );
}

#[test]
fn the_filter_never_shows_a_hidden_file_the_pane_hides() {
    let listing = pane("name-filter-hidden");

    let result =
        set_listing_name_filter(listing.id(), Some("zsh"), false, None, &[], false).expect("listing is cached");

    assert_eq!(result.total_count, 0);
}

#[test]
fn the_cursor_and_selection_ride_along_into_the_filtered_rows() {
    let listing = pane("name-filter-carry");
    // Rows: alpha.pdf 0, bravo.txt 1, charlie.pdf 2, delta.txt 3.
    let result = set_listing_name_filter(listing.id(), Some("pdf"), false, Some("charlie.pdf"), &[1, 2], false)
        .expect("listing is cached");

    assert_eq!(result.new_cursor_index, Some(1));
    assert_eq!(
        result.new_selected_indices,
        vec![1],
        "bravo.txt drops out: no operation acts on a row the user can't see"
    );
}

#[test]
fn a_cursor_on_a_row_the_filter_leaves_out_has_nowhere_to_go() {
    let listing = pane("name-filter-cursor-out");

    let result = set_listing_name_filter(listing.id(), Some("pdf"), false, Some("bravo.txt"), &[], false)
        .expect("listing is cached");

    assert_eq!(result.new_cursor_index, None);
}

#[test]
fn a_filter_change_discards_old_rows_at_the_real_flush_boundary() {
    let listing = pane("name-filter-drop");
    notify_added(listing.id(), entry("echo.txt"));
    assert_eq!(pending_changes_for_test(listing.id()).len(), 1);

    set_listing_name_filter(listing.id(), Some("pdf"), false, None, &[], false).expect("listing is cached");

    assert!(take_batches_for_test(listing.id()).is_empty());
}

#[test]
fn a_new_epoch_enqueue_between_filter_unlock_and_return_survives_flush() {
    let listing = pane("name-filter-new-epoch-pending-race");
    notify_added(listing.id(), entry("echo.txt"));

    let result = super::name_filter::set_listing_name_filter_inner(
        listing.id(),
        Some("pdf"),
        false,
        None,
        &[],
        false,
        None,
        || {
            // This callback runs after the real cache write lock is released,
            // exactly where a watcher can enqueue before the setter returns.
            hold_for_test(listing.id());
            notify_added(listing.id(), entry("bravo.pdf"));
            assert_eq!(pending_changes_for_test(listing.id()).len(), 1);
        },
    )
    .expect("listing is cached");

    let batches = take_batches_for_test(listing.id());
    assert_eq!(batches.len(), 1);
    let diff = &batches[0];
    assert_eq!(diff.sequence, result.sequence.unwrap() + 1);
    assert_eq!(diff.changes.len(), 1, "old-epoch rows are discarded at flush");
    let change = &diff.changes[0];
    assert_eq!(change.change_type, DiffChangeType::Add);
    assert_eq!(change.entry.name, "bravo.pdf");
    assert_eq!(change.index, 1);
    assert!(pending_changes_for_test(listing.id()).is_empty());
}

#[test]
fn a_new_file_the_filter_leaves_out_is_nothing_the_pane_hears_about() {
    let listing = pane("name-filter-diff");
    set_listing_name_filter(listing.id(), Some("pdf"), false, None, &[], false).expect("listing is cached");
    hold_for_test(listing.id());

    notify_added(listing.id(), entry("echo.txt"));
    assert!(pending_changes_for_test(listing.id()).is_empty());

    notify_added(listing.id(), entry("bravo.pdf"));
    let queued: Vec<(DiffChangeType, String, usize)> = pending_changes_for_test(listing.id())
        .into_iter()
        .map(|c| (c.change_type, c.entry.name, c.index))
        .collect();
    assert_eq!(queued, vec![(DiffChangeType::Add, "bravo.pdf".to_string(), 1)]);
}

#[test]
fn a_batch_diff_speaks_the_filtered_rows() {
    let filter = NameFilter::new("pdf");
    let old = entries();
    let new = sorted(vec![
        entry("alpha.pdf"),
        entry("bravo.pdf"),
        entry("delta.txt"),
        entry("echo.txt"),
    ]);

    let changes: Vec<(DiffChangeType, String, usize)> = compute_diff(&old, &new, false, filter.as_ref())
        .into_iter()
        .map(|c| (c.change_type, c.entry.name, c.index))
        .collect();

    assert_eq!(
        changes,
        vec![
            (DiffChangeType::Add, "bravo.pdf".to_string(), 1),
            (DiffChangeType::Remove, "charlie.pdf".to_string(), 1),
        ],
        "delta.txt and echo.txt are nothing the filtered pane shows"
    );
}

/// The user's own example: typing only ever narrows down to the last match.
fn one_onet_onets(tag: &str) -> TestListingGuard {
    let listing = TestListing::new()
        .path(DIR)
        .include_hidden(false)
        .entries(sorted(vec![entry("one.txt"), entry("onet.txt"), entry("onets.txt")]))
        .insert(tag);
    hold_for_test(listing.id());
    listing
}

fn type_pattern(listing: &TestListingGuard, pattern: &str) -> super::name_filter::NameFilterResult {
    set_listing_name_filter(listing.id(), Some(pattern), false, None, &[], true).expect("listing is cached")
}

#[test]
fn typing_narrows_down_to_the_last_match_and_refuses_to_go_past_it() {
    let listing = one_onet_onets("name-filter-refuse");

    let one = type_pattern(&listing, "one");
    assert!(one.accepted);
    assert_eq!(one.total_count, 3);

    let ts = type_pattern(&listing, "ts");
    assert!(ts.accepted);
    assert_eq!(row_names(&listing), vec!["onets.txt"]);

    let tst = type_pattern(&listing, "tst");
    assert!(!tst.accepted, "the last `t` matches nothing, so it's refused");
    assert_eq!(tst.total_count, 1);
    assert_eq!(row_names(&listing), vec!["onets.txt"], "the previous filter stays");
}

#[test]
fn wildcards_narrow_and_refuse_the_same_way() {
    let listing = one_onet_onets("name-filter-refuse-wild");

    assert_eq!(
        type_pattern(&listing, "o*t").total_count,
        3,
        "`.txt` ends every name in a t"
    );
    assert_eq!(type_pattern(&listing, "one?s").total_count, 1);
    assert!(!type_pattern(&listing, "one??z").accepted);
    assert_eq!(row_names(&listing), vec!["onets.txt"]);
}

#[test]
fn a_refused_pattern_keeps_the_cursor_and_selection_where_they_were() {
    let listing = one_onet_onets("name-filter-refuse-carry");
    type_pattern(&listing, "onet");
    // Rows now: onet.txt 0, onets.txt 1.
    let result = set_listing_name_filter(listing.id(), Some("onetx"), false, Some("onets.txt"), &[0], true)
        .expect("listing is cached");

    assert!(!result.accepted);
    assert_eq!(result.new_cursor_index, Some(1));
    assert_eq!(result.new_selected_indices, vec![0]);
}

#[test]
fn a_refusal_looks_past_the_rows_the_current_filter_hides() {
    let listing = one_onet_onets("name-filter-refuse-wider");
    type_pattern(&listing, "onets");

    let wider = type_pattern(&listing, "one.");
    assert!(wider.accepted, "one.txt matches, though the current filter hides it");
    assert_eq!(row_names(&listing), vec!["one.txt"]);
}

#[test]
fn a_change_read_in_the_old_row_space_is_never_sent_after_the_filter_changed() {
    let listing = pane("name-filter-race");
    notify_added(listing.id(), entry("echo.txt"));
    set_listing_name_filter(listing.id(), Some("pdf"), false, None, &[], false).expect("listing is cached");
    assert!(take_batches_for_test(listing.id()).is_empty());
    hold_for_test(listing.id());
    notify_added(listing.id(), entry("bravo.pdf"));
    let sent = take_batches_for_test(listing.id());
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].changes[0].index, 1);
    assert_eq!(sent[0].from_sequence, 2);
    assert_eq!(sent[0].sequence, 3);
}

#[test]
fn a_filter_change_names_the_sequence_its_rows_start_at() {
    let listing = pane("name-filter-seq");
    let changed =
        set_listing_name_filter(listing.id(), Some("pdf"), false, None, &[], false).expect("listing is cached");
    assert_eq!(changed.sequence, Some(1));
    let same = set_listing_name_filter(listing.id(), Some("pdf"), false, None, &[], false).expect("listing is cached");
    assert_eq!(same.sequence, None, "nothing changed, so the pane keeps its sequence");
}

#[test]
fn a_re_read_that_crossed_a_filter_change_is_diffed_again_in_the_new_rows() {
    let listing = pane("name-filter-reread");
    // A re-read diffed with no filter (epoch 0): echo.txt lands as row 4.
    let mut fresh = entries();
    fresh.push(entry("echo.pdf"));
    let fresh = sorted(fresh);
    set_listing_name_filter(listing.id(), Some("pdf"), false, None, &[], false).expect("listing is cached");
    hold_for_test(listing.id());
    update_listing_entries(listing.id(), fresh, OverlayRows::Unchanged);
    let sent = take_batches_for_test(listing.id())
        .into_iter()
        .flat_map(|batch| batch.changes);

    let rows: Vec<(DiffChangeType, String, usize)> = sent.map(|c| (c.change_type, c.entry.name, c.index)).collect();
    // In the "pdf" rows (alpha.pdf, charlie.pdf) the new file is row 2.
    assert_eq!(rows, vec![(DiffChangeType::Add, "echo.pdf".to_string(), 2)]);
}

#[test]
fn a_filter_change_invalidates_guarded_selection_from_the_previous_rows() {
    use super::operations::{ListingLookupError, get_selection_snapshot};
    let listing = pane("name-filter-selection-revision");
    set_listing_name_filter(listing.id(), Some("pdf"), false, None, &[], false).unwrap();
    assert!(matches!(
        get_selection_snapshot(listing.id(), false, &[1], 0),
        Err(ListingLookupError::Changed { .. })
    ));
    let snapshot = get_selection_snapshot(listing.id(), false, &[1], 1).unwrap();
    assert_eq!(snapshot.paths, vec![entry("charlie.pdf").path]);
    assert!(matches!(
        super::name_filter::set_listing_name_filter_guarded(listing.id(), None, false, None, &[1], false, Some(0)),
        Err(ListingLookupError::Changed { .. })
    ));
    assert_eq!(row_names(&listing), vec!["alpha.pdf", "charlie.pdf"]);
}

#[test]
fn scratch_drift_is_reconciled_before_a_guarded_filter_consumes_selection() {
    use super::operations::ListingLookupError;
    use crate::file_system::staging::{ShowTempsGuard, StagingTemp};
    let _settings = ShowTempsGuard::set(false);
    let owner = std::sync::Arc::new(());
    let temp = StagingTemp::mint(
        &std::path::Path::new(DIR).join("a.pdf"),
        Some(std::sync::Arc::downgrade(&owner)),
    );
    let name = temp.path().file_name().unwrap().to_str().unwrap();
    let listing = TestListing::new()
        .path(DIR)
        .include_hidden(false)
        .entries(sorted(vec![entry(name), entry("z.pdf")]))
        .insert("name-filter-scratch-guard");
    hold_for_test(listing.id());
    assert_eq!(row_names(&listing), vec!["z.pdf"]);
    drop(owner);
    assert!(matches!(
        super::name_filter::set_listing_name_filter_guarded(listing.id(), Some("z"), false, None, &[0], false, Some(0)),
        Err(ListingLookupError::Changed { .. })
    ));
    let result =
        super::name_filter::set_listing_name_filter_guarded(listing.id(), Some("z"), false, None, &[1], false, Some(1))
            .unwrap();
    assert_eq!(result.new_selected_indices, vec![0]);
    assert_eq!(result.sequence, Some(2));
    assert_eq!(row_names(&listing), vec!["z.pdf"]);
}
