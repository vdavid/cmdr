//! What the portal exposes to the walkers that are NOT a pane: the delete
//! walkers, the copy scan, and the drive index.
//!
//! A virtual entry is a name with no inode behind it. A pane can render one; a
//! walker that tries to stat, copy, or remove one meets a path that isn't
//! there. These cells pin that NO walker reaches the six virtual category
//! folders, which is what the route plus the listing overlay buy: the rows
//! reach a pane through `crate::listing_overlays` and every other reader lists
//! through `Volume`, which doesn't hold them.

#![cfg(test)]

use std::path::{Path, PathBuf};

use super::test_fixtures::{Fixture, cleanup, temp_dir};
use crate::file_system::LocalPosixVolume;
use crate::file_system::listing::caching::try_get_authoritative_listing;
use crate::file_system::listing::caching_test_support::TestListing;
use crate::file_system::volume::Volume;

const CATEGORIES: [&str; 6] = ["branches", "tags", "commits", "stash", "worktrees", "submodules"];

/// A one-commit repo with a branch, enough for every category to answer.
fn repo(name: &str) -> PathBuf {
    let dir = temp_dir("walker_exposure", name);
    let mut fixture = Fixture::init(dir.clone());
    fixture.commit_file("README.md", b"hello\n", "initial");
    fixture.create_branch("feature/foo");
    dir
}

/// The volume's own answer for `.git` is what is on disk and nothing else. The
/// six category rows join a PANE's listing one layer up, in the listing
/// pipeline, which is what keeps every walker below from meeting them.
#[tokio::test]
async fn a_volume_listing_of_dot_git_carries_no_virtual_category() {
    let dir = repo("volume_listing");
    super::wiring::set_virtual_portal_enabled(true);
    let volume = LocalPosixVolume::new("Test", &dir);

    let names: Vec<String> = volume
        .list_directory(Path::new(".git"), None)
        .await
        .expect("listing .git succeeds")
        .into_iter()
        .map(|e| e.name)
        .collect();

    assert!(names.contains(&"HEAD".to_string()), "real entries are there: {names:?}");
    for category in CATEGORIES {
        assert!(
            !names.contains(&category.to_string()),
            "{category} must not reach a volume listing: {names:?}"
        );
    }
    cleanup(&dir);
}

/// And what a PANE listing of the same directory shows: the real entries the
/// volume read, plus the six rows the overlay contributed.
#[tokio::test]
async fn an_overlay_decorated_listing_of_dot_git_shows_the_six_rows() {
    let dir = repo("overlay_listing");
    super::wiring::set_virtual_portal_enabled(true);
    super::overlay::register();
    let volume: std::sync::Arc<dyn Volume> = std::sync::Arc::new(LocalPosixVolume::new("Test", &dir));
    let dot_git = dir.join(".git");

    let mut entries = volume.list_directory(&dot_git, None).await.expect("listing .git");
    let added = crate::listing_overlays::decorate(&volume, &dot_git, &mut entries).await;
    assert_eq!(added, 6, "one row per category");

    let names: Vec<String> = entries.into_iter().map(|e| e.name).collect();
    assert!(names.contains(&"HEAD".to_string()), "real entries stay: {names:?}");
    for category in CATEGORIES {
        assert!(
            names.contains(&category.to_string()),
            "{category} missing from the pane's listing: {names:?}"
        );
    }
    cleanup(&dir);
}

/// With the portal off the overlay contributes nothing, so `.git/` is exactly
/// the directory on disk.
#[tokio::test]
async fn the_toggle_off_contributes_nothing_to_a_dot_git_listing() {
    let dir = repo("overlay_toggle_off");
    super::overlay::register();
    let volume: std::sync::Arc<dyn Volume> = std::sync::Arc::new(LocalPosixVolume::new("Test", &dir));
    let dot_git = dir.join(".git");

    super::wiring::set_virtual_portal_enabled(false);
    let mut entries = volume.list_directory(&dot_git, None).await.expect("listing .git");
    let added = crate::listing_overlays::decorate(&volume, &dot_git, &mut entries).await;
    super::wiring::set_virtual_portal_enabled(true);

    assert_eq!(added, 0);
    let names: Vec<String> = entries.into_iter().map(|e| e.name).collect();
    for category in CATEGORIES {
        assert!(!names.contains(&category.to_string()), "{category} in {names:?}");
    }
    cleanup(&dir);
}

