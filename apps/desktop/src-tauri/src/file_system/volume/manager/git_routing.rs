//! Git-portal routing for the [`VolumeManager`]: the half of
//! [`resolve`](VolumeManager::resolve) that sends a path reaching into a repo's
//! virtual `.git` trees to a read-only
//! [`GitPortalVolume`](crate::file_system::git::volume::GitPortalVolume), plus
//! the LRU that caps how many stay registered.
//!
//! The archive half is `archive_routing.rs` and the dispatcher over both is
//! `routing.rs`; this is a third inherent `impl VolumeManager` block.

use super::VolumeManager;
use super::routing::{ResolvedVolume, RoutedKind};
use crate::file_system::git;
use crate::file_system::volume::Volume;
use crate::ignore_poison::IgnorePoison;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Max number of `GitPortalVolume`s kept registered at once. Each one pins an
/// open `gix` repository, so browsing through a directory of checkouts must not
/// accumulate them without bound. Eviction is harmless: the next navigation
/// re-resolves and re-registers, and the repo handle itself lives in the
/// portal's shared cache either way.
const GIT_PORTAL_LRU_CAP: usize = 8;

impl VolumeManager {
    /// Routes `path` to the portal volume for its repo, or `None` when the path
    /// isn't the portal's (no `.git/<category>/` segment) or the portal is
    /// switched off.
    ///
    /// ❗ Zero I/O to decide. `git::path::portal_route` is a segment check, and
    /// the volume answers `NotFound` on first use if that `.git` turns out not
    /// to be a repository it can open. That's what keeps the check affordable on
    /// `resolve`, which runs on every path-bearing call.
    ///
    /// The toggle is consulted HERE rather than inside the volume: with the
    /// portal off there is no route, so a `.git/branches` path is an ordinary
    /// directory on the parent volume (usually missing), which is exactly what
    /// the user asked to see.
    pub(super) fn resolve_git_portal(&self, volume_id: &str, path: &Path) -> Option<ResolvedVolume> {
        if !git::wiring::is_virtual_portal_enabled() {
            return None;
        }
        let repo_root = git::path::portal_route(path)?;
        // The requested volume physically holds the repo, so it's the portal's
        // parent (shared lane key and space info).
        let parent = self.get(volume_id)?;
        // A protocol-only backend (direct SMB, MTP, ADB) can hold a directory
        // called `.git` with a real `branches/` inside it, and `gix` can't open
        // any of it. Routing there would turn that ordinary directory into
        // `NotFound`. Same question the overlay asks, so the portal appears in
        // exactly one set of places.
        if !git::wiring::volume_holds_real_repos(parent.as_ref()) {
            return None;
        }
        Some(self.register_git_portal(parent, repo_root, path))
    }

    /// Registers (or reuses) the portal volume for `repo_root`, bumps the LRU,
    /// and returns it resolved.
    fn register_git_portal(&self, parent: Arc<dyn Volume>, repo_root: PathBuf, path: &Path) -> ResolvedVolume {
        // Canonical, so two spellings of one repo (`/tmp/x` and `/private/tmp/x`
        // on macOS, a symlinked checkout) share a single registration rather
        // than fighting over the ID.
        let repo_root = std::fs::canonicalize(&repo_root).unwrap_or(repo_root);
        let portal_id = git_portal_volume_id(&repo_root);
        let volume = Arc::new(git::wiring::portal().volume_for(repo_root, Arc::clone(&parent)));
        self.register_if_absent(&portal_id, volume);
        self.touch_git_portal_lru(&portal_id);

        match self.get(&portal_id) {
            Some(volume) => ResolvedVolume {
                volume: Some(volume),
                path: path.to_path_buf(),
                routed: Some(RoutedKind::GitPortal),
            },
            // Registered then evicted before we could read it back (only
            // reachable under a pathologically small cap). Fall back to the
            // parent volume.
            None => ResolvedVolume::passthrough(Some(parent), path),
        }
    }

    /// Records `id` as the most-recently-resolved portal and unregisters the
    /// least-recently-resolved ones past [`GIT_PORTAL_LRU_CAP`]. Unregisters
    /// OUTSIDE the LRU lock, so the LRU and volumes locks are never held at
    /// once. Mirrors the archive LRU.
    fn touch_git_portal_lru(&self, id: &str) {
        let evicted = {
            let mut lru = self.git_portal_lru.lock_ignore_poison();
            super::routing::touch_routed_lru(&mut lru, id, GIT_PORTAL_LRU_CAP)
        };
        for old in evicted {
            self.unregister(&old);
        }
    }
}

