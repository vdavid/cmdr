//! The AppKit half of a child window's slide: fade in while moving down `SLIDE_DISTANCE_PT`, and
//! the reverse on close, both through the window's `animator` proxy. Every function here MUST run
//! on the AppKit main thread and checks a `MainThreadMarker` to enforce it.

use objc2::runtime::{AnyObject, Bool};
use objc2::{MainThreadMarker, class, msg_send};
use objc2_app_kit::{NSWindowAnimationBehavior, NSWorkspace};
use objc2_core_foundation::CGRect;
use tauri::WebviewWindow;

use super::ledger::Frame;
use super::{SLIDE_DISTANCE_PT, SLIDE_DURATION};

/// How `show` brought a window up.
pub(super) enum Shown {
    /// It slid in.
    Slid,
    /// It appeared without a slide (Reduce motion is on).
    Plain,
    /// It was already on screen, or minimized, so it was left alone.
    AlreadyShowing,
}

/// The window's live, non-null `NSWindow`, with proof we're on the main thread.
fn ns_window(window: &WebviewWindow) -> Result<(*mut AnyObject, MainThreadMarker), String> {
    let mtm = MainThreadMarker::new().ok_or("must run on the AppKit main thread")?;
    let ptr = window.ns_window().map_err(|e| format!("ns_window() failed: {e}"))? as *mut AnyObject;
    if ptr.is_null() {
        return Err("NSWindow pointer is null".into());
    }
    Ok((ptr, mtm))
}

/// macOS Accessibility > Display > Reduce motion. Read on every show and close, so a change in
/// System Settings applies to the next window without a restart.
fn reduce_motion(_mtm: MainThreadMarker) -> bool {
    NSWorkspace::sharedWorkspace().accessibilityDisplayShouldReduceMotion()
}

impl From<CGRect> for Frame {
    fn from(r: CGRect) -> Self {
        Frame {
            x: r.origin.x,
            y: r.origin.y,
            width: r.size.width,
            height: r.size.height,
        }
    }
}

impl From<Frame> for CGRect {
    fn from(f: Frame) -> Self {
        CGRect::new(
            objc2_core_foundation::CGPoint::new(f.x, f.y),
            objc2_core_foundation::CGSize::new(f.width, f.height),
        )
    }
}

/// The slide's off-target end: `SLIDE_DISTANCE_PT` above `frame`. AppKit's y axis points up, so a
/// higher origin is further up the screen.
fn raised(frame: CGRect) -> CGRect {
    let mut up = frame;
    up.origin.y += SLIDE_DISTANCE_PT;
    up
}

/// Animates the window's alpha and frame together over `SLIDE_DURATION`.
///
/// # Safety
/// `ns_window` must be a live `NSWindow`, and the caller must be on the AppKit main thread.
unsafe fn animate(ns_window: *mut AnyObject, alpha: f64, frame: CGRect) {
    // SAFETY: the caller guarantees a live `NSWindow` and the main thread. `+currentContext` and
    // `-animator` return autoreleased objects we only message within this scope, never retain.
    // `-setDuration:` takes an `NSTimeInterval` (f64), `-setAlphaValue:` a `CGFloat` (f64), and
    // `-setFrame:display:` an `NSRect` (`CGRect` on 64-bit) plus a `BOOL`; all return void.
    unsafe {
        let _: () = msg_send![class!(NSAnimationContext), beginGrouping];
        let context: *mut AnyObject = msg_send![class!(NSAnimationContext), currentContext];
        let _: () = msg_send![context, setDuration: SLIDE_DURATION.as_secs_f64()];
        let animator: *mut AnyObject = msg_send![ns_window, animator];
        let _: () = msg_send![animator, setAlphaValue: alpha];
        let _: () = msg_send![animator, setFrame: frame, display: true];
        let _: () = msg_send![class!(NSAnimationContext), endGrouping];
    }
}

/// Orders a hidden window front, sliding it in unless Reduce motion is on.
///
/// We drive the slide ourselves, so AppKit's own open animation (`animationBehavior`) is switched
/// off first: left on, it would play on top of ours. It has to be set before the window is first
/// ordered front, which is why the opener creates the window hidden.
pub(super) fn show(window: &WebviewWindow) -> Result<Shown, String> {
    let (ns_window, mtm) = ns_window(window)?;
    // SAFETY: `ns_window` is the live, non-null `NSWindow` Tauri owns for this webview and we're on
    // the main thread (both checked by `ns_window()` above), which every message here requires.
    // `-isVisible` / `-isMiniaturized` return a `BOOL`, `-frame` an `NSRect` (`CGRect` on 64-bit);
    // the setters take an `NSWindowAnimationBehavior` (an `NSInteger` enum), a `CGFloat`, and an
    // `NSRect` plus `BOOL`; the action takes an `id` sender (nil here). No ownership to manage.
    unsafe {
        let visible: Bool = msg_send![ns_window, isVisible];
        let minimized: Bool = msg_send![ns_window, isMiniaturized];
        if visible.as_bool() || minimized.as_bool() {
            return Ok(Shown::AlreadyShowing);
        }
        let _: () = msg_send![ns_window, setAnimationBehavior: NSWindowAnimationBehavior::None];
        if reduce_motion(mtm) {
            let _: () = msg_send![ns_window, makeKeyAndOrderFront: std::ptr::null_mut::<AnyObject>()];
            return Ok(Shown::Plain);
        }
        let target: CGRect = msg_send![ns_window, frame];
        let _: () = msg_send![ns_window, setAlphaValue: 0.0_f64];
        let _: () = msg_send![ns_window, setFrame: raised(target), display: false];
        let _: () = msg_send![ns_window, makeKeyAndOrderFront: std::ptr::null_mut::<AnyObject>()];
        animate(ns_window, 1.0, target);
    }
    Ok(Shown::Slid)
}

/// Starts a slide-out (fade while moving back up) and returns the frame the window left from, or
/// `None` when it shouldn't slide: Reduce motion is on, or the window isn't on screen (minimized or
/// already hidden), where a slide would play to nobody and only delay the close.
pub(super) fn start_slide_out(window: &WebviewWindow) -> Result<Option<Frame>, String> {
    let (ns_window, mtm) = ns_window(window)?;
    if reduce_motion(mtm) {
        return Ok(None);
    }
    // SAFETY: `ns_window` is the live, non-null `NSWindow` Tauri owns for this webview and we're on
    // the main thread (both checked by `ns_window()` above), which `-isVisible` /
    // `-isMiniaturized` (return `BOOL`), `-frame` (returns an `NSRect`, `CGRect` on 64-bit), and
    // `animate` require.
    unsafe {
        let visible: Bool = msg_send![ns_window, isVisible];
        let minimized: Bool = msg_send![ns_window, isMiniaturized];
        if !visible.as_bool() || minimized.as_bool() {
            return Ok(None);
        }
        let rest: CGRect = msg_send![ns_window, frame];
        animate(ns_window, 0.0, raised(rest));
        Ok(Some(rest.into()))
    }
}

/// Slides a window whose close was cancelled back to `rest` at full opacity. A newer animation of
/// the same properties replaces a running one, so this takes over from a slide-out midway.
pub(super) fn slide_back(window: &WebviewWindow, rest: Frame) -> Result<(), String> {
    let (ns_window, _mtm) = ns_window(window)?;
    // SAFETY: `ns_window` is the live, non-null `NSWindow` Tauri owns for this webview and we're on
    // the main thread (both checked by `ns_window()` above), as `animate` requires.
    unsafe { animate(ns_window, 1.0, rest.into()) };
    Ok(())
}
