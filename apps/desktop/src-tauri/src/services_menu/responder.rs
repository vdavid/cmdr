//! The object that answers AppKit for file send types, and the send-type
//! registration that makes AppKit ask at all.
//!
//! ❗ Everything here is main-thread-only AppKit and takes a `MainThreadMarker`.

use std::cell::RefCell;

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, Bool, ProtocolObject};
use objc2::{ClassType, MainThreadMarker, MainThreadOnly, Message, define_class, msg_send};
use objc2_app_kit::{
    NSApplication, NSPasteboard, NSPasteboardItem, NSPasteboardType, NSPasteboardTypeFileURL, NSPasteboardWriting,
    NSResponder, NSServicesMenuRequestor, NSUpdateDynamicServices, NSWindow,
};
use objc2_foundation::{NSArray, NSObjectProtocol, NSString, NSURL};

use crate::native_drag::type_plan::{DragSessionLocality, plan_pasteboard_items};

use super::selection;

/// The two pasteboard types Cmdr advertises it can send, and the ones it answers for.
///
/// ❗ BOTH, and the deprecated one is the load-bearing half: of the 25 installed
/// services that take files, 20 declare ONLY `NSFilenamesPboardType`, 5 declare ONLY
/// a file-url spelling, and not one declares both (verified on macOS 26.6.2 by
/// counting `NSSendTypes` in `/System/Library/CoreServices/pbs -dump_pboard`,
/// 2026-09-07). Registering only the modern type would hide 20 of the 25, which is
/// the bug this module exists to fix.
///
/// So `NSFilenamesPboardType` stays despite Apple's header deprecating it: the
/// services that read it are shipping today and won't be rewritten because a header
/// says so. Cmdr's drag-out publishes it for the same reason
/// (`native_drag/type_plan.rs`).
#[expect(
    deprecated,
    reason = "20 of 25 installed file services declare NSFilenamesPboardType and nothing else"
)]
fn advertised_types() -> [&'static NSPasteboardType; 2] {
    use objc2_app_kit::NSFilenamesPboardType;
    // SAFETY: both are AppKit's own `NSPasteboardType` string constants, alive for
    // the process lifetime once AppKit is loaded, which it is by the time any
    // caller here runs (they all hold a `MainThreadMarker`).
    unsafe { [NSFilenamesPboardType, NSPasteboardTypeFileURL] }
}

/// Whether Cmdr answers for this send type.
///
/// Comparing the incoming constant against AppKit's own is the API's contract, not
/// error string-matching: these are identifiers the framework hands us back, and no
/// wording or locale can move them.
fn is_file_send_type(send_type: &NSPasteboardType) -> bool {
    advertised_types().iter().any(|known| **known == *send_type)
}

/// Tells `NSApplication` that Cmdr can hand over file paths.
///
/// Empty return types: Cmdr takes nothing BACK from a service, so no service that
/// wants to replace the selection appears. That's the right answer for a file
/// manager, and it matches what Finder offers.
pub fn register_send_types(mtm: MainThreadMarker) {
    let app = NSApplication::sharedApplication(mtm);
    let send: Retained<NSArray<NSPasteboardType>> = NSArray::from_slice(&advertised_types());
    let empty: Retained<NSArray<NSPasteboardType>> = NSArray::new();
    app.registerServicesMenuSendTypes_returnTypes(&send, &empty);
    // Apple's guidance is to register early and then refresh, which is what this
    // pair is; `install` runs before the menu bar is built for the same reason.
    NSUpdateDynamicServices();
}

