//! A cross-type Overwrite a person answered (a folder landing on a file, or a
//! file landing on a folder) sets the destination ASIDE rather than deleting it,
//! and the operation's ending decides what happens to it: dropped once the
//! replacement is in, put back (or kept beside it under a ` (recovered)` name)
//! when the replacement never fully landed.
//!
//! A `#[path]` child of `copy.rs`, so `super::` is `copy` and `super::super::` is
//! `volume`.

use super::super::{FaultyOp, FaultyVolume};
use super::tests::{make_state, make_volumes};
use super::*;
use crate::file_system::volume::InMemoryVolume;
use crate::file_system::write_operations::event_sinks::CollectorEventSink;
use crate::file_system::write_operations::state::ConflictResolutionResponse;
use crate::file_system::write_operations::types::{
    ConflictResolution, WriteConflictEvent, WriteConflictResolvedEvent, WriteErrorEvent, WriteOperationError,
    WriteSourceItemDoneEvent,
};

/// Answers every prompt with a plain `Overwrite`, and, when `rollback_after_first`
/// is set, flips the operation to `RollingBack` the moment one source has fully
/// landed, which is a Rollback clicked after the replacement was complete.
struct OverwriteThenMaybeRollback {
    inner: CollectorEventSink,
    state: Arc<WriteOperationState>,
    rollback_after_first: bool,
}

impl OverwriteThenMaybeRollback {
    fn new(state: &Arc<WriteOperationState>, rollback_after_first: bool) -> Arc<Self> {
        Arc::new(Self {
            inner: CollectorEventSink::new(),
            state: Arc::clone(state),
            rollback_after_first,
        })
    }
}

impl OperationEventSink for OverwriteThenMaybeRollback {
    fn emit_progress(&self, event: WriteProgressEvent) {
        if self.rollback_after_first && event.phase == WriteOperationPhase::Copying && event.files_done >= 1 {
            // RollingBack = 1
            self.state.intent.store(1, Ordering::Relaxed);
        }
        self.inner.emit_progress(event);
    }
    fn emit_complete(&self, e: WriteCompleteEvent) {
        self.inner.emit_complete(e);
    }
    fn emit_cancelled(&self, e: WriteCancelledEvent) {
        self.inner.emit_cancelled(e);
    }
    fn emit_error(&self, e: WriteErrorEvent) {
        self.inner.emit_error(e);
    }
    fn emit_conflict(&self, e: WriteConflictEvent) {
        let clash = e.conflict_id;
        self.inner.emit_conflict(e);
        let _ = self.state.conflict_slot.answer(
            clash,
            ConflictResolutionResponse {
                resolution: ConflictResolution::Overwrite,
                apply_to_all: false,
            },
        );
    }
    fn emit_conflict_resolved(&self, e: WriteConflictResolvedEvent) {
        self.inner.emit_conflict_resolved(e);
    }
    fn emit_source_item_done(&self, _e: WriteSourceItemDoneEvent) {}
    fn emit_scan_progress(&self, _e: crate::file_system::write_operations::types::ScanProgressEvent) {}
    fn emit_scan_conflict(&self, _c: crate::file_system::write_operations::types::ConflictInfo) {}
    fn emit_dry_run_complete(&self, _r: crate::file_system::write_operations::types::DryRunResult) {}
    fn emit_settled(&self, _e: crate::file_system::write_operations::types::WriteSettledEvent) {}
}

fn stop_config() -> VolumeCopyConfig {
    VolumeCopyConfig {
        // Only an answered prompt may cross types; a blanket Overwrite skips.
        conflict_resolution: ConflictResolution::Stop,
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    }
}

/// Every byte of the file at `path`.
async fn read_all(volume: &Arc<dyn Volume>, path: &str) -> Vec<u8> {
    let mut stream = volume.open_read_stream(Path::new(path)).await.unwrap();
    let mut bytes = Vec::new();
    while let Some(chunk) = stream.next_chunk().await {
        bytes.extend(chunk.unwrap());
    }
    bytes
}

/// The names at `dir` on `volume`, sorted.
async fn names_in(volume: &Arc<dyn Volume>, dir: &str) -> Vec<String> {
    let mut names: Vec<String> = volume
        .list_directory(Path::new(dir), None)
        .await
        .unwrap()
        .into_iter()
        .map(|e| e.name)
        .collect();
    names.sort();
    names
}

/// A concrete source, so a [`FaultyVolume`] can wrap it.
fn in_memory_source() -> Arc<InMemoryVolume> {
    Arc::new(InMemoryVolume::new("Source").with_space_info(10_000_000, 10_000_000))
}

