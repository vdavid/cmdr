//! Tests for new-folder / new-file creation.
//!
//! The `*_core` tests drive the create logic directly (no operation manager),
//! moved here with the logic from `commands/file_system/mod.rs`. The descriptor
//! test pins the busy-set wiring (root → nothing busy, non-root → the volume).

use super::*;
use std::fs;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::file_system::volume::{LaneKey, ListingProgress};
use crate::file_system::{FileEntry, Volume, VolumeError};

fn unique(label: &str) -> String {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    format!("create-test-{label}-{n}-{:?}", std::thread::current().id())
}

fn create_test_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("cmdr_create_test_{}", name));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("Failed to create test directory");
    dir
}

fn cleanup_test_dir(path: &PathBuf) {
    let _ = fs::remove_dir_all(path);
}

/// Registers a real local-FS "root" volume so `create_*_core` with
/// `volume_id = None` (→ "root") exercises the timed `Volume` path, the same one
/// production hits. Idempotent via `register_if_absent`.
fn ensure_root_volume() {
    use crate::file_system::get_volume_manager;
    use crate::file_system::volume::LocalPosixVolume;
    use std::sync::Arc;
    get_volume_manager().register_if_absent("root", Arc::new(LocalPosixVolume::new("Test root", "/")));
}

// ============================================================================
// Descriptor / busy-set wiring
// ============================================================================

#[test]
fn instant_descriptor_marks_only_nonroot_volumes_busy() {
    // Root (or no volume) → no busy volume (root is never ejectable).
    let root_none = instant_descriptor(WriteOperationType::CreateFolder, None, "new");
    assert!(root_none.volume_ids.is_empty());
    assert!(root_none.lanes.is_empty(), "instant ops never reserve a lane");

    let root_explicit = instant_descriptor(WriteOperationType::CreateFile, Some("root"), "new");
    assert!(root_explicit.volume_ids.is_empty());

    // A real volume → marked busy for the op's duration.
    let device = instant_descriptor(WriteOperationType::CreateFolder, Some("usb-42"), "new");
    assert_eq!(device.volume_ids, vec!["usb-42".to_string()]);
    assert!(device.lanes.is_empty());
    assert_eq!(device.summary.source.as_deref(), Some("new"));
}

// ============================================================================
// create_directory_core
// ============================================================================

#[tokio::test]
async fn create_directory_success() {
    ensure_root_volume();
    let tmp = create_test_dir("create_success");
    let parent = tmp.to_string_lossy().to_string();
    let result = create_directory_core(None, &parent, "new-folder").await;
    assert!(result.is_ok());
    let (created_path, _) = result.unwrap();
    assert!(created_path.is_dir());
    assert!(created_path.to_string_lossy().ends_with("new-folder"));
    cleanup_test_dir(&tmp);
}

#[tokio::test]
async fn create_directory_already_exists() {
    ensure_root_volume();
    let tmp = create_test_dir("create_exists");
    let parent = tmp.to_string_lossy().to_string();
    fs::create_dir(tmp.join("existing")).unwrap();
    let result = create_directory_core(None, &parent, "existing").await;
    assert!(result.is_err());
    // allowed-error-string-match: the module returns a String; message is the signal
    assert!(result.unwrap_err().contains("already exists"));
    cleanup_test_dir(&tmp);
}

#[tokio::test]
async fn create_directory_empty_name() {
    let tmp = create_test_dir("create_empty");
    let parent = tmp.to_string_lossy().to_string();
    let result = create_directory_core(None, &parent, "").await;
    assert!(result.is_err());
    // allowed-error-string-match: the module returns a String; message is the signal
    assert!(result.unwrap_err().contains("cannot be empty"));
    cleanup_test_dir(&tmp);
}

#[tokio::test]
async fn create_directory_invalid_chars() {
    let tmp = create_test_dir("create_invalid");
    let parent = tmp.to_string_lossy().to_string();
    let result = create_directory_core(None, &parent, "foo/bar").await;
    assert!(result.is_err());
    // allowed-error-string-match: the module returns a String; message is the signal
    assert!(result.unwrap_err().contains("invalid characters"));

    let result = create_directory_core(None, &parent, "foo\0bar").await;
    assert!(result.is_err());
    // allowed-error-string-match: the module returns a String; message is the signal
    assert!(result.unwrap_err().contains("invalid characters"));
    cleanup_test_dir(&tmp);
}

