//! The volume engines (the moves, and the merge walk every copy runs) on a
//! backend whose `is_directory` FOLLOWS a link.
//!
//! `LocalPosixVolume` answers `is_directory` with an `lstat`, so a suite on it
//! can't see what an ADB phone or an SFTP server does: their stat follows the
//! link, and a link to a folder answers "directory". Anything that recursed on
//! that answer walked into the link's target and deleted or wrote there, a
//! folder the user never selected (#140). `Volume::entry_kind` is the question
//! that can't be fooled, and these cells pin that every recursing site asks it.
//!
//! The rig is a real `LocalPosixVolume` over a tempdir (real links, real
//! `rename`), behind a wrapper that keeps the trait's DEFAULT `is_directory` and
//! `entry_kind`, the two answers a stat-following backend gets.

#![cfg(unix)]

use super::super::conflict_responder_test_support::ConflictResponderSink;
use super::copy::copy_volumes_with_progress;
use super::move_cross::move_volumes_with_progress;
use super::move_same::move_within_same_volume_with_progress;
use super::rename_merge_test_support::{exists, make_state, mkdir, read, write_file};
use crate::file_system::listing::FileEntry;
use crate::file_system::volume::{
    CopyScanResult, InMemoryVolume, ListingProgress, LocalPosixVolume, SpaceInfo, StreamLength, StreamWriteProgress,
    Volume, VolumeError, VolumeReadStream, WriteMode,
};
use crate::file_system::write_operations::event_sinks::{CollectorEventSink, OperationEventSink};
use crate::file_system::write_operations::state::WriteOperationState;
use crate::file_system::write_operations::types::{ConflictResolution, VolumeCopyConfig};
use crate::ignore_poison::IgnorePoison;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use tempfile::TempDir;

/// A `LocalPosixVolume` minus its `lstat`-based `is_directory` and `entry_kind`
/// overrides: both fall to the trait defaults, which read `get_metadata`, whose
/// `is_directory` is true for a link to a folder (with `is_symlink` beside it).
struct LinkFollowingVolume {
    inner: LocalPosixVolume,
    /// `Some((name, target))` stages a race: the moment `name` is renamed away
    /// (a conflict resolution setting it aside), a link to `target` takes the
    /// freed name, as another writer could between the resolver and the landing.
    link_takes_freed_name: Option<(PathBuf, PathBuf)>,
}

impl Volume for LinkFollowingVolume {
    fn name(&self) -> &str {
        self.inner.name()
    }
    fn root(&self) -> &Path {
        self.inner.root()
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn supports_streaming(&self) -> bool {
        true
    }
    fn list_directory<'a>(
        &'a self,
        path: &'a Path,
        on_progress: Option<&'a (dyn Fn(ListingProgress) + Sync)>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
        self.inner.list_directory(path, on_progress)
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
    fn delete<'a>(&'a self, path: &'a Path) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        self.inner.delete(path)
    }
    fn rename<'a>(
        &'a self,
        from: &'a Path,
        to: &'a Path,
        force: bool,
    ) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            self.inner.rename(from, to, force).await?;
            if let Some((name, target)) = &self.link_takes_freed_name
                && from == name
            {
                std::os::unix::fs::symlink(target, self.inner.root().join(name)).expect("racing symlink");
            }
            Ok(())
        })
    }
    fn create_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        self.inner.create_directory(path)
    }
    fn scan_for_copy<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<CopyScanResult, VolumeError>> + Send + 'a>> {
        self.inner.scan_for_copy(path)
    }
    fn open_read_stream<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<Box<dyn VolumeReadStream>, VolumeError>> + Send + 'a>> {
        self.inner.open_read_stream(path)
    }
    fn write_from_stream<'a>(
        &'a self,
        dest: &'a Path,
        mode: WriteMode,
        length: StreamLength,
        stream: Box<dyn VolumeReadStream>,
        on_progress: &'a (dyn Fn(StreamWriteProgress) -> std::ops::ControlFlow<()> + Sync),
    ) -> Pin<Box<dyn Future<Output = Result<u64, VolumeError>> + Send + 'a>> {
        self.inner.write_from_stream(dest, mode, length, stream, on_progress)
    }
    fn get_space_info<'a>(&'a self) -> Pin<Box<dyn Future<Output = Result<SpaceInfo, VolumeError>> + Send + 'a>> {
        self.inner.get_space_info()
    }
}

