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
pub struct SysFSEventStream(SysFSEventStreamRef);

// Safety:
// - According to the Apple documentation, it's safe to move `CFRef`s across threads.
//   https://developer.apple.com/library/archive/documentation/Cocoa/Conceptual/Multithreading/ThreadSafetySummary/ThreadSafetySummary.html
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
// Safety:
// - The [documentation] for `FSEventStreamContext` states that `release` is only
//   called when the stream is deallocated, so it is safe to convert `info` back into a
//   box and drop it.
//
// [docs]: https://developer.apple.com/documentation/coreservices/fseventstreamcontext?language=objc
#[macro_export]
macro_rules! impl_release_callback {
    ($name: ident, $ctx_ty: ty) => {
        extern "C" fn $name(ctx: *mut std::ffi::c_void) {
            unsafe {
                drop(Box::from_raw(ctx as *mut $ctx_ty));
            }
        }
    };
    ($name: ident, const $ctx_ty: ty) => {
        extern "C" fn $name(ctx: *const std::ffi::c_void) {
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
    /// # Errors
    /// Return error when there's any invalid path in `paths_to_watch`.
    pub fn new<P: AsRef<Path>>(
        callback: FSEventStreamCallback,
        context: &SysFSEventStreamContext,
        paths_to_watch: impl IntoIterator<Item = P>,
        since_when: FSEventStreamEventId,
        latency: Duration,
        flags: FSEventStreamCreateFlags,
    ) -> io::Result<Self> {
        let cf_paths: Vec<_> = paths_to_watch
            .into_iter()
            .map(|item| str_path_to_cfstring_ref(item.as_ref()))
            .collect::<Result<_, _>>()?;
        let cf_path_array = CFArray::from_CFTypes(&cf_paths);
        Ok(Self(unsafe {
            FSEventStreamCreate(
                kCFAllocatorDefault,
                callback,
                context,
                cf_path_array.as_concrete_TypeRef(),
                since_when,
                latency.as_secs_f64() as CFTimeInterval,
                flags,
            )
        }))
    }
    pub fn show(&mut self) {
        unsafe { FSEventStreamShow(self.0) }
    }
    /// # Safety
    /// `run_loop_mode` must be a valid `CFStringRef` (for example, `kCFRunLoopDefaultMode`).
    pub unsafe fn schedule(&mut self, run_loop: &CFRunLoop, run_loop_mode: CFStringRef) {
        unsafe {
            FSEventStreamScheduleWithRunLoop(self.0, run_loop.as_concrete_TypeRef(), run_loop_mode);
        }
    }
    /// # Safety
    /// `run_loop_mode` must be a valid `CFStringRef` (for example, `kCFRunLoopDefaultMode`).
    pub unsafe fn unschedule(&mut self, run_loop: &CFRunLoop, run_loop_mode: CFStringRef) {
        unsafe {
            FSEventStreamUnscheduleFromRunLoop(
                self.0,
                run_loop.as_concrete_TypeRef(),
                run_loop_mode,
            );
        }
    }
    pub fn start(&mut self) -> bool {
        unsafe { FSEventStreamStart(self.0) != 0 }
    }
    pub fn flush_sync(&mut self) {
        unsafe { FSEventStreamFlushSync(self.0) };
    }
    pub fn stop(&mut self) {
        unsafe { FSEventStreamStop(self.0) };
    }
    pub fn invalidate(&mut self) {
        unsafe { FSEventStreamInvalidate(self.0) };
    }
}

impl Drop for SysFSEventStream {
    fn drop(&mut self) {
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
}
