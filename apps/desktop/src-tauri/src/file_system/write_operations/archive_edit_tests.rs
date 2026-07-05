//! Headless tests for the archive-edit driver: it runs the mutator as a managed
//! op and emits the right terminal events, with no Tauri runtime (a
//! `CollectorEventSink` captures events).

use std::future::Future;
use std::io::{Read, Write};
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use zip::write::SimpleFileOptions;
use zip::{ZipArchive, ZipWriter};

use super::super::types::CollectorEventSink;
use super::*;
use crate::file_system::volume::VolumeError;
use crate::file_system::volume::backends::archive::mutator::{AddEntry, AddSource};
use crate::ignore_poison::IgnorePoison;

/// Builds a one-entry zip at `path`.
fn write_simple_zip(path: &Path, entry: &str, content: &[u8]) {
    let file = std::fs::File::create(path).expect("create zip");
    let mut writer = ZipWriter::new(file);
    writer
        .start_file(entry, SimpleFileOptions::default())
        .expect("start entry");
    writer.write_all(content).expect("write entry");
    writer.finish().expect("finish zip");
}

/// Reads one entry's decompressed bytes back, or `None` if absent.
fn read_entry(path: &Path, name: &str) -> Option<Vec<u8>> {
    let file = std::fs::File::open(path).ok()?;
    let mut archive = ZipArchive::new(file).ok()?;
    let mut entry = archive.by_name(name).ok()?;
    let mut buf = Vec::new();
    entry.read_to_end(&mut buf).ok()?;
    Some(buf)
}

/// Polls until `predicate` holds or a bounded timeout elapses, yielding to the
/// runtime so the spawned op makes progress.
async fn wait_until(mut predicate: impl FnMut() -> bool) -> bool {
    for _ in 0..3000 {
        if predicate() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
    false
}

#[tokio::test]
async fn a_successful_edit_rewrites_the_archive_and_emits_complete_then_settled() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let path = tmp.path().join("a.zip");
    write_simple_zip(&path, "keep.txt", b"keep");

    let events = Arc::new(CollectorEventSink::new());
    let request = ArchiveEditRequest {
        archive_path: path.clone(),
        parent_volume_id: "root".to_string(),
        changeset: Changeset {
            adds: vec![AddEntry {
                inner_path: "added.txt".to_string(),
                source: AddSource::Bytes(b"new bytes".to_vec()),
            }],
            ..Default::default()
        },
        summary: OperationSummaryText::default(),
        move_sources_to_delete: vec![],
        skipped_count: 0,
    };

    let start = archive_edit_start(Arc::clone(&events) as Arc<dyn OperationEventSink>, request, 0)
        .await
        .expect("start archive edit");
    assert_eq!(start.operation_type, WriteOperationType::ArchiveEdit);

    assert!(
        wait_until(|| !events.complete.lock_ignore_poison().is_empty()).await,
        "a write-complete should fire"
    );

    // The archive was actually rewritten.
    assert_eq!(read_entry(&path, "keep.txt").as_deref(), Some(b"keep".as_slice()));
    assert_eq!(read_entry(&path, "added.txt").as_deref(), Some(b"new bytes".as_slice()));

    {
        let complete = events.complete.lock_ignore_poison();
        assert_eq!(complete.len(), 1);
        assert_eq!(complete[0].operation_type, WriteOperationType::ArchiveEdit);
    }
    // No error, and settle fired for the same op.
    assert!(
        events.errors.lock_ignore_poison().is_empty(),
        "no write-error on success"
    );
    assert!(
        wait_until(|| !events.settled.lock_ignore_poison().is_empty()).await,
        "write-settled should fire"
    );
    // A `root`-parent edit settles with no volume id (`None`, not `"root"`).
    assert_eq!(events.settled.lock_ignore_poison()[0].volume_id, None);
}

#[tokio::test]
async fn route_archive_delete_removes_entries_and_completes() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let path = tmp.path().join("a.zip");
    {
        let file = std::fs::File::create(&path).expect("create zip");
        let mut writer = ZipWriter::new(file);
        for name in ["keep.txt", "drop.txt"] {
            writer.start_file(name, SimpleFileOptions::default()).expect("start");
            writer.write_all(name.as_bytes()).expect("write");
        }
        writer.finish().expect("finish");
    }

    let events = Arc::new(CollectorEventSink::new());
    // The FE sends full paths inside the archive.
    let sources = vec![path.join("drop.txt")];
    route_archive_delete(Arc::clone(&events) as Arc<dyn OperationEventSink>, &sources, "root", 0)
        .await
        .expect("start delete");

    assert!(
        wait_until(|| !events.complete.lock_ignore_poison().is_empty()).await,
        "the delete should complete"
    );
    assert!(read_entry(&path, "drop.txt").is_none(), "the entry was removed");
    assert_eq!(read_entry(&path, "keep.txt").as_deref(), Some(b"keep.txt".as_slice()));
}

#[tokio::test]
async fn route_archive_delete_reports_the_deleted_count_not_the_retained_count() {
    // Deleting ONE entry from a 3-entry zip must report `files_processed == 1`
    // (the number DELETED), not the retained-entry count (2). Pins the archive
    // edit's `files_processed` semantics against the "Delete complete: 2 files" bug.
    let tmp = tempfile::tempdir().expect("tempdir");
    let path = tmp.path().join("a.zip");
    write_multi_zip(
        &path,
        &[("drop.txt", b"drop"), ("keep1.txt", b"one"), ("keep2.txt", b"two")],
    );

    let events = Arc::new(CollectorEventSink::new());
    route_archive_delete(
        Arc::clone(&events) as Arc<dyn OperationEventSink>,
        &[path.join("drop.txt")],
        "root",
        0,
    )
    .await
    .expect("start delete");

    assert!(wait_until(|| !events.complete.lock_ignore_poison().is_empty()).await);
    assert!(read_entry(&path, "drop.txt").is_none(), "the entry was removed");
    let complete = events.complete.lock_ignore_poison();
    assert_eq!(
        complete[0].files_processed, 1,
        "a one-file delete must report 1 processed file, not the retained-entry count"
    );
}

