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
use tempfile::TempDir;

use crate::file_system::volume::{LaneKey, ListingProgress};
use crate::file_system::{FileEntry, Volume, VolumeError};

fn unique(label: &str) -> String {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    format!("create-test-{label}-{n}-{:?}", std::thread::current().id())
}

/// A scratch directory owned by ONE test run, removed when the returned handle
/// drops (unwind included).
///
/// It has to be process-unique, not merely test-unique: a fixed
/// `/tmp/cmdr_create_test_<label>` is shared by every process on the machine, so
/// two concurrent suite runs (parallel worktrees, or CI beside a local run) land
/// on the same path and the second run's `remove_dir_all` deletes the first
/// run's fixture out from under it. `label` survives only to name the dir
/// readably while it exists.
fn create_test_dir(label: &str) -> TempDir {
    tempfile::Builder::new()
        .prefix(&format!("cmdr_create_test_{label}_"))
        .tempdir()
        .expect("Failed to create test directory")
}

/// Registers a real local-FS "root" volume so `create_*_core` with
/// `volume_id = None` (→ "root") exercises the timed `Volume` path, the same one
/// production hits. Idempotent via `register_if_absent`.
fn ensure_root_volume() {
    use crate::file_system::volume::LocalPosixVolume;
    use crate::file_system::volume::manager::get_volume_manager;
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
async fn create_directory_managed_journals_a_create_folder_op() {
    use crate::operation_log::capture::WriterJournal;
    use crate::operation_log::store::{
        open_read_connection, operation_log_db_path, read_operation, read_operation_items,
    };
    use crate::operation_log::types::{EntryType, OpKind, RollbackState};
    use crate::operation_log::writer::OperationLogWriter;

    ensure_root_volume();
    let jdir = tempfile::tempdir().expect("jdir");
    let jdb = operation_log_db_path(jdir.path());
    // Serializes journal-slot tests under plain `cargo test`; clears on drop.
    let _journal = crate::operation_log::TestJournalGuard::install(Arc::new(WriterJournal::new(
        OperationLogWriter::spawn(&jdb).expect("writer"),
    )));

    let tmp = create_test_dir("managed_journal");
    let parent = tmp.path().to_string_lossy().to_string();
    let created = create_directory_managed(
        None,
        parent,
        "made-folder".to_string(),
        crate::operation_log::types::Initiator::User,
    )
    .await
    .expect("mkdir");

    let conn = open_read_connection(&jdb).expect("read conn");
    // Exactly one CreateFolder op, rollbackable (net-new), with one Dir item row.
    let ops = crate::operation_log::store::recent_operations(&conn, 10).expect("ops");
    let op = ops
        .iter()
        .find(|o| o.kind == OpKind::CreateFolder)
        .expect("a create_folder op");
    assert_eq!(op.rollback_state, RollbackState::Rollbackable);
    let items = read_operation_items(&conn, &op.op_id, 10).expect("items");
    assert_eq!(items.len(), 1, "expected one created-dir row, got {items:?}");
    assert_eq!(items[0].entry_type, EntryType::Dir);
    assert_eq!(items[0].source_name, "made-folder");
    let _ = read_operation(&conn, &op.op_id);
    assert!(created.ends_with("made-folder"));
}

#[tokio::test]
async fn create_directory_success() {
    ensure_root_volume();
    let tmp = create_test_dir("create_success");
    let parent = tmp.path().to_string_lossy().to_string();
    let result = create_directory_core(None, &parent, "new-folder").await;
    assert!(result.is_ok());
    let (created_path, _) = result.unwrap();
    assert!(created_path.is_dir());
    assert!(created_path.to_string_lossy().ends_with("new-folder"));
}

#[tokio::test]
async fn create_directory_already_exists() {
    ensure_root_volume();
    let tmp = create_test_dir("create_exists");
    let parent = tmp.path().to_string_lossy().to_string();
    fs::create_dir(tmp.path().join("existing")).unwrap();
    let result = create_directory_core(None, &parent, "existing").await;
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), MutationError::AlreadyExists { .. }));
}

#[tokio::test]
async fn create_directory_empty_name() {
    let tmp = create_test_dir("create_empty");
    let parent = tmp.path().to_string_lossy().to_string();
    let result = create_directory_core(None, &parent, "").await;
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), MutationError::NameEmpty));
}

