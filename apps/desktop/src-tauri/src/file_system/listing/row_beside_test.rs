//! Tests for `get_file_beside`: reading the row next to a row you know by name.

use super::FileEntry;
use super::caching_test_support::{TestListing, TestListingGuard};
use super::operations::{RowBeside, get_file_beside};

fn make_entry(name: &str) -> FileEntry {
    FileEntry::new(name.to_string(), format!("/dir/{}", name), false, false)
}

fn listing(tag: &str, names: &[&str]) -> TestListingGuard {
    TestListing::new()
        .volume("test")
        .path("/dir")
        .entries(names.iter().copied().map(make_entry).collect())
        .insert(tag)
}

fn beside(guard: &TestListingGuard, name: &str, side: RowBeside, include_hidden: bool) -> Option<String> {
    get_file_beside(guard.id(), name, side, include_hidden)
        .unwrap()
        .map(|entry| entry.name)
}

#[test]
fn reads_the_row_on_either_side_of_the_named_one() {
    let listing = listing("beside-both-sides", &["a.txt", "b.txt", "c.txt"]);

    assert_eq!(
        beside(&listing, "b.txt", RowBeside::Next, true).as_deref(),
        Some("c.txt")
    );
    assert_eq!(
        beside(&listing, "b.txt", RowBeside::Previous, true).as_deref(),
        Some("a.txt")
    );
}

#[test]
fn answers_nothing_at_either_end_of_the_listing() {
    let listing = listing("beside-ends", &["a.txt", "b.txt"]);

    assert_eq!(beside(&listing, "a.txt", RowBeside::Previous, true), None);
    assert_eq!(beside(&listing, "b.txt", RowBeside::Next, true), None);
}

#[test]
fn answers_nothing_for_a_name_the_listing_no_longer_holds() {
    // The caller's anchor was renamed or deleted under it: there is no row beside
    // a row that isn't there, and guessing one is what this call exists to avoid.
    let listing = listing("beside-missing-anchor", &["a.txt", "b.txt"]);

    assert_eq!(beside(&listing, "gone.txt", RowBeside::Next, true), None);
    assert_eq!(beside(&listing, "gone.txt", RowBeside::Previous, true), None);
}

#[test]
fn steps_over_a_hidden_row_the_pane_is_not_showing() {
    let listing = listing("beside-hidden", &[".hidden.txt", "a.txt", "b.txt"]);

    assert_eq!(
        beside(&listing, "a.txt", RowBeside::Previous, false),
        None,
        "with hidden files off, a.txt is the first row"
    );
    assert_eq!(
        beside(&listing, "a.txt", RowBeside::Previous, true).as_deref(),
        Some(".hidden.txt")
    );
}
