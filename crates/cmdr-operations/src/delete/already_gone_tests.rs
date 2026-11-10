//! Deleting or trashing an item that's already gone counts as done.
//!
//! Regression anchor (ERR-X84HN, 2026-10-28): another app moved a file out of a
//! folder a pane was showing, the pane's watcher missed it, and the person's
//! permanent delete of that row stopped with `SourceNotFound` and auto-sent an
//! error report. The file was gone, which is what the person wanted.

use std::fs;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use super::super::OperationEventSink;
use super::super::event_sinks::CollectorEventSink;
use super::super::scan_cache::{ScanOutcome, ScanPreviewState};
use super::super::state::{CachedScanResult, FileInfo};
use super::super::testing::TestOperations;
use super::super::types::{
    AlreadyGone, ScanPreviewPurpose, SourceItemOutcome, WriteCompleteEvent, WriteOperationConfig, WriteOperationError,
};
use super::walker::delete_files_with_progress_inner;
use cmdr_fs::entry::FileEntry;
use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::testing::{TestDir, wait_until_async};
use cmdr_fs::volume::{CopyScanResult, EntryKind, InMemoryVolume, Volume, VolumeError};
use cmdr_operation_log::types::Initiator;

const WAIT: Duration = Duration::from_secs(5);

async fn settled_completion(events: &CollectorEventSink) -> WriteCompleteEvent {
    wait_until_async(WAIT, "the operation to settle", || {
        !events.settled.lock_ignore_poison().is_empty()
    })
    .await;
    assert!(
        events.errors.lock_ignore_poison().is_empty(),
        "an item that's already gone is not a failure: {:?}",
        events.errors.lock_ignore_poison()
    );
    let complete = events.complete.lock_ignore_poison();
    assert_eq!(complete.len(), 1, "exactly one completion");
    complete[0].clone()
}

