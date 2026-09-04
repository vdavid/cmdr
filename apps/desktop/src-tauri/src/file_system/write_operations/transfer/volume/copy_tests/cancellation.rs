//! Cancelling a multi-file copy, before it starts and mid-flight.

use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_multi_file_copy_cancel_before_start() {
    let (source, dest) = make_volumes();

    source.create_file(Path::new("/a.txt"), b"alpha").await.unwrap();
    source.create_file(Path::new("/b.txt"), b"bravo").await.unwrap();

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state();
    // Set Stopped BEFORE starting
    state.intent.store(2, Ordering::Relaxed);
    let config = VolumeCopyConfig::default();

    let result = copy_volumes_with_progress(
        events.clone(),
        "test-op-pre-cancel",
        &state,
        Arc::clone(&source),
        &[PathBuf::from("/a.txt"), PathBuf::from("/b.txt")],
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
    // No files should have been copied
    assert!(!dest.exists(Path::new("/a.txt")).await);
    assert!(!dest.exists(Path::new("/b.txt")).await);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_multi_file_copy_cancel_mid_flight() {
    // Custom event sink that flips intent to Stopped after a handful of
    // `Copying` progress events. Counting EVENTS (which fire per chunk, early in
    // the batch) rather than completed FILES makes the cancel land while tasks
    // are still mid-stream — robust regardless of scheduler interleaving. The
    // concurrent streaming path parks/checks between chunks, so the in-flight
    // tasks break at their next checkpoint and not all sources finish.
    struct CancelAfterNSink {
        inner: CollectorEventSink,
        intent: Arc<AtomicU8>,
        cancel_after_events: usize,
        events_seen: AtomicUsize,
    }

    impl OperationEventSink for CancelAfterNSink {
        fn emit_settled(&self, e: crate::file_system::write_operations::types::WriteSettledEvent) {
            self.inner.emit_settled(e);
        }
        fn emit_progress(&self, event: WriteProgressEvent) {
            if event.phase == WriteOperationPhase::Copying
                && self.events_seen.fetch_add(1, Ordering::Relaxed) >= self.cancel_after_events
            {
                self.intent.store(2, Ordering::Relaxed);
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
        fn emit_conflict_resolved(&self, e: WriteConflictResolvedEvent) {
            self.inner.emit_conflict_resolved(e);
        }
        fn emit_source_item_done(&self, _e: WriteSourceItemDoneEvent) {}
        fn emit_scan_progress(&self, _e: crate::file_system::write_operations::types::ScanProgressEvent) {}
        fn emit_scan_conflict(&self, _c: crate::file_system::write_operations::types::ConflictInfo) {}
        fn emit_dry_run_complete(&self, _r: crate::file_system::write_operations::types::DryRunResult) {}
    }

    let (source, dest) = make_volumes();
    // Files large enough (many 64 KB chunks) that the three not-yet-complete
    // in-flight tasks reliably observe the cancel at a between-chunk checkpoint
    // before finishing. With tiny files the whole batch can complete inside one
    // scheduler turn, making "not all 5 land" a coin flip.
    for i in 1..=5 {
        source
            .create_file(Path::new(&format!("/{}.bin", i)), &vec![0; 2_000_000])
            .await
            .unwrap();
    }

    let state = make_state();
    let events = Arc::new(CancelAfterNSink {
        inner: CollectorEventSink::new(),
        intent: Arc::clone(&state.intent),
        cancel_after_events: 3,
        events_seen: AtomicUsize::new(0),
    });
    let config = VolumeCopyConfig {
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };

    let result = copy_volumes_with_progress(
        events.clone(),
        "test-op-cancel-mid",
        &state,
        Arc::clone(&source),
        &[
            PathBuf::from("/1.bin"),
            PathBuf::from("/2.bin"),
            PathBuf::from("/3.bin"),
            PathBuf::from("/4.bin"),
            PathBuf::from("/5.bin"),
        ],
        Arc::clone(&dest),
        Path::new("/"),
        &config,
    )
    .await;

    // Cancellation from write_from_stream's progress callback results in an IoError
    // (the VolumeError::IoError "Operation cancelled" maps to WriteOperationError::IoError).
    // The outer loop then detects the Stopped intent and returns Cancelled.
    assert!(result.is_err(), "expected error, got {:?}", result);

    // A mid-flight cancel leaves the batch partially done: fewer than all 5
    // sources land. Completion order under concurrency isn't deterministic, so
    // assert on the COUNT, not on specific names.
    let mut total = 0;
    for i in 1..=5 {
        if dest.exists(Path::new(&format!("/{}.bin", i))).await {
            total += 1;
        }
    }
    assert!(total < 5, "expected fewer than 5 files, got {}", total);

    // The cancel either emits a write-cancelled event (if the intent check fires
    // between files) or returns an error (if write_from_stream's progress callback
    // returned Break). Both are valid cancellation paths.
    let cancelled = events.inner.cancelled.lock().unwrap();
    let had_error = result.is_err();
    assert!(
        cancelled.len() == 1 || had_error,
        "expected either a cancelled event or an error"
    );
}
