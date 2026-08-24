//! The inbox: what is waiting, when it comes due, and what a restart does to it.

use std::time::Duration;

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
    inbox.admit(
        arrivals("/Users/someone/Downloads", 3, 100),
        IMPORTANT,
        DEFAULT_HOT_DELAY,
        1_000,
    );

    assert_eq!(inbox.next_deadline(), Some(1_000 + DEFAULT_HOT_DELAY.as_secs()));
    assert!(!inbox.due_at(1_000), "not due the instant it arrives");
    assert!(
        inbox.due_at(1_000 + DEFAULT_HOT_DELAY.as_secs()),
        "due when its deadline lands"
    );
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
    // A middling folder first: a warm delay.
    inbox.admit(
        arrivals("/tmp/quiet", 1, 100),
        FolderImportance::Scored(0.4),
        DEFAULT_HOT_DELAY,
        1_000,
    );
    let warm_deadline = inbox.next_deadline().expect("something is waiting");

    // The same folder and window, now with enough change to matter.
    inbox.admit(arrivals("/tmp/quiet", 40, 100), IMPORTANT, DEFAULT_HOT_DELAY, 1_100);

    assert_eq!(inbox.len(), 1, "same folder and window, so one row");
    let hotter = inbox.next_deadline().expect("still waiting");
    assert!(
        hotter < warm_deadline,
        "the deadline must move in ({hotter} vs {warm_deadline})"
    );

    // And a later trickle of nothing-much must not push it back out.
    inbox.admit(
        arrivals("/tmp/quiet", 1, 100),
        FolderImportance::Scored(0.4),
        DEFAULT_HOT_DELAY,
        1_200,
    );
    assert_eq!(
        inbox.next_deadline(),
        Some(hotter),
        "a trickle cannot postpone a deadline"
    );
}

/// A cold bundle rides along on the next wake and never causes one of its own, so it waits with
/// NO deadline at all.
///
/// Given a deadline like every other row, a trickle in a barely-scored folder comes due on its
/// own and spends a whole model turn reporting that something happened in a cache directory.
#[test]
fn a_cold_bundle_sets_no_deadline_of_its_own() {
    let mut inbox = Inbox::default();
    inbox.admit(
        arrivals("/tmp/junk", 5, 100),
        FolderImportance::Floored,
        DEFAULT_HOT_DELAY,
        1_000,
    );

    assert_eq!(inbox.len(), 1, "it still waits, ready to ride along");
    assert_eq!(inbox.next_deadline(), None, "but nothing is due because of it");
    assert!(
        !inbox.due_at(1_000 + 100 * 24 * 60 * 60),
        "not tomorrow, not next month either"
    );
}

/// ⚠️ **The trap this whole change turns on.** `Option`'s derived `Ord` puts `None` below every
/// `Some`, so writing the merge as `row.deliver_by.min(incoming)` compiles, reads right, and does
/// the opposite: a junk contribution ERASES the deadline a real one established, and that folder
/// never wakes again.
#[test]
fn a_cold_contribution_cannot_erase_a_waiting_deadline() {
    let mut inbox = Inbox::default();
    inbox.admit(
        arrivals("/Users/someone/Downloads", 3, 100),
        IMPORTANT,
        DEFAULT_HOT_DELAY,
        1_000,
    );
    let due = inbox.next_deadline().expect("the hot row waits for something");

    // The same folder-window, arriving again as junk: no deadline of its own.
    inbox.admit(
        arrivals("/Users/someone/Downloads", 1, 100),
        FolderImportance::Floored,
        DEFAULT_HOT_DELAY,
        1_050,
    );

    assert_eq!(
        inbox.next_deadline(),
        Some(due),
        "no-deadline loses to a real deadline, in both merge directions"
    );
}

/// The other direction of the same merge: a row that never had a deadline takes the first real
/// one offered, or the folder that finally got interesting keeps waiting forever.
#[test]
fn a_real_deadline_lands_on_a_row_that_had_none() {
    let mut inbox = Inbox::default();
    inbox.admit(
        arrivals("/Users/someone/Downloads", 1, 100),
        FolderImportance::Floored,
        DEFAULT_HOT_DELAY,
        1_000,
    );
    assert_eq!(inbox.next_deadline(), None, "cold, so nothing is due");

    inbox.admit(
        arrivals("/Users/someone/Downloads", 3, 100),
        IMPORTANT,
        DEFAULT_HOT_DELAY,
        1_050,
    );

    assert_eq!(inbox.next_deadline(), Some(1_050 + DEFAULT_HOT_DELAY.as_secs()));
}

