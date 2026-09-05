//! What the portal exposes to the walkers that are NOT a pane: the delete
//! walkers, the copy scan, and the drive index.
//!
//! A virtual entry is a name with no inode behind it. A pane can render one; a
//! walker that tries to stat, copy, or remove one meets a path that isn't
//! there. These cells pin which walkers can currently reach the six virtual
//! category folders, so the routing work that follows can't quietly widen that
//! set.

#![cfg(test)]

use std::path::Path;

use super::test_fixtures::{Fixture, cleanup, temp_dir};
use crate::file_system::LocalPosixVolume;
use crate::file_system::listing::caching::try_get_authoritative_listing;
use crate::file_system::listing::caching_test_support::TestListing;
use crate::file_system::volume::{Volume, VolumeError};

const CATEGORIES: [&str; 6] = ["branches", "tags", "commits", "stash", "worktrees", "submodules"];

/// A one-commit repo with a branch, enough for every category to answer.
fn repo(name: &str) -> std::path::PathBuf {
    let dir = temp_dir("walker_exposure", name);
    let mut fixture = Fixture::init(dir.clone());
    fixture.commit_file("README.md", b"hello\n", "initial");
    fixture.create_branch("feature/foo");
    dir
}

/// The pane's own read: `Volume::list_directory` on `.git` runs through the
/// portal hook, so all six categories are in the listing.
#[tokio::test]
async fn a_volume_listing_of_dot_git_carries_the_six_virtual_categories() {
    let dir = repo("volume_listing");
    super::set_virtual_portal_enabled(true);
    let volume = LocalPosixVolume::new("Test", &dir);

    let names: Vec<String> = volume
        .list_directory(Path::new(".git"), None)
        .await
        .expect("listing .git succeeds")
        .into_iter()
        .map(|e| e.name)
        .collect();

    for category in CATEGORIES {
        assert!(
            names.contains(&category.to_string()),
            "{category} missing from {names:?}"
        );
    }
    cleanup(&dir);
}

/// The volume-aware delete walker (every non-boot volume: an external disk, a
/// share, a phone) lists through that same hook, so it meets the six virtual
/// folders as if they were directories to descend into and remove.
#[tokio::test]
async fn the_volume_delete_walker_meets_the_virtual_folders() {
    let dir = repo("volume_delete_walk");
    super::set_virtual_portal_enabled(true);
    let volume = LocalPosixVolume::new("Test", &dir);

    // What `delete_volume_files_with_progress_inner` does per directory.
    let children = volume.list_directory(Path::new(".git"), None).await.unwrap();
    let virtual_children: Vec<&str> = children
        .iter()
        .filter(|e| CATEGORIES.contains(&e.name.as_str()))
        .map(|e| e.name.as_str())
        .collect();
    assert_eq!(virtual_children.len(), 6, "walker sees {virtual_children:?}");

    // And every one of them refuses the removal that follows.
    for category in CATEGORIES {
        let err = volume
            .delete(&Path::new(".git").join(category))
            .await
            .expect_err("a virtual folder can't be removed");
        assert!(matches!(err, VolumeError::NotSupported), "{category}: {err:?}");
    }
    cleanup(&dir);
}

/// The same guard is a path-shape check, not a portal check, so it also refuses
/// the REAL files under `.git`. A volume-routed delete of a repo folder can't
/// remove `.git/config`, which is what makes the walker's exposure a data bug
/// rather than a cosmetic one: the operation stops with the repo half-deleted.
#[tokio::test]
async fn the_mutation_guard_also_refuses_real_files_under_dot_git() {
    let dir = repo("real_file_guard");
    let volume = LocalPosixVolume::new("Test", &dir);

    for real in ["config", "HEAD"] {
        assert!(Path::new(&dir).join(".git").join(real).exists(), "{real} is on disk");
        let err = volume
            .delete(&Path::new(".git").join(real))
            .await
            .expect_err("today's guard refuses it");
        assert!(matches!(err, VolumeError::NotSupported), "{real}: {err:?}");
    }

    // The guard doesn't consult the toggle, so turning the portal off changes
    // nothing here.
    super::set_virtual_portal_enabled(false);
    let err = volume
        .delete(Path::new(".git/config"))
        .await
        .expect_err("still refused");
    assert!(matches!(err, VolumeError::NotSupported), "{err:?}");
    super::set_virtual_portal_enabled(true);

    cleanup(&dir);
}

