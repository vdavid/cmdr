//! macOS: a mouse's dedicated back / forward navigation, read from AppKit rather
//! than from the DOM.
//!
//! The frontend maps `MouseEvent.button === 3 / 4` to `nav.back` / `nav.forward`
//! (`routes/(main)/mouse-nav.ts`), which is what a plain five-button mouse
//! delivers. On macOS that path can't be relied on, because the mouse's own
//! driver often replaces the button entirely:
//!
//! **Gotcha / Why:** with Logi Options+ installed, an MX-series mouse's thumb
//! buttons emit NO mouse button at all. Options+ binds them to
//! `OSX_GESTURE_BACK` / `OSX_GESTURE_FORWARD` with `hidUsage: 0`, and posts a
//! macOS swipe instead — `NSEventType::Swipe`, `deltaX` `+1` for back and `-1`
//! for forward. So neither the DOM's `button === 3 / 4` nor an `otherMouseDown`
//! monitor ever fires for them. (Verified with an AppKit event probe against a
//! Logitech MX Master 4 + Logi Options+, macOS 27, 2026-09-04; probe output and
//! the Options+ config trail in `docs/notes/mx-side-buttons-swipe-2026-09-04.md`.)
//!
//! Both roads therefore land here: this module watches the `otherMouse` buttons
//! AND the swipe gesture, and emits `mouse-nav` to the main window, which
//! dispatches the same bus command as `⌘[` / `⌘]`. The DOM path stays as the
//! Linux/WebKitGTK route.
//!
//! ## Why the monitor swallows the events it recognizes
//!
//! Returning `null` from the handler drops the event before any window sees it.
//! That's deliberate on both roads. For the BUTTONS: WebKit's mapping of extra
//! mouse buttons onto the DOM's three-button vocabulary is not something we
//! control, and Cmdr gives the MIDDLE button real gestures (close a tab, open a
//! folder in a background tab); an X1 press that arrived as a middle click would
//! close whatever it happened to be over. For the SWIPE: it's the gesture
//! WKWebView's own back / forward navigation reads, which would pop the SvelteKit
//! SPA history underneath us. Every other `otherMouse` event (the middle button
//! included) is handed straight back.
//!
//! The monitor is LOCAL, so it only sees events already destined for this app —
//! no accessibility permission, and nothing observed while another app is front.

use std::ptr::NonNull;

use log::{debug, warn};
use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2_app_kit::{NSEvent, NSEventMask, NSEventType};
use tauri::{AppHandle, Manager, Runtime};
use tauri_specta::Event as _;

use crate::window_events::{MouseNav, MouseNavDirection};

/// Fourth mouse button (X1), conventionally "back". Same numbering the UI Events
/// spec gives `MouseEvent.button`, which is why the two sides agree.
const BUTTON_BACK: isize = 3;

/// Fifth mouse button (X2), conventionally "forward".
const BUTTON_FORWARD: isize = 4;

/// The only window a pane-history walk means anything in.
const MAIN_WINDOW_LABEL: &str = "main";

/// What one AppKit event means to us. Three outcomes, because "ours" and
/// "carries a direction" aren't the same thing: a swipe arrives as a pair, and
/// only its second half knows which way it went.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Action {
    /// Not ours. Hand the event back untouched.
    PassThrough,
    /// Ours, but there's nothing to dispatch: the press half of a button
    /// (we navigate on release) or the `Began` half of a swipe (`deltaX` is 0
    /// until the gesture ends). Swallowed so the pair is consistent.
    SwallowOnly,
    /// Ours, and it says which way to walk the history.
    Navigate(MouseNavDirection),
}

/// The history direction a side button drives, or `None` for a button we don't
/// own (the middle button, and anything past the fifth).
fn direction_for_button(button_number: isize) -> Option<MouseNavDirection> {
    match button_number {
        BUTTON_BACK => Some(MouseNavDirection::Back),
        BUTTON_FORWARD => Some(MouseNavDirection::Forward),
        _ => None,
    }
}

/// The history direction a swipe drives, or `None` when it carries no direction
/// yet. AppKit's `swipeWithEvent:` sign convention: a swipe to the RIGHT
/// (positive `deltaX`) goes back, to the left goes forward — the same way the
/// gesture reads in every macOS app, and what Options+ posts for the thumb
/// buttons.
fn direction_for_swipe(delta_x: f64) -> Option<MouseNavDirection> {
    if delta_x > 0.0 {
        Some(MouseNavDirection::Back)
    } else if delta_x < 0.0 {
        Some(MouseNavDirection::Forward)
    } else {
        None
    }
}

/// Classifies one event. Pure, so the whole decision table is unit-testable
/// without an `NSEvent`; the handler below only reads the fields and acts.
fn action_for(event_type: NSEventType, button_number: isize, delta_x: f64) -> Action {
    if event_type == NSEventType::Swipe {
        // A swipe is ours whichever half it is: swallowing only the half that
        // carries a direction would leak the `Began` event to the webview.
        return match direction_for_swipe(delta_x) {
            Some(direction) => Action::Navigate(direction),
            None => Action::SwallowOnly,
        };
    }

    let Some(direction) = direction_for_button(button_number) else {
        return Action::PassThrough;
    };

    // Navigate on the UP edge (the press is only swallowed), mirroring the DOM
    // path and every other click gesture in the app.
    if event_type == NSEventType::OtherMouseUp {
        Action::Navigate(direction)
    } else {
        Action::SwallowOnly
    }
}

