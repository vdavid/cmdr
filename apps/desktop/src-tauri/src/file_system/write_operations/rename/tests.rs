//! Tests for the rename module: the descriptor's busy-set wiring, the managed
//! wrapper's transparency to the caller (same returns as the old command), and
//! the end-to-end busy-marking of a non-root volume during the mutation.

use super::*;
use std::fs;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::file_system::volume::{LaneKey, ListingProgress};
use crate::file_system::write_operations::busy_volume_ids;
use crate::file_system::{FileEntry, Volume, VolumeError, get_volume_manager};

fn unique(label: &str) -> String {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    format!("rename-test-{label}-{n}-{:?}", std::thread::current().id())
}

fn create_test_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("cmdr_rename_mod_test_{}", name));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("Failed to create test directory");
    dir
}

// ============================================================================
// Descriptor / busy-set wiring
// ============================================================================

#[test]
fn rename_descriptor_marks_only_nonroot_volumes_busy() {
    let from = Path::new("/parent/old.txt");
    let to = Path::new("/parent/new.txt");

    // Root → no busy volume (root is never ejectable), no lane, from→to summary.
    let root = rename_descriptor(from, to, "root");
    assert!(root.volume_ids.is_empty(), "root marks nothing busy");
    assert!(root.lanes.is_empty(), "instant ops never reserve a lane");
    assert_eq!(root.operation_type, WriteOperationType::Rename);
    assert_eq!(root.summary.source.as_deref(), Some("old.txt"));
    assert_eq!(root.summary.destination.as_deref(), Some("new.txt"));

    // A real volume → marked busy for the op's duration.
    let device = rename_descriptor(from, to, "usb-42");
    assert_eq!(device.volume_ids, vec!["usb-42".to_string()]);
    assert!(device.lanes.is_empty());
}

// ============================================================================
// Managed-wrapper transparency (local root: same returns as the old command)
// ============================================================================

#[tokio::test]
async fn rename_managed_local_success() {
    let tmp = create_test_dir("managed_ok");
    let old = tmp.join("old.txt");
    let new = tmp.join("new.txt");
    fs::write(&old, "content").unwrap();
    let result = rename_managed(old.clone(), new.clone(), false, "root".to_string()).await;
    assert!(result.is_ok());
    assert!(!old.exists());
    assert_eq!(fs::read_to_string(&new).unwrap(), "content");
    let _ = fs::remove_dir_all(&tmp);
}

#[tokio::test]
async fn rename_managed_local_conflict_without_force_is_transparent() {
    let tmp = create_test_dir("managed_conflict");
    let old = tmp.join("old.txt");
    let new = tmp.join("new.txt");
    fs::write(&old, "old").unwrap();
    fs::write(&new, "new").unwrap();
    let result = rename_managed(old.clone(), new.clone(), false, "root".to_string()).await;
    assert!(result.is_err());
    // allowed-error-string-match: the module returns a String; message is the signal
    assert!(result.unwrap_err().contains("already exists"));
    assert!(old.exists() && new.exists(), "both intact on conflict");
    let _ = fs::remove_dir_all(&tmp);
}

#[tokio::test]
async fn rename_managed_local_force_overwrites() {
    let tmp = create_test_dir("managed_force");
    let old = tmp.join("old.txt");
    let new = tmp.join("new.txt");
    fs::write(&old, "new content").unwrap();
    fs::write(&new, "old content").unwrap();
    let result = rename_managed(old.clone(), new.clone(), true, "root".to_string()).await;
    assert!(result.is_ok());
    assert!(!old.exists());
    assert_eq!(fs::read_to_string(&new).unwrap(), "new content");
    let _ = fs::remove_dir_all(&tmp);
}

// ============================================================================
// End-to-end busy marking through the module entry point
// ============================================================================

/// A minimal test `Volume` whose `rename` parks on a `Notify` until released, so
/// a test can observe the busy set mid-mutation. Everything else is a stub.
struct BlockingVolume {
    name: String,
    root: PathBuf,
    started: Arc<tokio::sync::Notify>,
    release: Arc<tokio::sync::Notify>,
}

impl Volume for BlockingVolume {
    fn name(&self) -> &str {
        &self.name
    }
    fn root(&self) -> &Path {
        &self.root
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn list_directory<'a>(
        &'a self,
        _path: &'a Path,
        _on_progress: Option<&'a (dyn Fn(ListingProgress) + Sync)>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
        Box::pin(async { Ok(vec![]) })
    }
    fn get_metadata<'a>(
        &'a self,
        _path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<FileEntry, VolumeError>> + Send + 'a>> {
        Box::pin(async { Err(VolumeError::NotSupported) })
    }
    fn exists<'a>(&'a self, _path: &'a Path) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        Box::pin(async { false })
    }
    fn is_directory<'a>(
        &'a self,
        _path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<bool, VolumeError>> + Send + 'a>> {
        Box::pin(async { Err(VolumeError::NotSupported) })
    }
    fn lane_key(&self) -> LaneKey {
        LaneKey::new(self.name.clone())
    }
    fn rename<'a>(
        &'a self,
        _from: &'a Path,
        _to: &'a Path,
        _force: bool,
    ) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            self.started.notify_one();
            self.release.notified().await;
            Ok(())
        })
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rename_managed_marks_nonroot_volume_busy_during_op() {
    let volume_id = unique("busy-vol");
    let started = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    let volume = Arc::new(BlockingVolume {
        name: volume_id.clone(),
        root: PathBuf::from("/"),
        started: Arc::clone(&started),
        release: Arc::clone(&release),
    });
    get_volume_manager().register(&volume_id, volume);

    let vid = volume_id.clone();
    let handle =
        tokio::spawn(async move { rename_managed(PathBuf::from("/old"), PathBuf::from("/new"), false, vid).await });

    // Wait until the volume's rename is parked mid-flight.
    started.notified().await;
    assert!(
        busy_volume_ids().contains(&volume_id),
        "a non-root volume must be busy while its rename runs"
    );

    // Release → the rename (and the managed op) completes.
    release.notify_one();
    let result = handle.await.expect("task joins");
    assert!(result.is_ok(), "the managed rename returns success");
    assert!(
        !busy_volume_ids().contains(&volume_id),
        "the volume must be freed once the rename finishes"
    );
}
