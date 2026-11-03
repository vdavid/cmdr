//! Stream-based `FSEvents` interface.

use std::ffi::{c_void, CStr, OsStr};
use std::fmt::{Display, Formatter};
use std::io;
use std::os::raw::c_char;
use std::os::unix::ffi::OsStrExt;
use std::panic::catch_unwind;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::channel;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::thread;
use std::time::Duration;

use core_foundation::array::CFArray;
use core_foundation::base::{CFIndex, FromVoid};
use core_foundation::dictionary::CFDictionary;
use core_foundation::number::CFNumber;
use core_foundation::runloop::{kCFRunLoopBeforeWaiting, kCFRunLoopDefaultMode, CFRunLoop};
use core_foundation::string::CFString;
use futures_core::Stream;
use futures_util::stream::{iter, StreamExt};
use log::{error, trace};
use tokio_stream::wrappers::ReceiverStream;

use crate::ffi::{
    extended_data_path_key, extended_file_id_key, kFSEventStreamCreateFlagFileEvents,
    kFSEventStreamCreateFlagUseCFTypes, kFSEventStreamCreateFlagUseExtendedData, CFRunLoopExt,
    FSEventStreamCreateFlags, FSEventStreamEventFlags, FSEventStreamEventId, SysFSEventStream, SysFSEventStreamContext,
    SysFSEventStreamRef,
};
pub use crate::flags::StreamFlags;
use crate::impl_release_callback;
use crate::observer::create_oneshot_observer;
use crate::utils::FlagsExt;

#[cfg(test)]
pub(crate) static TEST_RUNNING_RUNLOOP_COUNT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// An owned permission to stop an [`EventStream`] and terminate its backing `RunLoop`.
///
/// A `EventStreamHandler` *detaches* the associated Stream and `RunLoop` when it is dropped, which
/// means that there is no longer any handle to them and no way to `abort` them.
///
/// Dropping the handler without first calling [`abort`](EventStreamHandler::abort) is not
/// recommended because this leaves a spawned thread behind and causes memory leaks.
pub struct EventStreamHandler {
    runloop: Option<(CFRunLoop, thread::JoinHandle<()>)>,
}

// SAFETY: The only non-`Send` field is the `CFRunLoop`, a retained CF reference. Retain and
// release are thread-safe for every CF object, and the run-loop calls `abort` makes from the
// handler's thread (`CFRunLoopAddObserver`, `CFRunLoopRemoveObserver`, `CFRunLoopIsWaiting`,
// `CFRunLoopStop`) are documented as callable from any thread ("Core Foundation run loop
// functions are generally thread-safe", Threading Programming Guide § Run Loops, and the Thread
// Safety Summary).
unsafe impl Send for EventStreamHandler {}

impl EventStreamHandler {
    /// Stop an [`EventStream`] and terminate its backing `RunLoop`.
    ///
    /// Calling this method multiple times has no extra effect and won't cause any panic, error,
    /// or undefined behavior.
    pub fn abort(&mut self) {
        if let Some((runloop, thread_handle)) = self.runloop.take() {
            let (tx, rx) = channel();
            let observer = create_oneshot_observer(kCFRunLoopBeforeWaiting, tx);
            // SAFETY: `kCFRunLoopDefaultMode` is an immutable `CFStringRef` constant exported by
            // CoreFoundation, initialized before any Rust code runs; reading it is a plain load.
            runloop.add_observer(&observer, unsafe { kCFRunLoopDefaultMode });

            if !runloop.is_waiting() {
                // Wait the RunLoop to enter Waiting state.
                rx.recv().expect("channel to receive BeforeWaiting signal");
            }

            // SAFETY: As above, a read of CoreFoundation's immutable mode constant.
            runloop.remove_observer(&observer, unsafe { kCFRunLoopDefaultMode });
            runloop.stop();

            // Wait for the thread to shut down.
            thread_handle.join().expect("thread to shut down");
        }
    }
}

