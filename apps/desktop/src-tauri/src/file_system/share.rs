//! The macOS share sheet: `NSSharingServicePicker` over the selected files.
//!
//! One entry point, [`show_share_sheet`], reached from the file context menu's
//! `Share…` item. AirDrop, Mail, Messages, Notes, and every installed share
//! extension come from the system; Cmdr only hands over the file URLs and says
//! where the popover should hang.
//!
//! ❗ Everything here is main-thread-only AppKit, so [`show_share_sheet`] takes a
//! `MainThreadMarker` before touching anything and answers `NotOnMainThread` when
//! it can't get one.

use std::cell::RefCell;
use std::path::{Path, PathBuf};

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{AnyThread, MainThreadMarker};
use objc2_app_kit::{NSEvent, NSSharingServicePicker, NSView, NSWindow};
use objc2_foundation::{NSArray, NSPoint, NSRect, NSRectEdge, NSSize, NSString, NSURL};

/// Why a share sheet didn't open. Every variant is a state the caller can log
/// distinctly; none of them is a message.
#[derive(Debug, PartialEq, Eq)]
pub enum ShareError {
    /// The selection was empty, or none of it converted to a file URL.
    NothingToShare,
    /// Tauri handed back no `NSWindow` for the window we were asked to anchor to.
    WindowUnavailable,
    /// The window has no content view (it's mid-teardown).
    NoContentView,
    /// Reached from somewhere other than the AppKit main thread.
    NotOnMainThread,
}

/// How big the anchor rect is. The picker hangs off a rect, and the pointer is a
/// point, so it gets a small square: big enough that AppKit's own edge logic has
/// something to measure, small enough that the popover's arrow lands on the
/// pointer rather than beside it.
const ANCHOR_SIDE: f64 = 2.0;

/// The rect the picker hangs off, from the pointer and the view it has to land in.
///
/// The pointer is where the user's eye already is (they just clicked a menu item),
/// so that's the anchor. It's CLAMPED into `bounds` first, and that clamp is the
/// NORMAL path, not a rare guard: the file context menu is taller than the pane it
/// pops from, so `Share…` usually sits below the window's bottom edge and the raw
/// pointer lands outside the content view. `showRelativeToRect:` takes the rect at
/// face value, so unclamped the popover would go to a corner on nearly every share.
/// Clamped, it hangs off the window's nearest edge at the pointer's own x.
///
/// Both are in the anchoring view's own coordinates.
pub fn anchor_rect_in_view(pointer: NSPoint, bounds: NSRect) -> NSRect {
    let clamp = |value: f64, min: f64, len: f64| {
        // A degenerate view (zero or negative extent) has no inside to clamp into;
        // its origin is the only honest answer, and `f64::clamp` panics on min > max.
        if len <= 0.0 { min } else { value.clamp(min, min + len) }
    };
    NSRect {
        origin: NSPoint {
            x: clamp(pointer.x, bounds.origin.x, bounds.size.width) - ANCHOR_SIDE / 2.0,
            y: clamp(pointer.y, bounds.origin.y, bounds.size.height) - ANCHOR_SIDE / 2.0,
        },
        size: NSSize {
            width: ANCHOR_SIDE,
            height: ANCHOR_SIDE,
        },
    }
}

thread_local! {
    /// The picker currently on screen.
    ///
    /// `showRelativeToRect:ofView:preferredEdge:` does NOT take ownership: with the
    /// only strong reference living in a local, the picker is released the moment
    /// this function returns and the popover can vanish before it draws. Holding the
    /// last one here keeps it alive for as long as it's up, and the next share
    /// releases it. Thread-local rather than a `static`, because `Retained` is
    /// neither `Send` nor `Sync` and this only ever runs on the main thread.
    static LIVE_PICKER: RefCell<Option<Retained<NSSharingServicePicker>>> = const { RefCell::new(None) };
}

