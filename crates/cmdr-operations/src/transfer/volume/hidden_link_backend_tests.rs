//! The volume engines on a backend whose LISTINGS never mark a link: a Samba
//! share lists a Unix symlink as the folder it points to, with no
//! `is_symlink`, while its stat (`entry_kind`) does report the link
//! (`crates/cmdr-smb/DETAILS.md` § "Junctions and symlinks", #613).
//!
//! `link_following_backend_tests.rs` covers a backend whose STAT follows a
//! link; its listings still mark one, so a walk could trust them. Here a walk
//! that takes a listed folder at its word merges through a link: it renames
//! the target's files out of their folder, or lands incoming files in it.
//!
//! The rig is a real `LocalPosixVolume` over a tempdir (real links, real
//! `rename`), reporting `BackendKind::Smb` and stripping `is_symlink` from
//! every listing.

#![cfg(unix)]

use super::copy::copy_volumes_with_progress;
use super::move_cross::move_volumes_with_progress;
use super::move_same::move_within_same_volume_with_progress;
use super::rename_merge_test_support::{exists, make_state, mkdir, read, write_file};
use crate::event_sinks::CollectorEventSink;
use crate::testing::TestOperations;
use crate::types::{ConflictResolution, VolumeCopyConfig};
use cmdr_fs::entry::FileEntry;
use cmdr_fs::volume::{
    BackendKind, BatchScanResult, EntryKind, InMemoryVolume, ListingProgress, ScanBoundary, Volume, VolumeError,
};
use cmdr_local::LocalPosixVolume;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use tempfile::TempDir;

/// A `LocalPosixVolume` that lists the way a Samba share does.
struct HiddenLinkVolume {
    inner: LocalPosixVolume,
}

impl Volume for HiddenLinkVolume {
    crate::forward_volume_methods!(
        inner => name,
        root,
        get_metadata,
        exists,
        is_directory,
        create_file,
        create_directory,
        delete,
        rename,
        get_space_info,
        scan_for_copy,
        open_read_stream,
        write_from_stream,
    );

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn backend_kind(&self) -> BackendKind {
        BackendKind::Smb
    }
    fn supports_streaming(&self) -> bool {
        true
    }
    fn list_directory<'a>(
        &'a self,
        path: &'a Path,
        on_progress: Option<&'a (dyn Fn(ListingProgress) + Sync)>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let mut entries = self.inner.list_directory(path, on_progress).await?;
            for entry in &mut entries {
                entry.is_symlink = false;
            }
            Ok(entries)
        })
    }
    /// The stat names the link, as Samba 4.23's does.
    fn entry_kind<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<EntryKind, VolumeError>> + Send + 'a>> {
        self.inner.entry_kind(path)
    }
    fn scan_for_copy_batch_with_boundary<'a>(
        &'a self,
        paths: &'a [PathBuf],
        boundary: &'a ScanBoundary<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<BatchScanResult, VolumeError>> + Send + 'a>> {
        self.inner.scan_for_copy_batch_with_boundary(paths, boundary)
    }
}

fn samba_volume() -> (Arc<dyn Volume>, TempDir) {
    let dir = TempDir::new().unwrap();
    let volume: Arc<dyn Volume> = Arc::new(HiddenLinkVolume {
        inner: LocalPosixVolume::local_folder("V", dir.path().to_path_buf(), crate::host::testing::pane_order_host()),
    });
    (volume, dir)
}

/// A folder holding one file, OUTSIDE both `src` and `dst`, so anything that
/// reaches it visibly reached past the transfer.
fn plant_target(root: &Path) {
    write_file(root, "outside/target/inside.txt", b"OUTSIDE THE SELECTION");
}

fn link(root: &Path, at: &str, to: &str) {
    cmdr_fs::testing::symlink(root.join(to), root.join(at)).expect("symlink");
}

fn is_link(root: &Path, rel: &str) -> bool {
    std::fs::symlink_metadata(root.join(rel))
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false)
}

fn assert_target_untouched(root: &Path) {
    assert_eq!(
        read(root, "outside/target/inside.txt"),
        b"OUTSIDE THE SELECTION",
        "the link's target must keep its own file"
    );
    let names: Vec<_> = std::fs::read_dir(root.join("outside/target"))
        .expect("list the target")
        .map(|e| e.expect("entry").file_name())
        .collect();
    assert_eq!(names.len(), 1, "nothing may land in the link's target, got {names:?}");
}

fn config(resolution: ConflictResolution) -> VolumeCopyConfig {
    VolumeCopyConfig {
        conflict_resolution: resolution,
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    }
}

async fn move_album_within(engine: &TestOperations, volume: &Arc<dyn Volume>, op_id: &str) {
    let result = move_within_same_volume_with_progress(
        Arc::new(CollectorEventSink::new()),
        op_id,
        &make_state(engine),
        Arc::clone(volume),
        &[PathBuf::from("src/album")],
        Path::new("dst"),
        &config(ConflictResolution::Skip),
    )
    .await;
    assert!(result.is_ok(), "{op_id}: expected Ok, got {result:?}");
}

