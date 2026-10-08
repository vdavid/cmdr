//! macOS 27 "Liquid Glass" slider reader (System Settings > Appearance > Liquid Glass).
//!
//! The slider writes `NSGlassTintAmount` to the global domain: `0.0` is the clearest glass,
//! `1.0` the most tinted. The frontend turns it into the `--glass-tint` CSS variable, which
//! drives the fill and blur of every frosted-glass surface (`$lib/glass-tint.ts`).
//!
//! ## Risks (knowingly accepted)
//!
//! - **`NSGlassTintAmount` is undocumented.** If Apple renames it, the read returns `None` and
//!   the frontend falls back to the middle of the slider. No crash, just no system integration.
//!   (verified on macOS 27.0: moving the slider rewrites the key live, `defaults read -g
//!   NSGlassTintAmount`, 2026-09-30). The value is stored as a string (`"0.2670789"`), so both
//!   string and number shapes are accepted.
//!
//! ## Following a change
//!
//! No notification announces a slider move, and cross-process KVO on a global-domain key lags
//! by seconds or never fires without a forced sync (measured in
//! <https://github.com/i-am-logger/macos-liquid-glass/blob/master/MEASUREMENTS.md>). The user
//! can only move the slider while System Settings is frontmost, so we re-read on
//! `NSApplicationDidBecomeActive` (on the default center) — coming back to Cmdr is the moment the value can have
//! changed — after a `CFPreferencesAppSynchronize`, without which this process keeps serving
//! its cached copy.

use std::ptr::NonNull;
use std::sync::Mutex;

use log::{debug, info, warn};
use objc2_core_foundation::{
    CFNumber, CFPreferencesAppSynchronize, CFPreferencesCopyValue, CFString, CFType, kCFPreferencesAnyApplication,
    kCFPreferencesAnyHost, kCFPreferencesCurrentUser,
};
use tauri::{AppHandle, Runtime};
use tauri_specta::Event as _;

use crate::IgnorePoison;
use crate::system_events::GlassTintChanged;

/// Global-domain key the Liquid Glass slider writes.
const GLASS_TINT_KEY: &str = "NSGlassTintAmount";

/// Turns the stored preference into a tint in `0.0..=1.0`. Accepts the string shape the
/// slider writes and a plain number (what `defaults write -g … -float` stores). Anything
/// else, or a non-finite number, reads as "unknown".
fn tint_from_preference(value: &CFType) -> Option<f32> {
    let raw = if let Some(s) = value.downcast_ref::<CFString>() {
        s.to_string().trim().parse::<f64>().ok()?
    } else {
        let n = value.downcast_ref::<CFNumber>()?;
        n.as_f64()?
    };
    if !raw.is_finite() {
        return None;
    }
    Some(raw.clamp(0.0, 1.0) as f32)
}

/// Reads the current slider value, re-syncing the global domain first so a write from
/// System Settings is seen. `None` when the key is missing or unreadable.
///
/// CFPreferences is thread-safe (`src-tauri/DETAILS.md` § "Which Apple APIs skip the
/// main-thread rule"), so this needs no `MainThreadMarker`.
fn read_glass_tint() -> Option<f32> {
    let key = CFString::from_str(GLASS_TINT_KEY);
    // SAFETY: the three CoreFoundation domain constants are immutable `'static` `CFStringRef`s
    // the framework initialises before any Rust in this process runs; reading them is `unsafe`
    // only because they cross the FFI boundary.
    let (application, user, host) = unsafe {
        (
            kCFPreferencesAnyApplication,
            kCFPreferencesCurrentUser,
            kCFPreferencesAnyHost,
        )
    };
    // Drops this process's cached copy of the global domain, so the read below sees what
    // System Settings wrote. Without it, a changed slider only shows up after a relaunch.
    if !CFPreferencesAppSynchronize(application) {
        debug!(target: "glass_tint", "Couldn't sync the global preferences domain; reading the cached value");
    }
    let value = CFPreferencesCopyValue(&key, application, user, host)?;
    tint_from_preference(&value)
}

