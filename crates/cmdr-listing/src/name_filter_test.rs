//! The quick filter: what a pattern matches, and that a filtered pane's row
//! space is the one every reader, every carried cursor and selection, and every
//! `directory-diff` speaks.

use super::cached_listing::OverlayRows;
use super::diff::{DiffChangeType, compute_diff};
use super::host::ScratchVisibility;
use super::name_filter::NameFilter;
use super::pipeline::Prepared;
use super::sorting_test_support::sort_entries;
use super::testing::{TestEngine, TestListing, TestListingGuard, pending_changes, replace_entries};
use super::{OnNoMatch, PaneMarks};
use cmdr_fs::entry::FileEntry;
use cmdr_sort::{DirectorySortMode, SortColumn, SortOrder};

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
fn a_cyrillic_pattern_ignores_case_and_unicode_form() {
    assert!(matches("отч", "Годовой Отчёт.pdf"));
    assert!(matches("ОТЧЁТ", "годовой отчёт.pdf"));
    // Decomposed "й" (и + U+0306) and "ё" (е + U+0308) in the name, composed in the pattern.
    assert!(matches("мой", "Мои\u{306} файл.txt"));
    assert!(matches("отчё", "Отче\u{308}т.pdf"));
    assert!(!matches("отчё", "Отчет.pdf"));
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
        .insert(&TestEngine::new(), tag);
    listing.engine().inner.diffs.hold(listing.id());
    listing
}

fn row_names(listing: &TestListingGuard) -> Vec<String> {
    let count = listing
        .engine()
        .inner
        .get_total_count(listing.id(), false)
        .expect("listing is cached");
    (0..count)
        .map(|row| {
            listing
                .engine()
                .inner
                .get_file_at(listing.id(), row, false)
                .expect("listing is cached")
                .expect("row is within the count")
                .name
        })
        .collect()
}

#[test]
fn a_filtered_pane_shows_only_the_rows_that_match() {
    let listing = pane("name-filter-rows");

    let result = listing
        .engine()
        .inner
        .set_listing_name_filter(listing.id(), Some("pdf"), false, None, &[], false)
        .expect("listing is cached");

    assert_eq!(result.total_count, 2);
    assert_eq!(row_names(&listing), vec!["alpha.pdf", "charlie.pdf"]);
    assert_eq!(
        listing
            .engine()
            .inner
            .find_file_index(listing.id(), "charlie.pdf", false)
            .ok()
            .flatten(),
        Some(1)
    );
    assert_eq!(
        listing
            .engine()
            .inner
            .find_file_index(listing.id(), "bravo.txt", false)
            .ok()
            .flatten(),
        None
    );
}

#[test]
fn clearing_the_filter_shows_every_row_again() {
    let listing = pane("name-filter-clear");
    listing
        .engine()
        .inner
        .set_listing_name_filter(listing.id(), Some("pdf"), false, None, &[], false)
        .expect("listing is cached");

    let result = listing
        .engine()
        .inner
        .set_listing_name_filter(listing.id(), None, false, None, &[], false)
        .expect("listing is cached");

    assert_eq!(result.total_count, 4, "hidden files stay hidden");
    assert_eq!(
        row_names(&listing),
        vec!["alpha.pdf", "bravo.txt", "charlie.pdf", "delta.txt"]
    );
}

#[test]
fn the_filter_never_shows_a_hidden_file_the_pane_hides() {
    let listing = pane("name-filter-hidden");

    let result = listing
        .engine()
        .inner
        .set_listing_name_filter(listing.id(), Some("zsh"), false, None, &[], false)
        .expect("listing is cached");

    assert_eq!(result.total_count, 0);
}

#[test]
fn the_cursor_and_selection_ride_along_into_the_filtered_rows() {
    let listing = pane("name-filter-carry");
    // Rows: alpha.pdf 0, bravo.txt 1, charlie.pdf 2, delta.txt 3.
    let result = listing
        .engine()
        .inner
        .set_listing_name_filter(listing.id(), Some("pdf"), false, Some("charlie.pdf"), &[1, 2], false)
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

    let result = listing
        .engine()
        .inner
        .set_listing_name_filter(listing.id(), Some("pdf"), false, Some("bravo.txt"), &[], false)
        .expect("listing is cached");

    assert_eq!(result.new_cursor_index, None);
}

