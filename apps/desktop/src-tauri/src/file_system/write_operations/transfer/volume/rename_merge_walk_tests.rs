//! Perf regression pin: a non-conflicting same-volume move never walks the
//! moved folder's interior, and its stat count stays O(top-level items).
//!
//! The whole point of the rename-merge fast path is that `rename` moves a
//! subtree in one call; a preflight or a probe that lists the interior would
//! quietly turn an instant move into an O(tree) one. The counting wrapper below
//! records every `list_directory`, `get_metadata`, and `scan_for_copy` with the
//! path it was asked about, so the assertions can name what got walked. Merge
//! semantics live in `rename_merge_tests.rs`.

use super::move_same::move_within_same_volume_with_progress;
use super::rename_merge_test_support::{exists, make_state, mkdir, write_file};
use crate::file_system::listing::FileEntry;
use crate::file_system::volume::{LocalPosixVolume, Volume, VolumeError};
use crate::file_system::write_operations::event_sinks::CollectorEventSink;
use crate::file_system::write_operations::types::VolumeCopyConfig;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tempfile::TempDir;

/// Wraps a `LocalPosixVolume` and counts `list_directory` + `get_metadata` +
/// `scan_for_copy` calls, with the paths listed, so a test can assert a
/// non-conflicting same-volume move never walks the moved folder's interior.
struct CountingVolume {
    inner: Arc<LocalPosixVolume>,
    listed: Arc<std::sync::Mutex<Vec<PathBuf>>>,
    stat_calls: Arc<AtomicUsize>,
}

impl CountingVolume {
    fn new(root: &Path) -> Arc<Self> {
        Arc::new(Self {
            inner: Arc::new(LocalPosixVolume::new("V", root.to_path_buf())),
            listed: Arc::new(std::sync::Mutex::new(Vec::new())),
            stat_calls: Arc::new(AtomicUsize::new(0)),
        })
    }
}

impl Volume for CountingVolume {
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
        self.listed.lock().unwrap().push(path.to_path_buf());
        self.inner.list_directory(path, on_progress)
    }
    fn get_metadata<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<FileEntry, VolumeError>> + Send + 'a>> {
        self.stat_calls.fetch_add(1, Ordering::Relaxed);
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
        self.inner.rename(from, to, force)
    }
    fn scan_for_copy<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<crate::file_system::volume::CopyScanResult, VolumeError>> + Send + 'a>>
    {
        // A `scan_for_copy` that RECURSED would defeat the perf contract; count
        // it as a stat and assert the count stays O(top-level). LocalPosix's
        // `scan_for_copy` does recurse to count a subtree's bytes — exactly what
        // we must NOT trigger for a non-conflicting move, so a recursing call
        // here would also show up as deep `list_directory`s if it used the trait.
        self.stat_calls.fetch_add(1, Ordering::Relaxed);
        self.inner.scan_for_copy(path)
    }
}

/// THE perf contract: a non-conflicting same-volume move of a deep folder must
/// NOT walk the folder's interior. It lists only the top level (for the batch
/// stat of the selected items), renames once, and never lists the moved folder.
/// Stat count stays O(top-level items), not O(subtree).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn non_conflicting_move_does_no_subtree_walk() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    // A deep, wide source folder. If anything walked the interior, the listed
    // paths would include `src/album/**` and the count would blow up.
    for i in 0..50 {
        write_file(root, &format!("src/album/f{:02}.txt", i), b"x");
    }
    for i in 0..50 {
        write_file(root, &format!("src/album/deep/g{:02}.txt", i), b"y");
    }
    // Dest dir exists but has NO `album` — so the move is non-conflicting.
    mkdir(root, "dst");

    let volume: Arc<dyn Volume> = CountingVolume::new(root);
    let counting = volume.as_any().downcast_ref::<CountingVolume>().unwrap();
    let listed = Arc::clone(&counting.listed);
    let stat_calls = Arc::clone(&counting.stat_calls);

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state();
    let config = VolumeCopyConfig {
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };

    let result = move_within_same_volume_with_progress(
        events.clone(),
        "op-perf-pin",
        &state,
        Arc::clone(&volume),
        &[PathBuf::from("src/album")],
        Path::new("dst"),
        &config,
    )
    .await;
    assert!(result.is_ok(), "expected Ok, got {:?}", result);

    // The folder moved wholesale.
    assert!(exists(root, "dst/album/f00.txt"));
    assert!(exists(root, "dst/album/deep/g00.txt"));
    assert!(!exists(root, "src/album"));

    // NO listing ever touched the moved folder's interior. The only listings
    // allowed are of the source/dest PARENTS, never `album` or anything below.
    let listed = listed.lock().unwrap();
    for p in listed.iter() {
        let s = p.to_string_lossy();
        assert!(
            !s.contains("album"),
            "a non-conflicting move must NOT list the moved folder's interior; listed {}",
            s
        );
    }

    // Stat count is O(top-level items): one selected item here. The batch stat
    // of the top-level sources plus the driver's per-top-level dest probe are
    // the only stats; nothing scales with the 100+ interior entries.
    let stats = stat_calls.load(Ordering::Relaxed);
    assert!(
        stats <= 4,
        "stat count must be O(top-level items), got {} (subtree has 100+ entries)",
        stats
    );
}
