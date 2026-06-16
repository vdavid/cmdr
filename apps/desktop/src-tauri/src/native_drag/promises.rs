//! macOS file-promise providers + delegate for dragging virtual files
//! (MTP/SMB) out to Finder.
//!
//! When a virtual-session drag starts (see [`super::start_drag`]), each dragged
//! item gets an [`NSFilePromiseProvider`] instead of a `file://` URL. The
//! provider carries a *promise*: it tells Finder "I'll produce this file's bytes
//! when you ask." Finder asks by calling our delegate's
//! `filePromiseProvider:writePromiseToURL:completionHandler:` on the provider's
//! operation queue; we stream the bytes off the device into the Finder-chosen
//! destination via the plain-Rust [`super::fulfillment`] service, then call the
//! completion block.
//!
//! ## The delegate is paper-thin; all real work is in `fulfillment`
//!
//! - `fileNameForType:` runs on the MAIN thread and returns the leaf name we
//!   already know — zero I/O, zero locks shared with fulfillment.
//! - `writePromiseToURL:completionHandler:` runs on the operation-queue thread.
//!   It `block_on`s the async fulfillment service (the queue is ours and
//!   serial, so blocking it is fine and the documented pattern) and calls the
//!   completion block with `null` on success or a mapped `NSError` on failure.
//! - `operationQueueForFilePromiseProvider:` returns ONE shared serial queue per
//!   drag session: MTP is serial USB anyway (parallel fulfillments would just
//!   contend on the device lock), ordering is predictable, and a session-level
//!   "N of M done" progress notion stays computable. SMB could parallelize;
//!   v1 favors one code path. To loosen for SMB later, build that session's
//!   queue with `maxConcurrentOperationCount > 1`.
//!
//! ## Main-thread invariant
//!
//! The fulfillment service never performs synchronous main-thread work from the
//! queue thread (it does volume I/O + a cheap downloads-watcher mutex, no
//! `run_on_main_thread`). So `block_on`-ing it on the queue thread can't
//! deadlock against a busy main thread. See [`super::fulfillment`]'s module doc.
//!
//! ## Delegate-lifetime model (load-bearing)
//!
//! `NSFilePromiseProvider.delegate` is a WEAK reference: the provider does NOT
//! retain its delegate. If the delegate were a drag-start local that drops when
//! `start_drag` returns, it would deallocate, the provider's weak `delegate`
//! would zero, and Finder would query a nil delegate and silently produce no
//! file. So each drag session's delegates + providers live in process-global,
//! MAIN-THREAD-CONFINED storage ([`with_retained_store`]), keyed by the drag
//! sequence number, and are freed only when BOTH:
//!
//! 1. the gesture has ended (`draggingSession:endedAtPoint:operation:` on the
//!    drag source — see [`super::source`]'s `session_ended`), AND
//! 2. every in-flight fulfillment has completed (each `writePromiseToURL` bumps
//!    an in-flight counter on entry and drops it after calling the completion
//!    handler).
//!
//! This defends against the ordering reality the plan flags: AppKit ends the
//! session at the DROP, but fulfillment runs AFTER (Finder pumps the queue once
//! the gesture is over). If we freed on session-end alone, an in-flight
//! fulfillment's provider could lose its delegate mid-write. Gating cleanup on
//! "ended AND in-flight == 0" keeps every object alive across both the gesture
//! and the (possibly later, queue-thread) fulfillment, without leaking
//! permanently. A fulfillment that completes after session-end triggers the
//! final cleanup itself when it drops the last in-flight count.
//!
//! ## Why two stores
//!
//! The `Retained<…>` AppKit objects are not `Send`, but the in-flight counter
//! and `gesture_ended` flag are touched from the queue thread. So they're split:
//!
//! - **[`COUNTERS`]** — plain `Send` bookkeeping (`in_flight`, `gesture_ended`),
//!   a `Mutex<HashMap>` any thread can touch. This decides *when* cleanup fires.
//! - **The retained store** — the `Retained` delegates + providers, accessed
//!   ONLY on the main thread (`with_retained_store`, asserts `MainThreadMarker`).
//!   Registration runs on main at drag-start; cleanup is dispatched back to main
//!   via [`run_on_main`] when the counters say "ended and drained." The shared
//!   serial queue rides in the delegate's ivar as a [`SendQueue`] (NSOperationQueue
//!   is documented thread-safe), so returning it from the queue thread needs no
//!   main-thread hop.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::{AllocAnyThread, DefinedClass, MainThreadMarker, define_class, msg_send};
use objc2_app_kit::{NSFilePromiseProvider, NSFilePromiseProviderDelegate};
use objc2_foundation::{NSError, NSObject, NSObjectProtocol, NSOperationQueue, NSString, NSURL};
use tauri::AppHandle;
use tauri_specta::Event as _;

