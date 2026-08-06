//! Rollback / dest-inside-source / cancel-mid-stream tests for `volume_copy`,
//! split out of `volume_copy_tests.rs`. These cover user-initiated Rollback at
//! various points (after the first file, mid-merge-stream, after a finalize
//! rename), the rejection of copying a directory into its own descendant, and
//! cancel-mid-stream data preservation. The `RollbackAfterFirstFileSink`,
//! `TripIntentOnFirstByteSink`, and `TripIntentAtFilesDoneSink` doubles trip the
//! operation's intent at the precise moment each scenario needs.
//!
//! Shared fixtures `make_state` / `make_volumes` live in `volume_copy_tests.rs`
//! (`super::tests`) so they aren't duplicated.

use super::tests::{make_state, make_volumes};
use super::*;
use crate::file_system::volume::InMemoryVolume;
use crate::file_system::write_operations::types::{
    CollectorEventSink, ConflictResolution, WriteConflictEvent, WriteErrorEvent, WriteSourceItemDoneEvent,
};
use std::sync::atomic::AtomicU8;

/// Sink that flips the operation's intent to `RollingBack` once it sees a
/// `Copying`-phase progress event reporting at least one fully-copied file.
/// For a single directory source, `files_done` only reaches 1 after the whole
/// directory has been copied (the post-source milestone), so this fires the
/// user-initiated Rollback AFTER the merge completed — the finding's scenario.
struct RollbackAfterFirstFileSink {
    inner: CollectorEventSink,
    intent: Arc<AtomicU8>,
}

impl OperationEventSink for RollbackAfterFirstFileSink {
    fn emit_settled(&self, e: crate::file_system::write_operations::types::WriteSettledEvent) {
        self.inner.emit_settled(e);
    }
    fn emit_progress(&self, event: WriteProgressEvent) {
        if event.phase == WriteOperationPhase::Copying && event.files_done >= 1 {
            // RollingBack = 1
            self.intent.store(1, Ordering::Relaxed);
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
        self.inner.emit_conflict(e);
    }
    fn emit_source_item_done(&self, _e: WriteSourceItemDoneEvent) {}
    fn emit_scan_progress(&self, _e: crate::file_system::write_operations::types::ScanProgressEvent) {}
    fn emit_scan_conflict(&self, _c: crate::file_system::write_operations::types::ConflictInfo) {}
    fn emit_dry_run_complete(&self, _r: crate::file_system::write_operations::types::DryRunResult) {}
}

/// Rollback of a directory source that MERGED into a pre-existing destination
/// directory must delete only the files this operation wrote — never dest-only
/// files that pre-existed the copy.
///
/// Regression for the cross-volume rollback bug: a directory source recorded the
/// top-level dest directory in `copied_paths`, so Rollback recursively deleted
/// the whole merged tree, including a sentinel file the operation never touched.
/// "Overwrite means merge for dirs," so dest-only files legitimately coexist in a
/// merged directory — and Rollback (the advertised safe undo) was destroying them.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rollback_of_merged_directory_preserves_preexisting_dest_files() {
    let (source, dest) = make_volumes();

    // Source directory with two new files the op will write.
    source.create_directory(Path::new("/album")).await.unwrap();
    source
        .create_file(Path::new("/album/new1.bin"), &vec![0u8; 200_000])
        .await
        .unwrap();
    source
        .create_file(Path::new("/album/new2.bin"), &vec![0u8; 200_000])
        .await
        .unwrap();

    // Pre-existing dest directory of the same name, holding a unique sentinel
    // file that the operation must never touch.
    dest.create_directory(Path::new("/album")).await.unwrap();
    dest.create_file(Path::new("/album/sentinel.txt"), b"precious user data")
        .await
        .unwrap();

