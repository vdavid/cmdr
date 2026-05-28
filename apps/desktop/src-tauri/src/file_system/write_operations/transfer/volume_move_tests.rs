//! Unit tests for `move_volumes_with_progress` (cross-volume) and
//! `move_within_same_volume_with_progress` (same-volume rename).
//!
//! These tests drive the sink-based inner functions directly with a
//! `CollectorEventSink` + `InMemoryVolume`, mirroring `volume_copy_tests.rs`.
//! Tests target the data-safety invariants the bulk-skip / per-iter skip
//! work introduced: pre-known-conflict bulk skip lands before any destructive
//! call, skipped conflicts bump `files_done` so the bar doesn't stall, and
//! cancel between sources stops further transfers.

use super::*;
use crate::file_system::volume::{InMemoryVolume, LocalPosixVolume};
use crate::file_system::write_operations::state::ConflictResolutionResponse;
use crate::file_system::write_operations::types::{
    CollectorEventSink, WriteCancelledEvent, WriteConflictEvent, WriteProgressEvent, WriteSourceItemDoneEvent,
};
use std::sync::atomic::{AtomicU8, Ordering};

fn make_state() -> Arc<WriteOperationState> {
    Arc::new(WriteOperationState::new(Duration::from_millis(50)))
}

/// State whose `progress_interval` mirrors what the production wrapper does:
/// derived from the config. Without this, tests that set
/// `config.progress_interval_ms = 0` would still see the default 50 ms throttle
/// (state ignores the config it didn't construct from).
fn make_state_with_interval_ms(ms: u64) -> Arc<WriteOperationState> {
    Arc::new(WriteOperationState::new(Duration::from_millis(ms)))
}

fn make_volumes() -> (Arc<dyn Volume>, Arc<dyn Volume>) {
    (
        Arc::new(InMemoryVolume::new("Source").with_space_info(10_000_000, 10_000_000)),
        Arc::new(InMemoryVolume::new("Dest").with_space_info(10_000_000, 10_000_000)),
    )
}

// ============================================================================
// move_volumes_with_progress — cross-volume copy+delete
// ============================================================================

/// Happy path: every source lands at dest and is gone from source. Completion
/// event reports the right totals.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cross_volume_move_happy_path() {
    let (source, dest) = make_volumes();

    source.create_file(Path::new("/a.txt"), b"alpha").await.unwrap();
    source.create_file(Path::new("/b.txt"), b"bravo").await.unwrap();
    source.create_directory(Path::new("/dir")).await.unwrap();
    source.create_file(Path::new("/dir/c.txt"), b"charlie").await.unwrap();

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state();
    let config = VolumeCopyConfig::default();

    let result = move_volumes_with_progress(
        events.clone(),
        "op-move-happy",
        &state,
        Arc::clone(&source),
        &[PathBuf::from("/a.txt"), PathBuf::from("/b.txt"), PathBuf::from("/dir")],
        Arc::clone(&dest),
        Path::new("/"),
        &config,
    )
    .await;

    assert!(result.is_ok(), "expected Ok, got {:?}", result);

    // Sources are gone, dest has the data.
    assert!(!source.exists(Path::new("/a.txt")).await);
    assert!(!source.exists(Path::new("/b.txt")).await);
    assert!(!source.exists(Path::new("/dir")).await);
    let mut a = dest.open_read_stream(Path::new("/a.txt")).await.unwrap();
    assert_eq!(a.next_chunk().await.unwrap().unwrap(), b"alpha");
    let mut b = dest.open_read_stream(Path::new("/b.txt")).await.unwrap();
    assert_eq!(b.next_chunk().await.unwrap().unwrap(), b"bravo");
    let mut c = dest.open_read_stream(Path::new("/dir/c.txt")).await.unwrap();
    assert_eq!(c.next_chunk().await.unwrap().unwrap(), b"charlie");

    // Completion event with 3 top-level sources processed.
    let complete = events.complete.lock().unwrap();
    assert_eq!(complete.len(), 1);
    assert_eq!(complete[0].files_processed, 3);
}