/// A source whose FIRST file read refuses, so a folder source fails after its
/// directory is already standing at the destination. `IoError` without an errno
/// isn't retryable, so the leaf gives up on the first attempt.
fn source_failing_first_read(source: Arc<InMemoryVolume>) -> Arc<FaultyVolume<InMemoryVolume>> {
    FaultyVolume::wrapping(source)
        .failing_call(
            FaultyOp::OpenReadStream,
            1,
            VolumeError::IoError {
                message: "simulated source read failure".to_string(),
                raw_os_error: None,
            },
        )
        .arc()
}

/// A folder replacing the user's FILE fails halfway: the folder already holds the
/// name, so the file comes back beside it under a ` (recovered)` name.
///
/// Pre-fix the resolver deleted the file before the folder's first byte, and the
/// failure left neither the file nor a complete folder.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_folder_that_fails_halfway_over_a_file_keeps_the_file() {
    let (_, dest) = make_volumes();
    let source = in_memory_source();
    source.create_directory(Path::new("/clash")).await.unwrap();
    source
        .create_file(Path::new("/clash/a.txt"), b"incoming a")
        .await
        .unwrap();
    source
        .create_file(Path::new("/clash/b.txt"), b"incoming b")
        .await
        .unwrap();
    dest.create_file(Path::new("/clash"), b"the user's only copy")
        .await
        .unwrap();
    let faulty = source_failing_first_read(source);

    let state = make_state();
    let events = OverwriteThenMaybeRollback::new(&state, false);
    let result = copy_volumes_with_progress(
        events.clone(),
        "op-folder-over-file-fails",
        &state,
        faulty.clone(),
        &[PathBuf::from("/clash")],
        Arc::clone(&dest),
        Path::new("/"),
        &stop_config(),
    )
    .await;

    assert!(faulty.fault_fired(FaultyOp::OpenReadStream));
    let err = result.expect_err("the unreadable child fails the copy");
    assert!(
        matches!(err.error, WriteOperationError::OriginalsKeptAside { ref recovered, .. } if recovered.len() == 1),
        "the failure names where the user's file went: {:?}",
        err.error
    );
    assert_eq!(read_all(&dest, "/clash (recovered)").await, b"the user's only copy");
    assert_eq!(names_in(&dest, "/").await, vec!["clash", "clash (recovered)"]);
}

/// A file replacing the user's FOLDER fails before its bytes land: the name is
/// still free, so the folder goes straight back, children and all.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_file_that_fails_over_a_folder_puts_the_folder_back() {
    let (_, dest) = make_volumes();
    let source = in_memory_source();
    source.create_file(Path::new("/clash"), b"incoming").await.unwrap();
    dest.create_directory(Path::new("/clash")).await.unwrap();
    dest.create_file(Path::new("/clash/precious.txt"), b"precious user data")
        .await
        .unwrap();
    let faulty = source_failing_first_read(source);

    let state = make_state();
    let events = OverwriteThenMaybeRollback::new(&state, false);
    let result = copy_volumes_with_progress(
        events.clone(),
        "op-file-over-folder-fails",
        &state,
        faulty.clone(),
        &[PathBuf::from("/clash")],
        Arc::clone(&dest),
        Path::new("/"),
        &stop_config(),
    )
    .await;

    assert!(faulty.fault_fired(FaultyOp::OpenReadStream));
    assert!(result.is_err(), "the unreadable source fails the copy");
    assert_eq!(read_all(&dest, "/clash/precious.txt").await, b"precious user data");
    assert_eq!(names_in(&dest, "/").await, vec!["clash"], "and nothing is left aside");
}

/// The same shape one level down, inside a folder merge: a source SUBFOLDER
/// landing on a destination FILE of the same name.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_deep_folder_that_fails_over_a_file_keeps_the_file() {
    let (_, dest) = make_volumes();
    let source = in_memory_source();
    source.create_directory(Path::new("/album")).await.unwrap();
    source.create_directory(Path::new("/album/sub")).await.unwrap();
    source
        .create_file(Path::new("/album/sub/x.txt"), b"incoming")
        .await
        .unwrap();
    dest.create_directory(Path::new("/album")).await.unwrap();
    dest.create_file(Path::new("/album/sub"), b"the user's sub file")
        .await
        .unwrap();
    let faulty = source_failing_first_read(source);

    let state = make_state();
    let events = OverwriteThenMaybeRollback::new(&state, false);
    let result = copy_volumes_with_progress(
        events.clone(),
        "op-deep-folder-over-file-fails",
        &state,
        faulty.clone(),
        &[PathBuf::from("/album")],
        Arc::clone(&dest),
        Path::new("/"),
        &stop_config(),
    )
    .await;

    assert!(faulty.fault_fired(FaultyOp::OpenReadStream));
    assert!(result.is_err(), "the unreadable child fails the copy");
    assert_eq!(read_all(&dest, "/album/sub (recovered)").await, b"the user's sub file");
}

