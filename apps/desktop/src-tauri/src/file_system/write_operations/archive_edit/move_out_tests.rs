//! Headless tests for the out-of-zip MOVE compound op (`route_archive_move_out`):
//! extract through the copy engine, then a batch archive delete only on a fully
//! clean extract. The data-safety core: a dest failure, a skipped collision, or a
//! cancel must delete NOTHING from the archive.

use std::future::Future;
use std::pin::Pin;

use super::test_support::*;
use crate::file_system::volume::VolumeError;

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
    fn lane_key(&self) -> crate::file_system::volume::LaneKey {
        // Delegate to the inner volume so the injected unique lane key (below)
        // wins over the trait default (root `/`, shared across instances).
        self.inner.lane_key()
    }
    // `write_from_stream` deliberately NOT implemented — the trait default
    // returns `NotSupported`, which is the dest-side failure this double injects.
}

/// Builds a local-backed `ArchiveVolume` over `archive_path` for move-out tests.
///
/// The parent carries a UNIQUE lane key (via `with_lane_key`) so each test's
/// move-out gets its own operation-manager lane. The move-out reserves
/// `source_volume.lane_key()` (the archive's parent), and an `InMemoryVolume`
/// otherwise defaults its lane to its root `/` — shared across every instance,
/// serializing the whole move-out suite onto one lane (see
/// `test_support::unique_lane_id` for the isolation rationale).
fn archive_source_volume(archive_path: &Path) -> Arc<dyn Volume> {
    use crate::file_system::volume::InMemoryVolume;
    use crate::file_system::volume::backends::archive::{ArchiveFormat, ArchiveVolume};
    let parent: Arc<dyn Volume> = Arc::new(
        InMemoryVolume::new("parent")
            .with_local_fs_access()
            .with_lane_key(unique_lane_id()),
    );
    Arc::new(ArchiveVolume::new(
        parent,
        archive_path.to_path_buf(),
        ArchiveFormat::Zip,
    ))
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

    wait_until_async(Duration::from_secs(5), "the write-complete event", || {
        !events.complete.lock_ignore_poison().is_empty()
    })
    .await;
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
    wait_until_async(Duration::from_secs(5), "the write-settled event", || {
        !events.settled.lock_ignore_poison().is_empty()
    })
    .await;
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
        inner: Arc::new(InMemoryVolume::new("dest").with_lane_key(unique_lane_id())),
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

    wait_until_async(Duration::from_secs(5), "the write-error event", || {
        !events.errors.lock_ignore_poison().is_empty()
    })
    .await;
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
async fn move_out_a_lone_skipped_source_stays_in_the_archive() {
    use crate::file_system::volume::backends::LocalPosixVolume;
    use crate::file_system::write_operations::route_archive_move_out;

    let tmp = tempfile::tempdir().expect("tempdir");
    let archive = tmp.path().join("a.zip");
    write_multi_zip(&archive, &[("keep.txt", b"keep"), ("move_me.txt", b"payload")]);

    // The destination already holds `move_me.txt`, so a Skip-policy extract skips
    // the only source. Nothing extracted cleanly, so nothing is deleted from the
    // archive (the lone skipped source survives).
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

    wait_until_async(Duration::from_secs(5), "the write-complete event", || {
        !events.complete.lock_ignore_poison().is_empty()
    })
    .await;
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

    wait_until_async(
        Duration::from_secs(5),
        "a terminal event (cancelled or complete)",
        || !events.cancelled.lock_ignore_poison().is_empty() || !events.complete.lock_ignore_poison().is_empty(),
    )
    .await;
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
async fn move_out_dir_with_a_deep_skipped_child_keeps_that_child_in_the_archive() {
    use crate::file_system::volume::backends::LocalPosixVolume;
    use crate::file_system::write_operations::route_archive_move_out;

    let tmp = tempfile::tempdir().expect("tempdir");
    let archive = tmp.path().join("a.zip");
    // A directory source with two children; one will collide at the destination.
    write_multi_zip(
        &archive,
        &[("dir/keep_me.txt", b"payload"), ("dir/collides.txt", b"archived")],
    );

    // The destination already holds `dir/collides.txt`, so a Skip-policy extract
    // skips it DEEP inside the directory subtree while `dir/keep_me.txt` lands.
    let dest_dir = tmp.path().join("out");
    std::fs::create_dir_all(dest_dir.join("dir")).expect("mkdir dest/dir");
    std::fs::write(dest_dir.join("dir/collides.txt"), b"EXISTING").expect("pre-existing dest child");

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
        vec![archive.join("dir")],
        "dest".to_string(),
        dest_volume,
        dest_dir.clone(),
        config,
    )
    .await
    .expect("start move-out");

    wait_until_async(Duration::from_secs(5), "the write-complete event", || {
        !events.complete.lock_ignore_poison().is_empty()
    })
    .await;
    // CRITICAL (data safety): the deep-skipped child never landed at the dest, so
    // its archive entry MUST survive — deleting the whole `dir/` subtree would
    // lose it.
    assert_eq!(
        read_entry(&archive, "dir/collides.txt").as_deref(),
        Some(b"archived".as_slice()),
        "a deep-skipped child must NOT be deleted from the archive (no data loss)"
    );
    // The pre-existing destination copy is untouched by Skip.
    assert_eq!(
        std::fs::read(dest_dir.join("dir/collides.txt")).ok().as_deref(),
        Some(b"EXISTING".as_slice()),
        "Skip keeps the pre-existing destination file"
    );
}

/// A destination volume that delegates everything to an inner `InMemoryVolume`
/// but FAILS `write_from_stream` for one target file name. Used to make exactly
/// one source of a move-out error while its predecessors land durably, proving
/// the archive delete drops ONLY the durable prefix.
struct FailOnNameVolume {
    inner: Arc<crate::file_system::volume::InMemoryVolume>,
    fail_name: String,
}

impl Volume for FailOnNameVolume {
    fn name(&self) -> &str {
        self.inner.name()
    }
    fn root(&self) -> &Path {
        self.inner.root()
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn lane_key(&self) -> crate::file_system::volume::LaneKey {
        self.inner.lane_key()
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
    fn create_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        self.inner.create_directory(path)
    }
    fn create_directory_all<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        self.inner.create_directory_all(path)
    }
    fn write_from_stream<'a>(
        &'a self,
        dest: &'a Path,
        size: u64,
        stream: Box<dyn crate::file_system::volume::VolumeReadStream>,
        on_progress: &'a (dyn Fn(u64, u64) -> std::ops::ControlFlow<()> + Sync),
    ) -> Pin<Box<dyn Future<Output = Result<u64, VolumeError>> + Send + 'a>> {
        let fails = dest
            .file_name()
            .map(|n| n.to_string_lossy() == self.fail_name)
            .unwrap_or(false);
        if fails {
            return Box::pin(async {
                Err(VolumeError::IoError {
                    message: "injected write failure".to_string(),
                    raw_os_error: None,
                })
            });
        }
        self.inner.write_from_stream(dest, size, stream, on_progress)
    }
}

