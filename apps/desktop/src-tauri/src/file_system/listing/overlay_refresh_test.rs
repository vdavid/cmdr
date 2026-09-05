//! What a watcher-driven refresh does to a listing an overlay decorated.
//!
//! `.git/` is a real directory, so a pane sitting on one arms a real FSEvents
//! watch and every `.git/*` write drives a `FullRefresh` through here. The
//! refresh re-reads through the VOLUME, which holds none of the portal's six
//! category rows, so it has to run the overlays again in the same place the
//! first read did. Without that, one `git commit` under an open `.git/` pane
//! would silently empty the portal out of it.
//!
//! What the overlays themselves decide: `src/listing_overlays/tests.rs` and
//! `file_system/git/overlay_tests.rs`.

use std::path::PathBuf;
use std::sync::Arc;

use crate::file_system::listing::caching::notify_full_refresh;
use crate::file_system::listing::caching_test_support::{TestListing, unique_test_id};
use crate::file_system::listing::sorting::{DirectorySortMode, SortColumn, SortOrder};
use crate::file_system::volume::manager::get_volume_manager;
use crate::file_system::volume::{LocalPosixVolume, Volume};
use cmdr_git::test_fixtures::{Fixture, cleanup, temp_dir};

const CATEGORIES: [&str; 6] = ["branches", "tags", "commits", "stash", "worktrees", "submodules"];

fn repo(name: &str) -> PathBuf {
    let dir = temp_dir("overlay_refresh", name);
    let mut fixture = Fixture::init(dir.clone());
    fixture.commit_file("README.md", b"hello\n", "initial");
    fixture.create_branch("feature/foo");
    dir
}

/// A `FullRefresh` on a repo's `.git/` keeps the six portal rows AND the flag
/// that stops a walker reusing the listing; with the portal off the same
/// refresh drops both, so the listing becomes an honest picture of the
/// directory again.
///
/// One cell rather than two: the portal switch is process-global, so a sibling
/// cell flipping it mid-run would decide this one's answer.
#[tokio::test]
async fn a_full_refresh_of_dot_git_tracks_what_the_overlay_contributes() {
    let dir = repo("full_refresh");
    let dot_git = dir.join(".git");
    crate::file_system::git::wiring::set_virtual_portal_enabled(true);
    crate::file_system::git::overlay::register();

    let volume_id = unique_test_id("overlay-refresh-vol");
    let volume: Arc<dyn Volume> = Arc::new(LocalPosixVolume::new("Repo", &dir));
    get_volume_manager().register(&volume_id, Arc::clone(&volume));

    // A pane's listing as the first read left it: real entries plus the six.
    let mut entries = volume.list_directory(&dot_git, None).await.expect("listing .git");
    let added = crate::listing_overlays::decorate(&volume, &dot_git, &mut entries).await;
    assert_eq!(added, 6);
    let listing = TestListing::new()
        .volume(&volume_id)
        .path(dot_git.clone())
        .entries(entries)
        .overlay_rows(added)
        .insert("overlay-refresh");

    // What the FSEvents watch on `.git/` drives after any `.git/*` write.
    refresh(&volume_id, &dot_git, listing.id()).await;

    let names = listing.entry_names();
    assert!(names.iter().any(|n| n == "HEAD"), "real entries survive: {names:?}");
    for category in CATEGORIES {
        assert!(
            names.iter().any(|n| n == category),
            "{category} lost to the refresh: {names:?}"
        );
    }
    assert!(
        listing.with_listing(|l| l.has_overlay_rows()),
        "the refreshed listing is still a pane view, not a walker's picture of the directory"
    );

    // Portal off: the same refresh leaves the directory and nothing else, and
    // clears the flag so a delete walker may reuse the listing again.
    crate::file_system::git::wiring::set_virtual_portal_enabled(false);
    refresh(&volume_id, &dot_git, listing.id()).await;
    crate::file_system::git::wiring::set_virtual_portal_enabled(true);

    let names = listing.entry_names();
    for category in CATEGORIES {
        assert!(!names.iter().any(|n| n == category), "{category} in {names:?}");
    }
    assert!(
        !listing.with_listing(|l| l.has_overlay_rows()),
        "nothing was contributed, so nothing marks this a pane view"
    );

    get_volume_manager().unregister(&volume_id);
    cleanup(&dir);
}

/// One watcher-driven `FullRefresh` of `path`, landing on `listing_id`.
async fn refresh(volume_id: &str, path: &std::path::Path, listing_id: &str) {
    notify_full_refresh(
        volume_id.to_string(),
        path.to_path_buf(),
        vec![(
            listing_id.to_string(),
            SortColumn::Name,
            SortOrder::Ascending,
            DirectorySortMode::LikeFiles,
        )],
    )
    .await;
}