/// A Rollback clicked after the folder fully replaced the file removes the folder
/// and puts the file back at its own name.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_rollback_puts_the_replaced_file_back() {
    let (source, dest) = make_volumes();
    source.create_directory(Path::new("/clash")).await.unwrap();
    source
        .create_file(Path::new("/clash/inside.txt"), b"incoming")
        .await
        .unwrap();
    dest.create_file(Path::new("/clash"), b"the user's only copy")
        .await
        .unwrap();

    let state = make_state();
    let events = OverwriteThenMaybeRollback::new(&state, true);
    let _ = copy_volumes_with_progress(
        events.clone(),
        "op-folder-over-file-rollback",
        &state,
        Arc::clone(&source),
        &[PathBuf::from("/clash")],
        Arc::clone(&dest),
        Path::new("/"),
        &stop_config(),
    )
    .await;

    assert_eq!(
        events.inner.cancelled.lock_ignore_poison().len(),
        1,
        "the rollback really ran"
    );
    assert_eq!(read_all(&dest, "/clash").await, b"the user's only copy");
    assert_eq!(names_in(&dest, "/").await, vec!["clash"], "and nothing is left aside");
}

/// The guard: a replacement that lands drops the aside, whichever way round.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_replacement_that_lands_leaves_no_aside_behind() {
    let (source, dest) = make_volumes();
    source.create_directory(Path::new("/folder_in")).await.unwrap();
    source
        .create_file(Path::new("/folder_in/inside.txt"), b"incoming")
        .await
        .unwrap();
    source
        .create_file(Path::new("/file_in"), b"incoming file")
        .await
        .unwrap();
    dest.create_file(Path::new("/folder_in"), b"old file").await.unwrap();
    dest.create_directory(Path::new("/file_in")).await.unwrap();
    dest.create_file(Path::new("/file_in/old.txt"), b"old").await.unwrap();

    let state = make_state();
    let events = OverwriteThenMaybeRollback::new(&state, false);
    let result = copy_volumes_with_progress(
        events.clone(),
        "op-cross-type-lands",
        &state,
        Arc::clone(&source),
        &[PathBuf::from("/folder_in"), PathBuf::from("/file_in")],
        Arc::clone(&dest),
        Path::new("/"),
        &stop_config(),
    )
    .await;

    assert!(result.is_ok(), "both replacements land: {result:?}");
    assert_eq!(names_in(&dest, "/").await, vec!["file_in", "folder_in"]);
    assert_eq!(read_all(&dest, "/folder_in/inside.txt").await, b"incoming");
    assert_eq!(read_all(&dest, "/file_in").await, b"incoming file");
}

/// A LINK at the destination is a leaf, never a folder, whatever it points at:
/// a folder answered Overwrite over it sets the LINK aside, and dropping that
/// aside after the folder lands removes the link alone. The folder it pointed at
/// keeps every file.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_folder_replacing_a_dir_link_drops_the_link_and_never_its_target() {
    use crate::file_system::volume::LocalPosixVolume;

    let dest_dir = tempfile::tempdir().expect("tempdir");
    let root = dest_dir.path().to_path_buf();
    std::fs::create_dir(root.join("target")).unwrap();
    std::fs::write(root.join("target/precious.txt"), b"the user's data").unwrap();
    std::os::unix::fs::symlink(root.join("target"), root.join("clash")).unwrap();
    let dest: Arc<dyn Volume> = Arc::new(LocalPosixVolume::local_folder("Dest", root.clone()));

    let source = in_memory_source();
    source.create_directory(Path::new("/clash")).await.unwrap();
    source
        .create_file(Path::new("/clash/inside.txt"), b"incoming")
        .await
        .unwrap();

    let state = make_state();
    let events = OverwriteThenMaybeRollback::new(&state, false);
    copy_volumes_with_progress(
        events.clone(),
        "op-folder-over-dir-link",
        &state,
        source,
        &[PathBuf::from("/clash")],
        Arc::clone(&dest),
        Path::new("/"),
        &stop_config(),
    )
    .await
    .expect("the folder lands");

    let clash = std::fs::symlink_metadata(root.join("clash")).unwrap();
    assert!(clash.is_dir(), "a real folder now holds the name, not the link");
    assert_eq!(std::fs::read(root.join("clash/inside.txt")).unwrap(), b"incoming");
    assert_eq!(
        std::fs::read(root.join("target/precious.txt")).unwrap(),
        b"the user's data",
        "the link's target is untouched"
    );
    let mut names: Vec<String> = std::fs::read_dir(&root)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .collect();
    names.sort();
    assert_eq!(names, vec!["clash", "target"], "and the link's aside is gone");
}