fn skipped_gone(events: &CollectorEventSink) -> Vec<(String, SourceItemOutcome, bool)> {
    events
        .source_items_done
        .lock_ignore_poison()
        .iter()
        .filter(|event| event.outcome == SourceItemOutcome::Skipped)
        .map(|event| (event.source_path.clone(), event.outcome, event.source_removed))
        .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_local_delete_of_a_file_that_is_already_gone_completes() {
    let root = TestDir::new("already-gone-local-one");
    let gone = root.join("song.m4a");
    let engine = TestOperations::new();
    let events = Arc::new(CollectorEventSink::new());

    engine
        .delete_files_start(
            Arc::clone(&events) as Arc<dyn OperationEventSink>,
            vec![gone.clone()],
            WriteOperationConfig::default(),
            None,
            Initiator::User,
            None,
        )
        .await
        .expect("start delete");

    let complete = settled_completion(&events).await;
    assert_eq!(
        complete.already_gone,
        Some(AlreadyGone {
            item_count: 1,
            total_count: 1,
            name: Some("song.m4a".to_string()),
        })
    );
    assert_eq!(complete.files_processed, 0);
    assert_eq!(
        skipped_gone(&events),
        vec![(gone.display().to_string(), SourceItemOutcome::Skipped, true)],
        "the gone source is reported removed, so search snapshots drop it"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_local_delete_deletes_what_is_there_and_reports_what_was_already_gone() {
    let root = TestDir::new("already-gone-local-mixed");
    let present = root.join("present.txt");
    fs::write(&present, b"here").expect("present file");
    let gone_a = root.join("gone-a.txt");
    let gone_b = root.join("gone-b.txt");
    let engine = TestOperations::new();
    let events = Arc::new(CollectorEventSink::new());

    engine
        .delete_files_start(
            Arc::clone(&events) as Arc<dyn OperationEventSink>,
            vec![gone_a, present.clone(), gone_b],
            WriteOperationConfig::default(),
            None,
            Initiator::User,
            None,
        )
        .await
        .expect("start delete");

    let complete = settled_completion(&events).await;
    assert!(!present.exists(), "the item that was there is deleted");
    assert_eq!(complete.files_processed, 1);
    assert_eq!(
        complete.already_gone,
        Some(AlreadyGone {
            item_count: 2,
            total_count: 3,
            name: None,
        }),
        "a name only rides along for a single gone item"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_delete_with_nothing_gone_carries_no_already_gone_note() {
    let root = TestDir::new("already-gone-none");
    let present = root.join("present.txt");
    fs::write(&present, b"here").expect("present file");
    let engine = TestOperations::new();
    let events = Arc::new(CollectorEventSink::new());

    engine
        .delete_files_start(
            Arc::clone(&events) as Arc<dyn OperationEventSink>,
            vec![present],
            WriteOperationConfig::default(),
            None,
            Initiator::User,
            None,
        )
        .await
        .expect("start delete");

    let complete = settled_completion(&events).await;
    assert_eq!(complete.already_gone, None);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_volume_delete_of_an_item_that_is_already_gone_completes() {
    let volume = Arc::new(InMemoryVolume::new("naspi"));
    volume.create_file(Path::new("/here.txt"), b"here").await.expect("file");
    let engine = TestOperations::new();
    let volume_id = format!("already-gone-volume-{}", std::process::id());
    engine.volumes().register(&volume_id, volume.clone() as Arc<dyn Volume>);
    let events = Arc::new(CollectorEventSink::new());

    engine
        .delete_files_start(
            Arc::clone(&events) as Arc<dyn OperationEventSink>,
            vec![PathBuf::from("/here.txt"), PathBuf::from("/gone.txt")],
            WriteOperationConfig::default(),
            Some(volume_id),
            Initiator::User,
            None,
        )
        .await
        .expect("start delete");

    let complete = settled_completion(&events).await;
    assert!(
        !volume.exists(Path::new("/here.txt")).await,
        "the item that was there is deleted"
    );
    assert_eq!(
        complete.already_gone,
        Some(AlreadyGone {
            item_count: 1,
            total_count: 2,
            name: Some("gone.txt".to_string()),
        })
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_trash_of_an_item_that_is_already_gone_completes_without_a_refusal() {
    let root = TestDir::new("already-gone-trash");
    let gone = root.join("gone.txt");
    let engine = TestOperations::new();
    let events = Arc::new(CollectorEventSink::new());

    engine
        .trash_files_start(
            Arc::clone(&events) as Arc<dyn OperationEventSink>,
            vec![gone],
            None,
            WriteOperationConfig::default(),
            Initiator::User,
            None,
        )
        .await
        .expect("start trash");

    let complete = settled_completion(&events).await;
    assert!(
        complete.refused.is_none(),
        "a gone item is not something the OS refused"
    );
    assert_eq!(
        complete.already_gone,
        Some(AlreadyGone {
            item_count: 1,
            total_count: 1,
            name: Some("gone.txt".to_string()),
        })
    );
}

/// The race inside a running delete: a file the scan listed vanishes before its
/// unlink. Its absence is what the delete was for, so the walk carries on.
#[test]
fn a_local_delete_carries_on_past_a_file_that_vanished_after_the_scan() {
    let root = TestDir::new("already-gone-race");
    let folder = root.join("folder");
    fs::create_dir(&folder).expect("folder");
    let stays = folder.join("a.txt");
    let vanishes = folder.join("b.txt");
    fs::write(&stays, b"a").expect("a");
    fs::write(&vanishes, b"b").expect("b");

    let engine = TestOperations::new();
    let mut files = Vec::new();
    for file in [&stays, &vanishes] {
        let metadata = fs::symlink_metadata(file).expect("scanned file exists");
        files.push(FileInfo::new(file.clone(), root.to_path_buf(), &metadata));
    }
    let preview = CachedScanResult::from_local_walk(
        ScanPreviewPurpose::Delete,
        vec![folder.clone()],
        files,
        vec![folder.clone()],
        2,
        2,
        vec![(
            folder.clone(),
            CopyScanResult {
                file_count: 2,
                dir_count: 0,
                total_bytes: 2,
                dedup_bytes: 2,
                top_level_is_directory: true,
                top_level_modified_at: None,
                top_level_created_at: None,
            },
        )],
        None,
    );
    let preview_id = "already-gone-race-preview".to_string();
    engine
        .stores()
        .previews()
        .insert_scan_result(preview_id.clone(), preview);
    fs::remove_file(&vanishes).expect("another app takes b.txt");

    let op_id = "already-gone-race-op";
    let op = engine.register_state(op_id, engine.new_state());
    let sink = CollectorEventSink::new();
    let config = WriteOperationConfig {
        preview_id: Some(preview_id),
        ..WriteOperationConfig::default()
    };

    let result = delete_files_with_progress_inner(&sink, op_id, op.state(), std::slice::from_ref(&folder), &config);

    assert!(result.is_ok(), "{result:?}");
    assert!(
        !folder.exists(),
        "the rest of the folder is deleted, the folder with it"
    );
    assert_eq!(sink.complete.lock_ignore_poison().len(), 1);
}

/// Counts the top-level probes (`get_metadata` and `entry_kind`) per path, and
/// forwards everything a volume delete asks to an `InMemoryVolume`.
struct ProbeCountingVolume {
    inner: InMemoryVolume,
    probes: std::sync::Mutex<std::collections::HashMap<PathBuf, usize>>,
}

impl ProbeCountingVolume {
    fn count(&self, path: &Path) {
        *self.probes.lock_ignore_poison().entry(path.to_path_buf()).or_default() += 1;
    }
    fn probes_of(&self, path: &str) -> usize {
        self.probes
            .lock_ignore_poison()
            .get(Path::new(path))
            .copied()
            .unwrap_or(0)
    }
}

impl Volume for ProbeCountingVolume {
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
        on_progress: Option<&'a (dyn Fn(cmdr_fs::volume::ListingProgress) + Sync)>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
        self.inner.list_directory(path, on_progress)
    }
    fn get_metadata<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<FileEntry, VolumeError>> + Send + 'a>> {
        self.count(path);
        self.inner.get_metadata(path)
    }
    fn exists<'a>(&'a self, path: &'a Path) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        self.inner.exists(path)
    }
    fn is_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<bool, VolumeError>> + Send + 'a>> {
        self.count(path);
        self.inner.is_directory(path)
    }
    fn entry_kind<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<EntryKind, VolumeError>> + Send + 'a>> {
        self.count(path);
        self.inner.entry_kind(path)
    }
    fn delete<'a>(&'a self, path: &'a Path) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        self.inner.delete(path)
    }
}

/// Regression anchor: the already-gone probe came on top of the walk's own
/// top-level probe, so an S3 delete sent a second HEAD (and a folder a second
/// listing) per selected item, more than its cost estimate counts
/// (`s3_integration_the_engine_sends_what_the_estimate_counts_*`). The probe's
/// answer now feeds the walk: one probe per top-level item, and what's there
/// still goes.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_volume_delete_probes_each_selected_item_once() {
    let volume = Arc::new(ProbeCountingVolume {
        inner: InMemoryVolume::new("bucket"),
        probes: std::sync::Mutex::new(std::collections::HashMap::new()),
    });
    volume
        .inner
        .create_file(Path::new("/file.txt"), b"x")
        .await
        .expect("file");
    volume
        .inner
        .create_directory(Path::new("/folder"))
        .await
        .expect("folder");
    volume
        .inner
        .create_file(Path::new("/folder/child.txt"), b"y")
        .await
        .expect("child");
    let engine = TestOperations::new();
    let volume_id = format!("already-gone-probe-once-{}", std::process::id());
    engine.volumes().register(&volume_id, volume.clone() as Arc<dyn Volume>);
    let events = Arc::new(CollectorEventSink::new());

    engine
        .delete_files_start(
            Arc::clone(&events) as Arc<dyn OperationEventSink>,
            vec![PathBuf::from("/file.txt"), PathBuf::from("/folder")],
            WriteOperationConfig::default(),
            Some(volume_id),
            Initiator::User,
            None,
        )
        .await
        .expect("start delete");

    let complete = settled_completion(&events).await;
    assert_eq!(complete.already_gone, None);
    assert!(
        !volume.inner.exists(Path::new("/file.txt")).await,
        "the file is deleted"
    );
    assert!(
        !volume.inner.exists(Path::new("/folder")).await,
        "the folder is deleted"
    );
    assert_eq!(
        volume.probes_of("/file.txt"),
        1,
        "{:?}",
        volume.probes.lock_ignore_poison()
    );
    assert_eq!(
        volume.probes_of("/folder"),
        1,
        "{:?}",
        volume.probes.lock_ignore_poison()
    );
}

// ============================================================================
// The confirming dialog's preview found the item gone
// ============================================================================

fn unique_preview_id(label: &str) -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    format!(
        "already-gone-{label}-{}-{}",
        N.fetch_add(1, Ordering::Relaxed),
        std::process::id()
    )
}

