//! macOS NSPasteboard FFI for file URL clipboard operations.
//!
//! NSPasteboard is main-thread-only. Every public function takes a
//! `MainThreadMarker` as compile-time proof: callers obtain it inside an
//! `app.run_on_main_thread()` closure, so an off-main call won't compile.
//!
//! Runtime opt-out: setting `CMDR_CLIPBOARD_BACKEND=mock` at process start
//! makes every call delegate to the shared in-process store in `super::store`
//! instead of touching NSPasteboard. The env value is sampled once via a
//! `LazyLock`; flipping it mid-process has no effect. This is the dev /
//! debug equivalent of the compile-time `playwright-e2e` mock module: a
//! prod-feature build can be flipped to mock without recompiling when a
//! dev wants to inspect Cmdr's clipboard payloads without polluting the
//! system pasteboard.

use std::path::PathBuf;
use std::sync::LazyLock;

use objc2::ClassType;
use objc2::MainThreadMarker;
use objc2::msg_send;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_app_kit::{
    NSPasteboard, NSPasteboardReadingOptionKey, NSPasteboardTypeString, NSPasteboardURLReadingFileURLsOnlyKey,
};
use objc2_foundation::{NSArray, NSDictionary, NSString, NSURL};

use super::store;

/// Sampled once at first access. `true` when `CMDR_CLIPBOARD_BACKEND=mock`
/// was set in the process env at that point.
static USE_MOCK_BACKEND: LazyLock<bool> =
    LazyLock::new(|| std::env::var("CMDR_CLIPBOARD_BACKEND").as_deref() == Ok("mock"));

fn use_mock() -> bool {
    *USE_MOCK_BACKEND
}

/// Writes file URLs to the system pasteboard.
///
/// Places both file URL items (for Finder-compatible paste) and plain-text
/// newline-separated paths (for pasting into text editors).
pub fn write_file_urls_to_clipboard(_mtm: MainThreadMarker, paths: &[PathBuf]) -> Result<(), String> {
    if paths.is_empty() {
        return Err("No paths to write to clipboard".to_string());
    }

    if use_mock() {
        store::write_paths(paths);
        log::info!(target: "clipboard", "[mock-env] wrote {} file URL(s) to in-process clipboard", paths.len());
        return Ok(());
    }

    let pasteboard = NSPasteboard::generalPasteboard();

    // Build NSURL objects for each path
    let urls: Vec<Retained<NSURL>> = paths
        .iter()
        .map(|p| {
            let ns_path = NSString::from_str(&p.to_string_lossy());
            NSURL::fileURLWithPath(&ns_path)
        })
        .collect();

    // Clear existing contents
    pasteboard.clearContents();

    // Write file URLs via writeObjects. NSURL conforms to NSPasteboardWriting,
    // so we use msg_send! to pass the array without ProtocolObject generic juggling.
    let url_refs: Vec<&NSURL> = urls.iter().map(|u| &**u).collect();
    let url_array = NSArray::from_slice(&url_refs);
    // SAFETY: `pasteboard` is the live process-lifetime `generalPasteboard` singleton; `writeObjects:`
    // takes an `NSArray<id<NSPasteboardWriting>>` and `NSURL` conforms to `NSPasteboardWriting`, so
    // every element of `url_array` is a valid writer. Returns `BOOL`, decoded as `bool`. Main thread
    // proven by the `MainThreadMarker` parameter.
    let success: bool = unsafe { msg_send![&pasteboard, writeObjects: &*url_array] };
    if !success {
        return Err("NSPasteboard writeObjects returned false".to_string());
    }

    // Also write plain-text paths (newline-separated) so pasting into text editors works
    let text = paths.iter().map(|p| p.to_string_lossy()).collect::<Vec<_>>().join("\n");
    let ns_text = NSString::from_str(&text);
    // SAFETY: reading the AppKit-exported `NSPasteboardTypeString` global (a `&'static NSString`
    // constant the framework initializes at load); the deref just borrows that constant.
    let pasteboard_type = unsafe { NSPasteboardTypeString };
    pasteboard.setString_forType(&ns_text, pasteboard_type);

    log::info!("Wrote {} file URL(s) to clipboard", paths.len());
    Ok(())
}

