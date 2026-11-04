//! A write on an in-memory volume that patches through this engine reaches an
//! open listing, the way the browser demo's panes see a copy land.

use std::path::Path;

use cmdr_fs::volume::{InMemoryVolume, Volume};

use cmdr_listing::testing::{TestEngine, TestListing};

use super::backend_listings;

fn names_after(listing: &cmdr_listing::testing::TestListingGuard) -> Vec<String> {
    let mut names = listing.entry_names();
    names.sort();
    names
}

#[tokio::test]
async fn creates_renames_and_deletes_reach_an_open_listing() {
    let engine = TestEngine::new();
    let listing = TestListing::new()
        .volume("mem")
        .path("/docs")
        .insert(&engine, "backend-listings");
    let volume = InMemoryVolume::new("Mem").with_pane_patches("mem", backend_listings(&engine));
    volume.create_directory(Path::new("/docs")).await.expect("docs");

    volume
        .create_file(Path::new("/docs/a.txt"), b"aaa")
        .await
        .expect("a lands");
    volume
        .create_directory(Path::new("/docs/sub"))
        .await
        .expect("sub lands");
    assert_eq!(names_after(&listing), ["a.txt", "sub"]);

    volume
        .rename(Path::new("/docs/a.txt"), Path::new("/docs/b.txt"), false)
        .await
        .expect("the rename lands");
    assert_eq!(names_after(&listing), ["b.txt", "sub"]);

    volume.delete(Path::new("/docs/b.txt")).await.expect("the delete lands");
    assert_eq!(names_after(&listing), ["sub"]);
}

#[tokio::test]
async fn a_volume_with_no_pane_patches_tells_nobody() {
    let engine = TestEngine::new();
    let listing = TestListing::new()
        .volume("mem")
        .path("/docs")
        .insert(&engine, "backend-listings-off");
    let volume = InMemoryVolume::new("Mem");
    volume.create_directory(Path::new("/docs")).await.expect("docs");
    volume
        .create_file(Path::new("/docs/a.txt"), b"aaa")
        .await
        .expect("a lands");
    assert!(names_after(&listing).is_empty());
}