#[test]
fn a_filter_change_discards_old_rows_at_the_real_flush_boundary() {
    let listing = pane("name-filter-drop");
    listing.engine().inner.notify_added(listing.id(), entry("echo.txt"));
    assert_eq!(pending_changes(listing.engine(), listing.id()).len(), 1);

    listing
        .engine()
        .inner
        .set_listing_name_filter(listing.id(), Some("pdf"), false, None, &[], false)
        .expect("listing is cached");

    assert!(listing.engine().inner.diffs.take_pending(listing.id()).is_empty());
}

#[test]
fn a_new_epoch_enqueue_between_filter_unlock_and_return_survives_flush() {
    let listing = pane("name-filter-new-epoch-pending-race");
    listing.engine().inner.notify_added(listing.id(), entry("echo.txt"));

    let result = listing
        .engine()
        .inner
        .set_listing_name_filter_inner(
            listing.id(),
            Some("pdf"),
            false,
            PaneMarks::default(),
            OnNoMatch::Apply,
            None,
            || {
                // This callback runs after the real cache write lock is released,
                // exactly where a watcher can enqueue before the setter returns.
                listing.engine().inner.diffs.hold(listing.id());
                listing.engine().inner.notify_added(listing.id(), entry("bravo.pdf"));
                assert_eq!(pending_changes(listing.engine(), listing.id()).len(), 1);
            },
        )
        .expect("listing is cached");

    let batches = listing.engine().inner.diffs.take_pending(listing.id());
    assert_eq!(batches.len(), 1);
    let diff = &batches[0];
    assert_eq!(diff.sequence, result.sequence.unwrap() + 1);
    assert_eq!(diff.changes.len(), 1, "old-epoch rows are discarded at flush");
    let change = &diff.changes[0];
    assert_eq!(change.change_type, DiffChangeType::Add);
    assert_eq!(change.entry.name, "bravo.pdf");
    assert_eq!(change.index, 1);
    assert!(pending_changes(listing.engine(), listing.id()).is_empty());
}

#[test]
fn a_new_file_the_filter_leaves_out_is_nothing_the_pane_hears_about() {
    let listing = pane("name-filter-diff");
    listing
        .engine()
        .inner
        .set_listing_name_filter(listing.id(), Some("pdf"), false, None, &[], false)
        .expect("listing is cached");
    listing.engine().inner.diffs.hold(listing.id());

    listing.engine().inner.notify_added(listing.id(), entry("echo.txt"));
    assert!(pending_changes(listing.engine(), listing.id()).is_empty());

    listing.engine().inner.notify_added(listing.id(), entry("bravo.pdf"));
    let queued: Vec<(DiffChangeType, String, usize)> = pending_changes(listing.engine(), listing.id())
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

    let changes: Vec<(DiffChangeType, String, usize)> =
        compute_diff(&old, &new, false, filter.as_ref(), ScratchVisibility::default())
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
        .insert(&TestEngine::new(), tag);
    listing.engine().inner.diffs.hold(listing.id());
    listing
}

fn type_pattern(listing: &TestListingGuard, pattern: &str) -> super::quick_filter::NameFilterResult {
    listing
        .engine()
        .inner
        .set_listing_name_filter(listing.id(), Some(pattern), false, None, &[], true)
        .expect("listing is cached")
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
    let result = listing
        .engine()
        .inner
        .set_listing_name_filter(listing.id(), Some("onetx"), false, Some("onets.txt"), &[0], true)
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
    listing.engine().inner.notify_added(listing.id(), entry("echo.txt"));
    listing
        .engine()
        .inner
        .set_listing_name_filter(listing.id(), Some("pdf"), false, None, &[], false)
        .expect("listing is cached");
    assert!(listing.engine().inner.diffs.take_pending(listing.id()).is_empty());
    listing.engine().inner.diffs.hold(listing.id());
    listing.engine().inner.notify_added(listing.id(), entry("bravo.pdf"));
    let sent = listing.engine().inner.diffs.take_pending(listing.id());
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].changes[0].index, 1);
    assert_eq!(sent[0].from_sequence, 2);
    assert_eq!(sent[0].sequence, 3);
}

