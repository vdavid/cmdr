//! Losing the share mid-walk: the typed terminal disconnect, the
//! consecutive-failure backstop, and which errors count as either.

use super::*;

/// THE regression test for the reported prod bug. A volume disconnects after
/// listing K of N dirs: the walk must STOP promptly (not churn the remaining
/// N−K queued dirs into empty rows), return the typed `DeviceDisconnected`
/// error, and — crucially — the caller must write NO `scan_completed_at`
/// (asserted at the manager level; here we assert the typed error + prompt
/// stop, which is what the completion handler routes on).
#[tokio::test]
async fn disconnect_mid_walk_stops_promptly_and_returns_typed_error() {
    use crate::indexing::writer::IndexWriter;

    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = dir.path().join("vol-scan-disconnect.db");
    let _store = IndexStore::open(&db_path).expect("open store");
    let writer = IndexWriter::spawn(&db_path, crate::indexing::NoopEventSink::shared()).expect("spawn writer");

    // Root + 200 empty subdirs (≫ FULL_LISTING_BUDGET). BFS: list root (call 1)
    // discovers 200 dirs, then lists them concurrently (up to FULL_LISTING_BUDGET in
    // flight). The 4th list call returns a typed disconnect. The walk must stop
    // topping up and drop the in-flight listings rather than churning all 200.
    let n_subdirs = 200;
    let fail_after_calls = 4;
    let calls = Arc::new(AtomicU64::new(0));
    let vol: Arc<dyn Volume> = Arc::new(CountingDisconnectVolume {
        inner: wide_tree(n_subdirs),
        fail_after_calls,
        calls: Arc::clone(&calls),
        untyped_failure: false,
    });

    let cancelled = CancellationToken::new();
    let result = scan_volume_via_trait(
        vol,
        PathBuf::from("/"),
        writer.clone(),
        progress(),
        cancelled,
        ScanPacer::unpaced(),
    )
    .await;

    // The typed terminal error, NOT a clean Ok (which is today's bug: a clean
    // finish over silently-empty rows). Matched by the TYPED variant.
    match result {
        Err(VolumeScanError::Volume(VolumeError::DeviceDisconnected(_))) => {}
        other => panic!("expected typed DeviceDisconnected terminal error, got {other:?}"),
    }

    // Prompt stop: the walk bailed within ~one concurrency window of the disconnect
    // and did NOT churn the remaining queued dirs. With concurrency the count is no
    // longer exactly `fail_after_calls` (up to FULL_LISTING_BUDGET listings were already
    // in flight), but it's bounded well below the full `n_subdirs`.
    let made = calls.load(Ordering::Relaxed) as usize;
    assert!(
        made < n_subdirs,
        "walk must stop at the disconnect, not churn all {n_subdirs} queued dirs (made {made})",
    );
    assert!(
        made <= 1 + FULL_LISTING_BUDGET + fail_after_calls,
        "walk must stop within ~one concurrency window of the disconnect (made {made})",
    );

    writer.flush().await.expect("flush");
    writer.shutdown();
}

/// The consecutive-failure backstop: a disconnect-shaped error that does NOT
/// map to the typed variant (here `IoError`) must still abort the walk after
/// `CONSECUTIVE_FAILURE_ABORT` consecutive failures, rather than churning
/// every queued dir into an empty row.
#[tokio::test]
async fn consecutive_untyped_failures_trip_the_backstop() {
    use crate::indexing::writer::IndexWriter;

    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = dir.path().join("vol-scan-backstop.db");
    let _store = IndexStore::open(&db_path).expect("open store");
    let writer = IndexWriter::spawn(&db_path, crate::indexing::NoopEventSink::shared()).expect("spawn writer");

    // Enough subdirs that the backstop (N consecutive) trips well before the
    // queue drains, even with up to FULL_LISTING_BUDGET listings in flight. Root lists
    // fine (call 1), then every subdir listing fails with an untyped IoError.
    let n_subdirs = CONSECUTIVE_FAILURE_ABORT * 6;
    let calls = Arc::new(AtomicU64::new(0));
    let vol: Arc<dyn Volume> = Arc::new(CountingDisconnectVolume {
        inner: wide_tree(n_subdirs),
        fail_after_calls: 2, // root ok, then every child fails
        calls: Arc::clone(&calls),
        untyped_failure: true,
    });

    let cancelled = CancellationToken::new();
    let result = scan_volume_via_trait(
        vol,
        PathBuf::from("/"),
        writer.clone(),
        progress(),
        cancelled,
        ScanPacer::unpaced(),
    )
    .await;

    match result {
        Err(VolumeScanError::ConsecutiveFailures { count, .. }) => {
            assert_eq!(count, CONSECUTIVE_FAILURE_ABORT, "aborts at exactly the threshold");
        }
        other => panic!("expected ConsecutiveFailures backstop abort, got {other:?}"),
    }

    // Bounded stop: the backstop aborts after ~root + one concurrency window +
    // N failures (concurrency means some listings were already in flight), and the
    // remaining dirs were never attempted — well short of the full queue.
    let made = calls.load(Ordering::Relaxed) as usize;
    assert!(
        made < n_subdirs,
        "backstop must stop well short of churning the whole {n_subdirs}-dir queue (made {made})",
    );
    assert!(
        made <= 1 + FULL_LISTING_BUDGET + CONSECUTIVE_FAILURE_ABORT,
        "backstop stops within ~one concurrency window of the threshold (made {made})",
    );

    writer.flush().await.expect("flush");
    writer.shutdown();
}