use super::session_summary::{ItemOutcome, SessionSummary, summarize};
use crate::ignore_poison::IgnorePoison;
use crate::system_events::{SessionCompleteEvent, SessionStartedEvent};

/// The `NSError` domain for a drag-out fulfillment failure. Finder reads
/// `localizedDescription` and shows its own alert.
const ERROR_DOMAIN: &str = "com.veszelovszki.cmdr.drag-out";

/// The `drag-out-session-*` payload structs (`SessionStartedEvent` /
/// `SessionCompleteEvent`) live in the always-compiled `crate::system_events`
/// so `collect_events!` can reference them (this module is macOS-only). They're
/// typed `tauri_specta::Event`s; the emit sites below build and `.emit()` them.
impl SessionCompleteEvent {
    fn from_summary(session_key: isize, summary: SessionSummary) -> Self {
        Self {
            session_key: session_key as i64,
            files_succeeded: summary.files_succeeded,
            folders_succeeded: summary.folders_succeeded,
            failures: summary.failures,
        }
    }
}

// ============================================================================
// Global app handle (for dispatching cleanup back to the main thread)
// ============================================================================

static APP_HANDLE: OnceLock<AppHandle> = OnceLock::new();

/// Stores the app handle used to dispatch session cleanup to the main thread.
/// Called once at startup (`lib.rs::setup`).
pub fn set_app_handle(handle: AppHandle) {
    let _ = APP_HANDLE.set(handle);
}

/// Runs `f` on the AppKit main thread (async dispatch, never blocking). Used to
/// free the retained store off the queue thread without a sync main-thread hop
/// (which the main-thread invariant forbids). A no-op if the app handle isn't
/// set yet (only true before startup, when no drag can exist).
fn run_on_main(f: impl FnOnce() + Send + 'static) {
    if let Some(app) = APP_HANDLE.get()
        && let Err(e) = app.run_on_main_thread(f)
    {
        log::warn!(target: "drag_out", "run_on_main_thread for session cleanup failed: {e}");
    }
}

// ============================================================================
// Counters (Send bookkeeping; decides WHEN cleanup fires)
// ============================================================================

#[derive(Default)]
struct SessionCounters {
    gesture_ended: bool,
    in_flight: usize,
    /// Top-level dragged items in this session (seeded at provider build). Lets
    /// the started-toast read "Copying N items…" before any item finishes.
    total_items: usize,
    /// Whether the `drag-out-session-started` event already fired (the FIRST
    /// fulfillment to enter emits it; later ones don't re-emit).
    started_emitted: bool,
    /// Per-item outcomes, recorded as each fulfillment finishes. Folded into the
    /// `drag-out-session-complete` payload when the session drains.
    outcomes: Vec<ItemOutcome>,
}

/// Per-session `Send` bookkeeping, keyed by drag sequence number.
static COUNTERS: OnceLock<Mutex<HashMap<isize, SessionCounters>>> = OnceLock::new();

fn counters() -> &'static Mutex<HashMap<isize, SessionCounters>> {
    COUNTERS.get_or_init(|| Mutex::new(HashMap::new()))
}

// ============================================================================
// Retained store (main-thread-confined; the actual AppKit objects)
// ============================================================================

/// The retained AppKit objects for one session: the delegates the providers
/// reference WEAKLY (the thing we must keep alive), plus our own strong refs to
/// the providers for an explicit lifetime model. Accessed ONLY on the main
/// thread, so storing `!Send` `Retained` here is sound.
struct SessionObjects {
    #[allow(
        dead_code,
        reason = "Held strongly so the providers' weak delegate refs stay non-nil"
    )]
    delegates: Vec<Retained<PromiseDelegate>>,
    #[allow(
        dead_code,
        reason = "Held so providers outlive the gesture explicitly (AppKit also retains them)"
    )]
    providers: Vec<Retained<NSFilePromiseProvider>>,
}