#[tokio::test]
async fn copy_into_adds_a_local_directory_tree_and_skips_conflicts() {
    use crate::file_system::volume::backends::LocalPosixVolume;
    use crate::file_system::write_operations::route_archive_copy_into;

    let tmp = tempfile::tempdir().expect("tempdir");
    // The archive already holds `payload/existing.txt`, so a Skip-policy copy of a
    // colliding file leaves it untouched while adding the new ones.
    let archive = tmp.path().join("a.zip");
    {
        let file = std::fs::File::create(&archive).expect("create zip");
        let mut writer = ZipWriter::new(file);
        writer
            .start_file("payload/existing.txt", SimpleFileOptions::default())
            .expect("start");
        writer.write_all(b"OLD").expect("write");
        writer.finish().expect("finish");
    }

    // A local source tree: payload/{existing.txt, fresh.txt, sub/deep.txt}.
    let src_root = tmp.path().join("src");
    std::fs::create_dir_all(src_root.join("payload/sub")).expect("mkdir src");
    std::fs::write(src_root.join("payload/existing.txt"), b"NEW").expect("w1");
    std::fs::write(src_root.join("payload/fresh.txt"), b"fresh").expect("w2");
    std::fs::write(src_root.join("payload/sub/deep.txt"), b"deep").expect("w3");

    // A local-FS source volume rooted at src_root (drives `local_path()`).
    let source_volume: Arc<dyn Volume> = Arc::new(LocalPosixVolume::new("src", src_root.clone()));

    let events = Arc::new(CollectorEventSink::new());
    // Destination is the archive ROOT, so the source dir `payload` lands as `payload/`.
    let dest = archive.clone();
    route_archive_copy_into(
        Arc::clone(&events) as Arc<dyn OperationEventSink>,
        source_volume,
        vec![PathBuf::from("payload")],
        dest,
        "root".to_string(),
        ConflictResolution::Skip,
        0,
        false,
    )
    .await
    .expect("start copy-into");

    assert!(
        wait_until(|| !events.complete.lock_ignore_poison().is_empty()).await,
        "copy-into should complete"
    );
    // The colliding file kept its OLD bytes (Skip), the new files were added.
    assert_eq!(
        read_entry(&archive, "payload/existing.txt").as_deref(),
        Some(b"OLD".as_slice())
    );
    assert_eq!(
        read_entry(&archive, "payload/fresh.txt").as_deref(),
        Some(b"fresh".as_slice())
    );
    assert_eq!(
        read_entry(&archive, "payload/sub/deep.txt").as_deref(),
        Some(b"deep".as_slice())
    );
}

// ---- Out-of-zip MOVE (extract + batch archive delete) ---------------------

/// Builds a multi-entry zip at `path`.
fn write_multi_zip(path: &Path, entries: &[(&str, &[u8])]) {
    let file = std::fs::File::create(path).expect("create zip");
    let mut writer = ZipWriter::new(file);
    for (name, content) in entries {
        writer.start_file(*name, SimpleFileOptions::default()).expect("start");
        writer.write_all(content).expect("write");
    }
    writer.finish().expect("finish zip");
}

/// A destination volume whose streaming write ALWAYS fails: it delegates reads,
/// metadata, and space to an inner `InMemoryVolume` but never implements
/// `write_from_stream`, so the trait default (`NotSupported`) turns every
/// extracted-file write into a destination-side failure. Used to prove that a
/// dest-side failure during move-out deletes NOTHING from the archive.
struct FailingWriteVolume {
    inner: Arc<crate::file_system::volume::InMemoryVolume>,
}

impl Volume for FailingWriteVolume {
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
    ) -> Pin<Box<dyn Future<Output = Result<Vec<crate::file_system::listing::FileEntry>, VolumeError>> + Send + 'a>>
    {
        self.inner.list_directory(path, on_progress)
    }
    fn get_metadata<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<crate::file_system::listing::FileEntry, VolumeError>> + Send + 'a>> {
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
    fn get_space_info<'a>(
        &'a self,
    ) -> Pin<Box<dyn Future<Output = Result<crate::file_system::volume::SpaceInfo, VolumeError>> + Send + 'a>> {
        self.inner.get_space_info()
    }
    fn supports_streaming(&self) -> bool {
        true
    }
    fn supports_export(&self) -> bool {
        true
    }
    // `write_from_stream` deliberately NOT implemented — the trait default
    // returns `NotSupported`, which is the dest-side failure this double injects.
}

/// Builds a local-backed `ArchiveVolume` over `archive_path` for move-out tests.
fn archive_source_volume(archive_path: &Path) -> Arc<dyn Volume> {
    use crate::file_system::volume::InMemoryVolume;
    use crate::file_system::volume::backends::archive::ArchiveVolume;
    let parent: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("parent").with_local_fs_access());
    Arc::new(ArchiveVolume::new(parent, archive_path.to_path_buf()))
}

fn move_out_config() -> crate::file_system::VolumeCopyConfig {
    crate::file_system::VolumeCopyConfig {
        progress_interval_ms: 0,
        ..Default::default()
    }
}

#[tokio::test]
async fn move_out_lands_files_at_dest_deletes_entries_and_keeps_the_remainder() {
    use crate::file_system::volume::backends::LocalPosixVolume;
    use crate::file_system::write_operations::route_archive_move_out;

    let tmp = tempfile::tempdir().expect("tempdir");
    let archive = tmp.path().join("a.zip");
    write_multi_zip(&archive, &[("keep.txt", b"keep"), ("move_me.txt", b"payload")]);

    let dest_dir = tmp.path().join("out");
    std::fs::create_dir_all(&dest_dir).expect("mkdir dest");

    let source_volume = archive_source_volume(&archive);
    let dest_volume: Arc<dyn Volume> = Arc::new(LocalPosixVolume::new("dest", dest_dir.clone()));

    let events = Arc::new(CollectorEventSink::new());
    route_archive_move_out(
        Arc::clone(&events) as Arc<dyn OperationEventSink>,
        "root".to_string(),
        source_volume,
        vec![archive.join("move_me.txt")],
        "dest".to_string(),
        dest_volume,
        dest_dir.clone(),
        move_out_config(),
    )
    .await
    .expect("start move-out");

    assert!(
        wait_until(|| !events.complete.lock_ignore_poison().is_empty()).await,
        "move-out should complete"
    );
    // The file landed at the destination.
    assert_eq!(
        std::fs::read(dest_dir.join("move_me.txt")).ok().as_deref(),
        Some(b"payload".as_slice()),
        "the moved file must land at the destination"
    );
    // Its archive entry is gone, and the untouched sibling survives.
    assert!(
        read_entry(&archive, "move_me.txt").is_none(),
        "the moved entry must be deleted from the archive"
    );
    assert_eq!(
        read_entry(&archive, "keep.txt").as_deref(),
        Some(b"keep".as_slice()),
        "the untouched sibling must remain in the archive"
    );
    {
        let complete = events.complete.lock_ignore_poison();
        assert_eq!(complete[0].operation_type, WriteOperationType::Move);
    }
    // A `root`-parent op carries no settle volume id (it's `None`, not `"root"`).
    assert!(wait_until(|| !events.settled.lock_ignore_poison().is_empty()).await);
    assert_eq!(
        events.settled.lock_ignore_poison()[0].volume_id,
        None,
        "a root-parent move-out settles with no volume id"
    );
}

