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

use crate::file_system::volume::manager::get_volume_manager;
use crate::file_system::volume::{LaneKey, ListingProgress};
use crate::file_system::write_operations::busy_volume_ids;
use crate::file_system::{FileEntry, Volume, VolumeError};
use crate::test_support::TestDir;

fn unique(label: &str) -> String {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    format!("rename-test-{label}-{n}-{:?}", std::thread::current().id())
}

fn create_test_dir(name: &str) -> TestDir {
    TestDir::new(&format!("rename_mod_test_{}", name))
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
    let result = rename_managed(old.clone(), new.clone(), false, "root".to_string(), Initiator::User).await;
    assert!(result.is_ok());
    assert!(!old.exists());
    assert_eq!(fs::read_to_string(&new).unwrap(), "content");
    let _ = fs::remove_dir_all(&tmp);
}

#[tokio::test]
async fn rename_managed_renames_a_zip_file_itself() {
    // The `.zip` file is a regular file: renaming it must work like any other file.
    // Only a rename INSIDE the archive is refused (covered by the reject test).
    let tmp = create_test_dir("managed_zip_rename");
    let old = tmp.join("bundle.zip");
    let new = tmp.join("renamed.zip");
    fs::write(&old, b"PK\x03\x04rest").unwrap();
    let result = rename_managed(old.clone(), new.clone(), false, "root".to_string(), Initiator::User).await;
    assert!(result.is_ok(), "renaming the .zip file itself must succeed: {result:?}");
    assert!(!old.exists());
    assert!(new.exists());
    let _ = fs::remove_dir_all(&tmp);
}

#[tokio::test]
async fn rename_managed_local_conflict_without_force_is_transparent() {
    let tmp = create_test_dir("managed_conflict");
    let old = tmp.join("old.txt");
    let new = tmp.join("new.txt");
    fs::write(&old, "old").unwrap();
    fs::write(&new, "new").unwrap();
    let result = rename_managed(old.clone(), new.clone(), false, "root".to_string(), Initiator::User).await;
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), MutationError::AlreadyExists { .. }));
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
    let result = rename_managed(old.clone(), new.clone(), true, "root".to_string(), Initiator::User).await;
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
    let handle = tokio::spawn(async move {
        rename_managed(
            PathBuf::from("/old"),
            PathBuf::from("/new"),
            false,
            vid,
            Initiator::User,
        )
        .await
    });

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

#[tokio::test]
async fn rename_managed_routes_an_in_archive_rename_to_the_edit_driver() {
    // Routing detection is parent-aware (`VolumeManager::path_is_inside_archive`),
    // so it needs a registered local `"root"` volume to confirm the boundary — as
    // production always has. (nextest isolates the global per test process.)
    get_volume_manager().register_if_absent(
        "root",
        Arc::new(crate::file_system::volume::LocalPosixVolume::new("Test root", "/")),
    );

    let tmp = create_test_dir("archive_rename");
    let zip = tmp.join("bundle.zip");
    fs::write(&zip, b"PK\x03\x04not-a-real-body").expect("write zip magic");

    // A rename INSIDE the archive routes to the managed edit driver. With no app
    // handle wired in the unit test, `global_tauri_sink()` is absent, so routing
    // surfaces the "app isn't ready" signal — which proves the ROUTING fork fired
    // rather than the old flat "isn't available yet" refusal or an instant rename.
    let from = zip.join("old.txt");
    let to = zip.join("new.txt");
    let err = rename_managed(from, to, false, "root".to_string(), Initiator::User)
        .await
        .expect_err("routing needs an app handle the unit test doesn't wire");
    // The archive-specific variant is how we tell the routing fork fired from a
    // natural rename failure.
    assert!(
        matches!(
            err,
            MutationError::ArchiveNotEditable | MutationError::ArchiveEditNotReady
        ),
        "expected the archive-routing signal, got: {err:?}"
    );

    // A cross-boundary rename (OUT of the archive) is refused as a move, a
    // deterministic routing decision that needs no app handle.
    let outside = tmp.join("out.txt");
    let cross = rename_managed(zip.join("old.txt"), outside, false, "root".to_string(), Initiator::User)
        .await
        .expect_err("a cross-boundary rename is refused");
    assert!(
        matches!(cross, MutationError::RenameOutOfArchive),
        "a cross-boundary rename should suggest a move instead, got: {cross:?}"
    );
    let _ = fs::remove_dir_all(&tmp);
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
async fn route_archive_rename_onto_an_existing_name_errors_without_building_a_temp() {
    // Renaming an in-archive entry onto a name that already exists must be
    // rejected up front with the standard "already exists" message, so the FE
    // shows the friendly copy instead of the raw `zip` "Duplicate filename" — and
    // no temp is built.
    let tmp = create_test_dir("archive_rename_dup");
    let zip = tmp.join("bundle.zip");
    write_real_zip(&zip, &[("old.txt", b"o"), ("taken.txt", b"t")]);

    let from = zip.join("old.txt");
    let to = zip.join("taken.txt");
    let err = route_archive_rename(&from, &to, "root")
        .await
        .expect_err("renaming onto an existing name must be refused");
    assert!(
        matches!(&err, MutationError::AlreadyExists { name } if name == "taken.txt"),
        "got: {err:?}",
    );

    let temps: Vec<_> = fs::read_dir(&tmp)
        .expect("read dir")
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().contains(".cmdr-tmp-"))
        .collect();
    assert!(
        temps.is_empty(),
        "a pre-checked duplicate rename must not build a temp, found {temps:?}"
    );
    let _ = fs::remove_dir_all(&tmp);
}

