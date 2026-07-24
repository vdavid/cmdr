//! Tests for the same-volume rename-merge fast path
//! (`move_within_same_volume_with_progress` + `rename_merge_directory`).
//!
//! These drive the real `move_within_same_volume_with_progress` pipeline so the
//! whole stack — top-level hints, the driver's top-level conflict detection, the
//! resolver short-circuit for dir-vs-dir, and the recursive rename-merge — runs
//! exactly as in production.
//!
//! ## Why `LocalPosixVolume` over a tempdir, not `InMemoryVolume`
//!
//! The rename-merge depends on two real backend semantics that `InMemoryVolume`
//! does NOT model: `rename` of a directory moves its WHOLE subtree in one call,
//! and `delete` of a non-empty directory FAILS (empty-only). `InMemoryVolume`'s
//! `rename` moves only the single keyed entry (orphaning children) and its
//! `delete` removes any entry unconditionally. `LocalPosixVolume` over a tempdir
//! gives the real POSIX semantics the rename-merge is built on, on both Linux
//! (CI) and macOS. The case-fold tests use a dedicated case-insensitive wrapper
//! so they're portable regardless of the host filesystem's case sensitivity.

use super::conflict_responder_test_support::{
    ConflictResponderSink, file_conflict_count, folder_conflict_count_any_dir,
};
use super::volume_move_same::move_within_same_volume_with_progress;
use crate::file_system::listing::FileEntry;
use crate::file_system::volume::{LocalPosixVolume, Volume, VolumeError};
use crate::file_system::write_operations::state::{WriteOperationState, cancel_write_operation};
use crate::file_system::write_operations::test_support::TestOperationGuard;
use crate::file_system::write_operations::types::{
    CollectorEventSink, ConflictResolution, VolumeCopyConfig, WriteOperationError,
};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use tempfile::TempDir;

fn make_state() -> Arc<WriteOperationState> {
    Arc::new(WriteOperationState::new(Duration::from_millis(0)))
}

/// A `LocalPosixVolume` rooted at a fresh tempdir. The `TempDir` is returned so
/// the caller keeps it alive for the test's duration.
fn local_volume() -> (Arc<dyn Volume>, TempDir) {
    let dir = TempDir::new().unwrap();
    let vol: Arc<dyn Volume> = Arc::new(LocalPosixVolume::new("V", dir.path().to_path_buf()));
    (vol, dir)
}

/// Writes a file at a volume-relative path, creating parents on disk.
fn write_file(root: &Path, rel: &str, content: &[u8]) {
    let abs = root.join(rel);
    std::fs::create_dir_all(abs.parent().unwrap()).unwrap();
    std::fs::write(abs, content).unwrap();
}

fn mkdir(root: &Path, rel: &str) {
    std::fs::create_dir_all(root.join(rel)).unwrap();
}

fn read(root: &Path, rel: &str) -> Vec<u8> {
    std::fs::read(root.join(rel)).unwrap()
}

fn exists(root: &Path, rel: &str) -> bool {
    root.join(rel).exists()
}

// ============================================================================
// Merge with zero folder prompts
// ============================================================================

/// A top-level folder collision merges with NO folder-level prompt. Dest-only
/// files survive, source-only files arrive, and a non-clashing nested subtree
/// rides across on one rename.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rename_merge_no_folder_prompt_dest_only_survives() {
    let (volume, dir) = local_volume();
    let root = dir.path();

    // Source /album: a fresh file + a nested subtree with no dest clash.
    write_file(root, "src/album/fresh.txt", b"SRC-fresh");
    write_file(root, "src/album/sub/deep.txt", b"SRC-deep");
    // Dest /album: a dest-only file that must survive the merge.
    write_file(root, "dst/album/keep.txt", b"DEST-keep");

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state();
    // Stop policy: a folder-level prompt would BLOCK forever (no responder).
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Stop,
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };

    let result = move_within_same_volume_with_progress(
        events.clone(),
        "op-merge-no-prompt",
        &state,
        Arc::clone(&volume),
        &[PathBuf::from("src/album")],
        Path::new("dst"),
        &config,
    )
    .await;
    assert!(result.is_ok(), "expected Ok, got {:?}", result);

    assert_eq!(
        folder_conflict_count_any_dir(&events),
        0,
        "a folder merge must never prompt"
    );
    // Dest-only file preserved.
    assert_eq!(read(root, "dst/album/keep.txt"), b"DEST-keep");
    // Source-only file + nested subtree arrived.
    assert_eq!(read(root, "dst/album/fresh.txt"), b"SRC-fresh");
    assert_eq!(read(root, "dst/album/sub/deep.txt"), b"SRC-deep");
    // Whole source spine deleted (all moved).
    assert!(!exists(root, "src/album"), "fully-moved source spine must be gone");
}