#[tokio::test]
async fn move_out_dest_failure_deletes_nothing_and_leaves_the_archive_readable() {
    use crate::file_system::volume::InMemoryVolume;
    use crate::file_system::write_operations::route_archive_move_out;

    let tmp = tempfile::tempdir().expect("tempdir");
    let archive = tmp.path().join("a.zip");
    write_multi_zip(&archive, &[("keep.txt", b"keep"), ("move_me.txt", b"payload")]);

    let source_volume = archive_source_volume(&archive);
    let dest_volume: Arc<dyn Volume> = Arc::new(FailingWriteVolume {
        inner: Arc::new(InMemoryVolume::new("dest")),
    });

    let events = Arc::new(CollectorEventSink::new());
    route_archive_move_out(
        Arc::clone(&events) as Arc<dyn OperationEventSink>,
        "root".to_string(),
        source_volume,
        vec![archive.join("move_me.txt")],
        "dest".to_string(),
        dest_volume,
        PathBuf::from("/out"),
        move_out_config(),
    )
    .await
    .expect("start move-out");

    assert!(
        wait_until(|| !events.errors.lock_ignore_poison().is_empty()).await,
        "a dest-side failure must surface a write-error"
    );
    // CRITICAL: nothing was deleted — the archive is byte-for-byte intact.
    assert_eq!(
        read_entry(&archive, "move_me.txt").as_deref(),
        Some(b"payload".as_slice()),
        "a dest failure must NOT delete the archive entry (no data loss)"
    );
    assert_eq!(read_entry(&archive, "keep.txt").as_deref(), Some(b"keep".as_slice()));
    assert!(
        events.complete.lock_ignore_poison().is_empty(),
        "no write-complete on a failed move-out"
    );
}

#[tokio::test]
async fn move_out_with_a_skipped_collision_deletes_nothing() {
    use crate::file_system::volume::backends::LocalPosixVolume;
    use crate::file_system::write_operations::route_archive_move_out;

    let tmp = tempfile::tempdir().expect("tempdir");
    let archive = tmp.path().join("a.zip");
    write_multi_zip(&archive, &[("keep.txt", b"keep"), ("move_me.txt", b"payload")]);

    // The destination already holds `move_me.txt`, so a Skip-policy extract skips
    // it — nothing lands, so all-or-nothing must delete NOTHING from the archive.
    let dest_dir = tmp.path().join("out");
    std::fs::create_dir_all(&dest_dir).expect("mkdir dest");
    std::fs::write(dest_dir.join("move_me.txt"), b"EXISTING").expect("pre-existing dest");

    let source_volume = archive_source_volume(&archive);
    let dest_volume: Arc<dyn Volume> = Arc::new(LocalPosixVolume::new("dest", dest_dir.clone()));

    let events = Arc::new(CollectorEventSink::new());
    let config = crate::file_system::VolumeCopyConfig {
        progress_interval_ms: 0,
        conflict_resolution: ConflictResolution::Skip,
        ..Default::default()
    };
    route_archive_move_out(
        Arc::clone(&events) as Arc<dyn OperationEventSink>,
        "root".to_string(),
        source_volume,
        vec![archive.join("move_me.txt")],
        "dest".to_string(),
        dest_volume,
        dest_dir.clone(),
        config,
    )
    .await
    .expect("start move-out");

    assert!(
        wait_until(|| !events.complete.lock_ignore_poison().is_empty()).await,
        "the move-out should complete (nothing extracted, nothing deleted)"
    );
    // All-or-nothing on a skip: the archive entry survives, and the pre-existing
    // destination file is untouched.
    assert_eq!(
        read_entry(&archive, "move_me.txt").as_deref(),
        Some(b"payload".as_slice()),
        "a skipped collision must NOT delete the archive entry"
    );
    assert_eq!(read_entry(&archive, "keep.txt").as_deref(), Some(b"keep".as_slice()));
    assert_eq!(
        std::fs::read(dest_dir.join("move_me.txt")).ok().as_deref(),
        Some(b"EXISTING".as_slice()),
        "Skip keeps the pre-existing destination file"
    );
}

#[tokio::test]
async fn move_out_cancel_leaves_the_archive_untouched() {
    use crate::file_system::volume::backends::LocalPosixVolume;
    use crate::file_system::write_operations::{cancel_write_operation, route_archive_move_out};

    let tmp = tempfile::tempdir().expect("tempdir");
    let archive = tmp.path().join("a.zip");
    write_multi_zip(&archive, &[("keep.txt", b"keep"), ("move_me.txt", b"payload")]);

    let dest_dir = tmp.path().join("out");
    std::fs::create_dir_all(&dest_dir).expect("mkdir dest");

    let source_volume = archive_source_volume(&archive);
    let dest_volume: Arc<dyn Volume> = Arc::new(LocalPosixVolume::new("dest", dest_dir.clone()));

    let events = Arc::new(CollectorEventSink::new());
    let start = route_archive_move_out(
        Arc::clone(&events) as Arc<dyn OperationEventSink>,
        "root".to_string(),
        source_volume,
        vec![archive.join("move_me.txt")],
        "dest".to_string(),
        dest_volume,
        dest_dir.clone(),
        move_out_config(),
    )
    .await
    .expect("start move-out");

    // Cancel synchronously before yielding, so the extract observes Stopped at
    // its first cancellation check (the deferred can't run until this task
    // awaits). `false` = keep already-copied files, no rollback.
    cancel_write_operation(&start.operation_id, false);

    assert!(
        wait_until(|| {
            !events.cancelled.lock_ignore_poison().is_empty() || !events.complete.lock_ignore_poison().is_empty()
        })
        .await,
        "the cancelled move-out should reach a terminal event"
    );
    // The archive is untouched: nothing was deleted on cancel.
    assert_eq!(
        read_entry(&archive, "move_me.txt").as_deref(),
        Some(b"payload".as_slice()),
        "cancel must NOT delete the archive entry"
    );
    assert_eq!(read_entry(&archive, "keep.txt").as_deref(), Some(b"keep".as_slice()));
    assert!(
        events.cancelled.lock_ignore_poison().len() == 1 || events.complete.lock_ignore_poison().len() == 1,
        "exactly one terminal event"
    );
}

#[tokio::test]
async fn a_missing_archive_emits_a_write_error_not_a_panic() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let path = tmp.path().join("ghost.zip"); // never created

    let events = Arc::new(CollectorEventSink::new());
    let request = ArchiveEditRequest {
        archive_path: path.clone(),
        parent_volume_id: "root".to_string(),
        changeset: Changeset {
            mkdirs: vec!["dir".to_string()],
            ..Default::default()
        },
        summary: OperationSummaryText::default(),
        move_sources_to_delete: vec![],
        skipped_count: 0,
    };

    archive_edit_start(Arc::clone(&events) as Arc<dyn OperationEventSink>, request, 0)
        .await
        .expect("start archive edit");

    assert!(
        wait_until(|| !events.errors.lock_ignore_poison().is_empty()).await,
        "a missing archive should surface a write-error"
    );
    assert!(
        events.complete.lock_ignore_poison().is_empty(),
        "no write-complete on failure"
    );
    // Settle still fires (torn-down cleanly, no hang).
    assert!(
        wait_until(|| !events.settled.lock_ignore_poison().is_empty()).await,
        "write-settled fires even on the error path"
    );
}

