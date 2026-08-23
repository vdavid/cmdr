//! Cancelling a move, on both paths: nothing half-transferred survives, and the
//! `write-cancelled` event always reaches the frontend.
//!
//! The cancel path used to return `Err(Cancelled)` without emitting the event,
//! which left the FE dialog open forever. Each test here pins one shape: cancel
//! before any source is touched (cross-volume and same-volume), and cancel
//! mid-batch, where the already-moved files must be at the destination and gone
//! from the source, with nothing sitting on both sides or neither.
//!
//! Shared fixtures and the `CancelAfterFirstSink` double live in
//! `volume/move_test_support.rs` (`super::test_support`).

use super::super::move_same::move_within_same_volume_with_progress;
use super::test_support::{
    CancelAfterFirstSink, config_default, make_state, make_state_with_interval_ms, make_volumes,
};
use super::*;
use crate::file_system::volume::InMemoryVolume;
use crate::file_system::write_operations::event_sinks::CollectorEventSink;

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
    // top check. Uses the existing convention in `volume/copy_tests.rs` of a
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
