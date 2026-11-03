#![allow(clippy::module_name_repetitions)]

use std::ffi::c_void;
use std::panic::catch_unwind;
use std::sync::mpsc::Sender;

use core_foundation::base::{kCFAllocatorDefault, TCFType};
use core_foundation::runloop::{
    CFRunLoopActivity, CFRunLoopObserver, CFRunLoopObserverContext, CFRunLoopObserverCreate,
    CFRunLoopObserverRef,
};

use crate::utils::FlagsExt;

pub struct ObserverContextInfo {
    interest: CFRunLoopActivity,
    tx: Sender<CFRunLoopActivity>,
}

impl_release_callback!(release_observer_ctx, const ObserverContextInfo);

extern "C" fn observer_callback(
    _observer: CFRunLoopObserverRef,
    activity: CFRunLoopActivity,
    info: *mut c_void,
) {
    drop(catch_unwind(move || {
        let ctx: &ObserverContextInfo = unsafe { &*(info.cast()) };
        if ctx.interest.contains(activity) {
            let _ = ctx.tx.send(activity);
        }
    }));
}

pub fn create_oneshot_observer(
    interest: CFRunLoopActivity,
    tx: Sender<CFRunLoopActivity>,
) -> CFRunLoopObserver {
    // On the stack: `CFRunLoopObserverCreate` copies the struct out, so a boxed one leaked.
    let mut ctx = CFRunLoopObserverContext {
        version: 0,
        info: Box::into_raw(Box::new(ObserverContextInfo { interest, tx })).cast(),
        retain: None,
        release: Some(release_observer_ctx),
        copyDescription: None,
    };
    // SAFETY: `ctx` is a valid, initialized `CFRunLoopObserverContext` for the duration of the
    // call, and CF copies its fields rather than keeping the pointer. With `retain` NULL, the
    // observer takes over the boxed `info` and calls `release_observer_ctx` on it exactly once,
    // when the observer is deallocated. `CFRunLoopObserverCreate` returns a +1 reference (Create
    // rule), which `wrap_under_create_rule` adopts, so dropping the wrapper is the matching
    // `CFRelease` (it also asserts the ref isn't NULL, which only allocation failure produces).
    unsafe {
        CFRunLoopObserver::wrap_under_create_rule(CFRunLoopObserverCreate(
            kCFAllocatorDefault,
            interest,
            0,
            0,
            observer_callback,
            &mut ctx,
        ))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc::{channel, RecvTimeoutError};
    use std::time::Duration;

    use core_foundation::runloop::kCFRunLoopBeforeWaiting;

    use super::create_oneshot_observer;

    /// The observer owns its info: dropping the last reference releases it (and with it the
    /// sender), so a receiver waiting on a dead observer sees a disconnect instead of hanging.
    #[test]
    fn dropping_the_observer_releases_its_context() {
        let (tx, rx) = channel();
        let observer = create_oneshot_observer(kCFRunLoopBeforeWaiting, tx);
        drop(observer);
        assert_eq!(
            rx.recv_timeout(Duration::from_secs(5)),
            Err(RecvTimeoutError::Disconnected)
        );
    }
}
