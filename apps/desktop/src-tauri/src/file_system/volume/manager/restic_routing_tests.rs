//! The restic route: what it routes, what it leaves on the parent, what a
//! confirmation costs, and how it orders against the git and archive routes.

use super::*;
use crate::file_system::volume::{InMemoryVolume, VolumeError};
use cmdr_fs::entry::FileEntry;
use cmdr_fs::volume::ListingProgress;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};

const REPO: &str = "/Volumes/naspi/backups/m3-restic";
const VIEW: &str = "/Volumes/naspi/backups/m3-restic/Browse snapshots";

/// A parent that counts its listings, over an in-memory volume.
struct CountingParent {
    inner: InMemoryVolume,
    listings: AtomicUsize,
}

impl CountingParent {
    fn new(inner: InMemoryVolume) -> Self {
        Self {
            inner,
            listings: AtomicUsize::new(0),
        }
    }

    fn listings(&self) -> usize {
        self.listings.load(Ordering::Relaxed)
    }
}

impl Volume for CountingParent {
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn root(&self) -> &Path {
        self.inner.root()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn list_directory<'a>(
        &'a self,
        path: &'a Path,
        on_progress: Option<&'a (dyn Fn(ListingProgress) + Sync)>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
        self.listings.fetch_add(1, Ordering::Relaxed);
        self.inner.list_directory(path, on_progress)
    }

    fn get_metadata<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<FileEntry, VolumeError>> + Send + 'a>> {
        self.inner.get_metadata(path)
    }
}

/// Lays out the names a restic repo's folder holds. Routing only reads the
/// folder's listing (`is_restic_repo`), so the shape is enough; a cell that
/// browses snapshots builds a real repo with `FixtureRepo`.
async fn repo_shaped(volume: &InMemoryVolume, root: &Path) {
    volume
        .create_file(&root.join("config"), b"encrypted")
        .await
        .expect("config");
    for dir in ["data", "index", "keys", "snapshots", "locks"] {
        volume.create_directory(&root.join(dir)).await.expect("repo dir");
    }
}

/// A manager whose `volume_id` is an in-memory volume holding a repo at
/// [`REPO`]. Each cell picks its own id: the hub's memo is the app's one hub,
/// shared by every cell in the binary.
async fn manager_over_a_repo(volume_id: &str) -> (VolumeManager, Arc<CountingParent>) {
    let inner = InMemoryVolume::new("naspi");
    repo_shaped(&inner, Path::new(REPO)).await;
    let parent = Arc::new(CountingParent::new(inner));
    let manager = VolumeManager::new();
    manager.register(volume_id, Arc::clone(&parent) as Arc<dyn Volume>);
    (manager, parent)
}

#[tokio::test]
async fn the_row_and_every_path_below_it_route_to_the_view() {
    let (manager, _parent) = manager_over_a_repo("restic-routes-below").await;

    for path in [
        PathBuf::from(VIEW),
        Path::new(VIEW).join("latest"),
        Path::new(VIEW).join("2026-10-12 21.00.03 (a1b2c3d4)/Users/me/notes.txt"),
    ] {
        let resolved = manager.resolve("restic-routes-below", &path).await;
        assert_eq!(resolved.routed, Some(RoutedKind::Restic), "{}", path.display());
        // The path travels through untouched; the view classifies it itself.
        assert_eq!(resolved.path, path);
        assert_eq!(resolved.volume.expect("the view").root(), Path::new(VIEW));
    }
}

/// The repo folder and its real files are the parent's: listable, copyable,
/// deletable, exactly like any folder.
#[tokio::test]
async fn the_repo_folder_and_its_real_files_stay_on_the_parent() {
    let (manager, _parent) = manager_over_a_repo("restic-real-files").await;

    for path in [
        PathBuf::from(REPO),
        Path::new(REPO).join("config"),
        Path::new(REPO).join("snapshots"),
        Path::new(REPO).join("data/ab"),
    ] {
        let resolved = manager.resolve("restic-real-files", &path).await;
        assert_eq!(resolved.routed, None, "{} stays on the parent", path.display());
        assert_eq!(resolved.volume.expect("parent").name(), "naspi");
    }
}

