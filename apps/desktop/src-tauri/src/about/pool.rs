//! The card's own pool of 8 MB-stack threads, separate from the context menu's so
//! a card wedged on a dead mount can't starve a right-click.

use std::sync::LazyLock;
use std::time::Duration;

use cmdr_about::gather::Task;

use crate::file_system::framework_pool::{Pool, PoolConfig};

/// A card's jobs run four at a time; the ceiling bounds the threads a mount that
/// stops answering can hold for the process's life. A job past its budget is
/// reported `TimedOut` while its thread may stay lost until the call returns, so
/// it's this ceiling, ❌ not the budget, that bounds the leak.
///
/// ❗ `spare_after` is what keeps one slow lookup from starving the next card: a
/// worker 250 ms into one job (a cold git walk, a receipt lookup, a search on a
/// busy machine) stops counting, so the next card's quick rows get threads of
/// their own instead of queueing behind it and missing their budgets. Most rows
/// answer in a few milliseconds; 250 ms is past every quick one and well inside a
/// row's second. The ceiling still bounds a wedged volume.
pub(super) const POOL: PoolConfig = PoolConfig {
    name: "about_facts",
    target_workers: 4,
    max_workers: 12,
    wedged_after: Duration::from_secs(10),
    spare_after: Some(Duration::from_millis(250)),
};

static ABOUT_POOL: LazyLock<Pool> = LazyLock::new(|| Pool::new(POOL));

/// Runs `task` on the about pool, first saying once per wedge which call a lost
/// worker is stuck in.
///
/// A job names its call with `framework_pool::note_activity(call, kind)`, where
/// `kind` is the fact's kind, ❌ never the path or a value: this line rides error
/// reports.
pub(crate) fn submit(task: Task) {
    let wedged = ABOUT_POOL.wedged_count();
    if wedged > 0 {
        log::warn!(
            target: "about",
            "{wedged} of {} card threads are still inside a call that never answered ({} queued)",
            ABOUT_POOL.worker_count(),
            ABOUT_POOL.queue_len()
        );
    }
    for stuck in ABOUT_POOL.newly_wedged() {
        let call = stuck
            .activity
            .as_ref()
            .map_or("an unnamed call", |activity| activity.call);
        let kind = stuck.activity.as_ref().map_or("", |activity| activity.subject.as_str());
        log::warn!(
            target: "about",
            "{} has been in {call} ({kind}) for {:?}",
            stuck.thread,
            stuck.busy_for
        );
    }
    ABOUT_POOL.submit(task);
}
