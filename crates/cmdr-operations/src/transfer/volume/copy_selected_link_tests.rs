//! A SELECTED symlink copied to another volume, through the dialog's preview.
//!
//! The copy dialog's local preview walks with the local engines' stance: a link
//! is a leaf, so `~/Dropbox -> ~/Library/CloudStorage/Dropbox` counts as one
//! tiny file. A cross-volume copy materializes a link instead (`transfer/DETAILS.md`
//! § "Symlinks are opaque to a move"), so that preview describes a different
//! transfer than the one about to run. Trusting it streamed the link as a file,
//! and the read stream, which follows the link, refused: "Cannot stream a
//! directory". These cells run the REAL local walk, publish it the way the
//! dialog does, and copy through it.

use crate::testing::TestOperations;
use std::collections::HashSet;
use std::os::unix::fs::symlink;
use std::time::{Duration, Instant};

use super::copy::copy_volumes_with_progress;
use super::move_cross::move_volumes_with_progress;
use super::rename_merge_test_support::make_state;
use crate::event_sinks::CollectorEventSink;
use crate::scan_walker::{WalkContext, walk_sources_with_per_path};
use crate::state::CachedScanResult;
use crate::types::{VolumeCopyConfig, WriteOperationPhase, WriteProgressEvent};
use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::testing::TestDir;
use cmdr_fs::volume::{InMemoryVolume, Volume};
use cmdr_local::LocalPosixVolume;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Walks `sources` with the local preview's walker and publishes the result
/// under `preview_id`, exactly as a settled copy-dialog preview would be.
fn publish_local_preview(engine: &TestOperations, preview_id: &str, sources: &[PathBuf]) {
    let sidecars = cmdr_fs::apple_double::SidecarRule::new();
    let ctx: WalkContext<'_, String> = WalkContext {
        listings: engine.host().listings(),
        park_while_paused: &|| {},
        progress_interval: Duration::from_secs(3600),
        is_cancelled: &|| false,
        on_io_error: &|_, e| e.to_string(),
        on_cancelled: &|| "Cancelled".to_string(),
        on_symlink_loop: &|path| format!("Symlink loop detected: {}", path.display()),
        on_mount_boundary: None,
        on_progress: &|_, _, _, _, _| {},
        on_file: None,
        online_only: None,
        sidecars: &sidecars,
    };
    let (mut files, mut dirs) = (Vec::new(), Vec::new());
    let (mut total_bytes, mut dedup_bytes) = (0u64, 0u64);
    let per_path = walk_sources_with_per_path(
        sources,
        &mut files,
        &mut dirs,
        &mut total_bytes,
        &mut dedup_bytes,
        &mut Instant::now(),
        &mut HashSet::new(),
        &mut HashSet::new(),
        None,
        &ctx,
    )
    .expect("the fixture tree walks cleanly");
    engine.stores().previews().insert_scan_result(
        preview_id.to_string(),
        CachedScanResult::from_local_walk(
            crate::types::ScanPreviewPurpose::Transfer,
            sources.to_vec(),
            files,
            dirs,
            total_bytes,
            dedup_bytes,
            per_path,
            None,
        ),
    );
}

/// The boot volume as production registers it: rooted at `/`, so the
/// preview's absolute paths are the volume's own paths.
fn boot_volume() -> Arc<dyn Volume> {
    Arc::new(LocalPosixVolume::local_folder(
        "Macintosh HD",
        PathBuf::from("/"),
        crate::host::testing::pane_order_host(),
    ))
}

fn dest_volume() -> Arc<dyn Volume> {
    Arc::new(InMemoryVolume::new("NAS").with_space_info(10_000_000, 10_000_000))
}

fn config_with_preview(preview_id: &str) -> VolumeCopyConfig {
    VolumeCopyConfig {
        progress_interval_ms: 0,
        preview_id: Some(preview_id.to_string()),
        ..VolumeCopyConfig::default()
    }
}

/// `target/` holds two files (one nested); `link` points at it.
fn plant_linked_folder(root: &Path) -> PathBuf {
    std::fs::create_dir_all(root.join("target/sub")).unwrap();
    std::fs::write(root.join("target/a.txt"), b"aaa").unwrap();
    std::fs::write(root.join("target/sub/b.txt"), b"bbbbb").unwrap();
    symlink(root.join("target"), root.join("link")).unwrap();
    root.join("link")
}

async fn read_dest(dest: &Arc<dyn Volume>, path: &str) -> Vec<u8> {
    dest.read_range(Path::new(path), 0, 1024)
        .await
        .unwrap_or_else(|e| panic!("{path} should have landed: {e:?}"))
}

