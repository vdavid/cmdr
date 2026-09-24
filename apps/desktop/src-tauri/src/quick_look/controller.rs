//! macOS-only `QuickLookController` plus the `QLPreviewPanelDataSource` /
//! `QLPreviewPanelDelegate` implementation.
//!
//! ## Threading
//!
//! Every method that touches `QLPreviewPanel` (open/set_path/close) MUST run on
//! the AppKit main thread. The public methods on this struct don't enforce it —
//! the Tauri command layer hops via `app.run_on_main_thread()` and then calls in.
//! Inside those closures we obtain a `MainThreadMarker` from `MainThreadMarker::new`
//! (cheap; panics if misused) and proceed.
//!
//! ## Singleton vs new-each-time
//!
//! `+[QLPreviewPanel sharedPreviewPanel]` returns the process-wide instance —
//! there's no "make your own." Every `open` call re-sets data source + delegate
//! on the same panel and calls `makeKeyAndOrderFront:`.
//!
//! ## Close detection
//!
//! We register an `NSNotificationCenter` observer once (on first open) for
//! `NSWindowWillCloseNotification` filtered to the panel object. The observer
//! method emits `quick-look-closed` so the frontend can flip `isOpen = false`
//! regardless of which path closed the panel (our `orderOut`, ✕ click, or Esc).
//!
//! ## Bindings glue
//!
//! The `objc2-quick-look-ui` crate exposes both protocols as implementable
//! traits with `unsafe trait` declarations (verified against docs.rs 0.3.2).
//! `NSURL` already conforms to `QLPreviewItem` out of the box, so the data
//! source just hands back the stored URL — no wrapper class needed.

use std::path::Path;
use std::ptr::NonNull;
use std::sync::Mutex;

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, Bool, ProtocolObject};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{NSEvent, NSEventMask, NSEventType, NSWindowAnimationBehavior, NSWindowDelegate};
use objc2_foundation::{
    NSInteger, NSNotification, NSNotificationCenter, NSNotificationName, NSObject, NSObjectProtocol, NSString, NSURL,
};
use objc2_quick_look_ui::{QLPreviewItem, QLPreviewPanel, QLPreviewPanelDataSource, QLPreviewPanelDelegate};
use tauri::{AppHandle, Manager, Wry};
use tauri_specta::Event as _;

use crate::ignore_poison::IgnorePoison;
use crate::quick_look::{QuickLookClosed, QuickLookKeyEvent};

const ESCAPE_KEY_CODE: u16 = 53;

/// Cross-thread state held by the Tauri-managed `Mutex<QuickLookController>`.
///
/// The bookkeeping (`current_url`, `is_open`) lives here. The actual AppKit
/// objects (panel, delegate) are only ever touched on the main thread inside
/// `run_on_main_thread` closures.
pub struct QuickLookController {
    /// Last URL we asked the panel to preview. `None` until the first `open`.
    current_url: Option<std::path::PathBuf>,
    /// Whether we currently consider the panel ours and on-screen. Flipped to
    /// `false` by the close-notification observer when the panel leaves.
    is_open: bool,
    /// The process-wide local Escape monitor is installed on the first open.
    escape_monitor_installed: bool,
}

impl QuickLookController {
    pub fn new() -> Self {
        Self {
            current_url: None,
            is_open: false,
            escape_monitor_installed: false,
        }
    }

    /// Whether the panel is currently considered open. Test-only observer of the
    /// state machine; production reads the `is_open` field directly.
    #[cfg(test)]
    pub fn is_open(&self) -> bool {
        self.is_open
    }

    /// The URL the panel is currently showing, if any. Test-only.
    #[cfg(test)]
    pub fn current_url(&self) -> Option<&Path> {
        self.current_url.as_deref()
    }

    // ------------------------------------------------------------------
    // Pure state transitions (no AppKit calls). Used by the `_on_main`
    // methods above for the bookkeeping half of each operation, and by
    // unit tests that exercise the state machine without a real panel.
    // The AppKit calls (`makeKeyAndOrderFront:`, `reloadData`, `orderOut:`)
    // can't be unit-tested without a main-thread AppKit runloop, but the
    // bookkeeping these mutate is what every other layer reads.
    // ------------------------------------------------------------------

    /// State half of `open_on_main`: record the path and mark the panel
    /// as open. Idempotent — calling twice just re-targets the URL.
    pub(crate) fn apply_open(&mut self, path: std::path::PathBuf) {
        self.current_url = Some(path);
        self.is_open = true;
    }

