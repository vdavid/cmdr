use std::ffi::c_void;
use std::fs;
use std::fs::File;
use std::io;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::channel;
use std::sync::{Arc, LazyLock};
use std::thread;
use std::thread::sleep;
use std::time::{Duration, Instant};

#[cfg(feature = "async-std")]
use async_std1 as async_std;
use futures_util::future::FutureExt;
use futures_util::stream::StreamExt;
use tempfile::tempdir;
#[cfg(feature = "tokio")]
use tokio1 as tokio;

use crate::ffi::{
    kFSEventStreamCreateFlagFileEvents, kFSEventStreamCreateFlagNoDefer,
    kFSEventStreamCreateFlagNone, kFSEventStreamCreateFlagUseCFTypes,
    kFSEventStreamCreateFlagUseExtendedData, kFSEventStreamEventIdSinceNow,
    FSEventStreamCreateFlags, FSEventStreamEventFlags, FSEventStreamEventId, SysFSEventStream,
    SysFSEventStreamContext, SysFSEventStreamRef,
};
use crate::stream::{
    create_event_stream, Event, StreamContextInfo, StreamFlags, TEST_RUNNING_RUNLOOP_COUNT,
};

#[cfg(feature = "tokio")]
static TEST_PARALLEL_LOCK: LazyLock<tokio::sync::Mutex<()>> =
    LazyLock::new(|| tokio::sync::Mutex::new(()));
#[cfg(feature = "async-std")]
static TEST_PARALLEL_LOCK: LazyLock<async_std::sync::Mutex<()>> =
    LazyLock::new(|| async_std::sync::Mutex::new(()));

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

    // Abort the stream from another thread, and wait for `abort` to return: it
    // joins the runloop thread, whose stream release drops the only sender.
    thread::spawn(move || {
        handler.abort();
    })
    .join()
    .expect("to join");

    // So the stream has already ended, with nothing left to wait for. A timeout
    // here raced `abort`'s own `fseventsd` round trips (stop, invalidate), which a
    // loaded host stretched past the 1 s it allowed: 67 failures in the check
    // runner's test log, none sooner than 1.3 s, 2026-08 to 2026-10.
    drop(
        stream
            .into_flatten()
            .collect::<Vec<_>>()
            .now_or_never()
            .expect("an aborted stream to have ended by the time `abort` returns"),
    );

    // The runloop should be released.
    assert_eq!(TEST_RUNNING_RUNLOOP_COUNT.load(Ordering::SeqCst), 0);
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
    let deadline = Instant::now() + DELIVERY_BUDGET;
    must_receive_fs_events_impl(
        kFSEventStreamCreateFlagFileEvents
            | kFSEventStreamCreateFlagUseCFTypes
            | kFSEventStreamCreateFlagUseExtendedData,
        !ci,
        !ci,
        deadline,
    )
    .await;
    must_receive_fs_events_impl(
        kFSEventStreamCreateFlagFileEvents | kFSEventStreamCreateFlagUseCFTypes,
        false,
        !ci,
        deadline,
    )
    .await;
    must_receive_fs_events_impl(kFSEventStreamCreateFlagFileEvents, false, !ci, deadline).await;
    must_receive_fs_events_impl(
        kFSEventStreamCreateFlagUseCFTypes | kFSEventStreamCreateFlagUseExtendedData,
        false,
        false,
        deadline,
    )
    .await;
    must_receive_fs_events_impl(kFSEventStreamCreateFlagUseCFTypes, false, false, deadline).await;
}

/// How long the five flag combinations get between them to have the watch deliver.
/// ONE shared budget, not one per combination: five stacked budgets could outlast
/// the 30 s nextest cap and turn a slow scenario into a killed process with no
/// panic. 25 s keeps this assertion authoritative (its message names the flags and
/// dumps the events) with the cap as the hang backstop. The producer below keeps
/// making fresh pairs the whole time, so this is a backstop for "the watch never
/// delivers", not a latency guess: measured 4.6–14.8 s over 8 runs on an M3 Max
/// under concurrent load, 2026-08-08.
const DELIVERY_BUDGET: Duration = Duration::from_secs(25);

/// One create/delete pair the producer made, and the inode it had.
struct Probe {
    path: PathBuf,
    inode: i64,
}

/// Whether `events` carry the file-level creation AND removal of one probe.
fn probe_was_delivered(events: &[Event], probes: &[Probe], verify_inode: bool) -> bool {
    probes.iter().any(|probe| {
        let delivered = |wanted: StreamFlags| {
            events.iter().any(|event| {
                event.path.as_path() == probe.path.as_path()
                    && event.flags.contains(wanted | StreamFlags::IS_FILE)
                    && (!verify_inode || event.inode == Some(probe.inode))
            })
        };
        delivered(StreamFlags::ITEM_CREATED) && delivered(StreamFlags::ITEM_REMOVED)
    })
}