// ============================================================================
// Renames that copy (an object store's folders): routed, never `rename`
// ============================================================================

/// An object store's double, registered under a fresh id, holding `/a/foo`
/// with `files` files in it.
async fn copying_store(label: &str, files: usize) -> (String, Arc<crate::file_system::volume::InMemoryVolume>) {
    let volume_id = unique(label);
    let volume = Arc::new(
        crate::file_system::volume::InMemoryVolume::new(&volume_id)
            .with_whole_publish()
            .with_renames_by_copy()
            .with_space_info(10_000_000, 10_000_000),
    );
    volume.create_directory(Path::new("/a")).await.unwrap();
    volume.create_directory(Path::new("/a/foo")).await.unwrap();
    for n in 0..files {
        volume
            .create_file(&PathBuf::from(format!("/a/foo/f{n:03}.txt")), b"xy")
            .await
            .unwrap();
    }
    get_volume_manager().register(&volume_id, Arc::clone(&volume) as Arc<dyn Volume>);
    (volume_id, volume)
}

/// ❗ A rename that copies never reaches `Volume::rename` (the double refuses
/// it, which would come back as a `Volume` refusal): it starts a move, which a
/// unit test, with no app to emit through, can't, and says so.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_rename_that_copies_starts_a_move_instead_of_calling_rename() {
    let (volume_id, volume) = copying_store("copy-rename", 2).await;

    let result = rename_managed(
        PathBuf::from("/a/foo"),
        PathBuf::from("/a/bar"),
        false,
        volume_id,
        Initiator::User,
    )
    .await;

    assert!(
        matches!(result, Err(MutationError::Unexpected { .. })),
        "routed to the move starter, never to `rename`: {result:?}"
    );
    assert!(volume.exists(Path::new("/a/foo/f000.txt")).await, "nothing moved");
}

/// F2 hears what a copying rename carries, and whether to confirm it first.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_validity_check_reports_the_move_a_copying_rename_runs_as() {
    let (small_id, _small) = copying_store("copy-validity-small", 3).await;
    let small = check_rename_validity_impl("/a".into(), "foo".into(), "bar".into(), small_id, None).await;
    assert_eq!(
        small.by_move,
        Some(RenameByMove {
            files: 3,
            bytes: 6,
            counted_all: true,
            confirm_first: false,
        })
    );

    let (big_id, _big) = copying_store("copy-validity-big", 101).await;
    let big = check_rename_validity_impl("/a".into(), "foo".into(), "bar".into(), big_id, None).await;
    let by_move = big.by_move.expect("a copying rename reports its move");
    assert!(by_move.confirm_first, "past 100 files it asks first: {by_move:?}");
    assert!(!by_move.counted_all, "and the count stopped at its cap: {by_move:?}");

    let plain_id = unique("plain-validity");
    let plain = Arc::new(crate::file_system::volume::InMemoryVolume::new(&plain_id));
    plain.create_directory(Path::new("/a/foo")).await.unwrap();
    get_volume_manager().register(&plain_id, plain as Arc<dyn Volume>);
    let one_call = check_rename_validity_impl("/a".into(), "foo".into(), "bar".into(), plain_id, None).await;
    assert_eq!(one_call.by_move, None, "a one-call rename carries nothing");
}

/// A few files that cost something to move (young objects on Wasabi) ask
/// first too: only a free, small, fully counted rename starts unasked.
#[test]
fn a_small_rename_that_costs_money_asks_first() {
    let small = crate::file_system::volume::SubtreeTally {
        files: 3,
        bytes: 6,
        folders: 1,
        per_file: Vec::new(),
        complete: true,
    };
    assert!(!RenameByMove::from_tally(Some(small.clone()), false).confirm_first);
    assert!(RenameByMove::from_tally(Some(small), true).confirm_first);
}