#[tokio::test]
async fn move_out_partial_skip_converges_deletes_landed_keeps_skipped() {
    use crate::file_system::volume::backends::LocalPosixVolume;
    use crate::file_system::write_operations::route_archive_move_out;

    let tmp = tempfile::tempdir().expect("tempdir");
    let archive = tmp.path().join("a.zip");
    write_multi_zip(&archive, &[("a.txt", b"AAA"), ("b.txt", b"BBB")]);

    // The destination already holds `b.txt`, so a Skip-policy extract skips it and
    // lands `a.txt`. Convergence: `a.txt` is deleted from the archive, `b.txt` stays.
    let dest_dir = tmp.path().join("out");
    std::fs::create_dir_all(&dest_dir).expect("mkdir dest");
    std::fs::write(dest_dir.join("b.txt"), b"EXISTING").expect("pre-existing dest");

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
        vec![archive.join("a.txt"), archive.join("b.txt")],
        "dest".to_string(),
        dest_volume,
        dest_dir.clone(),
        config,
    )
    .await
    .expect("start move-out");

    wait_until_async(Duration::from_secs(5), "the write-complete event", || {
        !events.complete.lock_ignore_poison().is_empty()
    })
    .await;
    // The landed source is gone from the archive; the skipped one survives.
    assert!(
        read_entry(&archive, "a.txt").is_none(),
        "the fully-extracted source must be deleted from the archive"
    );
    assert_eq!(
        read_entry(&archive, "b.txt").as_deref(),
        Some(b"BBB".as_slice()),
        "the skipped source must stay in the archive"
    );
    // The landed file is at the destination; the pre-existing one is untouched.
    assert_eq!(
        std::fs::read(dest_dir.join("a.txt")).ok().as_deref(),
        Some(b"AAA".as_slice())
    );
    assert_eq!(
        std::fs::read(dest_dir.join("b.txt")).ok().as_deref(),
        Some(b"EXISTING".as_slice())
    );
}