/// An `FSEvents` API event.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct Event {
    pub path: PathBuf,
    pub inode: Option<i64>,
    pub flags: StreamFlags,
    pub raw_flags: FSEventStreamEventFlags,
    pub id: FSEventStreamEventId,
}

impl Display for Event {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "[{}] path: {:?}({}), flags: {} ({:x})",
            self.id,
            self.path,
            self.inode.unwrap_or(-1),
            self.flags,
            self.raw_flags
        )
    }
}

/// A stream of `FSEvents` API event batches.
///
/// You may want a stream of [`Event`] instead of a stream of batches of it.
/// Call [`EventStream::into_flatten`] to get one.
///
/// Call [`create_event_stream`] to create it.
pub struct EventStream {
    stream: ReceiverStream<Vec<Event>>,
    /// Set to `true` when events are dropped (the internal channel is full, or an event couldn't be
    /// built), so the consumer knows to rescan.
    overflow: Arc<AtomicBool>,
}

impl EventStream {
    /// Flatten event batches and produce a stream of [`Event`].
    pub fn into_flatten(self) -> impl Stream<Item = Event> {
        self.flat_map(iter)
    }

    /// Returns `true` if the internal channel overflowed and events were dropped.
    ///
    /// Once this returns `true`, the stream's consumer should treat the event
    /// history as incomplete and trigger a full rescan.
    pub fn has_overflowed(&self) -> bool {
        self.overflow.load(Ordering::Relaxed)
    }

    /// Returns a shared handle to the overflow flag.
    ///
    /// Useful for passing the flag to a separate task that monitors overflow
    /// independently of consuming the stream.
    pub fn overflow_flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.overflow)
    }
}

impl Stream for EventStream {
    type Item = Vec<Event>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.stream.poll_next_unpin(cx)
    }
}

pub(crate) struct StreamContextInfo {
    event_handler: tokio::sync::mpsc::Sender<Vec<Event>>,
    /// Shared flag set to `true` when events are dropped: `try_send` failed, or an event couldn't be built.
    overflow: Arc<AtomicBool>,
}

impl_release_callback!(release_context, StreamContextInfo);

/// A CFRunLoop that can be sent across threads, from the worker that owns it to the handler.
struct SendableCFRunLoop(CFRunLoop);

// SAFETY: A `CFRunLoop` is a retained CF reference, and CF retain/release is thread-safe. The
// receiving thread only uses it through `EventStreamHandler::abort`, whose run-loop calls are
// documented as callable from any thread (see the `Send` impl on `EventStreamHandler`).
// https://developer.apple.com/library/archive/documentation/Cocoa/Conceptual/Multithreading/ThreadSafetySummary/ThreadSafetySummary.html
unsafe impl Send for SendableCFRunLoop {}

