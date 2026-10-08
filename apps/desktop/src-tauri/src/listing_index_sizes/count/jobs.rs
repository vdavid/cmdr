//! The count queue: one job per listing, fed by every ⌥⇧⏎ and Space on a folder.
//!
//! The first request for a listing becomes the job's OWNER: it runs the queue
//! until it drains. A request that arrives while a job runs appends its folders
//! (skipping ones already queued or being walked) and waits for the job to end,
//! so Space on three folders in a row counts all three, in order, rather than
//! each one abandoning the last. Esc ([`cancel`]) raises its stop. The owner
//! retains publication ownership until its final partial/restored rows land,
//! then releases the listing. Backend workers never publish rows themselves.

use std::collections::{HashMap, HashSet, VecDeque};
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use tokio::time::Instant;

use cmdr_fs::volume::ScanStopSignal;
use tokio::sync::watch;

use super::FolderSizeCountOutcome;
use crate::ignore_poison::IgnorePoison;
use crate::listing_index_sizes::refresh::RowSizes;

/// A folder waiting its turn.
pub(super) struct Queued {
    pub path: String,
    /// What the row showed when it was queued: put back if its walk never runs.
    pub before: RowSizes,
    pub before_manual: bool,
    pub since: Instant,
    /// Whether its hourglass went up, so a cancel knows which rows to put back.
    pub lit: bool,
}

/// One listing's count: its stop, its queue, and the answer its waiters get.
pub(super) struct Job {
    cancelled: AtomicBool,
    inner: Mutex<Inner>,
    done: watch::Sender<Option<FolderSizeCountOutcome>>,
}

#[derive(Default)]
struct Inner {
    queue: VecDeque<Queued>,
    /// The folder being walked now.
    current: Option<String>,
}

impl Job {
    fn new() -> Self {
        Self {
            cancelled: AtomicBool::new(false),
            inner: Mutex::new(Inner::default()),
            done: watch::channel(None).0,
        }
    }

    pub(super) fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }

    pub(super) fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }

    /// Appends the folders not already queued or being walked. Returns how many it took.
    fn append(&self, listing_id: &str, folders: Vec<(String, RowSizes)>, now: Instant) -> usize {
        let mut inner = self.inner.lock_ignore_poison();
        let manual = MANUAL.lock_ignore_poison();
        let mut known: HashSet<String> = inner.queue.iter().map(|q| q.path.clone()).collect();
        known.extend(inner.current.clone());
        let mut took = 0;
        for (path, before) in folders {
            if known.insert(path.clone()) {
                let before_manual = manual.get(listing_id).is_some_and(|paths| paths.contains(&path));
                inner.queue.push_back(Queued {
                    path,
                    before,
                    before_manual,
                    since: now,
                    lit: false,
                });
                took += 1;
            }
        }
        took
    }

    /// The queued folders whose hourglass is due (queued at least `delay` ago), marked lit.
    pub(super) fn due_to_light(&self, now: Instant, delay: std::time::Duration) -> Vec<(String, RowSizes)> {
        let mut inner = self.inner.lock_ignore_poison();
        inner
            .queue
            .iter_mut()
            .filter(|q| !q.lit && now.duration_since(q.since) >= delay)
            .map(|q| {
                q.lit = true;
                (q.path.clone(), q.before)
            })
            .collect()
    }

    /// Everything still waiting, emptied out: what a stopped job puts back.
    pub(super) fn drain(&self) -> Vec<Queued> {
        let mut inner = self.inner.lock_ignore_poison();
        inner.current = None;
        inner.queue.drain(..).collect()
    }

    pub(super) fn finish(&self, outcome: FolderSizeCountOutcome) {
        self.done.send_replace(Some(outcome));
    }
}

impl ScanStopSignal for Job {
    fn is_stopping_or_paused(&self) -> bool {
        self.is_cancelled()
    }

    fn stop_or_park<'a>(&'a self) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        Box::pin(async move { self.is_cancelled() })
    }

    fn stop_or_park_blocking(&self) -> bool {
        self.is_cancelled()
    }
}