define_class!(
    /// Sits in the main window's responder chain and answers for file send types.
    ///
    /// An `NSResponder` subclass rather than a view: Cmdr's file list is DOM inside
    /// one `WKWebView`, so there is no per-row `NSView` holding a selection for
    /// AppKit to ask. The web view stays AHEAD of this object in the chain, so it
    /// keeps answering for text types in text fields and only the file types fall
    /// through to here.
    #[unsafe(super(NSResponder))]
    #[name = "CmdrServicesResponder"]
    #[thread_kind = MainThreadOnly]
    pub struct CmdrServicesResponder;

    unsafe impl NSObjectProtocol for CmdrServicesResponder {}

    unsafe impl NSServicesMenuRequestor for CmdrServicesResponder {
        /// Writes the selection onto the services pasteboard.
        ///
        /// ⚠️ Called once per CANDIDATE SERVICE while the submenu is OPENING, not
        /// only when one is picked: AppKit writes the selection to a scratch
        /// pasteboard to test each service's `NSRequiredContext`. Measured on macOS
        /// 26.6.2 (2026-09-07): seven calls within 90 ms for one menu open, each
        /// carrying the whole 66-row selection.
        ///
        /// Writes BOTH shapes regardless of which one `types` asks for, because a
        /// pasteboard carrying more than a service declared costs nothing and a
        /// service reading the flavor it didn't declare first is a real thing. The
        /// layout is Finder's, and Cmdr's own drag-out already computes it:
        /// `public.file-url` per item, the whole `NSFilenamesPboardType` array on
        /// the first item.
        #[unsafe(method(writeSelectionToPasteboard:types:))]
        fn write_selection(&self, pboard: &NSPasteboard, _types: &NSArray<NSPasteboardType>) -> Bool {
            let paths: Vec<String> = selection::resolve()
                .iter()
                .filter_map(|path| path.to_str().map(str::to_string))
                .collect();
            if paths.is_empty() {
                log::debug!(target: "services_menu", "A service asked for the selection and there was none");
                return Bool::NO;
            }
            let items = build_pasteboard_items(&paths);
            pboard.clearContents();
            Bool::new(pboard.writeObjects(&items))
        }
    }

    impl CmdrServicesResponder {
        /// AppKit's "can you provide this?" probe, sent along the key window's
        /// responder chain while it decides which services the menu may show.
        ///
        /// `return_type` must be absent (or empty): a service that wants to hand
        /// data BACK has nowhere to put it, since Cmdr publishes no return types.
        #[unsafe(method_id(validRequestorForSendType:returnType:))]
        fn valid_requestor(
            &self,
            send_type: Option<&NSPasteboardType>,
            return_type: Option<&NSPasteboardType>,
        ) -> Option<Retained<AnyObject>> {
            let wants_return = return_type.is_some_and(|t| !t.is_empty());
            let answers =
                !wants_return && send_type.is_some_and(is_file_send_type) && selection::has_anything();
            // A `None` here is the chain's "not me", which lets AppKit carry on to
            // the next responder exactly as it did before this object existed.
            answers.then(|| {
                // SAFETY: `Self` is an `NSResponder` subclass, so the cast only
                // widens the static type of a pointer AppKit already treats as
                // `id`. The caller sends it `writeSelectionToPasteboard:types:`,
                // which this class implements.
                unsafe { Retained::cast_unchecked::<AnyObject>(self.retain()) }
            })
        }
    }
);

impl CmdrServicesResponder {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        // SAFETY: standard NSObject init chain on an allocated instance.
        unsafe { msg_send![Self::alloc(mtm), init] }
    }
}

