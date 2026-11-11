//! What each child window is doing right now: resting after its slide-in, or closing. Pure
//! bookkeeping, no AppKit, so every transition is unit-tested here.
//!
//! One mutex guards all of it, which is what makes a reopen and a close's next step race safely:
//! whichever takes the lock first wins, and the loser sees it.

use std::collections::HashMap;
use std::time::{Duration, Instant};

/// A window frame in AppKit's coordinates (origin bottom-left, in points). Only macOS fills one
/// in; elsewhere a close never slides, so its `rest` stays `None`.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(
    not(target_os = "macos"),
    allow(dead_code, reason = "only the AppKit path builds one")
)]
pub(crate) struct Frame {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Motion {
    /// The window slid in. Until `settles_at` it's still moving, so where it stands isn't where
    /// the user put it.
    SlidIn { settles_at: Instant },
    /// A close is under way. `ticket` names this close, so a late step of a cancelled one can't act
    /// on a window the user brought back. `rest` is where the window stood before it slid out, or
    /// `None` when it went without sliding.
    Closing { ticket: u64, rest: Option<Frame> },
}

/// The start of a close, as `Ledger::begin_close` hands it out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CloseTicket {
    pub ticket: u64,
    /// Whether the window came in with the slide, so it may leave with one.
    pub slid_in: bool,
}

/// What `Ledger::cancel_close` found.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Cancelled {
    /// No close was under way.
    NothingToCancel,
    /// A close without a slide: the window is hidden or about to be, and showing it is enough.
    Plain,
    /// A slide-out: slide the window back to `rest`.
    SlideBack { rest: Frame },
}

#[derive(Default)]
pub(crate) struct Ledger {
    windows: HashMap<String, Motion>,
    next_ticket: u64,
}

impl Ledger {
    /// A window just started its slide-in, which ends at `settles_at`.
    pub fn record_slide_in(&mut self, label: &str, settles_at: Instant) {
        self.windows.insert(label.to_owned(), Motion::SlidIn { settles_at });
    }

    pub fn is_closing(&self, label: &str) -> bool {
        matches!(self.windows.get(label), Some(Motion::Closing { .. }))
    }

    /// Starts a close. `None` when one is already under way: Escape plus the red button, or ⌘W
    /// pressed twice, is ordinary and must not schedule a second destroy.
    pub fn begin_close(&mut self, label: &str) -> Option<CloseTicket> {
        let slid_in = match self.windows.get(label) {
            Some(Motion::Closing { .. }) => return None,
            Some(Motion::SlidIn { .. }) => true,
            None => false,
        };
        self.next_ticket += 1;
        let ticket = self.next_ticket;
        self.windows
            .insert(label.to_owned(), Motion::Closing { ticket, rest: None });
        Some(CloseTicket { ticket, slid_in })
    }

    /// The close `ticket` has started its slide-out from `rest`.
    pub fn record_slide_out(&mut self, label: &str, ticket: u64, rest: Frame) {
        if let Some(Motion::Closing {
            ticket: current,
            rest: slot,
        }) = self.windows.get_mut(label)
            && *current == ticket
        {
            *slot = Some(rest);
        }
    }

    /// Whether the close `ticket` is still the one under way, so its next step may run.
    pub fn close_is_current(&self, label: &str, ticket: u64) -> bool {
        matches!(self.windows.get(label), Some(Motion::Closing { ticket: current, .. }) if *current == ticket)
    }

    /// The close `ticket` is about to destroy its window: forgets the window and reports `true`, or
    /// reports `false` when the close was cancelled meanwhile and the window must live.
    pub fn take_close(&mut self, label: &str, ticket: u64) -> bool {
        if self.close_is_current(label, ticket) {
            self.windows.remove(label);
            true
        } else {
            false
        }
    }

    /// The user brought the window back while it was closing: stops the close, so its remaining
    /// steps find a different (or no) ticket and do nothing. A window that slides back counts as
    /// slid in again, settling at `settles_at`.
    pub fn cancel_close(&mut self, label: &str, settles_at: Instant) -> Cancelled {
        match self.windows.get(label).copied() {
            Some(Motion::Closing { rest: Some(rest), .. }) => {
                self.record_slide_in(label, settles_at);
                Cancelled::SlideBack { rest }
            }
            Some(Motion::Closing { rest: None, .. }) => {
                self.windows.remove(label);
                Cancelled::Plain
            }
            _ => Cancelled::NothingToCancel,
        }
    }

    /// The window is gone, however it went.
    pub fn forget(&mut self, label: &str) {
        self.windows.remove(label);
    }

    /// Whether a rect the window's page reports at `now` is one the user chose. Mid-slide (in or
    /// out) it isn't: saving it would make every reopen land a little higher.
    pub fn accepts_rect_write(&self, label: &str, now: Instant) -> bool {
        match self.windows.get(label) {
            Some(Motion::SlidIn { settles_at }) => now >= *settles_at,
            Some(Motion::Closing { .. }) => false,
            None => true,
        }
    }
}