    /// State half of `set_path_on_main`: when open, re-target the URL.
    /// Returns `true` if the URL changed (AppKit should `reloadData`),
    /// `false` if we ignored the call because the panel isn't open.
    pub(crate) fn apply_set_path(&mut self, path: std::path::PathBuf) -> bool {
        if !self.is_open {
            return false;
        }
        self.current_url = Some(path);
        true
    }

    /// Open (or re-target) the panel for `path`. Must be called on the main thread.
    ///
    /// `NSOpenPanel` / `NSSavePanel` co-existence behavior is documented in
    /// `quick_look/DETAILS.md` § "Coexistence with NSOpenPanel".
    ///
    /// `app` is used by the delegate to emit `quick-look-key` /
    /// `quick-look-closed` events later, and by the close-notification
    /// observer registered the first time we ever open.
    pub fn open_on_main(&mut self, app: &AppHandle<Wry>, path: std::path::PathBuf) {
        let mtm = MainThreadMarker::new().expect("open_on_main requires the AppKit main thread");

        let Some(panel) = shared_panel(mtm) else {
            log::warn!(target: "quick_look", "QLPreviewPanel.sharedPreviewPanel returned nil; skipping open");
            return;
        };

        let delegate = ensure_delegate(app, &panel, mtm);
        if !self.escape_monitor_installed {
            self.escape_monitor_installed = install_escape_monitor(app);
        }
        set_delegate_url(&delegate, Some(&path));
        self.apply_open(path);

        // SAFETY: `setDataSource:` accepts a protocol object conforming to
        // QLPreviewPanelDataSource; ours does. `setDelegate:` accepts a raw
        // object; the panel introspects via -respondsToSelector:.
        unsafe {
            let data_source_proto = ProtocolObject::from_ref(&*delegate);
            panel.setDataSource(Some(data_source_proto));
            panel.setDelegate(Some(&*delegate as &AnyObject));
        }

        // The default Quick Look transition fades the panel in. Show it at once
        // so it can receive the next keystroke as soon as it is ordered front.
        panel.setAnimationBehavior(NSWindowAnimationBehavior::None);
        panel.makeKeyAndOrderFront(None);
        // SAFETY: `panel` is the live `sharedPreviewPanel`; `reloadData` re-queries our just-installed
        // data source. We hold a `MainThreadMarker` (asserted at fn entry), and `QLPreviewPanel`
        // requires the main thread.
        unsafe { panel.reloadData() };
        log::debug!(target: "quick_look", "panel opened for {:?}", self.current_url);
    }

    /// Re-target the panel to a new path. No-op if not currently open.
    pub fn set_path_on_main(&mut self, path: std::path::PathBuf) {
        let mtm = MainThreadMarker::new().expect("set_path_on_main requires the AppKit main thread");
        if !self.is_open {
            log::debug!(target: "quick_look", "set_path called while closed; ignoring");
            return;
        }

        let Some(panel) = shared_panel(mtm) else { return };

        // Only update if the delegate is still ours. If something else stole
        // the panel, our `current_url` is stale — bail rather than corrupt
        // the other consumer.
        let our_delegate = match try_current_delegate(&panel) {
            Some(d) => d,
            None => {
                log::debug!(target: "quick_look", "panel's delegate is no longer ours; treating as closed");
                self.is_open = false;
                return;
            }
        };

        set_delegate_url(&our_delegate, Some(&path));
        if self.apply_set_path(path) {
            // SAFETY: `panel` is the live `sharedPreviewPanel` and our delegate is still installed
            // (checked above); `reloadData` re-reads the URL we just set. Main thread (marker asserted
            // at fn entry), as `QLPreviewPanel` requires.
            unsafe { panel.reloadData() };
        }
    }

    /// Hide the panel. Release the controller lock before `orderOut:` because
    /// AppKit may deliver a close notification in the same event turn.
    pub fn close_on_main(state: &Mutex<Self>) {
        let mtm = MainThreadMarker::new().expect("close_on_main requires the AppKit main thread");
        if !state.lock_ignore_poison().is_open {
            return;
        }
        if let Some(panel) = shared_panel(mtm) {
            panel.orderOut(None);
        }
        // The close-notification observer will flip `is_open` and emit
        // `quick-look-closed`. We DON'T mirror the flip here: doing it twice
        // races with that callback and could falsely report "closed" before
        // the panel has left the screen, causing reopens to fail.
    }

