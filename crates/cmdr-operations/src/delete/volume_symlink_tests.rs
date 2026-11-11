//! A volume delete takes a symlink as the link it is, ❌ never its target's
//! contents, at every level and from every source of a top-level answer: the
//! child listing, the parent pane's listing, the backend's own stat, and a
//! cached scan preview.
//!
//! The rig is a real `LocalPosixVolume` over a tempdir (real links), behind a
//! wrapper that can answer `is_directory` the way a link-FOLLOWING backend does
//! (ADB, SFTP), can list a link as the folder it points to the way a Samba
//! share does (#613), and can let a pane's listing vouch for a folder.

#![cfg(unix)]

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::Duration;

use super::super::event_sinks::CollectorEventSink;
use super::super::host::testing::PaneListings;
use super::super::scan_preview::count_selected_links_as_links;
use super::super::state::{CachedScanResult, WriteOperationState};
use super::super::testing::TestOperations;
use super::super::types::WriteOperationConfig;
use super::volume_walker::delete_volume_files_with_progress_inner;
use cmdr_fs::entry::FileEntry;
use cmdr_fs::volume::{
    BackendKind, BatchScanResult, CopyScanResult, EntryKind, ListingProgress, ScanBoundary, Volume, VolumeError,
    WatchCoverage,
};
use cmdr_listing::testing::{TestListing, TestListingGuard};
use cmdr_local::LocalPosixVolume;
use tempfile::TempDir;

/// A `LocalPosixVolume` that can answer `is_directory` by FOLLOWING a link (the
/// trait default over `get_metadata`, as ADB and SFTP do) and can report every
/// listing as watcher-fresh, so a pane's cached listing answers for it. With
/// `hides_links` it plays a Samba share: an SMB volume whose listings never mark
/// a link, while its `entry_kind` (a stat) does.
struct LinkRig {
    inner: LocalPosixVolume,
    follows_links: bool,
    watched: bool,
    hides_links: bool,
    entry_kind_calls: AtomicUsize,
}

impl Volume for LinkRig {
    fn name(&self) -> &str {
        self.inner.name()
    }
    fn root(&self) -> &Path {
        self.inner.root()
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn backend_kind(&self) -> BackendKind {
        if self.hides_links {
            BackendKind::Smb
        } else {
            BackendKind::Local
        }
    }
    fn list_directory<'a>(
        &'a self,
        path: &'a Path,
        on_progress: Option<&'a (dyn Fn(ListingProgress) + Sync)>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let mut entries = self.inner.list_directory(path, on_progress).await?;
            if self.hides_links {
                for entry in &mut entries {
                    entry.is_symlink = false;
                }
            }
            Ok(entries)
        })
    }
    fn get_metadata<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<FileEntry, VolumeError>> + Send + 'a>> {
        self.inner.get_metadata(path)
    }
    fn exists<'a>(&'a self, path: &'a Path) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        self.inner.exists(path)
    }
    fn is_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<bool, VolumeError>> + Send + 'a>> {
        if self.follows_links {
            Box::pin(async move {
                std::fs::metadata(self.inner.root().join(path))
                    .map(|m| m.is_dir())
                    .map_err(|e| VolumeError::from_io_at(&e, path))
            })
        } else {
            self.inner.is_directory(path)
        }
    }
    fn entry_kind<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<EntryKind, VolumeError>> + Send + 'a>> {
        self.entry_kind_calls.fetch_add(1, Ordering::Relaxed);
        self.inner.entry_kind(path)
    }
    fn delete<'a>(&'a self, path: &'a Path) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        self.inner.delete(path)
    }
    fn listing_watch_coverage(&self, _path: &Path) -> WatchCoverage {
        if self.watched {
            WatchCoverage::EveryWriter
        } else {
            WatchCoverage::None
        }
    }
    fn scan_for_copy<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<CopyScanResult, VolumeError>> + Send + 'a>> {
        self.inner.scan_for_copy(path)
    }
    fn scan_for_copy_batch_with_boundary<'a>(
        &'a self,
        paths: &'a [PathBuf],
        boundary: &'a ScanBoundary<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<BatchScanResult, VolumeError>> + Send + 'a>> {
        self.inner.scan_for_copy_batch_with_boundary(paths, boundary)
    }
}