/// Conflict + Skip: dest keeps its old content, source is preserved (skip never
/// deletes source). Per-iter skip accounting bumps `files_done` so the bar
/// advances through the skip.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cross_volume_move_conflict_skip_preserves_source_and_dest() {
    let (source, dest) = make_volumes();

    source.create_file(Path::new("/keep.txt"), b"new").await.unwrap();
    source.create_file(Path::new("/fresh.txt"), b"fresh").await.unwrap();
    dest.create_file(Path::new("/keep.txt"), b"old").await.unwrap();

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state_with_interval_ms(0);
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Skip,
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };

    let result = move_volumes_with_progress(
        events.clone(),
        "op-move-skip",
        &state,
        Arc::clone(&source),
        &[PathBuf::from("/keep.txt"), PathBuf::from("/fresh.txt")],
        Arc::clone(&dest),
        Path::new("/"),
        &config,
    )
    .await;

    assert!(result.is_ok(), "expected Ok, got {:?}", result);

    // Skipped: dest keeps "old", source still has "new" (skip must not delete source).
    let mut dest_stream = dest.open_read_stream(Path::new("/keep.txt")).await.unwrap();
    assert_eq!(dest_stream.next_chunk().await.unwrap().unwrap(), b"old");
    assert!(
        source.exists(Path::new("/keep.txt")).await,
        "source must remain when conflict was skipped"
    );

    // Fresh source: moved through.
    let mut fresh = dest.open_read_stream(Path::new("/fresh.txt")).await.unwrap();
    assert_eq!(fresh.next_chunk().await.unwrap().unwrap(), b"fresh");
    assert!(!source.exists(Path::new("/fresh.txt")).await);

    // files_processed: both (1 skipped + 1 moved).
    let complete = events.complete.lock().unwrap();
    assert_eq!(complete[0].files_processed, 2);

    // Skip must have produced a progress event with files_done > 0 before the
    // copy completed. Otherwise the bar would stall through skipped conflicts.
    let progress = events.progress.lock().unwrap();
    let max_files_done = progress.iter().map(|p| p.files_done).max().unwrap_or(0);
    assert!(
        max_files_done >= 1,
        "expected at least one progress event bumping files_done; saw max {max_files_done}",
    );
}

/// Conflict + Overwrite: dest replaced with source content; source removed.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cross_volume_move_conflict_overwrite_replaces_dest_and_deletes_source() {
    let (source, dest) = make_volumes();
    source.create_file(Path::new("/f.txt"), b"new").await.unwrap();
    dest.create_file(Path::new("/f.txt"), b"old").await.unwrap();

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state();
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Overwrite,
        ..VolumeCopyConfig::default()
    };

    let result = move_volumes_with_progress(
        events.clone(),
        "op-move-overwrite",
        &state,
        Arc::clone(&source),
        &[PathBuf::from("/f.txt")],
        Arc::clone(&dest),
        Path::new("/"),
        &config,
    )
    .await;

    assert!(result.is_ok(), "expected Ok, got {:?}", result);

    let mut dest_stream = dest.open_read_stream(Path::new("/f.txt")).await.unwrap();
    assert_eq!(dest_stream.next_chunk().await.unwrap().unwrap(), b"new");
    assert!(!source.exists(Path::new("/f.txt")).await, "source must be deleted");
}

/// Stop mode emits `write-conflict` and waits on the oneshot. Drive a Skip-all
/// resolution from the test side to verify the chosen path applies.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cross_volume_move_conflict_stop_resolves_via_oneshot() {
    let (source, dest) = make_volumes();
    source.create_file(Path::new("/x.txt"), b"new").await.unwrap();
    dest.create_file(Path::new("/x.txt"), b"old").await.unwrap();

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state();
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Stop,
        ..VolumeCopyConfig::default()
    };

    // Race the resolver: wait until the inner installs a oneshot sender, then push Skip-all.
    let state_for_resolver = Arc::clone(&state);
    let resolver = tokio::spawn(async move {
        for _ in 0..200 {
            if let Some(tx) = state_for_resolver.conflict_resolution_tx.lock().unwrap().take() {
                let _ = tx.send(ConflictResolutionResponse {
                    resolution: ConflictResolution::Skip,
                    apply_to_all: true,
                });
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("conflict_resolution_tx was never installed");
    });

    let result = move_volumes_with_progress(
        events.clone(),
        "op-move-stop",
        &state,
        Arc::clone(&source),
        &[PathBuf::from("/x.txt")],
        Arc::clone(&dest),
        Path::new("/"),
        &config,
    )
    .await;
    resolver.await.unwrap();
    assert!(result.is_ok(), "expected Ok, got {:?}", result);

    // write-conflict was emitted (Stop's user-facing prompt).
    assert_eq!(events.conflicts.lock().unwrap().len(), 1);

    // Skip resolved: dest keeps old, source untouched.
    let mut dest_stream = dest.open_read_stream(Path::new("/x.txt")).await.unwrap();
    assert_eq!(dest_stream.next_chunk().await.unwrap().unwrap(), b"old");
    assert!(source.exists(Path::new("/x.txt")).await);
}

/// Pre-known-conflicts bulk-skip: the first non-zero progress event accounts
/// the full bulk-skipped set in one jump. The destructive copy/delete must NOT
/// have run for those sources (dest keeps old content, source still has new).
/// This pins the data-safety invariant the bulk-skip work introduced.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cross_volume_move_pre_known_conflicts_bulk_skip() {
    let (source, dest) = make_volumes();

    // 5 sources: a, c, e are pre-known conflicts; b, d are fresh.
    source.create_file(Path::new("/a.txt"), b"AA").await.unwrap();
    source.create_file(Path::new("/b.txt"), b"BBBB").await.unwrap();
    source.create_file(Path::new("/c.txt"), b"CCCCCC").await.unwrap();
    source.create_file(Path::new("/d.txt"), b"DDDDDDDD").await.unwrap();
    source.create_file(Path::new("/e.txt"), b"EEEEEEEEEE").await.unwrap();

    dest.create_file(Path::new("/a.txt"), b"old-a").await.unwrap();
    dest.create_file(Path::new("/c.txt"), b"old-c").await.unwrap();
    dest.create_file(Path::new("/e.txt"), b"old-e").await.unwrap();

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state_with_interval_ms(0);
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Skip,
        progress_interval_ms: 0,
        pre_known_conflicts: vec!["a.txt".to_string(), "c.txt".to_string(), "e.txt".to_string()],
        ..VolumeCopyConfig::default()
    };

    let result = move_volumes_with_progress(
        events.clone(),
        "op-move-bulk-skip",
        &state,
        Arc::clone(&source),
        &[
            PathBuf::from("/a.txt"),
            PathBuf::from("/b.txt"),
            PathBuf::from("/c.txt"),
            PathBuf::from("/d.txt"),
            PathBuf::from("/e.txt"),
        ],
        Arc::clone(&dest),
        Path::new("/"),
        &config,
    )
    .await;

    assert!(result.is_ok(), "expected Ok, got {:?}", result);

    // Critical: pre-known conflicts had their dest content preserved. Sources
    // still on source (skip never deletes source).
    for name in ["a.txt", "c.txt", "e.txt"] {
        let path = format!("/{}", name);
        let mut s = dest.open_read_stream(Path::new(&path)).await.unwrap();
        let chunk = s.next_chunk().await.unwrap().unwrap();
        assert!(
            chunk.starts_with(b"old-"),
            "dest {} must retain old content, got {:?}",
            name,
            chunk
        );
        assert!(
            source.exists(Path::new(&path)).await,
            "source {} must remain (skip is non-destructive)",
            name
        );
    }

    // Non-conflict sources moved through (data at dest, gone from source).
    assert!(!source.exists(Path::new("/b.txt")).await);
    assert!(!source.exists(Path::new("/d.txt")).await);

    // The first non-zero Copying progress event must bump files_done to 3 in
    // one shot (bulk-skip emit), not trickle one-per-conflict. Filter to
    // Copying phase to skip Scanning-phase tallies.
    let progress = events.progress.lock().unwrap();
    let first_nonzero = progress
        .iter()
        .find(|p| p.phase == WriteOperationPhase::Copying && p.files_done > 0)
        .expect("expected a Copying progress event with files_done > 0");
    assert_eq!(
        first_nonzero.files_done, 3,
        "bulk-skip must account 3 conflicts in one event, saw {first_nonzero:?}",
    );

    // Completion event accounts all 5 sources.
    let complete = events.complete.lock().unwrap();
    assert_eq!(complete[0].files_processed, 5);
}