/// Tauri command: the Liquid Glass slider value in `0.0..=1.0`, or `None` when macOS doesn't
/// report one.
#[tauri::command]
#[specta::specta]
pub async fn get_glass_tint_amount() -> Option<f32> {
    tauri::async_runtime::spawn_blocking(read_glass_tint)
        .await
        .unwrap_or_else(|e| {
            warn!(target: "glass_tint", "Reading the Liquid Glass tint panicked ({e}), reporting none");
            None
        })
}

/// The last value we reported, so re-activating the app without touching the slider emits
/// nothing.
static LAST_REPORTED: Mutex<Option<Option<f32>>> = Mutex::new(None);

/// Records `tint` and says whether it differs from the last value reported.
fn record_if_changed(tint: Option<f32>) -> bool {
    let mut last = LAST_REPORTED.lock_ignore_poison();
    if *last == Some(tint) {
        return false;
    }
    *last = Some(tint);
    true
}

/// Starts re-reading the slider whenever the app becomes active, and emits
/// `glass-tint-changed` when the value moved.
pub fn observe_glass_tint_changes<R: Runtime>(app_handle: AppHandle<R>) {
    use objc2_app_kit::NSApplicationDidBecomeActiveNotification;
    use objc2_foundation::{NSNotification, NSNotificationCenter};

    let initial = read_glass_tint();
    // allowed-discarded-outcome: seeds the baseline; the frontend reads the start value itself.
    record_if_changed(initial);
    debug!(target: "glass_tint", "Liquid Glass tint: {initial:?}");

    let block = block2::RcBlock::new(move |_notification: NonNull<NSNotification>| {
        // The CFPreferences sync is an XPC round-trip to `cfprefsd`; keep it off the main thread.
        let app = app_handle.clone();
        tauri::async_runtime::spawn_blocking(move || {
            let amount = read_glass_tint();
            if !record_if_changed(amount) {
                debug!(target: "glass_tint", "App became active; Liquid Glass tint unchanged: {amount:?}");
                return;
            }
            info!(target: "glass_tint", "Liquid Glass tint changed: {amount:?}");
            if let Err(e) = (GlassTintChanged { amount }).emit(&app) {
                warn!(target: "glass_tint", "Failed to emit glass-tint-changed event: {e}");
            }
        });
    });

    // ❗ The DEFAULT center: `NSApplication` posts its own activation there. The `NSWorkspace`
    // center never delivers it, so an observer there silently never fires.
    // SAFETY: `NSApplicationDidBecomeActiveNotification` is a valid notification name constant,
    // the default center is live for the process's lifetime, and `block` is a live `RcBlock`
    // with the expected `(NonNull<NSNotification>) -> ()` signature. The center retains the
    // observer for the app's lifetime; we never remove it because we want updates for the
    // whole session.
    unsafe {
        NSNotificationCenter::defaultCenter().addObserverForName_object_queue_usingBlock(
            Some(NSApplicationDidBecomeActiveNotification),
            None,
            None,
            &block,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn from_string(s: &str) -> Option<f32> {
        tint_from_preference(&CFString::from_str(s))
    }

    #[test]
    fn reads_the_string_the_slider_writes() {
        assert_eq!(from_string("0.2670789"), Some(0.267_078_9));
        assert_eq!(from_string("0"), Some(0.0));
        assert_eq!(from_string("1"), Some(1.0));
    }

    #[test]
    fn reads_a_number_too() {
        assert_eq!(tint_from_preference(&CFNumber::new_f64(0.6)), Some(0.6));
        assert_eq!(tint_from_preference(&CFNumber::new_i32(1)), Some(1.0));
    }

    #[test]
    fn clamps_out_of_range_values() {
        assert_eq!(from_string("-0.5"), Some(0.0));
        assert_eq!(from_string("3"), Some(1.0));
    }

    #[test]
    fn rejects_what_isnt_a_tint() {
        assert_eq!(from_string("tinted"), None);
        assert_eq!(from_string(""), None);
        assert_eq!(from_string("NaN"), None);
        assert_eq!(from_string("inf"), None);
    }

    #[test]
    fn reports_only_changes() {
        // One test owns the shared static, so the sequence below can't interleave with another.
        *LAST_REPORTED.lock_ignore_poison() = None;
        assert!(record_if_changed(Some(0.3)));
        assert!(!record_if_changed(Some(0.3)));
        assert!(record_if_changed(None));
        assert!(!record_if_changed(None));
        assert!(record_if_changed(Some(0.3)));
    }
}
