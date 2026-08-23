//! Unit tests for `move_within_same_volume_with_progress`, the same-volume
//! rename path: the happy path, the conflict matrix, pre-known-conflict bulk
//! skip, and destination auto-create.
//!
//! A same-volume move renames rather than copies, so the promises differ from
//! the cross-volume ones in `volume/move_tests.rs`: a Skip leaves both sides
//! exactly where they were, and Overwrite routes through
//! `apply_volume_conflict_resolution` because `InMemoryVolume::rename` honors
//! `force = false`.
//!
//! Cancel lives in `volume/move_cancel_tests.rs`, byte tallies in
//! `volume/move_progress_tests.rs`, and folder merges in
//! `volume/move_merge_tests.rs`. Shared fixtures live in
//! `volume/move_test_support.rs` (`super::test_support`).

use super::super::move_same::move_within_same_volume_with_progress;
use super::test_support::{make_state, make_state_with_interval_ms};
use super::*;
use crate::file_system::volume::InMemoryVolume;
use crate::file_system::write_operations::event_sinks::CollectorEventSink;
use crate::file_system::write_operations::types::ConflictResolution;

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

/// Same-volume rename into a not-yet-existing nested dest creates the dest
/// directory first, so the rename lands. The server-side-rename fast path is
/// preserved (a create into an existing dest is a no-op).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn same_volume_move_creates_missing_nested_dest() {
    let volume: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("V").with_space_info(10_000_000, 10_000_000));
    volume.create_file(Path::new("/a.txt"), b"alpha").await.unwrap();
    assert!(!volume.exists(Path::new("/archive")).await);

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state();

    let result = move_within_same_volume_with_progress(
        events.clone(),
        "op-same-move-mkdir",
        &state,
        Arc::clone(&volume),
        &[PathBuf::from("/a.txt")],
        Path::new("/archive/2026"),
        &VolumeCopyConfig::default(),
    )
    .await;

    assert!(
        result.is_ok(),
        "same-volume move into a missing nested dest should succeed: {:?}",
        result
    );

    for dir in ["/archive", "/archive/2026"] {
        assert!(
            volume.is_directory(Path::new(dir)).await.expect("ancestor statable"),
            "{dir} should be a directory"
        );
    }
    assert!(!volume.exists(Path::new("/a.txt")).await, "source renamed away");
    let mut a = volume.open_read_stream(Path::new("/archive/2026/a.txt")).await.unwrap();
    assert_eq!(a.next_chunk().await.unwrap().unwrap(), b"alpha");
}