// ---- Non-interactive policy coverage (Rename / conditional / move gating) --

/// Runs a non-interactive copy/move INTO `archive` of local dir `src_rel` under a
/// pre-resolved `policy`, landing at the archive root. Returns the collector.
async fn run_policy_copy_into(
    archive: &Path,
    src_root: &Path,
    src_rel: &str,
    policy: ConflictResolution,
    is_move: bool,
) -> Arc<CollectorEventSink> {
    use crate::file_system::volume::backends::LocalPosixVolume;
    use crate::file_system::write_operations::route_archive_copy_into;

    let source_volume: Arc<dyn Volume> = Arc::new(LocalPosixVolume::new("src", src_root.to_path_buf()));
    let events = Arc::new(CollectorEventSink::new());
    route_archive_copy_into(
        Arc::clone(&events) as Arc<dyn OperationEventSink>,
        source_volume,
        vec![PathBuf::from(src_rel)],
        archive.to_path_buf(),
        "root".to_string(),
        policy,
        0,
        is_move,
    )
    .await
    .expect("start policy copy-into");
    events
}

#[tokio::test]
async fn rename_policy_picks_the_next_free_numbered_name() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let archive = tmp.path().join("a.zip");
    // Both `existing.txt` AND `existing (1).txt` are taken, so Rename must land on
    // `existing (2).txt` — pinning the extension placement AND the skip-taken loop.
    write_multi_zip(
        &archive,
        &[
            ("d/existing.txt", b"OLD"),
            ("d/existing (1).txt", b"OLD1"),
            ("d/.env", b"OLD_ENV"),
        ],
    );

    let src_root = tmp.path().join("src");
    std::fs::create_dir_all(src_root.join("d")).expect("mkdir src");
    std::fs::write(src_root.join("d/existing.txt"), b"NEW").expect("w1");
    // A DOTFILE has no stem before its dot, so the whole name (incl. the leading
    // dot) is the stem and the ` (n)` suffix goes at the END: `.env (1)`, not
    // ` (1).env`. Pins the extension-placement guard.
    std::fs::write(src_root.join("d/.env"), b"ENV").expect("w2");

    let events = run_policy_copy_into(&archive, &src_root, "d", ConflictResolution::Rename, false).await;

    assert!(
        wait_until(|| !events.complete.lock_ignore_poison().is_empty()).await,
        "rename copy-into should complete"
    );
    // Originals untouched; the incoming file lands under the next free name with
    // the extension kept BEFORE the ` (n)` suffix.
    assert_eq!(
        read_entry(&archive, "d/existing.txt").as_deref(),
        Some(b"OLD".as_slice())
    );
    assert_eq!(
        read_entry(&archive, "d/existing (1).txt").as_deref(),
        Some(b"OLD1".as_slice())
    );
    assert_eq!(
        read_entry(&archive, "d/existing (2).txt").as_deref(),
        Some(b"NEW".as_slice()),
        "Rename must pick the next free ` (n)` name and keep the extension in place"
    );
    assert_eq!(
        read_entry(&archive, "d/.env").as_deref(),
        Some(b"OLD_ENV".as_slice()),
        "the original dotfile is kept"
    );
    assert_eq!(
        read_entry(&archive, "d/.env (1)").as_deref(),
        Some(b"ENV".as_slice()),
        "a renamed dotfile keeps its whole name as the stem: `.env (1)`, not ` (1).env`"
    );
}

#[tokio::test]
async fn overwrite_smaller_overwrites_only_a_strictly_smaller_entry() {
    // Case A: the archive entry is LARGER than the source → skip (kept).
    let tmp = tempfile::tempdir().expect("tempdir");
    let archive_a = tmp.path().join("a.zip");
    write_multi_zip(&archive_a, &[("d/f.txt", b"BIGDATA")]); // 7 bytes
    let src_a = tmp.path().join("srca");
    std::fs::create_dir_all(src_a.join("d")).expect("mkdir");
    std::fs::write(src_a.join("d/f.txt"), b"hi").expect("w"); // 2 bytes
    let events_a = run_policy_copy_into(&archive_a, &src_a, "d", ConflictResolution::OverwriteSmaller, false).await;
    assert!(wait_until(|| !events_a.complete.lock_ignore_poison().is_empty()).await);
    assert_eq!(
        read_entry(&archive_a, "d/f.txt").as_deref(),
        Some(b"BIGDATA".as_slice()),
        "a larger destination must NOT be overwritten under OverwriteSmaller"
    );

    // Case B: the archive entry is SMALLER than the source → overwrite.
    let archive_b = tmp.path().join("b.zip");
    write_multi_zip(&archive_b, &[("d/f.txt", b"hi")]); // 2 bytes
    let src_b = tmp.path().join("srcb");
    std::fs::create_dir_all(src_b.join("d")).expect("mkdir");
    std::fs::write(src_b.join("d/f.txt"), b"BIGDATA").expect("w"); // 7 bytes
    let events_b = run_policy_copy_into(&archive_b, &src_b, "d", ConflictResolution::OverwriteSmaller, false).await;
    assert!(wait_until(|| !events_b.complete.lock_ignore_poison().is_empty()).await);
    assert_eq!(
        read_entry(&archive_b, "d/f.txt").as_deref(),
        Some(b"BIGDATA".as_slice()),
        "a strictly smaller destination must be overwritten under OverwriteSmaller"
    );

    // Case C: EQUAL sizes → skip (the comparison is strict `<`, not `<=`).
    let archive_c = tmp.path().join("c.zip");
    write_multi_zip(&archive_c, &[("d/f.txt", b"AB")]); // 2 bytes
    let src_c = tmp.path().join("srcc");
    std::fs::create_dir_all(src_c.join("d")).expect("mkdir");
    std::fs::write(src_c.join("d/f.txt"), b"XY").expect("w"); // 2 bytes, equal
    let events_c = run_policy_copy_into(&archive_c, &src_c, "d", ConflictResolution::OverwriteSmaller, false).await;
    assert!(wait_until(|| !events_c.complete.lock_ignore_poison().is_empty()).await);
    assert_eq!(
        read_entry(&archive_c, "d/f.txt").as_deref(),
        Some(b"AB".as_slice()),
        "an equal-size destination must NOT be overwritten (strict `<`, never `<=`)"
    );
}