thread_local! {
    /// The retained store. `thread_local!` confines it to one thread; every
    /// access asserts `MainThreadMarker::new()`, so this is always the main
    /// thread's local. Registration (drag-start) and cleanup (dispatched via
    /// `run_on_main`) both run on main.
    static RETAINED_STORE: RefCell<HashMap<isize, SessionObjects>> = RefCell::new(HashMap::new());
}

/// Runs `f` against the main-thread retained store. Panics (via `expect`) if
/// called off the main thread — a programming error, since every caller
/// dispatches to main first.
fn with_retained_store<R>(f: impl FnOnce(&mut HashMap<isize, SessionObjects>) -> R) -> R {
    let _mtm =
        MainThreadMarker::new().expect("the drag-out promise retained store must only be touched on the main thread");
    RETAINED_STORE.with(|store| f(&mut store.borrow_mut()))
}

/// Frees a session's retained objects if its counters say the gesture has ended
/// and no fulfillment is in flight, AFTER emitting the completion toast event.
/// Dispatches the actual removal to the main thread. Idempotent: a missing
/// session is a no-op.
fn maybe_free_session(key: isize) {
    let drained = {
        let map = counters().lock_ignore_poison();
        map.get(&key)
            .map(|c| c.gesture_ended && c.in_flight == 0)
            .unwrap_or(false)
    };
    if !drained {
        return;
    }
    // Remove the counters now (cheap, Send) so a late duplicate trigger can't
    // double-dispatch. Take the recorded outcomes with it for the toast.
    let outcomes = counters().lock_ignore_poison().remove(&key).map(|c| c.outcomes);
    if let Some(outcomes) = outcomes {
        emit_session_complete(key, &outcomes);
    }
    run_on_main(move || {
        let removed = with_retained_store(|store| store.remove(&key).is_some());
        if removed {
            log::debug!(target: "drag_out", "session {key} freed (gesture ended, fulfillments drained)");
        }
    });
}

/// Marks a session's gesture as ended (from the drag source's
/// `draggingSession:endedAtPoint:operation:`). Frees the session if no
/// fulfillment is in flight; otherwise the last finishing fulfillment frees it.
pub(super) fn mark_gesture_ended(key: isize) {
    counters().lock_ignore_poison().entry(key).or_default().gesture_ended = true;
    maybe_free_session(key);
}

/// Bumps the in-flight fulfillment count (on `writePromiseToURL` entry). The
/// FIRST fulfillment to enter a session emits `drag-out-session-started` so the
/// FE can raise the signs-of-life in-progress toast before any byte lands.
fn enter_fulfillment(key: isize) {
    let started = {
        let mut map = counters().lock_ignore_poison();
        let c = map.entry(key).or_default();
        c.in_flight += 1;
        if c.started_emitted {
            None
        } else {
            c.started_emitted = true;
            Some(c.total_items)
        }
    };
    if let Some(total_items) = started {
        emit_session_started(key, total_items);
    }
}

/// Records a finished fulfillment's outcome and drops the in-flight count. If
/// that was the last in-flight fulfillment and the gesture already ended, the
/// completion toast fires and the session frees.
fn leave_fulfillment(key: isize, outcome: ItemOutcome) {
    {
        let mut map = counters().lock_ignore_poison();
        if let Some(c) = map.get_mut(&key) {
            c.in_flight = c.in_flight.saturating_sub(1);
            c.outcomes.push(outcome);
        }
    }
    maybe_free_session(key);
}

/// Emits `drag-out-session-started` to the main window. A no-op (with a warn)
/// if the app handle isn't set or the emit fails — the toast is best-effort and
/// never blocks fulfillment.
fn emit_session_started(key: isize, total_items: usize) {
    let Some(app) = APP_HANDLE.get() else {
        return;
    };
    let payload = SessionStartedEvent {
        session_key: key as i64,
        total_items,
    };
    if let Err(e) = payload.emit(app) {
        log::warn!(target: "drag_out", "failed to emit drag-out-session-started: {e}");
    }
}

