//! What the same-volume rename-merge does when a stat REFUSES TO ANSWER, and
//! what it does with no preflight hint at all.
//!
//! Both are the same question one level apart: the merge decides between
//! `rename_merge_directory` and the conflict resolver on "is this a directory?",
//! and the answer can be absent (no hint) or unavailable (a failed probe). The
//! merge-semantics tests live in `rename_merge_tests.rs`; these pin what happens
//! when the fact underneath them isn't there.
//!
//! `InMemoryVolume::set_stat_failing` can't drive these: the rename-merge needs
//! real POSIX `rename`-moves-a-subtree and empty-only `delete`, which is why
//! this family runs on `LocalPosixVolume` behind a wrapper.

use super::move_same::move_within_same_volume_with_progress;
use crate::file_system::listing::FileEntry;
use crate::file_system::volume::{LocalPosixVolume, Volume, VolumeError};
use crate::file_system::write_operations::event_sinks::CollectorEventSink;
use crate::file_system::write_operations::state::WriteOperationState;
use crate::file_system::write_operations::types::{ConflictResolution, VolumeCopyConfig, WriteOperationError};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;

fn make_state() -> Arc<WriteOperationState> {
    Arc::new(WriteOperationState::new(Duration::from_millis(0)))
}

/// Writes a file at a volume-relative path, creating parents on disk.
fn write_file(root: &Path, rel: &str, content: &[u8]) {
    let abs = root.join(rel);
    std::fs::create_dir_all(abs.parent().expect("path has a parent")).expect("create parents");
    std::fs::write(abs, content).expect("write file");
}

fn read(root: &Path, rel: &str) -> Vec<u8> {
    std::fs::read(root.join(rel)).expect("read file")
}

fn exists(root: &Path, rel: &str) -> bool {
    root.join(rel).exists()
}

/// Wraps a `LocalPosixVolume` and makes `is_directory` REFUSE TO ANSWER for one
/// path (typed `IoError`), while everything else works and the path keeps
/// existing. Models a dropped session or a hung mount, not a missing file. The
/// `InMemoryVolume` knob (`set_stat_failing`) can't be used here: the
/// rename-merge needs real POSIX `rename`-moves-a-subtree and empty-only
/// `delete` semantics, which is why this whole file runs on `LocalPosixVolume`.
struct StatFailingVolume {
    inner: Arc<LocalPosixVolume>,
    unanswerable: PathBuf,
    /// Names dropped from every `list_directory` result, so a source can reach
    /// the move loop with no preflight hint at all.
    hidden_from_listings: Vec<String>,
}

impl Volume for StatFailingVolume {
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
        Box::pin(async move {
            let mut entries = self.inner.list_directory(path, on_progress).await?;
            entries.retain(|e| !self.hidden_from_listings.contains(&e.name));
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
        if path == self.unanswerable {
            return Box::pin(async move {
                Err(VolumeError::IoError {
                    message: "Stat unavailable".to_string(),
                    raw_os_error: None,
                })
            });
        }
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
        self.inner.scan_for_copy(path)
    }
}

/// Inside a rename-merge, a child whose destination can't be stat'd must fail
/// that item and leave the destination exactly as it was.
///
/// The old shape guessed `false` on an unanswerable stat, which walked straight
/// into the `exists()` + `delete` fallthrough: on a destination that really is a
/// directory, that's a delete aimed at the user's folder. Both the resolver
/// (`conflict.rs`) and the merge's own dir check now propagate instead, so the
/// item fails and nothing on either side moves.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_merge_child_whose_destination_cannot_be_stat_d_fails_and_touches_nothing() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    write_file(root, "src/album/clash.txt", b"from source");
    write_file(root, "src/album/fine.txt", b"also from source");
    write_file(root, "dst/album/clash.txt", b"precious dest data");
    write_file(root, "dst/album/keep-me.txt", b"dest only");

    let volume: Arc<dyn Volume> = Arc::new(StatFailingVolume {
        inner: Arc::new(LocalPosixVolume::new("V", root.to_path_buf())),
        unanswerable: PathBuf::from("/dst/album/clash.txt"),
        hidden_from_listings: Vec::new(),
    });

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state();
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Overwrite,
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };

    let result = move_within_same_volume_with_progress(
        events.clone(),
        "op-merge-stat-fails",
        &state,
        Arc::clone(&volume),
        &[PathBuf::from("/src/album")],
        Path::new("/dst"),
        &config,
    )
    .await;

    assert!(
        matches!(&result, Err(WriteOperationError::IoError { .. })),
        "an unanswerable stat must fail the item; got {result:?}"
    );
    assert_eq!(
        read(root, "dst/album/clash.txt"),
        b"precious dest data",
        "the destination the stat couldn't classify must be untouched"
    );
    assert!(
        exists(root, "dst/album/keep-me.txt"),
        "a dest-only sibling must survive"
    );
    assert!(
        exists(root, "src/album/clash.txt"),
        "the source must survive too: nothing moved, so nothing may be swept"
    );
}

/// A source with NO preflight hint still takes the rename-merge path when both
/// sides are directories.
///
/// The hint goes missing whenever the source isn't in its parent's listing
/// (a stale listing, a name-encoding mismatch), which is exactly the case the
/// old `.unwrap_or(false)` answered with "file". A guessed `false` skips the
/// dir-vs-dir branch and sends the collision through `resolve_volume_conflict`
/// instead; that degrades to a merge rather than destroying anything, but it's
/// still the wrong branch chosen on a belief. Here the source folder merges and
/// the dest-only file survives, exactly as with a hint.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_hintless_directory_source_still_takes_the_rename_merge_path() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    write_file(root, "src/album/from-source.txt", b"source only");
    write_file(root, "dst/album/keep-me.txt", b"dest only");

    // `album` is dropped from every listing, so `top_level_move_hints` records
    // no hint for it and the move loop has to resolve the type itself.
    let volume: Arc<dyn Volume> = Arc::new(StatFailingVolume {
        inner: Arc::new(LocalPosixVolume::new("V", root.to_path_buf())),
        unanswerable: PathBuf::from("/nothing-here"),
        hidden_from_listings: vec!["album".to_string()],
    });

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state();
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Overwrite,
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };

    let result = move_within_same_volume_with_progress(
        events.clone(),
        "op-hintless-merge",
        &state,
        Arc::clone(&volume),
        &[PathBuf::from("/src/album")],
        Path::new("/dst"),
        &config,
    )
    .await;

    assert!(result.is_ok(), "the hintless merge must succeed: {result:?}");
    assert_eq!(
        read(root, "dst/album/keep-me.txt"),
        b"dest only",
        "a merge never removes a dest file the source doesn't shadow"
    );
    assert_eq!(
        read(root, "dst/album/from-source.txt"),
        b"source only",
        "the source's content must arrive in the merged destination"
    );
}