    let state = make_state();
    let events = Arc::new(RollbackAfterFirstFileSink {
        inner: CollectorEventSink::new(),
        intent: Arc::clone(&state.intent),
    });
    // Overwrite ⇒ dir-vs-dir merges into the existing dest directory.
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Overwrite,
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };

    let result = copy_volumes_with_progress(
        events.clone(),
        "test-op-rollback-merge",
        &state,
        Arc::clone(&source),
        &[PathBuf::from("/album")],
        Arc::clone(&dest),
        Path::new("/"),
        &config,
    )
    .await;

    // The operation ended via Rollback (cancellation-shaped result).
    assert!(
        result.is_err(),
        "expected a cancelled/rolled-back result, got {:?}",
        result
    );

    // THE BUG: the pre-existing sentinel must still be on the destination after
    // rollback. Rollback may delete only what the op created (new1/new2), never
    // the dest-only sentinel.
    assert!(
        dest.exists(Path::new("/album/sentinel.txt")).await,
        "rollback wrongly deleted a pre-existing dest-only file in the merged directory",
    );

    // And the files the op actually wrote should be gone (rollback removed them).
    assert!(
        !dest.exists(Path::new("/album/new1.bin")).await,
        "rollback should have removed the file the op created",
    );
    assert!(
        !dest.exists(Path::new("/album/new2.bin")).await,
        "rollback should have removed the file the op created",
    );
}

/// Copying a directory into its own descendant on the SAME volume must be
/// rejected up front with `DestinationInsideSource`, not recurse unboundedly.
///
/// Regression for the missing volume-path guard: the local copy path rejects
/// this via `validate_destination_not_inside_source`, but the cross-volume /
/// same-volume path had no equivalent. `copy_directory_streaming` re-lists each
/// subdir live, so copying folder `A` into `A/sub` on one share/device would
/// re-discover and re-copy the files it just wrote, growing the tree until the
/// volume fills.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn same_volume_copy_into_own_descendant_is_rejected() {
    // One volume used as BOTH source and dest (same Arc ⇒ Arc::ptr_eq is true,
    // matching how the command layer resolves a same-volume-id copy).
    let vol: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("V").with_space_info(10_000_000, 10_000_000));
    vol.create_directory(Path::new("/A")).await.unwrap();
    vol.create_directory(Path::new("/A/sub")).await.unwrap();
    vol.create_file(Path::new("/A/file.txt"), b"payload").await.unwrap();

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state();
    let config = VolumeCopyConfig::default();

    // Copy `/A` INTO `/A/sub` (its own descendant). Dest dir is `/A/sub`, so the
    // copied item lands at `/A/sub/A` — inside the source subtree.
    let result = copy_volumes_with_progress(
        events.clone(),
        "test-op-dest-inside-source",
        &state,
        Arc::clone(&vol),
        &[PathBuf::from("/A")],
        Arc::clone(&vol),
        Path::new("/A/sub"),
        &config,
    )
    .await;

    assert!(
        matches!(
            result,
            Err(WriteFailure {
                error: WriteOperationError::DestinationInsideSource { .. },
                ..
            })
        ),
        "expected DestinationInsideSource, got {:?}",
        result
    );
}

/// The dest-inside-source guard must NOT over-fire: a same-volume copy of a
/// directory into a SIBLING (not a descendant) is legitimate and must proceed.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn same_volume_copy_into_sibling_dir_is_allowed() {
    let vol: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("V").with_space_info(10_000_000, 10_000_000));
    vol.create_directory(Path::new("/A")).await.unwrap();
    vol.create_file(Path::new("/A/file.txt"), b"payload").await.unwrap();
    vol.create_directory(Path::new("/dest")).await.unwrap();

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state();
    let config = VolumeCopyConfig::default();

    // Copy `/A` into `/dest` (a sibling, not inside `/A`). Lands at `/dest/A`.
    let result = copy_volumes_with_progress(
        events.clone(),
        "test-op-sibling-dest",
        &state,
        Arc::clone(&vol),
        &[PathBuf::from("/A")],
        Arc::clone(&vol),
        Path::new("/dest"),
        &config,
    )
    .await;

    assert!(result.is_ok(), "sibling-dir copy should succeed: {:?}", result);
    assert!(vol.exists(Path::new("/dest/A/file.txt")).await);
}

