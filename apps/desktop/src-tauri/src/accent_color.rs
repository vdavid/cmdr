//! macOS accent color reader.
//!
//! Reads the user's system accent color via `NSColor.controlAccentColor()` and
//! observes `NSSystemColorsDidChangeNotification` to emit live updates.
//!
//! ## Edge cases
//!
//! - **Multicolor** (default macOS option): `controlAccentColor` returns blue.
//! - **Graphite**: Returns a desaturated gray (~`#8c8c8c`). The CSS `color-mix()`
//!   derivations still work but produce muted hover/subtle variants. This matches
//!   the user's intent for a neutral interface.
//! - **Light/dark mode switching**: `NSSystemColorsDidChangeNotification` fires on
//!   appearance changes too, so we re-read and emit the mode-appropriate color.
//!   WKWebView separately handles `prefers-color-scheme` media queries — our
//!   observer only needs to update the accent color.

use std::ptr::NonNull;

use log::{debug, info, warn};
use objc2_app_kit::{NSColor, NSColorSpace, NSSystemColorsDidChangeNotification};
use objc2_foundation::{NSNotification, NSNotificationCenter};
use tauri::{AppHandle, Emitter, Runtime};

/// macOS default blue accent (light mode fallback).
const FALLBACK_ACCENT_HEX: &str = "#007aff";

/// Reads the current system accent color and returns it as a hex string (for example, `#007aff`).
/// Falls back to macOS default blue if the color cannot be read.
fn read_accent_color() -> String {
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
#[tauri::command]
pub fn get_accent_color() -> String {
    read_accent_color()
}

/// Starts observing `NSSystemColorsDidChangeNotification`.
/// Emits `accent-color-changed` with the new hex value whenever the user
/// changes their accent color in System Settings or switches light/dark mode.
pub fn observe_accent_color_changes<R: Runtime>(app_handle: AppHandle<R>) {
    let initial = read_accent_color();
    debug!("System accent color: {initial}");

    let center = NSNotificationCenter::defaultCenter();

    // Use block-based observer so we don't need a selector or ObjC class.
    // The block captures the app handle to emit Tauri events.
    let block = block2::RcBlock::new(move |_notification: NonNull<NSNotification>| {
        let hex = read_accent_color();
        info!("Accent color changed: {hex}");
        if let Err(e) = app_handle.emit("accent-color-changed", &hex) {
            warn!("Failed to emit accent-color-changed event: {e}");
        }
    });

    // Safety: NSSystemColorsDidChangeNotification is a valid notification name constant.
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