/// A single transient failure followed by successes does NOT trip the
/// backstop: the consecutive counter resets on every success, so an isolated
/// bad dir is still skip-and-continue (the existing behavior we keep).
#[tokio::test]
async fn isolated_transient_failure_does_not_trip_backstop() {
    use crate::indexing::writer::IndexWriter;

    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = dir.path().join("vol-scan-transient.db");
    let _store = IndexStore::open(&db_path).expect("open store");
    let writer = IndexWriter::spawn(&db_path, crate::indexing::NoopEventSink::shared()).expect("spawn writer");

    // One subdir fails (untyped), the rest list fine. The scan completes
    // cleanly (the bad dir is skipped, stays listed_epoch=0).
    let inner = InMemoryVolume::with_entries(
        "Test",
        vec![
            entry("good", "/good", true, None),
            entry("a.txt", "/good/a.txt", false, Some(7)),
            entry("bad", "/bad", true, None),
            entry("alsogood", "/alsogood", true, None),
        ],
    );
    let vol: Arc<dyn Volume> = Arc::new(FailingListVolume {
        inner,
        fail_path: PathBuf::from("/bad"),
    });

    let cancelled = CancellationToken::new();
    scan_volume_via_trait(
        vol,
        PathBuf::from("/"),
        writer.clone(),
        progress(),
        cancelled,
        ScanPacer::unpaced(),
    )
    .await
    .expect("an isolated transient failure is skipped, scan completes");

    writer.flush().await.expect("flush");
    writer.shutdown();
}

/// `is_terminal_disconnect` routes the completion handler: true for a typed
/// `DeviceDisconnected` and the consecutive-failure backstop (keep honest
/// partial + Stale), false for a timeout / context / writer-send (discard).
#[test]
fn terminal_disconnect_classification() {
    assert!(
        VolumeScanError::Volume(VolumeError::DeviceDisconnected("x".into())).is_terminal_disconnect(),
        "typed DeviceDisconnected is a terminal disconnect"
    );
    assert!(
        VolumeScanError::ConsecutiveFailures {
            count: CONSECUTIVE_FAILURE_ABORT,
            last: "io".into()
        }
        .is_terminal_disconnect(),
        "the consecutive-failure backstop is a terminal disconnect"
    );
    // Non-disconnect terminations are NOT kept as honest partials.
    assert!(
        !VolumeScanError::Timeout(PathBuf::from("/wedged")).is_terminal_disconnect(),
        "a timeout is discarded, not kept"
    );
    assert!(
        !VolumeScanError::Volume(VolumeError::PermissionDenied("root".into())).is_terminal_disconnect(),
        "a non-disconnect volume error (root-fatal) is discarded"
    );
    assert!(
        !VolumeScanError::Cancelled(ScanSummary {
            total_entries: 12,
            total_dirs: 3,
            total_physical_bytes: 4096,
            duration_ms: 8,
        })
        .is_terminal_disconnect(),
        "a user cancel is discardable, so it must NOT keep the partial as Stale"
    );
    assert!(!VolumeScanError::WriterSend("gone".into()).is_terminal_disconnect());
    assert!(!VolumeScanError::Context("ctx".into()).is_terminal_disconnect());
}