/// The ~/Dropbox case: a selected link to a folder lands as that folder, with
/// everything it holds.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_selected_link_to_a_folder_copies_as_the_folder_through_a_preview() {
    let engine = TestOperations::new();
    let dir = TestDir::new("selected-link-folder");
    let link = plant_linked_folder(&dir);
    let preview_id = "selected-link-folder-preview";
    publish_local_preview(&engine, preview_id, std::slice::from_ref(&link));

    let dest = dest_volume();
    let result = copy_volumes_with_progress(
        Arc::new(CollectorEventSink::new()),
        "op-selected-link-folder",
        &make_state(&engine),
        boot_volume(),
        std::slice::from_ref(&link),
        Arc::clone(&dest),
        Path::new("/"),
        &config_with_preview(preview_id),
    )
    .await;

    assert!(result.is_ok(), "expected Ok, got {result:?}");
    assert_eq!(read_dest(&dest, "/link/a.txt").await, b"aaa");
    assert_eq!(read_dest(&dest, "/link/sub/b.txt").await, b"bbbbb");
}

/// The bars and the space check count what the copy MOVES: the folder's two
/// files and eight bytes, never the link's own one entry.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_selected_link_counts_what_it_points_at() {
    let engine = TestOperations::new();
    let dir = TestDir::new("selected-link-totals");
    let link = plant_linked_folder(&dir);
    let preview_id = "selected-link-totals-preview";
    publish_local_preview(&engine, preview_id, std::slice::from_ref(&link));

    let events = Arc::new(CollectorEventSink::new());
    let result = copy_volumes_with_progress(
        Arc::clone(&events) as _,
        "op-selected-link-totals",
        &make_state(&engine),
        boot_volume(),
        std::slice::from_ref(&link),
        dest_volume(),
        Path::new("/"),
        &config_with_preview(preview_id),
    )
    .await;
    assert!(result.is_ok(), "expected Ok, got {result:?}");

    let copying = last_copying_tick(&events);
    assert_eq!(copying.files_total, 2, "two files behind the link");
    assert_eq!(copying.bytes_total, 8, "their bytes, not the link's");
}

fn last_copying_tick(events: &CollectorEventSink) -> WriteProgressEvent {
    events
        .progress
        .lock_ignore_poison()
        .iter()
        .rfind(|e| e.phase == WriteOperationPhase::Copying)
        .cloned()
        .expect("the copy reports progress")
}

fn last_copying_bytes_total(events: &CollectorEventSink) -> u64 {
    last_copying_tick(events).bytes_total
}

/// A selected link to a FILE streams the file's bytes, and counts them.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_selected_link_to_a_file_copies_the_file_through_a_preview() {
    let engine = TestOperations::new();
    let dir = TestDir::new("selected-link-file");
    std::fs::write(dir.join("real.txt"), b"the real bytes").unwrap();
    symlink(dir.join("real.txt"), dir.join("alias.txt")).unwrap();
    let link = dir.join("alias.txt");
    let preview_id = "selected-link-file-preview";
    publish_local_preview(&engine, preview_id, std::slice::from_ref(&link));

    let dest = dest_volume();
    let events = Arc::new(CollectorEventSink::new());
    let result = copy_volumes_with_progress(
        Arc::clone(&events) as _,
        "op-selected-link-file",
        &make_state(&engine),
        boot_volume(),
        std::slice::from_ref(&link),
        Arc::clone(&dest),
        Path::new("/"),
        &config_with_preview(preview_id),
    )
    .await;

    assert!(result.is_ok(), "expected Ok, got {result:?}");
    assert_eq!(read_dest(&dest, "/alias.txt").await, b"the real bytes");
    assert_eq!(
        last_copying_bytes_total(&events),
        14,
        "the file's bytes, not the link's"
    );
}

/// A MOVE through the same preview lands the folder, then the source sweep
/// removes the LINK and nothing behind it: the target is someone's real folder
/// (a cloud drive, for `~/Dropbox`), which the person never selected.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_selected_link_moved_through_a_preview_removes_the_link_never_the_target() {
    let engine = TestOperations::new();
    let dir = TestDir::new("selected-link-move");
    let link = plant_linked_folder(&dir);
    let preview_id = "selected-link-move-preview";
    publish_local_preview(&engine, preview_id, std::slice::from_ref(&link));

    let dest = dest_volume();
    let result = move_volumes_with_progress(
        Arc::new(CollectorEventSink::new()),
        "op-selected-link-move",
        &make_state(&engine),
        boot_volume(),
        std::slice::from_ref(&link),
        Arc::clone(&dest),
        Path::new("/"),
        &config_with_preview(preview_id),
    )
    .await;

    assert!(result.is_ok(), "expected Ok, got {result:?}");
    assert_eq!(read_dest(&dest, "/link/a.txt").await, b"aaa");
    assert_eq!(read_dest(&dest, "/link/sub/b.txt").await, b"bbbbb");
    assert_eq!(
        std::fs::read(dir.join("target/a.txt")).unwrap(),
        b"aaa",
        "the target keeps its files"
    );
    assert_eq!(std::fs::read(dir.join("target/sub/b.txt")).unwrap(), b"bbbbb");
    assert!(
        std::fs::symlink_metadata(&link).is_err(),
        "the move took the link itself"
    );
}
