//! macOS system text-size reader.
//!
//! Reads the user's system-wide accessibility text size and emits live updates.
//!
//! ## Source of truth
//!
//! macOS persists the global accessibility text size as a UIKit-style Dynamic
//! Type category in `NSGlobalDomain` under the key
//! `UIPreferredContentSizeCategoryName` (e.g. `UICTContentSizeCategoryL` for the
//! default "Large"). The same key is read by Apple's first-party apps and is the
//! only signal we can pick up — there is no public AppKit API for this.
//!
//! ## Risks (knowingly accepted)
//!
//! - **`UIPreferredContentSizeCategoryName` is undocumented.** If Apple renames
//!   or removes it, `read_system_multiplier()` returns `1.0` (the "Large" default)
//!   and the user's in-app slider continues to work. No crash, just no system
//!   integration.
//! - **`com.apple.accessibility.api` is an undocumented distributed
//!   notification.** It's the same bus Apple's own components react to for
//!   "something in Accessibility changed". Same fallback story — if it stops
//!   firing, the value still reads correctly on next app launch.
//! - **Per-app overrides** (`com.apple.universalaccess` → `FontSizeCategory`)
//!   are intentionally ignored. Cmdr ships its own per-app slider in Settings;
//!   layering the system per-app override on top adds complexity for a feature
//!   users won't reach for.
//!
//! ## Mapping
//!
//! Multipliers match the standard UIKit Dynamic Type scale factors. Anything we
//! don't recognize falls back to `1.0`.
//!
//! ## Single source of truth
//!
//! The frontend (`settings-applier.ts`) compounds this multiplier with the
//! user's `appearance.textSize` slider in exactly one place. The backend never
//! computes the final scale — it only reports the system value.

use std::ptr::NonNull;

use log::{debug, info, warn};
use objc2_foundation::{NSDistributedNotificationCenter, NSNotification, NSString, NSUserDefaults};
use tauri::{AppHandle, Emitter, Runtime};

/// `NSGlobalDomain` key macOS writes when the user moves the
/// Accessibility > Display > Text Size slider.
const TEXT_SIZE_PREF_KEY: &str = "UIPreferredContentSizeCategoryName";

/// Distributed notification posted when any Accessibility setting changes.
const ACCESSIBILITY_CHANGED_NOTIFICATION: &str = "com.apple.accessibility.api";

/// Maps a UIKit `UIContentSizeCategory` raw name to its standard scale factor.
/// `Large` is the default (1.0). Unknown values fall back to 1.0.
fn category_to_multiplier(name: &str) -> f32 {
    match name {
        "UICTContentSizeCategoryXS" => 0.82,
        "UICTContentSizeCategoryS" => 0.88,
        "UICTContentSizeCategoryM" => 0.94,
        "UICTContentSizeCategoryL" => 1.0,
        "UICTContentSizeCategoryXL" => 1.12,
        "UICTContentSizeCategoryXXL" => 1.24,
        "UICTContentSizeCategoryXXXL" => 1.35,
        "UICTContentSizeCategoryAccessibilityM" => 1.64,
        "UICTContentSizeCategoryAccessibilityL" => 1.94,
        "UICTContentSizeCategoryAccessibilityXL" => 2.35,
        "UICTContentSizeCategoryAccessibilityXXL" => 2.76,
        "UICTContentSizeCategoryAccessibilityXXXL" => 3.12,
        _ => 1.0,
    }
}

/// Reads the current system text-size multiplier. Returns 1.0 when the key is
/// missing or unrecognized.
fn read_system_multiplier() -> f32 {
    let defaults = NSUserDefaults::standardUserDefaults();
    let key = NSString::from_str(TEXT_SIZE_PREF_KEY);
    let Some(value) = defaults.stringForKey(&key) else {
        return 1.0;
    };
    let raw = value.to_string();
    let multiplier = category_to_multiplier(&raw);
    debug!("System text size: {raw} -> {multiplier:.2}x");
    multiplier
}

/// Tauri command: returns the current system text-size multiplier.
#[tauri::command]
pub fn get_system_text_size_multiplier() -> f32 {
    read_system_multiplier()
}

/// Subscribes to the distributed accessibility-changed notification and emits
/// `system-text-size-changed` with the new multiplier whenever it fires.
///
/// The observer is intentionally never removed — we want updates for the entire
/// session.
pub fn observe_system_text_size_changes<R: Runtime>(app_handle: AppHandle<R>) {
    let initial = read_system_multiplier();
    debug!("Initial system text size multiplier: {initial:.2}x");

    let center = NSDistributedNotificationCenter::defaultCenter();
    let name = NSString::from_str(ACCESSIBILITY_CHANGED_NOTIFICATION);

    let block = block2::RcBlock::new(move |_notification: NonNull<NSNotification>| {
        let multiplier = read_system_multiplier();
        info!("System text size changed: {multiplier:.2}x");
        if let Err(e) = app_handle.emit("system-text-size-changed", multiplier) {
            warn!("Failed to emit system-text-size-changed event: {e}");
        }
    });

    // Safety: name is a valid NSString. The center retains the observer for the
    // app lifetime; we never deregister.
    unsafe {
        center.addObserverForName_object_queue_usingBlock(Some(&name), None, None, &block);
    }
}
