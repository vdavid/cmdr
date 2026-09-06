//! Which repositories the OPEN LISTINGS keep watched.
//!
//! A pane standing in a repo's virtual `.git` trees is the whole reason the
//! watcher exists for that repo, so arming follows the listing rather than the
//! frontend's `subscribe_git_state`. These cells drive the real listing-open and
//! listing-close paths and read the parked portal's watch count back.
//!
//! The parked portal's watcher is scripted in a test binary (`wiring::portal`),
//! so none of this pays for FSEvents.

#![cfg(test)]

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use crate::file_system::listing::operations::{list_directory_end, list_directory_start_with_volume};
use crate::file_system::listing::sorting::{DirectorySortMode, SortColumn, SortOrder};
use crate::file_system::volume::manager::get_volume_manager;
use crate::file_system::volume::{InMemoryVolume, LocalPosixVolume, Volume};
use crate::test_support::wait_until_async;
use cmdr_git::test_fixtures::{Fixture, cleanup, temp_dir};

use super::{arming, wiring};

/// A repository with one commit, plus the volume a pane would browse it through.
struct Repo {
    dir: PathBuf,
    root: PathBuf,
    volume_id: String,
}

impl Repo {
    fn build(name: &str) -> Self {
        wiring::set_virtual_portal_enabled(true);
        arming::register();

        let dir = temp_dir("arming", name);
        let mut fixture = Fixture::init(dir.clone());
        fixture.commit_file("README.md", b"hello\n", "initial");
        let root = std::fs::canonicalize(&dir).unwrap_or_else(|_| dir.clone());

        // Rooted at `/`, the way the boot disk is: these cells hand the listing
        // absolute paths, and a volume rooted at the repo would resolve them
        // against that root and look for the repo inside itself.
        let volume_id = format!("arming-{name}-{}", std::process::id());
        let volume: Arc<dyn Volume> = Arc::new(LocalPosixVolume::new("Disk", Path::new("/")));
        get_volume_manager().register(&volume_id, volume);

        Self { dir, root, volume_id }
    }

    fn dot_git(&self) -> PathBuf {
        self.root.join(".git")
    }

    /// Opens `path` the way a pane does, answering the listing id to close with.
    async fn open(&self, path: &Path) -> String {
        list_directory_start_with_volume(
            &self.volume_id,
            path,
            true,
            SortColumn::Name,
            SortOrder::Ascending,
            DirectorySortMode::LikeFiles,
        )
        .await
        .expect("the pane opens the listing")
        .listing_id
    }

    fn drop_it(self) {
        get_volume_manager().unregister(&self.volume_id);
        cleanup(&self.dir);
    }
}

/// Waits for the parked portal to be watching exactly `expected` repositories.
/// Arming is detached onto the blocking pool, the same shape as the listing
/// watcher's own arm.
async fn watched_repos_settle_at(expected: usize) {
    wait_until_async(Duration::from_secs(5), "the portal's watch count settles", || {
        wiring::portal().watched_repo_count() == expected
    })
    .await;
}

/// A LONE `branches/` pane keeps its repository watched. Nothing else is open:
/// no working-tree pane, and no frontend `subscribe_git_state`. Without this a
/// pane sitting in a virtual `.git` tree goes stale the moment the chip's
/// subscription is the only thing arming the watcher.
#[tokio::test]
async fn a_lone_portal_listing_arms_the_repos_watcher() {
    let repo = Repo::build("lone_portal_listing");
    assert_eq!(wiring::portal().watched_repo_count(), 0, "nothing is watched yet");

    let listing = repo.open(&repo.dot_git().join("branches")).await;
    watched_repos_settle_at(1).await;
    assert!(
        wiring::portal().fire_watcher(&repo.root),
        "the repo the pane is standing in has a live watch"
    );

    list_directory_end(&listing);
    watched_repos_settle_at(0).await;
    repo.drop_it();
}

/// A pane on the repo's own `.git/` arms it too. Its six category rows carry
/// live counts ("12 branches"), so a `git branch` has to reach it.
#[tokio::test]
async fn a_lone_dot_git_listing_arms_the_repos_watcher() {
    let repo = Repo::build("lone_dot_git_listing");

    let listing = repo.open(&repo.dot_git()).await;
    watched_repos_settle_at(1).await;

    list_directory_end(&listing);
    watched_repos_settle_at(0).await;
    repo.drop_it();
}

/// Two listings on one repository share ONE watch, and it survives until the
/// last of them closes. That's the refcount the frontend's subscription already
/// had, extended to listings.
#[tokio::test]
async fn two_listings_on_one_repo_share_a_watch_until_the_last_one_closes() {
    let repo = Repo::build("shared_watch");

    let branches = repo.open(&repo.dot_git().join("branches")).await;
    let dot_git = repo.open(&repo.dot_git()).await;
    watched_repos_settle_at(1).await;

    list_directory_end(&branches);
    watched_repos_settle_at(1).await;
    assert!(
        wiring::portal().fire_watcher(&repo.root),
        "the `.git/` listing still holds the watch"
    );

    list_directory_end(&dot_git);
    watched_repos_settle_at(0).await;
    repo.drop_it();
}

/// Which listings arm anything at all, asserted on the decision itself: driving
/// the open path can only prove the `true` cases, and a cell that waits for an
/// arm that never comes proves nothing.
#[test]
fn only_a_listing_inside_a_real_repos_dot_git_arms_a_watcher() {
    let local: Arc<dyn Volume> = Arc::new(LocalPosixVolume::new("Disk", Path::new("/")));
    let protocol: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("Phone"));
    let repo = Path::new("/work/proj");
    let dot_git = repo.join(".git");

    wiring::set_virtual_portal_enabled(true);
    assert_eq!(
        arming::repo_a_listing_watches(local.as_ref(), &dot_git.join("branches")),
        Some(repo.to_path_buf()),
        "a virtual tree is the portal's, and its repo is what the pane is reading"
    );
    assert_eq!(
        arming::repo_a_listing_watches(local.as_ref(), &dot_git),
        Some(repo.to_path_buf()),
        "the `.git/` landing rows carry live counts, so it arms too"
    );
    assert_eq!(
        arming::repo_a_listing_watches(local.as_ref(), repo),
        None,
        "an ordinary directory arms nothing"
    );
    assert_eq!(
        arming::repo_a_listing_watches(local.as_ref(), &dot_git.join("hooks")),
        None,
        "a real `.git/` subdirectory is the local volume's, not the portal's"
    );
    assert_eq!(
        arming::repo_a_listing_watches(protocol.as_ref(), &dot_git),
        None,
        "`gix` can't open a path only a protocol can reach"
    );

    wiring::set_virtual_portal_enabled(false);
    assert_eq!(
        arming::repo_a_listing_watches(local.as_ref(), &dot_git.join("branches")),
        None,
        "with the portal off there is nothing virtual to keep fresh"
    );
    wiring::set_virtual_portal_enabled(true);
}