// ============================================================================
// File policy inside the merge
// ============================================================================

/// Inside a merge, a clashing FILE follows the Skip policy: dest keeps its copy,
/// source keeps its original, and the source DIR survives (it still holds the
/// skipped child).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rename_merge_skip_child_leaves_source_dir_and_ancestors() {
    let (volume, dir) = local_volume();
    let root = dir.path();

    write_file(root, "src/album/clash.txt", b"SRC-clash");
    write_file(root, "src/album/sub/deeper/clash2.txt", b"SRC-deep-clash");
    write_file(root, "src/album/sub/deeper/fresh.txt", b"SRC-fresh");
    write_file(root, "dst/album/clash.txt", b"DEST-clash");
    write_file(root, "dst/album/sub/deeper/clash2.txt", b"DEST-deep-clash");

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state();
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Skip,
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };

    let result = move_within_same_volume_with_progress(
        events.clone(),
        "op-merge-skip",
        &state,
        Arc::clone(&volume),
        &[PathBuf::from("src/album")],
        Path::new("dst"),
        &config,
    )
    .await;
    assert!(result.is_ok(), "expected Ok, got {:?}", result);

    // Dest keeps both clashing copies; both sources survive (skip = keep both).
    assert_eq!(read(root, "dst/album/clash.txt"), b"DEST-clash");
    assert_eq!(read(root, "dst/album/sub/deeper/clash2.txt"), b"DEST-deep-clash");
    assert!(exists(root, "src/album/clash.txt"), "skipped source file must survive");
    assert!(
        exists(root, "src/album/sub/deeper/clash2.txt"),
        "skipped deep source file must survive"
    );
    // The non-clashing fresh file still moved.
    assert_eq!(read(root, "dst/album/sub/deeper/fresh.txt"), b"SRC-fresh");
    assert!(!exists(root, "src/album/sub/deeper/fresh.txt"));

    // Source dir + ALL its ancestors survive because they still hold skipped
    // children. Inside-out empty-only cleanup never deletes a dir with content.
    assert!(exists(root, "src/album"), "source dir holding a skipped child survives");
    assert!(
        exists(root, "src/album/sub/deeper"),
        "deepest source dir holding a skipped child survives"
    );
}

/// Inside a merge, a clashing FILE under Overwrite-all replaces the dest copy
/// (delete-then-rename), and the fully-emptied source spine is deleted.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rename_merge_overwrite_replaces_and_deletes_source_spine() {
    let (volume, dir) = local_volume();
    let root = dir.path();

    write_file(root, "src/album/clash.txt", b"SRC-NEW");
    write_file(root, "src/album/sub/clash2.txt", b"SRC-NEW-2");
    write_file(root, "dst/album/clash.txt", b"DEST-OLD");
    write_file(root, "dst/album/sub/clash2.txt", b"DEST-OLD-2");
    write_file(root, "dst/album/sub/keep.txt", b"DEST-keep");

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state();
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Overwrite,
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };

    let result = move_within_same_volume_with_progress(
        events.clone(),
        "op-merge-overwrite",
        &state,
        Arc::clone(&volume),
        &[PathBuf::from("src/album")],
        Path::new("dst"),
        &config,
    )
    .await;
    assert!(result.is_ok(), "expected Ok, got {:?}", result);
    assert_eq!(folder_conflict_count_any_dir(&events), 0);

    // Clashing files replaced with the source bytes.
    assert_eq!(read(root, "dst/album/clash.txt"), b"SRC-NEW");
    assert_eq!(read(root, "dst/album/sub/clash2.txt"), b"SRC-NEW-2");
    // Dest-only file untouched (merge invariant).
    assert_eq!(read(root, "dst/album/sub/keep.txt"), b"DEST-keep");
    // Everything moved → source spine gone, deepest-first.
    assert!(!exists(root, "src/album"), "fully-moved source spine must be deleted");
}