/// A row with no deadline is not a row due at the beginning of time. Taking the plain minimum
/// over the deadlines would answer `None` for a whole inbox because one junk row is in it.
#[test]
fn the_next_deadline_ignores_the_rows_that_have_none() {
    let mut inbox = Inbox::default();
    inbox.admit(
        arrivals("/tmp/junk", 5, 100),
        FolderImportance::Floored,
        DEFAULT_HOT_DELAY,
        1_000,
    );
    inbox.admit(
        arrivals("/Users/someone/Downloads", 3, 100),
        IMPORTANT,
        DEFAULT_HOT_DELAY,
        1_000,
    );

    assert_eq!(inbox.next_deadline(), Some(1_000 + DEFAULT_HOT_DELAY.as_secs()));
}

/// A restart defers what was already overdue, and a row with no deadline was never overdue.
/// Handing it `settled` would give every cold row a deadline on every launch, undoing the
/// ride-along entirely and inflating what the reconcile report claims it deferred.
#[test]
fn reconciling_leaves_a_row_with_no_deadline_alone() {
    let mut inbox = Inbox::default();
    inbox.admit(
        arrivals("/tmp/junk", 5, 100),
        FolderImportance::Floored,
        DEFAULT_HOT_DELAY,
        1_000,
    );

    let report = inbox.reconcile(2_000);

    assert_eq!(report.deferred, 0, "a row that was never due cannot be overdue");
    assert_eq!(inbox.next_deadline(), None, "and it must not acquire one at launch");
    assert_eq!(inbox.len(), 1, "it stays, to ride along on the next wake");
}

/// Merging keeps the counts: the row is what the folder did in that window, whatever order the
/// pieces arrived in.
#[test]
fn merging_a_row_sums_what_happened() {
    let mut inbox = Inbox::default();
    inbox.admit(
        arrivals("/Users/someone/Downloads", 3, 100),
        IMPORTANT,
        DEFAULT_HOT_DELAY,
        1_000,
    );
    inbox.admit(
        arrivals("/Users/someone/Downloads", 4, 100),
        IMPORTANT,
        DEFAULT_HOT_DELAY,
        1_010,
    );

    let drained = inbox.drain();
    assert_eq!(drained.len(), 1);
    assert_eq!(drained[0].bundle.counters.created, 7);
}

/// The same folder in a DIFFERENT window is a different row, for the reason the coalescer keeps
/// them apart: this morning's arrivals and tonight's must not share a deadline.
#[test]
fn the_same_folder_in_two_windows_waits_as_two_rows() {
    let mut inbox = Inbox::default();
    inbox.admit(
        arrivals("/Users/someone/Downloads", 3, 100),
        IMPORTANT,
        DEFAULT_HOT_DELAY,
        1_000,
    );
    inbox.admit(
        arrivals("/Users/someone/Downloads", 3, 40_000),
        IMPORTANT,
        DEFAULT_HOT_DELAY,
        1_000,
    );

    assert_eq!(inbox.len(), 2);
}

/// **Any wake drains the WHOLE inbox.** A hot bundle is what causes the wake; every cold one
/// rides along for free, because the expensive part is the model turn, not the row. This is
/// what makes a `MAX(interest)` wake policy fall out without anyone writing one.
#[test]
fn a_wake_drains_everything_including_the_cold_rows() {
    let mut inbox = Inbox::default();
    inbox.admit(
        arrivals("/Users/someone/Downloads", 5, 100),
        IMPORTANT,
        DEFAULT_HOT_DELAY,
        1_000,
    );
    inbox.admit(
        arrivals("/tmp/junk", 5, 100),
        FolderImportance::Floored,
        DEFAULT_HOT_DELAY,
        1_000,
    );
    inbox.admit(
        arrivals("/Users/someone/code/thing", 2, 100),
        FolderImportance::Unknown,
        DEFAULT_HOT_DELAY,
        1_000,
    );

    let drained = inbox.drain();

    assert_eq!(drained.len(), 3, "the cold rows ride along: {drained:?}");
    assert_eq!(inbox.len(), 0, "and the inbox is empty afterwards");
    assert_eq!(inbox.next_deadline(), None);
}