/// Top-level **directory** whose name matches a pre-known conflict must NOT
/// land in the bulk-skip set: bulk-skip drops the whole subtree in a single
/// counter bump (it's only correct when the top-level source is a FILE, in
/// which case dropping == leaving the dest copy intact). Directories must
/// fall through to per-iter conflict resolution instead, so the downstream
/// resolver decides what to do with them.
///
/// This pins the bulk-skip prelude's file-only contract (data-correctness
/// invariant the Playwright `Copy with Skip All preserves destination
/// files` spec broke before the fix). The per-iter resolver's behavior for
/// dir-vs-dir under Skip is a separate concern; this test only verifies
/// the bulk-skip exclusion.
///
/// Setup:
/// - source: `/file.txt` (file conflict), `/docs` (dir whose name also appears in
///   pre_known_conflicts because the FE's top-level conflict scan reports name collisions
///   regardless of type).
/// - dest: `/file.txt`, `/docs/guide.txt`.
/// - `pre_known_conflicts: ["file.txt", "docs"]`, `resolution = Skip`.
///
/// Expected:
/// - `file.txt` bulk-skips: the source still has it, dest still has `old-file`, the source side is
///   preserved (Skip is non-destructive).
/// - `docs` does NOT bulk-skip. We pin this by inspecting the FIRST non-zero progress event: with
///   the bug, it would account both `file.txt` AND `docs` (files_done = 2). With the fix, only
///   `file.txt` (files_done = 1).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cross_volume_move_top_level_directory_excluded_from_bulk_skip() {
    let (source, dest) = make_volumes();

    source.create_file(Path::new("/file.txt"), b"new-file").await.unwrap();
    source.create_directory(Path::new("/docs")).await.unwrap();
    source
        .create_file(Path::new("/docs/guide.txt"), b"new-guide")
        .await
        .unwrap();

    dest.create_file(Path::new("/file.txt"), b"old-file").await.unwrap();
    dest.create_directory(Path::new("/docs")).await.unwrap();
    dest.create_file(Path::new("/docs/guide.txt"), b"old-guide")
        .await
        .unwrap();

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state_with_interval_ms(0);
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Skip,
        progress_interval_ms: 0,
        pre_known_conflicts: vec!["file.txt".to_string(), "docs".to_string()],
        ..VolumeCopyConfig::default()
    };

    let result = move_volumes_with_progress(
        events.clone(),
        "op-move-bulk-skip-dir-excluded",
        &state,
        Arc::clone(&source),
        &[PathBuf::from("/file.txt"), PathBuf::from("/docs")],
        Arc::clone(&dest),
        Path::new("/"),
        &config,
    )
    .await;

    assert!(result.is_ok(), "expected Ok, got {:?}", result);

    // Skip is non-destructive: dest content preserved on both sides, source
    // files survive too.
    let mut dest_file = dest.open_read_stream(Path::new("/file.txt")).await.unwrap();
    assert_eq!(dest_file.next_chunk().await.unwrap().unwrap(), b"old-file");
    assert!(source.exists(Path::new("/file.txt")).await);

    // Pin the bulk-skip exclusion: the first non-zero Copying progress event
    // must account exactly ONE source (`file.txt`), not two. If `docs` were
    // bulk-skipped, this event would jump to `files_done = 2`. Filter to
    // Copying phase to skip Scanning-phase tallies.
    let progress = events.progress.lock().unwrap();
    let first_nonzero = progress
        .iter()
        .find(|p| p.phase == WriteOperationPhase::Copying && p.files_done > 0)
        .expect("expected a Copying progress event with files_done > 0");
    assert_eq!(
        first_nonzero.files_done, 1,
        "bulk-skip must account only the FILE conflict, not the directory; saw {first_nonzero:?}",
    );
}

