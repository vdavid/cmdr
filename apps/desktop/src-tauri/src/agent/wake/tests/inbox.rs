//! The inbox: what is waiting, when it comes due, and what a restart does to it.

use super::super::*;

/// A folder with `created` arrivals, in the window starting at `window_start`.
fn arrivals(folder: &str, created: u32, window_start: u64) -> EventBundle {
    EventBundle {
        folder: folder.to_string(),
        counters: ChangeCounters {
            created,
            ..ChangeCounters::default()
        },
        window_start,
        last_event_at: window_start,
    }
}

/// An important folder, so its bundles land in the hot tier and the arithmetic below is
/// something a reader can follow.
const IMPORTANT: FolderImportance = FolderImportance::Scored(0.9);

/// A bundle admitted to the inbox comes due at `now + wake_delay(interest)`: the interest
/// scorer decides WHETHER something matters, and the deadline turns that into WHEN.
#[test]
fn an_admitted_bundle_comes_due_after_its_interests_delay() {
    let mut inbox = Inbox::default();
    inbox.admit(arrivals("/Users/someone/Downloads", 3, 100), IMPORTANT, 1_000);

    assert_eq!(inbox.next_deadline(), Some(1_000 + HOT_DELAY.as_secs()));
    assert!(!inbox.due_at(1_000), "not due the instant it arrives");
    assert!(inbox.due_at(1_000 + HOT_DELAY.as_secs()), "due when its deadline lands");
}

/// More change for a folder already waiting MERGES into its row and can only pull the deadline
/// EARLIER, never push it out.
///
/// This is a starvation guard, not an optimization: a folder receiving a steady trickle would
/// otherwise have its deadline pushed back by every new arrival and never come due at all,
/// which is the one failure mode that would make the agent look asleep rather than patient.
#[test]
fn more_change_can_only_pull_a_deadline_earlier() {
    let mut inbox = Inbox::default();
    // A cold folder first: a long delay.
    inbox.admit(arrivals("/tmp/quiet", 1, 100), FolderImportance::Floored, 1_000);
    let cold_deadline = inbox.next_deadline().expect("something is waiting");

    // The same folder and window, now with enough change to matter.
    inbox.admit(arrivals("/tmp/quiet", 40, 100), IMPORTANT, 1_100);

    assert_eq!(inbox.len(), 1, "same folder and window, so one row");
    let warmer = inbox.next_deadline().expect("still waiting");
    assert!(
        warmer < cold_deadline,
        "the deadline must move in ({warmer} vs {cold_deadline})"
    );

    // And a later trickle of nothing-much must not push it back out.
    inbox.admit(arrivals("/tmp/quiet", 1, 100), FolderImportance::Floored, 1_200);
    assert_eq!(
        inbox.next_deadline(),
        Some(warmer),
        "a trickle cannot postpone a deadline"
    );
}

/// Merging keeps the counts: the row is what the folder did in that window, whatever order the
/// pieces arrived in.
#[test]
fn merging_a_row_sums_what_happened() {
    let mut inbox = Inbox::default();
    inbox.admit(arrivals("/Users/someone/Downloads", 3, 100), IMPORTANT, 1_000);
    inbox.admit(arrivals("/Users/someone/Downloads", 4, 100), IMPORTANT, 1_010);

    let drained = inbox.drain();
    assert_eq!(drained.len(), 1);
    assert_eq!(drained[0].bundle.counters.created, 7);
}

/// The same folder in a DIFFERENT window is a different row, for the reason the coalescer keeps
/// them apart: this morning's arrivals and tonight's must not share a deadline.
#[test]
fn the_same_folder_in_two_windows_waits_as_two_rows() {
    let mut inbox = Inbox::default();
    inbox.admit(arrivals("/Users/someone/Downloads", 3, 100), IMPORTANT, 1_000);
    inbox.admit(arrivals("/Users/someone/Downloads", 3, 40_000), IMPORTANT, 1_000);

    assert_eq!(inbox.len(), 2);
}

/// **Any wake drains the WHOLE inbox.** A hot bundle is what causes the wake; every cold one
/// rides along for free, because the expensive part is the model turn, not the row. This is
/// what makes a `MAX(interest)` wake policy fall out without anyone writing one.
#[test]
fn a_wake_drains_everything_including_the_cold_rows() {
    let mut inbox = Inbox::default();
    inbox.admit(arrivals("/Users/someone/Downloads", 5, 100), IMPORTANT, 1_000);
    inbox.admit(arrivals("/tmp/junk", 5, 100), FolderImportance::Floored, 1_000);
    inbox.admit(
        arrivals("/Users/someone/code/thing", 2, 100),
        FolderImportance::Unknown,
        1_000,
    );

    let drained = inbox.drain();

    assert_eq!(drained.len(), 3, "the cold rows ride along: {drained:?}");
    assert_eq!(inbox.len(), 0, "and the inbox is empty afterwards");
    assert_eq!(inbox.next_deadline(), None);
}

