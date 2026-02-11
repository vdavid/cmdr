//! Native drag interception for macOS via method swizzling on WryWebView.
//!
//! Swizzles `draggingEntered:`, `draggingUpdated:`, and `draggingExited:` to:
//! 1. Read drag image dimensions via `enumerateDraggingItems` (for overlay suppression)
//! 2. Read modifier key state via `[NSEvent modifierFlags]` (for copy/move detection)
//! 3. Swap the OS drag image for self-drags (delegated to `drag_image_swap`)
//!
//! Events emitted:
//! - `drag-image-size` `{ width, height }` — on drag enter
//! - `drag-modifiers` `{ altHeld }` — on drag enter and every drag update (only when changed)
//!
//! ## Resilience
//!
//! All native API calls are guarded against class/method removal. If wry renames its
//! internal webview class or macOS deprecates APIs we rely on, the swizzle degrades gracefully:
//! - Drag image detection disabled → the DOM overlay is always shown (redundant but functional)
//! - Modifier key detection disabled → falls back to JS keydown/keyup (works when webview has focus)
//! - Image swapping disabled → self-drags show the OS drag image over the window (functional)
//!
//! Rust panics inside swizzled functions are caught via `catch_unwind` to prevent crashes
//! across the FFI boundary. Warning messages include actionable guidance for future maintainers.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::ptr::NonNull;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

use objc2::runtime::{AnyClass, AnyObject, Bool, Imp, Sel};
use objc2::{msg_send, sel};
use objc2_app_kit::{NSDragOperation, NSDraggingItem, NSDraggingItemEnumerationOptions};
use objc2_foundation::{NSDictionary, NSInteger, NSRect, NSSize};
use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::drag_image_swap;

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
static ORIGINAL_EXITED_IMP: OnceLock<Imp> = OnceLock::new();
static APP_HANDLE: OnceLock<AppHandle> = OnceLock::new();

/// Tracks previous alt state so we only emit `drag-modifiers` when it changes.
static LAST_ALT_HELD: AtomicBool = AtomicBool::new(false);

// Warn-once flags to prevent log spam for issues that recur on every drag event.
static WARNED_NSEVENT_MISSING: AtomicBool = AtomicBool::new(false);
static WARNED_ENTERED_PANIC: AtomicBool = AtomicBool::new(false);
static WARNED_UPDATED_PANIC: AtomicBool = AtomicBool::new(false);
static WARNED_EXITED_PANIC: AtomicBool = AtomicBool::new(false);
static WARNED_NSARRAY_MISSING: AtomicBool = AtomicBool::new(false);
static WARNED_DRAGGED_IMAGE_REMOVED: AtomicBool = AtomicBool::new(false);

/// Logs a warning message at most once per app session.
pub(crate) fn warn_once(flag: &AtomicBool, msg: &str) {
    if !flag.swap(true, Ordering::Relaxed) {
        log::warn!("{msg}");
    }
}

/// Installs swizzles on WryWebView. Call once during app setup.
pub fn install(app_handle: AppHandle) {
    APP_HANDLE.set(app_handle).ok();

    unsafe {
        let Some(cls) = AnyClass::get(c"WryWebView") else {
            log::warn!(
                "drag_image_detection: WryWebView class not found — swizzle skipped. \
                 Drag image detection and modifier tracking during drags are disabled. \
                 This is likely caused by a wry update that renamed the webview class; \
                 search wry's source for the ObjC class name and update the c\"WryWebView\" \
                 lookup in drag_image_detection.rs."
            );
            return;
        };

        // Swizzle draggingEntered:
        if let Some(method) = cls.instance_method(sel!(draggingEntered:)) {
            ORIGINAL_ENTERED_IMP.set(method.implementation()).ok();
            method.set_implementation(std::mem::transmute::<*const (), Imp>(
                swizzled_dragging_entered as *const (),
            ));
        } else {
            log::warn!(
                "drag_image_detection: draggingEntered: not found on WryWebView — \
                 drag image size detection is disabled. \
                 Wry may have changed how it implements NSDraggingDestination; \
                 check wry's drag-and-drop event handling in its ObjC layer."
            );
        }

        // Swizzle draggingUpdated:
        if let Some(method) = cls.instance_method(sel!(draggingUpdated:)) {
            ORIGINAL_UPDATED_IMP.set(method.implementation()).ok();
            method.set_implementation(std::mem::transmute::<*const (), Imp>(
                swizzled_dragging_updated as *const (),
            ));
        } else {
            log::warn!(
                "drag_image_detection: draggingUpdated: not found on WryWebView — \
                 live modifier key tracking during drags is disabled. \
                 Wry may have changed how it implements NSDraggingDestination; \
                 check wry's drag-and-drop event handling in its ObjC layer."
            );
        }

        // Swizzle draggingExited: for self-drag image swapping (transparent → rich on window exit)
        if let Some(method) = cls.instance_method(sel!(draggingExited:)) {
            ORIGINAL_EXITED_IMP.set(method.implementation()).ok();
            method.set_implementation(std::mem::transmute::<*const (), Imp>(
                swizzled_dragging_exited as *const (),
            ));
        } else {
            log::warn!(
                "drag_image_detection: draggingExited: not found on WryWebView — \
                 drag image swapping on window exit is disabled. \
                 Wry may have changed how it implements NSDraggingDestination; \
                 check wry's drag-and-drop event handling in its ObjC layer."
            );
        }

        log::info!("drag_image_detection: swizzles installed on WryWebView");
    }
}