/// Create a new [`EventStream`] and [`EventStreamHandler`] pair.
///
/// # Errors
/// Return error when `paths_to_watch` is empty or holds an invalid path, or when `FSEvents`
/// refuses to create or start the stream.
///
/// # Panics
/// Panic when the given flags combination is illegal.
pub fn create_event_stream<P: AsRef<Path>>(
    paths_to_watch: impl IntoIterator<Item = P>,
    since_when: FSEventStreamEventId,
    latency: Duration,
    flags: FSEventStreamCreateFlags,
) -> io::Result<(EventStream, EventStreamHandler)> {
    if flags.contains(kFSEventStreamCreateFlagUseExtendedData) && !flags.contains(kFSEventStreamCreateFlagUseCFTypes) {
        panic!("UseExtendedData requires UseCFTypes");
    }

    /// Channel capacity for the internal event buffer. Large enough to absorb
    /// sustained bursts (e.g. `npm install` creating 100K files) without dropping
    /// events, while still bounded to prevent runaway memory growth.
    /// At ~300 bytes per batch slot, 32K ≈ 10 MB worst case.
    const CHANNEL_CAPACITY: usize = 32_768;

    let (event_tx, event_rx) = tokio::sync::mpsc::channel(CHANNEL_CAPACITY);

    let overflow = Arc::new(AtomicBool::new(false));

    // We need to associate the stream context with our callback in order to propagate events
    // to the rest of the system. This will be owned by the stream, and will be freed when the
    // stream is closed. This means we will leak the context if we panic before reacing
    // `FSEventStreamRelease`.
    let context = StreamContextInfo {
        event_handler: event_tx,
        overflow: Arc::clone(&overflow),
    };

    let stream_context = SysFSEventStreamContext::new(context, release_context);

    let callback = if flags.contains(kFSEventStreamCreateFlagUseCFTypes) {
        if flags.contains(kFSEventStreamCreateFlagUseExtendedData) {
            if flags.contains(kFSEventStreamCreateFlagFileEvents) {
                cf_ext_with_id_callback
            } else {
                cf_ext_callback
            }
        } else {
            cf_callback
        }
    } else {
        normal_callback
    };

    let mut stream = SysFSEventStream::new(callback, &stream_context, paths_to_watch, since_when, latency, flags)?;

    // channel to pass runloop around
    let (runloop_tx, runloop_rx) = channel();

    let thread_handle = thread::spawn(move || {
        #[cfg(test)]
        TEST_RUNNING_RUNLOOP_COUNT.fetch_add(1, Ordering::SeqCst);

        let current_runloop = CFRunLoop::get_current();

        // SAFETY: `kCFRunLoopDefaultMode` is CoreFoundation's immutable, always-valid mode
        // `CFStringRef`, which is `schedule`'s whole contract. Scheduling on THIS thread's run
        // loop is what pins every callback (and so every access to the stream context) to this
        // thread, and the stream is stopped and invalidated here before it's released.
        unsafe { stream.schedule(&current_runloop, kCFRunLoopDefaultMode) };
        if stream.start() {
            // `abort` ends `CFRunLoopRun` with `CFRunLoopStop`.
            runloop_tx
                .send(Some(SendableCFRunLoop(current_runloop)))
                .expect("send runloop to stream");

            CFRunLoop::run_current();
            stream.stop();
        } else {
            // Without a started stream there's nothing to run, and `abort` would wait forever
            // for a `BeforeWaiting` that never comes, so report the failure instead.
            drop(runloop_tx.send(None));
        }
        stream.invalidate();

        #[cfg(test)]
        TEST_RUNNING_RUNLOOP_COUNT.fetch_sub(1, Ordering::SeqCst);
    });

    let Some(SendableCFRunLoop(runloop)) = runloop_rx.recv().expect("receive runloop from worker") else {
        thread_handle.join().expect("thread to shut down");
        return Err(io::Error::other("FSEventStreamStart couldn't start the stream"));
    };

    let stream = ReceiverStream::new(event_rx);
    Ok((
        EventStream { stream, overflow },
        EventStreamHandler {
            runloop: Some((runloop, thread_handle)),
        },
    ))
}

// Flags never fail to parse: `from_bits_retain` keeps bits newer than this crate, so an event
// carrying one still reaches the consumer with its known bits intact.
enum CallbackError {
    ToI64,
}

