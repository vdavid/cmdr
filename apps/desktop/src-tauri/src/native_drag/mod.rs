//! Native multi-type drag for macOS.
//!
//! Replaces `drag::start_drag` so Cmdr advertises both `public.file-url`
//! AND `public.utf8-plain-text` on the pasteboard. Apps like Warp read the
//! text type to insert escaped paths at the cursor; without it they receive
//! nothing, because they don't subscribe to file URLs.
//!
//! ## Layout: locality-aware, decided once per session
//!
//! What each `NSPasteboardItem` advertises is a pure policy keyed on the drag
//! SESSION's locality (see [`type_plan`]), not branched per item. A single drag
//! can never mix local and virtual items (single-pane selections, single-volume
//! panes), so the plan is computed once for the whole session.
//!
//! - **Local sessions** (real local FS or OS-mounted shares) keep the legacy
//!   layout byte-for-byte:
//!   - One `NSDraggingItem` per file (Finder/IntelliJ iterate items reading file URLs).
//!   - `public.file-url` (the URL's `absoluteString`) on every item.
//!   - `public.utf8-plain-text` on every item: the first item carries all paths
//!     shell-escaped and space-joined (the "drop into terminal" gesture); later
//!     items carry just their own escaped path so item-iterating consumers don't
//!     see duplicates.
//!   - `NSFilenamesPboardType` (legacy `NSArray<NSString>` of all paths) on the
//!     first item only. Required for stock wry's `collect_paths`
//!     ([drag_drop.rs:18-32](https://github.com/tauri-apps/wry/blob/dev/src/wkwebview/drag_drop.rs#L18-L32)),
//!     which reads only this type and `unwrap()`s if it's missing. Drop this
//!     once [wry#1723](https://github.com/tauri-apps/wry/pull/1723) ships.
//! - **Virtual sessions** (MTP, direct SMB, search-results — paths with no local
//!   backing) carry no legacy types: no file-url, no text, no filenames, across
//!   EVERY item. A virtual path's `file://` URL is bogus and the legacy types are
//!   what Finder turned into a `.textClipping` junk file. Promise-only items
//!   still fire wry's drop event with empty paths (no panic), so in-app
//!   self-drags keep working via recorded identity. The `NSFilePromiseProvider`
//!   writer attached to each virtual item makes an external drop download the
//!   real bytes (see [`promises`] / [`fulfillment`]).
//!
//! The rich PNG icon is set as `setDraggingFrame:contents:` on every item
//! regardless of locality, so the existing `drag_image_swap` swizzle keeps
//! working unchanged (it operates on `NSDraggingItem`s regardless of writer
//! type), and the system-rendered count badge keeps working for >1 item.

pub mod fulfillment;
pub mod promises;
pub mod session_summary;
pub mod source;
pub mod type_plan;
pub mod uti;

pub use promises::set_app_handle;
pub use type_plan::DragSessionLocality;
use type_plan::{PasteboardItemPlan, plan_pasteboard_items};

use std::path::{Path, PathBuf};

use objc2::MainThreadMarker;
use objc2::msg_send;
use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject};
use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};

const NS_LEFT_MOUSE_DRAGGED: usize = 7;

const TYPE_FILE_URL: &str = "public.file-url";
const TYPE_STRING: &str = "public.utf8-plain-text";
/// Legacy pasteboard type carrying an `NSArray<NSString>` of file paths (no `file://` prefix).
/// Deprecated since 10.13. Required for stock wry's `collect_paths` (see module docs)
/// and as a defensive fallback for any old Mac app that reads only this type.
const TYPE_FILENAMES: &str = "NSFilenamesPboardType";