/// The volume-aware delete walker (every non-boot volume: an external disk, a
/// share, a phone) lists through `Volume::list_directory`, which no longer
/// carries a row with nothing behind it. A repo delete on an external disk used
/// to meet all six and refuse each with `NotSupported`, stopping with the repo
/// half-gone.
#[tokio::test]
async fn the_volume_delete_walker_never_meets_a_virtual_folder() {
    let dir = repo("volume_delete_walk");
    super::wiring::set_virtual_portal_enabled(true);
    let volume = LocalPosixVolume::new("Test", &dir);

    // What `delete_volume_files_with_progress_inner` does per directory.
    let children = volume.list_directory(Path::new(".git"), None).await.unwrap();
    let virtual_children: Vec<&str> = children
        .iter()
        .filter(|e| CATEGORIES.contains(&e.name.as_str()))
        .map(|e| e.name.as_str())
        .collect();
    assert!(virtual_children.is_empty(), "walker sees {virtual_children:?}");

    // And the whole `.git/` really comes off, which is the operation the six
    // phantom rows used to stop half-way through.
    let mut stack = vec![PathBuf::from(".git")];
    let mut directories = Vec::new();
    while let Some(next) = stack.pop() {
        for child in volume.list_directory(&next, None).await.expect("listing a real dir") {
            let relative = next.join(&child.name);
            if child.is_directory {
                stack.push(relative);
            } else {
                volume.delete(&relative).await.expect("a real file under .git deletes");
            }
        }
        directories.push(next);
    }
    for directory in directories.into_iter().rev() {
        volume.delete(&directory).await.expect("a real dir under .git deletes");
    }
    assert!(!dir.join(".git").exists(), ".git is gone");
    cleanup(&dir);
}

/// A REAL file under `.git` is an ordinary local file: readable, writable,
/// renamable, deletable, with the portal on as much as off. Nothing in the
/// local backend knows the word "git" any more, which is what makes that true
/// by construction rather than by a guard nobody must widen.
#[tokio::test]
async fn real_files_under_dot_git_stay_fully_mutable() {
    let dir = repo("real_file_mutability");
    super::wiring::set_virtual_portal_enabled(true);
    let volume = LocalPosixVolume::new("Test", &dir);

    // Reading `.git/config`'s real bytes and streaming them back out to a copy
    // beside it: the two stream methods that used to be claimed by the portal.
    let source = volume
        .open_read_stream(Path::new(".git/config"))
        .await
        .expect("`.git/config` streams as an ordinary file");
    let size = std::fs::metadata(dir.join(".git/config"))
        .expect("config is on disk")
        .len();
    volume
        .write_from_stream(Path::new(".git/config.bak"), size, source, &|_, _| {
            std::ops::ControlFlow::Continue(())
        })
        .await
        .expect("writing under `.git` lands");
    assert_eq!(
        std::fs::read(dir.join(".git/config.bak"))
            .expect("the copy is on disk")
            .len() as u64,
        size
    );

    // Creating, renaming, and removing beside it.
    volume
        .create_file(Path::new(".git/scratch"), b"x")
        .await
        .expect("creating under .git");
    volume
        .rename(Path::new(".git/scratch"), Path::new(".git/scratch2"), false)
        .await
        .expect("renaming under .git");
    volume
        .delete(Path::new(".git/scratch2"))
        .await
        .expect("deleting under .git");
    volume.delete(Path::new(".git/HEAD")).await.expect("removing HEAD");
    assert!(!dir.join(".git/HEAD").exists());

    cleanup(&dir);
}

