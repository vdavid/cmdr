//! What a move INTO a zip does with its originals when the source changes while
//! the move runs.
//!
//! The move reads its sources (pulls a remote one into scratch, or walks a local
//! one), rewrites the archive, and only then removes the originals. Anything
//! saved over or added in the source in that window exists ONLY there, so the
//! removal takes exactly what the move carried, and only while it still looks
//! the way the move found it.
//!
//! The window is reproduced by a sink that edits the source on the archive
//! rewrite's first progress tick: the sources were read before it, and the
//! originals go after it.

use std::sync::Mutex;

use futures_util::FutureExt;

use super::test_support::*;
use crate::file_system::write_operations::route_archive_copy_into;
use crate::file_system::write_operations::types::{
    ConflictInfo, DryRunResult, ScanProgressEvent, WriteCancelledEvent, WriteCompleteEvent, WriteConflictEvent,
    WriteConflictResolvedEvent, WriteErrorEvent, WriteProgressEvent, WriteSettledEvent, WriteSourceItemDoneEvent,
};

/// Collects every event, and runs `edit` once, on the first progress tick.
struct EditOnFirstProgress {
    inner: CollectorEventSink,
    edit: Mutex<Option<Box<dyn FnOnce() + Send>>>,
}

impl EditOnFirstProgress {
    fn new(edit: impl FnOnce() + Send + 'static) -> Arc<Self> {
        Arc::new(Self {
            inner: CollectorEventSink::new(),
            edit: Mutex::new(Some(Box::new(edit))),
        })
    }
}

impl OperationEventSink for EditOnFirstProgress {
    fn emit_progress(&self, event: WriteProgressEvent) {
        if let Some(edit) = self.edit.lock_ignore_poison().take() {
            edit();
        }
        self.inner.emit_progress(event);
    }
    fn emit_complete(&self, event: WriteCompleteEvent) {
        self.inner.emit_complete(event);
    }
    fn emit_cancelled(&self, event: WriteCancelledEvent) {
        self.inner.emit_cancelled(event);
    }
    fn emit_error(&self, event: WriteErrorEvent) {
        self.inner.emit_error(event);
    }
    fn emit_conflict(&self, event: WriteConflictEvent) {
        self.inner.emit_conflict(event);
    }
    fn emit_conflict_resolved(&self, event: WriteConflictResolvedEvent) {
        self.inner.emit_conflict_resolved(event);
    }
    fn emit_source_item_done(&self, event: WriteSourceItemDoneEvent) {
        self.inner.emit_source_item_done(event);
    }
    fn emit_scan_progress(&self, event: ScanProgressEvent) {
        self.inner.emit_scan_progress(event);
    }
    fn emit_scan_conflict(&self, conflict: ConflictInfo) {
        self.inner.emit_scan_conflict(conflict);
    }
    fn emit_dry_run_complete(&self, result: DryRunResult) {
        self.inner.emit_dry_run_complete(result);
    }
    fn emit_settled(&self, event: WriteSettledEvent) {
        self.inner.emit_settled(event);
    }
}

/// Moves `source_rel` from `source_volume` into `archive` and waits for the
/// completion event.
async fn move_into(
    events: &Arc<EditOnFirstProgress>,
    source_volume: Arc<dyn Volume>,
    source_rel: &str,
    archive: &Path,
) -> WriteCompleteEvent {
    route_archive_copy_into(
        Arc::clone(events) as Arc<dyn OperationEventSink>,
        source_volume,
        vec![PathBuf::from(source_rel)],
        archive.to_path_buf(),
        unique_lane_id(),
        ConflictResolution::Overwrite,
        0,
        true,
        None,
        None,
    )
    .await
    .expect("start the move into the zip");
    wait_until_async(Duration::from_secs(5), "a terminal event", || {
        !events.inner.complete.lock_ignore_poison().is_empty() || !events.inner.errors.lock_ignore_poison().is_empty()
    })
    .await;
    assert!(
        events.edit.lock_ignore_poison().is_none(),
        "precondition: the edit ran before the originals went"
    );
    let complete = events.inner.complete.lock_ignore_poison();
    complete
        .first()
        .cloned()
        .unwrap_or_else(|| panic!("the move must complete: {:?}", events.inner.errors.lock_ignore_poison()))
}