/// The rename-merge's SOURCE side: a link inside the moved folder meets a real
/// folder of its name. Merging it would rename the target's file out of it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_merged_move_never_merges_a_hidden_source_link_into_a_real_folder() {
    let engine = TestOperations::new();
    let (volume, dir) = samba_volume();
    let root = dir.path();
    plant_target(root);
    write_file(root, "src/album/plain.txt", b"PLAIN");
    link(root, "src/album/link", "outside/target");
    mkdir(root, "dst/album/link");

    move_album_within(&engine, &volume, "op-hidden-merge-source-link").await;

    assert_target_untouched(root);
    assert!(is_link(root, "src/album/link"), "the skipped link stays home, a link");
    assert_eq!(
        std::fs::read_dir(root.join("dst/album/link")).unwrap().count(),
        0,
        "the destination's real folder stays empty"
    );
    assert_eq!(read(root, "dst/album/plain.txt"), b"PLAIN", "the sibling still moves");
}

/// The rename-merge's DESTINATION side: a real folder meets a link of its name.
/// Merging into it would rename the source's files into the link's target.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_merged_move_never_merges_into_a_hidden_destination_link() {
    let engine = TestOperations::new();
    let (volume, dir) = samba_volume();
    let root = dir.path();
    plant_target(root);
    write_file(root, "src/album/sub/mine.txt", b"MINE");
    mkdir(root, "dst/album");
    link(root, "dst/album/sub", "outside/target");

    move_album_within(&engine, &volume, "op-hidden-merge-dest-link").await;

    assert_target_untouched(root);
    assert_eq!(
        read(root, "src/album/sub/mine.txt"),
        b"MINE",
        "the skipped folder stays home"
    );
    assert!(is_link(root, "dst/album/sub"), "the destination link is untouched");
}

/// The TOP level: a selected link meeting a real folder of its name. The move's
/// hints come from listing the source's parent, which calls the link a folder.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_moved_hidden_link_never_merges_into_a_real_folder() {
    let engine = TestOperations::new();
    let (volume, dir) = samba_volume();
    let root = dir.path();
    plant_target(root);
    mkdir(root, "src");
    link(root, "src/album", "outside/target");
    mkdir(root, "dst/album");

    move_album_within(&engine, &volume, "op-hidden-top-link").await;

    assert_target_untouched(root);
    assert!(is_link(root, "src/album"), "the skipped link stays home, a link");
    assert_eq!(
        std::fs::read_dir(root.join("dst/album")).unwrap().count(),
        0,
        "the destination's real folder stays empty"
    );
}

/// A copy ONTO the share: the destination level's listing calls a link a
/// folder, and merging into it lands the incoming files in its target.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_copy_never_merges_into_a_hidden_destination_link() {
    let engine = TestOperations::new();
    let (dest, dir) = samba_volume();
    let root = dir.path();
    plant_target(root);
    mkdir(root, "dst/album");
    link(root, "dst/album/link", "outside/target");
    let source: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("Source").with_space_info(10_000_000, 10_000_000));
    for dir in ["/album", "/album/link"] {
        source.create_directory(Path::new(dir)).await.expect("source dir");
    }
    for (path, content) in [
        ("/album/plain.txt", b"PLAIN".as_slice()),
        ("/album/link/new.txt", b"NEW"),
    ] {
        source.create_file(Path::new(path), content).await.expect("source file");
    }

    let result = copy_volumes_with_progress(
        Arc::new(CollectorEventSink::new()),
        "op-hidden-copy-dest-link",
        &make_state(&engine),
        source,
        &[PathBuf::from("/album")],
        Arc::clone(&dest),
        Path::new("dst"),
        &config(ConflictResolution::Skip),
    )
    .await;
    assert!(result.is_ok(), "expected Ok, got {result:?}");

    assert_target_untouched(root);
    assert!(is_link(root, "dst/album/link"), "the link is still the link");
    assert_eq!(
        read(root, "dst/album/plain.txt"),
        b"PLAIN",
        "the rest of the folder merged"
    );
}

/// A move OFF the share: the copy walk follows the hidden link (a cross-volume
/// transfer materializes one), and the source sweep, which asks `entry_kind` of
/// every folder, removes the link and spares its target.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_cross_volume_move_of_a_folder_holding_a_hidden_link_spares_the_target() {
    let engine = TestOperations::new();
    let (source, dir) = samba_volume();
    let root = dir.path();
    plant_target(root);
    write_file(root, "src/album/plain.txt", b"PLAIN");
    link(root, "src/album/link", "outside/target");
    let dest: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("Dest").with_space_info(10_000_000, 10_000_000));

    let result = move_volumes_with_progress(
        Arc::new(CollectorEventSink::new()),
        "op-hidden-cross-move",
        &make_state(&engine),
        Arc::clone(&source),
        &[PathBuf::from("src/album")],
        Arc::clone(&dest),
        Path::new("/"),
        &config(ConflictResolution::Skip),
    )
    .await;
    assert!(result.is_ok(), "expected Ok, got {result:?}");

    assert_target_untouched(root);
    assert!(!exists(root, "src/album"), "the moved folder is gone, link and all");
    assert!(
        dest.exists(Path::new("/album/link/inside.txt")).await,
        "the copy carried the target's file"
    );
}