/// Defines an `FSEventStreamCallback` around `$body`, which turns one batch into events.
///
/// The FSEvents callback contract every `SAFETY:` comment in these callbacks leans on
/// (`FSEventStreamCallback` docs and `FSEvents/FSEvents.h`):
///
/// - FSEvents calls back only on the run loop the stream is scheduled on (our worker thread in
///   `create_event_stream`), one call at a time, and never after `FSEventStreamInvalidate`.
/// - `info` is the context's `info` pointer: our boxed `StreamContextInfo`. It's freed only by
///   `release_context`, which FSEvents calls when the stream is deallocated, after the worker
///   has stopped and invalidated it, so it outlives every callback. We only ever take `&` to it.
/// - `event_flags` and `event_ids` point to `num_events` elements each, valid for the call.
/// - `event_paths` holds `num_events` entries, owned by FSEvents (Get rule) and released after
///   the callback returns. Its shape is fixed by the create flags, which pick the callback: a
///   `CFArrayRef` of `CFStringRef` with `UseCFTypes`, a `CFArrayRef` of `CFDictionaryRef` (a
///   `CFStringRef` under `path`, a `CFNumberRef` under `fileID`) with `UseExtendedData`, and a C
///   array of NUL-terminated C strings without `UseCFTypes`.
/// - Nothing borrowed from these outlives the call: `$body`'s iterator is collected into owned
///   `Event`s before the callback returns.
macro_rules! define_callback {
    ($name: ident, ($num: ident, $paths: ident, $flags: ident, $ids: ident)$body: block) => {
        extern "C" fn $name(
            stream_ref: SysFSEventStreamRef,
            info: *mut c_void,
            num_events: usize,                           // size_t numEvents
            event_paths: *mut c_void,                    // void *eventPaths
            event_flags: *const FSEventStreamEventFlags, // const FSEventStreamEventFlags eventFlags[]
            event_ids: *const FSEventStreamEventId,      // const FSEventStreamEventId eventIds[]
        ) {
            fn callback_impl(
                _stream_ref: SysFSEventStreamRef,
                info: *mut c_void,
                num_events: usize,                           // size_t numEvents
                event_paths: *mut c_void,                    // void *eventPaths
                event_flags: *const FSEventStreamEventFlags, // const FSEventStreamEventFlags eventFlags[]
                event_ids: *const FSEventStreamEventId,      // const FSEventStreamEventId eventIds[]
            ) {
                fn event_iter(
                    $num: usize,
                    $paths: *mut c_void,
                    $flags: *const FSEventStreamEventFlags,
                    $ids: *const FSEventStreamEventId,
                ) -> impl Iterator<Item = Result<Event, CallbackError>> {
                    $body
                }

                trace!("Received {} event(s)", num_events);

                // SAFETY: `info` is our live `StreamContextInfo` for the whole callback (see the
                // macro docs), and shared references are all anyone takes to it. Its fields are
                // `Sync` (an mpsc `Sender` and an `AtomicBool`).
                let context = unsafe { &*(info as *const StreamContextInfo) };
                let StreamContextInfo {
                    event_handler,
                    overflow,
                } = context;

                // An event we can't build is an event the consumer never sees, so raise the
                // overflow flag: that's the consumer's "history is incomplete, rescan" signal.
                let events = event_iter(num_events, event_paths, event_flags, event_ids)
                    .filter_map(|event| match event {
                        Ok(event) => Some(event),
                        Err(CallbackError::ToI64) => {
                            error!("Unable to convert inode field to i64 (event dropped, overflow flagged)");
                            overflow.store(true, Ordering::Relaxed);
                            None
                        }
                    })
                    .collect();

                if let Err(e) = event_handler.try_send(events) {
                    if !overflow.swap(true, Ordering::Relaxed) {
                        // Log once on first failure; subsequent drops are silent.
                        // `try_send` fails for two reasons that need different responses
                        // upstream: `Full` (consumer is slow → overflow flag drives a
                        // rescan) and `Closed` (consumer dropped → events lost
                        // permanently until the stream is restarted). The error's
                        // Display impl distinguishes them ("no available capacity" vs
                        // "channel closed"), so the message below carries both signals.
                        error!("FSEvents try_send failed (events dropped): {}", e);
                    }
                }
            }

            // A panic must not unwind into FSEvents' C frames. Catching it drops the whole
            // batch, so it raises the overflow flag like any other lost event (a missing
            // extended-data key, for example, panics in `CFDictionary::get`).
            let outcome = catch_unwind(move || {
                callback_impl(stream_ref, info, num_events, event_paths, event_flags, event_ids);
            });
            if outcome.is_err() {
                error!("FSEvents callback panicked (batch dropped, overflow flagged)");
                // SAFETY: The panic unwound out of `callback_impl`, but `info` is still our live
                // `StreamContextInfo` (FSEvents frees it only after invalidation, see the macro
                // docs), and we only take a shared reference to an atomic.
                let context = unsafe { &*(info as *const StreamContextInfo) };
                context.overflow.store(true, Ordering::Relaxed);
            }
        }
    };
}

