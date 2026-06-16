//! macOS accent color reader.
//!
//! Reads the user's system accent color via `NSColor.controlAccentColor()` and
//! observes `NSSystemColorsDidChangeNotification` to emit live updates.
//!
//! ## Edge cases
//!
//! - **Multicolor** (default macOS option): `controlAccentColor` returns blue.
//! - **Graphite**: Returns a desaturated gray (~`#8c8c8c`). The CSS `color-mix()` derivations still
//!   work but produce muted hover/subtle variants. This matches the user's intent for a neutral
//!   interface.
//! - **Light/dark mode switching**: `NSSystemColorsDidChangeNotification` fires on appearance
//!   changes too, so we re-read and emit the mode-appropriate color. WKWebView separately handles
//!   `prefers-color-scheme` media queries, so our observer only needs to update the accent color.

use std::ptr::NonNull;

use log::{debug, info, warn};
use objc2::MainThreadMarker;
use objc2_app_kit::{NSColor, NSColorSpace, NSSystemColorsDidChangeNotification};
use objc2_foundation::{NSNotification, NSNotificationCenter};
use tauri::{AppHandle, Runtime};
use tauri_specta::Event as _;

use crate::system_events::AccentColorChanged;

/// Brand fallback accent (mustard gold from getcmdr.com).
/// Only used if NSColor.controlAccentColor() cannot be read.
const FALLBACK_ACCENT_HEX: &str = "#d4a006";

/// Reads the current system accent color and returns it as a hex string (for example, `#007aff`).
/// Falls back to macOS default blue if the color cannot be read.
///
/// `NSColor` is AppKit-main-thread-only. The `mtm: MainThreadMarker` is the
/// compile-time proof we're on the main thread; the objc2 0.3 `NSColor`
/// class methods don't take it as an argument, so its presence alone is what
/// makes an off-main call fail to compile.
fn read_accent_color(_mtm: MainThreadMarker) -> String {
    let accent = NSColor::controlAccentColor();

    // Convert to sRGB color space so we get predictable RGB components.
    let srgb = NSColorSpace::sRGBColorSpace();
    let Some(converted) = accent.colorUsingColorSpace(&srgb) else {
        warn!("Could not convert accent color to sRGB, using fallback");
        return FALLBACK_ACCENT_HEX.to_owned();
    };

    let r = converted.redComponent();
    let g = converted.greenComponent();
    let b = converted.blueComponent();

    // Clamp to [0, 1] and convert to 8-bit hex
    let r8 = (r.clamp(0.0, 1.0) * 255.0).round() as u8;
    let g8 = (g.clamp(0.0, 1.0) * 255.0).round() as u8;
    let b8 = (b.clamp(0.0, 1.0) * 255.0).round() as u8;

    format!("#{r8:02x}{g8:02x}{b8:02x}")
}

/// Tauri command: returns the current macOS accent color as a hex string.
///
/// `NSColor` is main-thread-only, so we hop to the AppKit main thread via
/// `run_on_main_thread` and read there, mirroring the clipboard commands.
/// Returns the brand fallback if the main-thread hop or channel fails.
#[tauri::command]
#[specta::specta]
pub fn get_accent_color(app: AppHandle) -> String {
    let (tx, rx) = std::sync::mpsc::channel();
    if app
        .run_on_main_thread(move || {
            let mtm = MainThreadMarker::new().expect("run_on_main_thread runs on the main thread");
            let _ = tx.send(read_accent_color(mtm));
        })
        .is_err()
    {
        warn!("Couldn't hop to the main thread to read accent color, using fallback");
        return FALLBACK_ACCENT_HEX.to_owned();
    }

    rx.recv().unwrap_or_else(|e| {
        warn!("Couldn't receive accent color from the main thread ({e}), using fallback");
        FALLBACK_ACCENT_HEX.to_owned()
    })
}

/// Starts observing `NSSystemColorsDidChangeNotification`.
/// Emits `accent-color-changed` with the new hex value whenever the user
/// changes their accent color in System Settings or switches light/dark mode.
pub fn observe_accent_color_changes<R: Runtime>(app_handle: AppHandle<R>) {
    // This runs at startup on the main thread (called from the Tauri setup hook).
    let mtm = MainThreadMarker::new().expect("observe_accent_color_changes runs on the main thread");
    let initial = read_accent_color(mtm);
    debug!("System accent color: {initial}");

    let center = NSNotificationCenter::defaultCenter();

    // Use block-based observer so we don't need a selector or ObjC class.
    // The block captures the app handle to emit Tauri events.
    let block = block2::RcBlock::new(move |_notification: NonNull<NSNotification>| {
        // NSNotificationCenter delivers this on the thread that posted the
        // notification; system-color changes post on the main thread.
        let mtm = MainThreadMarker::new().expect("NSSystemColorsDidChange is delivered on the main thread");
        let hex = read_accent_color(mtm);
        info!("Accent color changed: {hex}");
        if let Err(e) = (AccentColorChanged { hex }).emit(&app_handle) {
            warn!("Failed to emit accent-color-changed event: {e}");
        }
    });

    // SAFETY: NSSystemColorsDidChangeNotification is a valid notification name constant.
    // The observer is retained by NSNotificationCenter for the lifetime of the app.
    // We intentionally never remove it because we want updates for the entire session.
    unsafe {
        center.addObserverForName_object_queue_usingBlock(
            Some(NSSystemColorsDidChangeNotification),
            None,
            None,
            &block,
        );
    }
}