/// Cancellation between sources stops further transfers and emits `write-cancelled`.
/// This was a latent bug pre-M1-step-4: the cancel path returned `Err(Cancelled)`
/// but never emitted the event, leaving the FE dialog open. Fixed inline.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cross_volume_move_cancel_between_sources_emits_cancelled_event() {
    let (source, dest) = make_volumes();
    source.create_file(Path::new("/a.txt"), b"alpha").await.unwrap();
    source.create_file(Path::new("/b.txt"), b"bravo").await.unwrap();
    source.create_file(Path::new("/c.txt"), b"charlie").await.unwrap();

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state();
    // Pre-cancel before the loop runs: every iteration sees the cancel at the
    // top check. Uses the existing convention in `volume_copy_tests.rs` of a
    // direct `intent.store` for the simulated cancel; the public-path
    // `cancel_write_operation` requires the state to be in the cache, which
    // the outer wrapper (not under test here) is responsible for.
    state.intent.store(2, Ordering::Relaxed); // Stopped

    let result = move_volumes_with_progress(
        events.clone(),
        "op-move-cancel",
        &state,
        Arc::clone(&source),
        &[
            PathBuf::from("/a.txt"),
            PathBuf::from("/b.txt"),
            PathBuf::from("/c.txt"),
        ],
        Arc::clone(&dest),
        Path::new("/"),
        &config_default(),
    )
    .await;

    assert!(matches!(
        result,
        Err(WriteFailure {
            error: WriteOperationError::Cancelled { .. },
            ..
        })
    ));

    // Nothing transferred (cancel before any iteration).
    assert!(source.exists(Path::new("/a.txt")).await);
    assert!(source.exists(Path::new("/b.txt")).await);
    assert!(source.exists(Path::new("/c.txt")).await);
    assert!(!dest.exists(Path::new("/a.txt")).await);

    // The critical assertion: write-cancelled was emitted. Pre-fix this would
    // be empty.
    let cancelled = events.cancelled.lock().unwrap();
    assert_eq!(cancelled.len(), 1, "expected exactly one write-cancelled event");
    assert!(!cancelled[0].rolled_back, "move has no rollback");
    assert_eq!(cancelled[0].operation_type, WriteOperationType::Move);
}