/// Reads event `idx`'s flags and id.
///
/// # Safety
/// `flags` and `ids` must point to arrays of more than `idx` elements, as FSEvents' callback
/// arguments do for every `idx < num_events`.
unsafe fn flags_and_id(
    flags: *const FSEventStreamEventFlags,
    ids: *const FSEventStreamEventId,
    idx: usize,
) -> (FSEventStreamEventFlags, FSEventStreamEventId) {
    // SAFETY: In bounds by this function's contract, and both arrays are plain integers that
    // FSEvents initialized.
    unsafe { (*flags.add(idx), *ids.add(idx)) }
}

/// The `path` string of an extended-data event dictionary.
fn extended_data_path(dict: &CFDictionary<CFString>) -> PathBuf {
    let value = *dict.get(extended_data_path_key());
    // SAFETY: FSEvents documents the `path` value as a `CFStringRef`, borrowed from the
    // dictionary (Get rule): `from_void` wraps it without a retain, and the wrapper doesn't
    // outlive `dict`.
    let path = unsafe { CFString::from_void(value) };
    PathBuf::from(path.to_string())
}

define_callback!(cf_ext_with_id_callback, (num, paths, flags, ids){
    // SAFETY: With `UseExtendedData`, `paths` is a `CFArrayRef` of `CFDictionaryRef` that
    // FSEvents keeps alive for the callback (see the macro docs); `from_void` borrows it
    // without a retain, and the iterator holding it is collected before the callback returns.
    let paths = unsafe { CFArray::<CFDictionary<CFString>>::from_void(paths) };
    (0..num).map(move |idx| {
        // SAFETY: `idx < num`, the length of `paths`, `flags`, and `ids` alike.
        let dict = unsafe { paths.get_unchecked(idx as CFIndex) };
        // SAFETY: `idx < num`, the length of both arrays.
        let (flags, id) = unsafe { flags_and_id(flags, ids, idx) };
        let file_id = *dict.get(extended_file_id_key());
        // SAFETY: FSEvents documents the `fileID` value as a `CFNumberRef`, borrowed from
        // `dict` (Get rule) for no longer than this closure.
        let inode = unsafe { CFNumber::from_void(file_id) }
            .to_i64()
            .ok_or(CallbackError::ToI64)?;
        Ok(Event {
            path: extended_data_path(&dict),
            inode: Some(inode),
            flags: StreamFlags::from_bits_retain(flags),
            raw_flags: flags,
            id,
        })
    })
});

define_callback!(cf_ext_callback, (num, paths, flags, ids){
    // SAFETY: As in `cf_ext_with_id_callback`: with `UseExtendedData`, `paths` is a borrowed
    // `CFArrayRef` of `CFDictionaryRef`, alive for the callback.
    let paths = unsafe { CFArray::<CFDictionary<CFString>>::from_void(paths) };
    (0..num).map(move |idx| {
        // SAFETY: `idx < num`, the length of `paths`, `flags`, and `ids` alike.
        let dict = unsafe { paths.get_unchecked(idx as CFIndex) };
        // SAFETY: `idx < num`, the length of both arrays.
        let (flags, id) = unsafe { flags_and_id(flags, ids, idx) };
        Ok(Event {
            path: extended_data_path(&dict),
            inode: None,
            flags: StreamFlags::from_bits_retain(flags),
            raw_flags: flags,
            id,
        })
    })
});

define_callback!(cf_callback, (num, paths, flags, ids){
    // SAFETY: With `UseCFTypes` (and no extended data), `paths` is a `CFArrayRef` of
    // `CFStringRef` that FSEvents keeps alive for the callback; `from_void` borrows it without
    // a retain, and the iterator holding it is collected before the callback returns.
    let paths = unsafe { CFArray::<CFString>::from_void(paths) };
    (0..num).map(move |idx| {
        // SAFETY: `idx < num`, the length of `paths`, `flags`, and `ids` alike.
        let path = unsafe { paths.get_unchecked(idx as CFIndex) };
        // SAFETY: `idx < num`, the length of both arrays.
        let (flags, id) = unsafe { flags_and_id(flags, ids, idx) };
        Ok(Event {
            path: PathBuf::from(path.to_string()),
            inode: None,
            flags: StreamFlags::from_bits_retain(flags),
            raw_flags: flags,
            id,
        })
    })
});