    /// Called by the close-notification observer.
    pub(crate) fn mark_closed(&mut self) {
        self.is_open = false;
        self.current_url = None;
    }
}

impl Default for QuickLookController {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Panel singleton helpers
// ============================================================================

fn shared_panel(mtm: MainThreadMarker) -> Option<Retained<QLPreviewPanel>> {
    // SAFETY: `sharedPreviewPanel` is the documented entry point and returns
    // an autoreleased panel; objc2 retains it for us.
    unsafe { QLPreviewPanel::sharedPreviewPanel(mtm) }
}

/// Catch Escape before Quick Look's opening transition or event routing can
/// discard it. Only consume a key addressed to our panel; other windows keep
/// their own Escape behavior.
fn install_escape_monitor(app: &AppHandle<Wry>) -> bool {
    let app = app.clone();
    let block = block2::RcBlock::new(move |event: NonNull<NSEvent>| -> *mut NSEvent {
        // SAFETY: AppKit keeps this event alive throughout the local monitor callback.
        let event_ref = unsafe { event.as_ref() };
        let command_modifiers = (1 << 18) | (1 << 19) | (1 << 20);
        if event_ref.keyCode() != ESCAPE_KEY_CODE || event_ref.modifierFlags().0 & command_modifiers != 0 {
            return event.as_ptr();
        }
        let Some(mtm) = MainThreadMarker::new() else {
            return event.as_ptr();
        };
        let Some(panel) = shared_panel(mtm) else {
            return event.as_ptr();
        };
        if event_ref.windowNumber() != panel.windowNumber() || try_current_delegate(&panel).is_none() {
            return event.as_ptr();
        }
        let Some(state) = app.try_state::<crate::quick_look::QuickLookState>() else {
            return event.as_ptr();
        };
        if !state.lock_ignore_poison().is_open {
            return event.as_ptr();
        }
        // The guard is gone before AppKit can deliver the close notification.
        panel.orderOut(None);
        std::ptr::null_mut()
    });

    // SAFETY: The block has AppKit's `(NSEvent *) -> NSEvent *` signature;
    // returning the original event passes it on, and null consumes Escape.
    let monitor = unsafe { NSEvent::addLocalMonitorForEventsMatchingMask_handler(NSEventMask::KeyDown, &block) };
    match monitor {
        Some(monitor) => {
            let _installed = Retained::into_raw(monitor);
            true
        }
        None => {
            log::warn!(target: "quick_look", "AppKit refused the Quick Look Escape monitor");
            false
        }
    }
}

fn try_current_delegate(panel: &QLPreviewPanel) -> Option<Retained<QuickLookDelegate>> {
    // SAFETY: `delegate` returns Option<Retained<AnyObject>>. We downcast to
    // QuickLookDelegate; mismatched class returns Err and we hand back None.
    let delegate = unsafe { panel.delegate() }?;
    delegate.downcast::<QuickLookDelegate>().ok()
}

// ============================================================================
// Delegate
// ============================================================================

/// Ivars for the delegate NSObject. `AppHandle` is `Send + Sync + Clone` and
/// the URL is wrapped in `Mutex` because the data-source callback can run from
/// any main-thread invocation, but we set it from `open` / `set_path` paths
/// that already hold the controller's `Mutex` — using a plain `Mutex` here is
/// belt-and-braces.
pub(crate) struct DelegateIvars {
    pub(crate) app: AppHandle<Wry>,
    pub(crate) url: Mutex<Option<Retained<NSURL>>>,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = DelegateIvars]
    pub(crate) struct QuickLookDelegate;

    unsafe impl NSObjectProtocol for QuickLookDelegate {}

    /// `NSWindowDelegate` is the super-protocol of `QLPreviewPanelDelegate`.
    /// We have no callbacks we want to handle here (the close notification
    /// path is via `NSNotificationCenter`, not the delegate), so this is an
    /// empty conformance marker.
    unsafe impl NSWindowDelegate for QuickLookDelegate {}

