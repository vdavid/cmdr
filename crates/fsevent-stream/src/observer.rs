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
    let ctx = Box::into_raw(Box::new(CFRunLoopObserverContext {
        version: 0,
        info: Box::into_raw(Box::new(ObserverContextInfo { interest, tx })).cast(),
        retain: None,
        release: Some(release_observer_ctx),
        copyDescription: None,
    }));
    unsafe {
        CFRunLoopObserver::wrap_under_create_rule(CFRunLoopObserverCreate(
            kCFAllocatorDefault,
            interest,
            0,
            0,
            observer_callback,
            ctx,
        ))
    }
}
