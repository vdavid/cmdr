//! A `directory-diff` speaks the rows of the pane showing its listing.
//!
//! A pane numbers its rows over the entries it SHOWS, so with hidden files off
//! a change to a hidden entry is nothing the pane can draw, and a change to a
//! visible one sits at its row, not at its position among all entries. On a pane
//! on `~` the first half is the idle cost: every dotfile write used to refetch
//! the visible rows. The second half is the cursor: an add reported at its entry
//! index slid the cursor a row off whenever hidden entries sat above it.
//!
//! Every test holds the listing's flush (`hold_for_test`), so "nothing queued"
//! is an answer, not a race lost to the 50 ms timer.

use super::caching::{apply_tags_to_listing, notify_added, notify_removed, publish_replacement};
use super::caching_test_support::{TestListing, TestListingGuard};
use super::diff::{DiffChange, DiffChangeType};
use super::diff_emitter::{hold_for_test, pending_changes_for_test};
use super::metadata::{FileEntry, TagRef};
use super::operations::{find_file_index, set_listing_include_hidden};
use super::sorting::{DirectorySortMode, SortColumn, SortOrder, sort_entries};

const DIR: &str = "/test/pane-diff";

fn entry(name: &str, size: u64) -> FileEntry {
    FileEntry {
        size: Some(size),
        permissions: 0o644,
        extended_metadata_loaded: true,
        ..FileEntry::new(name.to_string(), format!("{DIR}/{name}"), false, false)
    }
}