fn following_volume() -> (Arc<dyn Volume>, TempDir) {
    let dir = TempDir::new().unwrap();
    let volume: Arc<dyn Volume> = Arc::new(LinkFollowingVolume {
        inner: LocalPosixVolume::local_folder("V", dir.path().to_path_buf()),
        link_takes_freed_name: None,
    });
    (volume, dir)
}

/// A same-volume merge's folder child answered Overwrite against a FILE: the
/// resolver sets the file aside, and before the folder lands a LINK to a folder
/// takes the freed name. The landing must not read that link as "a directory is
/// here now, merge into it": that renames the source's files into its target.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_link_racing_into_a_freed_name_is_never_merged_into() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    let volume: Arc<dyn Volume> = Arc::new(LinkFollowingVolume {
        inner: LocalPosixVolume::local_folder("V", root.to_path_buf()),
        link_takes_freed_name: Some((PathBuf::from("dst/album/thing"), root.join("outside/target"))),
    });
    plant_target(root);
    write_file(root, "src/album/thing/mine.txt", b"MINE");
    write_file(root, "dst/album/thing", b"THE USER'S FILE");
    let state = make_state();
    let events = Arc::new(ConflictResponderSink::new(&state, ConflictResolution::Overwrite, false));

    let result = move_within_same_volume_with_progress(
        events,
        "op-follow-racing-link",
        &state,
        Arc::clone(&volume),
        &[PathBuf::from("src/album")],
        Path::new("dst"),
        &config(ConflictResolution::Stop),
    )
    .await;

    assert!(is_link(root, "dst/album/thing"), "the race was staged: {result:?}");
    assert_eq!(read(root, "outside/target/inside.txt"), b"OUTSIDE THE SELECTION");
    assert!(
        !exists(root, "outside/target/mine.txt"),
        "nothing may land in the link's target, got {result:?}"
    );
    assert_eq!(
        read(root, "src/album/thing/mine.txt"),
        b"MINE",
        "the folder that couldn't land stays home"
    );
    assert!(result.is_err(), "the taken name refuses the landing, got {result:?}");
    let kept = std::fs::read_dir(root.join("dst/album"))
        .expect("list dst/album")
        .map(|e| e.expect("entry").path())
        .filter(|p| std::fs::symlink_metadata(p).is_ok_and(|m| m.is_file()))
        .any(|p| std::fs::read(p).expect("read") == b"THE USER'S FILE");
    assert!(kept, "the file that was set aside is still in the folder");
}

/// The shared fixture: a target folder holding one file, OUTSIDE the selection.
fn plant_target(root: &Path) {
    write_file(root, "outside/target/inside.txt", b"OUTSIDE THE SELECTION");
}

fn link(root: &Path, at: &str, to: &str) {
    std::os::unix::fs::symlink(root.join(to), root.join(at)).expect("symlink");
}

fn is_link(root: &Path, rel: &str) -> bool {
    std::fs::symlink_metadata(root.join(rel))
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false)
}

fn config(resolution: ConflictResolution) -> VolumeCopyConfig {
    VolumeCopyConfig {
        conflict_resolution: resolution,
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    }
}

async fn move_across(source: &Arc<dyn Volume>, op_id: &str, sources: &[PathBuf]) -> Arc<dyn Volume> {
    let dest: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("Dest").with_space_info(10_000_000, 10_000_000));
    let result = move_volumes_with_progress(
        Arc::new(CollectorEventSink::new()),
        op_id,
        &make_state(),
        Arc::clone(source),
        sources,
        Arc::clone(&dest),
        Path::new("/"),
        &config(ConflictResolution::Skip),
    )
    .await;
    assert!(result.is_ok(), "{op_id}: expected Ok, got {result:?}");
    dest
}