/// Inside a merge, a clashing FILE under Stop emits a per-file `write-conflict`
/// (NOT a folder one), and resumes on the scripted answer.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rename_merge_stop_file_clash_prompts_and_resumes() {
    let (volume, dir) = local_volume();
    let root = dir.path();

    write_file(root, "src/album/clash.txt", b"SRC-NEW");
    write_file(root, "dst/album/clash.txt", b"DEST-OLD");

    let state = make_state();
    let events = Arc::new(ConflictResponderSink::new(&state, ConflictResolution::Overwrite, false));
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Stop,
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };

    let result = move_within_same_volume_with_progress(
        events.clone(),
        "op-merge-stop",
        &state,
        Arc::clone(&volume),
        &[PathBuf::from("src/album")],
        Path::new("dst"),
        &config,
    )
    .await;
    assert!(result.is_ok(), "expected Ok, got {:?}", result);

    // Exactly one FILE prompt, zero FOLDER prompts — sink-derived, race-free.
    assert_eq!(file_conflict_count(&events.inner), 1, "exactly one file clash prompted");
    assert_eq!(
        folder_conflict_count_any_dir(&events.inner),
        0,
        "the folder itself never prompts"
    );
    // The Overwrite answer landed the source bytes.
    assert_eq!(read(root, "dst/album/clash.txt"), b"SRC-NEW");
}

// ============================================================================
// Source-dir cleanup matrix
// ============================================================================

/// All-Rename: every clashing child resolves to Rename (lands as `name (1)`), so
/// every source child moves out and the spine deletes inside-out.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rename_merge_all_rename_deletes_source_spine() {
    let (volume, dir) = local_volume();
    let root = dir.path();

    write_file(root, "src/album/clash.txt", b"SRC");
    write_file(root, "src/album/sub/clash2.txt", b"SRC2");
    write_file(root, "dst/album/clash.txt", b"DEST");
    write_file(root, "dst/album/sub/clash2.txt", b"DEST2");

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state();
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Rename,
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };

    let result = move_within_same_volume_with_progress(
        events.clone(),
        "op-merge-rename",
        &state,
        Arc::clone(&volume),
        &[PathBuf::from("src/album")],
        Path::new("dst"),
        &config,
    )
    .await;
    assert!(result.is_ok(), "expected Ok, got {:?}", result);

    // Originals preserved at dest; renamed copies landed beside them.
    assert_eq!(read(root, "dst/album/clash.txt"), b"DEST");
    assert_eq!(read(root, "dst/album/clash (1).txt"), b"SRC");
    assert_eq!(read(root, "dst/album/sub/clash2.txt"), b"DEST2");
    assert_eq!(read(root, "dst/album/sub/clash2 (1).txt"), b"SRC2");
    // All children moved → source spine deleted inside-out.
    assert!(
        !exists(root, "src/album"),
        "all-Rename empties and deletes the source spine"
    );
}

/// An errored deep child preserves the source dir and its ancestors. A read-only
/// nested dest subdir makes the child rename fail; the source must survive.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rename_merge_errored_child_preserves_source_spine() {
    // Root bypasses POSIX permission bits, so the read-only dest subdir below
    // wouldn't block the rename and the error path wouldn't trigger. The Linux
    // CI Rust suite runs as root in Docker; skip there (mirrors the geteuid==0
    // guards in the permission-dependent integration tests).
    #[cfg(unix)]
    // SAFETY: (test) `geteuid` takes no arguments, shares no memory, and can't fail — it just
    // returns the caller's effective uid. We compare the returned integer to 0 to detect root.
    if unsafe { libc::geteuid() } == 0 {
        return;
    }

    let (volume, dir) = local_volume();
    let root = dir.path();

    write_file(root, "src/album/ok.txt", b"OK");
    write_file(root, "src/album/sub/blocked.txt", b"SRC");
    // Dest has the same subtree; make the dest subdir read-only so renaming a
    // child INTO it fails (POSIX requires write on the target directory).
    write_file(root, "dst/album/sub/other.txt", b"DEST");

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let sub = root.join("dst/album/sub");
        std::fs::set_permissions(&sub, std::fs::Permissions::from_mode(0o555)).unwrap();
    }

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state();
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Stop,
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };

    let result = move_within_same_volume_with_progress(
        events.clone(),
        "op-merge-error",
        &state,
        Arc::clone(&volume),
        &[PathBuf::from("src/album")],
        Path::new("dst"),
        &config,
    )
    .await;

    // Restore permissions so the TempDir can clean up.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(root.join("dst/album/sub"), std::fs::Permissions::from_mode(0o755));
    }

    // On Unix the blocked rename errors out; the source spine must survive.
    #[cfg(unix)]
    {
        assert!(result.is_err(), "a blocked child rename must surface as an error");
        assert!(
            exists(root, "src/album/sub/blocked.txt"),
            "errored child must leave the source in place"
        );
        assert!(exists(root, "src/album"), "errored child preserves the source spine");
    }
    #[cfg(not(unix))]
    {
        let _ = result;
    }
}