#[tokio::test]
async fn create_directory_nonexistent_parent() {
    ensure_root_volume();
    let result = create_directory_core(None, "/nonexistent_path_12345", "test").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn create_directory_unregistered_volume_errors_without_fs_write() {
    // An unregistered volume_id used to fall back to an untimed synchronous
    // `std::fs::create_dir` on the async executor. Now it returns a typed
    // "Volume not found" error and writes nothing.
    let tmp = create_test_dir("create_unregistered_vol");
    let parent = tmp.to_string_lossy().to_string();
    let result = create_directory_core(Some("no-such-volume-xyz".to_string()), &parent, "would-be-folder").await;
    assert!(result.is_err());
    // allowed-error-string-match: the module returns a String; message is the signal
    assert!(result.unwrap_err().contains("Volume not found"));
    assert!(
        !tmp.join("would-be-folder").exists(),
        "no directory should be created when the volume isn't registered"
    );
    cleanup_test_dir(&tmp);
}

// ============================================================================
// create_file_core
// ============================================================================

#[tokio::test]
async fn create_file_unregistered_volume_errors_without_fs_write() {
    // Same contract as the directory case: an unregistered volume_id returns
    // a typed error instead of an untimed `std::fs::File::create_new`.
    let tmp = create_test_dir("create_file_unregistered_vol");
    let parent = tmp.to_string_lossy().to_string();
    let result = create_file_core(Some("no-such-volume-xyz".to_string()), &parent, "would-be-file.txt").await;
    assert!(result.is_err());
    // allowed-error-string-match: the module returns a String; message is the signal
    assert!(result.unwrap_err().contains("Volume not found"));
    assert!(
        !tmp.join("would-be-file.txt").exists(),
        "no file should be created when the volume isn't registered"
    );
    cleanup_test_dir(&tmp);
}

#[tokio::test]
async fn create_file_success() {
    ensure_root_volume();
    let tmp = create_test_dir("create_file_success");
    let parent = tmp.to_string_lossy().to_string();
    let result = create_file_core(None, &parent, "new-file.txt").await;
    assert!(result.is_ok());
    let (created_path, _) = result.unwrap();
    assert!(created_path.is_file());
    assert!(created_path.to_string_lossy().ends_with("new-file.txt"));
    assert_eq!(fs::read(&created_path).unwrap(), b"");
    cleanup_test_dir(&tmp);
}

#[tokio::test]
async fn create_file_already_exists() {
    ensure_root_volume();
    let tmp = create_test_dir("create_file_exists");
    let parent = tmp.to_string_lossy().to_string();
    fs::write(tmp.join("existing.txt"), b"hello").unwrap();
    let result = create_file_core(None, &parent, "existing.txt").await;
    assert!(result.is_err());
    // allowed-error-string-match: the module returns a String; message is the signal
    assert!(result.unwrap_err().contains("already exists"));
    cleanup_test_dir(&tmp);
}

#[tokio::test]
async fn create_file_empty_name() {
    let tmp = create_test_dir("create_file_empty");
    let parent = tmp.to_string_lossy().to_string();
    let result = create_file_core(None, &parent, "").await;
    assert!(result.is_err());
    // allowed-error-string-match: the module returns a String; message is the signal
    assert!(result.unwrap_err().contains("cannot be empty"));
    cleanup_test_dir(&tmp);
}

#[tokio::test]
async fn create_file_invalid_chars() {
    let tmp = create_test_dir("create_file_invalid");
    let parent = tmp.to_string_lossy().to_string();
    let result = create_file_core(None, &parent, "foo/bar.txt").await;
    assert!(result.is_err());
    // allowed-error-string-match: the module returns a String; message is the signal
    assert!(result.unwrap_err().contains("invalid characters"));

    let result = create_file_core(None, &parent, "foo\0bar.txt").await;
    assert!(result.is_err());
    // allowed-error-string-match: the module returns a String; message is the signal
    assert!(result.unwrap_err().contains("invalid characters"));
    cleanup_test_dir(&tmp);
}

// ============================================================================
// Managed wrapper (end-to-end through the manager)
// ============================================================================

#[tokio::test]
async fn create_directory_managed_creates_folder_and_cleans_up_record() {
    ensure_root_volume();
    let tmp = create_test_dir("create_managed_ok");
    let parent = tmp.to_string_lossy().to_string();
    let result = create_directory_managed(None, parent, "made".to_string()).await;
    assert!(result.is_ok(), "managed create returns the new path");
    let path = result.unwrap();
    assert!(path.ends_with("made"));
    assert!(Path::new(&path).is_dir());
    // The instant op's record is cleaned up once the create finishes.
    assert!(
        manager::manager()
            .list()
            .iter()
            .all(|o| o.operation_type != WriteOperationType::CreateFolder),
        "no lingering CreateFolder record after the managed create settles"
    );
    cleanup_test_dir(&tmp);
}

// ============================================================================
// Error mapping: a volume's PermissionDenied surfaces as the friendly message
// ============================================================================

/// A test `Volume` whose `create_directory` / `create_file` always return
/// `PermissionDenied`, to exercise the core's error mapping. Everything else is a
/// stub.
struct DeniedVolume {
    name: String,
    root: PathBuf,
}

impl Volume for DeniedVolume {
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
    fn create_directory<'a>(
        &'a self,
        _path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        Box::pin(async { Err(VolumeError::PermissionDenied("denied".to_string())) })
    }
    fn create_file<'a>(
        &'a self,
        _path: &'a Path,
        _content: &'a [u8],
    ) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        Box::pin(async { Err(VolumeError::PermissionDenied("denied".to_string())) })
    }
}