/// A selected link moved to another volume lands there as a copy of what it
/// points at (the destination can't hold a link), and the source sweep removes
/// the LINK. The target keeps every byte.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_cross_volume_move_of_a_dir_link_removes_the_link_never_the_target() {
    let (source, dir) = following_volume();
    let root = dir.path();
    plant_target(root);
    mkdir(root, "src");
    link(root, "src/album", "outside/target");

    let dest = move_across(&source, "op-follow-cross-top-link", &[PathBuf::from("src/album")]).await;

    assert_eq!(
        read(root, "outside/target/inside.txt"),
        b"OUTSIDE THE SELECTION",
        "the source sweep must delete the link, never walk into its target"
    );
    assert!(
        !is_link(root, "src/album") && !exists(root, "src/album"),
        "the moved link is gone"
    );
    assert!(
        dest.exists(Path::new("/album/inside.txt")).await,
        "the destination got the copy"
    );
}

/// The same sweep one level down: a moved folder holding a link to a folder.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_cross_volume_move_of_a_folder_holding_a_dir_link_spares_the_target() {
    let (source, dir) = following_volume();
    let root = dir.path();
    plant_target(root);
    write_file(root, "src/album/plain.txt", b"PLAIN");
    link(root, "src/album/link", "outside/target");

    let dest = move_across(&source, "op-follow-cross-child-link", &[PathBuf::from("src/album")]).await;

    assert_eq!(read(root, "outside/target/inside.txt"), b"OUTSIDE THE SELECTION");
    assert!(!exists(root, "src/album"), "the moved folder is gone, link and all");
    assert!(dest.exists(Path::new("/album/plain.txt")).await);
}

/// A same-volume move of a real folder onto a same-named LINK to a folder: the
/// destination's type answer must not follow the link, or the merge renames the
/// source's files into the link's target.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_same_volume_move_onto_a_dir_link_never_lands_in_the_target() {
    let (volume, dir) = following_volume();
    let root = dir.path();
    plant_target(root);
    write_file(root, "src/album/mine.txt", b"MINE");
    mkdir(root, "dst");
    link(root, "dst/album", "outside/target");

    let result = move_within_same_volume_with_progress(
        Arc::new(CollectorEventSink::new()),
        "op-follow-same-dest-link",
        &make_state(),
        Arc::clone(&volume),
        &[PathBuf::from("src/album")],
        Path::new("dst"),
        &config(ConflictResolution::Skip),
    )
    .await;
    assert!(result.is_ok(), "expected Ok, got {result:?}");

    assert!(
        !exists(root, "outside/target/mine.txt"),
        "nothing may land in the link's target"
    );
    assert_eq!(
        read(root, "src/album/mine.txt"),
        b"MINE",
        "the skipped folder stays home"
    );
    assert!(is_link(root, "dst/album"), "the destination link is untouched");
}

// The same two moves on a PLAIN `LocalPosixVolume` (an `lstat` backend), plus
// the stamp-then-sweep pair an into-zip move uses, on both kinds of backend. A
// move's source sweep deletes a LEDGER (`source_sweep.rs`), and a link in it is a
// leaf: the link goes, nothing under it is re-listed or deleted.