/// One `NSPasteboardItem` per path, carrying Finder's layout.
fn build_pasteboard_items(paths: &[String]) -> Retained<NSArray<ProtocolObject<dyn NSPasteboardWriting>>> {
    let [filenames_type, file_url_type] = advertised_types();
    let items: Vec<Retained<NSPasteboardItem>> = plan_pasteboard_items(paths, DragSessionLocality::Local)
        .iter()
        .map(|plan| {
            let item = NSPasteboardItem::new();
            if let Some(path) = plan.file_url.as_deref() {
                // `public.file-url` wants the URL's absolute string (`file:///…`
                // with a percent-encoded path); a serialized property list is
                // misparsed ("An invalid URL was found on the pasteboard").
                let url = NSURL::fileURLWithPath(&NSString::from_str(path));
                if let Some(absolute) = url.absoluteString() {
                    item.setString_forType(&absolute, file_url_type);
                }
            }
            if let Some(names) = plan.filenames.as_deref() {
                let names: Vec<Retained<NSString>> = names.iter().map(|name| NSString::from_str(name)).collect();
                let names: Retained<NSArray<NSString>> = NSArray::from_retained_slice(&names);
                // SAFETY: `NSFilenamesPboardType`'s property list is an
                // `NSArray<NSString>` of POSIX paths, which is exactly what `names` is.
                unsafe { item.setPropertyList_forType(&names, filenames_type) };
            }
            item
        })
        .collect();
    let writers: Vec<&ProtocolObject<dyn NSPasteboardWriting>> =
        items.iter().map(|item| ProtocolObject::from_ref(&**item)).collect();
    NSArray::from_slice(&writers)
}

thread_local! {
    /// The responder spliced into the main window's chain.
    ///
    /// `nextResponder` is an UNRETAINED reference, so an object whose only strong
    /// reference is a local would be freed the moment [`attach_to_window`] returns,
    /// leaving the view before it pointing at dead memory. Thread-local rather than
    /// a `static`, because `Retained` is neither `Send` nor `Sync` and this only ever
    /// runs on the main thread.
    static ATTACHED: RefCell<Option<Retained<CmdrServicesResponder>>> = const { RefCell::new(None) };
}

/// Splices the selection responder into the main window's responder chain, between
/// the content view and the window.
///
/// ⚠️ It has to go BELOW the window, not after it. `NSWindow` does NOT forward
/// `validRequestorForSendType:returnType:` to its own `nextResponder` (measured on
/// macOS 26.6.2, 2026-09-07: the same call reached nothing from the window and
/// walked the whole view chain from the first responder), so a responder placed
/// after the window is never asked. `DETAILS.md` § "How AppKit reaches us".
///
/// Everything already in the chain keeps first refusal: the walk runs from the first
/// responder up through the views, so the `WKWebView` still answers for text types in
/// a text field and only what nobody claimed arrives here. The content view's own
/// `nextResponder` becomes this object's, so nothing leaves the chain.
///
/// The MAIN window alone, deliberately: the file services act on the pane selection,
/// so Settings and the viewer keep the plain menu. Idempotent, so a second call is
/// safe.
///
/// # Safety
///
/// `ns_window` must be a live `NSWindow` (or null, which is answered with a log line).
pub unsafe fn attach_to_window(mtm: MainThreadMarker, ns_window: *mut std::ffi::c_void) {
    if ns_window.is_null() {
        log::warn!(target: "services_menu", "The main window handed out a null NSWindow");
        return;
    }
    // SAFETY: the caller's contract is that `ns_window` is a live `NSWindow`, and
    // this runs synchronously on the main thread, so the window can't be torn down
    // while the reference is held. Null was rejected above.
    let window: &NSWindow = unsafe { &*ns_window.cast::<NSWindow>() };
    let Some(content_view) = window.contentView() else {
        log::warn!(target: "services_menu", "The main window has no content view to attach to");
        return;
    };

    // SAFETY: `nextResponder` / `setNextResponder:` are unsafe because the reference
    // is unretained. Both ends survive: the content view's original next responder is
    // the window, which outlives its own view, and ours is held in `ATTACHED` for the
    // process lifetime.
    unsafe {
        let current = content_view.nextResponder();
        if current
            .as_deref()
            .is_some_and(|next| next.isKindOfClass(CmdrServicesResponder::class()))
        {
            return;
        }
        let responder = CmdrServicesResponder::new(mtm);
        responder.setNextResponder(current.as_deref());
        content_view.setNextResponder(Some(&responder));
        ATTACHED.with(|slot| slot.replace(Some(responder)));
    }
    log::debug!(target: "services_menu", "Selection responder attached to the main window");
}
