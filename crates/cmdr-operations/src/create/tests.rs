//! Tests for new-folder / new-file creation.
//!
//! The `*_core` tests drive the create logic directly (no operation manager),
//! moved here with the logic from `commands/file_system/mod.rs`. The descriptor
//! test pins the busy-set wiring (root → nothing busy, non-root → the volume).

use super::*;
use crate::host::testing::PaneListings;
use crate::testing::{TestOperationLog, TestOperations};
use std::fs;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tempfile::TempDir;

use cmdr_fs::entry::FileEntry;
use cmdr_fs::volume::{LaneKey, ListingProgress};
use cmdr_fs::volume::{Volume, VolumeError};

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

// ============================================================================
// Descriptor / busy-set wiring
// ============================================================================

#[test]
fn instant_descriptor_marks_only_nonroot_volumes_busy() {
    let engine = TestOperations::new();
    let host = engine.host();
    // Root (or no volume) → no busy volume (root is never ejectable).
    let root_none = instant_descriptor(host, WriteOperationType::CreateFolder, None, "new");
    assert!(root_none.volume_ids.is_empty());
    assert!(root_none.lanes.is_empty(), "instant ops never reserve a lane");

    let root_explicit = instant_descriptor(host, WriteOperationType::CreateFile, Some("root"), "new");
    assert!(root_explicit.volume_ids.is_empty());

    // A real volume → marked busy for the op's duration.
    let device = instant_descriptor(host, WriteOperationType::CreateFolder, Some("usb-42"), "new");
    assert_eq!(device.volume_ids, vec!["usb-42".to_string()]);
    assert!(device.lanes.is_empty());
    assert_eq!(device.summary.source.as_deref(), Some("new"));

    // A host with no local disk (the browser demo): its `root` is a volume like any other.
    engine.volumes().without_local_disk();
    let demo_root = instant_descriptor(host, WriteOperationType::CreateFile, Some("root"), "new");
    assert_eq!(demo_root.volume_ids, vec!["root".to_string()]);
}

// ============================================================================
// create_directory_core_on
// ============================================================================

#[tokio::test]
async fn create_directory_managed_journals_a_create_folder_op() {
    use cmdr_operation_log::store::{open_read_connection, read_operation, read_operation_items};
    use cmdr_operation_log::types::{EntryType, OpKind, RollbackState};

    let log = TestOperationLog::new("create_managed_journal");
    let engine = TestOperations::journaling_into(&log);
    engine.register_local_root();

    let tmp = create_test_dir("managed_journal");
    let parent = tmp.path().to_string_lossy().to_string();
    let created = engine
        .create_directory_managed(
            None,
            parent,
            "made-folder".to_string(),
            cmdr_operation_log::types::Initiator::User,
        )
        .await
        .expect("mkdir");

    let conn = open_read_connection(log.db()).expect("read conn");
    // Exactly one CreateFolder op, rollbackable (net-new), with one Dir item row.
    let ops = cmdr_operation_log::store::recent_operations(&conn, 10).expect("ops");
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
    let engine = TestOperations::new();
    engine.register_local_root();
    let tmp = create_test_dir("create_success");
    let parent = tmp.path().to_string_lossy().to_string();
    let result = create_directory_core_on(engine.host(), None, &parent, "new-folder").await;
    assert!(result.is_ok());
    let (created_path, _) = result.unwrap();
    assert!(created_path.is_dir());
    assert!(created_path.to_string_lossy().ends_with("new-folder"));
}

#[tokio::test]
async fn create_directory_already_exists() {
    let engine = TestOperations::new();
    engine.register_local_root();
    let tmp = create_test_dir("create_exists");
    let parent = tmp.path().to_string_lossy().to_string();
    fs::create_dir(tmp.path().join("existing")).unwrap();
    let result = create_directory_core_on(engine.host(), None, &parent, "existing").await;
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), MutationError::AlreadyExists { .. }));
}

