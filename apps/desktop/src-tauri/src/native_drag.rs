//! Native multi-type drag for macOS.
//!
//! Replaces `drag::start_drag` so Cmdr advertises both `public.file-url`
//! AND `public.utf8-plain-text` on the pasteboard. Apps like Warp read the
//! text type to insert escaped paths at the cursor; without it they receive
//! nothing, because they don't subscribe to file URLs.
//!
//! ## Layout
//!
//! - One `NSDraggingItem` per file (Finder/IntelliJ iterate items reading file URLs).
//! - Each item's `NSPasteboardItem` vends `public.file-url`.
//! - The first item additionally vends `public.utf8-plain-text` with all paths
//!   shell-escaped and space-joined (so `pasteboard.string(forType:)` returns the
//!   joined list — the standard "drop into terminal" gesture).
//! - The first item also vends `NSFilenamesPboardType` (legacy `NSArray<NSString>`
//!   of all paths). Required for compatibility with stock wry's `collect_paths`
//!   ([drag_drop.rs:18-32](https://github.com/tauri-apps/wry/blob/dev/src/wkwebview/drag_drop.rs#L18-L32))
//!   which reads only this type and `unwrap()`s if it's missing — and for any
//!   pre-10.13 Mac app that still calls `propertyListForType(NSFilenamesPboardType)`
//!   directly. Drop this once [wry#1723](https://github.com/tauri-apps/wry/pull/1723)
//!   is merged and a wry release containing it ships through `tauri-runtime-wry`.
//! - The rich PNG icon is set as `setDraggingFrame:contents:` on every item, so
//!   the existing `drag_image_swap` swizzle keeps working unchanged (it operates
//!   on `NSDraggingItem`s regardless of writer type).

use std::path::{Path, PathBuf};
use std::ptr::NonNull;
use std::sync::OnceLock;

use objc2::msg_send;
use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject, ClassBuilder, Sel};
use objc2::sel;
use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};

const NS_LEFT_MOUSE_DRAGGED: usize = 7;

/// Permissive operation mask published to the destination: `Copy | Link | Generic | Move`.
/// macOS arbitrates the chosen operation based on modifier keys (Alt → Copy, Cmd → Move,
/// Ctrl-Alt → Link) and the destination's preference. Restricting the mask up-front makes
/// destinations like Warp reject the drop entirely (terminals only accept Copy).
const PERMISSIVE_OP_MASK: usize = 1 | 2 | 4 | 16;

const TYPE_FILE_URL: &str = "public.file-url";
const TYPE_STRING: &str = "public.utf8-plain-text";
/// Legacy pasteboard type carrying an `NSArray<NSString>` of file paths (no `file://` prefix).
/// Deprecated since 10.13. Required for stock wry's `collect_paths` (see module docs)
/// and as a defensive fallback for any old Mac app that reads only this type.
const TYPE_FILENAMES: &str = "NSFilenamesPboardType";