/// A test sink that flips the operation intent to a chosen terminal state the
/// moment the copy first reports forward byte progress (a `Copying` event with
/// `bytes_done > 0`). For a directory source that's an INTRA-stream interruption:
/// the trip lands while `copy_single_path` is still mid-directory-stream, so it
/// returns `Err(Cancelled)` and leaves `last_dest_path` pointing at the dest
/// directory ROOT — the exact mid-merge cell. Parameterized so the same setup
/// exercises both Cancel (`Stopped`) and Rollback (`RollingBack`).
struct TripIntentOnFirstByteSink {
    inner: CollectorEventSink,
    intent: Arc<AtomicU8>,
    /// `OperationIntent` discriminant to store: `1` = RollingBack, `2` = Stopped.
    target_intent: u8,
}

impl OperationEventSink for TripIntentOnFirstByteSink {
    fn emit_settled(&self, e: crate::file_system::write_operations::types::WriteSettledEvent) {
        self.inner.emit_settled(e);
    }
    fn emit_progress(&self, event: WriteProgressEvent) {
        if event.phase == WriteOperationPhase::Copying && event.bytes_done > 0 {
            self.intent.store(self.target_intent, Ordering::Relaxed);
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
        self.inner.emit_conflict(e);
    }
    fn emit_source_item_done(&self, _e: WriteSourceItemDoneEvent) {}
    fn emit_scan_progress(&self, _e: crate::file_system::write_operations::types::ScanProgressEvent) {}
    fn emit_scan_conflict(&self, _c: crate::file_system::write_operations::types::ConflictInfo) {}
    fn emit_dry_run_complete(&self, _r: crate::file_system::write_operations::types::DryRunResult) {}
}

/// Builds a source dir with `n` files plus a pre-populated dest dir of the SAME
/// name holding a unique sentinel the operation must never touch. Returns the
/// `(source, dest)` volume pair. Used by the mid-stream merge interruption tests.
async fn make_merge_scenario(n: usize) -> (Arc<dyn Volume>, Arc<dyn Volume>) {
    let (source, dest) = make_volumes();

    source.create_directory(Path::new("/album")).await.unwrap();
    for i in 0..n {
        source
            .create_file(Path::new(&format!("/album/new{}.bin", i)), &vec![0u8; 200_000])
            .await
            .unwrap();
    }

    // Pre-existing dest directory of the same name with a sentinel file.
    dest.create_directory(Path::new("/album")).await.unwrap();
    dest.create_file(Path::new("/album/sentinel.txt"), b"precious user data")
        .await
        .unwrap();

    (source, dest)
}

/// CANCEL (keep-partials, `Stopped`) during a directory MERGE that is still
/// mid-copy must preserve a pre-existing dest-only sentinel.
///
/// The HIGH-A fix made the COMPLETED merged-dir Rollback safe by recording
/// per-file destinations. But a directory source interrupted MID-STREAM took the
/// `Err`/cancel arm, which discarded the per-task `CreatedPaths` ledger and fell
/// back to recursively deleting `last_dest_path` — the top-level dest DIRECTORY
/// root. On a merge that recursive delete destroyed the user's untouched
/// sentinel. This is the sibling cell flagged during the round-4 fix.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancel_mid_merge_stream_preserves_preexisting_dest_file() {
    let (source, dest) = make_merge_scenario(3).await;

    let state = make_state();
    let events = Arc::new(TripIntentOnFirstByteSink {
        inner: CollectorEventSink::new(),
        intent: Arc::clone(&state.intent),
        target_intent: 2, // Stopped
    });
    // Overwrite ⇒ dir-vs-dir merges into the existing dest directory.
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Overwrite,
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };

    let result = copy_volumes_with_progress(
        events.clone(),
        "test-op-cancel-mid-merge",
        &state,
        Arc::clone(&source),
        &[PathBuf::from("/album")],
        Arc::clone(&dest),
        Path::new("/"),
        &config,
    )
    .await;

    assert!(result.is_err(), "expected a cancelled result, got {:?}", result);

    // THE BUG: cancel mid-merge-stream must not destroy the pre-existing sentinel.
    assert!(
        dest.exists(Path::new("/album/sentinel.txt")).await,
        "cancel mid-merge-stream wrongly deleted a pre-existing dest-only file",
    );
}

