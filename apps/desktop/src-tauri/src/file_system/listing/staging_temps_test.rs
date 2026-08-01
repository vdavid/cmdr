//! The listing side of `file_system::staging`: a scratch file a running
//! operation owns stays out of the pane, and everything else doesn't.
//!
//! The 2026-07-31 wedge's visible tail: a 768-file copy to a NAS finished with
//! one file still shown under its `.cmdr-tmp-*` name, because the SMB watcher
//! won the race against the rename and added the temp to the listing after the
//! rename event that would have cleared it.

use super::caching_test_support::{TestListing, TestListingGuard};
use super::metadata::FileEntry;
use super::operations::{find_file_index, get_file_at, get_file_range, get_total_count};
use crate::file_system::staging::{ShowTempsGuard, StagingTemp};
use std::path::Path;
use std::sync::Arc;

fn entry(name: &str) -> FileEntry {
    FileEntry::new(name.to_string(), format!("/{name}"), false, false)
}

/// A listing of one ordinary file plus `temp_name`.
fn listing_with_temp(tag: &str, temp_name: &str) -> TestListingGuard {
    TestListing::new()
        .volume("test")
        .path("/")
        .entries(vec![entry("photo.jpg"), entry(temp_name)])
        .insert(tag)
}

/// A temp a running operation is currently writing. The returned `Arc` IS the
/// operation: drop it and the operation has settled.
fn in_flight_temp(operation: &Arc<()>) -> StagingTemp {
    StagingTemp::mint(Path::new("/photo.jpg"), Some(Arc::downgrade(operation)))
}

fn running_operation() -> Arc<()> {
    Arc::new(())
}

fn name_of(temp: &StagingTemp) -> String {
    temp.path()
        .file_name()
        .expect("a minted temp always has a file name")
        .to_string_lossy()
        .into_owned()
}

/// The bug, at the layer that fixes it: a temp a running copy owns doesn't
/// reach the pane, however it got into the cache.
#[tokio::test]
async fn a_temp_a_running_operation_owns_is_left_out_of_the_listing() {
    let _show = ShowTempsGuard::set(false);
    let op = running_operation();
    let temp = in_flight_temp(&op);
    let name = name_of(&temp);
    let listing = listing_with_temp("listing-temp-hidden", &name);

    assert_eq!(
        get_total_count(listing.id(), true).unwrap(),
        1,
        "only the real file counts"
    );
    let range = get_file_range(listing.id(), 0, 10, true).unwrap();
    assert_eq!(range.len(), 1);
    assert_eq!(range[0].name, "photo.jpg");
}

/// David's first edge case: if the copy wedges and the temps really are left
/// behind, we owe the user the truth about what's on disk.
#[tokio::test]
async fn a_leftover_from_a_finished_operation_is_shown() {
    let _show = ShowTempsGuard::set(false);
    let op = running_operation();
    let temp = in_flight_temp(&op);
    let name = name_of(&temp);
    let listing = listing_with_temp("listing-temp-leftover", &name);

    assert_eq!(get_total_count(listing.id(), true).unwrap(), 1, "hidden while running");
    drop(op);

    // The operation is gone. The guard deliberately isn't dropped: it stands in
    // for the wedged task the driver abandoned without winding down.
    assert_eq!(
        get_total_count(listing.id(), true).unwrap(),
        2,
        "a leftover nobody owns is a real file and must be visible"
    );
    let range = get_file_range(listing.id(), 0, 10, true).unwrap();
    assert!(range.iter().any(|e| e.name == name), "the leftover must be listed");
}

/// David's second edge case, the one that rules out filtering in the watcher: an
/// entry the pane can be handed but never told to drop becomes a permanent ghost
/// pointing at nothing.
///
/// Filtering on the READ path is what makes that impossible, and this pins the
/// property it rests on: every accessor asks the same question, so no accessor
/// can hand out an entry another one denies. An accessor added later that
/// bypasses `visible_entries` fails here.
#[tokio::test]
async fn every_accessor_agrees_on_whether_a_temp_is_there() {
    let _show = ShowTempsGuard::set(false);
    let op = running_operation();
    let temp = in_flight_temp(&op);
    let name = name_of(&temp);
    let listing = listing_with_temp("listing-temp-agree", &name);

    assert_eq!(get_total_count(listing.id(), true).unwrap(), 1);
    assert_eq!(get_file_range(listing.id(), 0, 10, true).unwrap().len(), 1);
    assert_eq!(
        find_file_index(listing.id(), &name, true).unwrap(),
        None,
        "type-to-jump must not land on an entry the pane isn't showing"
    );
    assert!(
        get_file_at(listing.id(), 1, true).unwrap().is_none(),
        "index 1 is past the end of what the pane can see"
    );
}

/// Hiding scratch is its own axis: showing dotfiles isn't a request to watch
/// Cmdr's temporary files, and hiding them isn't a request to hide dotfiles.
#[tokio::test]
async fn the_scratch_filter_is_independent_of_the_dotfile_filter() {
    let _show = ShowTempsGuard::set(false);
    let op = running_operation();
    let temp = in_flight_temp(&op);
    let name = name_of(&temp);
    let listing = TestListing::new()
        .volume("test")
        .path("/")
        .entries(vec![entry("photo.jpg"), entry(".gitignore"), entry(&name)])
        .insert("listing-temp-axes");

    assert_eq!(
        get_total_count(listing.id(), true).unwrap(),
        2,
        "hidden files shown: the dotfile appears, the scratch file still doesn't"
    );
    assert_eq!(
        get_total_count(listing.id(), false).unwrap(),
        1,
        "hidden files off: neither appears"
    );
}

/// The Settings > Advanced escape hatch shows the in-flight ones too.
#[tokio::test]
async fn the_setting_shows_in_flight_temps() {
    let op = running_operation();
    let temp = in_flight_temp(&op);
    let name = name_of(&temp);
    let listing = listing_with_temp("listing-temp-setting", &name);

    let _show = ShowTempsGuard::set(true);
    assert_eq!(
        get_total_count(listing.id(), true).unwrap(),
        2,
        "the setting shows Cmdr's scratch files"
    );
}
