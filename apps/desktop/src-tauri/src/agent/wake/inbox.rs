//! The inbox: bundles waiting with deliver-by deadlines, and what a restart does to them.
//!
//! The pipeline's only real output decision is WHEN to wake the agent (agent-spec §6.2). The
//! interest scorer decides whether something matters; this turns that into a deadline, holds
//! the row until something wakes, and hands everything over at once when it does.
//!
//! Pure over values, clock injected per call, like the stages before it: `now` is always an
//! argument, never a read. That is what makes a settle window and a staleness horizon testable
//! without waiting out either one.

use std::time::Duration;

use super::{EventBundle, FolderImportance, Interest, ScoredBundle, WakeReadiness, interest, wake_delay};

/// How long after launch the inbox holds back rows that came due while the app was closed.
///
/// Launch replays the index journal, and that roll-forward is itself a burst of corrected
/// events. Waking mid-burst would have the agent report the app's own catch-up as though the
/// user had just done it, so the first digest after a restart waits for the noise to pass.
pub const SETTLE_AFTER_LAUNCH: Duration = Duration::from_secs(60);

/// How old a row's newest change may be before a restart drops it.
///
/// A bundle describing changes from three weeks ago is archaeology: the user has moved on, and
/// what the folder holds TODAY is something the agent can look up if it ever cares. Note this
/// is the inbox, not the proposal spine — a proposal never expires, because it is a decision
/// the user still owes an answer to. Pre-proposal signal is different.
pub const STALE_AFTER: Duration = Duration::from_secs(7 * 24 * 60 * 60);

/// One folder-window waiting for its deadline.
#[derive(Debug, Clone, PartialEq)]
pub struct InboxRow {
    pub bundle: EventBundle,
    /// The strongest claim any contribution to this row made.
    pub interest: Interest,
    /// When this row is due, unix seconds. `None` for a cold row.
    pub deliver_by: Option<u64>,
}

/// What a restart did to the inbox. Counted rather than silently applied, so a log line can
/// say what the agent was not told and why.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ReconcileReport {
    /// Rows whose newest change was older than [`STALE_AFTER`].
    pub dropped_stale: usize,
    /// Rows that were already overdue and now wait out [`SETTLE_AFTER_LAUNCH`].
    pub deferred: usize,
}

/// Bundles waiting to be delivered to the agent.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Inbox {
    rows: Vec<InboxRow>,
}

impl Inbox {
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// The waiting rows as scored bundles, WITHOUT draining.
    ///
    /// A wake shapes its digest from these before it commits to anything, so a budget too
    /// small to say anything, or a store that will not take a new thread, costs nothing: the
    /// rows are still waiting afterwards.
    pub fn scored(&self) -> Vec<ScoredBundle> {
        self.rows
            .iter()
            .map(|row| ScoredBundle {
                bundle: row.bundle.clone(),
                interest: row.interest,
            })
            .collect()
    }

    /// The rows as they wait, for the persistence edge to write.
    pub fn rows(&self) -> &[InboxRow] {
        &self.rows
    }

    /// Rebuild an inbox from stored rows, which is what a launch does before reconciling.
    pub fn from_rows(rows: Vec<InboxRow>) -> Self {
        Inbox { rows }
    }

    /// Take a bundle in, or merge it into the row already waiting for that folder-window.
    ///
    /// A merge can only pull the deadline EARLIER and can only raise the stored interest. That
    /// asymmetry is a starvation guard: a folder receiving a steady trickle would otherwise
    /// have its deadline pushed back by every new arrival and never come due at all, which is
    /// the one failure that would make the agent look asleep rather than patient. For the same
    /// reason a later, duller contribution cannot demote what an earlier burst established.
    ///
    /// `hot_delay` is the user's cadence setting, arriving as a value like every other input
    /// here: the core stays pure, and a caller that re-prices the inbox after a settings change
    /// does it by admitting against the new number rather than by reaching into a stored one.
    pub fn admit(&mut self, bundle: EventBundle, importance: FolderImportance, hot_delay: Duration, now: u64) {
        let existing = self
            .rows
            .iter_mut()
            .find(|row| row.bundle.folder == bundle.folder && row.bundle.window_start == bundle.window_start);

        match existing {
            Some(row) => {
                row.bundle.counters.merge(&bundle.counters);
                row.bundle.last_event_at = row.bundle.last_event_at.max(bundle.last_event_at);
                // Re-scored against the MERGED counters, so more change can earn a sooner wake.
                let scored = interest(&row.bundle, importance);
                row.deliver_by = soonest(row.deliver_by, deadline_for(scored, hot_delay, now));
                if scored.value() > row.interest.value() {
                    row.interest = scored;
                }
            }
            None => {
                let scored = interest(&bundle, importance);
                self.rows.push(InboxRow {
                    bundle,
                    interest: scored,
                    deliver_by: deadline_for(scored, hot_delay, now),
                });
            }
        }
    }

