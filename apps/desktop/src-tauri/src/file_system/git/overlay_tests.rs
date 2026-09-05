//! What the git contributor claims, and what the watcher's refresh reaches.
//!
//! The seam's own rules (registration, the shadowing merge) live in
//! `src/listing_overlays/tests.rs`; the walker-exposure consequences in
//! `walker_exposure_tests.rs`.

#![cfg(test)]

use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::overlay::GitPortalOverlay;
use crate::file_system::listing::caching_test_support::{TestListing, WatchCoverageVolume};
use crate::file_system::volume::{LocalPosixVolume, Volume, WatchCoverage};
use crate::listing_overlays::{ListingOverlay, decorate};
use cmdr_git::test_fixtures::{Fixture, cleanup, temp_dir};

fn repo(name: &str) -> PathBuf {
    let dir = temp_dir("git_overlay", name);
    let mut fixture = Fixture::init(dir.clone());
    fixture.commit_file("README.md", b"hello\n", "initial");
    fixture.create_branch("feature/foo");
    dir
}

/// A directory merely NAMED `.git` inside a repo's working tree gets no rows.
/// `gix` discovery walks UP, so without this check the outer repo's branches
/// would be listed inside a test corpus's placeholder folder.
#[tokio::test]
async fn a_dot_git_that_is_not_this_repos_gitdir_gets_no_rows() {
    let dir = repo("stray_dot_git");
    super::wiring::set_virtual_portal_enabled(true);
    super::overlay::register();

    let stray = dir.join("fixtures").join("sample").join(".git");
    std::fs::create_dir_all(&stray).expect("make the placeholder");

    let volume: Arc<dyn Volume> = Arc::new(LocalPosixVolume::new("Test", &dir));
    let mut entries = volume.list_directory(&stray, None).await.expect("listing it");
    assert_eq!(decorate(&volume, &stray, &mut entries).await, 0, "{entries:?}");

    // The repo's own `.git` still gets them, so the check isn't just refusing
    // everything.
    let real = dir.join(".git");
    let mut real_entries = volume.list_directory(&real, None).await.expect("listing .git");
    assert_eq!(decorate(&volume, &real, &mut real_entries).await, 6);

    cleanup(&dir);
}

/// A protocol-only backend (direct SMB, MTP, ADB) never gets the portal: `gix`
/// can't open a path only a protocol can reach, and a real `branches/` folder on
/// a share must stay an ordinary folder. The route asks the same question, so
/// the two seams appear in exactly one set of places.
#[test]
fn a_volume_with_no_local_path_never_gets_the_portal() {
    super::wiring::set_virtual_portal_enabled(true);
    let local = LocalPosixVolume::new("Local", Path::new("/"));
    let remote = WatchCoverageVolume::new("Remote", WatchCoverage::EveryWriter);

    assert!(super::wiring::volume_holds_real_repos(&local));
    assert!(!super::wiring::volume_holds_real_repos(&remote));

    let overlay = GitPortalOverlay;
    let dot_git = Path::new("/anywhere/repo/.git");
    assert!(overlay.applies_to(&local, dot_git));
    assert!(!overlay.applies_to(&remote, dot_git));
}

/// Only a directory called `.git` is claimed, never a real entry under one and
/// never a sibling.
#[test]
fn only_the_dot_git_directory_itself_is_claimed() {
    super::wiring::set_virtual_portal_enabled(true);
    let volume = LocalPosixVolume::new("Local", Path::new("/"));
    let overlay = GitPortalOverlay;

    assert!(overlay.applies_to(&volume, Path::new("/repo/.git")));
    for other in [
        "/repo",
        "/repo/.git/refs",
        "/repo/.git/hooks",
        "/repo/src",
        "/repo/.github",
    ] {
        assert!(!overlay.applies_to(&volume, Path::new(other)), "{other}");
    }
}