/// Cancel mid-batch (after some sources moved): completed transfers stay at
/// dest, source is deleted for those — no data loss, no half-state for the
/// in-progress source.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cross_volume_move_cancel_mid_batch_preserves_completed() {
    /// Sink that flips intent to Stopped after one successful file moves.
    /// Watches `emit_progress` events with `files_done >= 1`.
    struct CancelAfterFirstSink {
        inner: CollectorEventSink,
        intent: Arc<AtomicU8>,
    }
    impl OperationEventSink for CancelAfterFirstSink {
        fn emit_progress(&self, event: WriteProgressEvent) {
            if event.phase == WriteOperationPhase::Copying && event.files_done >= 1 {
                self.intent.store(2, Ordering::Relaxed); // Stopped
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

    let (source, dest) = make_volumes();
    for name in ["a", "b", "c", "d", "e"] {
        source
            .create_file(Path::new(&format!("/{}.txt", name)), name.as_bytes())
            .await
            .unwrap();
    }

    let state = make_state_with_interval_ms(0);
    let events = Arc::new(CancelAfterFirstSink {
        inner: CollectorEventSink::new(),
        intent: Arc::clone(&state.intent),
    });
    let config = VolumeCopyConfig {
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };

    let result = move_volumes_with_progress(
        events.clone(),
        "op-move-cancel-mid",
        &state,
        Arc::clone(&source),
        &[
            PathBuf::from("/a.txt"),
            PathBuf::from("/b.txt"),
            PathBuf::from("/c.txt"),
            PathBuf::from("/d.txt"),
            PathBuf::from("/e.txt"),
        ],
        Arc::clone(&dest),
        Path::new("/"),
        &config,
    )
    .await;

    assert!(matches!(
        result,
        Err(WriteFailure {
            error: WriteOperationError::Cancelled { .. },
            ..
        })
    ));

    // At least one moved, but not all five.
    let mut moved = 0;
    for name in ["a", "b", "c", "d", "e"] {
        if dest.exists(Path::new(&format!("/{}.txt", name))).await {
            moved += 1;
        }
    }
    assert!((1..5).contains(&moved), "expected partial move (1..5), got {moved}");

    // For each moved file, source must be gone (no half-move where both sides
    // hold the data). For each NOT-moved file, source still has it (we'd lose
    // data otherwise).
    for name in ["a", "b", "c", "d", "e"] {
        let p = format!("/{}.txt", name);
        let at_dest = dest.exists(Path::new(&p)).await;
        let at_source = source.exists(Path::new(&p)).await;
        // Exactly one location has it; never both, never neither.
        assert!(
            at_dest != at_source,
            "{p}: at_dest={at_dest} at_source={at_source} (data loss or duplication)",
        );
    }

    // Cancel event emitted.
    let cancelled = events.inner.cancelled.lock().unwrap();
    assert_eq!(cancelled.len(), 1);
}

/// Real-FS smoke: drive the cross-volume move against `LocalPosixVolume`. The
/// in-memory tests cover the logic; this catches divergence on the
/// `LocalPosixVolume`-specific paths (`local_path` short-circuit lives one
/// level up in `move_between_volumes`, so calling the inner directly with two
/// `LocalPosixVolume`s still exercises the streaming copy+delete shape).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cross_volume_move_on_real_local_volumes() {
    use std::fs;
    let base = std::env::temp_dir().join("cmdr_move_real_fs");
    let src_dir = base.join("src");
    let dst_dir = base.join("dst");
    let _ = fs::remove_dir_all(&base);
    fs::create_dir_all(&src_dir).unwrap();
    fs::create_dir_all(&dst_dir).unwrap();

    fs::write(src_dir.join("doc.txt"), "hello").unwrap();
    fs::write(src_dir.join("note.txt"), "world").unwrap();

    let source: Arc<dyn Volume> = Arc::new(LocalPosixVolume::new("Source", src_dir.to_str().unwrap()));
    let dest: Arc<dyn Volume> = Arc::new(LocalPosixVolume::new("Dest", dst_dir.to_str().unwrap()));

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state();
    let result = move_volumes_with_progress(
        events.clone(),
        "op-move-real-fs",
        &state,
        Arc::clone(&source),
        &[PathBuf::from("doc.txt"), PathBuf::from("note.txt")],
        Arc::clone(&dest),
        Path::new(""),
        &VolumeCopyConfig::default(),
    )
    .await;

    assert!(result.is_ok(), "expected Ok, got {:?}", result);

    assert!(!src_dir.join("doc.txt").exists());
    assert!(!src_dir.join("note.txt").exists());
    assert_eq!(fs::read_to_string(dst_dir.join("doc.txt")).unwrap(), "hello");
    assert_eq!(fs::read_to_string(dst_dir.join("note.txt")).unwrap(), "world");

    let _ = fs::remove_dir_all(&base);
}

fn config_default() -> VolumeCopyConfig {
    VolumeCopyConfig::default()
}

// ============================================================================
// move_within_same_volume_with_progress — same-volume rename
// ============================================================================

/// Happy-path same-volume rename: files end up at their new paths via `Volume::rename`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn same_volume_move_happy_path() {
    let volume: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("V").with_space_info(10_000_000, 10_000_000));
    volume.create_file(Path::new("/a.txt"), b"alpha").await.unwrap();
    volume.create_file(Path::new("/b.txt"), b"bravo").await.unwrap();
    volume.create_directory(Path::new("/dst")).await.unwrap();

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state();

    let result = move_within_same_volume_with_progress(
        events.clone(),
        "op-same-move-happy",
        &state,
        Arc::clone(&volume),
        &[PathBuf::from("/a.txt"), PathBuf::from("/b.txt")],
        Path::new("/dst"),
        &VolumeCopyConfig::default(),
    )
    .await;

    assert!(result.is_ok(), "expected Ok, got {:?}", result);

    // Files were renamed.
    assert!(!volume.exists(Path::new("/a.txt")).await);
    assert!(!volume.exists(Path::new("/b.txt")).await);
    let mut a = volume.open_read_stream(Path::new("/dst/a.txt")).await.unwrap();
    assert_eq!(a.next_chunk().await.unwrap().unwrap(), b"alpha");
    let mut b = volume.open_read_stream(Path::new("/dst/b.txt")).await.unwrap();
    assert_eq!(b.next_chunk().await.unwrap().unwrap(), b"bravo");

    let complete = events.complete.lock().unwrap();
    assert_eq!(complete[0].files_processed, 2);
}