    /// `QLPreviewPanelDataSource`. The panel needs to know how many items we
    /// show (always 1 in v1) and which URL to load for each index.
    unsafe impl QLPreviewPanelDataSource for QuickLookDelegate {
        #[unsafe(method(numberOfPreviewItemsInPreviewPanel:))]
        fn number_of_preview_items(&self, _panel: Option<&QLPreviewPanel>) -> NSInteger {
            // v1 always shows exactly the cursor item. Multi-selection
            // "carousel" mode (Finder-style: arrow keys cycle through the
            // selected set, panel renders Nth preview at index N) is a
            // future enhancement — to add it: keep a `Vec<NSURL>` instead of
            // a single `Option<NSURL>` in `DelegateIvars`, return the
            // vector's length here, and look up by index in
            // `previewItemAtIndex`. The frontend would call a new
            // `quick_look_set_paths(paths[])` IPC whenever the selection
            // size > 1, mirroring the cursor-follow $effect but on the
            // selection set.
            let has_url = self.ivars().url.lock().ok().map(|g| g.is_some()).unwrap_or(false);
            if has_url { 1 } else { 0 }
        }

        #[unsafe(method_id(previewPanel:previewItemAtIndex:))]
        fn preview_item_at_index(
            &self,
            _panel: Option<&QLPreviewPanel>,
            _index: NSInteger,
        ) -> Option<Retained<ProtocolObject<dyn QLPreviewItem>>> {
            // NSURL conforms to QLPreviewItem out of the box (verified on
            // docs.rs 0.3.2: `impl QLPreviewItem for NSURL`).
            //
            // `method_id` wraps the return in its own conversion machinery, so
            // the function body must have a single tail expression of type
            // `Option<Retained<T>>` (no early `return` or `?`). We compute the
            // value once and let the macro coerce.
            self.ivars()
                .url
                .lock()
                .ok()
                .and_then(|guard| guard.as_ref().cloned())
                .map(ProtocolObject::from_retained)
        }
    }

    /// `QLPreviewPanelDelegate`. We only implement `handleEvent:` so we can
    /// intercept key events while the panel is key and forward them to the
    /// focused pane via Tauri events. Mouse events return NO so the panel can
    /// handle them natively; Escape closes if the local monitor missed it.
    unsafe impl QLPreviewPanelDelegate for QuickLookDelegate {
        #[unsafe(method(previewPanel:handleEvent:))]
        fn handle_event(&self, panel: Option<&QLPreviewPanel>, event: Option<&NSEvent>) -> Bool {
            let Some(event) = event else { return Bool::NO };
            let event_type = event.r#type();
            if event_type != NSEventType::KeyDown {
                return Bool::NO;
            }
            let Some(payload) = build_key_event(event) else { return Bool::NO };

            // This delegate only sees keys the panel did not handle. Close on
            // Escape if it reaches us despite the local monitor above.
            if payload.key == "Escape" && !payload.meta_key && !payload.ctrl_key && !payload.alt_key {
                if let Some(panel) = panel {
                    panel.orderOut(None);
                    return Bool::YES;
                }
                return Bool::NO;
            }

            if let Err(e) = payload.emit(&self.ivars().app) {
                log::warn!(target: "quick_look", "failed to emit quick-look-key: {e}");
            }
            Bool::YES
        }
    }

    impl QuickLookDelegate {
        /// Selector dispatched by `NSNotificationCenter` for the
        /// panel-will-close notification. Flips `is_open` to false on the
        /// controller and emits the frontend close event.
        #[unsafe(method(quickLookPanelWillClose:))]
        fn panel_will_close(&self, _notification: *const NSNotification) {
            let app = self.ivars().app.clone();
            if let Some(state) = app.try_state::<crate::quick_look::QuickLookState>() {
                state.lock_ignore_poison().mark_closed();
            }
            if let Err(e) = QuickLookClosed.emit(&app) {
                log::warn!(target: "quick_look", "failed to emit quick-look-closed: {e}");
            }
            log::debug!(target: "quick_look", "panel closed");
        }
    }
);

impl QuickLookDelegate {
    fn new(app: AppHandle<Wry>, mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(DelegateIvars {
            app,
            url: Mutex::new(None),
        });
        // SAFETY: standard NSObject init chain.
        unsafe { msg_send![super(this), init] }
    }
}

fn set_delegate_url(delegate: &QuickLookDelegate, path: Option<&Path>) {
    let url = path.map(|p| {
        let s = NSString::from_str(&p.to_string_lossy());
        NSURL::fileURLWithPath(&s)
    });
    if let Ok(mut guard) = delegate.ivars().url.lock() {
        *guard = url;
    }
}

