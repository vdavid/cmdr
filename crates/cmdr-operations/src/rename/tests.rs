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

use crate::Operations;
use crate::testing::{TestOperationLog, TestOperations};
use cmdr_fs::entry::FileEntry;
use cmdr_fs::testing::TestDir;
use cmdr_fs::volume::{LaneKey, ListingProgress};
use cmdr_fs::volume::{Volume, VolumeError};

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
    let root = rename_descriptor(from, to, "root", true);
    assert!(root.volume_ids.is_empty(), "root marks nothing busy");
    assert!(root.lanes.is_empty(), "instant ops never reserve a lane");
    assert_eq!(root.operation_type, WriteOperationType::Rename);
    assert_eq!(root.summary.source.as_deref(), Some("old.txt"));
    assert_eq!(root.summary.destination.as_deref(), Some("new.txt"));

    // A real volume → marked busy for the op's duration.
    let device = rename_descriptor(from, to, "usb-42", false);
    assert_eq!(device.volume_ids, vec!["usb-42".to_string()]);
    assert!(device.lanes.is_empty());
}

// ============================================================================
// Managed-wrapper transparency (local root: same returns as the old command)
// ============================================================================

/// An instant rename journals through the real store: its own `rename` row,
/// finalized `done` and rollbackable, with one item row naming both ends. The
/// op id isn't returned, so the row is found by the test's unique file name.
#[tokio::test]
async fn an_instant_rename_journals_its_row_through_the_real_store() {
    use cmdr_operation_log::store::{open_read_connection, read_operation_items, recent_operations};
    use cmdr_operation_log::types::{ExecutionStatus, OpKind, RollbackState};

    let log = TestOperationLog::new("rename_journal_db");
    let engine = TestOperations::journaling_into(&log);
    let tmp = create_test_dir("journaled");
    let old_name = format!("{}.txt", unique("journaled-old"));
    fs::write(tmp.join(&old_name), "content").expect("source");

    engine
        .rename_managed(
            tmp.join(&old_name),
            tmp.join("new.txt"),
            false,
            "root".to_string(),
            Initiator::User,
        )
        .await
        .expect("rename");

    let conn = open_read_connection(log.db()).expect("read conn");
    let (op, items) = recent_operations(&conn, 50)
        .expect("ops")
        .into_iter()
        .filter(|op| op.kind == OpKind::Rename)
        .map(|op| {
            let items = read_operation_items(&conn, &op.op_id, 10).expect("items");
            (op, items)
        })
        .find(|(_, items)| items.iter().any(|item| item.source_name == old_name))
        .expect("the rename's own row");
    assert_eq!(op.execution_status, ExecutionStatus::Done);
    assert_eq!(op.rollback_state, RollbackState::Rollbackable);
    assert_eq!(items.len(), 1, "one row: the whole entry moves by one rename");
    assert_eq!(items[0].dest_name.as_deref(), Some("new.txt"));
}

#[tokio::test]
async fn rename_managed_local_success() {
    let engine = TestOperations::new();
    let tmp = create_test_dir("managed_ok");
    let old = tmp.join("old.txt");
    let new = tmp.join("new.txt");
    fs::write(&old, "content").unwrap();
    let result = engine
        .rename_managed(old.clone(), new.clone(), false, "root".to_string(), Initiator::User)
        .await;
    assert!(result.is_ok());
    assert!(!old.exists());
    assert_eq!(fs::read_to_string(&new).unwrap(), "content");
    let _ = fs::remove_dir_all(&tmp);
}

#[tokio::test]
async fn rename_managed_renames_a_zip_file_itself() {
    // The `.zip` file is a regular file: renaming it must work like any other file.
    // Only a rename INSIDE the archive is refused (covered by the reject test).
    let engine = TestOperations::new();
    let tmp = create_test_dir("managed_zip_rename");
    let old = tmp.join("bundle.zip");
    let new = tmp.join("renamed.zip");
    fs::write(&old, b"PK\x03\x04rest").unwrap();
    let result = engine
        .rename_managed(old.clone(), new.clone(), false, "root".to_string(), Initiator::User)
        .await;
    assert!(result.is_ok(), "renaming the .zip file itself must succeed: {result:?}");
    assert!(!old.exists());
    assert!(new.exists());
    let _ = fs::remove_dir_all(&tmp);
}