/// A monotonic key generator for virtual drag sessions. Used as the
/// `session_key` under which a session's promise delegates/providers register in
/// `promises::COUNTERS` and the source's end-callback frees them. A monotonic
/// counter (not the drag sequence number, which is only known AFTER the session
/// begins) lets us register the delegates BEFORE the drag starts — the weak
/// delegate refs must be alive the instant Finder might query them.
fn next_session_key() -> isize {
    use std::sync::atomic::{AtomicIsize, Ordering};
    static NEXT: AtomicIsize = AtomicIsize::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

/// Begins a native drag session for the given file paths. The pasteboard layout
/// is decided by `locality`:
///
/// - [`DragSessionLocality::Local`] advertises the legacy file-url + text +
///   filenames layout per item, and the source carries no promise session.
/// - [`DragSessionLocality::Virtual`] attaches an `NSFilePromiseProvider` to
///   each item (so dropping on Finder downloads the real bytes via the
///   fulfillment service) and NO materializable representations. The session's
///   delegates/providers register under a fresh `session_key` and are freed when
///   the gesture ends and fulfillments drain (see [`promises`]).
///
/// `source_volume_id` is the source volume for a virtual session (the id the
/// fulfillment service resolves to stream bytes). Ignored for local sessions.
/// Must be called on the AppKit main thread.
pub fn start_drag(
    window: &tauri::WebviewWindow,
    paths: Vec<PathBuf>,
    icon_path: &Path,
    locality: DragSessionLocality,
    source_volume_id: Option<&str>,
) -> Result<(), String> {
    if paths.is_empty() {
        return Err("No paths to drag".into());
    }
    if !icon_path.exists() {
        return Err(format!("Drag icon missing: {}", icon_path.display()));
    }

    let mtm = MainThreadMarker::new().ok_or("start_drag must run on the AppKit main thread")?;

    // Compose the per-item pasteboard plan ONCE for the whole session. Mixing
    // local and virtual items is impossible by construction (single-pane
    // selections, single-volume panes), so the plan is keyed on one locality
    // value, never branched per item.
    let path_strings: Vec<String> = paths.iter().map(|p| p.to_string_lossy().into_owned()).collect();
    let item_plans = plan_pasteboard_items(&path_strings, locality);

    // For a virtual session, build one promise provider per item up front (on
    // main). They register their delegates under `session_key` so the weak
    // delegate refs stay alive across the gesture and fulfillment. Local
    // sessions get `NO_PROMISE_SESSION` (no providers, end callback is a no-op).
    let (session_key, promise_providers) = match locality {
        DragSessionLocality::Virtual => {
            let key = next_session_key();
            let volume_id = source_volume_id.unwrap_or_default().to_string();
            let items: Vec<promises::PromiseItem> = paths
                .iter()
                .map(|path| {
                    let is_directory = false; // Resolved at fulfillment time; UTI uses the leaf below.
                    let leaf = path
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| "file".to_string());
                    let uti = uti::uti_for_item(&leaf, is_directory);
                    promises::PromiseItem {
                        leaf,
                        uti,
                        source_volume_id: volume_id.clone(),
                        source_path: path.clone(),
                    }
                })
                .collect();
            let providers = promises::build_session_providers(mtm, key, items);
            (key, Some(providers))
        }
        DragSessionLocality::Local => (source::NO_PROMISE_SESSION, None),
    };

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

        let pb_types = PasteboardTypes {
            nsurl_cls,
            nsmutarr_cls,
            file_url_type: NSString::from_str(TYPE_FILE_URL),
            string_type: NSString::from_str(TYPE_STRING),
            filenames_type: NSString::from_str(TYPE_FILENAMES),
        };

        // One `NSDraggingItem` per file regardless of locality, so the drag image
        // and system-rendered count badge are unaffected. The pasteboard WRITER
        // differs by locality:
        //
        // - Local: a plain `NSPasteboardItem` carrying the pre-computed plan
        //   (file-url + text + filenames).
        // - Virtual: the item's `NSFilePromiseProvider` (itself an
        //   `NSPasteboardWriting`), so an external drop downloads the real bytes.
        //   No legacy types — the provider is the whole payload.
        for (i, (path, plan)) in paths.iter().zip(item_plans.iter()).enumerate() {
            // The dragging-item writer: a promise provider for virtual sessions,
            // else a plan-filled `NSPasteboardItem`.
            let writer: *mut AnyObject = if let Some(providers) = promise_providers.as_ref() {
                // SAFETY: `NSFilePromiseProvider` conforms to `NSPasteboardWriting`,
                // which is what `initWithPasteboardWriter:` expects. One provider
                // per item, same order as `paths`.
                Retained::as_ptr(&providers[i]) as *mut AnyObject
            } else {
                let item_alloc: *mut AnyObject = msg_send![nspbi_cls, alloc];
                let item: *mut AnyObject = msg_send![item_alloc, init];
                apply_item_plan(item, plan, path, &pb_types)?;
                item
            };

            let drag_item_alloc: *mut AnyObject = msg_send![nsdi_cls, alloc];
            let drag_item: *mut AnyObject = msg_send![drag_item_alloc, initWithPasteboardWriter: writer];
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

        // `mouseEventWithType:...` is a class method: send to NSEvent itself.
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

        // The source carries the session key so its `endedAtPoint` callback can
        // free the virtual session's promise objects once fulfillments drain.
        let source = source::build_drag_source(mtm, session_key);

        let _: *mut AnyObject = msg_send![
            content_view,
            beginDraggingSessionWithItems: dragging_items,
            event: drag_event,
            source: &*source,
        ];
    }

    Ok(())
}