/// The job running for each listing.
static JOBS: LazyLock<Mutex<HashMap<String, Arc<Job>>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// What a request became.
pub(super) enum Joined {
    /// No job was running: this request runs the queue.
    Owner(Arc<Job>),
    /// Cancelled, but its owner is still finalizing rows. Retry after it finishes.
    Retiring(watch::Receiver<Option<FolderSizeCountOutcome>>),
    /// A job was running and took the folders (`appended` of them); wait for its end.
    Waiter {
        appended: usize,
        done: watch::Receiver<Option<FolderSizeCountOutcome>>,
    },
}

/// Queues `folders` for `listing_id`: on the running job, or on a new one this
/// caller then runs.
pub(super) fn enqueue(listing_id: &str, folders: Vec<(String, RowSizes)>, now: Instant) -> Joined {
    let mut jobs = JOBS.lock_ignore_poison();
    if let Some(job) = jobs.get(listing_id) {
        if job.is_cancelled() {
            return Joined::Retiring(job.done.subscribe());
        }
        let appended = job.append(listing_id, folders, now);
        return Joined::Waiter {
            appended,
            done: job.done.subscribe(),
        };
    }
    let job = Arc::new(Job::new());
    job.append(listing_id, folders, now);
    jobs.insert(listing_id.to_string(), Arc::clone(&job));
    Joined::Owner(job)
}

/// The next folder for the owner to walk, or `None` when the queue drained, in
/// which case the job leaves the map (under the map lock, so a request racing the
/// end either lands in this queue first or starts a new job).
pub(super) fn next(listing_id: &str, job: &Arc<Job>) -> Option<Queued> {
    let mut jobs = JOBS.lock_ignore_poison();
    let mut inner = job.inner.lock_ignore_poison();
    if let Some(next) = inner.queue.pop_front() {
        inner.current = Some(next.path.clone());
        return Some(next);
    }
    inner.current = None;
    if jobs.get(listing_id).is_some_and(|current| Arc::ptr_eq(current, job)) {
        jobs.remove(listing_id);
    }
    None
}

/// Takes `job` out of the map when it's still the listing's: a job that stopped
/// on its own (its listing closed under it).
pub(super) fn end(listing_id: &str, job: &Arc<Job>) {
    let mut jobs = JOBS.lock_ignore_poison();
    if jobs.get(listing_id).is_some_and(|current| Arc::ptr_eq(current, job)) {
        jobs.remove(listing_id);
    }
}

/// Stops the job running for `listing_id` (Esc, or the listing closing). Reports
/// whether one was running.
pub(crate) fn cancel(listing_id: &str) -> bool {
    match JOBS.lock_ignore_poison().get(listing_id) {
        Some(job) => {
            job.cancel();
            true
        }
        None => false,
    }
}

/// Folders whose shown size came from an on-demand count, per listing. On a
/// volume the drive index covers, the index owns every size it has, so a plan
/// skips any folder with a size... except one of these: a count the user stopped
/// leaves a lower bound that is ours, not the index's, and must stay retryable.
static MANUAL: LazyLock<Mutex<HashMap<String, HashSet<String>>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

pub(super) fn mark_manual(listing_id: &str, path: &str) {
    MANUAL
        .lock_ignore_poison()
        .entry(listing_id.to_string())
        .or_default()
        .insert(path.to_string());
}

pub(super) fn unmark_manual(listing_id: &str, path: &str) {
    if let Some(paths) = MANUAL.lock_ignore_poison().get_mut(listing_id) {
        paths.remove(path);
    }
}

/// The manual folders of `listing_id`, after forgetting listings that closed.
pub(super) fn manual_paths(listing_id: &str, is_open: impl Fn(&str) -> bool) -> HashSet<String> {
    let mut manual = MANUAL.lock_ignore_poison();
    manual.retain(|id, _| is_open(id));
    manual.get(listing_id).cloned().unwrap_or_default()
}
