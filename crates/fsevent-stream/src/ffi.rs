//! Raw `FSEvents` ffi bindings.
//!
//! Some of the bindings are lightly wrapped to adapt to Rust's ownership model and provide a more ergonomic experience.

#![allow(
    non_snake_case,
    non_upper_case_globals,
    clippy::unreadable_literal,
    clippy::declare_interior_mutable_const
)]

use std::ffi::c_void;
use std::io;
use std::marker::{PhantomData, PhantomPinned};
use std::os::raw::c_uint;
use std::path::Path;
use std::time::Duration;

use core_foundation::array::{CFArray, CFArrayRef};
use core_foundation::base::{
    kCFAllocatorDefault, Boolean, CFAllocatorCopyDescriptionCallBack, CFAllocatorRef,
    CFAllocatorReleaseCallBack, CFAllocatorRetainCallBack, CFIndex, TCFType,
};
use core_foundation::date::CFTimeInterval;
use core_foundation::runloop::{CFRunLoop, CFRunLoopIsWaiting, CFRunLoopMode, CFRunLoopRef};
use core_foundation::string::{CFString, CFStringRef};
use core_foundation::url::{kCFURLPOSIXPathStyle, CFURL};
use once_cell::unsync::Lazy;

fn str_path_to_cfstring_ref(source: &Path) -> io::Result<CFString> {
    CFURL::from_path(source, source.is_dir())
        .ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))
        .map(|path| path.absolute().get_file_system_path(kCFURLPOSIXPathStyle))
}

pub(crate) trait CFRunLoopExt {
    fn is_waiting(&self) -> bool;
}

impl CFRunLoopExt for CFRunLoop {
    fn is_waiting(&self) -> bool {
        // SAFETY: `self` holds a retained, valid `CFRunLoopRef` for the call. `CFRunLoopIsWaiting`
        // only reads the run loop's state and is callable from any thread (CF run-loop
        // functions are thread-safe, Threading Programming Guide § Run Loops).
        unsafe { CFRunLoopIsWaiting(self.as_concrete_TypeRef()) != 0 }
    }
}

#[doc(hidden)]
#[repr(C)]
pub struct __FSEventStream {
    _data: [u8; 0],
    _marker: PhantomData<(*mut u8, PhantomPinned)>,
}

pub type SysFSEventStreamRef = *mut __FSEventStream;

/// An ergonomic wrapper of [`SysFSEventStreamRef`](SysFSEventStreamRef).
///
/// This wrapper complies with Rust's ownership model, and releases its resource when dropped.
///
/// Invariant: the inner ref is non-NULL and owned (+1, from `FSEventStreamCreate`), which `new`
/// guarantees. The methods are safe because FSEvents validates call order itself: `start` on an
/// unscheduled stream returns false, and `stop`, `flush_sync`, and a repeated `invalidate` are
/// no-ops (verified on macOS 27.0.1, a probe test calling each out of order, 2026-11-03).
pub struct SysFSEventStream(SysFSEventStreamRef);

// SAFETY: The wrapper owns its stream ref outright and isn't `Sync`, so moving it hands the stream
// to exactly one thread at a time; `FSEventStream` objects aren't tied to the thread that created
// them. `create_event_stream` creates it on the caller's thread and then uses it only on the
// worker, where it's scheduled, run, invalidated, and released.
unsafe impl Send for SysFSEventStream {}

pub type FSEventStreamCallback = extern "C" fn(
    SysFSEventStreamRef,            // ConstFSEventStreamRef streamRef
    *mut c_void,                    // void *clientCallBackInfo
    usize,                          // size_t numEvents
    *mut c_void,                    // void *eventPaths
    *const FSEventStreamEventFlags, // const FSEventStreamEventFlags eventFlags[]
    *const FSEventStreamEventId,    // const FSEventStreamEventId eventIds[]
);

pub type FSEventStreamEventId = u64;

pub type FSEventStreamCreateFlags = c_uint;

pub type FSEventStreamEventFlags = c_uint;

pub const kFSEventStreamEventIdSinceNow: FSEventStreamEventId = 0xFFFFFFFFFFFFFFFF;