/// Skip mode preserves the existing dest entry and leaves the source untouched.
/// Per-iter skip accounting bumps `files_moved` so the bar shows progress.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn same_volume_move_conflict_skip_preserves_both() {
    let volume: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("V").with_space_info(10_000_000, 10_000_000));
    volume.create_file(Path::new("/src/a.txt"), b"new").await.unwrap();
    volume.create_file(Path::new("/dst/a.txt"), b"old").await.unwrap();

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state_with_interval_ms(0);
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Skip,
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };

    let result = move_within_same_volume_with_progress(
        events.clone(),
        "op-same-move-skip",
        &state,
        Arc::clone(&volume),
        &[PathBuf::from("/src/a.txt")],
        Path::new("/dst"),
        &config,
    )
    .await;

    assert!(result.is_ok(), "expected Ok, got {:?}", result);

    // Source still there; dest still "old" (skip didn't overwrite).
    assert!(volume.exists(Path::new("/src/a.txt")).await);
    let mut s = volume.open_read_stream(Path::new("/dst/a.txt")).await.unwrap();
    assert_eq!(s.next_chunk().await.unwrap().unwrap(), b"old");

    // files_processed counts the skip.
    let complete = events.complete.lock().unwrap();
    assert_eq!(complete[0].files_processed, 1);
}

/// Overwrite via the rename path: the existing dest entry is replaced by the
/// renamed source. (InMemoryVolume's rename respects `force=false`; the
/// resolver routes Overwrite through `apply_volume_conflict_resolution` which
/// deletes the dest before rename.)
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn same_volume_move_conflict_overwrite_replaces_dest() {
    let volume: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("V").with_space_info(10_000_000, 10_000_000));
    volume.create_file(Path::new("/src/a.txt"), b"new").await.unwrap();
    volume.create_file(Path::new("/dst/a.txt"), b"old").await.unwrap();

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state();
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Overwrite,
        ..VolumeCopyConfig::default()
    };

    let result = move_within_same_volume_with_progress(
        events.clone(),
        "op-same-move-overwrite",
        &state,
        Arc::clone(&volume),
        &[PathBuf::from("/src/a.txt")],
        Path::new("/dst"),
        &config,
    )
    .await;

    assert!(result.is_ok(), "expected Ok, got {:?}", result);
    assert!(!volume.exists(Path::new("/src/a.txt")).await);
    let mut s = volume.open_read_stream(Path::new("/dst/a.txt")).await.unwrap();
    assert_eq!(s.next_chunk().await.unwrap().unwrap(), b"new");
}

/// Pre-cancel same-volume move: nothing renamed, `write-cancelled` emitted.
/// Pins the same latent-bug fix as the cross-volume variant.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn same_volume_move_cancel_emits_cancelled_event() {
    let volume: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("V").with_space_info(10_000_000, 10_000_000));
    volume.create_file(Path::new("/a.txt"), b"alpha").await.unwrap();
    volume.create_file(Path::new("/b.txt"), b"bravo").await.unwrap();
    volume.create_directory(Path::new("/dst")).await.unwrap();

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state();
    state.intent.store(2, Ordering::Relaxed); // Stopped

    let result = move_within_same_volume_with_progress(
        events.clone(),
        "op-same-move-cancel",
        &state,
        Arc::clone(&volume),
        &[PathBuf::from("/a.txt"), PathBuf::from("/b.txt")],
        Path::new("/dst"),
        &VolumeCopyConfig::default(),
    )
    .await;

    assert!(matches!(result, Err(WriteOperationError::Cancelled { .. })));

    // Nothing renamed.
    assert!(volume.exists(Path::new("/a.txt")).await);
    assert!(volume.exists(Path::new("/b.txt")).await);
    assert!(!volume.exists(Path::new("/dst/a.txt")).await);

    let cancelled = events.cancelled.lock().unwrap();
    assert_eq!(cancelled.len(), 1, "expected exactly one write-cancelled event");
    assert!(!cancelled[0].rolled_back);
}

/// Pre-known conflicts under Skip-all bulk-skip upfront for same-volume rename.
/// The rename closure must NOT have been called for the bulk-skipped sources;
/// any data they had at dest must still be there.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn same_volume_move_pre_known_conflicts_bulk_skip() {
    let volume: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("V").with_space_info(10_000_000, 10_000_000));
    // Sources in /src/, dests under /dst/. a, c are pre-known conflicts; b is fresh.
    for name in ["a.txt", "b.txt", "c.txt"] {
        volume
            .create_file(Path::new(&format!("/src/{}", name)), b"new")
            .await
            .unwrap();
    }
    for name in ["a.txt", "c.txt"] {
        volume
            .create_file(Path::new(&format!("/dst/{}", name)), b"old")
            .await
            .unwrap();
    }

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state_with_interval_ms(0);
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Skip,
        progress_interval_ms: 0,
        pre_known_conflicts: vec!["a.txt".to_string(), "c.txt".to_string()],
        ..VolumeCopyConfig::default()
    };

    let result = move_within_same_volume_with_progress(
        events.clone(),
        "op-same-move-bulk-skip",
        &state,
        Arc::clone(&volume),
        &[
            PathBuf::from("/src/a.txt"),
            PathBuf::from("/src/b.txt"),
            PathBuf::from("/src/c.txt"),
        ],
        Path::new("/dst"),
        &config,
    )
    .await;

    assert!(result.is_ok(), "expected Ok, got {:?}", result);

    // Bulk-skipped: dest still "old", source still in /src/.
    for name in ["a.txt", "c.txt"] {
        let mut s = volume
            .open_read_stream(Path::new(&format!("/dst/{}", name)))
            .await
            .unwrap();
        assert_eq!(s.next_chunk().await.unwrap().unwrap(), b"old");
        assert!(volume.exists(Path::new(&format!("/src/{}", name))).await);
    }
    // Fresh moved.
    assert!(!volume.exists(Path::new("/src/b.txt")).await);
    let mut b = volume.open_read_stream(Path::new("/dst/b.txt")).await.unwrap();
    assert_eq!(b.next_chunk().await.unwrap().unwrap(), b"new");

    // First non-zero Copying event must account both bulk-skipped conflicts at
    // once. Filter to Copying phase to skip Scanning-phase tallies.
    let progress = events.progress.lock().unwrap();
    let first_nonzero = progress
        .iter()
        .find(|p| p.phase == WriteOperationPhase::Copying && p.files_done > 0)
        .expect("expected a Copying progress event with files_done > 0");
    assert_eq!(
        first_nonzero.files_done, 2,
        "bulk-skip must account 2 conflicts in one event",
    );
}