#[tokio::test]
async fn rename_managed_local_conflict_without_force_is_transparent() {
    let engine = TestOperations::new();
    let tmp = create_test_dir("managed_conflict");
    let old = tmp.join("old.txt");
    let new = tmp.join("new.txt");
    fs::write(&old, "old").unwrap();
    fs::write(&new, "new").unwrap();
    let result = engine
        .rename_managed(old.clone(), new.clone(), false, "root".to_string(), Initiator::User)
        .await;
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), MutationError::AlreadyExists { .. }));
    assert!(old.exists() && new.exists(), "both intact on conflict");
    let _ = fs::remove_dir_all(&tmp);
}

#[tokio::test]
async fn rename_managed_local_force_overwrites() {
    let engine = TestOperations::new();
    let tmp = create_test_dir("managed_force");
    let old = tmp.join("old.txt");
    let new = tmp.join("new.txt");
    fs::write(&old, "new content").unwrap();
    fs::write(&new, "old content").unwrap();
    let result = engine
        .rename_managed(old.clone(), new.clone(), true, "root".to_string(), Initiator::User)
        .await;
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
    let engine = TestOperations::new();
    engine.volumes().register(&volume_id, volume);

    let vid = volume_id.clone();
    let ops = Operations::clone(&engine);
    let handle = tokio::spawn(async move {
        ops.rename_managed(
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
        engine.busy_volume_ids().contains(&volume_id),
        "a non-root volume must be busy while its rename runs"
    );

    // Release → the rename (and the managed op) completes.
    release.notify_one();
    let result = handle.await.expect("task joins");
    assert!(result.is_ok(), "the managed rename returns success");
    assert!(
        !engine.busy_volume_ids().contains(&volume_id),
        "the volume must be freed once the rename finishes"
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
async fn route_archive_rename_onto_an_existing_name_errors_without_building_a_temp() {
    // Renaming an in-archive entry onto a name that already exists must be
    // rejected up front with the standard "already exists" message, so the FE
    // shows the friendly copy instead of the raw `zip` "Duplicate filename" — and
    // no temp is built.
    let engine = TestOperations::new();
    let tmp = create_test_dir("archive_rename_dup");
    let zip = tmp.join("bundle.zip");
    write_real_zip(&zip, &[("old.txt", b"o"), ("taken.txt", b"t")]);

    let from = zip.join("old.txt");
    let to = zip.join("taken.txt");
    let err = route_archive_rename(&engine, &from, &to, "root")
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
async fn copying_store(
    engine: &TestOperations,
    label: &str,
    files: usize,
) -> (String, Arc<cmdr_fs::volume::InMemoryVolume>) {
    let volume_id = unique(label);
    let volume = Arc::new(
        cmdr_fs::volume::InMemoryVolume::new(&volume_id)
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
    engine
        .volumes()
        .register(&volume_id, Arc::clone(&volume) as Arc<dyn Volume>);
    (volume_id, volume)
}

/// ❗ A rename that copies never reaches `Volume::rename` (the double refuses
/// it, which would come back as a `Volume` refusal): it starts a move, which a
/// unit test, with no app to emit through, can't, and says so.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_rename_that_copies_starts_a_move_instead_of_calling_rename() {
    let engine = TestOperations::new();
    let (volume_id, volume) = copying_store(&engine, "copy-rename", 2).await;

    let result = engine
        .rename_managed(
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
    let engine = TestOperations::new();
    let (small_id, _small) = copying_store(&engine, "copy-validity-small", 3).await;
    let small = engine
        .check_rename_validity_impl("/a".into(), "foo".into(), "bar".into(), small_id)
        .await
        .priced(|_| false);
    assert_eq!(
        small.by_move,
        Some(RenameByMove {
            files: 3,
            bytes: 6,
            counted_all: true,
            confirm_first: false,
        })
    );

    let (big_id, _big) = copying_store(&engine, "copy-validity-big", 101).await;
    let big = engine
        .check_rename_validity_impl("/a".into(), "foo".into(), "bar".into(), big_id)
        .await
        .priced(|_| false);
    let by_move = big.by_move.expect("a copying rename reports its move");
    assert!(by_move.confirm_first, "past 100 files it asks first: {by_move:?}");
    assert!(!by_move.counted_all, "and the count stopped at its cap: {by_move:?}");

    let plain_id = unique("plain-validity");
    let plain = Arc::new(cmdr_fs::volume::InMemoryVolume::new(&plain_id));
    plain.create_directory(Path::new("/a/foo")).await.unwrap();
    engine.volumes().register(&plain_id, plain as Arc<dyn Volume>);
    let one_call = engine
        .check_rename_validity_impl("/a".into(), "foo".into(), "bar".into(), plain_id)
        .await
        .priced(|_| false);
    assert_eq!(one_call.by_move, None, "a one-call rename carries nothing");
}

/// The check hands the host what a fully counted copying rename carries, and
/// a move the host prices as costing money asks first. An unfinished count is
/// never priced: it asks first anyway.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_host_prices_what_a_counted_copying_rename_carries() {
    let engine = TestOperations::new();
    let (small_id, _small) = copying_store(&engine, "copy-validity-priced", 3).await;
    let mut priced = None;
    let small = engine
        .check_rename_validity_impl("/a".into(), "foo".into(), "bar".into(), small_id)
        .await
        .priced(|facts| {
            priced = Some((facts.files, facts.bytes, facts.selected_folders));
            true
        });
    assert_eq!(priced, Some((3, 6, 1)), "the price sees the counted folder");
    assert!(small.by_move.expect("a copying rename reports its move").confirm_first);

    let (big_id, _big) = copying_store(&engine, "copy-validity-unpriced", 101).await;
    let big = engine
        .check_rename_validity_impl("/a".into(), "foo".into(), "bar".into(), big_id)
        .await
        .priced(|_| panic!("an unfinished count isn't priced"));
    assert!(big.by_move.expect("a copying rename reports its move").confirm_first);
}

/// A few files that cost something to move (young objects on Wasabi) ask
/// first too: only a free, small, fully counted rename starts unasked.
#[test]
fn a_small_rename_that_costs_money_asks_first() {
    let small = cmdr_fs::volume::SubtreeTally {
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
    use crate::event_sinks::CollectorEventSink;
    use cmdr_fs::ignore_poison::IgnorePoison;

    let engine = TestOperations::new();
    let (volume_id, volume) = copying_store(&engine, "copy-batch", 2).await;
    volume.create_file(Path::new("/a/note.txt"), b"n").await.unwrap();
    let mut rows = Vec::new();
    for (id, from, to) in [("1", "/a/foo", "/a/bar"), ("2", "/a/note.txt", "/a/memo.txt")] {
        rows.push(BulkRenameRow {
            row_id: id.to_string(),
            source: PathBuf::from(from),
            destination: PathBuf::from(to),
            expected_fingerprint: crate::SourceFingerprint::capture_remote(volume.as_ref(), Path::new(from))
                .await
                .expect("a fingerprint"),
        });
    }
    let events = Arc::new(CollectorEventSink::new());

    let started = engine
        .start_renames(events.clone(), volume_id, rows, Initiator::Agent)
        .await
        .expect("the batch starts");

    assert_eq!(
        started.operation.operation_type,
        WriteOperationType::Move,
        "one move, not a rename batch"
    );
    assert_eq!(started.swaps_left_out, 0);
    cmdr_fs::testing::wait_until_async(std::time::Duration::from_secs(10), "the move to settle", || {
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
    use crate::event_sinks::CollectorEventSink;

    let engine = TestOperations::new();
    let (volume_id, volume) = copying_store(&engine, "copy-batch-swap", 1).await;
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
            expected_fingerprint: crate::SourceFingerprint::capture_remote(volume.as_ref(), Path::new(from))
                .await
                .expect("a fingerprint"),
        });
    }

    let started = engine
        .start_renames(Arc::new(CollectorEventSink::new()), volume_id, rows, Initiator::Agent)
        .await
        .expect("the batch starts");

    assert_eq!(started.swaps_left_out, 2, "both renames of the swap");
}

#[tokio::test]
async fn a_batch_the_executor_runs_leaves_no_swap_out() {
    use crate::event_sinks::CollectorEventSink;

    let engine = TestOperations::new();
    let tmp = TestDir::new("bulk-swap-local");
    fs::write(tmp.join("x.txt"), "x").unwrap();
    fs::write(tmp.join("y.txt"), "y").unwrap();
    let mut rows = Vec::new();
    for (id, from, to) in [("1", "x.txt", "y.txt"), ("2", "y.txt", "x.txt")] {
        let source = tmp.join(from);
        rows.push(BulkRenameRow {
            row_id: id.to_string(),
            expected_fingerprint: crate::SourceFingerprint::capture_local(&source).expect("a fingerprint"),
            source,
            destination: tmp.join(to),
        });
    }

    let started = engine
        .start_renames(
            Arc::new(CollectorEventSink::new()),
            "root".to_string(),
            rows,
            Initiator::Agent,
        )
        .await
        .expect("the batch starts");

    assert_eq!(started.swaps_left_out, 0, "the executor swaps through a temporary name");
}

// ============================================================================
// Case-only renames off the local disk
// ============================================================================

/// An SMB share's lookups, over an `InMemoryVolume`: a name finds its entry in
/// any case, and the entry comes back named the way it was asked for, as
/// `SmbVolume::get_metadata` names it. The listing keeps the stored spelling.
struct CaseFoldingVolume {
    inner: cmdr_fs::volume::InMemoryVolume,
}

impl Volume for CaseFoldingVolume {
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
        on_progress: Option<&'a (dyn Fn(ListingProgress) + Sync)>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
        self.inner.list_directory(path, on_progress)
    }
    fn get_metadata<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<FileEntry, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let (Some(dir), Some(asked)) = (path.parent(), path.file_name().and_then(|n| n.to_str())) else {
                return self.inner.get_metadata(path).await;
            };
            let entries = self.inner.list_directory(dir, None).await?;
            let found = entries
                .into_iter()
                .find(|entry| entry.name.eq_ignore_ascii_case(asked))
                .ok_or_else(|| VolumeError::NotFound(path.display().to_string()))?;
            Ok(FileEntry {
                name: asked.to_string(),
                ..found
            })
        })
    }
    fn lane_key(&self) -> LaneKey {
        LaneKey::new(self.inner.name().to_string())
    }
}

/// QA 2026-10-14: F2 `c.txt` → `C.txt` at a QNAP share's root opened "A file
/// with this name already exists", comparing the file with itself. The stat of
/// `C.txt` finds `c.txt` on a case-insensitive share, so the check has to tell
/// that the entry it found IS the one being renamed, as the local disk does by
/// inode.
#[tokio::test]
async fn a_case_only_rename_on_a_case_insensitive_volume_is_not_a_conflict() {
    let volume_id = unique("case-folding");
    let inner = cmdr_fs::volume::InMemoryVolume::new(&volume_id);
    inner.create_directory(Path::new("/share")).await.unwrap();
    inner.create_file(Path::new("/share/c.txt"), b"abcd").await.unwrap();
    let engine = TestOperations::new();
    engine
        .volumes()
        .register(&volume_id, Arc::new(CaseFoldingVolume { inner }) as Arc<dyn Volume>);

    let check = engine
        .check_rename_validity_impl("/share".into(), "c.txt".into(), "C.txt".into(), volume_id)
        .await
        .priced(|_| false);

    assert!(check.valid, "{check:?}");
    assert!(
        !check.has_conflict || check.is_case_only_rename,
        "the only `C.txt` there is `c.txt` itself, so the rename goes ahead: {check:?}"
    );
}

/// The other half: a case-sensitive volume holding both `c.txt` and `C.txt`
/// has a real conflict, and the dialog has to show it.
#[tokio::test]
async fn a_case_only_rename_onto_a_real_sibling_is_still_a_conflict() {
    let volume_id = unique("case-sensitive");
    let volume = Arc::new(cmdr_fs::volume::InMemoryVolume::new(&volume_id));
    volume.create_directory(Path::new("/share")).await.unwrap();
    volume.create_file(Path::new("/share/c.txt"), b"abcd").await.unwrap();
    volume.create_file(Path::new("/share/C.txt"), b"other!").await.unwrap();
    let engine = TestOperations::new();
    engine.volumes().register(&volume_id, volume as Arc<dyn Volume>);

    let check = engine
        .check_rename_validity_impl("/share".into(), "c.txt".into(), "C.txt".into(), volume_id)
        .await
        .priced(|_| false);

    assert!(check.has_conflict, "{check:?}");
    assert!(!check.is_case_only_rename, "`C.txt` is a different file: {check:?}");
    assert_eq!(check.conflict.map(|c| c.size), Some(6));
}