pub const kFSEventStreamCreateFlagNone: FSEventStreamCreateFlags = 0x00000000;
pub const kFSEventStreamCreateFlagUseCFTypes: FSEventStreamCreateFlags = 0x00000001;
pub const kFSEventStreamCreateFlagNoDefer: FSEventStreamCreateFlags = 0x00000002;
pub const kFSEventStreamCreateFlagWatchRoot: FSEventStreamCreateFlags = 0x00000004;
pub const kFSEventStreamCreateFlagIgnoreSelf: FSEventStreamCreateFlags = 0x00000008;
pub const kFSEventStreamCreateFlagFileEvents: FSEventStreamCreateFlags = 0x00000010;
pub const kFSEventStreamCreateFlagMarkSelf: FSEventStreamCreateFlags = 0x00000020;
pub const kFSEventStreamCreateFlagUseExtendedData: FSEventStreamCreateFlags = 0x00000040;

pub const kFSEventStreamEventFlagNone: FSEventStreamEventFlags = 0x00000000;
pub const kFSEventStreamEventFlagMustScanSubDirs: FSEventStreamEventFlags = 0x00000001;
pub const kFSEventStreamEventFlagUserDropped: FSEventStreamEventFlags = 0x00000002;
pub const kFSEventStreamEventFlagKernelDropped: FSEventStreamEventFlags = 0x00000004;
pub const kFSEventStreamEventFlagEventIdsWrapped: FSEventStreamEventFlags = 0x00000008;
pub const kFSEventStreamEventFlagHistoryDone: FSEventStreamEventFlags = 0x00000010;
pub const kFSEventStreamEventFlagRootChanged: FSEventStreamEventFlags = 0x00000020;
pub const kFSEventStreamEventFlagMount: FSEventStreamEventFlags = 0x00000040;
pub const kFSEventStreamEventFlagUnmount: FSEventStreamEventFlags = 0x00000080;
pub const kFSEventStreamEventFlagItemCreated: FSEventStreamEventFlags = 0x00000100;
pub const kFSEventStreamEventFlagItemRemoved: FSEventStreamEventFlags = 0x00000200;
pub const kFSEventStreamEventFlagItemInodeMetaMod: FSEventStreamEventFlags = 0x00000400;
pub const kFSEventStreamEventFlagItemRenamed: FSEventStreamEventFlags = 0x00000800;
pub const kFSEventStreamEventFlagItemModified: FSEventStreamEventFlags = 0x00001000;
pub const kFSEventStreamEventFlagItemFinderInfoMod: FSEventStreamEventFlags = 0x00002000;
pub const kFSEventStreamEventFlagItemChangeOwner: FSEventStreamEventFlags = 0x00004000;
pub const kFSEventStreamEventFlagItemXattrMod: FSEventStreamEventFlags = 0x00008000;
pub const kFSEventStreamEventFlagItemIsFile: FSEventStreamEventFlags = 0x00010000;
pub const kFSEventStreamEventFlagItemIsDir: FSEventStreamEventFlags = 0x00020000;
pub const kFSEventStreamEventFlagItemIsSymlink: FSEventStreamEventFlags = 0x00040000;
pub const kFSEventStreamEventFlagOwnEvent: FSEventStreamEventFlags = 0x00080000;
pub const kFSEventStreamEventFlagItemIsHardlink: FSEventStreamEventFlags = 0x00100000;
pub const kFSEventStreamEventFlagItemIsLastHardlink: FSEventStreamEventFlags = 0x00200000;
pub const kFSEventStreamEventFlagItemCloned: FSEventStreamEventFlags = 0x00400000;

pub const kFSEventStreamEventExtendedDataPathKey: Lazy<CFString> =
    Lazy::new(|| CFString::new("path"));
pub const kFSEventStreamEventExtendedFileIDKey: Lazy<CFString> =
    Lazy::new(|| CFString::new("fileID"));

#[repr(C)]
pub struct SysFSEventStreamContext {
    pub version: CFIndex,
    pub info: *mut c_void,
    pub retain: Option<CFAllocatorRetainCallBack>,
    pub release: Option<CFAllocatorReleaseCallBack>,
    pub copy_description: Option<CFAllocatorCopyDescriptionCallBack>,
}