#[tokio::test]
async fn overwrite_older_overwrites_only_a_strictly_older_entry() {
    use zip::DateTime;
    use zip::write::SimpleFileOptions;

    // Build a zip whose single entry carries a controlled modification date.
    fn write_zip_with_mtime(path: &Path, name: &str, content: &[u8], dt: DateTime) {
        let file = std::fs::File::create(path).expect("create zip");
        let mut writer = ZipWriter::new(file);
        writer
            .start_file(name, SimpleFileOptions::default().last_modified_time(dt))
            .expect("start");
        writer.write_all(content).expect("write");
        writer.finish().expect("finish");
    }
    let old = DateTime::from_date_and_time(2020, 1, 1, 0, 0, 0).expect("2020 date");
    let new = DateTime::from_date_and_time(2024, 1, 1, 0, 0, 0).expect("2024 date");
    let src_2020 = filetime::FileTime::from_unix_time(1_577_836_800, 0); // 2020-01-01Z
    let src_2024 = filetime::FileTime::from_unix_time(1_704_067_200, 0); // 2024-01-01Z

    let tmp = tempfile::tempdir().expect("tempdir");

    // Case A: archive entry is NEWER (2024) than the source (2020) → skip.
    let archive_a = tmp.path().join("a.zip");
    write_zip_with_mtime(&archive_a, "d/f.txt", b"KEEP", new);
    let src_a = tmp.path().join("srca");
    std::fs::create_dir_all(src_a.join("d")).expect("mkdir");
    std::fs::write(src_a.join("d/f.txt"), b"INCOMING").expect("w");
    filetime::set_file_mtime(src_a.join("d/f.txt"), src_2020).expect("mtime");
    let events_a = run_policy_copy_into(&archive_a, &src_a, "d", ConflictResolution::OverwriteOlder, false).await;
    assert!(wait_until(|| !events_a.complete.lock_ignore_poison().is_empty()).await);
    assert_eq!(
        read_entry(&archive_a, "d/f.txt").as_deref(),
        Some(b"KEEP".as_slice()),
        "a newer destination must NOT be overwritten under OverwriteOlder"
    );

    // Case B: archive entry is OLDER (2020) than the source (2024) → overwrite.
    let archive_b = tmp.path().join("b.zip");
    write_zip_with_mtime(&archive_b, "d/f.txt", b"KEEP", old);
    let src_b = tmp.path().join("srcb");
    std::fs::create_dir_all(src_b.join("d")).expect("mkdir");
    std::fs::write(src_b.join("d/f.txt"), b"INCOMING").expect("w");
    filetime::set_file_mtime(src_b.join("d/f.txt"), src_2024).expect("mtime");
    let events_b = run_policy_copy_into(&archive_b, &src_b, "d", ConflictResolution::OverwriteOlder, false).await;
    assert!(wait_until(|| !events_b.complete.lock_ignore_poison().is_empty()).await);
    assert_eq!(
        read_entry(&archive_b, "d/f.txt").as_deref(),
        Some(b"INCOMING".as_slice()),
        "a strictly older destination must be overwritten under OverwriteOlder"
    );

    // Case C: EQUAL mtimes → skip (the comparison is strict `<`, not `<=`). Derive
    // the source mtime from the archive entry's ACTUAL parsed value so the two are
    // bit-for-bit equal regardless of DOS-datetime timezone conversion.
    use crate::file_system::volume::backends::archive::{ArchiveIndex, LocalFileSource};
    let archive_c = tmp.path().join("c.zip");
    write_zip_with_mtime(&archive_c, "d/f.txt", b"KEEP", old);
    let parsed_mtime = {
        let src = LocalFileSource::open(&archive_c).expect("open archive");
        let index = ArchiveIndex::parse(&src).expect("parse index");
        index.get("d/f.txt").and_then(|n| n.modified).expect("entry mtime")
    };
    let src_c = tmp.path().join("srcc");
    std::fs::create_dir_all(src_c.join("d")).expect("mkdir");
    std::fs::write(src_c.join("d/f.txt"), b"INCOMING").expect("w");
    filetime::set_file_mtime(
        src_c.join("d/f.txt"),
        filetime::FileTime::from_unix_time(parsed_mtime, 0),
    )
    .expect("mtime");
    let events_c = run_policy_copy_into(&archive_c, &src_c, "d", ConflictResolution::OverwriteOlder, false).await;
    assert!(wait_until(|| !events_c.complete.lock_ignore_poison().is_empty()).await);
    assert_eq!(
        read_entry(&archive_c, "d/f.txt").as_deref(),
        Some(b"KEEP".as_slice()),
        "an equal-mtime destination must NOT be overwritten (strict `<`, never `<=`)"
    );
}

#[tokio::test]
async fn move_into_deletes_the_source_only_on_a_clean_transfer() {
    // Clean move (no collision) → the top-level source is deleted after commit.
    let tmp = tempfile::tempdir().expect("tempdir");
    let archive = tmp.path().join("a.zip");
    write_multi_zip(&archive, &[("placeholder.txt", b"x")]);
    let src_root = tmp.path().join("src");
    std::fs::create_dir_all(src_root.join("d")).expect("mkdir");
    std::fs::write(src_root.join("d/a.txt"), b"aaa").expect("w");

    let events = run_policy_copy_into(&archive, &src_root, "d", ConflictResolution::Overwrite, true).await;
    assert!(wait_until(|| !events.complete.lock_ignore_poison().is_empty()).await);
    assert_eq!(read_entry(&archive, "d/a.txt").as_deref(), Some(b"aaa".as_slice()));
    assert!(
        wait_until(|| !src_root.join("d").exists()).await,
        "a clean move INTO the archive must delete the source after commit"
    );
}

#[tokio::test]
async fn move_into_with_a_skipped_collision_keeps_the_source() {
    // A move where a collision is Skipped must NOT delete the source (its bytes
    // didn't fully land) — the move invariant. Pins `is_move && !any_skipped`.
    let tmp = tempfile::tempdir().expect("tempdir");
    let archive = tmp.path().join("a.zip");
    write_multi_zip(&archive, &[("d/a.txt", b"OLD")]);
    let src_root = tmp.path().join("src");
    std::fs::create_dir_all(src_root.join("d")).expect("mkdir");
    std::fs::write(src_root.join("d/a.txt"), b"NEW").expect("w1"); // collides → Skip
    std::fs::write(src_root.join("d/b.txt"), b"bbb").expect("w2"); // lands

    let events = run_policy_copy_into(&archive, &src_root, "d", ConflictResolution::Skip, true).await;
    assert!(wait_until(|| !events.complete.lock_ignore_poison().is_empty()).await);
    // The non-colliding file landed, the colliding one kept its OLD bytes, and the
    // source survives because something was skipped.
    assert_eq!(read_entry(&archive, "d/b.txt").as_deref(), Some(b"bbb".as_slice()));
    assert_eq!(read_entry(&archive, "d/a.txt").as_deref(), Some(b"OLD".as_slice()));
    assert!(
        src_root.join("d/a.txt").exists() && src_root.join("d/b.txt").exists(),
        "a partial (skipped) move must NOT delete the source"
    );
}