/// CRITICAL data-loss regression (#139), remote source. A file saved over and a
/// file that appeared while the move ran exist only in the source, so both stay;
/// an untouched original still goes.
#[tokio::test]
async fn a_remote_source_keeps_what_changed_or_appeared_during_a_move_into_a_zip() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let archive = tmp.path().join("a.zip");
    write_multi_zip(&archive, &[("placeholder.txt", b"x")]);
    let (source_id, source) =
        register_remote_source(&[("d/draft.txt", b"first draft"), ("d/report.txt", b"report")]).await;
    let source_volume: Arc<dyn Volume> = get_volume_manager().get(&source_id).expect("source volume");

    let edited = Arc::clone(&source);
    let events = EditOnFirstProgress::new(move || {
        let draft = Path::new("/d/draft.txt");
        let modified = edited
            .get_metadata(draft)
            .now_or_never()
            .unwrap()
            .unwrap()
            .modified_at
            .unwrap();
        edited.delete(draft).now_or_never().unwrap().unwrap();
        edited
            .create_file(draft, b"final draft")
            .now_or_never()
            .unwrap()
            .unwrap();
        edited.set_modified_at(draft, Some(modified + 5));
        edited
            .create_file(Path::new("/d/download.zip"), b"arrived mid-move")
            .now_or_never()
            .unwrap()
            .unwrap();
    });

    let complete = move_into(&events, source_volume, "d", &archive).await;

    assert_eq!(
        read_entry(&archive, "d/draft.txt").as_deref(),
        Some(b"first draft".as_slice())
    );
    assert!(
        source.exists(Path::new("/d/draft.txt")).await,
        "the saved-over original stays"
    );
    assert!(source.exists(Path::new("/d/download.zip")).await, "the newcomer stays");
    assert!(
        !source.exists(Path::new("/d/report.txt")).await,
        "an untouched original still goes"
    );
    let left = complete.appeared_during_move.expect("what stayed is news");
    assert_eq!((left.item_count, left.changed_count), (1, 1));
    assert_eq!(left.folder_name, "d");

    get_volume_manager().unregister(&source_id);
}

/// The same for a local source, which the move used to remove with
/// `remove_dir_all`.
#[tokio::test]
async fn a_local_source_keeps_what_changed_or_appeared_during_a_move_into_a_zip() {
    use crate::file_system::volume::backends::LocalPosixVolume;

    let tmp = tempfile::tempdir().expect("tempdir");
    let archive = tmp.path().join("a.zip");
    write_multi_zip(&archive, &[("placeholder.txt", b"x")]);
    let src_root = tmp.path().join("src");
    std::fs::create_dir_all(src_root.join("d")).expect("mkdir");
    std::fs::write(src_root.join("d/draft.txt"), b"first draft").expect("seed");
    std::fs::write(src_root.join("d/report.txt"), b"report").expect("seed");

    let draft = src_root.join("d/draft.txt");
    let newcomer = src_root.join("d/download.zip");
    let (draft_in_sink, newcomer_in_sink) = (draft.clone(), newcomer.clone());
    let events = EditOnFirstProgress::new(move || {
        std::fs::write(&draft_in_sink, b"final draft, longer").expect("save over");
        std::fs::write(&newcomer_in_sink, b"arrived mid-move").expect("newcomer");
    });
    let source_volume: Arc<dyn Volume> = Arc::new(LocalPosixVolume::local_folder("src", src_root.clone()));

    let complete = move_into(&events, source_volume, "d", &archive).await;

    assert_eq!(std::fs::read(&draft).expect("kept"), b"final draft, longer");
    assert_eq!(std::fs::read(&newcomer).expect("kept"), b"arrived mid-move");
    assert!(
        !src_root.join("d/report.txt").exists(),
        "an untouched original still goes"
    );
    let left = complete.appeared_during_move.expect("what stayed is news");
    assert_eq!((left.item_count, left.changed_count), (1, 1));
}
