//! The look-alike finder: form-only matches, exact names, and when it lists.

use std::path::Path;
use std::sync::Arc;

use super::*;
use crate::file_system::listing::caching_test_support::{
    TestListing, TestListingGuard, WatchCoverageVolume, unique_test_id,
};
use crate::file_system::volume::manager::get_volume_manager;
use crate::file_system::volume::{InMemoryVolume, WatchCoverage};
use crate::file_system::write_operations::create::create_directory_core;
use crate::file_system::write_operations::mutation_error::MutationError;

const CAFE_NFC: &str = "caf\u{e9}.txt";
const CAFE_NFD: &str = "cafe\u{301}.txt";

fn entry(name: &str) -> FileEntry {
    FileEntry::new(name.to_string(), format!("/dir/{name}"), false, false)
}

#[test]
fn one_entry_spelled_in_another_form_is_the_look_alike() {
    let entries = [entry("caf\u{e9}.txt"), entry("other.txt")];
    match among(&entries, "cafe\u{301}.txt") {
        LookAlike::One(found) => assert_eq!(found.name, "caf\u{e9}.txt"),
        other => panic!("expected the composed entry, got {other:?}"),
    }
}

#[test]
fn an_exact_name_answers_for_itself() {
    let entries = [entry("caf\u{e9}.txt"), entry("cafe\u{301}.txt")];
    assert!(matches!(among(&entries, "cafe\u{301}.txt"), LookAlike::None));
}

#[test]
fn a_name_differing_in_case_is_not_a_look_alike() {
    let entries = [entry("Report.docx")];
    assert!(matches!(among(&entries, "report.docx"), LookAlike::None));
}

#[test]
fn two_look_alikes_and_no_exact_name_is_several() {
    // "élő" three ways: fully composed, fully decomposed, and half of each.
    let entries = [entry("\u{e9}l\u{151}"), entry("e\u{301}lo\u{30b}")];
    assert!(matches!(among(&entries, "\u{e9}lo\u{30b}"), LookAlike::Several));
}

#[tokio::test]
async fn a_byte_exact_volume_lists_to_find_the_look_alike() {
    let volume = InMemoryVolume::new("v");
    volume.create_directory(Path::new("/dir")).await.unwrap();
    volume.create_file(Path::new("/dir/caf\u{e9}.txt"), b"x").await.unwrap();
    match look_alike_in(&volume, Path::new("/dir"), "cafe\u{301}.txt").await {
        Ok(LookAlike::One(found)) => assert_eq!(found.path, "/dir/caf\u{e9}.txt"),
        other => panic!("expected the composed entry, got {other:?}"),
    }
}

#[tokio::test]
async fn a_folder_that_is_not_there_holds_no_look_alike() {
    let volume = InMemoryVolume::new("v");
    assert!(matches!(
        look_alike_in(&volume, Path::new("/nowhere"), "cafe\u{301}.txt").await,
        Ok(LookAlike::None)
    ));
}

/// A registered byte-exact volume whose watch the test pins, holding an empty
/// `/dir`, plus a pane's listing of `/dir` holding `pane_holds`. The volume
/// itself holds nothing, so an answer naming `pane_holds` came from the pane.
async fn share_with_pane_listing(
    tag: &str,
    coverage: WatchCoverage,
    pane_holds: &str,
) -> (String, Arc<WatchCoverageVolume>, TestListingGuard) {
    let id = unique_test_id(&format!("look-alike-{tag}"));
    let volume = Arc::new(WatchCoverageVolume::new(&id, coverage));
    volume
        .create_directory(Path::new("/dir"))
        .await
        .expect("the fixture folder");
    get_volume_manager().register(&id, Arc::clone(&volume) as Arc<dyn Volume>);
    let listing = TestListing::new()
        .volume(&id)
        .path("/dir")
        .entries(vec![entry(pane_holds)])
        .insert(tag);
    (id, volume, listing)
}

/// A pane already showing the folder answers the look-alike check, so a new
/// name on a share costs no listing of the folder the person is looking at
/// (ERR-AREUV: seconds per new folder in a 3,340-entry share folder).
#[tokio::test]
async fn a_folder_a_watched_pane_shows_is_answered_from_its_listing() {
    let (id, volume, _listing) = share_with_pane_listing("pane-hit", WatchCoverage::EveryWriter, CAFE_NFC).await;

    let answer = ListedFolders::with_pane_listings(volume.as_ref(), &id)
        .look_alike_in(Path::new("/dir"), CAFE_NFD)
        .await;

    match answer {
        Ok(LookAlike::One(found)) => assert_eq!(found.name, CAFE_NFC),
        other => panic!("expected the pane's composed entry, got {other:?}"),
    }
    assert_eq!(volume.list_calls(), 0, "listed a folder the pane already holds");
    get_volume_manager().unregister(&id);
}

/// A watch blind to other writers (an OS-mounted share) can miss the very entry
/// another client spelled the other way, so its listing doesn't stand in for a
/// read.
#[tokio::test]
async fn a_pane_listing_only_this_machine_keeps_current_is_read_past() {
    let (id, volume, _listing) = share_with_pane_listing("pane-blind", WatchCoverage::ThisMachineOnly, CAFE_NFC).await;

    let answer = ListedFolders::with_pane_listings(volume.as_ref(), &id)
        .look_alike_in(Path::new("/dir"), CAFE_NFD)
        .await;

    assert!(matches!(answer, Ok(LookAlike::None)), "{answer:?}");
    assert_eq!(volume.list_calls(), 1);
    get_volume_manager().unregister(&id);
}

/// A caller that didn't name the volume lists, pane or no pane.
#[tokio::test]
async fn without_a_volume_id_the_folder_is_listed() {
    let (id, volume, _listing) = share_with_pane_listing("pane-unnamed", WatchCoverage::EveryWriter, CAFE_NFC).await;

    let answer = ListedFolders::new(volume.as_ref())
        .look_alike_in(Path::new("/dir"), CAFE_NFD)
        .await;

    assert!(matches!(answer, Ok(LookAlike::None)), "{answer:?}");
    assert_eq!(volume.list_calls(), 1);
    get_volume_manager().unregister(&id);
}

/// End to end: New folder beside a watched pane refuses the look-alike the pane
/// shows, creates a free name, and never lists the folder.
#[tokio::test]
async fn a_new_folder_beside_a_watched_pane_is_placed_without_a_listing() {
    let (id, volume, _listing) = share_with_pane_listing("pane-create", WatchCoverage::EveryWriter, CAFE_NFC).await;

    let refused = create_directory_core(Some(id.clone()), "/dir", CAFE_NFD).await;
    let created = create_directory_core(Some(id.clone()), "/dir", "\u{f6}k\u{f6}r").await;

    assert!(
        matches!(refused, Err(MutationError::AlreadyExists { .. })),
        "{refused:?}"
    );
    assert!(created.is_ok(), "{created:?}");
    assert_eq!(volume.list_calls(), 0);
    get_volume_manager().unregister(&id);
}
