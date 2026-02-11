//! Self-drag image swapping for macOS.
//!
//! When dragging files between panes (self-drag), swaps the OS drag image:
//! - **Inside the window**: transparent 1x1 image (hidden, DOM overlay takes over)
//! - **Outside the window**: rich canvas-rendered PNG (restored on `draggingExited:`)
//!
//! Uses `NSDraggingItem.setDraggingFrame:contents:` to change images mid-drag.
//!
//! ## Timing invariant
//!
//! `SELF_DRAG_ACTIVE` is set via IPC from the frontend and read from AppKit swizzle
//! callbacks on the main thread. The `@crabnebula/tauri-plugin-drag` `startDrag()`
//! resolves **before** macOS delivers `draggingEntered:`/`draggingExited:` events.
//! Therefore, state must **never** be cleared from JS async callbacks that run after
//! `startDrag` resolves — they race with the swizzle. State is only cleared on drop
//! (via the Tauri `clear_self_drag_overlay` command from the frontend drop handler).

use std::ffi::CString;
use std::ptr::NonNull;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use objc2::msg_send;
use objc2::runtime::{AnyClass, AnyObject, Bool};
use objc2_app_kit::{NSDraggingItem, NSDraggingItemEnumerationOptions};
use objc2_foundation::{NSDictionary, NSInteger, NSPoint, NSRect, NSSize};

use crate::drag_image_detection::warn_once;

/// Whether a self-drag is active. When true, `draggingEntered:` swaps the OS drag image
/// to transparent so the DOM overlay provides visual feedback instead.
static SELF_DRAG_ACTIVE: AtomicBool = AtomicBool::new(false);
/// Path to the rich drag image for self-drags. Used by `draggingExited:` to swap back
/// to the rich image when the cursor leaves the window.
static SELF_DRAG_RICH_PATH: Mutex<Option<String>> = Mutex::new(None);

// Warn-once flags
static WARNED_NSIMAGE_MISSING: AtomicBool = AtomicBool::new(false);

// --- Public API for self-drag state ---

/// Marks a self-drag as active and stores the rich image path.
/// `draggingEntered:` swaps items to transparent (DOM overlay takes over).
/// `draggingExited:` swaps items back to the rich image (visible outside the window).
pub fn set_self_drag_active(rich_image_path: String) {
    SELF_DRAG_ACTIVE.store(true, Ordering::Relaxed);
    if let Ok(mut guard) = SELF_DRAG_RICH_PATH.lock() {
        *guard = Some(rich_image_path);
    }
}

/// Clears the self-drag state. Call on drop completion or drag cancellation.
pub fn clear_self_drag_state() {
    SELF_DRAG_ACTIVE.store(false, Ordering::Relaxed);
    if let Ok(mut guard) = SELF_DRAG_RICH_PATH.lock() {
        *guard = None;
    }
}

// --- Swizzle hooks (called from drag_image_detection swizzles) ---

/// Called from the `draggingEntered:` swizzle. If a self-drag is active, swaps all
/// drag items to a transparent 1x1 image so the OS cursor is invisible over our window.
pub unsafe fn on_drag_entered(drag_info: &AnyObject) {
    if !SELF_DRAG_ACTIVE.load(Ordering::Relaxed) {
        return;
    }
    let transparent = unsafe { create_transparent_nsimage() };
    if !transparent.is_null() {
        unsafe { swap_drag_items_to_image(drag_info, transparent) };
    }
}

/// Called from the `draggingExited:` swizzle. Swaps drag items back to the rich PNG
/// so it's visible outside the window.
pub unsafe fn on_drag_exited(drag_info: &AnyObject) {
    let rich_path = SELF_DRAG_RICH_PATH.lock().ok().and_then(|g| g.as_ref().cloned());
    if let Some(path) = rich_path {
        let image = unsafe { load_nsimage_from_path(&path) };
        if !image.is_null() {
            unsafe { swap_drag_items_to_image(drag_info, image) };
        }
    }
}

// --- NSImage helpers ---

/// Creates a 1x1 transparent NSImage (no image representations = fully transparent).
unsafe fn create_transparent_nsimage() -> *mut AnyObject {
    let Some(cls) = AnyClass::get(c"NSImage") else {
        warn_once(
            &WARNED_NSIMAGE_MISSING,
            "drag_image_swap: NSImage class not found — drag image swapping is disabled. \
             NSImage is a core AppKit class; if it's missing, check whether the framework \
             is loaded correctly.",
        );
        return std::ptr::null_mut();
    };
    let size = NSSize {
        width: 1.0,
        height: 1.0,
    };
    unsafe {
        let image: *mut AnyObject = msg_send![cls, alloc];
        msg_send![image, initWithSize: size]
    }
}

/// Loads an NSImage from a file path. Returns null if the file doesn't exist or can't be loaded.
unsafe fn load_nsimage_from_path(path: &str) -> *mut AnyObject {
    let Some(nsimage_cls) = AnyClass::get(c"NSImage") else {
        warn_once(
            &WARNED_NSIMAGE_MISSING,
            "drag_image_swap: NSImage class not found — drag image swapping is disabled. \
             NSImage is a core AppKit class; if it's missing, check whether the framework \
             is loaded correctly.",
        );
        return std::ptr::null_mut();
    };
    let Some(nsstring_cls) = AnyClass::get(c"NSString") else {
        return std::ptr::null_mut();
    };
    let Ok(c_path) = CString::new(path) else {
        return std::ptr::null_mut();
    };
    unsafe {
        let ns_path: *mut AnyObject = msg_send![nsstring_cls, stringWithUTF8String: c_path.as_ptr()];
        if ns_path.is_null() {
            return std::ptr::null_mut();
        }
        let image: *mut AnyObject = msg_send![nsimage_cls, alloc];
        msg_send![image, initWithContentsOfFile: ns_path]
    }
}

/// Swaps the drag image on all dragging items to the given NSImage.
/// Uses enumerateDraggingItems + setDraggingFrame:contents: to update the visual mid-drag.
unsafe fn swap_drag_items_to_image(drag_info: &AnyObject, image: *mut AnyObject) {
    if image.is_null() {
        return;
    }

    let image_size: NSSize = unsafe { msg_send![image, size] };
    let location: NSPoint = unsafe { msg_send![drag_info, draggingLocation] };

    let frame = NSRect {
        origin: NSPoint {
            x: location.x - image_size.width / 2.0,
            y: location.y - image_size.height / 2.0,
        },
        size: image_size,
    };

    let Some(nsurl_cls) = AnyClass::get(c"NSURL") else {
        return;
    };
    let Some(nsarray_cls) = AnyClass::get(c"NSArray") else {
        return;
    };
    let class_array: *const AnyObject =
        unsafe { msg_send![nsarray_cls, arrayWithObject: nsurl_cls as *const AnyClass] };
    if class_array.is_null() {
        return;
    }

    let empty_dict_owned = NSDictionary::new();
    let empty_dict: &NSDictionary = empty_dict_owned.as_ref();

    let block = block2::RcBlock::new(
        move |item: NonNull<NSDraggingItem>, _idx: NSInteger, _stop: NonNull<Bool>| unsafe {
            let _: () = msg_send![item.as_ptr(), setDraggingFrame: frame, contents: image];
        },
    );

    let opts = NSDraggingItemEnumerationOptions(0);
    unsafe {
        let _: () = msg_send![
            drag_info,
            enumerateDraggingItemsWithOptions: opts.0,
            forView: std::ptr::null::<AnyObject>(),
            classes: class_array,
            searchOptions: empty_dict,
            usingBlock: &*block,
        ];
    }
}