/// Generate a callback that free the context when the stream created by [`SysFSEventStream::new`](SysFSEventStream::new) is released.
///
/// Usage: `impl_release_callback!(release_ctx, YourCtxType)`
///
/// The generated function is sound only as the `release` of a context whose `info` came from
/// `Box::into_raw(Box::<YourCtxType>::new(..))` (as [`SysFSEventStreamContext::new`] makes it) and
/// whose `retain` is NULL: CF then calls `release` exactly once, when the owning object is
/// deallocated, and never touches `info` again.
#[macro_export]
macro_rules! impl_release_callback {
    ($name: ident, $ctx_ty: ty) => {
        extern "C" fn $name(ctx: *mut std::ffi::c_void) {
            // SAFETY: Per the macro's contract, `ctx` is the `Box<$ctx_ty>` leaked into the
            // context's `info`, and with `retain` NULL, CF calls this exactly once, when the
            // stream or observer is deallocated (`FSEventStreamContext` docs). Nothing reads
            // `info` after that, so reclaiming the box here is the single matching free.
            unsafe {
                drop(Box::from_raw(ctx as *mut $ctx_ty));
            }
        }
    };
    ($name: ident, const $ctx_ty: ty) => {
        extern "C" fn $name(ctx: *const std::ffi::c_void) {
            // SAFETY: As in the arm above: the one release of the `Box<$ctx_ty>` behind `info`,
            // called by CF once, when its owner is deallocated (`CFRunLoopObserverContext`).
            unsafe {
                drop(Box::from_raw(ctx as *mut $ctx_ty));
            }
        }
    };
}

impl SysFSEventStreamContext {
    /// Create a new [`SysFSEventStreamContext`](SysFSEventStreamContext).
    ///
    /// `release_callback` can be constructed using [`impl_release_callback`](impl_release_callback) macro.
    pub fn new<T>(ctx: T, release_callback: CFAllocatorReleaseCallBack) -> Self {
        let ctx = Box::into_raw(Box::new(ctx));
        Self {
            version: 0,
            info: ctx.cast(),
            retain: None,
            release: Some(release_callback),
            copy_description: None,
        }
    }
}

impl SysFSEventStream {
    /// Create a new [`SysFSEventStream`](SysFSEventStream).
    ///
    /// Takes ownership of `context.info` whatever the outcome: on success the stream calls
    /// `context.release` when it's deallocated, and on failure it has been called by the time this
    /// returns. So a context must go to exactly one `new` call.
    ///
    /// # Errors
    /// Return error when `paths_to_watch` is empty or holds an invalid path, or when `FSEvents`
    /// refuses to create the stream.
    pub fn new<P: AsRef<Path>>(
        callback: FSEventStreamCallback,
        context: &SysFSEventStreamContext,
        paths_to_watch: impl IntoIterator<Item = P>,
        since_when: FSEventStreamEventId,
        latency: Duration,
        flags: FSEventStreamCreateFlags,
    ) -> io::Result<Self> {
        let cf_paths = paths_to_watch
            .into_iter()
            .map(|item| str_path_to_cfstring_ref(item.as_ref()))
            .collect::<io::Result<Vec<_>>>()
            .and_then(|cf_paths| {
                // `FSEventStreamCreate` returns NULL for an empty list; reject it here so the
                // error says why.
                if cf_paths.is_empty() {
                    Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "no paths to watch",
                    ))
                } else {
                    Ok(cf_paths)
                }
            });
        let cf_paths = match cf_paths {
            Ok(cf_paths) => cf_paths,
            Err(err) => {
                // FSEvents never saw the context, so its info is still ours to release.
                if let Some(release) = context.release {
                    release(context.info);
                }
                return Err(err);
            }
        };
        let cf_path_array = CFArray::from_CFTypes(&cf_paths);
        // SAFETY: `context` points to a live, fully initialized `FSEventStreamContext` (version 0)
        // for the duration of the call, and FSEvents copies it rather than keeping the pointer.
        // `cf_path_array` is a valid `CFArray` of `CFString`s that FSEvents retains as needed.
        // `callback` is an `extern "C" fn` with the `FSEventStreamCallback` signature.
        let stream = unsafe {
            FSEventStreamCreate(
                kCFAllocatorDefault,
                callback,
                context,
                cf_path_array.as_concrete_TypeRef(),
                since_when,
                latency.as_secs_f64() as CFTimeInterval,
                flags,
            )
        };
        if stream.is_null() {
            // Don't release `context.info` here: FSEvents already did before returning NULL
            // (verified on macOS 27.0.1, a probe test with a logging release callback,
            // 2026-11-03), so releasing it again would double free.
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "FSEventStreamCreate returned no stream",
            ));
        }
        Ok(Self(stream))
    }
    pub fn show(&mut self) {
        // SAFETY: `self.0` is a valid, owned stream ref (type invariant); `FSEventStreamShow`
        // only prints the stream's description to stderr.
        unsafe { FSEventStreamShow(self.0) }
    }
    /// # Safety
    /// `run_loop_mode` must be a valid `CFStringRef` (for example, `kCFRunLoopDefaultMode`).
    pub unsafe fn schedule(&mut self, run_loop: &CFRunLoop, run_loop_mode: CFStringRef) {
        // SAFETY: `self.0` is a valid stream ref (type invariant), `run_loop` is a retained
        // `CFRunLoopRef` for the call, and the caller vouches for `run_loop_mode`. From here on
        // FSEvents calls back on `run_loop`'s thread, which is fine for any context we build:
        // `StreamContextInfo` is `Sync`.
        unsafe {
            FSEventStreamScheduleWithRunLoop(self.0, run_loop.as_concrete_TypeRef(), run_loop_mode);
        }
    }
    /// # Safety
    /// `run_loop_mode` must be a valid `CFStringRef` (for example, `kCFRunLoopDefaultMode`).
    pub unsafe fn unschedule(&mut self, run_loop: &CFRunLoop, run_loop_mode: CFStringRef) {
        // SAFETY: As in `schedule`: a valid stream ref, a retained run loop, and a mode the
        // caller vouches for.
        unsafe {
            FSEventStreamUnscheduleFromRunLoop(
                self.0,
                run_loop.as_concrete_TypeRef(),
                run_loop_mode,
            );
        }
    }
    pub fn start(&mut self) -> bool {
        // SAFETY: `self.0` is a valid stream ref (type invariant). FSEvents rejects a start on an
        // unscheduled stream by returning false (see the type docs).
        unsafe { FSEventStreamStart(self.0) != 0 }
    }
    pub fn flush_sync(&mut self) {
        // SAFETY: `self.0` is a valid stream ref (type invariant). Pending events are delivered
        // synchronously on this thread, which the `Sync` context tolerates.
        unsafe { FSEventStreamFlushSync(self.0) };
    }
    pub fn stop(&mut self) {
        // SAFETY: `self.0` is a valid stream ref (type invariant); stopping a stream that isn't
        // started is a no-op.
        unsafe { FSEventStreamStop(self.0) };
    }
    pub fn invalidate(&mut self) {
        // SAFETY: `self.0` is a valid stream ref (type invariant). After this returns the stream
        // is unscheduled from every run loop, so no further callback can run.
        unsafe { FSEventStreamInvalidate(self.0) };
    }
}

