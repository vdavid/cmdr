//! The private DiskArbitration symbol the approver needs, resolved at runtime.
//!
//! `DARegisterIdleCallback` (`DiskArbitrationPrivate.h:331`) is how a client hears that DA's queue
//! went quiet, which is the only signal that a REFUSED unmount settled: no public callback marks a
//! refusal to an observer. ❌ Never linked against: a missing symbol costs the resume, never the
//! build or the launch.

use std::ffi::c_void;
use std::sync::OnceLock;

use objc2_disk_arbitration::DASession;

/// `typedef void (*DAIdleCallback)(void * context)`.
pub(super) type IdleCallback = unsafe extern "C-unwind" fn(context: *mut c_void);

/// `void DARegisterIdleCallback(DASessionRef, DAIdleCallback, void * context)`.
pub(super) type RegisterIdleCallback =
    unsafe extern "C-unwind" fn(session: &DASession, callback: IdleCallback, context: *mut c_void);

/// `DARegisterIdleCallback`, or `None` on a macOS that doesn't export it.
pub(super) fn register_idle_callback() -> Option<RegisterIdleCallback> {
    static SYMBOL: OnceLock<Option<RegisterIdleCallback>> = OnceLock::new();
    *SYMBOL.get_or_init(|| {
        // SAFETY: `RTLD_DEFAULT` searches the images already loaded, and the name is a NUL-
        // terminated C string literal. A missing symbol answers null, which is checked below.
        let symbol = unsafe { libc::dlsym(libc::RTLD_DEFAULT, c"DARegisterIdleCallback".as_ptr()) };
        if symbol.is_null() {
            return None;
        }
        // SAFETY: the symbol resolved from DiskArbitration is that function, whose signature is
        // `RegisterIdleCallback`'s. A pointer to a function and a function pointer have the same
        // size and representation on every platform Cmdr builds for.
        Some(unsafe { std::mem::transmute::<*mut c_void, RegisterIdleCallback>(symbol) })
    })
}
