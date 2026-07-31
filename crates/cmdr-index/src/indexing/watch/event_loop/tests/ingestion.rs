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

// ── Backlog trend ────────────────────────────────────────────────────

/// A cold start hands the replay a backlog in the hundreds of thousands and it
/// drains monotonically for minutes. That is the system working, so every report
/// along the way must read as progress, never as a warning: the queue DEPTH says
/// nothing about whether anything is wrong.
#[test]
fn a_draining_backlog_reports_progress_not_a_warning() {
    let mut tracker = BacklogTracker::new();
    let t0 = Instant::now();

    // The first sample has nothing to compare against, so it can only report depth.
    let (warn, line) = tracker
        .sample("Replay", 831_060, t0)
        .expect("the first sample always reports");
    assert!(!warn, "a backlog we know nothing about yet is not a warning");
    assert!(line.contains("831,060 events"), "got: {line}");

    // Draining: the number that matters is the trend, and it belongs in the line.
    let (warn, line) = tracker
        .sample("Replay", 787_194, t0 + INGESTION_WARN_INTERVAL)
        .expect("a sample past the interval reports");
    assert!(!warn, "a backlog that is draining is progress, not a warning");
    assert!(
        line.contains("down 43,866"),
        "the trend belongs in the line; got: {line}"
    );
}

/// The condition that actually needs attention: the queue is NOT draining. Only
/// then does the line escalate to a warning.
#[test]
fn a_backlog_that_is_not_draining_warns() {
    let mut tracker = BacklogTracker::new();
    let t0 = Instant::now();
    tracker.sample("Replay", 500_000, t0);

    let (warn, line) = tracker
        .sample("Replay", 512_000, t0 + INGESTION_WARN_INTERVAL)
        .expect("a sample past the interval reports");
    assert!(warn, "a growing queue is the case worth waking someone for");
    assert!(line.contains("not draining"), "got: {line}");
    assert!(line.contains("up 12,000"), "got: {line}");

    // Flat counts as not draining too: no progress is being made.
    let (warn, _) = tracker
        .sample("Replay", 512_000, t0 + INGESTION_WARN_INTERVAL * 2)
        .expect("a sample past the interval reports");
    assert!(warn, "a flat queue is not draining either");
}

/// A sustained backlog reports at a steady cadence, not on every flush tick.
#[test]
fn backlog_reports_are_rate_limited() {
    let mut tracker = BacklogTracker::new();
    let t0 = Instant::now();
    assert!(tracker.sample("Replay", 400_000, t0).is_some());
    assert!(
        tracker
            .sample("Replay", 390_000, t0 + Duration::from_millis(200))
            .is_none(),
        "a report inside the interval is suppressed"
    );
    assert!(
        tracker
            .sample("Replay", 380_000, t0 + INGESTION_WARN_INTERVAL)
            .is_some()
    );
}

/// Dropping back to healthy ends the episode: the next backlog is compared
/// against its own first sample, not against a depth from minutes ago.
#[test]
fn a_healthy_queue_ends_the_episode() {
    let mut tracker = BacklogTracker::new();
    let t0 = Instant::now();
    tracker.sample("Replay", 500_000, t0);
    tracker.reset();

    let (warn, line) = tracker
        .sample("Replay", 600_000, t0 + Duration::from_millis(10))
        .expect("a fresh episode reports immediately");
    assert!(!warn, "a fresh episode has no trend yet, so it cannot warn");
    assert!(!line.contains("up "), "no stale comparison; got: {line}");
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