#[tokio::test]
async fn create_directory_core_maps_permission_denied_to_friendly_message() {
    let vid = unique("denied-dir");
    get_volume_manager().register(
        &vid,
        Arc::new(DeniedVolume {
            name: vid.clone(),
            root: PathBuf::from("/"),
        }),
    );
    let result = create_directory_core(Some(vid), "/somewhere", "folder").await;
    assert!(result.is_err());
    // allowed-error-string-match: the module returns a String; message is the signal
    let msg = result.unwrap_err();
    assert!(msg.contains("Permission denied"), "got: {msg}");
    assert!(msg.contains("folder") && msg.contains("/somewhere"), "got: {msg}");
}

#[tokio::test]
async fn create_file_core_maps_permission_denied_to_friendly_message() {
    let vid = unique("denied-file");
    get_volume_manager().register(
        &vid,
        Arc::new(DeniedVolume {
            name: vid.clone(),
            root: PathBuf::from("/"),
        }),
    );
    let result = create_file_core(Some(vid), "/somewhere", "file.txt").await;
    assert!(result.is_err());
    // allowed-error-string-match: the module returns a String; message is the signal
    let msg = result.unwrap_err();
    assert!(msg.contains("Permission denied"), "got: {msg}");
    assert!(msg.contains("file.txt") && msg.contains("/somewhere"), "got: {msg}");
}

/// Writes a file whose first bytes are a zip signature (enough for the boundary
/// magic check; these tests never parse the archive).
fn write_zip_magic(path: &Path) {
    fs::write(path, b"PK\x03\x04not-a-real-body").expect("write zip magic");
}

#[tokio::test]
async fn create_directory_core_rejects_a_target_inside_an_archive() {
    let dir = create_test_dir("archive-mkdir");
    let zip = dir.join("bundle.zip");
    write_zip_magic(&zip);

    // Parent is inside the archive → read-only until zip mutation lands.
    let parent = zip.join("sub");
    let err = create_directory_core(None, &parent.to_string_lossy(), "newdir")
        .await
        .expect_err("creating inside an archive must be refused");
    // allowed-error-string-match: the fn returns a String, and the archive-specific
    // refusal is the signal that the FORK fired — a natural mkdir failure (volume
    // not found, ENOTDIR) also errors, so `is_err()` alone wouldn't prove the guard.
    assert!(err.contains("archive"), "expected the archive refusal, got: {err}");
    cleanup_test_dir(&dir);
}

#[tokio::test]
async fn create_file_core_rejects_a_target_inside_an_archive() {
    let dir = create_test_dir("archive-mkfile");
    let zip = dir.join("bundle.zip");
    write_zip_magic(&zip);

    // The archive root itself is also read-only.
    let err = create_file_core(None, &zip.to_string_lossy(), "new.txt")
        .await
        .expect_err("creating inside an archive must be refused");
    // allowed-error-string-match: see `create_directory_core_rejects_...`.
    assert!(err.contains("archive"), "expected the archive refusal, got: {err}");
    cleanup_test_dir(&dir);
}