/// Opens the system share sheet over `paths`, anchored under the pointer.
///
/// `ns_window` is the raw `NSWindow` pointer Tauri hands out for the window the
/// context menu was popped from (`WebviewWindow::ns_window`). The popover hangs
/// off that window's content view, which is the only view Cmdr has: the file list
/// is DOM inside one `WKWebView`, so there's no per-row `NSView` to anchor to.
///
/// # Safety
///
/// `ns_window` must be a live `NSWindow` (or null, which is answered with
/// [`ShareError::WindowUnavailable`]).
pub unsafe fn show_share_sheet(ns_window: *mut std::ffi::c_void, paths: &[PathBuf]) -> Result<(), ShareError> {
    // The marker IS the main-thread proof every AppKit call below needs. Answered
    // rather than asserted: a caller that forgot to hop should get a log line, not
    // a panic inside an AppKit callback (which aborts the process).
    let _mtm: MainThreadMarker = MainThreadMarker::new().ok_or(ShareError::NotOnMainThread)?;
    if ns_window.is_null() {
        return Err(ShareError::WindowUnavailable);
    }
    // SAFETY: the caller's contract is that `ns_window` is a live `NSWindow`; it's
    // Tauri's own window pointer, which outlives this synchronous call because the
    // window can't close while the main thread is inside it. Null was rejected above.
    let window: &NSWindow = unsafe { &*ns_window.cast::<NSWindow>() };

    let urls = file_urls(paths);
    if urls.is_empty() {
        return Err(ShareError::NothingToShare);
    }
    let content_view = window.contentView().ok_or(ShareError::NoContentView)?;

    let picker = build_picker(&urls);
    let rect = anchor_rect_in_view(pointer_in_view(window, &content_view), content_view.bounds());
    // `NSRectEdge::MinY` is the BOTTOM edge in AppKit's unflipped view coordinates,
    // so the popover opens downward from the pointer, the way a menu does. AppKit
    // flips it up on its own when the screen has no room below. (The content view is
    // wry's `WKWebView` and is unflipped: verified on macOS 26.5.2, 2026-09-07, by
    // watching the sheet hang below its arrow.)
    picker.showRelativeToRect_ofView_preferredEdge(rect, &content_view, NSRectEdge::MinY);
    LIVE_PICKER.with(|slot| slot.replace(Some(picker)));
    Ok(())
}

/// The pointer, in the content view's coordinates.
///
/// Read at share time rather than carried from the right-click: the user clicked
/// `Share…` in a menu that opened at the right-click, so the pointer is already the
/// closest thing to "what they're looking at", and it needs no extra IPC field that
/// could go stale.
fn pointer_in_view(window: &NSWindow, content_view: &NSView) -> NSPoint {
    let on_screen = NSEvent::mouseLocation();
    let in_window = window.convertPointFromScreen(on_screen);
    content_view.convertPoint_fromView(in_window, None)
}

/// The selection as file URLs, dropping anything whose path isn't valid UTF-8.
///
/// A dropped path costs one item in the sheet; the alternative (refusing the whole
/// share) costs the user the gesture.
fn file_urls(paths: &[PathBuf]) -> Vec<Retained<NSURL>> {
    paths.iter().filter_map(|path| file_url(path)).collect()
}

fn file_url(path: &Path) -> Option<Retained<NSURL>> {
    let path_str = path.to_str()?;
    Some(NSURL::fileURLWithPath(&NSString::from_str(path_str)))
}

