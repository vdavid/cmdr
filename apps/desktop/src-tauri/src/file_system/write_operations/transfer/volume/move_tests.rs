//! Unit tests for `move_volumes_with_progress`, the cross-volume move
//! (copy + delete source): the happy path, the conflict×resolution matrix,
//! pre-known-conflict bulk skip, and destination auto-create.
//!
//! These drive the sink-based inner function directly with a
//! `CollectorEventSink` + `InMemoryVolume`, mirroring `volume/copy_tests.rs`.
//! Tests target the data-safety invariants the bulk-skip / per-iter skip
//! work introduced: pre-known-conflict bulk skip lands before any destructive
//! call, and skipped conflicts bump `files_done` so the bar doesn't stall.
//!
//! The rest of the move suite: `volume/move_same_tests.rs` (same-volume
//! rename), `volume/move_cancel_tests.rs`, `volume/move_failure_tests.rs`,
//! `volume/move_progress_tests.rs`, and `volume/move_merge_tests.rs`. Shared
//! fixtures and doubles live in `volume/move_test_support.rs`
//! (`super::test_support`).

use super::super::super::conflict_responder_test_support::await_prompted_clash;
use super::test_support::{make_state, make_state_with_interval_ms, make_volumes};
use super::*;
use crate::file_system::volume::LocalPosixVolume;
use crate::file_system::write_operations::state::ConflictResolutionResponse;
use crate::file_system::write_operations::types::{CollectorEventSink, ConflictResolution};
use crate::test_support::TestDir;

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

/// Conflict + Rename (matrix cell): the original dest is kept untouched, the
/// incoming source lands under `name (1)`, and the source is deleted (it moved).
/// Closes the cross-volume Rename cell of the move conflict×resolution matrix.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cross_volume_move_conflict_rename_keeps_dest_and_renames_incoming() {
    let (source, dest) = make_volumes();
    source.create_file(Path::new("/notes.txt"), b"incoming").await.unwrap();
    dest.create_file(Path::new("/notes.txt"), b"existing").await.unwrap();

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state();
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Rename,
        ..VolumeCopyConfig::default()
    };

    let result = move_volumes_with_progress(
        events.clone(),
        "op-move-rename",
        &state,
        Arc::clone(&source),
        &[PathBuf::from("/notes.txt")],
        Arc::clone(&dest),
        Path::new("/"),
        &config,
    )
    .await;

    assert!(result.is_ok(), "expected Ok, got {:?}", result);

    // Original dest kept untouched.
    let mut keep = dest.open_read_stream(Path::new("/notes.txt")).await.unwrap();
    assert_eq!(keep.next_chunk().await.unwrap().unwrap(), b"existing");

    // Incoming landed under the renamed name.
    let mut renamed = dest.open_read_stream(Path::new("/notes (1).txt")).await.unwrap();
    assert_eq!(renamed.next_chunk().await.unwrap().unwrap(), b"incoming");

    // Source moved => deleted.
    assert!(
        !source.exists(Path::new("/notes.txt")).await,
        "source must be deleted after Rename"
    );
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
    let events_for_resolver = Arc::clone(&events);
    let resolver = tokio::spawn(async move {
        let clash = await_prompted_clash(&events_for_resolver).await;
        let _ = state_for_resolver.conflict_slot.answer(
            clash,
            ConflictResolutionResponse {
                resolution: ConflictResolution::Skip,
                apply_to_all: true,
            },
        );
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

/// Real-FS smoke: drive the cross-volume move against `LocalPosixVolume`. The
/// in-memory tests cover the logic; this catches divergence on the
/// `LocalPosixVolume`-specific paths (`local_path` short-circuit lives one
/// level up in `move_between_volumes`, so calling the inner directly with two
/// `LocalPosixVolume`s still exercises the streaming copy+delete shape).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cross_volume_move_on_real_local_volumes() {
    use std::fs;
    let base = TestDir::new("move_real_fs");
    let src_dir = base.join("src");
    let dst_dir = base.join("dst");
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
}

/// Cross-volume move into a not-yet-existing nested dest creates the folder
/// (and ancestors) on the dest volume, then lands the files.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cross_volume_move_creates_missing_nested_dest() {
    let (source, dest) = make_volumes();
    source.create_file(Path::new("/a.txt"), b"alpha").await.unwrap();
    source.create_file(Path::new("/b.txt"), b"bravo").await.unwrap();
    assert!(!dest.exists(Path::new("/incoming")).await);

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state();
    let config = VolumeCopyConfig::default();

    let result = move_volumes_with_progress(
        events.clone(),
        "op-move-mkdir",
        &state,
        Arc::clone(&source),
        &[PathBuf::from("/a.txt"), PathBuf::from("/b.txt")],
        Arc::clone(&dest),
        Path::new("/incoming/2026/trip"),
        &config,
    )
    .await;

    assert!(
        result.is_ok(),
        "move into a missing nested dest should succeed: {:?}",
        result
    );

    for dir in ["/incoming", "/incoming/2026", "/incoming/2026/trip"] {
        assert!(
            dest.is_directory(Path::new(dir)).await.expect("ancestor statable"),
            "{dir} should be a directory"
        );
    }
    // Sources gone, files landed in the freshly-created dest.
    assert!(!source.exists(Path::new("/a.txt")).await);
    assert!(!source.exists(Path::new("/b.txt")).await);
    assert!(dest.exists(Path::new("/incoming/2026/trip/a.txt")).await);
    assert!(dest.exists(Path::new("/incoming/2026/trip/b.txt")).await);
}