#[tokio::test]
async fn create_directory_empty_name() {
    let engine = TestOperations::new();
    let tmp = create_test_dir("create_empty");
    let parent = tmp.path().to_string_lossy().to_string();
    let result = create_directory_core_on(engine.host(), None, &parent, "").await;
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), MutationError::NameEmpty));
}

#[tokio::test]
async fn create_directory_invalid_chars() {
    let engine = TestOperations::new();
    let tmp = create_test_dir("create_invalid");
    let parent = tmp.path().to_string_lossy().to_string();
    let result = create_directory_core_on(engine.host(), None, &parent, "foo/bar").await;
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), MutationError::NameHasDisallowedCharacter));

    let result = create_directory_core_on(engine.host(), None, &parent, "foo\0bar").await;
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), MutationError::NameHasDisallowedCharacter));
}

#[tokio::test]
async fn create_directory_nonexistent_parent() {
    let engine = TestOperations::new();
    engine.register_local_root();
    let result = create_directory_core_on(engine.host(), None, "/nonexistent_path_12345", "test").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn create_directory_unregistered_volume_errors_without_fs_write() {
    // An unregistered volume_id used to fall back to an untimed synchronous
    // `std::fs::create_dir` on the async executor. Now it returns a typed
    // "Volume not found" error and writes nothing.
    let engine = TestOperations::new();
    let tmp = create_test_dir("create_unregistered_vol");
    let parent = tmp.path().to_string_lossy().to_string();
    let result = create_directory_core_on(
        engine.host(),
        Some("no-such-volume-xyz".to_string()),
        &parent,
        "would-be-folder",
    )
    .await;
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), MutationError::VolumeGone { .. }));
    assert!(
        !tmp.path().join("would-be-folder").exists(),
        "no directory should be created when the volume isn't registered"
    );
}

// ============================================================================
// create_file_core_on
// ============================================================================

#[tokio::test]
async fn create_file_unregistered_volume_errors_without_fs_write() {
    // Same contract as the directory case: an unregistered volume_id returns
    // a typed error instead of an untimed `std::fs::File::create_new`.
    let engine = TestOperations::new();
    let tmp = create_test_dir("create_file_unregistered_vol");
    let parent = tmp.path().to_string_lossy().to_string();
    let result = create_file_core_on(
        engine.host(),
        Some("no-such-volume-xyz".to_string()),
        &parent,
        "would-be-file.txt",
    )
    .await;
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), MutationError::VolumeGone { .. }));
    assert!(
        !tmp.path().join("would-be-file.txt").exists(),
        "no file should be created when the volume isn't registered"
    );
}

#[tokio::test]
async fn create_file_success() {
    let engine = TestOperations::new();
    engine.register_local_root();
    let tmp = create_test_dir("create_file_success");
    let parent = tmp.path().to_string_lossy().to_string();
    let result = create_file_core_on(engine.host(), None, &parent, "new-file.txt").await;
    assert!(result.is_ok());
    let (created_path, _) = result.unwrap();
    assert!(created_path.is_file());
    assert!(created_path.to_string_lossy().ends_with("new-file.txt"));
    assert_eq!(fs::read(&created_path).unwrap(), b"");
}

#[tokio::test]
async fn create_file_already_exists() {
    let engine = TestOperations::new();
    engine.register_local_root();
    let tmp = create_test_dir("create_file_exists");
    let parent = tmp.path().to_string_lossy().to_string();
    fs::write(tmp.path().join("existing.txt"), b"hello").unwrap();
    let result = create_file_core_on(engine.host(), None, &parent, "existing.txt").await;
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), MutationError::AlreadyExists { .. }));
}

#[tokio::test]
async fn create_file_empty_name() {
    let engine = TestOperations::new();
    let tmp = create_test_dir("create_file_empty");
    let parent = tmp.path().to_string_lossy().to_string();
    let result = create_file_core_on(engine.host(), None, &parent, "").await;
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), MutationError::NameEmpty));
}

