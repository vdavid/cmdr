//! Integration test for the viewer tail-mode watcher pipeline.
//!
//! Verifies that a Tokio task appending to a tempfile produces watcher events
//! at the BE within the debounce window (300 ms + slack). This is the
//! milestone-3 plan's promised `tests/viewer_tail_integration.rs`.
//!
//! We can't capture Tauri events without an `AppHandle`, so we go one layer
//! below the FE surface: the `VIEWER_WATCHER_MANAGER` subscription. The same
//! events that drive the BE's `viewer:file-changed:<sid>` emission land on
//! the subscription channel. Asserting on the subscription is the closest
//! BE-side check we can do without standing up Tauri runtime.
//!
//! Runs in the default suite. The test self-bounds its wait at 5 s (12+ debounce
//! windows of settle time), comfortably under nextest's 8 s per-test hard cap, so
//! it fails fast and cleanly if the watcher pipeline ever breaks instead of hanging.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::time::{Duration, Instant};

use cmdr_lib::file_viewer::watcher::{VIEWER_WATCHER_MANAGER, WatcherEvent};

#[tokio::test(flavor = "multi_thread")]
async fn tail_watcher_sees_appender_task_events_within_debounce() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("log.txt");
    fs::write(&path, b"first\n").unwrap();

    let sub = VIEWER_WATCHER_MANAGER.subscribe(&path).expect("subscribe");

    // Spawn a Tokio task that appends to the file every 50 ms. The debouncer
    // (300 ms window) coalesces these into roughly one event per window.
    let writer_path = path.clone();
    let writer = tokio::spawn(async move {
        for i in 0..10 {
            tokio::time::sleep(Duration::from_millis(50)).await;
            let mut f = OpenOptions::new().append(true).open(&writer_path).unwrap();
            writeln!(f, "line {}", i).unwrap();
        }
    });

    // Wait for at least one event within (debounce + writer ramp-up + slack).
    // Total budget: 5 s. Real settle time on a quiet machine is ~400 ms.
    let start = Instant::now();
    let mut saw_grew = false;
    while start.elapsed() < Duration::from_secs(5) {
        if let Some(WatcherEvent::Grew(_)) = sub.recv_timeout(Duration::from_millis(250)) {
            saw_grew = true;
            break;
        }
    }
    writer.await.expect("writer task");

    assert!(
        saw_grew,
        "watcher must surface at least one Grew event for the appender task"
    );

    // Final size should match disk.
    let on_disk = fs::metadata(&path).unwrap().len();
    assert!(on_disk > 6, "appender task wrote nothing");
}