/// The tree every cell deletes from: `dir/` holds `plain.txt` and `link`, a
/// link to `outside/`, which holds `keep.txt`. Paths go in volume-relative.
struct Tree {
    dir: TempDir,
}

impl Tree {
    fn plant() -> Self {
        let dir = TempDir::new().expect("tempdir");
        let root = dir.path();
        std::fs::create_dir_all(root.join("dir")).expect("dir");
        std::fs::create_dir_all(root.join("outside")).expect("outside");
        std::fs::write(root.join("dir/plain.txt"), b"PLAIN").expect("plain");
        std::fs::write(root.join("outside/keep.txt"), b"KEEP").expect("keep");
        cmdr_fs::testing::symlink("../outside", root.join("dir/link")).expect("link");
        Self { dir }
    }

    fn root(&self) -> &Path {
        self.dir.path()
    }

    fn volume(&self, follows_links: bool, watched: bool) -> Arc<LinkRig> {
        self.rig(follows_links, watched, false)
    }

    /// A Samba share's shape: SMB, listings that never mark a link.
    fn samba_volume(&self) -> Arc<LinkRig> {
        self.rig(false, false, true)
    }

    fn rig(&self, follows_links: bool, watched: bool, hides_links: bool) -> Arc<LinkRig> {
        Arc::new(LinkRig {
            inner: LocalPosixVolume::local_folder(
                "V",
                self.root().to_path_buf(),
                crate::host::testing::pane_order_host(),
            ),
            follows_links,
            watched,
            hides_links,
            entry_kind_calls: AtomicUsize::new(0),
        })
    }

    fn assert_target_intact(&self) {
        assert_eq!(
            std::fs::read(self.root().join("outside/keep.txt")).ok().as_deref(),
            Some(&b"KEEP"[..]),
            "the link's target must keep its contents"
        );
    }

    fn assert_link_gone(&self) {
        assert!(
            std::fs::symlink_metadata(self.root().join("dir/link")).is_err(),
            "the link itself must be deleted"
        );
    }
}

fn unique(suffix: &str) -> String {
    static N: AtomicU64 = AtomicU64::new(0);
    format!(
        "dellink_{}_{}_{}",
        suffix,
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    )
}

fn make_state(engine: &TestOperations) -> Arc<WriteOperationState> {
    Arc::new(engine.new_operation_state(Duration::from_millis(50)))
}

async fn delete(
    engine: &TestOperations,
    volume: Arc<LinkRig>,
    volume_id: &str,
    sources: &[PathBuf],
    preview_id: Option<String>,
) {
    let events = CollectorEventSink::new();
    let state = make_state(engine);
    let config = WriteOperationConfig {
        preview_id,
        ..WriteOperationConfig::default()
    };
    let result = delete_volume_files_with_progress_inner(
        volume as Arc<dyn Volume>,
        volume_id,
        &events,
        "op-delete-link",
        &state,
        sources,
        &config,
    )
    .await;
    assert!(result.is_ok(), "the delete must succeed: {result:?}");
}