#[tokio::test]
async fn create_file_invalid_chars() {
    let engine = TestOperations::new();
    let tmp = create_test_dir("create_file_invalid");
    let parent = tmp.path().to_string_lossy().to_string();
    let result = create_file_core_on(engine.host(), None, &parent, "foo/bar.txt").await;
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), MutationError::NameHasDisallowedCharacter));

    let result = create_file_core_on(engine.host(), None, &parent, "foo\0bar.txt").await;
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), MutationError::NameHasDisallowedCharacter));
}

// ============================================================================
// Managed wrapper (end-to-end through the manager)
// ============================================================================

#[tokio::test]
async fn create_directory_managed_creates_folder_and_cleans_up_record() {
    let engine = TestOperations::new();
    engine.register_local_root();
    let tmp = create_test_dir("create_managed_ok");
    let parent = tmp.path().to_string_lossy().to_string();
    let result = engine
        .create_directory_managed(
            None,
            parent,
            "made".to_string(),
            cmdr_operation_log::types::Initiator::User,
        )
        .await;
    assert!(result.is_ok(), "managed create returns the new path");
    let path = result.unwrap();
    assert!(path.ends_with("made"));
    assert!(Path::new(&path).is_dir());
    // The instant op's record is cleaned up once the create finishes.
    assert!(
        engine
            .manager()
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
        Box::pin(async move {
            Err(VolumeError::PermissionDenied {
                path: path.display().to_string(),
                raw_os_error: None,
            })
        })
    }
    fn create_file<'a>(
        &'a self,
        path: &'a Path,
        _content: &'a [u8],
    ) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            Err(VolumeError::PermissionDenied {
                path: path.display().to_string(),
                raw_os_error: None,
            })
        })
    }
}

#[tokio::test]
async fn create_directory_core_maps_permission_denied_to_friendly_message() {
    let vid = unique("denied-dir");
    let engine = TestOperations::new();
    engine.volumes().register(
        &vid,
        Arc::new(DeniedVolume {
            name: vid.clone(),
            root: PathBuf::from("/"),
        }),
    );
    let result = create_directory_core_on(engine.host(), Some(vid), "/somewhere", "folder").await;
    assert!(result.is_err());
    // The volume's own refusal rides through untouched, path and all, so the
    // frontend words it from the variant instead of parsing a sentence.
    let err = result.unwrap_err();
    assert!(
        matches!(&err, MutationError::Volume { error: VolumeError::PermissionDenied { path, .. } } if path.contains("/somewhere")),
        "got: {err:?}",
    );
}