/// Draining hands the bundles over already scored, so the compactor can rank them without
/// re-deriving anything the inbox already knew.
#[test]
fn drained_rows_carry_the_score_they_were_admitted_with() {
    let mut inbox = Inbox::default();
    inbox.admit(arrivals("/Users/someone/Downloads", 5, 100), IMPORTANT, 1_000);

    let drained = inbox.drain();
    assert!(drained[0].interest.value() > 0.0, "scored, not re-scored later");
}

/// A deadline that passed while the app was CLOSED does not fire the moment it launches.
///
/// Launch replays the index journal, and that roll-forward is itself a burst of corrected
/// events. Waking mid-burst would have the agent report the app's own catch-up as if the user
/// had just done it. The settle window is what keeps the first digest about the user.
#[test]
fn a_deadline_missed_while_closed_waits_out_the_settle_window() {
    let mut inbox = Inbox::default();
    inbox.admit(arrivals("/Users/someone/Downloads", 3, 100), IMPORTANT, 1_000);
    let launched_at = 50_000; // Long after that deadline passed.

    inbox.reconcile(launched_at);

    assert!(!inbox.due_at(launched_at), "not the instant we launch");
    assert!(
        !inbox.due_at(launched_at + SETTLE_AFTER_LAUNCH.as_secs() - 1),
        "nor part-way through settling"
    );
    assert!(
        inbox.due_at(launched_at + SETTLE_AFTER_LAUNCH.as_secs()),
        "but once the app has settled, it is genuinely overdue"
    );
}

/// A row about changes from weeks ago is archaeology: the user has moved on, and the folder's
/// state today is something the agent can look up if it ever cares. Dropped — but COUNTED, so
/// the drop is visible in a log rather than being a silent gap in what the agent was told.
#[test]
fn rows_older_than_the_staleness_horizon_are_dropped_and_counted() {
    let mut inbox = Inbox::default();
    let ancient = 1_000;
    let recent = 1_000_000;
    inbox.admit(arrivals("/Users/someone/old-thing", 3, ancient), IMPORTANT, ancient);
    inbox.admit(arrivals("/Users/someone/Downloads", 3, recent), IMPORTANT, recent);

    let report = inbox.reconcile(recent + 10);

    assert_eq!(report.dropped_stale, 1, "the ancient row goes");
    assert_eq!(inbox.len(), 1, "the recent one stays");
    assert_eq!(inbox.drain()[0].bundle.folder, "/Users/someone/Downloads");
}

/// Reconciling an empty inbox is a no-op that reports nothing, which is the common case: most
/// launches have nothing waiting.
#[test]
fn reconciling_an_empty_inbox_reports_nothing() {
    let mut inbox = Inbox::default();
    let report = inbox.reconcile(10_000);

    assert_eq!(report.dropped_stale, 0);
    assert_eq!(report.deferred, 0);
    assert!(!inbox.due_at(10_000 + SETTLE_AFTER_LAUNCH.as_secs()));
}

/// The gate at the storing end: an unconsented agent keeps no record of what the user has
/// been doing with their files, because nobody has agreed to that.
#[test]
fn an_unconsented_agent_stores_nothing() {
    let mut inbox = Inbox::default();

    let admitted = inbox.admit_if_permitted(
        WakeReadiness::NeedsConsent,
        arrivals("/Users/someone/Downloads", 3, 100),
        IMPORTANT,
        1_000,
    );

    assert!(!admitted);
    assert_eq!(inbox.len(), 0, "no row, not even a deferred one");
}

/// A missing key is a different kind of gap: the user opted in, so signal accumulates and
/// waits for them to close it.
#[test]
fn a_missing_key_still_lets_signal_accumulate() {
    let mut inbox = Inbox::default();

    let admitted = inbox.admit_if_permitted(
        WakeReadiness::NeedsApiKey,
        arrivals("/Users/someone/Downloads", 3, 100),
        IMPORTANT,
        1_000,
    );

    assert!(admitted);
    assert_eq!(inbox.len(), 1);
}