/// ROLLBACK (`RollingBack`) during a directory MERGE that is still mid-copy must
/// preserve a pre-existing dest-only sentinel. Same sibling cell as the cancel
/// case above, but on the rollback arm (which pushes `last_dest_path` /
/// `in_flight_partials` into `copied_paths` and deletes them recursively).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rollback_mid_merge_stream_preserves_preexisting_dest_file() {
    let (source, dest) = make_merge_scenario(3).await;

    let state = make_state();
    let events = Arc::new(TripIntentOnFirstByteSink {
        inner: CollectorEventSink::new(),
        intent: Arc::clone(&state.intent),
        target_intent: 1, // RollingBack
    });
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Overwrite,
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };

    let result = copy_volumes_with_progress(
        events.clone(),
        "test-op-rollback-mid-merge",
        &state,
        Arc::clone(&source),
        &[PathBuf::from("/album")],
        Arc::clone(&dest),
        Path::new("/"),
        &config,
    )
    .await;

    assert!(result.is_err(), "expected a rolled-back result, got {:?}", result);

    // THE BUG: rollback mid-merge-stream must not destroy the pre-existing sentinel.
    assert!(
        dest.exists(Path::new("/album/sentinel.txt")).await,
        "rollback mid-merge-stream wrongly deleted a pre-existing dest-only file",
    );
}

/// CONCURRENT-path sibling of the mid-merge-stream cell. With ≥3 sources
/// (InMemory `max_concurrent_ops` = 32 ⇒ concurrent path), a directory source
/// that's interrupted mid-stream must still preserve a pre-existing dest-only
/// sentinel in its merged directory. The concurrent task's `Err` arm previously
/// returned the dest dir ROOT for recursive cleanup, discarding the per-file
/// ledger; the fix threads the ledger out so cleanup/rollback is per-file.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancel_mid_merge_stream_concurrent_preserves_preexisting_dest_file() {
    let (source, dest) = make_volumes();

    // Directory source that merges into a pre-existing dest dir with a sentinel.
    source.create_directory(Path::new("/album")).await.unwrap();
    for i in 0..3 {
        source
            .create_file(Path::new(&format!("/album/new{}.bin", i)), &vec![0u8; 200_000])
            .await
            .unwrap();
    }
    dest.create_directory(Path::new("/album")).await.unwrap();
    dest.create_file(Path::new("/album/sentinel.txt"), b"precious user data")
        .await
        .unwrap();

    // Two sibling files so the batch has ≥3 sources → concurrent path.
    source
        .create_file(Path::new("/sib1.bin"), &vec![0u8; 200_000])
        .await
        .unwrap();
    source
        .create_file(Path::new("/sib2.bin"), &vec![0u8; 200_000])
        .await
        .unwrap();

    let state = make_state();
    let events = Arc::new(TripIntentOnFirstByteSink {
        inner: CollectorEventSink::new(),
        intent: Arc::clone(&state.intent),
        target_intent: 2, // Stopped
    });
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Overwrite,
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };

    let result = copy_volumes_with_progress(
        events.clone(),
        "test-op-cancel-mid-merge-concurrent",
        &state,
        Arc::clone(&source),
        &[
            PathBuf::from("/album"),
            PathBuf::from("/sib1.bin"),
            PathBuf::from("/sib2.bin"),
        ],
        Arc::clone(&dest),
        Path::new("/"),
        &config,
    )
    .await;

    assert!(result.is_err(), "expected a cancelled result, got {:?}", result);

    // THE BUG: the pre-existing sentinel must survive a mid-stream cancel on the
    // concurrent path too.
    assert!(
        dest.exists(Path::new("/album/sentinel.txt")).await,
        "concurrent cancel mid-merge-stream wrongly deleted a pre-existing dest-only file",
    );
}