/// Reads file URLs from the system pasteboard.
///
/// Uses `readObjectsForClasses:options:` with `NSURL` and `fileURLsOnly` to retrieve
/// only local file URLs (not remote HTTP URLs).
pub fn read_file_urls_from_clipboard(_mtm: MainThreadMarker) -> Result<Vec<PathBuf>, String> {
    if use_mock() {
        return Ok(store::read_paths());
    }

    let pasteboard = NSPasteboard::generalPasteboard();

    // Build class array containing NSURL's class
    let nsurl_class = NSURL::class();
    // SAFETY: `+[NSArray arrayWithObject:]` sent to the runtime-resolved `NSArray` class with
    // `nsurl_class` (NSURL's class object, an Objective-C `id`) as the single element. The selector
    // returns an autoreleased `NSArray`, decoded into a `Retained` (the +0 autoreleased result is
    // retained by objc2's return convention).
    let class_array: Retained<NSArray<objc2::runtime::AnyClass>> = unsafe {
        msg_send![
            objc2::runtime::AnyClass::get(c"NSArray").ok_or("NSArray class not found")?,
            arrayWithObject: nsurl_class,
        ]
    };

    // Options: fileURLsOnly = true
    // SAFETY: reading the AppKit-exported `NSPasteboardURLReadingFileURLsOnlyKey` global (a
    // `&'static NSPasteboardReadingOptionKey`, i.e. `NSString`, the framework initializes at load).
    let file_urls_only_key = unsafe { NSPasteboardURLReadingFileURLsOnlyKey };
    let yes_value: Retained<AnyObject> = unsafe {
        // SAFETY: `+[NSNumber numberWithBool:]` on the resolved `NSNumber` class returns an
        // autoreleased `NSNumber*`; `Retained::retain` claims +1 ownership and null-checks it.
        let cls = objc2::runtime::AnyClass::get(c"NSNumber").ok_or("NSNumber class not found")?;
        let obj: *mut AnyObject = msg_send![cls, numberWithBool: true];
        Retained::retain(obj).ok_or("Couldn't create NSNumber")?
    };

    let options: Retained<NSDictionary<NSPasteboardReadingOptionKey, AnyObject>> = unsafe {
        // SAFETY: `+[NSDictionary dictionaryWithObject:forKey:]` builds a single-entry dictionary
        // from the live `yes_value` (`NSNumber`) under the `file_urls_only_key` (`NSString`, conforms
        // to `NSCopying` as the key requires). Returns an autoreleased dictionary, retained into
        // `Retained` with the matching typed key/value parameters.
        msg_send![
            objc2::runtime::AnyClass::get(c"NSDictionary").ok_or("NSDictionary class not found")?,
            dictionaryWithObject: &*yes_value,
            forKey: file_urls_only_key,
        ]
    };

    // SAFETY: `class_array` is a live `NSArray<Class>` listing `NSURL`, `options` a live options
    // dictionary; `readObjectsForClasses:options:` reads matching items off the singleton pasteboard.
    // Main thread proven by the `MainThreadMarker` parameter.
    let objects = unsafe { pasteboard.readObjectsForClasses_options(&class_array, Some(&options)) };

    let Some(objects) = objects else {
        return Ok(Vec::new());
    };

    let count = objects.len();
    let mut paths = Vec::with_capacity(count);
    for i in 0..count {
        // SAFETY: `i` is in `0..objects.len()`, so `objectAtIndex:` returns a valid borrowed element
        // of the live `objects` array (an `NSURL`, since we filtered the read by class).
        let obj: &AnyObject = unsafe { msg_send![&objects, objectAtIndex: i] };
        // SAFETY: `obj` is an `NSURL`; `-path` returns an autoreleased `NSString*` (or nil for a
        // non-file URL), decoded as `Option<Retained<NSString>>`.
        let path_str: Option<Retained<NSString>> = unsafe { msg_send![obj, path] };
        if let Some(ns_str) = path_str {
            paths.push(PathBuf::from(ns_str.to_string()));
        }
    }

    Ok(paths)
}

/// Reads plain text from the system pasteboard.
///
/// Used by the frontend to paste text into input fields without triggering
/// WebKit's clipboard permission popup (which `navigator.clipboard.readText()` causes).
pub fn read_text_from_clipboard(_mtm: MainThreadMarker) -> Option<String> {
    if use_mock() {
        return store::read_text();
    }

    let pasteboard = NSPasteboard::generalPasteboard();
    // SAFETY: reading the AppKit-exported `NSPasteboardTypeString` global (a `&'static NSString`
    // constant the framework initializes at load).
    let pasteboard_type = unsafe { NSPasteboardTypeString };
    pasteboard.stringForType(pasteboard_type).map(|s| s.to_string())
}