/// Narrowing to one folder leaves that folder's rows alone and takes every other row with it,
/// windows included.
///
/// This is what a FORCED wake stands on. An E2E spec's premise is "the digest covers the folder
/// I staged", and the indexer's tap feeds the same inbox from whatever else the suite is doing:
/// a spec staging one folder got a digest tallying seven, and the thread it opened was named
/// after somebody else's.
#[test]
fn narrowing_to_one_folder_keeps_its_rows_and_drops_the_rest() {
    let mut inbox = Inbox::default();
    inbox.admit(
        arrivals("/Users/e2e/staged", 5, 60),
        IMPORTANT,
        DEFAULT_HOT_DELAY,
        1_000,
    );
    inbox.admit(
        arrivals("/Users/e2e/staged", 3, 120),
        IMPORTANT,
        DEFAULT_HOT_DELAY,
        1_000,
    );
    inbox.admit(
        arrivals("/Users/e2e/elsewhere", 5, 60),
        IMPORTANT,
        DEFAULT_HOT_DELAY,
        1_000,
    );
    inbox.admit(
        arrivals("/tmp/whatever-the-indexer-saw", 9, 60),
        FolderImportance::Unknown,
        DEFAULT_HOT_DELAY,
        1_000,
    );

    let dropped = inbox.retain_folder("/Users/e2e/staged");

    assert_eq!(dropped, 2, "the two rows nobody staged");
    assert_eq!(inbox.len(), 2, "both of the staged folder's windows stay");
    assert!(
        inbox.drain().iter().all(|row| row.bundle.folder == "/Users/e2e/staged"),
        "and the wake can only report on what was staged"
    );
}

/// Narrowing to a folder nothing is waiting for empties the inbox rather than leaving the
/// backlog to be reported on: a forced wake covers what it staged or nothing at all.
#[test]
fn narrowing_to_a_folder_with_nothing_waiting_leaves_an_empty_inbox() {
    let mut inbox = Inbox::default();
    inbox.admit(
        arrivals("/Users/e2e/elsewhere", 5, 60),
        IMPORTANT,
        DEFAULT_HOT_DELAY,
        1_000,
    );

    assert_eq!(inbox.retain_folder("/Users/e2e/staged"), 1);
    assert!(inbox.is_empty());
}

/// Draining hands the bundles over already scored, so the compactor can rank them without
/// re-deriving anything the inbox already knew.
#[test]
fn drained_rows_carry_the_score_they_were_admitted_with() {
    let mut inbox = Inbox::default();
    inbox.admit(
        arrivals("/Users/someone/Downloads", 5, 100),
        IMPORTANT,
        DEFAULT_HOT_DELAY,
        1_000,
    );

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
    inbox.admit(
        arrivals("/Users/someone/Downloads", 3, 100),
        IMPORTANT,
        DEFAULT_HOT_DELAY,
        1_000,
    );
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
    inbox.admit(
        arrivals("/Users/someone/old-thing", 3, ancient),
        IMPORTANT,
        DEFAULT_HOT_DELAY,
        ancient,
    );
    inbox.admit(
        arrivals("/Users/someone/Downloads", 3, recent),
        IMPORTANT,
        DEFAULT_HOT_DELAY,
        recent,
    );

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
        DEFAULT_HOT_DELAY,
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
        DEFAULT_HOT_DELAY,
        1_000,
    );

    assert!(admitted);
    assert_eq!(inbox.len(), 1);
}

/// ⚠️ **The re-pricing trap.** A merge is min-only, deliberately (the starvation guard above),
/// so a LENGTHENED cadence would never reach anything already waiting: the user asks for a
/// calmer agent, and every row currently queued still fires on the old, twitchy schedule. The
/// setting has to be pushed across the inbox explicitly.
#[test]
fn a_lengthened_cadence_reaches_the_rows_already_waiting() {
    let mut inbox = Inbox::default();
    inbox.admit(
        arrivals("/Users/someone/Downloads", 3, 100),
        IMPORTANT,
        DEFAULT_HOT_DELAY,
        1_000,
    );
    assert_eq!(inbox.next_deadline(), Some(1_000 + DEFAULT_HOT_DELAY.as_secs()));

    let calmer = Duration::from_secs(30 * 60);
    inbox.reprice(DEFAULT_HOT_DELAY, calmer);

    assert_eq!(
        inbox.next_deadline(),
        Some(1_000 + calmer.as_secs()),
        "the row keeps the moment it arrived and takes the new patience"
    );
}