/// Sink that flips intent to a chosen terminal state once the file counter
/// reaches `trip_at_files_done` on a `Copying` event. Lets a test land N files
/// first, THEN trigger Cancel/Rollback, so the post-loop bookkeeping operates on
/// a known set of completed files.
struct TripIntentAtFilesDoneSink {
    inner: CollectorEventSink,
    intent: Arc<AtomicU8>,
    trip_at_files_done: usize,
    target_intent: u8,
}

impl OperationEventSink for TripIntentAtFilesDoneSink {
    fn emit_settled(&self, e: crate::file_system::write_operations::types::WriteSettledEvent) {
        self.inner.emit_settled(e);
    }
    fn emit_progress(&self, event: WriteProgressEvent) {
        if event.phase == WriteOperationPhase::Copying && event.files_done >= self.trip_at_files_done {
            self.intent.store(self.target_intent, Ordering::Relaxed);
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
        self.inner.emit_conflict(e);
    }
    fn emit_source_item_done(&self, _e: WriteSourceItemDoneEvent) {}
    fn emit_scan_progress(&self, _e: crate::file_system::write_operations::types::ScanProgressEvent) {}
    fn emit_scan_conflict(&self, _c: crate::file_system::write_operations::types::ConflictInfo) {}
    fn emit_dry_run_complete(&self, _r: crate::file_system::write_operations::types::DryRunResult) {}
}

/// ROLLBACK-invariant, file→file RENAME cell (volume copy): a source file
/// collides with a pre-existing dest file and is resolved as Rename, so the op
/// lands `name (1)`. Rollback must remove only `name (1)` (what the op created)
/// and leave the original dest file untouched.
///
/// Pins that the volume rollback ledger records the Rename-landed path, not the
/// pre-existing dest file — the rollback-invariant ("remove a dest path iff this
/// op created it") for the Rename resolution.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rollback_after_rename_keeps_preexisting_dest_file() {
    let (source, dest) = make_volumes();

    // A non-conflicting source so rollback has a clean first file to land,
    // letting us trip Rollback after BOTH sources have been processed.
    source
        .create_file(Path::new("/fresh.bin"), b"fresh data")
        .await
        .unwrap();
    // The Rename source: collides with a pre-existing dest file of the same name.
    source.create_file(Path::new("/doc.txt"), b"incoming").await.unwrap();
    dest.create_file(Path::new("/doc.txt"), b"original dest").await.unwrap();

    let state = make_state();
    // Trip Rollback after 2 files done (both sources landed), so the rollback
    // pass deletes what the op created in reverse.
    let events = Arc::new(TripIntentAtFilesDoneSink {
        inner: CollectorEventSink::new(),
        intent: Arc::clone(&state.intent),
        trip_at_files_done: 2,
        target_intent: 1, // RollingBack
    });
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Rename,
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };

    let result = copy_volumes_with_progress(
        events.clone(),
        "test-op-rollback-rename",
        &state,
        Arc::clone(&source),
        &[PathBuf::from("/fresh.bin"), PathBuf::from("/doc.txt")],
        Arc::clone(&dest),
        Path::new("/"),
        &config,
    )
    .await;

    assert!(result.is_err(), "expected a rolled-back result, got {:?}", result);

    // The pre-existing dest file the op never created must survive rollback.
    assert!(
        dest.exists(Path::new("/doc.txt")).await,
        "rollback wrongly deleted the pre-existing dest file the Rename never replaced",
    );
    let mut stream = dest.open_read_stream(Path::new("/doc.txt")).await.unwrap();
    assert_eq!(
        stream.next_chunk().await.unwrap().unwrap(),
        b"original dest",
        "the pre-existing dest content must be intact after rollback",
    );
    // The Rename-landed copy the op DID create must be gone after rollback.
    assert!(
        !dest.exists(Path::new("/doc (1).txt")).await,
        "rollback should have removed the Rename-landed file the op created",
    );
    // The fresh non-conflicting file the op created must be gone after rollback.
    assert!(
        !dest.exists(Path::new("/fresh.bin")).await,
        "rollback should have removed the fresh file the op created",
    );
}