/// Begins a native drag session for the given file paths, advertising both
/// file URLs and shell-escaped text. Must be called on the AppKit main thread.
pub fn start_drag(window: &tauri::WebviewWindow, paths: Vec<PathBuf>, icon_path: &Path) -> Result<(), String> {
    if paths.is_empty() {
        return Err("No paths to drag".into());
    }
    if !icon_path.exists() {
        return Err(format!("Drag icon missing: {}", icon_path.display()));
    }

    let ns_window_ptr = window.ns_window().map_err(|e| format!("ns_window unavailable: {e}"))? as *mut AnyObject;
    if ns_window_ptr.is_null() {
        return Err("NSWindow pointer is null".into());
    }

    unsafe {
        let window: *mut AnyObject = ns_window_ptr;
        let content_view: *mut AnyObject = msg_send![window, contentView];
        if content_view.is_null() {
            return Err("contentView not found".into());
        }

        // Load the rich PNG icon as NSImage.
        let nsimage_cls = AnyClass::get(c"NSImage").ok_or("NSImage class missing")?;
        let icon_path_ns = NSString::from_str(&icon_path.to_string_lossy());
        let img_alloc: *mut AnyObject = msg_send![nsimage_cls, alloc];
        let img: *mut AnyObject = msg_send![img_alloc, initByReferencingFile: &*icon_path_ns];
        if img.is_null() {
            return Err("Failed to load drag icon".into());
        }

        let image_size: NSSize = msg_send![img, size];
        let cursor: NSPoint = msg_send![window, mouseLocationOutsideOfEventStream];
        let image_rect = NSRect {
            origin: NSPoint {
                x: cursor.x - image_size.width / 2.0,
                y: cursor.y - image_size.height / 2.0,
            },
            size: image_size,
        };

        // Build dragging items.
        let nsmutarr_cls = AnyClass::get(c"NSMutableArray").ok_or("NSMutableArray missing")?;
        let dragging_items: *mut AnyObject = msg_send![nsmutarr_cls, array];
        let nspbi_cls = AnyClass::get(c"NSPasteboardItem").ok_or("NSPasteboardItem missing")?;
        let nsdi_cls = AnyClass::get(c"NSDraggingItem").ok_or("NSDraggingItem missing")?;
        let nsurl_cls = AnyClass::get(c"NSURL").ok_or("NSURL missing")?;

        let file_url_type = NSString::from_str(TYPE_FILE_URL);
        let string_type = NSString::from_str(TYPE_STRING);
        let filenames_type = NSString::from_str(TYPE_FILENAMES);

        // Joined, shell-escaped text for the first item — what terminals get
        // when they read `pasteboard.string(forType:)`.
        let joined_text = paths
            .iter()
            .map(|p| shell_escape(&p.to_string_lossy()))
            .collect::<Vec<_>>()
            .join(" ");

        // First item also carries `NSFilenamesPboardType` (legacy NSArray<NSString> of
        // file paths). See module docs: required for stock wry compatibility and
        // pre-10.13 Mac apps. Build the array up-front so we can attach it below.
        let filenames_array: *mut AnyObject = msg_send![nsmutarr_cls, array];
        for path in &paths {
            let path_ns = NSString::from_str(&path.to_string_lossy());
            let _: () = msg_send![filenames_array, addObject: &*path_ns];
        }

        for (i, path) in paths.iter().enumerate() {
            let path_ns = NSString::from_str(&path.to_string_lossy());
            let url: *mut AnyObject = msg_send![
                nsurl_cls,
                fileURLWithPath: &*path_ns,
                isDirectory: false,
            ];
            if url.is_null() {
                return Err(format!("Failed to build URL for {}", path.display()));
            }

            // `public.file-url` wants the URL's absolute string (`file:///...` with
            // percent-encoded path). `setPropertyList:` with a serialized form was
            // misparsed by AppKit ("An invalid URL was found on the pasteboard") and
            // broke wry's `propertyListForType(NSFilenamesPboardType)` derivation.
            let abs_string: *mut AnyObject = msg_send![url, absoluteString];
            if abs_string.is_null() {
                return Err(format!("URL absoluteString returned nil for {}", path.display()));
            }

            let item_alloc: *mut AnyObject = msg_send![nspbi_cls, alloc];
            let item: *mut AnyObject = msg_send![item_alloc, init];
            let _: bool = msg_send![item, setString: abs_string, forType: &*file_url_type];

            // First item carries the joined text and the legacy filenames array; later
            // items just carry their own path so iterating consumers don't see duplicates.
            let text = if i == 0 {
                joined_text.clone()
            } else {
                shell_escape(&path.to_string_lossy())
            };
            let text_ns = NSString::from_str(&text);
            let _: bool = msg_send![item, setString: &*text_ns, forType: &*string_type];

            if i == 0 {
                let _: bool = msg_send![item, setPropertyList: filenames_array, forType: &*filenames_type];
            }

            let drag_item_alloc: *mut AnyObject = msg_send![nsdi_cls, alloc];
            let drag_item: *mut AnyObject = msg_send![drag_item_alloc, initWithPasteboardWriter: item];
            let _: () = msg_send![drag_item, setDraggingFrame: image_rect, contents: img];

            let _: () = msg_send![dragging_items, addObject: drag_item];
        }

        // Build a synthetic mouse-drag event. `beginDraggingSessionWithItems:event:source:`
        // requires an event; the current event in NSApp may be stale by the time we
        // reach the main-thread closure, so we fabricate one from the cursor location.
        let nsevent_cls = AnyClass::get(c"NSEvent").ok_or("NSEvent missing")?;
        let nsapp_cls = AnyClass::get(c"NSApplication").ok_or("NSApplication missing")?;
        let app: *mut AnyObject = msg_send![nsapp_cls, sharedApplication];
        let current_event: *mut AnyObject = msg_send![app, currentEvent];
        let timestamp: f64 = if current_event.is_null() {
            0.0
        } else {
            msg_send![current_event, timestamp]
        };
        let window_number: isize = msg_send![window, windowNumber];

        // `mouseEventWithType:...` is a class method — send to NSEvent itself.
        let drag_event: *mut AnyObject = msg_send![
            nsevent_cls,
            mouseEventWithType: NS_LEFT_MOUSE_DRAGGED,
            location: cursor,
            modifierFlags: 0usize,
            timestamp: timestamp,
            windowNumber: window_number,
            context: std::ptr::null_mut::<AnyObject>(),
            eventNumber: 0isize,
            clickCount: 1isize,
            pressure: 1.0f32,
        ];
        if drag_event.is_null() {
            return Err("Failed to build drag event".into());
        }

        let source = build_drag_source();

        let _: *mut AnyObject = msg_send![
            content_view,
            beginDraggingSessionWithItems: dragging_items,
            event: drag_event,
            source: &*source,
        ];
    }

    Ok(())
}