/// The registry id for the portal over `repo_root`: `git-portal-{hash(root)}`.
/// Backend-internal only — it never enters frontend state, history, or
/// persistence (the FE holds the parent drive id for display), so a fixed-seed
/// hash that's stable within a process is all it needs.
fn git_portal_volume_id(repo_root: &Path) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    repo_root.hash(&mut hasher);
    format!("git-portal-{:016x}", hasher.finish())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_system::git::test_fixtures::{Fixture, cleanup, temp_dir};
    use crate::file_system::volume::LocalPosixVolume;

    /// The portal's root for the repo at `dir`. Canonical, because registration
    /// canonicalizes so two spellings of one repo share a registration, and on
    /// macOS a temp dir is reached through a symlink (`/var` → `/private/var`).
    fn portal_root(dir: &Path) -> PathBuf {
        std::fs::canonicalize(dir).expect("canonical repo root").join(".git")
    }

    /// A repo with one commit and a branch, registered as a plain local volume
    /// under `"root"`, which is what a pane would be browsing.
    fn manager_over_a_repo(name: &str) -> (PathBuf, VolumeManager) {
        let dir = temp_dir("git_routing", name);
        let mut fixture = Fixture::init(dir.clone());
        fixture.commit_file("README.md", b"hello\n", "initial");
        fixture.create_branch("feature/foo");

        let manager = VolumeManager::new();
        manager.register("root", Arc::new(LocalPosixVolume::new("Root", &dir)));
        git::wiring::set_virtual_portal_enabled(true);
        (dir, manager)
    }

    #[tokio::test]
    async fn every_virtual_category_routes_to_the_portal() {
        let (dir, manager) = manager_over_a_repo("categories");

        for category in ["branches", "tags", "commits", "stash", "worktrees", "submodules"] {
            let path = dir.join(".git").join(category);
            let resolved = manager.resolve("root", &path).await;
            assert_eq!(
                resolved.routed,
                Some(RoutedKind::GitPortal),
                "{category} must reach the portal"
            );
            // The path travels through untouched; the volume maps it itself.
            assert_eq!(resolved.path, path);
            assert_eq!(resolved.volume.expect("portal volume").root(), portal_root(&dir));
        }

        // And so does a path deep inside a snapshot.
        let deep = dir.join(".git/branches/feature/foo/README.md");
        assert_eq!(manager.resolve("root", &deep).await.routed, Some(RoutedKind::GitPortal));

        cleanup(&dir);
    }

    /// `.git/` itself and the real files under it stay on the parent volume, so
    /// they keep every bit of their normal behavior: editable, renamable,
    /// deletable, and walkable when a repo folder is deleted.
    #[tokio::test]
    async fn the_portal_root_and_real_dot_git_files_stay_on_the_parent() {
        let (dir, manager) = manager_over_a_repo("real_entries");

        for path in [
            dir.join(".git"),
            dir.join(".git/config"),
            dir.join(".git/HEAD"),
            dir.join(".git/objects"),
            dir.join("README.md"),
        ] {
            let resolved = manager.resolve("root", &path).await;
            assert_eq!(resolved.routed, None, "{} must stay on the parent", path.display());
            assert_eq!(resolved.volume.expect("parent volume").name(), "Root");
        }

        cleanup(&dir);
    }

    /// With the portal switched off there is no route at all, so a `.git`
    /// listing is whatever the parent volume says it is: for a category folder
    /// no repo actually has on disk, that's `NotFound`, which is what a pane
    /// standing on one when the user flips the setting recovers from.
    #[tokio::test]
    async fn the_toggle_turns_the_route_off_and_an_open_virtual_pane_gets_not_found() {
        use crate::file_system::volume::VolumeError;

        let (dir, manager) = manager_over_a_repo("toggle");
        let path = dir.join(".git/branches");

        git::wiring::set_virtual_portal_enabled(false);
        let resolved = manager.resolve("root", &path).await;
        assert_eq!(resolved.routed, None);
        let parent = resolved.volume.expect("parent volume");
        assert_eq!(parent.name(), "Root");
        assert!(
            matches!(parent.list_directory(&path, None).await, Err(VolumeError::NotFound(_))),
            "the pane's recovery runs on a NotFound, ❌ never on a silent empty listing"
        );

        git::wiring::set_virtual_portal_enabled(true);
        assert_eq!(manager.resolve("root", &path).await.routed, Some(RoutedKind::GitPortal));

        cleanup(&dir);
    }

    /// A protocol-only backend (direct SMB, MTP, ADB) never routes, whatever the
    /// path looks like: `gix` can't open a path only a protocol can reach, and a
    /// real folder called `branches` on a share stays an ordinary folder. Same
    /// question the listing overlay asks.
    #[tokio::test]
    async fn a_volume_with_no_local_path_never_routes_to_the_portal() {
        use crate::file_system::listing::caching_test_support::WatchCoverageVolume;
        use crate::file_system::volume::WatchCoverage;

        let manager = VolumeManager::new();
        manager.register(
            "share",
            Arc::new(WatchCoverageVolume::new("Share", WatchCoverage::EveryWriter)),
        );
        git::wiring::set_virtual_portal_enabled(true);

        let resolved = manager
            .resolve("share", Path::new("/mnt/share/repo/.git/branches"))
            .await;
        assert_eq!(resolved.routed, None);
        assert_eq!(resolved.volume.expect("the share itself").name(), "Share");
    }

    /// A repo checked out inside another repo's working tree routes to the
    /// INNER one: the first `.git` segment wins, so the portal a path names is
    /// the repo whose `.git` it goes through.
    #[tokio::test]
    async fn a_repo_inside_a_repo_routes_to_the_inner_one() {
        let (outer, manager) = manager_over_a_repo("nested_outer");
        let inner = outer.join("vendor").join("inner");
        std::fs::create_dir_all(&inner).expect("make the inner checkout");
        let mut inner_fixture = Fixture::init(inner.clone());
        inner_fixture.commit_file("lib.rs", b"inner\n", "initial");

        let resolved = manager.resolve("root", &inner.join(".git/branches/main")).await;
        assert_eq!(resolved.routed, Some(RoutedKind::GitPortal));
        assert_eq!(
            resolved.volume.expect("portal volume").root(),
            portal_root(&inner),
            "the inner repo's portal serves it"
        );

        cleanup(&outer);
    }

    /// An archive in the repo's WORKING tree is untouched by git routing: the
    /// two routes don't overlap, so a `.zip` beside the source still opens as an
    /// archive.
    #[tokio::test]
    async fn an_archive_in_the_working_tree_still_routes_to_the_archive() {
        let (dir, manager) = manager_over_a_repo("working_tree_zip");
        let zip = dir.join("bundle.zip");
        std::fs::write(&zip, b"PK\x03\x04not-a-real-archive-body").expect("write zip magic");

        let resolved = manager.resolve("root", &zip.join("inner.txt")).await;
        assert_eq!(resolved.routed, Some(RoutedKind::Archive));

        cleanup(&dir);
    }

    /// One portal per repo, reused across navigations rather than re-minted.
    #[tokio::test]
    async fn two_paths_in_one_repo_share_one_portal_volume() {
        let (dir, manager) = manager_over_a_repo("shared");

        let first = manager
            .resolve("root", &dir.join(".git/branches"))
            .await
            .volume
            .expect("first");
        let second = manager
            .resolve("root", &dir.join(".git/tags"))
            .await
            .volume
            .expect("second");
        assert!(Arc::ptr_eq(&first, &second));

        cleanup(&dir);
    }

    /// Browsing more repos than the cap keeps a bounded number registered, and
    /// re-visiting an evicted one re-registers it.
    #[tokio::test]
    async fn the_lru_caps_how_many_portals_stay_registered() {
        let mut repos = Vec::new();
        let manager = VolumeManager::new();
        git::wiring::set_virtual_portal_enabled(true);

        for index in 0..=GIT_PORTAL_LRU_CAP {
            let dir = temp_dir("git_routing", &format!("lru{index}"));
            let mut fixture = Fixture::init(dir.clone());
            fixture.commit_file("README.md", b"hello\n", "initial");
            manager.register("root", Arc::new(LocalPosixVolume::new("Root", &dir)));
            manager.resolve("root", &dir.join(".git/branches")).await;
            repos.push(dir);
        }

        // The parent plus exactly the cap: the oldest portal was evicted.
        assert_eq!(manager.count(), 1 + GIT_PORTAL_LRU_CAP);
        let oldest = git_portal_volume_id(&std::fs::canonicalize(&repos[0]).expect("canonical"));
        assert!(manager.get(&oldest).is_none());

        manager.register("root", Arc::new(LocalPosixVolume::new("Root", &repos[0])));
        assert_eq!(
            manager.resolve("root", &repos[0].join(".git/branches")).await.routed,
            Some(RoutedKind::GitPortal)
        );
        assert!(manager.get(&oldest).is_some(), "re-visiting re-registers it");

        for dir in &repos {
            cleanup(dir);
        }
    }

    /// The portal is not a mount: a path inside it belongs to the volume
    /// holding the repo, which is what `inspect_file` and the index router route
    /// by. Without this, the portal's `<repo>/.git` root would be the longest
    /// matching prefix and would claim every virtual path.
    #[tokio::test]
    async fn mount_id_for_path_names_the_parent_mount_not_the_portal() {
        let dir = temp_dir("git_routing", "mount_id");
        let mut fixture = Fixture::init(dir.clone());
        fixture.commit_file("README.md", b"hello\n", "initial");

        let manager = VolumeManager::new();
        manager.register("ext", Arc::new(LocalPosixVolume::new("Ext", &dir)));
        git::wiring::set_virtual_portal_enabled(true);

        let virtual_path = dir.join(".git/branches/main");
        assert_eq!(
            manager.resolve("ext", &virtual_path).await.routed,
            Some(RoutedKind::GitPortal)
        );
        assert_eq!(
            manager.mount_id_for_path(&virtual_path.to_string_lossy()).as_deref(),
            Some("ext")
        );

        cleanup(&dir);
    }
}
