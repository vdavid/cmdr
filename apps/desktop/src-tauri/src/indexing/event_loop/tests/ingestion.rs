//! Ingestion-pressure classifier + unbounded-buffer absorb tests (Fix 2:
//! a slow drain must not cascade into a forced full scan).

use super::*;

/// The classifier's thresholds, including the load-bearing contract that a backlog
/// at the OLD bounded-channel capacity (20K) is merely "falling behind", NOT
/// "overflowing" — so it no longer forces a full scan.
#[test]
fn classifier_thresholds() {
    assert_eq!(classify_ingestion_pressure(0), IngestionPressure::Healthy);
    assert_eq!(
        classify_ingestion_pressure(INGESTION_BACKLOG_WARN),
        IngestionPressure::Healthy,
        "at the warn watermark is still healthy"
    );
    assert_eq!(
        classify_ingestion_pressure(INGESTION_BACKLOG_WARN + 1),
        IngestionPressure::FallingBehind,
        "just past the OLD 20K cap we log, we do NOT force a scan"
    );
    assert_eq!(
        classify_ingestion_pressure(INGESTION_HARD_CAP),
        IngestionPressure::FallingBehind,
        "at the hard cap is still falling-behind, not yet overflowing"
    );
    assert_eq!(
        classify_ingestion_pressure(INGESTION_HARD_CAP + 1),
        IngestionPressure::Overflowing,
        "only past the RAM-guard hard cap do we deliberately fall back to a full scan"
    );
}

/// Fix 2 repro: a backlog that would have tripped the OLD bounded 20K channel
/// (blocking the forward task → upstream FSEvents overflow → forced full scan) is
/// now absorbed by the unbounded buffer. The producer never blocks, nothing is
/// dropped, and the loop's decision at that depth is "keep draining", not "scan".
#[tokio::test]
async fn a_slow_drain_backlog_absorbs_without_forcing_a_scan() {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<watcher::FsChangeEvent>();

    // Pump well past the old 20K bounded capacity. Unbounded `send` returns
    // immediately, so the producer is never backpressured into the pre-fix overflow.
    let n = INGESTION_BACKLOG_WARN + 5_000; // 25K, comfortably over the old cap
    for i in 0..n {
        tx.send(watcher::FsChangeEvent {
            path: format!("/x/{i}"),
            event_id: i as u64,
            flags: watcher::FsEventFlags::default(),
        })
        .expect("an unbounded send never blocks or drops");
    }

    // The whole backlog is buffered (nothing dropped)...
    assert_eq!(rx.len(), n, "the unbounded buffer absorbed the entire backlog");
    // ...and at that depth the loop's decision is "keep draining", NOT force a scan
    // (the pre-fix cascade fired here).
    assert_eq!(
        classify_ingestion_pressure(rx.len()),
        IngestionPressure::FallingBehind,
        "a backlog past the OLD 20K cap no longer forces a full scan"
    );

    // A (simulated) slow consumer drains it fully; every event arrives, in order.
    let mut received = 0usize;
    while let Ok(event) = rx.try_recv() {
        assert_eq!(event.event_id, received as u64, "events drain in FIFO order");
        received += 1;
    }
    assert_eq!(
        received, n,
        "the slow drain eventually processes every event, none lost"
    );
}