// --- Drag source class ---

static SOURCE_CLASS: OnceLock<&'static AnyClass> = OnceLock::new();

/// Registers the drag source class once. The class implements
/// `NSDraggingSource`'s required method, returning [`PERMISSIVE_OP_MASK`].
fn source_class() -> &'static AnyClass {
    SOURCE_CLASS.get_or_init(|| {
        let superclass = AnyClass::get(c"NSObject").expect("NSObject class missing");
        let mut builder =
            ClassBuilder::new(c"CmdrDragSource", superclass).expect("CmdrDragSource: class registration failed");

        unsafe {
            builder.add_method(
                sel!(draggingSession:sourceOperationMaskForDraggingContext:),
                operation_mask as unsafe extern "C-unwind" fn(_, _, _, _) -> _,
            );
        }

        builder.register()
    })
}

unsafe extern "C-unwind" fn operation_mask(
    _this: NonNull<AnyObject>,
    _: Sel,
    _session: *mut AnyObject,
    _context: usize,
) -> usize {
    PERMISSIVE_OP_MASK
}

fn build_drag_source() -> Retained<AnyObject> {
    let cls = source_class();
    unsafe {
        let alloc: *mut AnyObject = msg_send![cls, alloc];
        let init: *mut AnyObject = msg_send![alloc, init];
        Retained::from_raw(init).expect("CmdrDragSource init returned nil")
    }
}

// --- Shell escaping ---

/// Single-quotes a path for paste into a POSIX shell. Returns the input unchanged
/// if it only contains characters that are universally safe outside quoting.
fn shell_escape(s: &str) -> String {
    let safe = !s.is_empty()
        && s.chars().all(|c| {
            c.is_ascii_alphanumeric() || matches!(c, '/' | '.' | '_' | '-' | '+' | ',' | ':' | '@' | '%' | '=')
        });
    if safe {
        s.to_string()
    } else {
        format!("'{}'", s.replace('\'', "'\\''"))
    }
}

#[cfg(test)]
mod tests {
    use super::shell_escape;

    #[test]
    fn shell_escape_safe_passthrough() {
        assert_eq!(shell_escape("/Users/me/file.jpg"), "/Users/me/file.jpg");
        assert_eq!(shell_escape("plain"), "plain");
    }

    #[test]
    fn shell_escape_quotes_spaces_and_unicode() {
        assert_eq!(shell_escape("/has space/x.jpg"), "'/has space/x.jpg'");
        assert_eq!(shell_escape("Anna fotók"), "'Anna fotók'");
    }

    #[test]
    fn shell_escape_handles_inner_single_quote() {
        assert_eq!(shell_escape("it's"), "'it'\\''s'");
    }

    #[test]
    fn shell_escape_empty_is_quoted() {
        assert_eq!(shell_escape(""), "''");
    }
}