/// Emits `drag-out-session-complete` with the folded summary. Skips the emit
/// entirely for an empty session (the user dropped on a non-Finder target, so
/// nothing was ever fulfilled) — no toast for a clean no-op.
fn emit_session_complete(key: isize, outcomes: &[ItemOutcome]) {
    let summary = summarize(outcomes);
    if summary.is_empty() {
        return;
    }
    let Some(app) = APP_HANDLE.get() else {
        return;
    };
    let payload = SessionCompleteEvent::from_summary(key, summary);
    if let Err(e) = payload.emit(app) {
        log::warn!(target: "drag_out", "failed to emit drag-out-session-complete: {e}");
    }
}

// ============================================================================
// Send wrapper for the (thread-safe) NSOperationQueue ivar
// ============================================================================

/// A `Send + Sync` wrapper around a `Retained<NSOperationQueue>`.
///
/// `NSOperationQueue` is documented thread-safe (you can enqueue from any
/// thread; retain/release is atomic), and the delegate only ever returns the
/// queue (no main-thread-only method calls on it). Carrying it in the delegate's
/// ivars lets `operationQueueForFilePromiseProvider:` hand it back from any
/// thread without a main-thread hop or a SESSIONS lookup.
struct SendQueue(Retained<NSOperationQueue>);

// SAFETY: NSOperationQueue is thread-safe per Apple's docs; we only clone
// (atomic retain) and return it. No main-thread-only state is touched through it, so moving the
// wrapper across threads is sound.
unsafe impl Send for SendQueue {}
// SAFETY: As above, NSOperationQueue is thread-safe (enqueue from any thread, atomic
// retain/release), and `&SendQueue` only ever clones or returns the queue, so shared `&` access
// from multiple threads is sound.
unsafe impl Sync for SendQueue {}

// ============================================================================
// Delegate
// ============================================================================

/// Per-item delegate state. All `Send + Sync` so the delegate can be touched
/// from the operation-queue thread (the leaf + source identity are read-only
/// after construction; the queue is thread-safe).
pub(super) struct DelegateIvars {
    /// The exact filename Finder should use, returned by `fileNameForType:`
    /// with zero I/O. This is the leaf we already know at drag-start.
    leaf: String,
    /// The source volume id to resolve at fulfillment time.
    source_volume_id: String,
    /// The source path on that volume (volume-relative).
    source_path: PathBuf,
    /// The drag sequence number, so a finishing fulfillment can decrement the
    /// in-flight count (and trigger cleanup if the gesture already ended).
    session_key: isize,
    /// The shared serial queue for this session (thread-safe to return).
    queue: SendQueue,
}

define_class!(
    // NOT `MainThreadOnly`: `writePromiseToURL:completionHandler:` runs on the
    // operation-queue thread, so the delegate object must be usable off-main.
    // The main-thread-only method (`fileNameForType:`) gets its
    // `MainThreadMarker` from the protocol signature, so the class-level marker
    // isn't needed; the ivars are all `Send + Sync`.
    #[unsafe(super(NSObject))]
    #[name = "CmdrPromiseDelegate"]
    #[ivars = DelegateIvars]
    pub(super) struct PromiseDelegate;

    unsafe impl NSObjectProtocol for PromiseDelegate {}

    unsafe impl NSFilePromiseProviderDelegate for PromiseDelegate {
        /// Runs on the MAIN thread. Returns the known leaf name, no I/O.
        #[unsafe(method_id(filePromiseProvider:fileNameForType:))]
        fn file_name_for_type(&self, _provider: &NSFilePromiseProvider, _file_type: &NSString) -> Retained<NSString> {
            NSString::from_str(&self.ivars().leaf)
        }

        /// Runs on the operation-queue thread. Streams the source bytes into
        /// the Finder-chosen `url`, then calls the completion block (null =
        /// success, NSError = failure).
        #[unsafe(method(filePromiseProvider:writePromiseToURL:completionHandler:))]
        fn write_promise_to_url(
            &self,
            _provider: &NSFilePromiseProvider,
            url: &NSURL,
            completion_handler: &block2::DynBlock<dyn Fn(*mut NSError)>,
        ) {
            let ivars = self.ivars();
            let key = ivars.session_key;
            enter_fulfillment(key);

            let dest = url.path().map(|p| PathBuf::from(p.to_string())).unwrap_or_default();

            log::debug!(
                target: "drag_out",
                "writePromiseToURL: {} {} -> {}",
                ivars.source_volume_id,
                ivars.source_path.display(),
                dest.display()
            );

            // Block the (serial, ours) queue thread on the async service. The
            // service never hops to the main thread, so this can't deadlock.
            let result = tauri::async_runtime::block_on(super::fulfillment::fulfill(
                &ivars.source_volume_id,
                &ivars.source_path,
                &dest,
            ));

            // Record the per-item outcome for the session-complete toast, then
            // signal Finder (null NSError = success; a mapped NSError = failure).
            let item_outcome = match result {
                Ok(fulfilled) => {
                    completion_handler.call((std::ptr::null_mut(),));
                    ItemOutcome::Succeeded {
                        is_dir: fulfilled.is_dir,
                    }
                }
                Err(err) => {
                    let ns_error = build_ns_error(&err);
                    // `into_raw` hands ownership to the completion block /
                    // Finder (which releases it). NOT a leak.
                    completion_handler.call((Retained::into_raw(ns_error),));
                    ItemOutcome::Failed {
                        leaf: ivars.leaf.clone(),
                    }
                }
            };

            leave_fulfillment(key, item_outcome);
        }

        /// Returns the shared serial queue for this provider's session.
        #[unsafe(method_id(operationQueueForFilePromiseProvider:))]
        fn operation_queue(&self, _provider: &NSFilePromiseProvider) -> Retained<NSOperationQueue> {
            self.ivars().queue.0.clone()
        }
    }
);

