#![allow(clippy::borrow_interior_mutable_const, clippy::cast_possible_wrap)]

use std::fs;
use std::fs::File;
use std::os::unix::fs::MetadataExt;
use std::sync::atomic::Ordering;
use std::sync::mpsc::channel;
use std::thread;
use std::thread::sleep;
use std::time::Duration;

#[cfg(feature = "async-std")]
use async_std1 as async_std;
use futures_util::stream::StreamExt;
use once_cell::sync::Lazy;
use tempfile::tempdir;
#[cfg(feature = "tokio")]
use tokio1 as tokio;

use crate::ffi::{
    kFSEventStreamCreateFlagFileEvents, kFSEventStreamCreateFlagNoDefer,
    kFSEventStreamCreateFlagNone, kFSEventStreamCreateFlagUseCFTypes,
    kFSEventStreamCreateFlagUseExtendedData, kFSEventStreamEventIdSinceNow,
    FSEventStreamCreateFlags,
};
use crate::stream::{
    create_event_stream, StreamContextInfo, StreamFlags, TEST_RUNNING_RUNLOOP_COUNT,
};

#[cfg(feature = "tokio")]
static TEST_PARALLEL_LOCK: Lazy<tokio::sync::Mutex<()>> = Lazy::new(|| tokio::sync::Mutex::new(()));
#[cfg(feature = "async-std")]
static TEST_PARALLEL_LOCK: Lazy<async_std::sync::Mutex<()>> =
    Lazy::new(|| async_std::sync::Mutex::new(()));

#[test]
fn must_steam_context_info_send_and_sync() {
    fn check_send<T: Send + Sync>() {}
    check_send::<StreamContextInfo>();
}

#[cfg(feature = "tokio")]
#[tokio::test]
async fn must_abort_stream_tokio() {
    must_abort_stream().await;
}

#[cfg(feature = "async-std")]
#[async_std::test]
async fn must_abort_stream_async_std() {
    must_abort_stream().await;
}

async fn must_abort_stream() {
    // Acquire the lock so that no other runloop can be created during this test.
    let _guard = TEST_PARALLEL_LOCK.lock().await;

    // Create the stream to be tested.
    let (stream, mut handler) = create_event_stream(
        ["."],
        kFSEventStreamEventIdSinceNow,
        Duration::ZERO,
        kFSEventStreamCreateFlagNone,
    )
    .expect("to be created");
    // Now there should be one runloop.
    assert_eq!(TEST_RUNNING_RUNLOOP_COUNT.load(Ordering::SeqCst), 1);

    // Abort the stream immediately.
    let abort_thread = thread::spawn(move || {
        handler.abort();
    });

    // The stream should complete soon.
    #[cfg(feature = "tokio")]
    drop(
        tokio::time::timeout(
            Duration::from_secs(1),
            stream.into_flatten().collect::<Vec<_>>(),
        )
        .await
        .expect("to complete"),
    );
    #[cfg(feature = "async-std")]
    drop(
        async_std::future::timeout(
            Duration::from_secs(1),
            stream.into_flatten().collect::<Vec<_>>(),
        )
        .await
        .expect("to complete"),
    );

    // The runloop should be released.
    assert_eq!(TEST_RUNNING_RUNLOOP_COUNT.load(Ordering::SeqCst), 0);

    abort_thread.join().expect("to join");
}

#[cfg(feature = "tokio")]
#[tokio::test]
async fn must_receive_fs_events_tokio() {
    must_receive_fs_events().await;
}

#[cfg(feature = "async-std")]
#[async_std::test]
async fn must_receive_fs_events_async_std() {
    must_receive_fs_events().await;
}