#[test]
fn a_filter_change_names_the_sequence_its_rows_start_at() {
    let listing = pane("name-filter-seq");
    let changed = listing
        .engine()
        .inner
        .set_listing_name_filter(listing.id(), Some("pdf"), false, None, &[], false)
        .expect("listing is cached");
    assert_eq!(changed.sequence, Some(1));
    let same = listing
        .engine()
        .inner
        .set_listing_name_filter(listing.id(), Some("pdf"), false, None, &[], false)
        .expect("listing is cached");
    assert_eq!(same.sequence, None, "nothing changed, so the pane keeps its sequence");
}

#[test]
fn a_re_read_that_crossed_a_filter_change_is_diffed_again_in_the_new_rows() {
    let listing = pane("name-filter-reread");
    // A re-read diffed with no filter (epoch 0): echo.txt lands as row 4.
    let mut fresh = entries();
    fresh.push(entry("echo.pdf"));
    let fresh = sorted(fresh);
    listing
        .engine()
        .inner
        .set_listing_name_filter(listing.id(), Some("pdf"), false, None, &[], false)
        .expect("listing is cached");
    listing.engine().inner.diffs.hold(listing.id());
    replace_entries(
        listing.engine(),
        listing.id(),
        Prepared::for_test(fresh, OverlayRows::Unchanged),
    );
    let sent = listing
        .engine()
        .inner
        .diffs
        .take_pending(listing.id())
        .into_iter()
        .flat_map(|batch| batch.changes);

    let rows: Vec<(DiffChangeType, String, usize)> = sent.map(|c| (c.change_type, c.entry.name, c.index)).collect();
    // In the "pdf" rows (alpha.pdf, charlie.pdf) the new file is row 2.
    assert_eq!(rows, vec![(DiffChangeType::Add, "echo.pdf".to_string(), 2)]);
}

#[test]
fn a_filter_change_invalidates_guarded_selection_from_the_previous_rows() {
    use super::operations::ListingLookupError;
    let listing = pane("name-filter-selection-revision");
    listing
        .engine()
        .inner
        .set_listing_name_filter(listing.id(), Some("pdf"), false, None, &[], false)
        .unwrap();
    assert!(matches!(
        listing
            .engine()
            .inner
            .get_selection_snapshot(listing.id(), false, &[1], 0),
        Err(ListingLookupError::Changed { .. })
    ));
    let snapshot = listing
        .engine()
        .inner
        .get_selection_snapshot(listing.id(), false, &[1], 1)
        .unwrap();
    assert_eq!(snapshot.paths, vec![entry("charlie.pdf").path]);
    assert!(matches!(
        listing.engine().inner.set_listing_name_filter_guarded(
            listing.id(),
            None,
            false,
            selecting(&[1]),
            OnNoMatch::Apply,
            Some(0)
        ),
        Err(ListingLookupError::Changed { .. })
    ));
    assert_eq!(row_names(&listing), vec!["alpha.pdf", "charlie.pdf"]);
}

#[test]
fn scratch_drift_is_reconciled_before_a_guarded_filter_consumes_selection() {
    use super::operations::ListingLookupError;
    use cmdr_fs::staging::StagingTemp;
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
        .insert(&TestEngine::new(), "name-filter-scratch-guard");
    listing.engine().inner.diffs.hold(listing.id());
    assert_eq!(row_names(&listing), vec!["z.pdf"]);
    drop(owner);
    assert!(matches!(
        listing.engine().inner.set_listing_name_filter_guarded(
            listing.id(),
            Some("z"),
            false,
            selecting(&[0]),
            OnNoMatch::Apply,
            Some(0)
        ),
        Err(ListingLookupError::Changed { .. })
    ));
    let result = listing
        .engine()
        .inner
        .set_listing_name_filter_guarded(
            listing.id(),
            Some("z"),
            false,
            selecting(&[1]),
            OnNoMatch::Apply,
            Some(1),
        )
        .unwrap();
    assert_eq!(result.new_selected_indices, vec![0]);
    assert_eq!(result.sequence, Some(2));
    assert_eq!(row_names(&listing), vec!["z.pdf"]);
}

/// A pane that selected `rows` and has no cursor.
fn selecting(rows: &[usize]) -> PaneMarks<'_> {
    PaneMarks {
        selected: Some(rows),
        ..PaneMarks::default()
    }
}