define_callback!(normal_callback, (num, paths, flags, ids){
    // Without `UseCFTypes`, `paths` is a C array of `num` C strings, freed by FSEvents after
    // the callback returns.
    let paths = paths as *const *const c_char;
    (0..num).map(move |idx| {
        // SAFETY: `idx < num`, the length of the `paths` array FSEvents passed.
        let path = unsafe { *paths.add(idx) };
        // SAFETY: `idx < num`, the length of both arrays.
        let (flags, id) = unsafe { flags_and_id(flags, ids, idx) };
        // SAFETY: Each entry is a non-NULL, NUL-terminated C string that FSEvents keeps alive
        // until the callback returns; the bytes are copied into an owned `PathBuf` right here.
        let path = unsafe { CStr::from_ptr(path) };
        Ok(Event {
            path: PathBuf::from(OsStr::from_bytes(path.to_bytes())),
            inode: None,
            flags: StreamFlags::from_bits_retain(flags),
            raw_flags: flags,
            id,
        })
    })
});

#[cfg(test)]
mod callback_tests {
    use std::ffi::CString;

    use super::*;

    /// A flag bit macOS might add later must not cost us the event: the watcher still needs its
    /// known bits (created, removed, renamed) to keep the index in step.
    #[test]
    fn event_with_unknown_flag_bit_survives_with_its_known_flags() {
        let (tx, mut rx) = tokio::sync::mpsc::channel(4);
        let overflow = Arc::new(AtomicBool::new(false));
        let mut context = StreamContextInfo {
            event_handler: tx,
            overflow: overflow.clone(),
        };

        let path = CString::new("/tmp/new-file").unwrap();
        let paths = [path.as_ptr()];
        let unknown_bit = 0x8000_0000;
        let known = StreamFlags::ITEM_CREATED | StreamFlags::IS_FILE;
        let flags = [known.bits() | unknown_bit];
        let ids = [42];

        normal_callback(
            std::ptr::null_mut(),
            std::ptr::addr_of_mut!(context).cast(),
            1,
            paths.as_ptr() as *mut c_void,
            flags.as_ptr(),
            ids.as_ptr(),
        );

        let batch = rx.try_recv().expect("callback sent a batch");
        assert_eq!(batch.len(), 1, "the event survived the parse");
        let event = &batch[0];
        assert!(event.flags.contains(known));
        assert_eq!(event.raw_flags, flags[0]);
        assert_eq!(event.path, PathBuf::from("/tmp/new-file"));
        assert_eq!(event.id, 42);
        assert!(!overflow.load(Ordering::Relaxed));
    }

    /// Regression: a panic inside the callback (here `CFDictionary::get` on an extended-data
    /// dictionary with no `path` key) used to drop the batch without raising the overflow flag,
    /// so the consumer never learned to rescan.
    #[test]
    fn panic_while_building_a_batch_flags_overflow() {
        use core_foundation::base::{CFType, TCFType};

        let (tx, mut rx) = tokio::sync::mpsc::channel(4);
        let overflow = Arc::new(AtomicBool::new(false));
        let mut context = StreamContextInfo {
            event_handler: tx,
            overflow: overflow.clone(),
        };

        let no_keys = CFDictionary::<CFString, CFType>::from_CFType_pairs(&[]);
        let paths = CFArray::from_CFTypes(&[no_keys]);
        let flags = [StreamFlags::ITEM_CREATED.bits()];
        let ids = [7];

        cf_ext_callback(
            std::ptr::null_mut(),
            std::ptr::addr_of_mut!(context).cast(),
            1,
            paths.as_concrete_TypeRef() as *mut c_void,
            flags.as_ptr(),
            ids.as_ptr(),
        );

        assert!(rx.try_recv().is_err(), "the batch was dropped");
        assert!(overflow.load(Ordering::Relaxed), "and the drop was flagged");
    }
}