async fn must_receive_fs_events() {
    // Acquire the lock so that runloop created in this test won't affect others.
    let _guard = TEST_PARALLEL_LOCK.lock().await;

    // Run sequentially to avoid runloop contention between concurrent FSEvents streams,
    // which causes directory-level events to be delivered instead of file-level events on
    // macOS Sequoia.
    let ci = option_env!("CI").is_some();
    must_receive_fs_events_impl(
        kFSEventStreamCreateFlagFileEvents
            | kFSEventStreamCreateFlagUseCFTypes
            | kFSEventStreamCreateFlagUseExtendedData,
        !ci,
        !ci,
    )
    .await;
    must_receive_fs_events_impl(
        kFSEventStreamCreateFlagFileEvents | kFSEventStreamCreateFlagUseCFTypes,
        false,
        !ci,
    )
    .await;
    must_receive_fs_events_impl(kFSEventStreamCreateFlagFileEvents, false, !ci).await;
    must_receive_fs_events_impl(
        kFSEventStreamCreateFlagUseCFTypes | kFSEventStreamCreateFlagUseExtendedData,
        false,
        false,
    )
    .await;
    must_receive_fs_events_impl(kFSEventStreamCreateFlagUseCFTypes, false, false).await;
}

async fn must_receive_fs_events_impl(
    flags: FSEventStreamCreateFlags,
    verify_inode: bool,
    verify_file_events: bool,
) {
    // Create the test dir.
    let dir = tempdir().expect("to be created");
    let test_file = dir
        .path()
        .canonicalize() // ensure it's an canonical path because FSEvent api returns that
        .expect("to succeed")
        .join("test_file");

    // Create a channel to inform the abort thread that fs operations are completed.
    let (tx, rx) = channel();

    // Create the stream to be tested.
    let (stream, mut handler) = create_event_stream(
        [dir.path()],
        kFSEventStreamEventIdSinceNow,
        Duration::ZERO,
        flags | kFSEventStreamCreateFlagNoDefer,
    )
    .expect("to be created");
    let abort_thread = thread::spawn(move || {
        // Once fs operations are completed, abort the stream.
        rx.recv().expect("to be signaled");
        // Tolerance time
        sleep(Duration::from_secs(1));
        handler.abort();
    });

    // First we create a file.
    let f = File::create(&test_file).expect("to be created");
    let inode = f.metadata().expect("to be fetched").ino() as i64;
    // Sync and wait so that ITEM_CREATE and ITEM_DELETE events won't be coalesced.
    // On macOS Sequoia, FSEvents needs a brief window between close() and unlink() to
    // deliver separate events; without this, rapid create+delete merges into a single event.
    f.sync_all().expect("to succeed");
    drop(f);
    sleep(Duration::from_millis(200));
    // Now we delete this file.
    fs::remove_file(&test_file).expect("to be removed");
    // Ensure the filesystem is up to date.
    unsafe { libc::sync() };
    // Signal the abort thread that we are ready.
    tx.send(()).expect("to signal");

    // It's fine to consume the stream later because it's reactive and can still be consumed if it's aborted.
    #[cfg(feature = "tokio")]
    let events: Vec<_> =
        tokio::time::timeout(Duration::from_secs(6), stream.into_flatten().collect())
            .await
            .expect("to complete");
    #[cfg(feature = "async-std")]
    let events: Vec<_> =
        async_std::future::timeout(Duration::from_secs(6), stream.into_flatten().collect())
            .await
            .expect("to complete");

    if verify_file_events {
        // We expect at least a create and a delete event. Additional directory-level events
        // may be present (macOS Sequoia delivers these alongside file events).
        assert!(events.len() >= 2, "expected at least 2 events, got {}", events.len());

        // Find the file creation event (by path and flags, not position).
        let create_event = events
            .iter()
            .find(|e| {
                e.path.as_path() == test_file.as_path()
                    && e.flags.contains(StreamFlags::ITEM_CREATED | StreamFlags::IS_FILE)
            })
            .expect("to find file creation event");
        if verify_inode {
            assert_eq!(create_event.inode, Some(inode));
        }

        // Find the file deletion event.
        let remove_event = events
            .iter()
            .find(|e| {
                e.path.as_path() == test_file.as_path()
                    && e.flags.contains(StreamFlags::ITEM_REMOVED | StreamFlags::IS_FILE)
            })
            .expect("to find file removal event");
        if verify_inode {
            assert_eq!(remove_event.inode, Some(inode));
        }
    } else {
        assert!(!events.is_empty());
    }

    abort_thread.join().expect("to join");
}
