//! Native drag interception for macOS via method swizzling on WryWebView.
//!
//! Swizzles `draggingEntered:` and `draggingUpdated:` to:
//! 1. Read drag image dimensions via `enumerateDraggingItems` (for overlay suppression)
//! 2. Read modifier key state via `[NSEvent modifierFlags]` (for copy/move detection)
//!
//! Events emitted:
//! - `drag-image-size` `{ width, height }` — on drag enter
//! - `drag-modifiers` `{ altHeld }` — on drag enter and every drag update (only when changed)

use std::ptr::NonNull;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

use objc2::runtime::{AnyClass, AnyObject, Bool, Imp, Sel};
use objc2::{msg_send, sel};
use objc2_app_kit::{NSDragOperation, NSDraggingItem, NSDraggingItemEnumerationOptions};
use objc2_foundation::{NSDictionary, NSInteger, NSRect};
use serde::Serialize;
use tauri::{AppHandle, Emitter};

/// NSEventModifierFlagOption = 1 << 19 (Option/Alt key)
const NS_EVENT_MODIFIER_FLAG_OPTION: usize = 1 << 19;

#[derive(Clone, Serialize)]
struct DragImageSize {
    width: f64,
    height: f64,
}

#[derive(Clone, Serialize)]
struct DragModifiers {
    #[serde(rename = "altHeld")]
    alt_held: bool,
}

static ORIGINAL_ENTERED_IMP: OnceLock<Imp> = OnceLock::new();
static ORIGINAL_UPDATED_IMP: OnceLock<Imp> = OnceLock::new();
static APP_HANDLE: OnceLock<AppHandle> = OnceLock::new();

/// Tracks previous alt state so we only emit `drag-modifiers` when it changes.
static LAST_ALT_HELD: AtomicBool = AtomicBool::new(false);

/// Installs swizzles on WryWebView. Call once during app setup.
pub fn install(app_handle: AppHandle) {
    APP_HANDLE.set(app_handle).ok();

    unsafe {
        let Some(cls) = AnyClass::get(c"WryWebView") else {
            log::warn!("drag_image_detection: WryWebView class not found, skipping swizzle");
            return;
        };

        // Swizzle draggingEntered:
        if let Some(method) = cls.instance_method(sel!(draggingEntered:)) {
            ORIGINAL_ENTERED_IMP.set(method.implementation()).ok();
            method.set_implementation(std::mem::transmute::<*const (), Imp>(
                swizzled_dragging_entered as *const (),
            ));
        } else {
            log::warn!("drag_image_detection: draggingEntered: not found on WryWebView");
        }

        // Swizzle draggingUpdated:
        if let Some(method) = cls.instance_method(sel!(draggingUpdated:)) {
            ORIGINAL_UPDATED_IMP.set(method.implementation()).ok();
            method.set_implementation(std::mem::transmute::<*const (), Imp>(
                swizzled_dragging_updated as *const (),
            ));
        } else {
            log::warn!("drag_image_detection: draggingUpdated: not found on WryWebView");
        }

        log::info!("drag_image_detection: swizzles installed on WryWebView");
    }
}

/// Reads the current Option/Alt key state from `[NSEvent modifierFlags]`.
/// This is a class method that reads hardware state — works even when the webview isn't focused.
fn is_option_held() -> bool {
    let flags: usize = unsafe { msg_send![AnyClass::get(c"NSEvent").unwrap(), modifierFlags] };
    flags & NS_EVENT_MODIFIER_FLAG_OPTION != 0
}

/// Emits `drag-modifiers` if the alt state changed since last emission.
fn emit_modifiers_if_changed() {
    let alt_held = is_option_held();
    let prev = LAST_ALT_HELD.swap(alt_held, Ordering::Relaxed);
    if alt_held != prev
        && let Some(app_handle) = APP_HANDLE.get()
    {
        let _ = app_handle.emit("drag-modifiers", DragModifiers { alt_held });
    }
}

/// Emits `drag-modifiers` unconditionally (used on drag enter to set initial state).
fn emit_modifiers_forced() {
    let alt_held = is_option_held();
    LAST_ALT_HELD.store(alt_held, Ordering::Relaxed);
    if let Some(app_handle) = APP_HANDLE.get() {
        let _ = app_handle.emit("drag-modifiers", DragModifiers { alt_held });
    }
}

// --- draggingEntered: swizzle ---