// ============================================================================
// Cancel mid-merge
// ============================================================================

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

// ============================================================================
// Case-insensitive backends + TOCTOU (the late-detected-collision net)
// ============================================================================

/// A `LocalPosixVolume` wrapper that makes `rename` and `list_directory`
/// case-insensitive, modeling SMB / APFS. Renaming onto an existing
/// case-folded name returns `AlreadyExists` even when the exact-match map
/// missed it; listing reflects the real on-disk (lowercased fixture) names so
/// the late-detection path can find the case-folded sibling.
struct CaseInsensitiveVolume {
    inner: Arc<LocalPosixVolume>,
}

impl CaseInsensitiveVolume {
    /// Resolves a path to its real on-disk casing by listing the parent and
    /// matching case-insensitively. Returns the input unchanged if no sibling
    /// matches (the name is free).
    async fn fold(&self, path: &Path) -> PathBuf {
        let parent = match path.parent() {
            Some(p) => p,
            None => return path.to_path_buf(),
        };
        let name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_lowercase(),
            None => return path.to_path_buf(),
        };
        if let Ok(entries) = self.inner.list_directory(parent, None).await {
            for e in entries {
                if e.name.to_lowercase() == name {
                    return parent.join(&e.name);
                }
            }
        }
        path.to_path_buf()
    }
}

impl Volume for CaseInsensitiveVolume {
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
        Box::pin(async move {
            let folded = self.fold(path).await;
            self.inner.get_metadata(&folded).await
        })
    }
    fn exists<'a>(&'a self, path: &'a Path) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        Box::pin(async move {
            let folded = self.fold(path).await;
            self.inner.exists(&folded).await
        })
    }
    fn is_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<bool, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let folded = self.fold(path).await;
            self.inner.is_directory(&folded).await
        })
    }
    fn delete<'a>(&'a self, path: &'a Path) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let folded = self.fold(path).await;
            self.inner.delete(&folded).await
        })
    }
    fn rename<'a>(
        &'a self,
        from: &'a Path,
        to: &'a Path,
        force: bool,
    ) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            // Case-insensitive collision: if a case-folded sibling already holds
            // the target name (different exact spelling), reject with
            // AlreadyExists like SMB / APFS would.
            let folded_to = self.fold(to).await;
            if !force && folded_to != *to && self.inner.exists(&folded_to).await {
                return Err(VolumeError::AlreadyExists(to.display().to_string()));
            }
            let folded_from = self.fold(from).await;
            self.inner.rename(&folded_from, to, force).await
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

// ============================================================================
// Perf regression pin: no subtree walk, O(top-level) stat count
// ============================================================================

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

/// A case-folded FILE collision (the exact-match map misses `Clash.txt` vs
/// `clash.txt`) prompts EXACTLY ONCE under Stop and resolves correctly — the
/// late-detected path routes it through the resolver instead of erroring.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn case_folded_file_collision_prompts_exactly_once() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    write_file(root, "src/album/Clash.txt", b"SRC");
    write_file(root, "dst/album/clash.txt", b"DEST");

    let volume: Arc<dyn Volume> = Arc::new(CaseInsensitiveVolume {
        inner: Arc::new(LocalPosixVolume::new("V", root.to_path_buf())),
    });

    let state = make_state();
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Stop,
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };
    // Answer Skip once; if the net re-prompted we'd see more than one recorded
    // file conflict.
    let events = Arc::new(ConflictResponderSink::new(&state, ConflictResolution::Skip, false));

    let result = move_within_same_volume_with_progress(
        events.clone(),
        "op-casefold-file",
        &state,
        Arc::clone(&volume),
        &[PathBuf::from("src/album")],
        Path::new("dst"),
        &config,
    )
    .await;
    assert!(result.is_ok(), "expected Ok, got {:?}", result);

    assert_eq!(
        file_conflict_count(&events.inner),
        1,
        "a case-folded file collision must prompt exactly once"
    );
    assert_eq!(folder_conflict_count_any_dir(&events.inner), 0);
}