/// A real folder called `Browse snapshots` outside a repo is an ordinary
/// folder, and it pays ONE confirmation listing however often it's resolved.
#[tokio::test]
async fn a_non_repo_browse_snapshots_folder_passes_through_for_one_listing() {
    let inner = InMemoryVolume::new("naspi");
    let real = Path::new("/Volumes/naspi/photos/Browse snapshots");
    inner.create_directory(real).await.expect("a real folder of that name");
    let parent = Arc::new(CountingParent::new(inner));
    let manager = VolumeManager::new();
    manager.register("restic-non-repo", Arc::clone(&parent) as Arc<dyn Volume>);

    for _ in 0..5 {
        let resolved = manager.resolve("restic-non-repo", &real.join("x.jpg")).await;
        assert_eq!(resolved.routed, None);
        assert_eq!(resolved.volume.expect("parent").name(), "naspi");
    }
    assert_eq!(parent.listings(), 1, "the no is remembered");
}

/// The overlay confirmed the repo when it added the row, so entering the row
/// costs no listing at all.
#[tokio::test]
async fn a_repo_the_overlay_met_routes_with_no_confirm_listing() {
    let (manager, parent) = manager_over_a_repo("restic-overlay-seeded").await;
    wiring::repos().overlay_row("restic-overlay-seeded", Path::new(REPO));

    let resolved = manager.resolve("restic-overlay-seeded", Path::new(VIEW)).await;
    assert_eq!(resolved.routed, Some(RoutedKind::Restic));
    assert_eq!(parent.listings(), 0);
}

/// A typed path or a restored session meets the repo cold: one listing, then
/// remembered.
#[tokio::test]
async fn a_cold_repo_costs_one_confirm_listing() {
    let (manager, parent) = manager_over_a_repo("restic-cold").await;

    for _ in 0..3 {
        assert_eq!(
            manager
                .resolve("restic-cold", &Path::new(VIEW).join("latest"))
                .await
                .routed,
            Some(RoutedKind::Restic)
        );
    }
    assert_eq!(parent.listings(), 1);
}

/// The sync sibling routes on the memo alone: a repo nobody confirmed yet
/// passes through rather than costing a `read_dir` on a scan walker.
#[tokio::test]
async fn resolve_local_only_routes_on_the_memo_and_never_lists() {
    let (manager, parent) = manager_over_a_repo("restic-local-only").await;
    let inside = Path::new(VIEW).join("latest");

    assert_eq!(manager.resolve_local_only("restic-local-only", &inside).routed, None);
    assert_eq!(parent.listings(), 0, "no confirm listing on the sync path");

    wiring::repos().overlay_row("restic-local-only", Path::new(REPO));
    assert_eq!(
        manager.resolve_local_only("restic-local-only", &inside).routed,
        Some(RoutedKind::Restic)
    );
}

/// A `.zip` inside a snapshot the view can't vouch for (here: a repo-shaped
/// folder nobody unlocked, so the view can't read the file) stays with the view,
/// which answers for the path as it would for any other.
#[tokio::test]
async fn an_unconfirmed_zip_inside_a_snapshot_stays_with_the_view() {
    let (manager, _parent) = manager_over_a_repo("restic-zip-inside").await;
    let path = Path::new(VIEW).join("latest/Users/me/bundle.zip/readme.txt");

    assert_eq!(
        manager.resolve("restic-zip-inside", &path).await.routed,
        Some(RoutedKind::Restic)
    );
}

/// A repo kept in a real FOLDER named like an archive still browses: the
/// unconfirmed archive candidate hands over instead of ending routing.
#[tokio::test]
async fn a_repo_inside_a_real_folder_named_like_an_archive_still_routes() {
    let inner = InMemoryVolume::new("naspi");
    let repo = Path::new("/Volumes/naspi/backups.zip/m3-restic");
    repo_shaped(&inner, repo).await;
    let manager = VolumeManager::new();
    manager.register("restic-zip-folder", Arc::new(inner));

    let resolved = manager
        .resolve("restic-zip-folder", &repo.join("Browse snapshots/latest"))
        .await;
    assert_eq!(resolved.routed, Some(RoutedKind::Restic));
}

/// A non-repo `Browse snapshots` folder doesn't end routing either: a repo
/// deeper in the path still routes.
#[tokio::test]
async fn a_repo_below_a_non_repo_browse_snapshots_folder_still_routes() {
    let inner = InMemoryVolume::new("naspi");
    let repo = Path::new("/Volumes/naspi/Browse snapshots/m3-restic");
    repo_shaped(&inner, repo).await;
    let manager = VolumeManager::new();
    manager.register("restic-nested-name", Arc::new(inner));

    let resolved = manager
        .resolve("restic-nested-name", &repo.join("Browse snapshots/latest"))
        .await;
    assert_eq!(resolved.routed, Some(RoutedKind::Restic));
    assert_eq!(resolved.volume.expect("the view").root(), repo.join("Browse snapshots"));
}

