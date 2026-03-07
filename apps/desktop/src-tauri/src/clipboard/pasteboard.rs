//! macOS NSPasteboard FFI for file URL clipboard operations.
//!
//! All functions assume they are called on the main thread. Callers must use
//! `app.run_on_main_thread()` when invoking from async Tauri commands.

use std::path::PathBuf;

use objc2::ClassType;
use objc2::msg_send;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_app_kit::{
    NSPasteboard, NSPasteboardReadingOptionKey, NSPasteboardTypeString, NSPasteboardURLReadingFileURLsOnlyKey,
};
use objc2_foundation::{NSArray, NSDictionary, NSString, NSURL};

/// Writes file URLs to the system pasteboard.
///
/// Places both file URL items (for Finder-compatible paste) and plain-text
/// newline-separated paths (for pasting into text editors).
pub fn write_file_urls_to_clipboard(paths: &[PathBuf]) -> Result<(), String> {
    if paths.is_empty() {
        return Err("No paths to write to clipboard".to_string());
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
    let success: bool = unsafe { msg_send![&pasteboard, writeObjects: &*url_array] };
    if !success {
        return Err("NSPasteboard writeObjects returned false".to_string());
    }

    // Also write plain-text paths (newline-separated) so pasting into text editors works
    let text = paths.iter().map(|p| p.to_string_lossy()).collect::<Vec<_>>().join("\n");
    let ns_text = NSString::from_str(&text);
    let pasteboard_type = unsafe { NSPasteboardTypeString };
    pasteboard.setString_forType(&ns_text, pasteboard_type);

    log::info!("Wrote {} file URL(s) to clipboard", paths.len());
    Ok(())
}

/// Reads file URLs from the system pasteboard.
///
/// Uses `readObjectsForClasses:options:` with `NSURL` and `fileURLsOnly` to retrieve
/// only local file URLs (not remote HTTP URLs).
pub fn read_file_urls_from_clipboard() -> Result<Vec<PathBuf>, String> {
    let pasteboard = NSPasteboard::generalPasteboard();

    // Build class array containing NSURL's class
    let nsurl_class = NSURL::class();
    let class_array: Retained<NSArray<objc2::runtime::AnyClass>> = unsafe {
        // NSArray<AnyClass> from a single class pointer
        msg_send![
            objc2::runtime::AnyClass::get(c"NSArray").ok_or("NSArray class not found")?,
            arrayWithObject: nsurl_class,
        ]
    };

    // Options: fileURLsOnly = true
    let file_urls_only_key = unsafe { NSPasteboardURLReadingFileURLsOnlyKey };
    let yes_value: Retained<AnyObject> = unsafe {
        let cls = objc2::runtime::AnyClass::get(c"NSNumber").ok_or("NSNumber class not found")?;
        let obj: *mut AnyObject = msg_send![cls, numberWithBool: true];
        Retained::retain(obj).ok_or("Couldn't create NSNumber")?
    };

    let options: Retained<NSDictionary<NSPasteboardReadingOptionKey, AnyObject>> = unsafe {
        msg_send![
            objc2::runtime::AnyClass::get(c"NSDictionary").ok_or("NSDictionary class not found")?,
            dictionaryWithObject: &*yes_value,
            forKey: file_urls_only_key,
        ]
    };

    let objects = unsafe { pasteboard.readObjectsForClasses_options(&class_array, Some(&options)) };

    let Some(objects) = objects else {
        return Ok(Vec::new());
    };

    let count = objects.len();
    let mut paths = Vec::with_capacity(count);
    for i in 0..count {
        let obj: &AnyObject = unsafe { msg_send![&objects, objectAtIndex: i] };
        let path_str: Option<Retained<NSString>> = unsafe { msg_send![obj, path] };
        if let Some(ns_str) = path_str {
            paths.push(PathBuf::from(ns_str.to_string()));
        }
    }

    Ok(paths)
}