#[tokio::test]
async fn create_file_core_maps_permission_denied_to_friendly_message() {
    let vid = unique("denied-file");
    let engine = TestOperations::new();
    engine.volumes().register(
        &vid,
        Arc::new(DeniedVolume {
            name: vid.clone(),
            root: PathBuf::from("/"),
        }),
    );
    let result = create_file_core_on(engine.host(), Some(vid), "/somewhere", "file.txt").await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        matches!(&err, MutationError::Volume { error: VolumeError::PermissionDenied { path, .. } } if path.contains("/somewhere")),
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
    let engine = TestOperations::new();
    let dir = create_test_dir("archive-mkdir");
    let zip = dir.path().join("bundle.zip");
    write_zip_magic(&zip);

    // Parent is inside the archive → read-only until zip mutation lands.
    let parent = zip.join("sub");
    let err = create_directory_core_on(engine.host(), None, &parent.to_string_lossy(), "newdir")
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
    let engine = TestOperations::new();
    let dir = create_test_dir("archive-mkfile");
    let zip = dir.path().join("bundle.zip");
    write_zip_magic(&zip);

    // The archive root itself is also read-only.
    let err = create_file_core_on(engine.host(), None, &zip.to_string_lossy(), "new.txt")
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
    let engine = TestOperations::new();
    let dir = create_test_dir("archive-dup-create");
    let zip = dir.path().join("bundle.zip");
    write_real_zip(&zip, &[("existing.txt", b"x"), ("sub/existing.txt", b"y")]);

    // mkfile onto an existing name at the archive root.
    let root_parent = zip.to_string_lossy().to_string();
    let err_file = route_archive_create(&engine, &root_parent, "existing.txt", ArchiveEntryKind::File, None)
        .await
        .expect_err("mkfile onto an existing inner name must be refused");
    assert!(
        matches!(&err_file, MutationError::AlreadyExists { name } if name == "existing.txt"),
        "got: {err_file:?}",
    );

    // mkdir onto an existing name inside a subdirectory.
    let sub_parent = zip.join("sub").to_string_lossy().to_string();
    let err_dir = route_archive_create(&engine, &sub_parent, "existing.txt", ArchiveEntryKind::Dir, None)
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

// ============================================================================
// Synthetic listing diff: the instant row lands only in panes on the create's volume
// ============================================================================

/// A create writes through ONE volume, so its instant row belongs only to panes
/// on that volume. A pane on another volume showing the same path string (an
/// SFTP pane on a remote `/tmp` beside a local `/tmp`) must not gain it; a local
/// pane on a different volume picks the entry up from its own watch instead.
#[test]
fn synthetic_entry_diff_lands_only_in_listings_on_the_creates_volume() {
    let panes = PaneListings::new();
    use cmdr_listing::testing::TestListing;

    let engine = TestOperations::seeing(&panes);
    let tmp = create_test_dir("synthetic_diff_volume");
    let new_path = tmp.path().join("made");
    fs::create_dir(&new_path).expect("create the new folder on disk");

    let on_root = TestListing::new()
        .path(tmp.path())
        .insert(panes.panes(), "synthetic-diff-root");
    let other_volume_id = unique("synthetic-diff-remote");
    let on_other = TestListing::new()
        .volume(&other_volume_id)
        .path(tmp.path())
        .insert(panes.panes(), "synthetic-diff-remote");

    // `None` is the local filesystem, `root`.
    patch_listings_after_create(engine.host(), None, &new_path, tmp.path());

    assert_eq!(
        on_root.entry_names(),
        vec!["made"],
        "the create's own volume gains the row"
    );
    assert!(
        on_other.entry_names().is_empty(),
        "a pane on another volume showing the same path stays untouched"
    );
}

/// A create whose path the listing already holds (the watcher stat'd it first,
/// or a stale row for a file deleted outside Cmdr whose removal hasn't landed)
/// refreshes that row with the create's own stat, as every other local mutation
/// does (`notify_added`'s upsert). No create overwrites (mkdir and mkfile refuse a
/// taken name, paste retries `pasted (N)`), so the row at that path is this file.
#[test]
fn synthetic_entry_diff_refreshes_a_row_the_listing_already_holds() {
    let panes = PaneListings::new();
    use cmdr_listing::DiffChangeType;
    use cmdr_listing::testing::TestListing;
    use cmdr_listing::testing::{hold_diffs, pending_changes};
    use cmdr_local::reading::get_single_entry;

    let engine = TestOperations::seeing(&panes);
    let tmp = create_test_dir("synthetic_diff_duplicate");
    let new_path = tmp.path().join("made.txt");
    fs::write(&new_path, b"fresh contents").expect("write the new file on disk");
    let mut stale = get_single_entry(&new_path).expect("stat the new file");
    stale.size = Some(1);
    let listing = TestListing::new()
        .path(tmp.path())
        .entries(vec![stale])
        .insert(panes.panes(), "synthetic-diff-duplicate");
    hold_diffs(listing.engine(), listing.id());

    patch_listings_after_create(engine.host(), None, &new_path, tmp.path());

    let entries = listing.entries();
    assert_eq!(entries.len(), 1, "the create upserts; it never adds a second row");
    assert_eq!(entries[0].size, Some(14), "the row carries the on-disk size");
    let changes = pending_changes(listing.engine(), listing.id());
    assert_eq!(changes.len(), 1, "the pane hears one change");
    assert!(
        matches!(changes[0].change_type, DiffChangeType::Modify),
        "the pane hears a modify, got {:?}",
        changes[0].change_type
    );
}