// --- Modifier key detection ---

/// Reads the current Option/Alt key state from `[NSEvent modifierFlags]`.
/// This is a class method that reads hardware state — works even when the webview isn't focused.
/// Returns `false` if NSEvent can't be found (graceful degradation).
fn is_option_held() -> bool {
    let Some(cls) = AnyClass::get(c"NSEvent") else {
        warn_once(
            &WARNED_NSEVENT_MISSING,
            "drag_image_detection: NSEvent class not found — Alt/Option detection during drags \
             is disabled. This is a core AppKit class and shouldn't disappear; if it did, check \
             whether macOS moved it to a different framework or renamed it. Modifier detection \
             falls back to JS keydown/keyup, which doesn't work during OS-level drags.",
        );
        return false;
    };
    let flags: usize = unsafe { msg_send![cls, modifierFlags] };
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

// --- Helpers for forwarding to original implementations ---

/// Forwards to wry's original `draggingEntered:`. Always safe to call — returns
/// `NSDragOperation::Copy` if the original wasn't saved (shouldn't happen in practice).
unsafe fn call_original_entered(this: &AnyObject, cmd: Sel, drag_info: &AnyObject) -> usize {
    unsafe {
        if let Some(&original) = ORIGINAL_ENTERED_IMP.get() {
            let f =
                std::mem::transmute::<Imp, unsafe extern "C-unwind" fn(&AnyObject, Sel, &AnyObject) -> usize>(original);
            f(this, cmd, drag_info)
        } else {
            NSDragOperation::Copy.0
        }
    }
}

/// Forwards to wry's original `draggingUpdated:`. Same fallback as above.
unsafe fn call_original_updated(this: &AnyObject, cmd: Sel, drag_info: &AnyObject) -> usize {
    unsafe {
        if let Some(&original) = ORIGINAL_UPDATED_IMP.get() {
            let f =
                std::mem::transmute::<Imp, unsafe extern "C-unwind" fn(&AnyObject, Sel, &AnyObject) -> usize>(original);
            f(this, cmd, drag_info)
        } else {
            NSDragOperation::Copy.0
        }
    }
}

/// Forwards to wry's original `draggingExited:`. Returns void.
/// If the original wasn't saved, this is a no-op (drag exit still works, wry just won't fire its handler).
unsafe fn call_original_exited(this: &AnyObject, cmd: Sel, drag_info: &AnyObject) {
    unsafe {
        if let Some(&original) = ORIGINAL_EXITED_IMP.get() {
            let f = std::mem::transmute::<Imp, unsafe extern "C-unwind" fn(&AnyObject, Sel, &AnyObject)>(original);
            f(this, cmd, drag_info)
        }
    }
}

// --- draggingEntered: swizzle ---

unsafe extern "C-unwind" fn swizzled_dragging_entered(this: &AnyObject, cmd: Sel, drag_info: &AnyObject) -> usize {
    // Our custom logic: read drag image size, emit modifier events, and swap image for self-drags.
    // Wrapped in catch_unwind to prevent any unexpected Rust panic from crossing the FFI boundary
    // and crashing the app mid-drag.
    let result = catch_unwind(AssertUnwindSafe(|| {
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

        // For self-drags, swap the OS drag image to transparent so it's invisible inside the window.
        // The rich PNG (set at drag start) remains as the session image shown outside the window.
        // The DOM overlay handles the visual feedback inside.
        unsafe { drag_image_swap::on_drag_entered(drag_info) };
    }));

    if result.is_err() {
        warn_once(
            &WARNED_ENTERED_PANIC,
            "drag_image_detection: panic in draggingEntered swizzle — drag image detection \
             and initial modifier state may not work for this session. \
             This is likely caused by a wry or macOS API change; check that NSDraggingItem's \
             draggingFrame() and NSEvent's modifierFlags still match the expected signatures \
             in drag_image_detection.rs.",
        );
    }

    // Always forward to wry's original implementation, even if our logic failed.
    unsafe { call_original_entered(this, cmd, drag_info) }
}

// --- draggingUpdated: swizzle ---

unsafe extern "C-unwind" fn swizzled_dragging_updated(this: &AnyObject, cmd: Sel, drag_info: &AnyObject) -> usize {
    // Only emit when modifier state changes (avoids flooding on every mouse move)
    let result = catch_unwind(AssertUnwindSafe(|| {
        emit_modifiers_if_changed();
    }));

    if result.is_err() {
        warn_once(
            &WARNED_UPDATED_PANIC,
            "drag_image_detection: panic in draggingUpdated swizzle — live modifier key \
             tracking during drags is disabled for this session. \
             Check NSEvent.modifierFlags usage in drag_image_detection.rs.",
        );
    }

    // Always forward to wry's original implementation.
    unsafe { call_original_updated(this, cmd, drag_info) }
}

// --- draggingExited: swizzle ---

unsafe extern "C-unwind" fn swizzled_dragging_exited(this: &AnyObject, cmd: Sel, drag_info: &AnyObject) {
    // Swap back to the rich image so it's visible outside the window.
    // setDraggingFrame:contents: modifications persist globally, so the transparent image
    // from draggingEntered: would remain visible outside without this swap-back.
    let result = catch_unwind(AssertUnwindSafe(|| {
        unsafe { drag_image_swap::on_drag_exited(drag_info) };
    }));

    if result.is_err() {
        warn_once(
            &WARNED_EXITED_PANIC,
            "drag_image_detection: panic in draggingExited swizzle — drag image swap-back \
             to rich preview is disabled for this session. \
             Check NSImage and NSDraggingItem usage in drag_image_detection.rs.",
        );
    }

    // Always forward to wry's original implementation.
    unsafe { call_original_exited(this, cmd, drag_info) }
}

// --- Drag image size reading ---

unsafe fn read_drag_image_size(drag_info: &AnyObject) -> (f64, f64) {
    let size = unsafe { enumerate_dragging_frames(drag_info) };
    if size.0 > 0.0 || size.1 > 0.0 {
        return size;
    }

    // Fallback: try the deprecated `draggedImage()` — works for same-process drags.
    // Guard with respondsToSelector: since Apple may remove this deprecated API entirely.
    let responds: Bool = unsafe { msg_send![drag_info, respondsToSelector: sel!(draggedImage)] };
    if !responds.as_bool() {
        warn_once(
            &WARNED_DRAGGED_IMAGE_REMOVED,
            "drag_image_detection: draggedImage selector no longer exists on NSDraggingInfo — \
             Apple removed this deprecated API in this macOS version. \
             The primary path (enumerateDraggingItems) still works; this only affects \
             same-process drag size detection as a fallback. \
             Remove the draggedImage fallback from read_drag_image_size() in \
             drag_image_detection.rs.",
        );
        return (0.0, 0.0);
    }

    let image: *const AnyObject = unsafe { msg_send![drag_info, draggedImage] };
    if !image.is_null() {
        let ns_size: NSSize = unsafe { msg_send![image, size] };
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
    let Some(nsarray_cls) = AnyClass::get(c"NSArray") else {
        warn_once(
            &WARNED_NSARRAY_MISSING,
            "drag_image_detection: NSArray class not found — drag frame enumeration is disabled. \
             NSArray is a core Foundation class; if it's missing, something is fundamentally \
             wrong with the ObjC runtime. Check if Foundation is loaded correctly.",
        );
        return (0.0, 0.0);
    };
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