/// Installs the local `NSEvent` monitor. Call once, from the Tauri setup hook.
///
/// The monitor lives for the whole session: there's no teardown point, and the
/// buttons should work for as long as the app is up. Same shape as the
/// notification observers in `accent_color.rs` / `reduce_transparency.rs`.
pub fn install<R: Runtime>(app_handle: AppHandle<R>) {
    // Compile-time proof of the AppKit main thread; `addLocalMonitor…` is an
    // AppKit call, and the setup hook is where we are.
    let _mtm = MainThreadMarker::new().expect("install runs on the main thread (Tauri setup hook)");

    let block = block2::RcBlock::new(move |event: NonNull<NSEvent>| -> *mut NSEvent {
        // SAFETY: AppKit passes the monitor a live `NSEvent` that stays valid for
        // the duration of the callback, and we only read from it here.
        let ns_event = unsafe { event.as_ref() };

        let event_type = ns_event.r#type();
        // `deltaX` is only meaningful on the gesture events; the button branch
        // ignores it, and the mask keeps anything else out.
        let delta_x = if event_type == NSEventType::Swipe {
            ns_event.deltaX()
        } else {
            0.0
        };
        let action = action_for(event_type, ns_event.buttonNumber(), delta_x);
        if action == Action::PassThrough {
            return event.as_ptr(); // not ours: hand it back untouched
        }

        // The gestures stay inert unless the main window is the one being used;
        // a settings or viewer window has no pane history to walk, and swallowing
        // there would eat an event its own webview might want.
        // (`Manager::get_focused_window` would say this directly, but it's behind
        // Tauri's `unstable` feature, so ask the main window itself.)
        let focused_is_main = app_handle
            .get_webview_window(MAIN_WINDOW_LABEL)
            .and_then(|window| window.is_focused().ok())
            .unwrap_or(false);
        if !focused_is_main {
            return event.as_ptr();
        }

        if let Action::Navigate(direction) = action {
            debug!(target: "mouse_nav", "Navigation gesture {:?}", direction);
            if let Err(e) = (MouseNav { direction }).emit_to(&app_handle, MAIN_WINDOW_LABEL) {
                warn!(target: "mouse_nav", "Couldn't emit mouse-nav: {e}");
            }
        }
        std::ptr::null_mut()
    });

    // SAFETY: `block` is a live `RcBlock` with the
    // `(NonNull<NSEvent>) -> *mut NSEvent` signature the handler parameter
    // declares, and AppKit copies it. The mask covers only the three event types
    // the handler inspects.
    let monitor = unsafe {
        NSEvent::addLocalMonitorForEventsMatchingMask_handler(
            NSEventMask::OtherMouseDown | NSEventMask::OtherMouseUp | NSEventMask::Swipe,
            &block,
        )
    };

    match monitor {
        // Leaking the monitor token is what keeps it installed: dropping the
        // `Retained` would release it and the buttons would go dead. There's no
        // point in the app's life where we'd want to remove it.
        Some(monitor) => {
            let _installed = Retained::into_raw(monitor);
            debug!(target: "mouse_nav", "Side-button monitor installed");
        }
        None => warn!(
            target: "mouse_nav",
            "AppKit refused the side-button event monitor; the mouse's back / forward buttons won't navigate"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A button number no gesture owns, for the swipe cases (a swipe's
    /// `buttonNumber` is meaningless and must never be consulted).
    const UNOWNED_BUTTON: isize = 0;

    #[test]
    fn maps_only_the_two_side_buttons() {
        assert_eq!(direction_for_button(BUTTON_BACK), Some(MouseNavDirection::Back));
        assert_eq!(direction_for_button(BUTTON_FORWARD), Some(MouseNavDirection::Forward));
    }

    #[test]
    fn leaves_every_other_button_alone() {
        // 2 is the middle button, which Cmdr's own gestures use.
        for button in [0, 1, 2, 5, 6] {
            assert_eq!(
                direction_for_button(button),
                None,
                "button {button} should not navigate"
            );
        }
    }

    #[test]
    fn maps_swipe_direction_by_the_appkit_sign_convention() {
        assert_eq!(direction_for_swipe(1.0), Some(MouseNavDirection::Back));
        assert_eq!(direction_for_swipe(-1.0), Some(MouseNavDirection::Forward));
    }

    #[test]
    fn a_directionless_swipe_navigates_nowhere() {
        assert_eq!(direction_for_swipe(0.0), None);
    }

    #[test]
    fn a_side_button_navigates_on_release_only() {
        assert_eq!(
            action_for(NSEventType::OtherMouseUp, BUTTON_BACK, 0.0),
            Action::Navigate(MouseNavDirection::Back)
        );
        assert_eq!(
            action_for(NSEventType::OtherMouseDown, BUTTON_BACK, 0.0),
            Action::SwallowOnly
        );
    }

    #[test]
    fn the_middle_button_is_handed_back_on_both_edges() {
        // Cmdr's own middle-click gestures live in the webview, so both halves
        // have to reach it.
        assert_eq!(action_for(NSEventType::OtherMouseDown, 2, 0.0), Action::PassThrough);
        assert_eq!(action_for(NSEventType::OtherMouseUp, 2, 0.0), Action::PassThrough);
    }

    #[test]
    fn both_halves_of_a_swipe_are_swallowed() {
        // Options+ posts the pair `deltaX: 0` (Began) then `deltaX: ±1` (Ended).
        assert_eq!(action_for(NSEventType::Swipe, UNOWNED_BUTTON, 0.0), Action::SwallowOnly);
        assert_eq!(
            action_for(NSEventType::Swipe, UNOWNED_BUTTON, 1.0),
            Action::Navigate(MouseNavDirection::Back)
        );
        assert_eq!(
            action_for(NSEventType::Swipe, UNOWNED_BUTTON, -1.0),
            Action::Navigate(MouseNavDirection::Forward)
        );
    }
}