#[tokio::test]
async fn move_out_error_mid_tree_deletes_only_the_durable_prefix() {
    use crate::file_system::volume::InMemoryVolume;
    use crate::file_system::write_operations::route_archive_move_out;

    let tmp = tempfile::tempdir().expect("tempdir");
    let archive = tmp.path().join("a.zip");
    write_multi_zip(&archive, &[("a.txt", b"AAA"), ("b.txt", b"BBB")]);

    // Two sources, serial order [a, b]. `a.txt` lands durably; `b.txt` fails on the
    // dest side. The archive delete drops ONLY the durable prefix (`a.txt`).
    let inner = Arc::new(
        InMemoryVolume::new("dest")
            .with_lane_key(unique_lane_id())
            .with_space_info(1 << 30, 1 << 30),
    );
    inner.create_directory(Path::new("/out")).await.expect("seed dest dir");
    let dest_volume: Arc<dyn Volume> = Arc::new(FailOnNameVolume {
        inner: Arc::clone(&inner),
        fail_name: "b.txt".to_string(),
    });
    let source_volume = archive_source_volume(&archive);

    let events = Arc::new(CollectorEventSink::new());
    route_archive_move_out(
        Arc::clone(&events) as Arc<dyn OperationEventSink>,
        "root".to_string(),
        source_volume,
        vec![archive.join("a.txt"), archive.join("b.txt")],
        "dest".to_string(),
        dest_volume,
        PathBuf::from("/out"),
        move_out_config(),
    )
    .await
    .expect("start move-out");

    wait_until_async(Duration::from_secs(5), "the write-error event", || {
        !events.errors.lock_ignore_poison().is_empty()
    })
    .await;
    // Convergence on error: the durable prefix `a.txt` is deleted, the failed
    // `b.txt` stays (no data loss — its bytes never landed).
    assert!(
        read_entry(&archive, "a.txt").is_none(),
        "the durable prefix must be deleted from the archive so a retry converges"
    );
    assert_eq!(
        read_entry(&archive, "b.txt").as_deref(),
        Some(b"BBB".as_slice()),
        "the failed source must NOT be deleted from the archive"
    );
    assert!(
        events.complete.lock_ignore_poison().is_empty(),
        "no write-complete on a failed move-out"
    );
}

#[tokio::test]
async fn move_out_rollback_deletes_nothing_from_the_archive() {
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

    // `true` = rollback (delete already-copied files). The archive must stay intact
    // regardless: a rolled-back extraction leaves nothing durable to move out.
    cancel_write_operation(&start.operation_id, true);

    wait_until_async(
        Duration::from_secs(5),
        "a terminal event (cancelled or complete)",
        || !events.cancelled.lock_ignore_poison().is_empty() || !events.complete.lock_ignore_poison().is_empty(),
    )
    .await;
    assert_eq!(
        read_entry(&archive, "move_me.txt").as_deref(),
        Some(b"payload".as_slice()),
        "rollback must NOT delete the archive entry"
    );
    assert_eq!(read_entry(&archive, "keep.txt").as_deref(), Some(b"keep".as_slice()));
}
