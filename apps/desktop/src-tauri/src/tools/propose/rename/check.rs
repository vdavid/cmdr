//! One running rename check: no time limit, live progress, and a way to stop it.
//!
//! A check stats every row, and a 500-row job on a busy NAS can take minutes while working
//! perfectly. So nothing here gives up on a clock; the user watches the count and decides.
//!
//! **Stopping answers at once and detaches the scan.** The scan runs as its own task and looks
//! at the token between rows. The waiter stops waiting the moment the token trips, so a stat
//! stuck on a dead mount can't hold the review hostage, and the scan ends by itself once that
//! stat returns. ❌ Never cancel by dropping the scan's future: that wedges an MTP phone
//! mid-transaction (`commands/CLAUDE.md`).
//!
//! **Progress comes from the waiter's clock, ❌ never from the scan's rows.** A scan whose first
//! stat hangs has finished no rows, and that's the very case that most needs a count and a Stop
//! button on screen. A check that ends inside [`PROGRESS_QUIET`] emits nothing at all, so the
//! ordinary local recheck after each toggle doesn't flicker.

use std::collections::HashMap;
use std::future::Future;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri_specta::Event;
use tokio_util::sync::CancellationToken;

use crate::ignore_poison::IgnorePoison;

/// How long a check runs before it says anything.
const PROGRESS_QUIET: Duration = Duration::from_millis(300);
/// How often a check past the quiet period reports, when its count moved.
const PROGRESS_EVERY: Duration = Duration::from_millis(200);

/// How far one proposal's running check has got.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
#[tauri_specta(event_name = "bulk-rename-check-progress")]
pub struct BulkRenameCheckProgress {
    pub proposal_id: String,
    /// Rows finished so far. 0 is a real answer: the first stat is still out.
    pub checked: u32,
    pub total: u32,
}

/// The check running for each proposal. At most one per proposal: a new one replaces the old,
/// because a watcher event or an edited name makes the older answer stale, and a scan left
/// running per recheck would pile up against a slow volume.
#[derive(Default)]
pub struct RenameChecks {
    running: Mutex<HashMap<String, Registered>>,
    next_generation: AtomicU64,
}

struct Registered {
    generation: u64,
    token: CancellationToken,
}

impl RenameChecks {
    /// Register a check for this proposal, stopping the one it replaces.
    pub(super) fn begin(&self, proposal_id: &str) -> RunningCheck<'_> {
        let generation = self.next_generation.fetch_add(1, Ordering::Relaxed);
        let token = CancellationToken::new();
        let replaced = self.running.lock_ignore_poison().insert(
            proposal_id.to_string(),
            Registered {
                generation,
                token: token.clone(),
            },
        );
        if let Some(replaced) = replaced {
            replaced.token.cancel();
        }
        RunningCheck {
            checks: self,
            proposal_id: proposal_id.to_string(),
            generation,
            tally: CheckTally {
                token,
                checked: Arc::new(AtomicU32::new(0)),
            },
        }
    }

    /// Stop this proposal's check. Answers whether one was running.
    pub fn cancel(&self, proposal_id: &str) -> bool {
        let running = self.running.lock_ignore_poison();
        let Some(check) = running.get(proposal_id) else {
            return false;
        };
        check.token.cancel();
        true
    }
}

/// A registered check. Leaves the registry when dropped, unless a newer one took its place.
pub(super) struct RunningCheck<'a> {
    checks: &'a RenameChecks,
    proposal_id: String,
    generation: u64,
    pub(super) tally: CheckTally,
}

impl Drop for RunningCheck<'_> {
    fn drop(&mut self) {
        let mut running = self.checks.running.lock_ignore_poison();
        if running
            .get(&self.proposal_id)
            .is_some_and(|check| check.generation == self.generation)
        {
            running.remove(&self.proposal_id);
        }
    }
}