/// Ensure the panel has our delegate installed and the close-notification
/// observer registered. Returns the retained delegate for the caller to also
/// pass into `setDataSource:` / `setDelegate:`.
fn ensure_delegate(app: &AppHandle<Wry>, panel: &QLPreviewPanel, mtm: MainThreadMarker) -> Retained<QuickLookDelegate> {
    if let Some(existing) = try_current_delegate(panel) {
        return existing;
    }
    let delegate = QuickLookDelegate::new(app.clone(), mtm);
    register_close_observer(&delegate, panel);
    delegate
}

fn register_close_observer(delegate: &QuickLookDelegate, panel: &QLPreviewPanel) {
    // `NSWindowWillCloseNotification` lives in AppKit; not exposed as a typed
    // static in objc2-app-kit 0.3, so we look it up by name. The string is
    // compared inside NSNotificationCenter; cost is negligible.
    let name = NSString::from_str("NSWindowWillCloseNotification");
    // SAFETY: NSNotificationName is a typedef for NSString*; the cast preserves
    // pointer identity.
    let name_proto: &NSNotificationName = unsafe { &*(name.as_ref() as *const NSString as *const NSNotificationName) };
    let center = NSNotificationCenter::defaultCenter();
    let panel_obj: &AnyObject = panel.as_ref();
    let observer_obj: &AnyObject = delegate.as_ref();
    // SAFETY: standard NSNotificationCenter API; we pass an object that
    // implements the named selector and outlives the app (singleton).
    unsafe {
        center.addObserver_selector_name_object(
            observer_obj,
            sel!(quickLookPanelWillClose:),
            Some(name_proto),
            Some(panel_obj),
        );
    }
    // We intentionally never call `removeObserver:`. The panel is process-wide
    // and our delegate lives as long as the app (it's retained by the panel
    // through setDelegate). When AppHandle drops at shutdown the delegate
    // goes away with it; until then leaving the observer registered is the
    // documented pattern for singleton observers.
}

fn build_key_event(event: &NSEvent) -> Option<QuickLookKeyEvent> {
    let chars = event.charactersIgnoringModifiers()?;
    let flags = event.modifierFlags();
    let key_code = event.keyCode();

    let key = ns_string_to_key(&chars);
    let code = key_code_to_dom_code(key_code).to_string();

    // NSEventModifierFlags bits — sourced from AppKit headers (mask values are
    // ABI-stable). Bits 17..=20 are the four modifiers we care about.
    let raw = flags.0;
    let shift_key = raw & (1 << 17) != 0;
    let ctrl_key = raw & (1 << 18) != 0;
    let alt_key = raw & (1 << 19) != 0;
    let meta_key = raw & (1 << 20) != 0;

    Some(QuickLookKeyEvent {
        key,
        code,
        shift_key,
        meta_key,
        alt_key,
        ctrl_key,
    })
}

fn ns_string_to_key(s: &NSString) -> String {
    let rust = s.to_string();
    let mut chars = rust.chars();
    let first = chars.next();
    let only_one = first.is_some() && chars.next().is_none();
    if only_one {
        let ch = first.expect("only_one is true only when first.is_some()");
        return match ch as u32 {
            0xF700 => "ArrowUp".to_string(),
            0xF701 => "ArrowDown".to_string(),
            0xF702 => "ArrowLeft".to_string(),
            0xF703 => "ArrowRight".to_string(),
            0xF729 => "Home".to_string(),
            0xF72B => "End".to_string(),
            0xF72C => "PageUp".to_string(),
            0xF72D => "PageDown".to_string(),
            0x001B => "Escape".to_string(),
            0x0009 => "Tab".to_string(),
            0x000D | 0x0003 => "Enter".to_string(),
            0x007F | 0x0008 => "Backspace".to_string(),
            // Plain ASCII / printable — pass through as-is.
            _ => rust,
        };
    }
    rust
}

/// Maps a virtual key code to its DOM `KeyboardEvent.code` value. Only covers
/// the keys we forward in M1; anything else falls through to an empty string
/// (the frontend ignores `code` when it's empty).
fn key_code_to_dom_code(kc: u16) -> &'static str {
    // Apple virtual key codes — see HIToolbox/Events.h.
    match kc {
        49 => "Space",
        53 => "Escape",
        36 => "Enter",
        48 => "Tab",
        51 => "Backspace",
        123 => "ArrowLeft",
        124 => "ArrowRight",
        125 => "ArrowDown",
        126 => "ArrowUp",
        115 => "Home",
        119 => "End",
        116 => "PageUp",
        121 => "PageDown",
        _ => "",
    }
}