/// A view isn't a parent for another view: a repo backed up inside a snapshot
/// stays bytes in that snapshot.
#[tokio::test]
async fn a_routed_volume_is_never_a_restic_parent() {
    let (manager, _parent) = manager_over_a_repo("restic-nested-view").await;
    let view = manager
        .resolve("restic-nested-view", Path::new(VIEW))
        .await
        .volume
        .expect("the view");
    assert!(view.routes_over_a_parent());
    manager.register("restic-nested-view-inner", Arc::clone(&view));
    let inner_repo_view = Path::new(VIEW).join("latest/backups/repo/Browse snapshots");
    let resolved = manager.resolve("restic-nested-view-inner", &inner_repo_view).await;
    assert_eq!(resolved.routed, None, "no view is minted over a view");
}

/// A `.git/branches` inside a snapshot is a backed-up folder: the restic
/// boundary sits further left, so the portal never sees it.
#[tokio::test]
async fn a_git_portal_path_inside_a_snapshot_belongs_to_the_view() {
    use cmdr_local::LocalPosixVolume;

    let dir = tempfile::tempdir().expect("tempdir");
    let repo = dir.path().join("repo");
    for folder in ["data", "index", "keys", "snapshots"] {
        std::fs::create_dir_all(repo.join(folder)).expect("repo folder");
    }
    std::fs::write(repo.join("config"), b"encrypted").expect("config");
    let manager = VolumeManager::new();
    manager.register(
        "restic-git-inside",
        Arc::new(LocalPosixVolume::local_folder(
            "Root",
            dir.path(),
            crate::volume_host::host(),
        )),
    );
    crate::file_system::git::wiring::set_virtual_portal_enabled(true);

    let path = repo.join("Browse snapshots/latest/proj/.git/branches/main");
    assert_eq!(
        manager.resolve("restic-git-inside", &path).await.routed,
        Some(RoutedKind::Restic)
    );
}

/// A repo backed up inside a git snapshot is the portal's: the `.git`
/// boundary sits further left, even with the hub ready to route it.
#[tokio::test]
async fn a_restic_path_inside_a_git_snapshot_belongs_to_the_portal() {
    use cmdr_git::test_fixtures::{Fixture, cleanup, temp_dir};
    use cmdr_local::LocalPosixVolume;

    let dir = temp_dir("restic_routing", "inside_git");
    let mut fixture = Fixture::init(dir.clone());
    fixture.commit_file("README.md", b"hello\n", "initial");
    let manager = VolumeManager::new();
    manager.register(
        "restic-inside-git",
        Arc::new(LocalPosixVolume::local_folder("Root", &dir, crate::volume_host::host())),
    );
    crate::file_system::git::wiring::set_virtual_portal_enabled(true);
    let backed_up_repo = dir.join(".git/branches/main/backup");
    wiring::repos().overlay_row("restic-inside-git", &backed_up_repo);

    let resolved = manager
        .resolve("restic-inside-git", &backed_up_repo.join("Browse snapshots/latest"))
        .await;
    assert_eq!(resolved.routed, Some(RoutedKind::GitPortal));

    cleanup(&dir);
}

/// One view per repo, reused across navigations.
#[tokio::test]
async fn two_paths_in_one_repo_share_one_view() {
    let (manager, _parent) = manager_over_a_repo("restic-shared").await;
    let first = manager
        .resolve("restic-shared", Path::new(VIEW))
        .await
        .volume
        .expect("first");
    let second = manager
        .resolve("restic-shared", &Path::new(VIEW).join("latest"))
        .await
        .volume
        .expect("second");
    assert!(Arc::ptr_eq(&first, &second));
}