/// The copy scan walks with `walkdir` against the resolved path and never asks
/// the volume for a listing, so no virtual entry reaches it. Counting the tree
/// twice (scan vs. a bare `walkdir`) is the assertion: an extra six would show
/// up as a difference.
#[tokio::test]
async fn the_copy_scan_counts_only_what_is_on_disk() {
    let dir = repo("copy_scan");
    super::set_virtual_portal_enabled(true);
    let volume = LocalPosixVolume::new("Test", &dir);

    let scanned = volume
        .scan_for_copy_batch(std::slice::from_ref(&dir))
        .await
        .expect("scanning the repo succeeds");

    let mut on_disk_dirs = 0usize;
    let mut on_disk_files = 0usize;
    for entry in walkdir::WalkDir::new(&dir).min_depth(1) {
        let entry = entry.unwrap();
        if entry.file_type().is_dir() {
            on_disk_dirs += 1;
        } else if entry.file_type().is_file() {
            on_disk_files += 1;
        }
    }

    assert_eq!(scanned.aggregate.file_count as usize, on_disk_files);
    assert_eq!(scanned.aggregate.dir_count as usize, on_disk_dirs);
    cleanup(&dir);
}

/// The LOCAL delete walker reads a directory from the listing cache instead of
/// the disk when the cache's watch covers every writer. Nothing under `.git` is
/// ever watched (`listing/streaming.rs` skips arming one for a path that may
/// not exist), so the oracle declines and the walker falls back to `read_dir`.
/// That is the single fact keeping virtual entries out of the local delete and
/// the scan preview.
#[tokio::test]
async fn the_listing_oracle_declines_a_cached_dot_git_listing() {
    let dir = repo("oracle_declines");
    let dot_git = dir.join(".git");

    let cached = LocalPosixVolume::new("Test", &dir)
        .list_directory(Path::new(".git"), None)
        .await
        .unwrap();
    assert!(
        cached.iter().any(|e| e.name == "branches"),
        "the cache holds a virtual entry"
    );

    let _guard = TestListing::new()
        .volume(crate::file_system::volume::DEFAULT_VOLUME_ID)
        .path(dot_git.clone())
        .entries(cached)
        .insert("git_portal_oracle");

    assert!(
        try_get_authoritative_listing(crate::file_system::volume::DEFAULT_VOLUME_ID, &dot_git).is_none(),
        "an unwatched .git listing must never substitute for a read"
    );
    cleanup(&dir);
}

/// In a linked worktree `git worktree add` writes `.git` as a FILE holding
/// `gitdir: <common>/worktrees/<name>`, not a directory. `classify` splits on
/// the path SEGMENT and never stats it, so the portal answers there exactly as
/// it does in the main worktree: `.git` lists the six categories even though
/// nothing on disk says "directory".
#[tokio::test]
async fn a_linked_worktree_serves_the_portal_from_a_dot_git_file() {
    let dir = repo("linked_worktree");
    super::set_virtual_portal_enabled(true);
    let linked = dir
        .parent()
        .unwrap()
        .join(format!("{}_linked", dir.file_name().unwrap().to_string_lossy()));
    super::test_fixtures::git_cli(
        &dir,
        &["worktree", "add", &linked.to_string_lossy(), "-b", "linked-branch"],
    );

    let gitlink = linked.join(".git");
    assert!(
        gitlink.is_file() && !gitlink.is_dir(),
        "the linked worktree's .git is a file"
    );

    let classified = super::path::classify(&gitlink);
    assert!(classified.is_some(), "classify answers for a .git that is a file");

    let names: Vec<String> = LocalPosixVolume::new("Test", &linked)
        .list_directory(Path::new(".git"), None)
        .await
        .expect("the portal lists a gitlink path")
        .into_iter()
        .map(|e| e.name)
        .collect();
    for category in CATEGORIES {
        assert!(
            names.contains(&category.to_string()),
            "{category} missing from {names:?}"
        );
    }

    // Real entries ride along too: `read_real_dot_git` follows the gitlink to
    // `<common>/worktrees/<name>/` and rewrites each entry's path under
    // `<linked>/.git/`. Those rewritten paths have no counterpart on disk, so a
    // real entry listed here can be seen but not opened.
    assert!(
        names.contains(&"HEAD".to_string()),
        "a real gitdir entry shows up: {names:?}"
    );
    let head = linked.join(".git").join("HEAD");
    assert!(
        std::fs::read(&head).is_err(),
        "the path the pane shows for HEAD doesn't resolve (the gitlink is a file)"
    );

    cleanup(&linked);
    cleanup(&dir);
}