// --- Pasteboard item construction ---

/// AppKit class pointers and `NSString` type identifiers needed to materialize a
/// [`PasteboardItemPlan`]. Bundled so the per-item builder takes one borrow
/// instead of a long positional argument list.
struct PasteboardTypes<'a> {
    nsurl_cls: &'a AnyClass,
    nsmutarr_cls: &'a AnyClass,
    file_url_type: Retained<NSString>,
    string_type: Retained<NSString>,
    filenames_type: Retained<NSString>,
}

/// Writes the planned representations onto a freshly-allocated `NSPasteboardItem`.
///
/// A virtual-session plan is empty (every field `None`), so this attaches no
/// representations at all — the item carries no `file://` URL, no text, no
/// filenames. A local-session plan attaches `public.file-url` (the path's URL
/// `absoluteString`), `public.utf8-plain-text`, and (first item only)
/// `NSFilenamesPboardType`. See [`type_plan`] for the policy.
///
/// # Safety
///
/// `item` must be a valid `NSPasteboardItem`, `types` must hold valid AppKit
/// classes, and this must run on the AppKit main thread.
unsafe fn apply_item_plan(
    item: *mut AnyObject,
    plan: &PasteboardItemPlan,
    path: &Path,
    types: &PasteboardTypes,
) -> Result<(), String> {
    unsafe {
        if let Some(url_path) = plan.file_url.as_deref() {
            let path_ns = NSString::from_str(url_path);
            let url: *mut AnyObject = msg_send![
                types.nsurl_cls,
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
            let _: bool = msg_send![item, setString: abs_string, forType: &*types.file_url_type];
        }

        if let Some(text) = plan.text.as_deref() {
            let text_ns = NSString::from_str(text);
            let _: bool = msg_send![item, setString: &*text_ns, forType: &*types.string_type];
        }

        if let Some(filenames) = plan.filenames.as_deref() {
            // Legacy `NSFilenamesPboardType` (`NSArray<NSString>` of file paths).
            // See module docs: required for stock wry compatibility.
            let filenames_array: *mut AnyObject = msg_send![types.nsmutarr_cls, array];
            for name in filenames {
                let name_ns = NSString::from_str(name);
                let _: () = msg_send![filenames_array, addObject: &*name_ns];
            }
            let _: bool = msg_send![item, setPropertyList: filenames_array, forType: &*types.filenames_type];
        }
    }

    Ok(())
}