/// An SMB share reconnecting replaces the parent under the same id; the view
/// over the retired one is minted again over the new one.
#[tokio::test]
async fn a_replaced_parent_gets_a_new_view() {
    let (manager, _parent) = manager_over_a_repo("restic-replaced").await;
    let before = manager
        .resolve("restic-replaced", Path::new(VIEW))
        .await
        .volume
        .expect("before");

    let reconnected = InMemoryVolume::new("naspi");
    repo_shaped(&reconnected, Path::new(REPO)).await;
    manager.force_register("restic-replaced", Arc::new(reconnected));

    let after = manager
        .resolve("restic-replaced", Path::new(VIEW))
        .await
        .volume
        .expect("after");
    assert!(
        !Arc::ptr_eq(&before, &after),
        "a view over the retired parent isn't reused"
    );

    let again = manager
        .resolve("restic-replaced", Path::new(VIEW))
        .await
        .volume
        .expect("again");
    assert!(Arc::ptr_eq(&after, &again), "and the new one is");
}

/// Browsing more repos than the cap keeps a bounded number registered, and
/// re-visiting an evicted one re-registers it.
#[tokio::test]
async fn the_lru_caps_how_many_views_stay_registered() {
    let inner = InMemoryVolume::new("naspi");
    let repos: Vec<PathBuf> = (0..=RESTIC_LRU_CAP)
        .map(|index| PathBuf::from(format!("/Volumes/naspi/repo{index}")))
        .collect();
    for repo in &repos {
        repo_shaped(&inner, repo).await;
    }
    let manager = VolumeManager::new();
    manager.register("restic-lru", Arc::new(inner));

    for repo in &repos {
        let resolved = manager.resolve("restic-lru", &repo.join("Browse snapshots")).await;
        assert_eq!(resolved.routed, Some(RoutedKind::Restic));
    }
    assert_eq!(manager.count(), 1 + RESTIC_LRU_CAP);
    let oldest = restic_volume_id("restic-lru", &repos[0]);
    assert!(manager.get(&oldest).is_none());

    manager.resolve("restic-lru", &repos[0].join("Browse snapshots")).await;
    assert!(manager.get(&oldest).is_some(), "re-visiting re-registers it");
}

/// The view isn't a mount: a path inside it belongs to the drive holding the
/// repo, which is what `inspect_file` and the index router route by.
#[tokio::test]
async fn mount_id_for_path_names_the_parent_not_the_view() {
    let inner = InMemoryVolume::new("naspi").with_root("/Volumes/naspi");
    repo_shaped(&inner, Path::new(REPO)).await;
    let manager = VolumeManager::new();
    manager.register("naspi", Arc::new(inner));

    let inside = Path::new(VIEW).join("latest/Users/me");
    assert_eq!(manager.resolve("naspi", &inside).await.routed, Some(RoutedKind::Restic));
    assert_eq!(
        manager.mount_id_for_path(&inside.to_string_lossy()).as_deref(),
        Some("naspi")
    );
}

/// The lexical gate in front of `resolve` names every path at or under the row,
/// and none of the repo's real files.
#[test]
fn the_cheap_gate_covers_the_view_and_nothing_real() {
    use super::super::path_routes_over_its_parent;
    assert!(path_routes_over_its_parent(Path::new(VIEW)));
    assert!(path_routes_over_its_parent(&Path::new(VIEW).join("latest/a.txt")));
    assert!(!path_routes_over_its_parent(Path::new(REPO)));
    assert!(!path_routes_over_its_parent(&Path::new(REPO).join("config")));
}

/// End to end: a real repo on the parent, entered through the route, lists its
/// snapshots once unlocked.
#[tokio::test]
async fn entering_the_row_lists_the_snapshots() {
    use cmdr_restic::test_fixtures::{FixtureRepo, unlock};
    use std::time::{Duration, SystemTime};

    let inner = InMemoryVolume::new("naspi");
    let fixture = FixtureRepo::init("pw");
    fixture.add_snapshot(SystemTime::UNIX_EPOCH + Duration::from_secs(1_791_000_000), "m3");
    fixture.write_into(&inner, Path::new(REPO)).await;
    let manager = VolumeManager::new();
    manager.register("restic-end-to-end", Arc::new(inner));

    let resolved = manager.resolve("restic-end-to-end", Path::new(VIEW)).await;
    assert_eq!(resolved.routed, Some(RoutedKind::Restic));
    let volume = resolved.volume.expect("the view");
    let view = volume
        .as_any()
        .downcast_ref::<cmdr_restic::ResticVolume>()
        .expect("a restic view");
    assert!(unlock(view, "pw").await);

    let names: Vec<String> = volume
        .list_directory(Path::new(VIEW), None)
        .await
        .expect("lists the snapshots")
        .into_iter()
        .map(|entry| entry.name)
        .collect();
    assert_eq!(names.len(), 2, "one snapshot plus latest: {names:?}");
    assert!(names.iter().any(|name| name == "latest"), "{names:?}");
}

