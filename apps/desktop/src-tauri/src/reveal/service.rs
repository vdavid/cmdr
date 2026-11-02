//! The macOS service behind "Open in Cmdr" in Finder's right-click menu.
//!
//! `Info.plist`'s `NSServices` entry declares it; this is the object AppKit calls when someone
//! picks it. It reads the paths off the pasteboard and hands them to the same door everything
//! else comes through, so the window raise, the parking until the frontend is up, and the pane
//! move are shared. `DETAILS.md` § "Open in Cmdr".
//!
//! ❗ Everything here is main-thread-only AppKit. Not to be confused with `crate::services_menu`,
//! which goes the other way: it lets Cmdr's own panes SEND files to other apps' services.

use std::cell::RefCell;
use std::path::PathBuf;
use std::sync::OnceLock;

use objc2::rc::Retained;
use objc2::runtime::AnyClass;
use objc2::{ClassType, MainThreadMarker, MainThreadOnly, define_class, msg_send};
use objc2_app_kit::{NSApplication, NSPasteboard, NSUpdateDynamicServices};
use objc2_foundation::{NSArray, NSObject, NSObjectProtocol, NSString, NSURL};
use tauri::{AppHandle, Wry};

use crate::tools::GoToTarget;

use super::delivery::{RevealOrigin, RevealRequest, take_requests};

/// What the service call has to reach. AppKit calls a method with no way to pass anything along,
/// and it only ever happens after `install` set this.
static APP: OnceLock<AppHandle<Wry>> = OnceLock::new();

define_class!(
    /// `NSApp.servicesProvider`. Its one method's name is `NSMessage` in `Info.plist`, plus the
    /// `:userData:error:` every service method has.
    #[unsafe(super(NSObject))]
    #[name = "CmdrOpenInCmdrService"]
    #[thread_kind = MainThreadOnly]
    struct OpenInCmdrService;

    unsafe impl NSObjectProtocol for OpenInCmdrService {}

    impl OpenInCmdrService {
        #[unsafe(method(openInCmdr:userData:error:))]
        fn open_in_cmdr(&self, pasteboard: &NSPasteboard, _user_data: Option<&NSString>, _error: *mut *mut NSString) {
            let paths = paths_on(pasteboard);
            log::info!(target: "reveal", "Open in Cmdr: {} path(s) from a service call", paths.len());
            let Some(app) = APP.get() else {
                return;
            };
            if paths.is_empty() {
                return;
            }
            take_requests(
                app,
                vec![RevealRequest {
                    paths,
                    target: GoToTarget::FOCUSED,
                    origin: RevealOrigin::OpenInCmdr,
                }],
            );
        }
    }
);

impl OpenInCmdrService {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        // SAFETY: standard NSObject init chain on an allocated instance.
        unsafe { msg_send![Self::alloc(mtm), init] }
    }
}

thread_local! {
    /// Keeps the provider alive for the process lifetime, whatever AppKit does with its reference.
    /// Thread-local because `Retained` of a main-thread-only class isn't `Send`.
    static PROVIDER: RefCell<Option<Retained<OpenInCmdrService>>> = const { RefCell::new(None) };
}

/// The file paths on a service's pasteboard.
///
/// ❗ Through `filePathURL`, because Finder can write file REFERENCE URLs (`file:///.file/id=…`),
/// whose text is no path at all.
fn paths_on(pasteboard: &NSPasteboard) -> Vec<PathBuf> {
    let classes: Retained<NSArray<AnyClass>> = NSArray::from_slice(&[NSURL::class()]);
    // SAFETY: `NSURL` conforms to `NSPasteboardReading`, which is what this array must hold, and
    // no options are passed.
    let Some(objects) = (unsafe { pasteboard.readObjectsForClasses_options(&classes, None) }) else {
        return Vec::new();
    };
    objects
        .iter()
        .filter_map(|object| object.downcast::<NSURL>().ok())
        .filter_map(|url| url.filePathURL())
        .filter_map(|url| url.path())
        .map(|path| PathBuf::from(path.to_string()))
        .collect()
}

/// Becomes `NSApp.servicesProvider`, so "Open in Cmdr" has someone to call.
///
/// Call from `setup`, which is `applicationDidFinishLaunching:`: on a cold launch AppKit delivers
/// the service call after that (measured on macOS 27.0.1, `DETAILS.md` § "Open in Cmdr").
pub fn install(app: &AppHandle<Wry>) {
    let Some(mtm) = MainThreadMarker::new() else {
        log::warn!(target: "reveal", "Not on the main thread; Open in Cmdr won't answer");
        return;
    };
    let _ = APP.set(app.clone());
    let provider = OpenInCmdrService::new(mtm);
    // SAFETY: the provider is alive and `PROVIDER` keeps it that way for the process lifetime.
    unsafe { NSApplication::sharedApplication(mtm).setServicesProvider(Some(&provider)) };
    PROVIDER.with(|slot| slot.replace(Some(provider)));
    NSUpdateDynamicServices();
}