#[tokio::test]
async fn copy_into_preserves_an_empty_source_directory() {
    // An empty source subdir must materialize as an explicit archive dir entry —
    // pins the `!index.exists(&inner) && planned.insert(...)` mkdir guard.
    let tmp = tempfile::tempdir().expect("tempdir");
    let archive = tmp.path().join("a.zip");
    write_multi_zip(&archive, &[("placeholder.txt", b"x")]);
    let src_root = tmp.path().join("src");
    std::fs::create_dir_all(src_root.join("d/empty_sub")).expect("mkdir empty subdir");
    std::fs::write(src_root.join("d/f.txt"), b"f").expect("w");

    let events = run_policy_copy_into(&archive, &src_root, "d", ConflictResolution::Skip, false).await;
    assert!(wait_until(|| !events.complete.lock_ignore_poison().is_empty()).await);
    // The empty directory survives as an explicit `d/empty_sub/` entry.
    let file = std::fs::File::open(&archive).expect("open");
    let mut zip = ZipArchive::new(file).expect("zip");
    assert!(
        zip.by_name("d/empty_sub/").is_ok(),
        "an empty source directory must be preserved as an explicit archive entry"
    );
}

// ---- Data safety: unrepresentable source entries (symlinks, special files) --
//
// A zip changeset can only carry real files and directories. A symlink or a
// special file (fifo/socket/device) the builder can't represent must NOT be
// silently dropped on a MOVE: dropping it while still deleting the source loses
// the user's data. The all-or-nothing move policy applies — any unrepresentable
// entry marks the batch skipped, so the source is preserved (the move degrades
// to a copy) and the skip is surfaced on the terminal event.

#[cfg(unix)]
#[tokio::test]
async fn move_into_a_top_level_symlink_preserves_the_source_and_surfaces_the_skip() {
    use crate::file_system::volume::backends::LocalPosixVolume;
    use crate::file_system::write_operations::route_archive_copy_into;

    let tmp = tempfile::tempdir().expect("tempdir");
    let archive = tmp.path().join("a.zip");
    write_multi_zip(&archive, &[("placeholder.txt", b"x")]);

    let src_root = tmp.path().join("src");
    std::fs::create_dir_all(&src_root).expect("mkdir src");
    std::fs::write(src_root.join("target.txt"), b"real").expect("target");
    std::os::unix::fs::symlink("target.txt", src_root.join("link")).expect("symlink");

    let source_volume: Arc<dyn Volume> = Arc::new(LocalPosixVolume::new("src", src_root.clone()));
    let events = Arc::new(CollectorEventSink::new());
    route_archive_copy_into(
        Arc::clone(&events) as Arc<dyn OperationEventSink>,
        source_volume,
        vec![PathBuf::from("link")],
        archive.clone(),
        "root".to_string(),
        ConflictResolution::Overwrite,
        0,
        true, // is_move
    )
    .await
    .expect("start move-into");

    assert!(wait_until(|| !events.complete.lock_ignore_poison().is_empty()).await);
    // DATA SAFETY: the source symlink survives (it can't be archived, so the move
    // deletes nothing), and it never entered the archive.
    assert!(
        std::fs::symlink_metadata(src_root.join("link")).is_ok(),
        "a moved symlink the archive can't represent must be preserved at the source"
    );
    assert!(
        read_entry(&archive, "link").is_none(),
        "the symlink must not enter the archive"
    );
    let complete = events.complete.lock_ignore_poison();
    assert!(
        complete[0].files_skipped >= 1,
        "the skipped symlink must be surfaced on the terminal event, got {}",
        complete[0].files_skipped
    );
}

#[cfg(unix)]
#[tokio::test]
async fn move_into_a_dir_containing_a_symlink_preserves_the_whole_source_tree() {
    use crate::file_system::volume::backends::LocalPosixVolume;
    use crate::file_system::write_operations::route_archive_copy_into;

    let tmp = tempfile::tempdir().expect("tempdir");
    let archive = tmp.path().join("a.zip");
    write_multi_zip(&archive, &[("placeholder.txt", b"x")]);

    // A dir with a real file AND a symlink inside. WalkDir yields the symlink but
    // the archive can't represent it — the whole move must degrade to a copy.
    let src_root = tmp.path().join("src");
    std::fs::create_dir_all(src_root.join("d")).expect("mkdir src");
    std::fs::write(src_root.join("d/real.txt"), b"real").expect("real");
    std::os::unix::fs::symlink("real.txt", src_root.join("d/link")).expect("symlink");

    let source_volume: Arc<dyn Volume> = Arc::new(LocalPosixVolume::new("src", src_root.clone()));
    let events = Arc::new(CollectorEventSink::new());
    route_archive_copy_into(
        Arc::clone(&events) as Arc<dyn OperationEventSink>,
        source_volume,
        vec![PathBuf::from("d")],
        archive.clone(),
        "root".to_string(),
        ConflictResolution::Overwrite,
        0,
        true, // is_move
    )
    .await
    .expect("start move-into");

    assert!(wait_until(|| !events.complete.lock_ignore_poison().is_empty()).await);
    // DATA SAFETY: the source tree survives (a contained symlink can't be archived,
    // so the batch is all-or-nothing skipped and the source dir is NOT removed).
    assert!(
        std::fs::symlink_metadata(src_root.join("d/link")).is_ok(),
        "a symlink inside a moved dir must be preserved (the source dir is not deleted)"
    );
    assert!(
        src_root.join("d/real.txt").exists(),
        "the source dir survives intact when the move degrades to a copy"
    );
    // The real file still landed in the archive (copy semantics for what CAN be
    // represented); only the source deletion is suppressed.
    assert_eq!(
        read_entry(&archive, "d/real.txt").as_deref(),
        Some(b"real".as_slice()),
        "the representable file is still copied into the archive"
    );
    let complete = events.complete.lock_ignore_poison();
    assert!(
        complete[0].files_skipped >= 1,
        "the skipped symlink must be surfaced, got {}",
        complete[0].files_skipped
    );
}

