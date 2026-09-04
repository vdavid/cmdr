//! Pausing a same-volume rename-merge mid-flight really stops it.
//!
//! Pause is a promise: the UI says "Paused", and the person who hit it because
//! they picked the wrong destination believes they have time to intervene. This
//! walk is the one where believing that matters most, because a child with no
//! destination counterpart is ONE server-side rename carrying an entire subtree,
//! so a single iteration can move gigabytes.
//!
//! The pause is wired to the merge's own progress rather than a wall clock, the
//! same way `rename_merge_cancel_tests.rs` wires its cancel: a `LocalPosixVolume`
//! wrapper pauses the op the instant the first child rename lands, so the walk is
//! provably mid-flight when the gate has to hold it.

use super::move_same::move_within_same_volume_with_progress;
use super::rename_merge_test_support::{exists, make_state, mkdir, write_file};
use crate::file_system::listing::FileEntry;
use crate::file_system::volume::{LocalPosixVolume, Volume, VolumeError};
use crate::file_system::write_operations::event_sinks::CollectorEventSink;
use crate::file_system::write_operations::test_support::TestOperationGuard;
use crate::file_system::write_operations::types::{ConflictResolution, VolumeCopyConfig};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use tempfile::TempDir;

/// Children in the fixture tree. Enough that an ungated walk would be finished
/// long before the test's window closes.
const CHILDREN: usize = 40;

/// A `LocalPosixVolume` wrapper that pauses the operation the instant the FIRST
/// child rename lands, so the pause is deterministically tied to the merge's own
/// progress instead of a wall clock. The first child still moves (we pause AFTER
/// its rename returns `Ok`); every later one is the gate's job to hold.
struct PauseOnFirstRenameVolume {
    inner: Arc<LocalPosixVolume>,
    state: Arc<crate::file_system::write_operations::state::WriteOperationState>,
    renames: AtomicUsize,
}

impl Volume for PauseOnFirstRenameVolume {
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
            if result.is_ok() && self.renames.fetch_add(1, Ordering::SeqCst) == 0 {
                self.state.pause_gate.pause();
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

/// Pausing mid-merge holds the walk at its per-child boundary, and resuming
/// finishes the tree intact.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_paused_rename_merge_stops_moving_children_until_it_resumes() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    for i in 0..CHILDREN {
        write_file(root, &format!("src/album/f{:02}.txt", i), b"SRC");
    }
    mkdir(root, "dst/album");

    let op = TestOperationGuard::register_state("rename-merge-pause", make_state());
    let volume: Arc<dyn Volume> = Arc::new(PauseOnFirstRenameVolume {
        inner: Arc::new(LocalPosixVolume::new("V", root.to_path_buf())),
        state: Arc::clone(op.state()),
        renames: AtomicUsize::new(0),
    });

    let events = Arc::new(CollectorEventSink::new());
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Stop,
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };

    let op_id = op.id().to_string();
    let state_for_move = Arc::clone(op.state());
    let events_for_move = Arc::clone(&events);
    let volume_for_move = Arc::clone(&volume);
    let mover = tokio::spawn(async move {
        move_within_same_volume_with_progress(
            events_for_move,
            &op_id,
            &state_for_move,
            volume_for_move,
            &[PathBuf::from("src/album")],
            Path::new("dst"),
            &config,
        )
        .await
    });

    let moved = |root: &Path| {
        (0..CHILDREN)
            .filter(|i| exists(root, &format!("dst/album/f{:02}.txt", i)))
            .count()
    };

    crate::test_support::wait_until_async(Duration::from_secs(5), "the first child to land", || moved(root) >= 1).await;

    // Parking has no "parked now" signal, so hold a window open: an ungated walk
    // would have renamed the remaining children many times over inside it.
    // allowed-test-sleep: negative assertion over a window; the park has nothing to await.
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(
        moved(root),
        1,
        "a paused merge holds at its per-child boundary: only the child that was already \
         renamed when the pause landed has moved"
    );

    op.state().pause_gate.resume();

    let result = tokio::time::timeout(Duration::from_secs(10), mover)
        .await
        .expect("a resumed merge finishes")
        .expect("the merge task joins");
    assert!(result.is_ok(), "the resumed merge completes, got {result:?}");
    assert_eq!(moved(root), CHILDREN, "and every child lands once the user resumes");
    assert!(
        !exists(root, "src/album"),
        "an emptied source level is deleted, exactly as an unpaused merge would"
    );
}