    /// Admit a bundle only if the gates permit STORING it, and report whether it landed.
    ///
    /// The gate lives here rather than at each call site so that "no consent, no rows" is one
    /// decision instead of a rule every producer has to remember. A caller that wants the
    /// unconditional behaviour still has [`admit`](Self::admit); the tap uses this one.
    pub fn admit_if_permitted(
        &mut self,
        readiness: WakeReadiness,
        bundle: EventBundle,
        importance: FolderImportance,
        hot_delay: Duration,
        now: u64,
    ) -> bool {
        if !readiness.admits_to_inbox() {
            return false;
        }
        self.admit(bundle, importance, hot_delay, now);
        true
    }

    /// The soonest deadline waiting, if anything is.
    pub fn next_deadline(&self) -> Option<u64> {
        // ⚠️ `filter_map`, not `map(…).min()`: `None` sorts BELOW every `Some`, so the plain
        // minimum would answer "nothing is waiting" for a full inbox holding one cold row.
        self.rows.iter().filter_map(|row| row.deliver_by).min()
    }

    /// Whether anything is due at `now`.
    pub fn due_at(&self, now: u64) -> bool {
        self.rows.iter().any(|row| row.deliver_by.is_some_and(|due| due <= now))
    }

    /// Hand over everything and empty the inbox.
    ///
    /// **Any wake drains the WHOLE inbox**, not just what came due. A hot bundle is what causes
    /// the wake; every cold one rides along for free, because the expensive part is the model
    /// turn, not the row. That is what makes a `MAX(interest)` wake policy fall out of the
    /// design instead of being written and maintained.
    pub fn drain(&mut self) -> Vec<ScoredBundle> {
        let mut rows = std::mem::take(&mut self.rows);
        // A stable order, matching the coalescer's, so two runs are diffable.
        rows.sort_by(|a, b| {
            a.bundle
                .window_start
                .cmp(&b.bundle.window_start)
                .then_with(|| a.bundle.folder.cmp(&b.bundle.folder))
        });
        rows.into_iter()
            .map(|row| ScoredBundle {
                bundle: row.bundle,
                interest: row.interest,
            })
            .collect()
    }

    /// Settle the inbox after a restart: drop what has gone stale, and hold what is already
    /// overdue until the app stops making noise about its own catch-up.
    pub fn reconcile(&mut self, launched_at: u64) -> ReconcileReport {
        let mut report = ReconcileReport::default();

        let horizon = launched_at.saturating_sub(STALE_AFTER.as_secs());
        self.rows.retain(|row| {
            let fresh = row.bundle.last_event_at >= horizon;
            if !fresh {
                report.dropped_stale += 1;
            }
            fresh
        });

        // ⚠️ Only rows that HAVE a deadline can be overdue. Comparing the `Option` directly would
        // hand every cold row a real deadline on every launch, undoing the ride-along and
        // inflating what the report claims it deferred.
        let settled = launched_at.saturating_add(SETTLE_AFTER_LAUNCH.as_secs());
        for row in &mut self.rows {
            if row.deliver_by.is_some_and(|due| due < settled) {
                row.deliver_by = Some(settled);
                report.deferred += 1;
            }
        }
        report
    }
}

/// When a bundle of this interest is due, from `now`, or `None` when it is not worth a wake of
/// its own.
fn deadline_for(scored: Interest, hot_delay: Duration, now: u64) -> Option<u64> {
    Some(now.saturating_add(wake_delay(scored, hot_delay)?.as_secs()))
}

/// Merge two deadlines for one row: the sooner of them, and a real deadline always beats none.
///
/// ⚠️ **Written out rather than `existing.min(incoming)`**, which compiles, reads right, and is
/// exactly backwards: `Option`'s derived `Ord` puts `None` below every `Some`, so a cold
/// contribution would ERASE the deadline a hot one established, and that folder would never wake
/// again. Having no deadline is the LONGEST wait there is, not the shortest.
fn soonest(existing: Option<u64>, incoming: Option<u64>) -> Option<u64> {
    match (existing, incoming) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (Some(only), None) | (None, Some(only)) => Some(only),
        (None, None) => None,
    }
}