unsafe extern "C-unwind" fn swizzled_dragging_entered(this: &AnyObject, cmd: Sel, drag_info: &AnyObject) -> usize {
    let size = unsafe { read_drag_image_size(drag_info) };

    if let Some(app_handle) = APP_HANDLE.get() {
        let _ = app_handle.emit(
            "drag-image-size",
            DragImageSize {
                width: size.0,
                height: size.1,
            },
        );
    }

    // Always emit modifiers on enter (initial state for this drag session)
    emit_modifiers_forced();

    if let Some(&original) = ORIGINAL_ENTERED_IMP.get() {
        let f = unsafe {
            std::mem::transmute::<Imp, unsafe extern "C-unwind" fn(&AnyObject, Sel, &AnyObject) -> usize>(original)
        };
        unsafe { f(this, cmd, drag_info) }
    } else {
        NSDragOperation::Copy.0
    }
}

// --- draggingUpdated: swizzle ---

unsafe extern "C-unwind" fn swizzled_dragging_updated(this: &AnyObject, cmd: Sel, drag_info: &AnyObject) -> usize {
    // Only emit when modifier state changes (avoids flooding on every mouse move)
    emit_modifiers_if_changed();

    if let Some(&original) = ORIGINAL_UPDATED_IMP.get() {
        let f = unsafe {
            std::mem::transmute::<Imp, unsafe extern "C-unwind" fn(&AnyObject, Sel, &AnyObject) -> usize>(original)
        };
        unsafe { f(this, cmd, drag_info) }
    } else {
        NSDragOperation::Copy.0
    }
}

// --- Drag image size reading ---

unsafe fn read_drag_image_size(drag_info: &AnyObject) -> (f64, f64) {
    let size = unsafe { enumerate_dragging_frames(drag_info) };
    if size.0 > 0.0 || size.1 > 0.0 {
        return size;
    }

    // Fallback: try the deprecated draggedImage() — works for same-process drags
    let image: *const AnyObject = unsafe { msg_send![drag_info, draggedImage] };
    if !image.is_null() {
        let ns_size: objc2_foundation::NSSize = unsafe { msg_send![image, size] };
        if ns_size.width > 0.0 || ns_size.height > 0.0 {
            return (ns_size.width, ns_size.height);
        }
    }

    (0.0, 0.0)
}

unsafe fn enumerate_dragging_frames(drag_info: &AnyObject) -> (f64, f64) {
    // NSURL matches file drags (Finder, etc). Constructed via ObjC msg_send because
    // AnyClass (Class in ObjC) is a valid object but doesn't fit objc2's typed NSArray.
    let Some(nsurl_cls) = AnyClass::get(c"NSURL") else {
        return (0.0, 0.0);
    };
    let nsarray_cls = AnyClass::get(c"NSArray").expect("NSArray class must exist");
    let class_array: *const AnyObject =
        unsafe { msg_send![nsarray_cls, arrayWithObject: nsurl_cls as *const AnyClass] };
    if class_array.is_null() {
        return (0.0, 0.0);
    }

    let empty_dict_owned = NSDictionary::new();
    let empty_dict: &NSDictionary = empty_dict_owned.as_ref();

    let min_x = std::cell::Cell::new(f64::MAX);
    let min_y = std::cell::Cell::new(f64::MAX);
    let max_x = std::cell::Cell::new(f64::MIN);
    let max_y = std::cell::Cell::new(f64::MIN);
    let found = std::cell::Cell::new(false);

    let block = block2::RcBlock::new(|item: NonNull<NSDraggingItem>, _idx: NSInteger, _stop: NonNull<Bool>| {
        let frame: NSRect = unsafe { item.as_ref() }.draggingFrame();

        found.set(true);
        let x = frame.origin.x;
        let y = frame.origin.y;
        let w = frame.size.width;
        let h = frame.size.height;

        if x < min_x.get() {
            min_x.set(x);
        }
        if y < min_y.get() {
            min_y.set(y);
        }
        if x + w > max_x.get() {
            max_x.set(x + w);
        }
        if y + h > max_y.get() {
            max_y.set(y + h);
        }
    });

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

    if found.get() {
        let width = max_x.get() - min_x.get();
        let height = max_y.get() - min_y.get();
        (width.max(0.0), height.max(0.0))
    } else {
        (0.0, 0.0)
    }
}
