//! The `NSDraggingSource` Cmdr publishes for every native drag.
//!
//! Two jobs:
//!
//! 1. **Operation mask** — `draggingSession:sourceOperationMaskForDraggingContext:`
//!    returns a permissive `Copy | Link | Generic | Move` mask so destinations
//!    (Finder, terminals) can pick the operation they support. Restricting it
//!    up front makes terminals reject the drop (they only accept Copy).
//! 2. **Session-end cleanup** — `draggingSession:endedAtPoint:operation:` tells
//!    the file-promise machinery the gesture is over, so a virtual session's
//!    delegates/providers can be freed once any in-flight fulfillment drains
//!    (see [`super::promises`]'s delegate-lifetime model).
//!
//! Each drag builds a fresh source carrying its own `session_key` (the same key
//! the session's promise delegates were registered under). AppKit retains the
//! source for the gesture's lifetime, so reading the key back in the end
//! callback is sound. A local-session drag uses `NO_PROMISE_SESSION` as its key,
//! making the end callback a no-op (there's nothing to free).

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send};
use objc2_app_kit::{NSDragOperation, NSDraggingContext, NSDraggingSession, NSDraggingSource};
use objc2_foundation::{NSObject, NSObjectProtocol, NSPoint};

/// Permissive operation mask published to the destination: `Copy | Link | Generic | Move`.
/// macOS arbitrates the chosen operation based on modifier keys (Alt → Copy, Cmd → Move,
/// Ctrl-Alt → Link) and the destination's preference. Restricting the mask up-front makes
/// destinations like Warp reject the drop entirely (terminals only accept Copy).
const PERMISSIVE_OP_MASK: usize = 1 | 2 | 4 | 16;

/// Sentinel session key for a local-session drag (no file promises to free).
/// The end callback is a no-op for this key.
pub(super) const NO_PROMISE_SESSION: isize = isize::MIN;

/// Ivar: the promise-session key this drag was registered under (or
/// [`NO_PROMISE_SESSION`] for a local drag with no promises).
pub(super) struct SourceIvars {
    session_key: isize,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[name = "CmdrDragSource"]
    #[thread_kind = MainThreadOnly]
    #[ivars = SourceIvars]
    pub(super) struct CmdrDragSource;

    unsafe impl NSObjectProtocol for CmdrDragSource {}

    unsafe impl NSDraggingSource for CmdrDragSource {
        #[unsafe(method(draggingSession:sourceOperationMaskForDraggingContext:))]
        fn operation_mask(&self, _session: &NSDraggingSession, _context: NSDraggingContext) -> NSDragOperation {
            NSDragOperation(PERMISSIVE_OP_MASK)
        }

        #[unsafe(method(draggingSession:endedAtPoint:operation:))]
        fn session_ended(&self, _session: &NSDraggingSession, _screen_point: NSPoint, _operation: NSDragOperation) {
            let key = self.ivars().session_key;
            if key != NO_PROMISE_SESSION {
                super::promises::mark_gesture_ended(key);
            }
        }
    }
);

impl CmdrDragSource {
    fn new(mtm: MainThreadMarker, session_key: isize) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(SourceIvars { session_key });
        // SAFETY: standard NSObject init chain.
        unsafe { msg_send![super(this), init] }
    }
}

/// Builds a drag source for a session keyed by `session_key` (or
/// [`NO_PROMISE_SESSION`] for a local drag). Returned as an `AnyObject` because
/// the call site passes it to `beginDraggingSessionWithItems:event:source:` via
/// raw `msg_send!`.
pub(super) fn build_drag_source(mtm: MainThreadMarker, session_key: isize) -> Retained<AnyObject> {
    let source = CmdrDragSource::new(mtm, session_key);
    // SAFETY: CmdrDragSource is an NSObject subclass; the cast preserves the
    // pointer. The begin-session call only sends NSDraggingSource selectors,
    // all of which the class implements.
    unsafe { Retained::cast_unchecked(source) }
}