/// End to end (#631): a real repo whose snapshot holds a real zip. The zip FILE
/// is the view's (a file in the backup, copyable as bytes), and everything inside
/// it is an archive read THROUGH the view: listed, read, and refused every write,
/// since a snapshot can't be rewritten.
#[tokio::test]
async fn a_zip_inside_a_snapshot_browses_as_an_archive_over_the_view() {
    use cmdr_archive::test_fixtures::{build_zip, dir, stored};
    use cmdr_restic::test_fixtures::{FixtureRepo, unlock};
    use std::time::{Duration, SystemTime};

    let source = crate::test_support::TestDir::new("restic-zip-in-snapshot");
    let zip_bytes = build_zip(&[
        stored("readme.txt", b"hello from the backup".to_vec()),
        dir("docs/"),
        stored("docs/notes.txt", b"inner notes".to_vec()),
    ]);
    std::fs::write(source.join("bundle.zip"), &zip_bytes).expect("source zip");
    let fixture = FixtureRepo::init("pw");
    fixture.backup(&source, SystemTime::UNIX_EPOCH + Duration::from_secs(1_791_000_000));
    let inner = InMemoryVolume::new("naspi");
    fixture.write_into(&inner, Path::new(REPO)).await;
    let manager = VolumeManager::new();
    manager.register("restic-zip-browse", Arc::new(inner));
    let view = manager
        .resolve("restic-zip-browse", Path::new(VIEW))
        .await
        .volume
        .expect("the view");
    let restic = view
        .as_any()
        .downcast_ref::<cmdr_restic::ResticVolume>()
        .expect("a restic view");
    assert!(unlock(restic, "pw").await);
    let canonical = std::fs::canonicalize(&*source).expect("canonical source");
    let zip = Path::new(VIEW)
        .join("latest")
        .join(canonical.strip_prefix("/").expect("absolute"));
    let zip = zip.join("bundle.zip");

    // Acting ON the zip (copy it out, view it, drag it) gets the backup's file;
    // only a listing enters it.
    let file = manager.resolve("restic-zip-browse", &zip).await;
    assert_eq!(
        file.routed,
        Some(RoutedKind::Restic),
        "the zip itself is a file in the backup"
    );
    let file = file.volume.expect("the view");
    let meta = file.get_metadata(&zip).await.expect("the backed-up zip");
    assert!(!meta.is_directory);
    assert_eq!(meta.size, Some(zip_bytes.len() as u64));
    let entered = manager.resolve_to_list("restic-zip-browse", &zip).await;
    assert_eq!(
        entered.routed,
        Some(RoutedKind::ArchiveInSnapshot),
        "a listing enters it"
    );

    let inside = manager.resolve("restic-zip-browse", &zip.join("docs")).await;
    assert_eq!(inside.routed, Some(RoutedKind::ArchiveInSnapshot));
    assert_eq!(inside.path, zip.join("docs"), "the path travels through untouched");
    let archive = inside.volume.expect("the archive");
    assert!(
        archive.as_any().downcast_ref::<cmdr_archive::ArchiveVolume>().is_some(),
        "an archive volume serves the inside"
    );
    let mut names: Vec<String> = archive
        .list_directory(&zip, None)
        .await
        .expect("lists the archive's root")
        .into_iter()
        .map(|entry| entry.name)
        .collect();
    names.sort();
    assert_eq!(names, ["docs", "readme.txt"]);
    let notes = archive
        .list_directory(&zip.join("docs"), None)
        .await
        .expect("lists a folder inside");
    assert_eq!(notes.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(), ["notes.txt"]);
    let mut stream = archive
        .open_read_stream(&zip.join("readme.txt"))
        .await
        .expect("opens an entry");
    let mut bytes = Vec::new();
    while let Some(chunk) = stream.next_chunk().await {
        bytes.extend_from_slice(&chunk.expect("a chunk"));
    }
    assert_eq!(bytes, b"hello from the backup");

    let kind = RoutedKind::ArchiveInSnapshot;
    assert!(
        !kind.takes_writes() && !kind.can_give_up_files(),
        "a snapshot is never rewritten"
    );
    assert!(kind.is_archive(), "every archive reader treats it as one");
}