/// A reviewed batch where a rename copies runs as ONE move with the new
/// names, and lands every one of them.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_batch_where_a_rename_copies_runs_as_one_move() {
    use crate::file_system::write_operations::event_sinks::CollectorEventSink;
    use crate::ignore_poison::IgnorePoison;

    let (volume_id, volume) = copying_store("copy-batch", 2).await;
    volume.create_file(Path::new("/a/note.txt"), b"n").await.unwrap();
    let mut rows = Vec::new();
    for (id, from, to) in [("1", "/a/foo", "/a/bar"), ("2", "/a/note.txt", "/a/memo.txt")] {
        rows.push(BulkRenameRow {
            row_id: id.to_string(),
            source: PathBuf::from(from),
            destination: PathBuf::from(to),
            expected_fingerprint: crate::file_system::write_operations::SourceFingerprint::capture_remote(
                volume.as_ref(),
                Path::new(from),
            )
            .await
            .expect("a fingerprint"),
        });
    }
    let events = Arc::new(CollectorEventSink::new());

    let started = start_renames(events.clone(), volume_id, rows, Initiator::Agent)
        .await
        .expect("the batch starts");

    assert_eq!(
        started.operation.operation_type,
        WriteOperationType::Move,
        "one move, not a rename batch"
    );
    assert_eq!(started.swaps_left_out, 0);
    crate::test_support::wait_until_async(std::time::Duration::from_secs(10), "the move to settle", || {
        !events.settled.lock_ignore_poison().is_empty()
    })
    .await;
    assert!(
        events.errors.lock_ignore_poison().is_empty(),
        "{:?}",
        events.errors.lock_ignore_poison()
    );
    assert!(volume.exists(Path::new("/a/bar/f001.txt")).await);
    assert!(volume.exists(Path::new("/a/memo.txt")).await);
    assert!(!volume.exists(Path::new("/a/foo")).await);
    assert!(!volume.exists(Path::new("/a/note.txt")).await);
}

/// A swap in a batch that runs as a move stays where it is, and the start
/// result says how many renames that left out, for the batch's result line.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_batch_as_a_move_reports_the_swaps_it_left_out() {
    use crate::file_system::write_operations::event_sinks::CollectorEventSink;

    let (volume_id, volume) = copying_store("copy-batch-swap", 1).await;
    volume.create_directory(Path::new("/a/bar")).await.unwrap();
    volume.create_file(Path::new("/a/note.txt"), b"n").await.unwrap();
    let mut rows = Vec::new();
    for (id, from, to) in [
        ("1", "/a/foo", "/a/bar"),
        ("2", "/a/bar", "/a/foo"),
        ("3", "/a/note.txt", "/a/memo.txt"),
    ] {
        rows.push(BulkRenameRow {
            row_id: id.to_string(),
            source: PathBuf::from(from),
            destination: PathBuf::from(to),
            expected_fingerprint: crate::file_system::write_operations::SourceFingerprint::capture_remote(
                volume.as_ref(),
                Path::new(from),
            )
            .await
            .expect("a fingerprint"),
        });
    }

    let started = start_renames(Arc::new(CollectorEventSink::new()), volume_id, rows, Initiator::Agent)
        .await
        .expect("the batch starts");

    assert_eq!(started.swaps_left_out, 2, "both renames of the swap");
}

#[tokio::test]
async fn a_batch_the_executor_runs_leaves_no_swap_out() {
    use crate::file_system::write_operations::event_sinks::CollectorEventSink;

    let tmp = TestDir::new("bulk-swap-local");
    fs::write(tmp.join("x.txt"), "x").unwrap();
    fs::write(tmp.join("y.txt"), "y").unwrap();
    let mut rows = Vec::new();
    for (id, from, to) in [("1", "x.txt", "y.txt"), ("2", "y.txt", "x.txt")] {
        let source = tmp.join(from);
        rows.push(BulkRenameRow {
            row_id: id.to_string(),
            expected_fingerprint: crate::file_system::write_operations::SourceFingerprint::capture_local(&source)
                .expect("a fingerprint"),
            source,
            destination: tmp.join(to),
        });
    }

    let started = start_renames(
        Arc::new(CollectorEventSink::new()),
        "root".to_string(),
        rows,
        Initiator::Agent,
    )
    .await
    .expect("the batch starts");

    assert_eq!(started.swaps_left_out, 0, "the executor swaps through a temporary name");
}