impl PromiseDelegate {
    /// Builds a delegate for one dragged item.
    fn new(
        leaf: String,
        source_volume_id: String,
        source_path: PathBuf,
        session_key: isize,
        queue: Retained<NSOperationQueue>,
    ) -> Retained<Self> {
        let this = Self::alloc().set_ivars(DelegateIvars {
            leaf,
            source_volume_id,
            source_path,
            session_key,
            queue: SendQueue(queue),
        });
        // SAFETY: standard NSObject init chain.
        unsafe { msg_send![super(this), init] }
    }
}

// ============================================================================
// Provider + queue construction (called from `start_drag`)
// ============================================================================

/// Builds one serial `NSOperationQueue` for a drag session.
/// `maxConcurrentOperationCount = 1` so fulfillments run one at a time (MTP is
/// serial USB; predictable ordering).
fn fresh_serial_queue() -> Retained<NSOperationQueue> {
    let queue = NSOperationQueue::new();
    queue.setMaxConcurrentOperationCount(1);
    queue
}

/// One dragged virtual item's identity, used to build its promise provider.
pub(super) struct PromiseItem {
    /// The leaf filename Finder should land the file under.
    pub leaf: String,
    /// The UTI describing the file type (from [`super::uti`]).
    pub uti: &'static str,
    /// Source volume id to resolve at fulfillment time.
    pub source_volume_id: String,
    /// Source path on that volume.
    pub source_path: PathBuf,
}

/// Builds the providers + delegates + shared serial queue for a virtual drag
/// session, registers the retained objects under `session_key` (the drag
/// sequence number) in the main-thread store, and returns the providers so the
/// caller can attach one to each `NSDraggingItem`.
///
/// Must run on the main thread (drag-start always does); takes `mtm` as proof.
/// `session_key` is looked up again by the source's
/// `draggingSession:endedAtPoint:operation:` to free the session.
pub(super) fn build_session_providers(
    _mtm: MainThreadMarker,
    session_key: isize,
    items: Vec<PromiseItem>,
) -> Vec<Retained<NSFilePromiseProvider>> {
    let queue = fresh_serial_queue();
    let total_items = items.len();

    let mut providers = Vec::with_capacity(items.len());
    let mut delegates = Vec::with_capacity(items.len());

    for item in items {
        let delegate = PromiseDelegate::new(
            item.leaf,
            item.source_volume_id,
            item.source_path,
            session_key,
            queue.clone(),
        );
        let uti = NSString::from_str(item.uti);
        let delegate_proto = ProtocolObject::from_ref(&*delegate);
        let provider =
            NSFilePromiseProvider::initWithFileType_delegate(NSFilePromiseProvider::alloc(), &uti, delegate_proto);

        providers.push(provider);
        delegates.push(delegate);
    }

    // Seed the counters (recording the top-level item count for the started
    // toast) and stash the retained objects (we're on main).
    counters()
        .lock_ignore_poison()
        .entry(session_key)
        .or_default()
        .total_items = total_items;
    with_retained_store(|store| {
        store.insert(
            session_key,
            SessionObjects {
                delegates,
                providers: providers.clone(),
            },
        );
    });

    providers
}

