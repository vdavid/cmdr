//! Watcher singleton tests.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::time::Duration;

use super::watcher::{
    VIEWER_WATCHER_MANAGER, ViewerSubscription, WatcherEvent, has_os_stream, test_only_emit, watch_for_real,
};

// Real macOS FSEvents delivery is laggy (~100 ms idle, seconds under load; see
// `file_viewer/CLAUDE.md`), so wait generously for the event to arrive. Kept
// below the 20 s nextest cap these tests get in `.config/nextest.toml` so the
// recv fails cleanly here instead of nextest SIGTERM-ing the whole process.
const EVENT_WAIT: Duration = Duration::from_secs(15);

/// Subscribes with a real OS stream: these four cells are the viewer watcher's only proof
/// that FSEvents delivers, so they opt out of the test build's scripted subscribe.
fn subscribe_for_real(path: &Path) -> ViewerSubscription {
    watch_for_real(path);
    let sub = VIEWER_WATCHER_MANAGER.subscribe(path).expect("subscribe");
    assert!(has_os_stream(path), "this cell needs a real FSEvents stream");
    sub
}

#[test]
fn watcher_observes_append_within_debounce() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let path = tmp.path().join("log.txt");
    fs::write(&path, b"first\n").unwrap();

    let sub = subscribe_for_real(&path);

    // Append after subscription is live so the debouncer sees the event.
    let mut f = OpenOptions::new().append(true).open(&path).unwrap();
    f.write_all(b"second\n").unwrap();
    drop(f);

    // Give the debouncer time to emit (300 ms debounce + slack).
    let event = sub.recv_timeout(EVENT_WAIT);
    let new_size = match event {
        Some(WatcherEvent::Grew(n)) => n,
        other => panic!("expected Grew(_), got {:?}", other),
    };
    assert_eq!(new_size, fs::metadata(&path).unwrap().len());
}

#[test]
fn watcher_observes_truncation_as_shrunk() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let path = tmp.path().join("log.txt");
    fs::write(&path, b"hello world\n").unwrap();

    let sub = subscribe_for_real(&path);

    // Truncate.
    let f = OpenOptions::new().write(true).truncate(true).open(&path).unwrap();
    drop(f);

    let event = sub.recv_timeout(EVENT_WAIT);
    assert!(matches!(event, Some(WatcherEvent::Shrunk)), "got {:?}", event);
}

#[test]
fn watcher_observes_inode_replace() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let path = tmp.path().join("log.txt");
    fs::write(&path, b"old\n").unwrap();

    let sub = subscribe_for_real(&path);

    // Atomic-replace via rename. Write a sibling, then rename it onto `path`.
    let new_path = tmp.path().join("log.txt.new");
    fs::write(&new_path, b"NEW content\n").unwrap();
    fs::rename(&new_path, &path).unwrap();

    let event = sub.recv_timeout(EVENT_WAIT);
    assert!(
        matches!(event, Some(WatcherEvent::Replaced) | Some(WatcherEvent::Grew(_))),
        "got {:?}",
        event,
    );
}

#[test]
fn watcher_debounces_rapid_writes() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let path = tmp.path().join("log.txt");
    fs::write(&path, b"x\n").unwrap();

    let sub = subscribe_for_real(&path);

    // Many small writes in <100 ms.
    let mut f = OpenOptions::new().append(true).open(&path).unwrap();
    for _ in 0..50 {
        f.write_all(b"y\n").unwrap();
    }
    drop(f);

    // First event should be Grew with the final size.
    let event = sub.recv_timeout(EVENT_WAIT);
    let new_size = match event {
        Some(WatcherEvent::Grew(n)) => n,
        other => panic!("expected Grew(_), got {:?}", other),
    };
    let final_size = fs::metadata(&path).unwrap().len();
    assert_eq!(new_size, final_size);

    // Drain follow-on events that the debouncer may emit. Verify there's at
    // most one queued event (coalescing kept us under the burst).
    let mut leftover = 0;
    while sub.try_recv().is_some() {
        leftover += 1;
        if leftover > 5 {
            panic!("debouncer did not coalesce the burst");
        }
    }
}

/// A test build subscribes with the bookkeeping and no OS stream, so a session test
/// that drives the manager through `test_only_emit` never waits on `fseventsd`. Only a
/// path that opted in with `watch_for_real` gets one.
#[test]
fn a_subscribe_in_a_test_build_opens_no_os_stream() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let path = tmp.path().join("log.txt");
    fs::write(&path, b"x\n").unwrap();

    let sub = VIEWER_WATCHER_MANAGER.subscribe(&path).expect("subscribe");

    assert!(
        !has_os_stream(&path),
        "a test-build subscribe must not open an OS stream"
    );
    assert_eq!(
        test_only_emit(sub.path(), WatcherEvent::MetadataOnly),
        1,
        "the scripted subscription still reaches its subscriber"
    );
    assert_eq!(sub.try_recv(), Some(WatcherEvent::MetadataOnly));
}

#[test]
fn subscription_drop_unregisters_path() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let path = tmp.path().join("log.txt");
    fs::write(&path, b"x\n").unwrap();

    let before = VIEWER_WATCHER_MANAGER.watch_count();
    let sub = VIEWER_WATCHER_MANAGER.subscribe(&path).expect("subscribe");
    assert_eq!(VIEWER_WATCHER_MANAGER.watch_count(), before + 1);
    drop(sub);
    assert_eq!(VIEWER_WATCHER_MANAGER.watch_count(), before);
}