/// A macOS `UF_HIDDEN` entry the way the local volume builds one: hidden, no dot.
fn flagged_hidden(name: &str, size: u64) -> FileEntry {
    FileEntry {
        is_hidden: true,
        ..entry(name, size)
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

/// Dotfiles and a flagged `Library` around three visible files, the shape of `~`.
fn home_entries() -> Vec<FileEntry> {
    sorted(vec![
        entry(".DS_Store", 6),
        entry(".zshrc", 10),
        flagged_hidden("Library", 20),
        entry("alpha.txt", 1),
        entry("charlie.txt", 3),
        entry("delta.txt", 4),
    ])
}

fn pane(tag: &str, include_hidden: bool, entries: Vec<FileEntry>) -> TestListingGuard {
    let listing = TestListing::new()
        .path(DIR)
        .include_hidden(include_hidden)
        .entries(entries)
        .insert(tag);
    hold_for_test(listing.id());
    listing
}

fn queued(listing: &TestListingGuard) -> Vec<(DiffChangeType, String, usize)> {
    pending_changes_for_test(listing.id())
        .into_iter()
        .map(|c: DiffChange| (c.change_type, c.entry.name, c.index))
        .collect()
}

fn row(listing: &TestListingGuard, name: &str, include_hidden: bool) -> usize {
    find_file_index(listing.id(), name, include_hidden)
        .expect("listing is cached")
        .unwrap_or_else(|| panic!("`{name}` is not a row of this pane"))
}

fn entry_index(listing: &TestListingGuard, name: &str) -> usize {
    listing
        .entry_names()
        .iter()
        .position(|n| n == name)
        .unwrap_or_else(|| panic!("`{name}` is not in the listing"))
}

fn cached(listing: &TestListingGuard, name: &str) -> FileEntry {
    listing
        .entries()
        .into_iter()
        .find(|e| e.name == name)
        .unwrap_or_else(|| panic!("`{name}` is not in the listing"))
}

/// The idle case: a dotfile rewritten in `~` while the pane hides dotfiles.
#[test]
fn a_dotfile_write_reaches_the_cache_but_not_a_pane_hiding_it() {
    let listing = pane("pane-diff-dotfile-off", false, home_entries());

    let mut rewritten = home_entries();
    rewritten.iter_mut().find(|e| e.name == ".zshrc").expect("fixture").size = Some(11);
    publish_replacement(listing.id(), rewritten, 0);

    assert_eq!(queued(&listing), vec![], "nothing the pane shows changed");
    assert_eq!(
        cached(&listing, ".zshrc").size,
        Some(11),
        "the listing still holds the new size, ready for when hidden files come on"
    );
}

#[test]
fn a_dotfile_write_reaches_a_pane_showing_hidden_files() {
    let listing = pane("pane-diff-dotfile-on", true, home_entries());

    let mut rewritten = home_entries();
    rewritten.iter_mut().find(|e| e.name == ".zshrc").expect("fixture").size = Some(11);
    publish_replacement(listing.id(), rewritten, 0);

    assert_eq!(
        queued(&listing),
        vec![(
            DiffChangeType::Modify,
            ".zshrc".to_string(),
            row(&listing, ".zshrc", true)
        )]
    );
}

/// `~/Library` is hidden by a flag, not a name. A size change to it is as
/// invisible to a pane hiding hidden files as a dotfile's is.
#[test]
fn a_flag_hidden_entry_changing_is_hidden_too() {
    let listing = pane("pane-diff-flagged", false, home_entries());

    let mut rewritten = home_entries();
    rewritten
        .iter_mut()
        .find(|e| e.name == "Library")
        .expect("fixture")
        .size = Some(99);
    publish_replacement(listing.id(), rewritten, 0);

    assert_eq!(queued(&listing), vec![]);
    assert_eq!(cached(&listing, "Library").size, Some(99));
}

/// Hidden and visible changes in one re-read: the pane hears exactly the visible
/// one, at its ROW. The entry index would be two or three rows further down.
#[test]
fn a_mixed_change_reaches_the_pane_as_only_its_visible_part_at_the_pane_row() {
    let listing = pane("pane-diff-mixed", false, home_entries());

    let mut rewritten = home_entries();
    rewritten.iter_mut().find(|e| e.name == ".zshrc").expect("fixture").size = Some(11);
    rewritten.push(entry(".viminfo", 5));
    rewritten.push(entry("bravo.txt", 2));
    publish_replacement(listing.id(), sorted(rewritten), 0);

    let bravo_row = row(&listing, "bravo.txt", false);
    assert_ne!(
        bravo_row,
        entry_index(&listing, "bravo.txt"),
        "the fixture must put hidden entries above `bravo.txt`, or this can't tell rows from entries"
    );
    assert_eq!(
        queued(&listing),
        vec![(DiffChangeType::Add, "bravo.txt".to_string(), bravo_row)]
    );
}

/// The single-entry path the SMB and MTP watchers and Cmdr's own writes take.
#[test]
fn a_single_add_reports_the_pane_row_and_a_hidden_one_reports_nothing() {
    let listing = pane("pane-diff-single-add", false, home_entries());

    notify_added(listing.id(), entry(".lesshst", 7));
    notify_removed(listing.id(), std::path::Path::new(&format!("{DIR}/.DS_Store")));
    notify_added(listing.id(), entry("bravo.txt", 2));

    let bravo_row = row(&listing, "bravo.txt", false);
    assert_ne!(bravo_row, entry_index(&listing, "bravo.txt"));
    assert_eq!(
        queued(&listing),
        vec![(DiffChangeType::Add, "bravo.txt".to_string(), bravo_row)]
    );
    assert!(listing.entry_names().contains(&".lesshst".to_string()));
    assert!(!listing.entry_names().contains(&".DS_Store".to_string()));
}

/// A removal is reported at the row it LEFT, in the pane before the change.
#[test]
fn a_removal_reports_the_row_it_left() {
    let listing = pane("pane-diff-remove", false, home_entries());
    let charlie_row = row(&listing, "charlie.txt", false);

    notify_removed(listing.id(), std::path::Path::new(&format!("{DIR}/charlie.txt")));

    assert_eq!(
        queued(&listing),
        vec![(DiffChangeType::Remove, "charlie.txt".to_string(), charlie_row)]
    );
}

/// `chflags hidden` on a file the pane shows: to a pane hiding hidden files the
/// row is gone, so the cursor and selection must hear a removal.
#[test]
fn a_visible_entry_turning_hidden_leaves_a_pane_hiding_hidden_files() {
    let listing = pane("pane-diff-turns-hidden-off", false, home_entries());
    let alpha_row = row(&listing, "alpha.txt", false);

    let mut rewritten = home_entries();
    rewritten
        .iter_mut()
        .find(|e| e.name == "alpha.txt")
        .expect("fixture")
        .is_hidden = true;
    publish_replacement(listing.id(), rewritten, 0);

    assert_eq!(
        queued(&listing),
        vec![(DiffChangeType::Remove, "alpha.txt".to_string(), alpha_row)]
    );
    assert!(cached(&listing, "alpha.txt").is_hidden, "the cache holds the new flag");
}

/// The same flip with hidden files shown keeps the row but dims it: a modify.
#[test]
fn a_visible_entry_turning_hidden_is_a_modify_to_a_pane_showing_hidden_files() {
    let listing = pane("pane-diff-turns-hidden-on", true, home_entries());

    let mut rewritten = home_entries();
    rewritten
        .iter_mut()
        .find(|e| e.name == "alpha.txt")
        .expect("fixture")
        .is_hidden = true;
    publish_replacement(listing.id(), rewritten, 0);

    assert_eq!(
        queued(&listing),
        vec![(
            DiffChangeType::Modify,
            "alpha.txt".to_string(),
            row(&listing, "alpha.txt", true)
        )]
    );
}

/// And the other way: a hidden entry un-hidden appears in the pane.
#[test]
fn a_hidden_entry_turning_visible_arrives_as_an_add() {
    let listing = pane("pane-diff-turns-visible", false, home_entries());

    let mut rewritten = home_entries();
    rewritten
        .iter_mut()
        .find(|e| e.name == "Library")
        .expect("fixture")
        .is_hidden = false;
    publish_replacement(listing.id(), rewritten, 0);

    assert_eq!(
        queued(&listing),
        vec![(
            DiffChangeType::Add,
            "Library".to_string(),
            row(&listing, "Library", false)
        )]
    );
}

#[test]
fn tags_on_a_hidden_entry_reach_nothing_the_pane_shows() {
    let listing = pane("pane-diff-tags", false, home_entries());
    let red = vec![TagRef {
        name: "Red".to_string(),
        color: 6,
    }];

    apply_tags_to_listing(listing.id(), vec![(format!("{DIR}/.zshrc"), red.clone())]);
    assert_eq!(queued(&listing), vec![]);
    assert_eq!(cached(&listing, ".zshrc").tags, red);

    apply_tags_to_listing(listing.id(), vec![(format!("{DIR}/delta.txt"), red)]);
    assert_eq!(
        queued(&listing),
        vec![(
            DiffChangeType::Modify,
            "delta.txt".to_string(),
            row(&listing, "delta.txt", false)
        )]
    );
}

/// Turning hidden files on switches the row space. What's queued was numbered in
/// the old one, and the pane re-reads its rows after the toggle anyway, so it goes.
#[test]
fn toggling_hidden_files_switches_the_row_space_and_drops_what_was_queued() {
    let listing = pane("pane-diff-toggle", false, home_entries());

    notify_added(listing.id(), entry("bravo.txt", 2));
    assert_eq!(queued(&listing).len(), 1);

    set_listing_include_hidden(listing.id(), true, None, None, None, false).expect("listing is cached");
    assert_eq!(queued(&listing), vec![], "the old row space's change is dropped");

    hold_for_test(listing.id());
    notify_added(listing.id(), entry(".lesshst", 7));
    assert_eq!(
        queued(&listing),
        vec![(
            DiffChangeType::Add,
            ".lesshst".to_string(),
            row(&listing, ".lesshst", true)
        )]
    );
}
