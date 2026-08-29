//! Cancelling a same-volume rename-merge mid-flight: what's already moved stays
//! moved, and what hasn't stays at the source.
//!
//! The cancel is wired to the merge's own progress rather than a wall clock: a
//! `LocalPosixVolume` wrapper fires `cancel_write_operation` the instant the
//! first child rename lands, so `rename_merge_directory`'s per-child recheck
//! bails on the next iteration with the rest of the tree untouched. The merge
//! semantics these tests assume are in `rename_merge_tests.rs`.

use super::move_same::move_within_same_volume_with_progress;
use super::rename_merge_test_support::{exists, make_state, mkdir, write_file};
use crate::file_system::listing::FileEntry;
use crate::file_system::volume::{LocalPosixVolume, Volume, VolumeError};
use crate::file_system::write_operations::event_sinks::CollectorEventSink;
use crate::file_system::write_operations::state::cancel_write_operation;
use crate::file_system::write_operations::test_support::TestOperationGuard;
use crate::file_system::write_operations::types::{ConflictResolution, VolumeCopyConfig, WriteOperationError};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tempfile::TempDir;

/// A `LocalPosixVolume` wrapper that fires `cancel_write_operation` the instant
/// the FIRST child rename lands, so the cancel is deterministically wired to the
/// operation's own progress instead of a wall clock. The first child still moves
/// (we cancel AFTER its rename returns `Ok`), and `rename_merge_directory`'s
/// per-child `is_cancelled` recheck at the top of the next loop iteration then
/// bails with `Cancelled` while the remaining 39 children are still at the
/// source. This kills the old 1 ms-sleep flake (a fast run finished the whole
/// merge before the sleep elapsed, so the op returned `Ok` and the test failed).
struct CancelOnFirstRenameVolume {
    inner: Arc<LocalPosixVolume>,
    operation_id: String,
    renames: AtomicUsize,
}

impl Volume for CancelOnFirstRenameVolume {
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
        on_progress: Option<&'a (dyn Fn(crate::file_system::volume::ListingProgress) + Sync)>,
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
    fn is_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<bool, VolumeError>> + Send + 'a>> {
        self.inner.is_directory(path)
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
            let result = self.inner.rename(from, to, force).await;
            // The instant the first child rename lands, cancel the op. The next
            // loop iteration's `is_cancelled` recheck bails with `Cancelled`
            // while children remain — no wall clock, no race.
            if result.is_ok() && self.renames.fetch_add(1, Ordering::SeqCst) == 0 {
                cancel_write_operation(&self.operation_id, false);
            }
            result
        })
    }
    fn scan_for_copy<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<crate::file_system::volume::CopyScanResult, VolumeError>> + Send + 'a>>
    {
        self.inner.scan_for_copy(path)
    }
}

/// Cancel mid-merge keeps already-renamed children at the destination and does
/// NOT delete a source dir that still holds unmoved children. The cancel is
/// deterministically tied to the first child rename (see
/// `CancelOnFirstRenameVolume`), so exactly one child moves and the rest stay at
/// the source — robust regardless of how fast the merge runs.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rename_merge_cancel_keeps_moved_children_and_preserves_source() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    // Many fresh children so the walk is mid-flight when cancel fires.
    for i in 0..40 {
        write_file(root, &format!("src/album/f{:02}.txt", i), b"SRC");
    }
    mkdir(root, "dst/album");

    let op = TestOperationGuard::register_state("rename-merge-cancel", make_state());
    let volume: Arc<dyn Volume> = Arc::new(CancelOnFirstRenameVolume {
        inner: Arc::new(LocalPosixVolume::new("V", root.to_path_buf())),
        operation_id: op.id().to_string(),
        renames: AtomicUsize::new(0),
    });

    let events = Arc::new(CollectorEventSink::new());
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Stop,
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };

    let result = move_within_same_volume_with_progress(
        events.clone(),
        op.id(),
        op.state(),
        Arc::clone(&volume),
        &[PathBuf::from("src/album")],
        Path::new("dst"),
        &config,
    )
    .await;

    assert!(
        matches!(result, Err(WriteOperationError::Cancelled { .. })),
        "cancel mid-merge surfaces as Cancelled, got {:?}",
        result
    );

    // The cancel fires right after the first child rename lands, so exactly one
    // child moved and the other 39 stay at the source. The source dir survives
    // because it still holds unmoved children (never deleted while content
    // remains).
    let moved = (0..40)
        .filter(|i| exists(root, &format!("dst/album/f{:02}.txt", i)))
        .count();
    let remaining = (0..40)
        .filter(|i| exists(root, &format!("src/album/f{:02}.txt", i)))
        .count();
    assert_eq!(moved + remaining, 40, "no child is lost on cancel");
    assert_eq!(moved, 1, "exactly the first child moved before the cancel landed");
    assert_eq!(remaining, 39, "the cancel stops the walk while children remain");
    assert!(
        exists(root, "src/album"),
        "source dir holding unmoved children is never deleted on cancel"
    );
}