// ============================================================================
// Regression: bytes_total flows through move progress events
// ============================================================================

/// Cross-volume move emits `bytes_total > 0` on every Copying-phase progress
/// event. Without this, the FE's `TransferProgressDialog` hides the Size
/// progress bar (the dialog gates it behind `{#if bytesTotal > 0}`), so the
/// user only saw the Files bar during MTP→local moves. The shared preflight
/// scan now feeds the real total into the driver and every per-source emit.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cross_volume_move_emits_bytes_total_on_progress() {
    let (source, dest) = make_volumes();
    source.create_file(Path::new("/a.txt"), b"alpha").await.unwrap();
    source.create_file(Path::new("/b.txt"), b"bravo-bravo").await.unwrap();
    let expected_total = (b"alpha".len() + b"bravo-bravo".len()) as u64;

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state_with_interval_ms(0);
    let config = VolumeCopyConfig {
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };

    let result = move_volumes_with_progress(
        events.clone(),
        "op-move-bytes-total",
        &state,
        Arc::clone(&source),
        &[PathBuf::from("/a.txt"), PathBuf::from("/b.txt")],
        Arc::clone(&dest),
        Path::new("/"),
        &config,
    )
    .await;
    assert!(result.is_ok(), "expected Ok, got {:?}", result);

    let progress = events.progress.lock().unwrap();
    let copying: Vec<_> = progress
        .iter()
        .filter(|p| p.phase == WriteOperationPhase::Copying)
        .collect();
    assert!(!copying.is_empty(), "expected at least one Copying progress event");
    for ev in &copying {
        assert_eq!(
            ev.bytes_total, expected_total,
            "every Copying progress event must carry the real bytes_total (got {} for files_done={})",
            ev.bytes_total, ev.files_done,
        );
    }

    let complete = events.complete.lock().unwrap();
    assert_eq!(complete[0].bytes_processed, expected_total);
}

/// Same-volume rename emits `bytes_total > 0` on every Copying-phase progress
/// event. Even though rename transfers no bytes, the FE shows the Size bar
/// tracking alongside the Files bar so the two stay visually paired.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn same_volume_move_emits_bytes_total_on_progress() {
    let volume: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("V").with_space_info(10_000_000, 10_000_000));
    volume.create_file(Path::new("/a.txt"), b"alpha").await.unwrap();
    volume.create_file(Path::new("/b.txt"), b"bravo-bravo").await.unwrap();
    volume.create_directory(Path::new("/dst")).await.unwrap();
    let expected_total = (b"alpha".len() + b"bravo-bravo".len()) as u64;

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state_with_interval_ms(0);
    let config = VolumeCopyConfig {
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };

    let result = move_within_same_volume_with_progress(
        events.clone(),
        "op-same-move-bytes-total",
        &state,
        Arc::clone(&volume),
        &[PathBuf::from("/a.txt"), PathBuf::from("/b.txt")],
        Path::new("/dst"),
        &config,
    )
    .await;
    assert!(result.is_ok(), "expected Ok, got {:?}", result);

    let progress = events.progress.lock().unwrap();
    let copying: Vec<_> = progress
        .iter()
        .filter(|p| p.phase == WriteOperationPhase::Copying)
        .collect();
    assert!(!copying.is_empty(), "expected at least one Copying progress event");
    for ev in &copying {
        assert_eq!(
            ev.bytes_total, expected_total,
            "every Copying progress event must carry the real bytes_total (got {} for files_done={})",
            ev.bytes_total, ev.files_done,
        );
    }

    let complete = events.complete.lock().unwrap();
    assert_eq!(complete[0].bytes_processed, expected_total);
}