/// What the scan shares with the waiter: the count, and whether to go on.
#[derive(Clone, Default)]
pub(super) struct CheckTally {
    token: CancellationToken,
    checked: Arc<AtomicU32>,
}

impl CheckTally {
    /// Call at the top of each row with how many came before it. `false` means stop scanning.
    pub(super) fn begin_row(&self, rows_before: usize) -> bool {
        self.checked
            .store(u32::try_from(rows_before).unwrap_or(u32::MAX), Ordering::Relaxed);
        !self.is_stopped()
    }

    pub(super) fn is_stopped(&self) -> bool {
        self.token.is_cancelled()
    }
}

/// Wait for the scan, reporting its count, until it answers or the check is stopped (`None`).
pub(super) async fn wait_for_scan<T>(
    scan: impl Future<Output = T>,
    tally: &CheckTally,
    mut report: impl FnMut(u32),
) -> Option<T> {
    tokio::pin!(scan);
    let mut ticks = tokio::time::interval_at(tokio::time::Instant::now() + PROGRESS_QUIET, PROGRESS_EVERY);
    let mut last_reported = None;
    loop {
        tokio::select! {
            biased;
            () = tally.token.cancelled() => return None,
            answer = &mut scan => return Some(answer),
            _ = ticks.tick() => {
                let checked = tally.checked.load(Ordering::Relaxed);
                if last_reported != Some(checked) {
                    report(checked);
                    last_reported = Some(checked);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_check_that_ends_inside_the_quiet_period_reports_nothing() {
        let tally = CheckTally::default();
        let mut reports = Vec::new();

        let answer = wait_for_scan(async { 7 }, &tally, |checked| reports.push(checked)).await;

        assert_eq!(answer, Some(7));
        assert!(reports.is_empty(), "a fast recheck must not flicker: {reports:?}");
    }

    /// The case that justifies the waiter's own clock: a first stat stuck on a dead mount has
    /// finished no rows, and the user still needs a count and a way out.
    #[tokio::test]
    async fn a_scan_stuck_on_its_first_row_still_reports_and_still_stops() {
        let checks = RenameChecks::default();
        let running = checks.begin("proposal");
        let tally = running.tally.clone();
        let (first_report, reported) = tokio::sync::oneshot::channel();
        let mut first_report = Some(first_report);

        let waiter = tokio::spawn(async move {
            wait_for_scan(std::future::pending::<()>(), &tally, |checked| {
                if let Some(sender) = first_report.take() {
                    sender.send(checked).expect("the test is still listening");
                }
            })
            .await
        });

        assert_eq!(reported.await.expect("the waiter reports"), 0);
        assert!(checks.cancel("proposal"));
        assert_eq!(waiter.await.expect("the waiter ends"), None);
    }

    #[test]
    fn a_new_check_stops_the_one_it_replaces_and_only_the_newest_unregisters() {
        let checks = RenameChecks::default();
        let older = checks.begin("proposal");
        let newer = checks.begin("proposal");

        assert!(older.tally.is_stopped());
        assert!(!newer.tally.is_stopped());

        // The older one ending must not take the newer one's registration with it.
        drop(older);
        assert!(checks.cancel("proposal"));
        assert!(newer.tally.is_stopped());

        drop(newer);
        assert!(!checks.cancel("proposal"), "nothing left to stop");
    }

    #[test]
    fn stopping_one_proposal_leaves_the_others_running() {
        let checks = RenameChecks::default();
        let first = checks.begin("first");
        let second = checks.begin("second");

        assert!(checks.cancel("first"));

        assert!(first.tally.is_stopped());
        assert!(!second.tally.is_stopped());
    }

    #[test]
    fn a_row_begun_after_the_stop_says_to_stop_scanning() {
        let checks = RenameChecks::default();
        let running = checks.begin("proposal");

        assert!(running.tally.begin_row(0));
        checks.cancel("proposal");
        assert!(!running.tally.begin_row(1));
    }
}