impl Drop for SysFSEventStream {
    fn drop(&mut self) {
        // SAFETY: `self.0` is the +1 reference `FSEventStreamCreate` returned (Create rule) and
        // this is its one matching release. When the count reaches zero FSEvents calls the
        // context's `release` on `info`; `create_event_stream` stops and invalidates the stream
        // first, so no callback can be running or come later when that frees the context.
        unsafe { FSEventStreamRelease(self.0) };
    }
}

#[link(name = "CoreServices", kind = "framework")]
extern "C" {
    fn FSEventStreamCreate(
        allocator: CFAllocatorRef,
        callback: FSEventStreamCallback,
        context: *const SysFSEventStreamContext,
        pathsToWatch: CFArrayRef,
        sinceWhen: FSEventStreamEventId,
        latency: CFTimeInterval,
        flags: FSEventStreamCreateFlags,
    ) -> SysFSEventStreamRef;

    fn FSEventStreamShow(stream_ref: SysFSEventStreamRef);
    fn FSEventStreamScheduleWithRunLoop(
        stream_ref: SysFSEventStreamRef,
        run_loop: CFRunLoopRef,
        run_loop_mode: CFRunLoopMode,
    );

    fn FSEventStreamUnscheduleFromRunLoop(
        stream_ref: SysFSEventStreamRef,
        run_loop: CFRunLoopRef,
        run_loop_mode: CFRunLoopMode,
    );

    fn FSEventStreamStart(stream_ref: SysFSEventStreamRef) -> Boolean;
    fn FSEventStreamFlushSync(stream_ref: SysFSEventStreamRef);
    fn FSEventStreamStop(stream_ref: SysFSEventStreamRef);
    fn FSEventStreamInvalidate(stream_ref: SysFSEventStreamRef);
    fn FSEventStreamRelease(stream_ref: SysFSEventStreamRef);

    pub fn FSEventsGetCurrentEventId() -> FSEventStreamEventId;

    /// The UUID of the FSEvents journal on device `dev` (a `dev_t`), or null when
    /// fseventsd has none for it. Create rule: the caller owns the returned ref.
    pub fn FSEventsCopyUUIDForDevice(dev: std::os::raw::c_int) -> core_foundation::uuid::CFUUIDRef;
}