/// Attentiveness applies just as immediately in the other direction: somebody who moves the
/// slider to its shortest stop expects the backlog to come due now, not on the old schedule.
#[test]
fn a_shortened_cadence_pulls_the_waiting_rows_in() {
    let mut inbox = Inbox::default();
    let calm = Duration::from_secs(30 * 60);
    inbox.admit(arrivals("/Users/someone/Downloads", 3, 100), IMPORTANT, calm, 1_000);

    inbox.reprice(calm, DEFAULT_HOT_DELAY);

    assert_eq!(inbox.next_deadline(), Some(1_000 + DEFAULT_HOT_DELAY.as_secs()));
}

/// A cold row has no deadline at ANY cadence: the slider moves the hot tier, and cold's
/// ride-along is not a delay that can be lengthened. Handing it one here would undo the whole
/// reason a cache directory stopped spending model turns.
#[test]
fn repricing_leaves_a_cold_row_without_a_deadline() {
    let mut inbox = Inbox::default();
    inbox.admit(
        arrivals("/Users/someone/Library/Caches/build", 1, 100),
        FolderImportance::Floored,
        DEFAULT_HOT_DELAY,
        1_000,
    );
    assert_eq!(inbox.next_deadline(), None, "cold to begin with");

    inbox.reprice(DEFAULT_HOT_DELAY, Duration::from_secs(2 * 60 * 60));

    assert_eq!(inbox.next_deadline(), None);
    assert_eq!(inbox.len(), 1, "and it is still riding along");
}

/// Moving the slider and moving it back leaves the inbox exactly where it was. The shift is
/// against the row's own arrival, not against whenever the user happened to open Settings, so
/// fiddling with the control cannot walk a deadline away from its folder.
#[test]
fn repricing_there_and_back_changes_nothing() {
    let mut inbox = Inbox::default();
    inbox.admit(
        arrivals("/Users/someone/Downloads", 3, 100),
        IMPORTANT,
        DEFAULT_HOT_DELAY,
        1_000,
    );
    // A warm row too, so the tier-specific delta is exercised rather than the hot one twice.
    inbox.admit(
        arrivals("/Users/someone/Documents", 1, 100),
        FolderImportance::Scored(0.4),
        DEFAULT_HOT_DELAY,
        1_000,
    );
    let before = inbox.rows().to_vec();

    inbox.reprice(DEFAULT_HOT_DELAY, Duration::from_secs(15 * 60));
    inbox.reprice(Duration::from_secs(15 * 60), DEFAULT_HOT_DELAY);

    assert_eq!(inbox.rows(), before.as_slice());
}

/// ⚠️ **Consent going away has to take the backlog with it.** Refusing new rows is only half
/// the gate: rows admitted while the user was consented are a record of what they have been
/// doing with their files, and the moment the purpose they agreed to is withdrawn (a revoke, or
/// a consent-copy bump that un-accepts everybody) keeping that record is exactly what
/// `readiness.rs` says the pipeline must not do.
#[test]
fn losing_consent_drops_what_was_already_waiting() {
    let mut inbox = Inbox::default();
    inbox.admit(
        arrivals("/Users/someone/Downloads", 3, 100),
        IMPORTANT,
        DEFAULT_HOT_DELAY,
        1_000,
    );
    inbox.admit(
        arrivals("/Users/someone/Desktop", 2, 100),
        IMPORTANT,
        DEFAULT_HOT_DELAY,
        1_000,
    );

    let dropped = inbox.purge_if_not_permitted(WakeReadiness::NeedsConsent);

    assert_eq!(dropped, 2, "and it says how many, so the log can be honest about it");
    assert!(inbox.is_empty());
    assert_eq!(inbox.next_deadline(), None, "so nothing is left to wake against");
}

/// The other three states are gaps the user can close, not a purpose they withdrew, so the
/// backlog waiting for them is theirs and stays put.
#[test]
fn a_closable_gap_keeps_the_backlog() {
    for readiness in [
        WakeReadiness::Ready,
        WakeReadiness::NeedsFullDiskAccess,
        WakeReadiness::NeedsApiKey,
    ] {
        let mut inbox = Inbox::default();
        inbox.admit(
            arrivals("/Users/someone/Downloads", 3, 100),
            IMPORTANT,
            DEFAULT_HOT_DELAY,
            1_000,
        );

        assert_eq!(
            inbox.purge_if_not_permitted(readiness),
            0,
            "{readiness:?} drops nothing"
        );
        assert_eq!(inbox.len(), 1, "{readiness:?} keeps the row");
    }
}