/// When a slide starting at `now` and running for `duration` has surely landed. AppKit drives the
/// animation from the display link, so its last frame can trail the nominal duration a little.
pub(crate) fn settles_at(now: Instant, duration: Duration) -> Instant {
    now + duration + SETTLE_MARGIN
}

/// Slack after a slide's nominal end before it counts as done.
pub(crate) const SETTLE_MARGIN: Duration = Duration::from_millis(40);

#[cfg(test)]
mod tests {
    use super::*;

    const REST: Frame = Frame {
        x: 100.0,
        y: 200.0,
        width: 600.0,
        height: 400.0,
    };

    #[test]
    fn a_window_that_slid_in_may_slide_out() {
        let mut ledger = Ledger::default();
        ledger.record_slide_in("settings", Instant::now());
        let close = ledger.begin_close("settings").expect("starts a close");
        assert!(close.slid_in);
    }

    #[test]
    fn a_window_that_never_slid_in_closes_without_a_slide() {
        // E2E windows are created visible, and Reduce motion shows a window without a slide.
        let mut ledger = Ledger::default();
        let close = ledger.begin_close("queue").expect("starts a close");
        assert!(!close.slid_in);
    }

    #[test]
    fn a_second_close_while_one_is_under_way_is_ignored() {
        let mut ledger = Ledger::default();
        assert!(ledger.begin_close("settings").is_some());
        assert_eq!(ledger.begin_close("settings"), None);
    }

    #[test]
    fn a_close_runs_to_the_destroy_when_nobody_cancels_it() {
        let mut ledger = Ledger::default();
        let close = ledger.begin_close("debug").expect("starts a close");
        assert!(ledger.close_is_current("debug", close.ticket));
        assert!(ledger.take_close("debug", close.ticket));
        // Forgotten, so a window opened later under the same label starts clean.
        assert!(!ledger.is_closing("debug"));
        assert!(ledger.begin_close("debug").is_some_and(|next| !next.slid_in));
    }

    #[test]
    fn cancelling_a_slide_out_slides_back_and_stops_the_destroy() {
        let mut ledger = Ledger::default();
        ledger.record_slide_in("settings", Instant::now());
        let close = ledger.begin_close("settings").expect("starts a close");
        ledger.record_slide_out("settings", close.ticket, REST);

        let settles = Instant::now() + Duration::from_millis(260);
        assert_eq!(
            ledger.cancel_close("settings", settles),
            Cancelled::SlideBack { rest: REST }
        );
        assert!(!ledger.close_is_current("settings", close.ticket));
        assert!(!ledger.take_close("settings", close.ticket));
        // Back to slid in, so the next close slides out again.
        assert!(ledger.begin_close("settings").is_some_and(|next| next.slid_in));
    }

    #[test]
    fn cancelling_a_plain_close_only_stops_the_destroy() {
        let mut ledger = Ledger::default();
        let close = ledger.begin_close("queue").expect("starts a close");
        assert_eq!(ledger.cancel_close("queue", Instant::now()), Cancelled::Plain);
        assert!(!ledger.take_close("queue", close.ticket));
    }

    #[test]
    fn cancelling_with_no_close_under_way_does_nothing() {
        let mut ledger = Ledger::default();
        ledger.record_slide_in("settings", Instant::now());
        assert_eq!(
            ledger.cancel_close("settings", Instant::now()),
            Cancelled::NothingToCancel
        );
        assert_eq!(
            ledger.cancel_close("shortcuts", Instant::now()),
            Cancelled::NothingToCancel
        );
    }

    #[test]
    fn a_stale_slide_out_frame_does_not_land_on_a_newer_close() {
        let mut ledger = Ledger::default();
        ledger.record_slide_in("settings", Instant::now());
        let first = ledger.begin_close("settings").expect("starts a close");
        ledger.cancel_close("settings", Instant::now());
        let second = ledger.begin_close("settings").expect("starts a close");
        ledger.record_slide_out("settings", first.ticket, REST);
        assert_eq!(ledger.cancel_close("settings", Instant::now()), Cancelled::Plain);
        assert_ne!(first.ticket, second.ticket);
    }

    #[test]
    fn rect_writes_mid_slide_are_refused() {
        // Regression anchor: the slide moves the window, and the page saves its rect on every
        // move, so a saved mid-slide rect shifted each reopen up by the slide distance.
        let mut ledger = Ledger::default();
        let now = Instant::now();
        let settles = now + Duration::from_millis(260);
        ledger.record_slide_in("settings", settles);
        assert!(!ledger.accepts_rect_write("settings", now));
        assert!(ledger.accepts_rect_write("settings", settles));

        ledger.begin_close("settings").expect("starts a close");
        assert!(!ledger.accepts_rect_write("settings", settles + Duration::from_secs(5)));
    }

    #[test]
    fn rect_writes_for_a_window_without_motion_are_accepted() {
        let ledger = Ledger::default();
        assert!(ledger.accepts_rect_write("shortcuts", Instant::now()));
    }

    #[test]
    fn forgetting_a_window_clears_its_close() {
        let mut ledger = Ledger::default();
        let close = ledger.begin_close("viewer-1").expect("starts a close");
        ledger.forget("viewer-1");
        assert!(!ledger.take_close("viewer-1", close.ticket));
    }
}