/// The toggle refreshes the pane the user is actually looking at, which means
/// asking the LISTING CACHE rather than the watcher registry: standing in
/// `.git/` doesn't imply a `subscribe_git_state` for that repo, and deriving the
/// set from subscribed repos left the pane showing six rows the portal no longer
/// served.
#[test]
fn the_toggle_refresh_reaches_a_dot_git_pane_with_no_repo_subscription() {
    let dir = repo("toggle_targets");
    let dot_git = std::fs::canonicalize(&dir).expect("canonical").join(".git");

    // No `subscribe_git_state` anywhere: just two open listings.
    assert_eq!(
        super::wiring::portal().watched_repo_count(),
        0,
        "this cell's premise is that nothing is subscribed"
    );
    let at_dot_git = TestListing::new()
        .volume("local-external-1234")
        .path(dot_git.clone())
        .insert("git-toggle-dot-git");
    let inside = TestListing::new()
        .volume("root")
        .path(dot_git.join("branches"))
        .insert("git-toggle-inside");
    let elsewhere = TestListing::new()
        .volume("root")
        .path(dir.join("src"))
        .insert("git-toggle-elsewhere");

    let targeted = super::wiring::listings_inside_a_dot_git();
    let paths: Vec<&PathBuf> = targeted.iter().map(|(_, path)| path).collect();

    assert!(paths.contains(&&dot_git), "the `.git/` pane itself: {paths:?}");
    assert!(
        paths.contains(&&dot_git.join("branches")),
        "and one inside it: {paths:?}"
    );
    assert!(
        !paths.iter().any(|p| p.ends_with("src")),
        "a listing outside any `.git` is left alone: {paths:?}"
    );

    drop(at_dot_git);
    drop(inside);
    drop(elsewhere);
    cleanup(&dir);
}

/// The watcher's post-change refresh reaches a portal listing on ANY volume, not
/// only the boot one. A repo lives just as happily on an external disk, which
/// gets its own volume id; filtering to the default volume left a portal pane
/// there showing stale children after a `git checkout`.
#[test]
fn the_refresh_reaches_a_portal_listing_on_a_non_default_volume() {
    let dir = repo("refresh_targets");
    let dot_git = std::fs::canonicalize(&dir).expect("canonical").join(".git");

    let on_external = TestListing::new()
        .volume("local-external-1234")
        .path(dot_git.join("branches"))
        .insert("git-refresh-external");
    let elsewhere = TestListing::new()
        .volume("local-external-1234")
        .path(dir.join("src"))
        .insert("git-refresh-elsewhere");

    let targeted = super::wiring::listings_under(&super::wiring::virtual_category_prefixes(&dot_git));
    let paths: Vec<&PathBuf> = targeted.iter().map(|(_, path)| path).collect();

    assert!(
        paths.contains(&&dot_git.join("branches")),
        "the portal listing on the external disk is refreshed: {paths:?}"
    );
    assert!(
        !paths.iter().any(|p| p.ends_with("src")),
        "an unrelated listing is left alone: {paths:?}"
    );

    drop(on_external);
    drop(elsewhere);
    cleanup(&dir);
}

/// ONE app-side switch, read by both seams: the overlay stops contributing the
/// `.git/` category rows and the route stops sending `.git/<category>/` to the
/// portal volume. The toggle is process-global, so the cell restores it.
#[test]
fn the_toggle_silences_both_portal_seams() {
    let dir = repo("portal_toggle");
    let dot_git = dir.join(".git");
    let overlay = GitPortalOverlay;
    let volume = LocalPosixVolume::new("Test", &dir);

    super::wiring::set_virtual_portal_enabled(true);
    assert!(super::wiring::is_virtual_portal_enabled());
    assert!(overlay.applies_to(&volume, &dot_git), "the overlay claims .git/");
    assert!(
        cmdr_git::portal_route(&dot_git.join("branches")).is_some(),
        "the route claims .git/branches"
    );

    super::wiring::set_virtual_portal_enabled(false);
    assert!(!super::wiring::is_virtual_portal_enabled());
    assert!(
        !overlay.applies_to(&volume, &dot_git),
        "with the portal off, .git/ is whatever is on disk"
    );

    super::wiring::set_virtual_portal_enabled(true);
    cleanup(&dir);
}