/// Wraps the URLs in the type-erased `NSArray` `initWithItems:` wants.
///
/// The picker takes anything conforming to `NSPasteboardWriting`, so its parameter
/// is `NSArray *` with no element type; `NSURL` is the right item for files and
/// folders, and it's what gives every service (AirDrop, Mail, Messages) the file
/// itself rather than a rendering of it.
fn build_picker(urls: &[Retained<NSURL>]) -> Retained<NSSharingServicePicker> {
    let items: Vec<&AnyObject> = urls.iter().map(|url| AsRef::<AnyObject>::as_ref(&**url)).collect();
    let items: Retained<NSArray> = NSArray::from_slice(&items);
    // SAFETY: `initWithItems:` requires each element to conform to
    // `NSPasteboardWriting`, be an `NSItemProvider`, or be an `NSDocument`. Every
    // element here is an `NSURL`, which conforms to `NSPasteboardWriting`, so the
    // "generic should be of the correct type" precondition on the binding holds.
    unsafe { NSSharingServicePicker::initWithItems(NSSharingServicePicker::alloc(), &items) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bounds() -> NSRect {
        NSRect {
            origin: NSPoint { x: 0.0, y: 0.0 },
            size: NSSize {
                width: 1000.0,
                height: 700.0,
            },
        }
    }

    /// The centre of the anchor rect is the point the popover's arrow aims at, so
    /// every assertion below reads it back rather than the origin.
    fn centre(rect: NSRect) -> (f64, f64) {
        (
            rect.origin.x + rect.size.width / 2.0,
            rect.origin.y + rect.size.height / 2.0,
        )
    }

    #[test]
    fn a_pointer_inside_the_view_anchors_where_it_is() {
        let rect = anchor_rect_in_view(NSPoint { x: 420.0, y: 310.0 }, bounds());
        assert_eq!(centre(rect), (420.0, 310.0));
        assert!(rect.size.width > 0.0 && rect.size.height > 0.0);
    }

    #[test]
    fn a_pointer_past_the_edge_is_pulled_back_onto_it() {
        // Not an exotic case: the context menu is taller than the pane, so `Share…`
        // usually sits below the window and this is what runs. Unclamped it's the
        // "picker in the wrong corner" bug — AppKit takes the rect literally.
        let far = anchor_rect_in_view(NSPoint { x: 5000.0, y: -400.0 }, bounds());
        assert_eq!(centre(far), (1000.0, 0.0));
    }

    #[test]
    fn a_pointer_just_below_the_view_keeps_its_x_and_lands_on_the_bottom_edge() {
        // The measured real case: clicking `Share…` put the pointer 10 pt under the
        // content view, and the popover's arrow has to stay on the pointer's column.
        let just_below = anchor_rect_in_view(NSPoint { x: 87.0, y: -10.0 }, bounds());
        assert_eq!(centre(just_below), (87.0, 0.0));
    }

    #[test]
    fn a_view_with_a_non_zero_origin_clamps_against_its_own_frame() {
        let shifted = NSRect {
            origin: NSPoint { x: 100.0, y: 50.0 },
            size: NSSize {
                width: 200.0,
                height: 100.0,
            },
        };
        assert_eq!(
            centre(anchor_rect_in_view(NSPoint { x: 0.0, y: 0.0 }, shifted)),
            (100.0, 50.0)
        );
        assert_eq!(
            centre(anchor_rect_in_view(NSPoint { x: 9999.0, y: 9999.0 }, shifted)),
            (300.0, 150.0)
        );
    }

    #[test]
    fn a_degenerate_view_answers_with_its_origin_instead_of_nan() {
        // A window mid-teardown reports a zero-sized content view. `f64::clamp`
        // panics when min > max, and this runs inside an AppKit callback that
        // aborts on panic, so the empty case has to be answered, not computed.
        let empty = NSRect {
            origin: NSPoint { x: 12.0, y: 34.0 },
            size: NSSize {
                width: 0.0,
                height: 0.0,
            },
        };
        assert_eq!(
            centre(anchor_rect_in_view(NSPoint { x: 500.0, y: 500.0 }, empty)),
            (12.0, 34.0)
        );
    }

    #[test]
    fn a_path_that_isnt_utf8_is_skipped_rather_than_sinking_the_whole_share() {
        use std::ffi::OsString;
        use std::os::unix::ffi::OsStringExt;

        let broken = PathBuf::from(OsString::from_vec(vec![b'/', 0xff, 0xfe]));
        let urls = file_urls(&[PathBuf::from("/tmp/ok.txt"), broken]);
        assert_eq!(urls.len(), 1);
    }
}
