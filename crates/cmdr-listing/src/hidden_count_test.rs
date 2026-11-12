//! A pane that shows no rows hears how many hidden items it hides, and keeps
//! hearing it as dotfiles come and go.
//!
//! The count rides every answer that carries a row count (`listing-complete`, a
//! `directory-diff` batch, the hidden-files toggle, a resort, the quick filter),
//! and is counted only while the pane shows no rows (`CachedListing::hidden_count`).
//! The staleness half: a change to a hidden entry moves no row, so it used to
//! publish nothing at all. A folder that looks empty must still hear the count
//! move, and a folder WITH rows must still hear nothing (the idle cost on `~`).
//!
//! Every diff test holds the listing's flush, so "nothing queued" is an answer,
//! not a race lost to the 50 ms timer.

use std::path::Path;
use std::sync::Arc;

use cmdr_fs::entry::FileEntry;
use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::testing::unique_test_id;
use cmdr_fs::volume::{InMemoryVolume, Volume};

use super::diff::DirectoryDiffBatch;
use super::sorting_test_support::sort_entries;
use super::testing::{TestEngine, TestListing, TestListingGuard, publish_replacement, read_to_end, request};
use cmdr_sort::{DirectorySortMode, SortColumn, SortOrder};

const DIR: &str = "/test/hidden-count";

fn entry(name: &str, size: u64) -> FileEntry {
    FileEntry {
        size: Some(size),
        permissions: 0o644,
        ..FileEntry::new(name.to_string(), format!("{DIR}/{name}"), false, false)
    }
}