/// The copy scan walks with `walkdir` against the resolved path and never asks
/// the volume for a listing, so no virtual entry reaches it. Counting the tree
/// twice (scan vs. a bare `walkdir`) is the assertion: an extra six would show
/// up as a difference.
#[tokio::test]
async fn the_copy_scan_counts_only_what_is_on_disk() {
    let dir = repo("copy_scan");
    super::wiring::set_virtual_portal_enabled(true);
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
/// the disk when the cache's watch covers every writer. `.git/` is a real
/// directory that CAN be watched now, so what keeps the six phantom rows out of
/// that walker is the overlay flag: a listing an overlay decorated is a pane
/// view, and the oracle declines it whatever its watch says.
#[tokio::test]
async fn the_listing_oracle_declines_an_overlay_decorated_dot_git_listing() {
    let dir = repo("oracle_declines");
    let dot_git = dir.join(".git");
    super::wiring::set_virtual_portal_enabled(true);
    super::overlay::register();

    let volume: std::sync::Arc<dyn Volume> = std::sync::Arc::new(LocalPosixVolume::new("Test", &dir));
    let mut cached = volume.list_directory(&dot_git, None).await.unwrap();
    let added = crate::listing_overlays::decorate(&volume, &dot_git, &mut cached).await;
    assert_eq!(added, 6, "the pane's listing holds the six virtual rows");

    let _guard = TestListing::new()
        .volume(crate::file_system::volume::DEFAULT_VOLUME_ID)
        .path(dot_git.clone())
        .entries(cached)
        .overlay_rows(added)
        .insert("git_portal_oracle");

    assert!(
        try_get_authoritative_listing(crate::file_system::volume::DEFAULT_VOLUME_ID, &dot_git).is_none(),
        "a pane's decorated listing must never substitute for a walker's read"
    );
    cleanup(&dir);
}

/// In a linked worktree `git worktree add` writes `.git` as a FILE holding
/// `gitdir: <common>/worktrees/<name>`, not a directory. The overlay contributes
/// to a DIRECTORY listing, so the `.git/` landing page isn't there; the
/// categories under it still are, because the route is lexical and `gix`
/// discovery follows the gitlink.
#[tokio::test]
async fn a_linked_worktree_serves_the_categories_but_has_no_dot_git_landing() {
    let dir = repo("linked_worktree");
    super::wiring::set_virtual_portal_enabled(true);
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

    // No landing listing: `.git` is a file, so the volume can't list it and the
    // overlay never runs. The rewritten real rows it used to show couldn't be
    // opened anyway (`<linked>/.git/HEAD` is a path THROUGH a file), so the
    // portal simply starts one level down here.
    let volume: std::sync::Arc<dyn Volume> = std::sync::Arc::new(LocalPosixVolume::new("Test", &linked));
    assert!(
        volume.list_directory(&gitlink, None).await.is_err(),
        "listing a gitlink file is an error, not a portal root"
    );

    // The categories under it are the portal's, reached through the route, which
    // is pure string work and never stats the `.git`.
    assert!(
        super::path::portal_route(&gitlink.join("branches")).is_some(),
        "a linked worktree's categories still route"
    );
    let (virt, ..) = super::path::classify(&gitlink.join("branches")).expect("classify follows the gitlink");
    assert_eq!(virt, super::path::VirtualGitPath::Category(super::path::Cat::Branches));

    let portal = std::sync::Arc::new(super::portal::GitPortal::new(
        crate::volume_host::host(),
        super::state_sink::no_git_state_sink(),
    ));
    let parent: std::sync::Arc<dyn Volume> = std::sync::Arc::new(LocalPosixVolume::new("Parent", &linked));
    let branches = portal
        .volume_for(linked.clone(), parent)
        .list_directory(&gitlink.join("branches"), None)
        .await
        .expect("the portal lists branches in a linked worktree");
    assert!(
        branches.iter().any(|e| e.name == "linked-branch"),
        "{:?}",
        branches.iter().map(|e| &e.name).collect::<Vec<_>>()
    );

    cleanup(&linked);
    cleanup(&dir);
}