fn plain_volume() -> (Arc<dyn Volume>, TempDir) {
    let dir = TempDir::new().unwrap();
    let volume: Arc<dyn Volume> = Arc::new(LocalPosixVolume::local_folder("V", dir.path().to_path_buf()));
    (volume, dir)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn on_an_lstat_backend_a_moved_dir_link_goes_as_the_link() {
    let (source, dir) = plain_volume();
    let root = dir.path();
    plant_target(root);
    mkdir(root, "src");
    link(root, "src/album", "outside/target");

    move_across(&source, "op-plain-cross-top-link", &[PathBuf::from("src/album")]).await;

    assert_eq!(read(root, "outside/target/inside.txt"), b"OUTSIDE THE SELECTION");
    assert!(!is_link(root, "src/album"), "the moved link is gone");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn on_an_lstat_backend_a_moved_folder_holding_a_dir_link_spares_the_target() {
    let (source, dir) = plain_volume();
    let root = dir.path();
    plant_target(root);
    write_file(root, "src/album/plain.txt", b"PLAIN");
    link(root, "src/album/link", "outside/target");

    move_across(&source, "op-plain-cross-child-link", &[PathBuf::from("src/album")]).await;

    assert_eq!(read(root, "outside/target/inside.txt"), b"OUTSIDE THE SELECTION");
    assert!(!exists(root, "src/album/plain.txt"), "the carried file went");
}

/// The into-zip pair: `stamp_source` never walks through a link, so the sweep
/// finds it uncarried and leaves it, and the target is never touched.
async fn stamp_then_sweep_spares_a_link_target(source: Arc<dyn Volume>, root: &Path) {
    plant_target(root);
    write_file(root, "src/album/plain.txt", b"PLAIN");
    link(root, "src/album/link", "outside/target");

    let album = Path::new("src/album");
    let carried = super::source_sweep::stamp_source(&source, album).await.expect("stamp");
    let left = super::source_sweep::sweep_carried_source(&source, album, &carried, &Default::default())
        .await
        .expect("sweep");

    assert_eq!(read(root, "outside/target/inside.txt"), b"OUTSIDE THE SELECTION");
    assert!(!exists(root, "src/album/plain.txt"), "the carried file went");
    assert!(
        is_link(root, "src/album/link"),
        "the link was never carried, so it stays"
    );
    assert_eq!(left.appeared, 1, "and it's reported as left behind");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stamp_then_sweep_spares_a_link_target_on_an_lstat_backend() {
    let (source, dir) = plain_volume();
    stamp_then_sweep_spares_a_link_target(source, dir.path()).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stamp_then_sweep_spares_a_link_target_on_a_following_backend() {
    let (source, dir) = following_volume();
    stamp_then_sweep_spares_a_link_target(source, dir.path()).await;
}

// A folder merge INTO a destination that holds a link to a folder, below the
// top level (#294). The destination listing reports such a link as a directory
// (with `is_symlink` beside it) on the `lstat` backend and the following one
// alike, so a walk that recursed on `is_directory` alone landed the incoming
// files in the link's target: stray files under Skip and Rename, and the
// target's same-named files REPLACED under Overwrite. A link is a leaf at every
// level, and a folder meeting one is a type clash.

/// The incoming tree: `/album/plain.txt`, and `/album/link/` holding a file
/// named like the one in the link's target plus one the target doesn't have.
async fn incoming_album() -> Arc<dyn Volume> {
    let source: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("Source").with_space_info(10_000_000, 10_000_000));
    for dir in ["/album", "/album/link"] {
        source.create_directory(Path::new(dir)).await.expect("source dir");
    }
    for (path, content) in [
        ("/album/plain.txt", b"PLAIN".as_slice()),
        ("/album/link/inside.txt", b"INCOMING"),
        ("/album/link/new.txt", b"NEW"),
    ] {
        source.create_file(Path::new(path), content).await.expect("source file");
    }
    source
}

/// The destination: `dst/album/` already exists and holds `link`, a link to the
/// folder outside the selection.
fn plant_album_holding_a_link(root: &Path) {
    plant_target(root);
    mkdir(root, "dst/album");
    link(root, "dst/album/link", "outside/target");
}

/// What every policy owes the folder the link points at: the file it held keeps
/// its bytes, and nothing new arrives.
fn assert_target_untouched(root: &Path, policy: &str) {
    assert_eq!(
        read(root, "outside/target/inside.txt"),
        b"OUTSIDE THE SELECTION",
        "{policy}: the link's target must keep its own file"
    );
    assert!(
        !exists(root, "outside/target/new.txt"),
        "{policy}: nothing may land in the link's target"
    );
}

async fn copy_album_onto(
    dest: &Arc<dyn Volume>,
    op_id: &str,
    events: Arc<dyn OperationEventSink>,
    state: &Arc<WriteOperationState>,
    resolution: ConflictResolution,
) {
    let source = incoming_album().await;
    let result = copy_volumes_with_progress(
        events,
        op_id,
        state,
        source,
        &[PathBuf::from("/album")],
        Arc::clone(dest),
        Path::new("dst"),
        &config(resolution),
    )
    .await;
    assert!(result.is_ok(), "{op_id}: expected Ok, got {result:?}");
}

/// Skip, and a BLANKET Overwrite (which never crosses types): the link stays
/// the link, its target is untouched, and the rest of the folder still merges.
async fn a_merge_leaves_a_deep_dest_link_alone(dest: Arc<dyn Volume>, root: &Path, resolution: ConflictResolution) {
    plant_album_holding_a_link(root);
    let policy = format!("{resolution:?}");

    copy_album_onto(
        &dest,
        &format!("op-deep-dest-link-{policy}"),
        Arc::new(CollectorEventSink::new()),
        &make_state(),
        resolution,
    )
    .await;

    assert_target_untouched(root, &policy);
    assert!(is_link(root, "dst/album/link"), "{policy}: the link is still the link");
    assert_eq!(
        read(root, "dst/album/plain.txt"),
        b"PLAIN",
        "{policy}: the rest of the folder merged"
    );
}

/// Rename: the incoming folder lands beside the link under a free name.
async fn a_renaming_merge_lands_beside_a_deep_dest_link(dest: Arc<dyn Volume>, root: &Path) {
    plant_album_holding_a_link(root);

    copy_album_onto(
        &dest,
        "op-deep-dest-link-rename",
        Arc::new(CollectorEventSink::new()),
        &make_state(),
        ConflictResolution::Rename,
    )
    .await;

    assert_target_untouched(root, "Rename");
    assert!(is_link(root, "dst/album/link"), "the link is still the link");
    assert_eq!(read(root, "dst/album/link (1)/inside.txt"), b"INCOMING");
    assert_eq!(read(root, "dst/album/link (1)/new.txt"), b"NEW");
}

/// An Overwrite a person ANSWERED for this clash: the prompt names a folder
/// landing on a non-folder, and the folder replaces the LINK. The target keeps
/// every byte.
async fn an_answered_overwrite_replaces_a_deep_dest_link_never_its_target(dest: Arc<dyn Volume>, root: &Path) {
    plant_album_holding_a_link(root);
    let state = make_state();
    let events = Arc::new(ConflictResponderSink::new(&state, ConflictResolution::Overwrite, false));

    copy_album_onto(
        &dest,
        "op-deep-dest-link-answered",
        Arc::clone(&events) as Arc<dyn OperationEventSink>,
        &state,
        ConflictResolution::Stop,
    )
    .await;

    {
        let prompts = events.inner.conflicts.lock_ignore_poison();
        assert_eq!(prompts.len(), 1, "one prompt, for the folder meeting the link");
        assert!(
            prompts[0].source_is_directory && !prompts[0].destination_is_directory,
            "the prompt is a folder landing on a non-folder, got {:?}",
            prompts[0]
        );
    }
    assert_target_untouched(root, "answered Overwrite");
    assert!(!is_link(root, "dst/album/link"), "the folder replaced the link");
    assert_eq!(read(root, "dst/album/link/inside.txt"), b"INCOMING");
    assert_eq!(read(root, "dst/album/link/new.txt"), b"NEW");
    let leftovers: Vec<_> = std::fs::read_dir(root.join("dst/album"))
        .expect("list dst/album")
        .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
        .filter(|name| name != "link" && name != "plain.txt")
        .collect();
    assert!(leftovers.is_empty(), "the set-aside link is dropped, got {leftovers:?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_skipping_merge_leaves_a_deep_dest_link_alone_on_a_following_backend() {
    let (dest, dir) = following_volume();
    a_merge_leaves_a_deep_dest_link_alone(dest, dir.path(), ConflictResolution::Skip).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_skipping_merge_leaves_a_deep_dest_link_alone_on_an_lstat_backend() {
    let (dest, dir) = plain_volume();
    a_merge_leaves_a_deep_dest_link_alone(dest, dir.path(), ConflictResolution::Skip).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_blanket_overwrite_merge_leaves_a_deep_dest_link_alone_on_a_following_backend() {
    let (dest, dir) = following_volume();
    a_merge_leaves_a_deep_dest_link_alone(dest, dir.path(), ConflictResolution::Overwrite).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_blanket_overwrite_merge_leaves_a_deep_dest_link_alone_on_an_lstat_backend() {
    let (dest, dir) = plain_volume();
    a_merge_leaves_a_deep_dest_link_alone(dest, dir.path(), ConflictResolution::Overwrite).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_renaming_merge_lands_beside_a_deep_dest_link_on_a_following_backend() {
    let (dest, dir) = following_volume();
    a_renaming_merge_lands_beside_a_deep_dest_link(dest, dir.path()).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_renaming_merge_lands_beside_a_deep_dest_link_on_an_lstat_backend() {
    let (dest, dir) = plain_volume();
    a_renaming_merge_lands_beside_a_deep_dest_link(dest, dir.path()).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_answered_overwrite_replaces_a_deep_dest_link_never_its_target_on_a_following_backend() {
    let (dest, dir) = following_volume();
    an_answered_overwrite_replaces_a_deep_dest_link_never_its_target(dest, dir.path()).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_answered_overwrite_replaces_a_deep_dest_link_never_its_target_on_an_lstat_backend() {
    let (dest, dir) = plain_volume();
    an_answered_overwrite_replaces_a_deep_dest_link_never_its_target(dest, dir.path()).await;
}

/// The same clash one level up, where the top-level pre-check meets it: the
/// selected folder's own name is a link in the destination.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_blanket_overwrite_copy_leaves_a_top_level_dest_link_alone() {
    let (dest, dir) = following_volume();
    let root = dir.path();
    plant_target(root);
    mkdir(root, "dst");
    link(root, "dst/link", "outside/target");
    let source = incoming_album().await;

    let result = copy_volumes_with_progress(
        Arc::new(CollectorEventSink::new()),
        "op-top-dest-link-copy",
        &make_state(),
        source,
        &[PathBuf::from("/album/link")],
        Arc::clone(&dest),
        Path::new("dst"),
        &config(ConflictResolution::Overwrite),
    )
    .await;
    assert!(result.is_ok(), "expected Ok, got {result:?}");

    assert_target_untouched(root, "top level");
    assert!(is_link(root, "dst/link"), "the link is still the link");
}

/// The walk with NO merge context (the scratch pull an into-zip copy of a remote
/// source runs) has no policy to ask, so a folder meeting a link there is
/// refused outright. It never recurses through one.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_policy_free_pull_refuses_a_deep_dest_link_instead_of_walking_through_it() {
    let (dest, dir) = plain_volume();
    let root = dir.path();
    plant_target(root);
    mkdir(root, "scratch/album");
    link(root, "scratch/album/link", "outside/target");
    // Only a name the target doesn't hold, so nothing but the link itself can
    // refuse this: a same-named file would be turned away as an occupied name.
    let source: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("Source").with_space_info(10_000_000, 10_000_000));
    for dir in ["/album", "/album/link"] {
        source.create_directory(Path::new(dir)).await.expect("source dir");
    }
    source
        .create_file(Path::new("/album/link/new.txt"), b"NEW")
        .await
        .expect("source file");

    let result = super::strategy::pull_path_to_local(
        &source,
        Path::new("/album"),
        true,
        &dest,
        Path::new("scratch/album"),
        &make_state(),
    )
    .await;

    assert!(
        matches!(result, Err(VolumeError::AlreadyExists(_))),
        "expected the taken name to be refused, got {result:?}"
    );
    assert_target_untouched(root, "policy-free pull");
    assert!(is_link(root, "scratch/album/link"), "the link is still the link");
}

/// A MOVE rides the same walk, and owes one thing more: the folder it couldn't
/// land stays in the source, because the sweep removes only what was carried.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_cross_volume_move_onto_a_deep_dest_link_keeps_the_target_and_the_unlanded_source() {
    let (dest, dir) = following_volume();
    let root = dir.path();
    plant_album_holding_a_link(root);
    let source = incoming_album().await;

    let result = move_volumes_with_progress(
        Arc::new(CollectorEventSink::new()),
        "op-deep-dest-link-move",
        &make_state(),
        Arc::clone(&source),
        &[PathBuf::from("/album")],
        Arc::clone(&dest),
        Path::new("dst"),
        &config(ConflictResolution::Overwrite),
    )
    .await;
    assert!(result.is_ok(), "expected Ok, got {result:?}");

    assert_target_untouched(root, "move");
    assert!(is_link(root, "dst/album/link"), "the link is still the link");
    assert_eq!(read(root, "dst/album/plain.txt"), b"PLAIN", "the rest moved");
    assert!(
        !source.exists(Path::new("/album/plain.txt")).await,
        "the carried file left the source"
    );
    for kept in ["/album/link/inside.txt", "/album/link/new.txt"] {
        assert!(
            source.exists(Path::new(kept)).await,
            "{kept} never landed, so the move must leave it in the source"
        );
    }
}