/// A preview still walking when the person confirms, as the dialog leaves it.
fn register_walking_preview(engine: &TestOperations, preview_id: &str) {
    engine.stores().previews().register_preview(
        preview_id.to_string(),
        Arc::new(ScanPreviewState {
            cancelled: std::sync::atomic::AtomicBool::new(false),
            progress_interval: Duration::from_millis(50),
            mac_only: cmdr_fs::mac_only_files::MacOnlyFiles::Include,
        }),
    );
}

fn settle_as_not_found(engine: &TestOperations, preview_id: &str, path: &Path) {
    engine.stores().previews().settle_preview(
        preview_id,
        ScanOutcome::Error(WriteOperationError::SourceNotFound {
            path: path.display().to_string(),
        }),
        None,
    );
}

/// Regression anchor (ERR-7YG5T, 2026-11-07): two deletes of one SMB file four
/// seconds apart. The first removed it; the second's preview then settled with
/// `NotFound`, and the scan wait failed the op on it before `split_on` ran, so a
/// delete of an already-gone file auto-sent an error report.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_volume_delete_whose_preview_found_the_item_gone_completes() {
    let volume = Arc::new(InMemoryVolume::new("naspi"));
    let engine = TestOperations::new();
    let volume_id = format!("already-gone-preview-volume-{}", std::process::id());
    engine.volumes().register(&volume_id, volume as Arc<dyn Volume>);
    let preview_id = unique_preview_id("volume");
    register_walking_preview(&engine, &preview_id);
    let gone = PathBuf::from("/notes/gone.json");
    let events = Arc::new(CollectorEventSink::new());

    engine
        .delete_files_start(
            Arc::clone(&events) as Arc<dyn OperationEventSink>,
            vec![gone.clone()],
            WriteOperationConfig {
                preview_id: Some(preview_id.clone()),
                ..WriteOperationConfig::default()
            },
            Some(volume_id),
            Initiator::User,
            None,
        )
        .await
        .expect("start delete");
    settle_as_not_found(&engine, &preview_id, &gone);

    let complete = settled_completion(&events).await;
    assert_eq!(
        complete.already_gone,
        Some(AlreadyGone {
            item_count: 1,
            total_count: 1,
            name: Some("gone.json".to_string()),
        })
    );
}