#[cfg(unix)]
#[tokio::test]
async fn move_into_a_broken_symlink_preserves_the_source() {
    use crate::file_system::volume::backends::LocalPosixVolume;
    use crate::file_system::write_operations::route_archive_copy_into;

    let tmp = tempfile::tempdir().expect("tempdir");
    let archive = tmp.path().join("a.zip");
    write_multi_zip(&archive, &[("placeholder.txt", b"x")]);

    // A dangling symlink (its target doesn't exist): `symlink_metadata` still
    // succeeds (it's an lstat), so it classifies as neither file nor dir.
    let src_root = tmp.path().join("src");
    std::fs::create_dir_all(&src_root).expect("mkdir src");
    std::os::unix::fs::symlink("/nonexistent/target-xyz", src_root.join("broken")).expect("symlink");

    let source_volume: Arc<dyn Volume> = Arc::new(LocalPosixVolume::new("src", src_root.clone()));
    let events = Arc::new(CollectorEventSink::new());
    route_archive_copy_into(
        Arc::clone(&events) as Arc<dyn OperationEventSink>,
        source_volume,
        vec![PathBuf::from("broken")],
        archive.clone(),
        "root".to_string(),
        ConflictResolution::Overwrite,
        0,
        true, // is_move
    )
    .await
    .expect("start move-into");

    assert!(wait_until(|| !events.complete.lock_ignore_poison().is_empty()).await);
    assert!(
        std::fs::symlink_metadata(src_root.join("broken")).is_ok(),
        "a broken symlink must be preserved at the source, never silently deleted"
    );
    let complete = events.complete.lock_ignore_poison();
    assert!(
        complete[0].files_skipped >= 1,
        "the skipped broken symlink must be surfaced, got {}",
        complete[0].files_skipped
    );
}

// ---- Interactive in-archive conflict prompt (Stop policy) -----------------

/// Starts an interactive (Stop-policy) copy INTO `archive` of local dir `src_rel`
/// (relative to `src_root`), landing at the archive root. Returns the collector +
/// the operation id (for `resolve_write_conflict`).
async fn start_interactive_copy_into(
    archive: &Path,
    src_root: &Path,
    src_rel: &str,
) -> (Arc<CollectorEventSink>, String) {
    use crate::file_system::volume::backends::LocalPosixVolume;
    use crate::file_system::write_operations::route_archive_copy_into;

    let source_volume: Arc<dyn Volume> = Arc::new(LocalPosixVolume::new("src", src_root.to_path_buf()));
    let events = Arc::new(CollectorEventSink::new());
    let start = route_archive_copy_into(
        Arc::clone(&events) as Arc<dyn OperationEventSink>,
        source_volume,
        vec![PathBuf::from(src_rel)],
        archive.to_path_buf(),
        "root".to_string(),
        ConflictResolution::Stop,
        0,
        false,
    )
    .await
    .expect("start interactive copy-into");
    (events, start.operation_id)
}

#[tokio::test]
async fn interactive_copy_into_prompts_on_a_file_collision_and_overwrite_replaces() {
    use crate::file_system::write_operations::resolve_write_conflict;

    let tmp = tempfile::tempdir().expect("tempdir");
    let archive = tmp.path().join("a.zip");
    write_multi_zip(&archive, &[("d/existing.txt", b"OLD")]);

    let src_root = tmp.path().join("src");
    std::fs::create_dir_all(src_root.join("d")).expect("mkdir src");
    std::fs::write(src_root.join("d/existing.txt"), b"NEW").expect("w1");
    std::fs::write(src_root.join("d/fresh.txt"), b"fresh").expect("w2");

    let (events, op_id) = start_interactive_copy_into(&archive, &src_root, "d").await;

    // The collision fires a prompt; answer Overwrite.
    assert!(
        wait_until(|| !events.conflicts.lock_ignore_poison().is_empty()).await,
        "a file collision must emit a write-conflict prompt"
    );
    resolve_write_conflict(&op_id, ConflictResolution::Overwrite, false);

    assert!(
        wait_until(|| !events.complete.lock_ignore_poison().is_empty()).await,
        "the edit should complete after the prompt is answered"
    );
    assert_eq!(
        read_entry(&archive, "d/existing.txt").as_deref(),
        Some(b"NEW".as_slice()),
        "Overwrite must replace the colliding entry"
    );
    assert_eq!(
        read_entry(&archive, "d/fresh.txt").as_deref(),
        Some(b"fresh".as_slice()),
        "the non-colliding file is added"
    );
    // A `root`-parent op carries no settle volume id (it's `None`, not `"root"`).
    assert!(wait_until(|| !events.settled.lock_ignore_poison().is_empty()).await);
    assert_eq!(
        events.settled.lock_ignore_poison()[0].volume_id,
        None,
        "a root-parent archive edit settles with no volume id"
    );
}

#[tokio::test]
async fn interactive_copy_into_skip_keeps_the_existing_entry() {
    use crate::file_system::write_operations::resolve_write_conflict;

    let tmp = tempfile::tempdir().expect("tempdir");
    let archive = tmp.path().join("a.zip");
    write_multi_zip(&archive, &[("d/existing.txt", b"OLD")]);

    let src_root = tmp.path().join("src");
    std::fs::create_dir_all(src_root.join("d")).expect("mkdir src");
    std::fs::write(src_root.join("d/existing.txt"), b"NEW").expect("w1");

    let (events, op_id) = start_interactive_copy_into(&archive, &src_root, "d").await;

    assert!(
        wait_until(|| !events.conflicts.lock_ignore_poison().is_empty()).await,
        "a file collision must prompt"
    );
    resolve_write_conflict(&op_id, ConflictResolution::Skip, false);

    assert!(
        wait_until(|| !events.complete.lock_ignore_poison().is_empty()).await,
        "the edit should complete"
    );
    assert_eq!(
        read_entry(&archive, "d/existing.txt").as_deref(),
        Some(b"OLD".as_slice()),
        "Skip must keep the existing entry untouched"
    );
}

#[tokio::test]
async fn interactive_apply_to_all_latches_and_stops_prompting() {
    use crate::file_system::write_operations::resolve_write_conflict;

    let tmp = tempfile::tempdir().expect("tempdir");
    let archive = tmp.path().join("a.zip");
    write_multi_zip(&archive, &[("d/one.txt", b"OLD1"), ("d/two.txt", b"OLD2")]);

    let src_root = tmp.path().join("src");
    std::fs::create_dir_all(src_root.join("d")).expect("mkdir src");
    std::fs::write(src_root.join("d/one.txt"), b"NEW1").expect("w1");
    std::fs::write(src_root.join("d/two.txt"), b"NEW2").expect("w2");

    let (events, op_id) = start_interactive_copy_into(&archive, &src_root, "d").await;

    // Answer the FIRST prompt with Skip + apply-to-all; the second collision must
    // be resolved from the latch WITHOUT a second prompt.
    assert!(
        wait_until(|| !events.conflicts.lock_ignore_poison().is_empty()).await,
        "the first collision must prompt"
    );
    resolve_write_conflict(&op_id, ConflictResolution::Skip, true);

    assert!(
        wait_until(|| !events.complete.lock_ignore_poison().is_empty()).await,
        "the edit should complete"
    );
    assert_eq!(
        events.conflicts.lock_ignore_poison().len(),
        1,
        "apply-to-all must suppress the second prompt"
    );
    // Both colliding entries kept their OLD bytes (Skip-all).
    assert_eq!(read_entry(&archive, "d/one.txt").as_deref(), Some(b"OLD1".as_slice()));
    assert_eq!(read_entry(&archive, "d/two.txt").as_deref(), Some(b"OLD2".as_slice()));
}