async fn must_receive_fs_events_impl(
    flags: FSEventStreamCreateFlags,
    verify_inode: bool,
    verify_file_events: bool,
    deadline: Instant,
) {
    // Create the test dir.
    let dir = tempdir().expect("to be created");
    let watch_dir = dir
        .path()
        .canonicalize() // ensure it's a canonical path because FSEvent api returns that
        .expect("to succeed");

    // Create the stream to be tested.
    let (stream, mut handler) = create_event_stream(
        [dir.path()],
        kFSEventStreamEventIdSinceNow,
        Duration::ZERO,
        flags | kFSEventStreamCreateFlagNoDefer,
    )
    .expect("to be created");

    // Keep producing create/delete pairs until the watch delivers one. macOS drops the
    // mutation that lands in a just-armed watch's window, and coalesces a lone
    // create+delete into a single event; neither is recoverable by waiting, so a single
    // attempt is a coin flip. Redoing the mutation is what makes the assertions below
    // reachable without weakening them.
    let stop = Arc::new(AtomicBool::new(false));
    let (probe_tx, probe_rx) = channel::<Probe>();
    let producer_stop = Arc::clone(&stop);
    let producer = thread::spawn(move || {
        let mut serial = 0_u32;
        while !producer_stop.load(Ordering::SeqCst) {
            let path = watch_dir.join(format!("test_file_{serial}"));
            serial += 1;

            // First we create a file.
            let f = File::create(&path).expect("to be created");
            let inode = f.metadata().expect("to be fetched").ino() as i64;
            // Sync and wait so that ITEM_CREATE and ITEM_DELETE events won't be coalesced.
            // On macOS Sequoia, FSEvents needs a brief window between close() and unlink() to
            // deliver separate events; without this, rapid create+delete merges into a single event.
            f.sync_all().expect("to succeed");
            drop(f);
            if probe_tx
                .send(Probe {
                    path: path.clone(),
                    inode,
                })
                .is_err()
            {
                return;
            }
            sleep(Duration::from_millis(200));

            // Now we delete this file.
            fs::remove_file(&path).expect("to be removed");
            // Ensure the filesystem is up to date.
            // SAFETY: (test) `sync(2)` takes no arguments, touches no Rust-owned memory, and
            // can't fail.
            unsafe { libc::sync() };
            sleep(Duration::from_millis(300));
        }
    });

    let mut events: Vec<Event> = Vec::new();
    let mut probes: Vec<Probe> = Vec::new();
    let mut stream = Box::pin(stream.into_flatten());
    let collect = async {
        loop {
            let Some(event) = stream.next().await else {
                return false;
            };
            events.push(event);
            while let Ok(probe) = probe_rx.try_recv() {
                probes.push(probe);
            }
            if !verify_file_events {
                // These flag combinations carry no file-level detail, so any event
                // at all is the whole claim (unchanged from the assertion below).
                return true;
            }
            if probe_was_delivered(&events, &probes, verify_inode) {
                return true;
            }
        }
    };
    let remaining = deadline.saturating_duration_since(Instant::now());
    #[cfg(feature = "tokio")]
    let delivered = tokio::time::timeout(remaining, collect)
        .await
        .unwrap_or(false);
    #[cfg(feature = "async-std")]
    let delivered = async_std::future::timeout(remaining, collect)
        .await
        .unwrap_or(false);

    // Stop producing before the temp dir goes away, then tear the stream down.
    stop.store(true, Ordering::SeqCst);
    producer.join().expect("to join");
    handler.abort();

    assert!(
        delivered,
        "the watch delivered no {} inside the test's {:?} budget for flags {:#x}; got {} event(s): {}",
        if verify_file_events {
            "file-level create+delete pair"
        } else {
            "event"
        },
        DELIVERY_BUDGET,
        flags,
        events.len(),
        events
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(", ")
    );
}

/// A context whose drop is counted, so a test can see `SysFSEventStream::new` release it exactly
/// once on every path (never leaked, never double freed).
struct CountedContext(Arc<AtomicUsize>);

impl Drop for CountedContext {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

crate::impl_release_callback!(release_counted_context, CountedContext);

extern "C" fn ignore_events(
    _stream_ref: SysFSEventStreamRef,
    _info: *mut c_void,
    _num_events: usize,
    _event_paths: *mut c_void,
    _event_flags: *const FSEventStreamEventFlags,
    _event_ids: *const FSEventStreamEventId,
) {
}

fn create_counted<P: AsRef<Path>>(
    paths: impl IntoIterator<Item = P>,
) -> (io::Result<SysFSEventStream>, Arc<AtomicUsize>) {
    let drops = Arc::new(AtomicUsize::new(0));
    let context =
        SysFSEventStreamContext::new(CountedContext(Arc::clone(&drops)), release_counted_context);
    let stream = SysFSEventStream::new(
        ignore_events,
        &context,
        paths,
        kFSEventStreamEventIdSinceNow,
        Duration::ZERO,
        kFSEventStreamCreateFlagNone,
    );
    (stream, drops)
}

#[test]
fn stream_releases_its_context_once_when_dropped() {
    let (stream, drops) = create_counted(["."]);
    let stream = stream.expect("to be created");
    assert_eq!(
        drops.load(Ordering::SeqCst),
        0,
        "the live stream still owns it"
    );
    drop(stream);
    assert_eq!(drops.load(Ordering::SeqCst), 1);
}

/// Regression: `FSEventStreamCreate` returns NULL for an empty path list, and we used to wrap
/// that NULL and go on to schedule, start, and release it.
#[test]
fn empty_path_list_is_an_error_and_releases_the_context_once() {
    let (stream, drops) = create_counted(Vec::<&Path>::new());
    assert_eq!(
        stream.err().map(|err| err.kind()),
        Some(io::ErrorKind::InvalidInput)
    );
    assert_eq!(drops.load(Ordering::SeqCst), 1);
}

/// Regression: a path `CFURL` rejects used to return early and leak the boxed context.
#[test]
fn invalid_path_is_an_error_and_releases_the_context_once() {
    let (stream, drops) = create_counted([""]);
    assert!(stream.is_err());
    assert_eq!(drops.load(Ordering::SeqCst), 1);
}

#[test]
fn create_event_stream_rejects_an_empty_path_list() {
    let created = create_event_stream(
        Vec::<&Path>::new(),
        kFSEventStreamEventIdSinceNow,
        Duration::ZERO,
        kFSEventStreamCreateFlagNone,
    );
    assert_eq!(
        created.err().map(|err| err.kind()),
        Some(io::ErrorKind::InvalidInput)
    );
}