// ============================================================================
// NSError mapping
// ============================================================================

/// The Cocoa `NSUserCancelledError` code (from `Foundation/NSError.h`). A
/// completion-handler error with this code tells AppKit the operation was
/// cancelled, so it suppresses the failure alert.
const NS_USER_CANCELLED_ERROR: isize = 3072;

/// Builds an `NSError` from a [`FulfillError`](super::fulfillment::FulfillError)
/// in the drag-out domain, with the friendly title as `localizedDescription`.
/// Finder surfaces this in its own alert.
fn build_ns_error(err: &super::fulfillment::FulfillError) -> Retained<NSError> {
    use objc2::runtime::AnyObject;
    use objc2_foundation::{NSCopying, NSDictionary, NSErrorUserInfoKey, NSLocalizedDescriptionKey};

    // A cancelled fulfillment uses the NSUserCancelledError code so Finder stays
    // quiet (no alert for a user/system abort); a real failure uses a generic
    // non-zero code and shows the friendly title.
    let code: isize = if err.cancelled { NS_USER_CANCELLED_ERROR } else { 1 };
    let domain = NSString::from_str(ERROR_DOMAIN);
    // localizedDescription = the short category-keyed title, paired into userInfo
    // under NSLocalizedDescriptionKey so AppKit renders our copy.
    let message = NSString::from_str(err.nserror_title());

    // SAFETY: `NSLocalizedDescriptionKey` is the documented userInfo key (an
    // `NSErrorUserInfoKey`, i.e. `NSString`, which conforms to `NSCopying`);
    // the value is an `NSString` (an `NSObject`). The typed
    // `NSDictionary<NSErrorUserInfoKey, AnyObject>` matches what
    // `errorWithDomain:code:userInfo:` expects.
    let user_info: Retained<NSDictionary<NSErrorUserInfoKey, AnyObject>> = unsafe {
        let key: &NSErrorUserInfoKey = NSLocalizedDescriptionKey;
        let key_copying: &ProtocolObject<dyn NSCopying> = ProtocolObject::from_ref(key);
        let value: &AnyObject = &message;
        NSDictionary::dictionaryWithObject_forKey(value, key_copying)
    };
    // SAFETY: `domain` is a valid NSString, `user_info` is the correctly-typed
    // dictionary. `errorWithDomain:code:userInfo:` is the canonical constructor.
    unsafe { NSError::errorWithDomain_code_userInfo(&domain, code, Some(&user_info)) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_system::volume::VolumeError;
    use crate::file_system::volume::friendly_error::listing_error_from_volume_error;

    /// Builds a `FulfillError` of the given category for the NSError-mapping tests.
    /// The exact title is category-keyed in `FulfillError::nserror_title`.
    fn sample_fulfill_error(err: VolumeError, cancelled: bool) -> super::super::fulfillment::FulfillError {
        super::super::fulfillment::FulfillError {
            error: listing_error_from_volume_error(&err, std::path::Path::new("/x.jpg")),
            cancelled,
        }
    }

    // ---- Delegate smoke: construct a provider, fileNameForType returns the leaf ----

    #[test]
    fn provider_delegate_returns_the_known_leaf_name() {
        let Some(mtm) = MainThreadMarker::new() else {
            // Not the main thread (nextest worker): skip rather than panic.
            return;
        };
        let queue = fresh_serial_queue();
        let delegate = PromiseDelegate::new(
            "sunset 2.jpg".to_string(),
            "phone".to_string(),
            PathBuf::from("/DCIM/photo-001.jpg"),
            4242,
            queue,
        );

        // Build a provider so we can query the delegate via the real selector.
        let uti = NSString::from_str("public.jpeg");
        let delegate_proto = ProtocolObject::from_ref(&*delegate);
        let provider =
            NSFilePromiseProvider::initWithFileType_delegate(NSFilePromiseProvider::alloc(), &uti, delegate_proto);

        // Invoke the delegate the way AppKit would: through the protocol method.
        // (`define_class!` synthesizes a `Sel`-taking impl, so the trait method
        // on the `ProtocolObject` is the right call surface in a test.)
        let proto: &ProtocolObject<dyn NSFilePromiseProviderDelegate> = ProtocolObject::from_ref(&*delegate);
        let any_type = NSString::from_str("public.jpeg");
        let name = proto.filePromiseProvider_fileNameForType(&provider, &any_type, mtm);
        assert_eq!(name.to_string(), "sunset 2.jpg");
    }

    // ---- NSError mapping ----

    #[test]
    fn ns_error_carries_domain_and_title() {
        if MainThreadMarker::new().is_none() {
            return;
        }
        // A serious read problem (EIO) → the serious-category NSError title.
        let err = sample_fulfill_error(
            VolumeError::IoError {
                message: "io".into(),
                raw_os_error: Some(5),
            },
            false,
        );
        let ns = build_ns_error(&err);
        assert_eq!(ns.domain().to_string(), ERROR_DOMAIN);
        assert_eq!(ns.code(), 1);
        assert_eq!(ns.localizedDescription().to_string(), err.nserror_title());
        assert!(!ns.localizedDescription().to_string().is_empty());
    }

    #[test]
    fn cancelled_error_uses_user_cancelled_code() {
        if MainThreadMarker::new().is_none() {
            return;
        }
        let err = sample_fulfill_error(VolumeError::Cancelled("cancelled".into()), true);
        let ns = build_ns_error(&err);
        assert_eq!(ns.code(), NS_USER_CANCELLED_ERROR);
    }

    // ---- Session lifetime: cleanup waits for gesture-end AND in-flight==0 ----
    //
    // These drive the COUNTERS state machine directly (the Send half that
    // decides WHEN cleanup fires). The retained-store removal is dispatched to
    // main via `run_on_main` and needs no app handle to assert the counter
    // logic, so these run on any thread.

    #[test]
    fn session_counters_wait_for_in_flight_to_drain() {
        let key = 999_001isize;
        counters().lock_ignore_poison().entry(key).or_default();

        // A fulfillment is in flight when the gesture ends: counters must stay.
        enter_fulfillment(key);
        mark_gesture_ended(key);
        assert!(
            counters().lock_ignore_poison().contains_key(&key),
            "must not free the session while a fulfillment is still in flight"
        );

        // The last fulfillment finishing frees the counters.
        leave_fulfillment(key, ItemOutcome::Succeeded { is_dir: false });
        assert!(
            !counters().lock_ignore_poison().contains_key(&key),
            "the last fulfillment finishing after gesture-end must free the session"
        );
    }

    #[test]
    fn outcomes_accumulate_until_the_session_drains() {
        let key = 999_003isize;
        counters().lock_ignore_poison().entry(key).or_default().total_items = 2;

        // Two fulfillments in flight; each records its outcome on leave.
        enter_fulfillment(key);
        enter_fulfillment(key);
        leave_fulfillment(key, ItemOutcome::Succeeded { is_dir: false });

        // After one leaves, the session is still alive (gesture not ended) and
        // carries one recorded outcome.
        {
            let map = counters().lock_ignore_poison();
            let c = map.get(&key).expect("session still alive");
            assert_eq!(c.outcomes.len(), 1);
            assert_eq!(c.in_flight, 1);
        }

        // The second leaves and the gesture ends: session drains and frees.
        leave_fulfillment(key, ItemOutcome::Failed { leaf: "x.jpg".into() });
        mark_gesture_ended(key);
        assert!(
            !counters().lock_ignore_poison().contains_key(&key),
            "session frees once both fulfillments drained and the gesture ended"
        );
    }

    #[test]
    fn session_counters_clear_at_gesture_end_when_no_fulfillment_in_flight() {
        let key = 999_002isize;
        counters().lock_ignore_poison().entry(key).or_default();
        mark_gesture_ended(key);
        assert!(
            !counters().lock_ignore_poison().contains_key(&key),
            "with no in-flight fulfillment, gesture-end frees the session immediately"
        );
    }
}