// ============================================================================
// State machine tests
// ============================================================================
//
// These exercise only the bookkeeping half of the controller (`apply_open`,
// `apply_set_path`, `mark_closed`, and the read accessors). The AppKit
// half (`makeKeyAndOrderFront:`, `reloadData`, `orderOut:`, and the
// `NSNotificationCenter` observer) needs a real main-thread runloop and a
// `QLPreviewPanel` instance — neither is reachable from a unit test. The
// bookkeeping is what every other layer reads (the IPC layer's
// `volume_supports_local_fs` gate, the close-event emitter, and the frontend
// `isOpen` flag), so these transitions are worth pinning even without the
// AppKit side. See `quick_look/CLAUDE.md` § "Testing gap" for the trade-off.
#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> std::path::PathBuf {
        std::path::PathBuf::from(s)
    }

    #[test]
    fn new_starts_closed_with_no_url() {
        let c = QuickLookController::new();
        assert!(!c.is_open());
        assert!(c.current_url().is_none());
    }

    #[test]
    fn apply_open_sets_url_and_flips_open() {
        let mut c = QuickLookController::new();
        c.apply_open(p("/tmp/a.png"));
        assert!(c.is_open());
        assert_eq!(c.current_url(), Some(p("/tmp/a.png").as_path()));
    }

    #[test]
    fn double_open_is_idempotent_and_re_targets_url() {
        let mut c = QuickLookController::new();
        c.apply_open(p("/tmp/a.png"));
        c.apply_open(p("/tmp/b.png"));
        assert!(c.is_open());
        assert_eq!(c.current_url(), Some(p("/tmp/b.png").as_path()));
    }

    #[test]
    fn set_path_before_open_is_a_no_op() {
        let mut c = QuickLookController::new();
        let changed = c.apply_set_path(p("/tmp/a.png"));
        assert!(!changed, "apply_set_path should report no change when closed");
        assert!(!c.is_open());
        assert!(c.current_url().is_none());
    }

    #[test]
    fn set_path_while_open_updates_url_and_reports_change() {
        let mut c = QuickLookController::new();
        c.apply_open(p("/tmp/a.png"));
        let changed = c.apply_set_path(p("/tmp/b.png"));
        assert!(changed, "apply_set_path should report a change when open");
        assert_eq!(c.current_url(), Some(p("/tmp/b.png").as_path()));
        assert!(c.is_open());
    }

    #[test]
    fn mark_closed_clears_both_fields() {
        let mut c = QuickLookController::new();
        c.apply_open(p("/tmp/a.png"));
        c.mark_closed();
        assert!(!c.is_open());
        assert!(c.current_url().is_none());
    }

    #[test]
    fn close_then_reopen_works() {
        // The full open → set_path → close → reopen lifecycle. `close_on_main`
        // itself can't run without AppKit, but its observer-driven state half
        // is `mark_closed`, which is what we drive here.
        let mut c = QuickLookController::new();
        c.apply_open(p("/tmp/a.png"));
        assert!(c.apply_set_path(p("/tmp/b.png")));
        c.mark_closed();
        assert!(!c.is_open());
        // After close, set_path is a no-op again.
        assert!(!c.apply_set_path(p("/tmp/c.png")));
        // Reopen flips everything back on.
        c.apply_open(p("/tmp/d.png"));
        assert!(c.is_open());
        assert_eq!(c.current_url(), Some(p("/tmp/d.png").as_path()));
    }

    #[test]
    fn mark_closed_when_already_closed_is_a_no_op() {
        let mut c = QuickLookController::new();
        c.mark_closed();
        assert!(!c.is_open());
        assert!(c.current_url().is_none());
    }

    #[test]
    fn key_code_mapping_covers_forwarded_keys() {
        // Light cross-check on the DOM-code translator the delegate uses to
        // build `quick-look-key` payloads. The frontend listener filters by
        // `key === ' '`, so the mapping for Space (the most-used) needs to
        // stay correct in particular.
        assert_eq!(key_code_to_dom_code(49), "Space");
        assert_eq!(key_code_to_dom_code(53), "Escape");
        assert_eq!(key_code_to_dom_code(125), "ArrowDown");
        assert_eq!(key_code_to_dom_code(126), "ArrowUp");
        assert_eq!(key_code_to_dom_code(9999), "");
    }
}