/// The local twin of the anchor above: the local delete waits on the same
/// preview bridge.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_local_delete_whose_preview_found_the_item_gone_completes() {
    let root = TestDir::new("already-gone-preview-local");
    let gone = root.join("gone.json");
    let engine = TestOperations::new();
    let preview_id = unique_preview_id("local");
    register_walking_preview(&engine, &preview_id);
    let events = Arc::new(CollectorEventSink::new());

    engine
        .delete_files_start(
            Arc::clone(&events) as Arc<dyn OperationEventSink>,
            vec![gone.clone()],
            WriteOperationConfig {
                preview_id: Some(preview_id.clone()),
                ..WriteOperationConfig::default()
            },
            None,
            Initiator::User,
            None,
        )
        .await
        .expect("start delete");
    settle_as_not_found(&engine, &preview_id, &gone);

    let complete = settled_completion(&events).await;
    assert_eq!(complete.already_gone.map(|gone| gone.item_count), Some(1));
}

/// The preview's "missing" steps aside only for an operation whose goal is the
/// absence. A copy of a missing source still fails on it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_copy_whose_preview_found_a_source_missing_still_fails() {
    let engine = TestOperations::new();
    let (events, _op_id, preview_id, dir) =
        crate::scan_bridge_tests::start_copy_awaiting_preview(&engine, "already-gone-copy").await;
    settle_as_not_found(&engine, &preview_id, &dir.join("src"));

    wait_until_async(WAIT, "the write-error event", || {
        !events.errors.lock_ignore_poison().is_empty()
    })
    .await;
    let errors = events.errors.lock_ignore_poison();
    assert!(
        matches!(errors[0].error, WriteOperationError::SourceNotFound { .. }),
        "{:?}",
        errors[0].error
    );
}

// ============================================================================
// The race inside a volume walk
// ============================================================================