#[tokio::test]
async fn create_directory_invalid_chars() {
    let tmp = create_test_dir("create_invalid");
    let parent = tmp.path().to_string_lossy().to_string();
    let result = create_directory_core(None, &parent, "foo/bar").await;
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), MutationError::NameHasDisallowedCharacter));

    let result = create_directory_core(None, &parent, "foo\0bar").await;
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), MutationError::NameHasDisallowedCharacter));
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
    let parent = tmp.path().to_string_lossy().to_string();
    let result = create_directory_core(Some("no-such-volume-xyz".to_string()), &parent, "would-be-folder").await;
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), MutationError::VolumeGone { .. }));
    assert!(
        !tmp.path().join("would-be-folder").exists(),
        "no directory should be created when the volume isn't registered"
    );
}

// ============================================================================
// create_file_core
// ============================================================================

#[tokio::test]
async fn create_file_unregistered_volume_errors_without_fs_write() {
    // Same contract as the directory case: an unregistered volume_id returns
    // a typed error instead of an untimed `std::fs::File::create_new`.
    let tmp = create_test_dir("create_file_unregistered_vol");
    let parent = tmp.path().to_string_lossy().to_string();
    let result = create_file_core(Some("no-such-volume-xyz".to_string()), &parent, "would-be-file.txt").await;
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), MutationError::VolumeGone { .. }));
    assert!(
        !tmp.path().join("would-be-file.txt").exists(),
        "no file should be created when the volume isn't registered"
    );
}

#[tokio::test]
async fn create_file_success() {
    ensure_root_volume();
    let tmp = create_test_dir("create_file_success");
    let parent = tmp.path().to_string_lossy().to_string();
    let result = create_file_core(None, &parent, "new-file.txt").await;
    assert!(result.is_ok());
    let (created_path, _) = result.unwrap();
    assert!(created_path.is_file());
    assert!(created_path.to_string_lossy().ends_with("new-file.txt"));
    assert_eq!(fs::read(&created_path).unwrap(), b"");
}

#[tokio::test]
async fn create_file_already_exists() {
    ensure_root_volume();
    let tmp = create_test_dir("create_file_exists");
    let parent = tmp.path().to_string_lossy().to_string();
    fs::write(tmp.path().join("existing.txt"), b"hello").unwrap();
    let result = create_file_core(None, &parent, "existing.txt").await;
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), MutationError::AlreadyExists { .. }));
}

#[tokio::test]
async fn create_file_empty_name() {
    let tmp = create_test_dir("create_file_empty");
    let parent = tmp.path().to_string_lossy().to_string();
    let result = create_file_core(None, &parent, "").await;
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), MutationError::NameEmpty));
}

#[tokio::test]
async fn create_file_invalid_chars() {
    let tmp = create_test_dir("create_file_invalid");
    let parent = tmp.path().to_string_lossy().to_string();
    let result = create_file_core(None, &parent, "foo/bar.txt").await;
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), MutationError::NameHasDisallowedCharacter));

    let result = create_file_core(None, &parent, "foo\0bar.txt").await;
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), MutationError::NameHasDisallowedCharacter));
}

// ============================================================================
// Managed wrapper (end-to-end through the manager)
// ============================================================================

#[tokio::test]
async fn create_directory_managed_creates_folder_and_cleans_up_record() {
    ensure_root_volume();
    let tmp = create_test_dir("create_managed_ok");
    let parent = tmp.path().to_string_lossy().to_string();
    let result = create_directory_managed(
        None,
        parent,
        "made".to_string(),
        crate::operation_log::types::Initiator::User,
    )
    .await;
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
    // Carries the PATH, like every real backend: `PermissionDenied` is defined to,
    // and a double that answers with its own word instead would let a regression
    // in the path payload pass here.
    fn create_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        Box::pin(async move { Err(VolumeError::PermissionDenied(path.display().to_string())) })
    }
    fn create_file<'a>(
        &'a self,
        path: &'a Path,
        _content: &'a [u8],
    ) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        Box::pin(async move { Err(VolumeError::PermissionDenied(path.display().to_string())) })
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
    // The volume's own refusal rides through untouched, path and all, so the
    // frontend words it from the variant instead of parsing a sentence.
    let err = result.unwrap_err();
    assert!(
        matches!(&err, MutationError::Volume { error: VolumeError::PermissionDenied(path) } if path.contains("/somewhere")),
        "got: {err:?}",
    );
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
    let err = result.unwrap_err();
    assert!(
        matches!(&err, MutationError::Volume { error: VolumeError::PermissionDenied(path) } if path.contains("/somewhere")),
        "got: {err:?}",
    );
}