/// A child resolved Overwrite that THEN collides on the case-folded name must
/// NOT prompt twice — the stored decision finalizes the case-folded replace.
///
/// To force the map to miss but the rename to collide, the dest holds the file
/// under a different casing than the source AND the exact source name. The
/// resolver answers Overwrite once; the late path finalizes it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn case_folded_overwrite_does_not_prompt_twice() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    write_file(root, "src/album/Photo.JPG", b"SRC-NEW");
    write_file(root, "dst/album/photo.jpg", b"DEST-OLD");

    let volume: Arc<dyn Volume> = Arc::new(CaseInsensitiveVolume {
        inner: Arc::new(LocalPosixVolume::new("V", root.to_path_buf())),
    });

    let state = make_state();
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Stop,
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };
    let events = Arc::new(ConflictResponderSink::new(&state, ConflictResolution::Overwrite, false));

    let result = move_within_same_volume_with_progress(
        events.clone(),
        "op-casefold-overwrite",
        &state,
        Arc::clone(&volume),
        &[PathBuf::from("src/album")],
        Path::new("dst"),
        &config,
    )
    .await;
    assert!(result.is_ok(), "expected Ok, got {:?}", result);

    assert_eq!(
        file_conflict_count(&events.inner),
        1,
        "a child resolved Overwrite must NOT re-prompt when its rename collides on case-fold"
    );
    // The overwrite landed: exactly one file under the dest album, holding the
    // new bytes (the dest's case-folded name is replaced in place).
    let entries = volume.list_directory(Path::new("dst/album"), None).await.unwrap();
    let jpgs: Vec<_> = entries
        .iter()
        .filter(|e| e.name.to_lowercase().ends_with(".jpg"))
        .collect();
    assert_eq!(jpgs.len(), 1, "case-folded overwrite must not leave a duplicate");
    let landed = read(root, &format!("dst/album/{}", jpgs[0].name));
    assert_eq!(landed, b"SRC-NEW", "the overwrite must land the source bytes");
}

// ============================================================================
// Dest-inside-source guard
// ============================================================================

/// Moving `/A` into `/A/sub` (its own descendant) on the same volume is
/// rejected before any rename runs.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn move_into_own_descendant_is_rejected() {
    let (volume, dir) = local_volume();
    let root = dir.path();
    write_file(root, "A/file.txt", b"x");
    mkdir(root, "A/sub");

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state();
    let config = VolumeCopyConfig::default();

    let result = move_within_same_volume_with_progress(
        events.clone(),
        "op-dest-inside",
        &state,
        Arc::clone(&volume),
        &[PathBuf::from("A")],
        Path::new("A/sub"),
        &config,
    )
    .await;

    assert!(
        matches!(result, Err(WriteOperationError::DestinationInsideSource { .. })),
        "moving a dir into its own descendant must be rejected, got {:?}",
        result
    );
    // Nothing was moved.
    assert!(exists(root, "A/file.txt"), "source untouched on a rejected move");
}

// ============================================================================
// Symlinks moved as opaque entries
// ============================================================================

/// A symlink child is renamed as an opaque entry — never descended.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rename_merge_moves_symlink_as_opaque_entry() {
    let (volume, dir) = local_volume();
    let root = dir.path();
    write_file(root, "src/album/real.txt", b"REAL");
    std::os::unix::fs::symlink("real.txt", root.join("src/album/link.txt")).unwrap();
    mkdir(root, "dst/album");

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state();
    let config = VolumeCopyConfig {
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };

    let result = move_within_same_volume_with_progress(
        events.clone(),
        "op-merge-symlink",
        &state,
        Arc::clone(&volume),
        &[PathBuf::from("src/album")],
        Path::new("dst"),
        &config,
    )
    .await;
    assert!(result.is_ok(), "expected Ok, got {:?}", result);

    // The symlink moved as a symlink (not dereferenced into a copy of the file).
    let link = root.join("dst/album/link.txt");
    let meta = std::fs::symlink_metadata(&link).unwrap();
    assert!(meta.file_type().is_symlink(), "symlink must move as a symlink");
    assert_eq!(read(root, "dst/album/real.txt"), b"REAL");
    assert!(!exists(root, "src/album"), "fully-moved source spine deleted");
}
