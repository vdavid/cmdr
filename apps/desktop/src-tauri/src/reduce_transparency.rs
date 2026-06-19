//! macOS "reduce transparency" reader.
//!
//! Reads the user's Accessibility > Display > Reduce transparency setting via
//! `NSWorkspace.accessibilityDisplayShouldReduceTransparency()` and observes
//! `NSWorkspaceAccessibilityDisplayOptionsDidChangeNotification` to emit live
//! updates.
//!
//! ## Notes
//!
//! - The accessibility-options notification posts on the `NSWorkspace`
//!   notification center (NOT the default center), so we subscribe there.
//! - It fires for the whole accessibility-display options group (reduce
//!   transparency, increase contrast, reduce motion, etc.), so we always
//!   re-read the current value rather than trusting the notification payload.

use std::ptr::NonNull;

use log::{debug, info, warn};
use objc2::MainThreadMarker;
use objc2_app_kit::{NSWorkspace, NSWorkspaceAccessibilityDisplayOptionsDidChangeNotification};
use objc2_foundation::NSNotification;
use tauri::{AppHandle, Runtime};
use tauri_specta::Event as _;

use crate::system_events::ReduceTransparencyChanged;

/// Reads whether the user has enabled "reduce transparency". Returns `false`
/// (the safe "don't reduce" default) if it can't be read.
///
/// `NSWorkspace` accessibility-display queries are AppKit-main-thread-only. The
/// `mtm: MainThreadMarker` is the compile-time proof we're on the main thread;
/// the objc2 0.3 method doesn't take it as an argument, so its presence alone
/// is what makes an off-main call fail to compile.
fn read_should_reduce_transparency(_mtm: MainThreadMarker) -> bool {
    NSWorkspace::sharedWorkspace().accessibilityDisplayShouldReduceTransparency()
}

/// Tauri command: returns whether macOS "reduce transparency" is enabled.
///
/// `NSWorkspace` accessibility queries are main-thread-only, so we hop to the
/// AppKit main thread via `run_on_main_thread` and read there, mirroring
/// `get_accent_color`. Returns `false` (don't reduce) if the main-thread hop or
/// channel fails.
#[tauri::command]
#[specta::specta]
pub fn get_should_reduce_transparency(app: AppHandle) -> bool {
    let (tx, rx) = std::sync::mpsc::channel();
    if app
        .run_on_main_thread(move || {
            let mtm = MainThreadMarker::new().expect("run_on_main_thread runs on the main thread");
            let _ = tx.send(read_should_reduce_transparency(mtm));
        })
        .is_err()
    {
        warn!("Couldn't hop to the main thread to read reduce-transparency, defaulting to false");
        return false;
    }

    rx.recv().unwrap_or_else(|e| {
        warn!("Couldn't receive reduce-transparency from the main thread ({e}), defaulting to false");
        false
    })
}

/// Starts observing `NSWorkspaceAccessibilityDisplayOptionsDidChangeNotification`.
/// Emits `reduce-transparency-changed` with the new value whenever the user
/// changes their Accessibility > Display options in System Settings.
pub fn observe_reduce_transparency_changes<R: Runtime>(app_handle: AppHandle<R>) {
    // This runs at startup on the main thread (called from the Tauri setup hook).
    let mtm = MainThreadMarker::new().expect("observe_reduce_transparency_changes runs on the main thread");
    let initial = read_should_reduce_transparency(mtm);
    debug!("Reduce transparency: {initial}");

    // Accessibility-display options post on the NSWorkspace notification center,
    // not the default center.
    let center = NSWorkspace::sharedWorkspace().notificationCenter();

    // Use block-based observer so we don't need a selector or ObjC class.
    // The block captures the app handle to emit Tauri events.
    let block = block2::RcBlock::new(move |_notification: NonNull<NSNotification>| {
        // NSNotificationCenter delivers this on the thread that posted the
        // notification; accessibility-options changes post on the main thread.
        let mtm = MainThreadMarker::new()
            .expect("NSWorkspaceAccessibilityDisplayOptionsDidChange is delivered on the main thread");
        let reduce = read_should_reduce_transparency(mtm);
        info!("Reduce transparency changed: {reduce}");
        if let Err(e) = (ReduceTransparencyChanged { reduce }).emit(&app_handle) {
            warn!("Failed to emit reduce-transparency-changed event: {e}");
        }
    });

    // SAFETY: NSWorkspaceAccessibilityDisplayOptionsDidChangeNotification is a valid
    // notification name constant, and `center` is the live `NSWorkspace` notification
    // center. `block` is a live `RcBlock` with the expected
    // `(NonNull<NSNotification>) -> ()` signature. The observer is retained by the
    // center for the lifetime of the app; we intentionally never remove it because we
    // want updates for the entire session.
    unsafe {
        center.addObserverForName_object_queue_usingBlock(
            Some(NSWorkspaceAccessibilityDisplayOptionsDidChangeNotification),
            None,
            None,
            &block,
        );
    }
}