#[tokio::test]
async fn interactive_cancel_during_a_prompt_leaves_the_archive_intact() {
    use crate::file_system::write_operations::cancel_write_operation;

    let tmp = tempfile::tempdir().expect("tempdir");
    let archive = tmp.path().join("a.zip");
    write_multi_zip(&archive, &[("d/existing.txt", b"OLD")]);

    let src_root = tmp.path().join("src");
    std::fs::create_dir_all(src_root.join("d")).expect("mkdir src");
    std::fs::write(src_root.join("d/existing.txt"), b"NEW").expect("w1");
    std::fs::write(src_root.join("d/fresh.txt"), b"fresh").expect("w2");

    let (events, op_id) = start_interactive_copy_into(&archive, &src_root, "d").await;

    assert!(
        wait_until(|| !events.conflicts.lock_ignore_poison().is_empty()).await,
        "the collision must prompt"
    );
    // Cancel while the prompt is pending: the planner's recv unblocks with an
    // error, the mutator never runs, and the archive is untouched.
    cancel_write_operation(&op_id, false);

    assert!(
        wait_until(|| !events.cancelled.lock_ignore_poison().is_empty()).await,
        "cancel during a prompt should reach write-cancelled"
    );
    assert_eq!(
        read_entry(&archive, "d/existing.txt").as_deref(),
        Some(b"OLD".as_slice()),
        "cancel must leave the existing entry untouched"
    );
    assert!(
        read_entry(&archive, "d/fresh.txt").is_none(),
        "cancel before commit must add nothing"
    );
    assert!(
        events.complete.lock_ignore_poison().is_empty(),
        "no write-complete on cancel"
    );
}

#[tokio::test]
async fn interactive_conflict_event_carries_both_sides_metadata() {
    use crate::file_system::write_operations::resolve_write_conflict;

    let tmp = tempfile::tempdir().expect("tempdir");
    let archive = tmp.path().join("a.zip");
    write_multi_zip(&archive, &[("d/existing.txt", b"OLDDD")]); // 5 bytes

    let src_root = tmp.path().join("src");
    std::fs::create_dir_all(src_root.join("d")).expect("mkdir src");
    std::fs::write(src_root.join("d/existing.txt"), b"NN").expect("w1"); // 2 bytes

    let (events, op_id) = start_interactive_copy_into(&archive, &src_root, "d").await;

    assert!(
        wait_until(|| !events.conflicts.lock_ignore_poison().is_empty()).await,
        "a file collision must prompt"
    );
    {
        let conflicts = events.conflicts.lock_ignore_poison();
        let ev = &conflicts[0];
        assert_eq!(ev.source_size, Some(2), "source size = incoming file length");
        assert_eq!(ev.destination_size, Some(5), "destination size = archive entry length");
        assert_eq!(ev.size_difference, Some(3), "size_difference = dest - source (5 - 2)");
        assert!(!ev.source_is_directory, "the incoming side is a file");
        assert!(
            !ev.destination_is_directory,
            "the colliding archive entry is a file, not a folder"
        );
    }
    resolve_write_conflict(&op_id, ConflictResolution::Skip, false);
    assert!(wait_until(|| !events.complete.lock_ignore_poison().is_empty()).await);
}

#[tokio::test]
async fn interactive_move_into_with_a_skipped_collision_keeps_the_source() {
    use crate::file_system::volume::backends::LocalPosixVolume;
    use crate::file_system::write_operations::{resolve_write_conflict, route_archive_copy_into};

    let tmp = tempfile::tempdir().expect("tempdir");
    let archive = tmp.path().join("a.zip");
    write_multi_zip(&archive, &[("d/a.txt", b"OLD")]);
    let src_root = tmp.path().join("src");
    std::fs::create_dir_all(src_root.join("d")).expect("mkdir src");
    std::fs::write(src_root.join("d/a.txt"), b"NEW").expect("w1"); // collides
    std::fs::write(src_root.join("d/b.txt"), b"bbb").expect("w2"); // lands

    let source_volume: Arc<dyn Volume> = Arc::new(LocalPosixVolume::new("src", src_root.to_path_buf()));
    let events = Arc::new(CollectorEventSink::new());
    let start = route_archive_copy_into(
        Arc::clone(&events) as Arc<dyn OperationEventSink>,
        source_volume,
        vec![PathBuf::from("d")],
        archive.clone(),
        "root".to_string(),
        ConflictResolution::Stop,
        0,
        true, // is_move
    )
    .await
    .expect("start interactive move-into");

    assert!(
        wait_until(|| !events.conflicts.lock_ignore_poison().is_empty()).await,
        "the collision must prompt"
    );
    resolve_write_conflict(&start.operation_id, ConflictResolution::Skip, false);

    assert!(wait_until(|| !events.complete.lock_ignore_poison().is_empty()).await);
    // Something was skipped → the move invariant keeps the source intact.
    assert!(
        src_root.join("d/a.txt").exists() && src_root.join("d/b.txt").exists(),
        "an interactive move with a skipped collision must NOT delete the source"
    );
}

#[tokio::test]
async fn interactive_dir_vs_dir_merges_without_prompting() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let archive = tmp.path().join("a.zip");
    // The archive already holds directory `d` (implied by `d/keep.txt`).
    write_multi_zip(&archive, &[("d/keep.txt", b"keep")]);

    let src_root = tmp.path().join("src");
    std::fs::create_dir_all(src_root.join("d")).expect("mkdir src");
    std::fs::write(src_root.join("d/new.txt"), b"new").expect("w1");

    let (events, _op_id) = start_interactive_copy_into(&archive, &src_root, "d").await;

    assert!(
        wait_until(|| !events.complete.lock_ignore_poison().is_empty()).await,
        "the merge should complete with no prompt"
    );
    // The directory collision merged silently — no prompt fired.
    assert!(
        events.conflicts.lock_ignore_poison().is_empty(),
        "dir-vs-dir must merge WITHOUT a conflict prompt"
    );
    assert_eq!(read_entry(&archive, "d/new.txt").as_deref(), Some(b"new".as_slice()));
    assert_eq!(read_entry(&archive, "d/keep.txt").as_deref(), Some(b"keep".as_slice()));
    // Merging into a dir that ALREADY exists in the archive must NOT synthesize a
    // redundant explicit `d/` directory entry (the mkdir guard skips existing dirs).
    let file = std::fs::File::open(&archive).expect("open");
    let mut zip = ZipArchive::new(file).expect("zip");
    assert!(
        zip.by_name("d/").is_err(),
        "a merge into a pre-existing archive dir must not add a redundant explicit dir entry"
    );
}
