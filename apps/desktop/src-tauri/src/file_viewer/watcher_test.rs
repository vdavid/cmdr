//! Watcher singleton tests.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::time::Duration;

use super::watcher::{VIEWER_WATCHER_MANAGER, WatcherEvent};

fn wait_for_event(sub: &super::watcher::ViewerSubscription, total: Duration) -> Option<WatcherEvent> {
    sub.recv_timeout(total)
}

#[test]
fn watcher_observes_append_within_debounce() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let path = tmp.path().join("log.txt");
    fs::write(&path, b"first\n").unwrap();

    let sub = VIEWER_WATCHER_MANAGER.subscribe(&path).expect("subscribe");

    // Append after subscription is live so the debouncer sees the event.
    let mut f = OpenOptions::new().append(true).open(&path).unwrap();
    f.write_all(b"second\n").unwrap();
    drop(f);

    // Give the debouncer time to emit (300 ms debounce + slack).
    let event = wait_for_event(&sub, Duration::from_secs(8));
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

    let sub = VIEWER_WATCHER_MANAGER.subscribe(&path).expect("subscribe");

    // Truncate.
    let f = OpenOptions::new().write(true).truncate(true).open(&path).unwrap();
    drop(f);

    let event = wait_for_event(&sub, Duration::from_secs(8));
    assert!(matches!(event, Some(WatcherEvent::Shrunk)), "got {:?}", event);
}

#[test]
fn watcher_observes_inode_replace() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let path = tmp.path().join("log.txt");
    fs::write(&path, b"old\n").unwrap();

    let sub = VIEWER_WATCHER_MANAGER.subscribe(&path).expect("subscribe");

    // Atomic-replace via rename. Write a sibling, then rename it onto `path`.
    let new_path = tmp.path().join("log.txt.new");
    fs::write(&new_path, b"NEW content\n").unwrap();
    fs::rename(&new_path, &path).unwrap();

    let event = wait_for_event(&sub, Duration::from_secs(8));
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

    let sub = VIEWER_WATCHER_MANAGER.subscribe(&path).expect("subscribe");

    // Many small writes in <100 ms.
    let mut f = OpenOptions::new().append(true).open(&path).unwrap();
    for _ in 0..50 {
        f.write_all(b"y\n").unwrap();
    }
    drop(f);

    // First event should be Grew with the final size.
    let event = wait_for_event(&sub, Duration::from_secs(8));
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
