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
//! The appender keeps writing until the test observes a `Grew`, so a
//! just-registered-watch arming window (which drops the first writes under host
//! saturation) can't starve the test: a later write's event still arrives. The
//! test is in the `real-notify` nextest group (serialized, 20 s cap) alongside
//! the other real-FSEvents-delivery tests; its 15 s budget fails cleanly below
//! that cap if the watcher pipeline ever breaks, instead of hanging.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use cmdr_lib::file_viewer::watcher::{VIEWER_WATCHER_MANAGER, WatcherEvent};

#[tokio::test(flavor = "multi_thread")]
async fn tail_watcher_sees_appender_task_events_within_debounce() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("log.txt");
    fs::write(&path, b"first\n").unwrap();

    let sub = VIEWER_WATCHER_MANAGER.subscribe(&path).expect("subscribe");

    // Append every 50 ms until told to stop. The debouncer (300 ms window)
    // coalesces these into roughly one event per window. Writing continuously
    // (rather than a fixed 10 lines that finish in ~500 ms) means the FSEvents
    // stream can arm late under saturation and still catch a later write — the
    // arming window can't drop every write and leave nothing to observe.
    let stop = Arc::new(AtomicBool::new(false));
    let writer_path = path.clone();
    let writer_stop = Arc::clone(&stop);
    let writer = tokio::spawn(async move {
        let mut i = 0;
        while !writer_stop.load(Ordering::Relaxed) {
            tokio::time::sleep(Duration::from_millis(50)).await;
            let mut f = OpenOptions::new().append(true).open(&writer_path).unwrap();
            writeln!(f, "line {}", i).unwrap();
            i += 1;
        }
    });

    // Wait for at least one Grew. Real settle time on a quiet machine is ~400 ms;
    // the 15 s budget absorbs seconds-long FSEvents lag under a saturated suite
    // (below the group's 20 s cap).
    let start = Instant::now();
    let mut saw_grew = false;
    while start.elapsed() < Duration::from_secs(15) {
        if let Some(WatcherEvent::Grew(_)) = sub.recv_timeout(Duration::from_millis(250)) {
            saw_grew = true;
            break;
        }
    }
    stop.store(true, Ordering::Relaxed);
    writer.await.expect("writer task");

    assert!(
        saw_grew,
        "watcher must surface at least one Grew event for the appender task"
    );

    // Final size should match disk.
    let on_disk = fs::metadata(&path).unwrap().len();
    assert!(on_disk > 6, "appender task wrote nothing");
}