/// Writes a file whose first bytes are a zip signature (enough for the boundary
/// magic check; these tests never parse the archive).
fn write_zip_magic(path: &Path) {
    fs::write(path, b"PK\x03\x04not-a-real-body").expect("write zip magic");
}

#[tokio::test]
async fn create_directory_core_rejects_a_target_inside_an_archive() {
    let dir = create_test_dir("archive-mkdir");
    let zip = dir.path().join("bundle.zip");
    write_zip_magic(&zip);

    // Parent is inside the archive → read-only until zip mutation lands.
    let parent = zip.join("sub");
    let err = create_directory_core(None, &parent.to_string_lossy(), "newdir")
        .await
        .expect_err("creating inside an archive must be refused");
    // The archive-specific variant is the signal that the FORK fired: a natural
    // mkdir failure (volume not found, ENOTDIR) also errors, so `is_err()` alone
    // wouldn't prove the guard.
    assert!(
        matches!(err, MutationError::ArchiveNotEditable),
        "expected the archive refusal, got: {err:?}",
    );
}

#[tokio::test]
async fn create_file_core_rejects_a_target_inside_an_archive() {
    let dir = create_test_dir("archive-mkfile");
    let zip = dir.path().join("bundle.zip");
    write_zip_magic(&zip);

    // The archive root itself is also read-only.
    let err = create_file_core(None, &zip.to_string_lossy(), "new.txt")
        .await
        .expect_err("creating inside an archive must be refused");
    // See `create_directory_core_rejects_...`.
    assert!(
        matches!(err, MutationError::ArchiveNotEditable),
        "expected the archive refusal, got: {err:?}",
    );
}

/// Builds a real, parseable zip with the given entries.
fn write_real_zip(path: &Path, entries: &[(&str, &[u8])]) {
    use std::io::Write;
    use zip::ZipWriter;
    use zip::write::SimpleFileOptions;
    let file = fs::File::create(path).expect("create zip");
    let mut writer = ZipWriter::new(file);
    for (name, content) in entries {
        writer.start_file(*name, SimpleFileOptions::default()).expect("start");
        writer.write_all(content).expect("write");
    }
    writer.finish().expect("finish");
}

#[tokio::test]
async fn route_archive_create_on_an_existing_inner_name_errors_without_building_a_temp() {
    // A duplicate mkdir/mkfile inside a zip must be rejected UP FRONT with the
    // standard "already exists" message (matching the real-FS paths), so the FE
    // shows the friendly copy instead of the raw `zip` "Duplicate filename" — and
    // no temp is built for the doomed edit.
    let dir = create_test_dir("archive-dup-create");
    let zip = dir.path().join("bundle.zip");
    write_real_zip(&zip, &[("existing.txt", b"x"), ("sub/existing.txt", b"y")]);

    // mkfile onto an existing name at the archive root.
    let root_parent = zip.to_string_lossy().to_string();
    let err_file = route_archive_create(&root_parent, "existing.txt", ArchiveEntryKind::File, None)
        .await
        .expect_err("mkfile onto an existing inner name must be refused");
    assert!(
        matches!(&err_file, MutationError::AlreadyExists { name } if name == "existing.txt"),
        "got: {err_file:?}",
    );

    // mkdir onto an existing name inside a subdirectory.
    let sub_parent = zip.join("sub").to_string_lossy().to_string();
    let err_dir = route_archive_create(&sub_parent, "existing.txt", ArchiveEntryKind::Dir, None)
        .await
        .expect_err("mkdir onto an existing inner name must be refused");
    assert!(
        matches!(&err_dir, MutationError::AlreadyExists { name } if name == "existing.txt"),
        "got: {err_dir:?}",
    );

    // Neither doomed edit built a temp.
    let temps: Vec<_> = fs::read_dir(dir.path())
        .expect("read dir")
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().contains(".cmdr-tmp-"))
        .collect();
    assert!(
        temps.is_empty(),
        "a pre-checked duplicate must not build a temp, found {temps:?}"
    );
}