/// A pane's listing of `parent`, built from the volume's own listing (which
/// sets `is_directory` on a link to a folder, as every pane's does).
#[must_use = "bind the guard; dropping it immediately removes the listing"]
async fn pane_shows(panes: &PaneListings, volume: &LinkRig, volume_id: &str, parent: &str) -> TestListingGuard {
    let entries = volume.list_directory(Path::new(parent), None).await.expect("list");
    let link = entries
        .iter()
        .find(|e| e.name == "link")
        .expect("the listing holds the link");
    assert!(
        link.is_directory && link.is_symlink,
        "precondition: a pane lists the link as a folder"
    );
    TestListing::new()
        .volume(volume_id)
        .path(volume.inner.root().join(parent))
        .entries(entries)
        .sequence(1)
        .insert(panes.panes(), "listing")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_folder_link_inside_a_deleted_folder_goes_as_a_link_and_its_target_stays() {
    let tree = Tree::plant();
    let engine = TestOperations::new();
    let vid = unique("nested");
    let volume = tree.volume(false, false);
    engine.volumes().register(&vid, volume.clone() as Arc<dyn Volume>);

    delete(&engine, volume, &vid, &[PathBuf::from("dir")], None).await;

    tree.assert_target_intact();
    assert!(!tree.root().join("dir").exists(), "the selected folder goes");
}

/// Samba lists a link as the folder it points to (#613), so the walk can't
/// trust a listing's word for any folder there: it stats each one before it
/// descends, ONE `entry_kind` per folder.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_folder_link_a_listing_hides_inside_a_deleted_folder_goes_as_a_link_and_its_target_stays() {
    let tree = Tree::plant();
    std::fs::create_dir_all(tree.root().join("dir/sub")).expect("sub");
    std::fs::write(tree.root().join("dir/sub/deep.txt"), b"DEEP").expect("deep");
    let engine = TestOperations::new();
    let vid = unique("samba");
    let volume = tree.samba_volume();
    engine.volumes().register(&vid, volume.clone() as Arc<dyn Volume>);
    let listed = volume.list_directory(Path::new("dir"), None).await.expect("list");
    let link = listed.iter().find(|e| e.name == "link").expect("listed");
    assert!(
        link.is_directory && !link.is_symlink,
        "precondition: the listing calls the link a plain folder"
    );

    delete(&engine, volume.clone(), &vid, &[PathBuf::from("dir")], None).await;

    tree.assert_target_intact();
    assert!(!tree.root().join("dir").exists(), "the selected folder goes");
    assert_eq!(
        volume.entry_kind_calls.load(Ordering::Relaxed),
        3,
        "one stat for the selected folder, then one per folder it lists (`sub`, `link`)"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_selected_folder_link_on_a_link_following_backend_goes_as_a_link() {
    let tree = Tree::plant();
    let engine = TestOperations::new();
    let vid = unique("following");
    let volume = tree.volume(true, false);
    engine.volumes().register(&vid, volume.clone() as Arc<dyn Volume>);

    delete(&engine, volume, &vid, &[PathBuf::from("dir/link")], None).await;

    tree.assert_target_intact();
    tree.assert_link_gone();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_selected_folder_link_whose_parent_a_pane_shows_goes_as_a_link() {
    let tree = Tree::plant();
    let panes = PaneListings::new();
    let engine = TestOperations::seeing(&panes);
    let vid = unique("parent_oracle");
    let volume = tree.volume(false, true);
    engine.volumes().register(&vid, volume.clone() as Arc<dyn Volume>);
    panes.show_volume(&vid, volume.clone() as Arc<dyn Volume>);
    let _listing = pane_shows(&panes, &volume, &vid, "dir").await;
    let source = tree.root().join("dir/link");

    delete(&engine, volume, &vid, &[source], None).await;

    tree.assert_target_intact();
    tree.assert_link_gone();
}

/// A pane's listing of a Samba share calls a selected link a plain folder, so
/// it can't vouch for a folder there: the walker asks `entry_kind` instead.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_selected_folder_link_a_pane_lists_as_a_folder_goes_as_a_link() {
    let tree = Tree::plant();
    let panes = PaneListings::new();
    let engine = TestOperations::seeing(&panes);
    let vid = unique("hidden_parent_oracle");
    let volume = tree.rig(false, true, true);
    engine.volumes().register(&vid, volume.clone() as Arc<dyn Volume>);
    panes.show_volume(&vid, volume.clone() as Arc<dyn Volume>);
    let _listing = pane_shows_hidden_link(&panes, &volume, &vid, "dir").await;

    delete(&engine, volume, &vid, &[tree.root().join("dir/link")], None).await;

    tree.assert_target_intact();
    tree.assert_link_gone();
}

/// The delete dialog's count over the same pane: the selected link is one item.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_delete_preview_counts_a_selected_link_a_pane_lists_as_a_folder_as_one_leaf() {
    let tree = Tree::plant();
    let panes = PaneListings::new();
    let vid = unique("hidden_preview_count");
    let volume = tree.rig(false, true, true);
    panes.show_volume(&vid, volume.clone() as Arc<dyn Volume>);
    let _listing = pane_shows_hidden_link(&panes, &volume, &vid, "dir").await;
    let sources = vec![tree.root().join("dir/link")];
    let mut batch = volume.scan_for_copy_batch(&sources).await.expect("scan");

    count_selected_links_as_links(panes.as_ref(), volume.as_ref(), &vid, &mut batch).await;

    let (_, link) = &batch.per_path[0];
    assert!(!link.top_level_is_directory, "a link isn't a folder to a delete");
    assert_eq!((link.file_count, link.dir_count, link.total_bytes), (1, 0, 0));
}

/// [`pane_shows`] for a rig whose listings hide links.
#[must_use = "bind the guard; dropping it immediately removes the listing"]
async fn pane_shows_hidden_link(
    panes: &PaneListings,
    volume: &LinkRig,
    volume_id: &str,
    parent: &str,
) -> TestListingGuard {
    let entries = volume.list_directory(Path::new(parent), None).await.expect("list");
    let link = entries.iter().find(|e| e.name == "link").expect("listed");
    assert!(
        link.is_directory && !link.is_symlink,
        "precondition: the pane lists the link as a plain folder"
    );
    TestListing::new()
        .volume(volume_id)
        .path(volume.inner.root().join(parent))
        .entries(entries)
        .sequence(1)
        .insert(panes.panes(), "listing")
}

/// A preview from the backend's own batch scan, which follows a selected link
/// (a copy's preflight needs that), so it says "directory" for the link.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_selected_folder_link_with_a_cold_preview_goes_as_a_link() {
    let tree = Tree::plant();
    let engine = TestOperations::new();
    let vid = unique("cold_preview");
    let volume = tree.volume(false, false);
    engine.volumes().register(&vid, volume.clone() as Arc<dyn Volume>);
    let sources = vec![PathBuf::from("dir/link")];
    let batch = volume.scan_for_copy_batch(&sources).await.expect("scan");
    assert!(
        batch.per_path[0].1.top_level_is_directory,
        "precondition: the backend's scan follows the selected link"
    );
    let preview_id = unique("preview");
    engine.stores().previews().insert_scan_result(
        preview_id.clone(),
        CachedScanResult::from_volume_batch(
            crate::types::ScanPreviewPurpose::Delete,
            sources.clone(),
            batch.aggregate.file_count,
            batch.aggregate.total_bytes,
            batch.aggregate.dedup_bytes,
            batch.per_path,
        ),
    );

    delete(&engine, volume, &vid, &sources, Some(preview_id)).await;

    tree.assert_target_intact();
    tree.assert_link_gone();
}

/// The delete dialog's count: a selected link is one item, with none of its
/// target's files, whichever way the batch scan counted it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_delete_preview_counts_a_selected_folder_link_as_one_leaf() {
    let tree = Tree::plant();
    let panes = PaneListings::new();
    let vid = unique("preview_count");
    let volume = tree.volume(false, false);
    let sources = vec![PathBuf::from("dir/link"), PathBuf::from("dir/plain.txt")];
    let mut batch = volume.scan_for_copy_batch(&sources).await.expect("scan");
    assert_eq!(
        batch.aggregate.total_bytes, 9,
        "precondition: the scan counted keep.txt behind the link"
    );

    count_selected_links_as_links(panes.as_ref(), volume.as_ref(), &vid, &mut batch).await;

    let (_, link) = &batch.per_path[0];
    assert!(!link.top_level_is_directory, "a link isn't a folder to a delete");
    assert_eq!((link.file_count, link.dir_count, link.total_bytes), (1, 0, 0));
    assert_eq!(batch.aggregate.file_count, 2, "the link and plain.txt");
    assert_eq!(batch.aggregate.total_bytes, 5, "only plain.txt's bytes");
}