/// Cross-volume move (no `preview_id`) emits multiple `Scanning`-phase
/// progress events with climbing tallies as `scan_for_copy_batch_with_progress`
/// walks the source list, not just one frozen event at `0/0/0/0`. Without the
/// per-listing progress wiring, programmatic / MCP-triggered moves against a
/// slow source (cold MTP, large SMB tree) sit on "Scanning... 0 bytes / 0
/// files / 0 dirs" for the entire scan duration.
///
/// `InMemoryVolume` inherits the default `scan_for_copy_batch_with_progress`,
/// which fires `on_progress` once per top-level path. With 4 sources we
/// expect the kickoff emit plus at least one mid-scan event showing a partial
/// tally before the scan finishes.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cross_volume_move_emits_scan_phase_tallies_during_walk() {
    let (source, dest) = make_volumes();
    let payload = vec![0u8; 4096];
    for i in 0..4 {
        source
            .create_file(Path::new(&format!("/a_{}.bin", i)), &payload)
            .await
            .unwrap();
    }
    let total_bytes = (payload.len() * 4) as u64;

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state_with_interval_ms(0);
    let config = VolumeCopyConfig {
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };

    let sources: Vec<PathBuf> = (0..4).map(|i| PathBuf::from(format!("/a_{}.bin", i))).collect();
    let result = move_volumes_with_progress(
        events.clone(),
        "op-move-scan-tally",
        &state,
        Arc::clone(&source),
        &sources,
        Arc::clone(&dest),
        Path::new("/"),
        &config,
    )
    .await;
    assert!(result.is_ok(), "expected Ok, got {:?}", result);

    let progress = events.progress.lock().unwrap();
    let scanning: Vec<_> = progress
        .iter()
        .filter(|p| p.phase == WriteOperationPhase::Scanning)
        .collect();

    // Pre-fix: exactly one Scanning event (the kickoff emit at 0/0/0/0).
    // Post-fix: kickoff + one event per scanned top-level path.
    assert!(
        scanning.len() >= 2,
        "expected multiple Scanning events during a 4-source walk, got {} ({:?})",
        scanning.len(),
        scanning
            .iter()
            .map(|e| (e.files_done, e.bytes_done))
            .collect::<Vec<_>>(),
    );
    for w in scanning.windows(2) {
        assert!(
            w[0].bytes_done <= w[1].bytes_done,
            "scan bytes_done must be non-decreasing across Scanning events, got {} then {}",
            w[0].bytes_done,
            w[1].bytes_done,
        );
    }
    let last = scanning.last().expect("at least one Scanning event");
    assert_eq!(
        last.files_done, 4,
        "final Scanning event should tally all 4 source files"
    );
    assert_eq!(
        last.bytes_done, total_bytes,
        "final Scanning event should tally all source bytes"
    );
}

/// Cross-volume move of a single large file emits multiple `Copying`-phase
/// progress events as chunks stream through, not just one event after the
/// whole file lands. Without intra-file progress the FE's "Moving..." dialog
/// shows `0 bytes / 0 files / 0 dirs` for the entire upload — bug observed
/// against an SMB destination with a 3.7 GB file.
///
/// `InMemoryVolume` streams in 64 KB chunks (see
/// `volume/backends/in_memory.rs::CHUNK_SIZE`), so 1 MB ≈ 16 callback
/// invocations; with `progress_interval_ms: 0` the throttle is open and
/// every chunk emits. The `>= 3` floor is well above "one tail emit per
/// source" (current buggy floor: 1) and well below the ~16 events the
/// fix produces, so it's robust against scheduler jitter on busy CI.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cross_volume_move_emits_intra_file_progress() {
    let (source, dest) = make_volumes();
    let payload: Vec<u8> = vec![0u8; 1_048_576];
    source.create_file(Path::new("/big.bin"), &payload).await.unwrap();

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state_with_interval_ms(0);
    let config = VolumeCopyConfig {
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };

    let result = move_volumes_with_progress(
        events.clone(),
        "op-move-intra-file",
        &state,
        Arc::clone(&source),
        &[PathBuf::from("/big.bin")],
        Arc::clone(&dest),
        Path::new("/"),
        &config,
    )
    .await;
    assert!(result.is_ok(), "expected Ok, got {:?}", result);

    let progress = events.progress.lock().unwrap();
    let copying: Vec<_> = progress
        .iter()
        .filter(|p| p.phase == WriteOperationPhase::Copying)
        .collect();

    // Pre-fix: exactly one event (the post-source throttled emit at the end).
    // Post-fix: ~16 intra-file events plus the post-source tail emit.
    assert!(
        copying.len() >= 3,
        "expected multiple Copying events to stream during a 1 MB move, got {} ({:?})",
        copying.len(),
        copying.iter().map(|e| (e.files_done, e.bytes_done)).collect::<Vec<_>>(),
    );

    // bytes_done is non-decreasing as the stream advances.
    for w in copying.windows(2) {
        assert!(
            w[0].bytes_done <= w[1].bytes_done,
            "bytes_done must be non-decreasing across Copying events, got {} then {}",
            w[0].bytes_done,
            w[1].bytes_done,
        );
    }

    // Final Copying event accounts for the whole transfer.
    let last = copying.last().expect("at least one Copying event");
    assert_eq!(last.bytes_done, payload.len() as u64);
    assert_eq!(last.files_done, 1);
}