/// An `InMemoryVolume` with someone else at work on it.
struct RacingVolume {
    inner: InMemoryVolume,
    /// Another app removes this right after the walk lists its parent.
    vanishes_after_listing: Option<PathBuf>,
    /// Answers `NotFound` to its delete yet stays put, as a backend whose
    /// delete follows a dangling link does.
    delete_answers_not_found: Option<PathBuf>,
}

impl Volume for RacingVolume {
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
        on_progress: Option<&'a (dyn Fn(cmdr_fs::volume::ListingProgress) + Sync)>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let listing = self.inner.list_directory(path, on_progress).await;
            if let Some(vanishes) = &self.vanishes_after_listing
                && vanishes.parent() == Some(path)
                && self.inner.exists(vanishes).await
            {
                self.inner.delete(vanishes).await.expect("another app takes it");
            }
            listing
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
        self.inner.is_directory(path)
    }
    fn entry_kind<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<EntryKind, VolumeError>> + Send + 'a>> {
        self.inner.entry_kind(path)
    }
    fn delete<'a>(&'a self, path: &'a Path) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        if self.delete_answers_not_found.as_deref() == Some(path) {
            return Box::pin(async move { Err(VolumeError::NotFound(path.display().to_string())) });
        }
        self.inner.delete(path)
    }
}

async fn racing_folder(vanishes: Option<&str>, answers_not_found: Option<&str>) -> Arc<RacingVolume> {
    let volume = Arc::new(RacingVolume {
        inner: InMemoryVolume::new("naspi"),
        vanishes_after_listing: vanishes.map(PathBuf::from),
        delete_answers_not_found: answers_not_found.map(PathBuf::from),
    });
    volume
        .inner
        .create_directory(Path::new("/folder"))
        .await
        .expect("folder");
    for name in ["/folder/a.txt", "/folder/b.txt"] {
        volume.inner.create_file(Path::new(name), b"x").await.expect("file");
    }
    volume
}

async fn delete_on(volume: &Arc<RacingVolume>, label: &str) -> Arc<CollectorEventSink> {
    let engine = TestOperations::new();
    let volume_id = format!("already-gone-{label}-{}", std::process::id());
    engine
        .volumes()
        .register(&volume_id, Arc::clone(volume) as Arc<dyn Volume>);
    let events = Arc::new(CollectorEventSink::new());
    engine
        .delete_files_start(
            Arc::clone(&events) as Arc<dyn OperationEventSink>,
            vec![PathBuf::from("/folder")],
            WriteOperationConfig::default(),
            Some(volume_id),
            Initiator::User,
            None,
        )
        .await
        .expect("start delete");
    wait_until_async(WAIT, "the operation to settle", || {
        !events.settled.lock_ignore_poison().is_empty()
    })
    .await;
    events
}

/// The volume twin of `already-gone-race`: a file the walk listed vanishes
/// before its delete. Its absence is what the delete was for, so the walk
/// carries on and the folder goes with the rest.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_volume_delete_carries_on_past_a_file_that_vanished_after_the_walk() {
    let volume = racing_folder(Some("/folder/b.txt"), None).await;

    let events = delete_on(&volume, "volume-race").await;

    let complete = settled_completion(&events).await;
    assert_eq!(complete.files_processed, 2, "the vanished file counts as done");
    assert!(
        !volume.inner.exists(Path::new("/folder")).await,
        "the folder is deleted"
    );
    let sources = events.source_items_done.lock_ignore_poison();
    assert!(
        sources
            .iter()
            .any(|event| event.source_path == "/folder" && event.source_removed),
        "{sources:?}"
    );
}

/// Only a POSITIVE absence counts: a delete that answers `NotFound` for an
/// entry the link-aware probe still sees (a dangling link on a backend whose
/// delete follows it) stays a failure, ❌ never a silent "done".
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_volume_delete_answering_not_found_for_an_entry_still_there_fails() {
    let volume = racing_folder(None, Some("/folder/b.txt")).await;

    let events = delete_on(&volume, "volume-false-not-found").await;

    {
        let errors = events.errors.lock_ignore_poison();
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert!(
            matches!(errors[0].error, WriteOperationError::SourceNotFound { .. }),
            "{:?}",
            errors[0].error
        );
    }
    assert!(volume.inner.exists(Path::new("/folder/b.txt")).await, "it stays");
}