fn dir(name: &str) -> FileEntry {
    FileEntry {
        permissions: 0o755,
        ..FileEntry::new(name.to_string(), format!("{DIR}/{name}"), true, false)
    }
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

/// A repo-shaped folder that looks empty: a `.git` folder and a dotfile.
fn looks_empty() -> Vec<FileEntry> {
    sorted(vec![dir(".git"), entry(".gitignore", 10)])
}

/// A pane on `entries` that has heard its count once, the way a real pane has:
/// `hidden-files-resync.ts` calls `set_listing_include_hidden` after every load.
fn pane(tag: &str, include_hidden: bool, entries: Vec<FileEntry>) -> TestListingGuard {
    let listing = TestListing::new()
        .path(DIR)
        .include_hidden(include_hidden)
        .entries(entries)
        .insert(&TestEngine::new(), tag);
    listing
        .engine()
        .inner
        .set_listing_include_hidden(listing.id(), include_hidden, None, None, None, false)
        .expect("the listing is cached");
    listing.engine().inner.diffs.hold(listing.id());
    listing
}

fn batches(listing: &TestListingGuard) -> Vec<DirectoryDiffBatch> {
    listing.engine().inner.diffs.pending_batches(listing.id())
}

/// `(total_count, hidden_count, changes)` per queued batch.
fn counts(listing: &TestListingGuard) -> Vec<(usize, usize, usize)> {
    batches(listing)
        .into_iter()
        .map(|b| (b.total_count, b.hidden_count, b.changes.len()))
        .collect()
}

fn path(name: &str) -> String {
    format!("{DIR}/{name}")
}

// ============================================================================
// listing-complete
// ============================================================================

async fn list(entries: Vec<FileEntry>, include_hidden: bool) -> (TestEngine, TestListingGuard) {
    let volume: Arc<dyn Volume> = Arc::new(InMemoryVolume::with_entries("Hidden", entries));
    let engine = TestEngine::builder().volume("hidden", volume).build();
    let listing = TestListingGuard::adopt(&engine, unique_test_id("hidden-count"));
    let mut asked = request(listing.id(), "hidden", "/");
    asked.include_hidden = include_hidden;
    read_to_end(&engine, &asked).await.expect("the listing lands");
    (engine, listing)
}

fn root_entry(name: &str, is_dir: bool) -> FileEntry {
    FileEntry::new(name.to_string(), format!("/{name}"), is_dir, false)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_folder_of_only_hidden_items_completes_with_their_count() {
    let (engine, listing) = list(vec![root_entry(".git", true), root_entry(".gitignore", false)], false).await;
    assert_eq!(
        engine.events.complete.lock_ignore_poison().as_slice(),
        [(listing.id().to_string(), 0)]
    );
    assert_eq!(
        engine.events.hidden_counts.lock_ignore_poison().as_slice(),
        [(listing.id().to_string(), 2)]
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_folder_with_rows_or_hidden_files_shown_completes_with_no_count() {
    let mixed = vec![root_entry(".git", true), root_entry("readme.md", false)];
    let (engine, listing) = list(mixed, false).await;
    assert_eq!(
        engine.events.hidden_counts.lock_ignore_poison().as_slice(),
        [(listing.id().to_string(), 0)],
        "counted only while the pane shows no rows"
    );

    let (engine, listing) = list(vec![root_entry(".git", true)], true).await;
    assert_eq!(
        engine.events.hidden_counts.lock_ignore_poison().as_slice(),
        [(listing.id().to_string(), 0)],
        "nothing is hidden with hidden files shown"
    );
}

// ============================================================================
// Hidden-only changes in a folder that looks empty
// ============================================================================

#[test]
fn a_dotfile_added_to_a_folder_that_looks_empty_publishes_the_new_count() {
    let listing = pane("hidden-count-add", false, looks_empty());

    listing.engine().inner.notify_added(listing.id(), entry(".env", 3));

    assert_eq!(counts(&listing), vec![(0, 3, 0)], "an empty batch carrying the count");
}

#[test]
fn a_dotfile_removed_from_a_folder_that_looks_empty_publishes_the_new_count() {
    let listing = pane("hidden-count-remove", false, looks_empty());

    listing
        .engine()
        .inner
        .notify_removed(listing.id(), Path::new(&path(".gitignore")));

    assert_eq!(counts(&listing), vec![(0, 1, 0)]);
}

#[test]
fn the_last_hidden_item_going_leaves_a_truly_empty_folder() {
    let listing = pane("hidden-count-last", false, sorted(vec![entry(".DS_Store", 6)]));

    listing
        .engine()
        .inner
        .notify_removed(listing.id(), Path::new(&path(".DS_Store")));

    assert_eq!(counts(&listing), vec![(0, 0, 0)]);
}

#[test]
fn a_dotfile_rewritten_in_a_folder_that_looks_empty_publishes_nothing() {
    let listing = pane("hidden-count-modify", false, looks_empty());

    listing
        .engine()
        .inner
        .notify_added(listing.id(), entry(".gitignore", 11));

    assert_eq!(counts(&listing), vec![], "the count didn't move");
}

#[test]
fn a_full_reread_adding_a_dotfile_to_a_folder_that_looks_empty_publishes_the_new_count() {
    let listing = pane("hidden-count-reread", false, looks_empty());

    let mut reread = looks_empty();
    reread.push(entry(".env", 3));
    publish_replacement(listing.engine(), listing.id(), sorted(reread), 0);

    assert_eq!(counts(&listing), vec![(0, 3, 0)]);
}

/// The idle-cost guard: on `~`, a dotfile coming or going must stay silent.
#[test]
fn a_dotfile_added_to_a_folder_with_rows_publishes_nothing() {
    let listing = pane(
        "hidden-count-has-rows",
        false,
        sorted(vec![entry(".zshrc", 1), entry("notes.txt", 2)]),
    );

    listing.engine().inner.notify_added(listing.id(), entry(".lesshst", 3));
    listing
        .engine()
        .inner
        .notify_removed(listing.id(), Path::new(&path(".zshrc")));

    assert_eq!(counts(&listing), vec![]);
}

#[test]
fn a_dotfile_added_with_hidden_files_shown_is_an_ordinary_add() {
    let listing = pane("hidden-count-shown", true, looks_empty());

    listing.engine().inner.notify_added(listing.id(), entry(".env", 3));

    assert_eq!(counts(&listing), vec![(3, 0, 1)]);
}

#[test]
fn the_last_visible_file_going_says_how_many_hidden_items_stay() {
    let mut entries = looks_empty();
    entries.push(entry("notes.txt", 2));
    let listing = pane("hidden-count-last-visible", false, sorted(entries));

    listing
        .engine()
        .inner
        .notify_removed(listing.id(), Path::new(&path("notes.txt")));

    assert_eq!(counts(&listing), vec![(0, 2, 1)]);
}

// ============================================================================
// Answers that carry a row count
// ============================================================================

#[test]
fn the_hidden_files_toggle_answers_the_count_for_its_new_setting() {
    let listing = pane("hidden-count-toggle", false, looks_empty());
    let inner = &listing.engine().inner;

    let shown = inner
        .set_listing_include_hidden(listing.id(), true, None, None, None, false)
        .expect("cached");
    assert_eq!((shown.total_count, shown.hidden_count), (2, 0));

    let hidden = inner
        .set_listing_include_hidden(listing.id(), false, None, None, None, false)
        .expect("cached");
    assert_eq!((hidden.total_count, hidden.hidden_count), (0, 2));
}

#[test]
fn a_resort_answers_the_count() {
    let listing = pane("hidden-count-resort", false, looks_empty());

    let result = listing
        .engine()
        .inner
        .resort_listing(
            listing.id(),
            SortColumn::Size,
            SortOrder::Descending,
            DirectorySortMode::LikeFiles,
            None,
            false,
            None,
            false,
            None,
        )
        .expect("cached");

    assert_eq!((result.total_count, result.hidden_count), (0, 2));
}

/// A filter that empties the pane counts only the hidden items IT matches: the
/// hint says what revealing hidden files would actually bring into view.
#[test]
fn the_quick_filter_counts_only_the_hidden_items_it_matches() {
    let mut entries = looks_empty();
    entries.push(entry("notes.txt", 2));
    let listing = pane("hidden-count-filter", false, sorted(entries));
    let inner = &listing.engine().inner;

    let git = inner
        .set_listing_name_filter(listing.id(), Some("git"), false, None, &[], false)
        .expect("cached");
    assert_eq!((git.total_count, git.hidden_count), (0, 2));

    let ignore = inner
        .set_listing_name_filter(listing.id(), Some("ignore"), false, None, &[], false)
        .expect("cached");
    assert_eq!((ignore.total_count, ignore.hidden_count), (0, 1));

    let nothing = inner
        .set_listing_name_filter(listing.id(), Some("zzz"), false, None, &[], false)
        .expect("cached");
    assert_eq!((nothing.total_count, nothing.hidden_count), (0, 0), "nothing to reveal");

    let notes = inner
        .set_listing_name_filter(listing.id(), Some("notes"), false, None, &[], false)
        .expect("cached");
    assert_eq!((notes.total_count, notes.hidden_count), (1, 0));
}
