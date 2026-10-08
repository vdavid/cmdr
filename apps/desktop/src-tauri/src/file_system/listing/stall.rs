//! Listing reads that stop answering: the stall watch, the background retries,
//! and the per-volume bound on reads stuck in the kernel.
//!
//! A read on a network mount whose server went silent blocks in the kernel until
//! the server answers again, holding a blocking-pool thread the whole time. This
//! module turns that silence into a state the pane can show (`listing-stalled`),
//! keeps the listing alive until the volume answers, and caps how many such
//! threads one volume can pin. Why each rule is what it is: `DETAILS.md` §
//! "Stalled listings".

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::Duration;

use futures_util::stream::{FuturesUnordered, StreamExt};
use tokio::sync::Notify;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

use crate::file_system::listing::foreign_path::{Listed, list_as_stored};
use crate::file_system::listing::stalled_on::StalledOn;
use crate::file_system::volume::friendly_error::{ErrorCategory, listing_error_from_volume_error};
use crate::file_system::volume::{BackendKind, ListingProgress, Volume, VolumeError};
use crate::ignore_poison::IgnorePoison;

/// When a listing counts as stalled, and how it retries once it has.
#[derive(Debug, Clone, Copy)]
pub(crate) struct StallPolicy {
    /// How long a read may go without a new entry before the listing reports it stalled.
    pub stall_after: Duration,
    /// The wait before the first background attempt, doubling per attempt.
    pub first_backoff: Duration,
    /// The longest wait between two background attempts.
    pub max_backoff: Duration,
}

impl StallPolicy {
    /// 8 s clears the 0.3–6 s a busy NAS holds a single request for (`listing_done_level`),
    /// so a slow answer never reads as a dead server.
    pub(crate) const PRODUCTION: StallPolicy = StallPolicy {
        stall_after: Duration::from_secs(8),
        first_backoff: Duration::from_secs(2),
        max_backoff: Duration::from_secs(30),
    };
}

/// How many listing reads may be in flight at once on a volume that has a hung one.
///
/// Each one can pin a blocking thread until the kernel lets go. Four leaves room for
/// a stalled listing's two reads plus a user's Retry while the old ones still hang.
pub(crate) const MAX_READS_ON_A_HUNG_VOLUME: usize = 4;

/// How many reads one listing keeps in flight once it has stalled, on a volume
/// whose reads block in the kernel: the stuck one, plus one fresh probe that can
/// answer the moment the server is back, even if the stuck one stays stuck.
const MAX_READS_PER_STALLED_LISTING: usize = 2;

// ============================================================================
// The per-volume registry of reads
// ============================================================================

// DEFAULT-OK: a read that was just filed is neither through the gate nor hung,
// which is exactly the zero value. It claims nothing about the disk.
#[derive(Clone, Copy, Default)]
struct ReadState {
    /// Let through the gate, so it's (or will be) holding a thread.
    admitted: bool,
    /// Its listing saw it go `stall_after` without a new entry.
    hung: bool,
}

/// Every volume's listing reads, and which of them are hung.
///
/// One per process in production (`StallWatch::production`); a test makes its own.
pub(crate) struct HungReads {
    volumes: Mutex<HashMap<String, HashMap<u64, ReadState>>>,
    /// Pinged whenever a read leaves or stops being hung, which can open the gate.
    loosened: Notify,
    next_id: AtomicU64,
}

impl HungReads {
    pub(crate) fn new() -> Self {
        Self {
            volumes: Mutex::new(HashMap::new()),
            loosened: Notify::new(),
            next_id: AtomicU64::new(0),
        }
    }

    /// Files a read that hasn't started yet. Its slot goes when the returned guard drops.
    fn register(self: &Arc<Self>, volume_id: &str) -> ReadSlot {
        let read_id = self.next_id.fetch_add(1, Ordering::Relaxed);
        self.volumes
            .lock_ignore_poison()
            .entry(volume_id.to_string())
            .or_default()
            .insert(read_id, ReadState::default());
        ReadSlot {
            reads: Arc::clone(self),
            volume_id: volume_id.to_string(),
            read_id,
        }
    }

    /// Waits until `slot`'s read may start: at once on a volume with no hung read,
    /// else once fewer than `MAX_READS_ON_A_HUNG_VOLUME` are in flight. Waits as a
    /// future, never a thread. `false` when `cancel` fired first.
    async fn admit(&self, slot: &ReadSlot, cancel: &CancellationToken) -> bool {
        loop {
            // Armed BEFORE the check, so a release between the check and the await
            // still wakes us.
            let loosened = self.loosened.notified();
            tokio::pin!(loosened);
            loosened.as_mut().enable();
            {
                let mut volumes = self.volumes.lock_ignore_poison();
                let Some(reads) = volumes.get_mut(&slot.volume_id) else {
                    return false;
                };
                let any_hung = reads.values().any(|r| r.admitted && r.hung);
                let admitted = reads.values().filter(|r| r.admitted).count();
                if !any_hung || admitted < MAX_READS_ON_A_HUNG_VOLUME {
                    if let Some(read) = reads.get_mut(&slot.read_id) {
                        read.admitted = true;
                    }
                    return true;
                }
            }
            tokio::select! {
                () = cancel.cancelled() => return false,
                () = &mut loosened => {}
            }
        }
    }

    fn set_hung(&self, volume_id: &str, read_id: u64, hung: bool) {
        let changed = {
            let mut volumes = self.volumes.lock_ignore_poison();
            match volumes.get_mut(volume_id).and_then(|reads| reads.get_mut(&read_id)) {
                Some(read) if read.hung != hung => {
                    read.hung = hung;
                    true
                }
                _ => false,
            }
        };
        if changed && !hung {
            self.loosened.notify_waiters();
        }
    }

    fn release(&self, volume_id: &str, read_id: u64) {
        {
            let mut volumes = self.volumes.lock_ignore_poison();
            if let Some(reads) = volumes.get_mut(volume_id) {
                reads.remove(&read_id);
                if reads.is_empty() {
                    volumes.remove(volume_id);
                }
            }
        }
        self.loosened.notify_waiters();
    }
}

/// A read's place in `HungReads`. Lives in the read's task, so the slot frees when
/// the kernel lets go of the read, not when the listing that asked moves on.
struct ReadSlot {
    reads: Arc<HungReads>,
    volume_id: String,
    read_id: u64,
}

impl Drop for ReadSlot {
    fn drop(&mut self) {
        self.reads.release(&self.volume_id, self.read_id);
    }
}

static PRODUCTION_READS: LazyLock<Arc<HungReads>> = LazyLock::new(|| Arc::new(HungReads::new()));

/// What a listing needs to watch its reads: the policy and the shared registry.
#[derive(Clone)]
pub(crate) struct StallWatch {
    pub policy: StallPolicy,
    pub reads: Arc<HungReads>,
}

impl StallWatch {
    pub(crate) fn production() -> Self {
        Self {
            policy: StallPolicy::PRODUCTION,
            reads: Arc::clone(&PRODUCTION_READS),
        }
    }
}

// ============================================================================
// The listing loop
// ============================================================================

/// How a listing's read ended.
pub(crate) enum ReadOutcome {
    Listed(Listed),
    Refused(VolumeError),
    Cancelled,
}

/// The most entries any of a listing's reads has reported, and when that last grew.
struct Beat {
    best: AtomicUsize,
    last_moved: Mutex<Instant>,
    moved: Notify,
}

impl Beat {
    fn new() -> Self {
        Self {
            best: AtomicUsize::new(0),
            last_moved: Mutex::new(Instant::now()),
            moved: Notify::new(),
        }
    }

    /// A count that only repeats isn't movement: a local read stuck on one `stat`
    /// reports the same number every tick.
    fn report(&self, entries: usize) {
        if entries > self.best.fetch_max(entries, Ordering::Relaxed) {
            *self.last_moved.lock_ignore_poison() = Instant::now();
            self.moved.notify_one();
        }
    }

    fn last_moved(&self) -> Instant {
        *self.last_moved.lock_ignore_poison()
    }
}

struct Backoff {
    next: Duration,
    max: Duration,
}

impl Backoff {
    fn new(policy: &StallPolicy) -> Self {
        Self {
            next: policy.first_backoff,
            max: policy.max_backoff,
        }
    }

    fn step(&mut self) -> Duration {
        let wait = self.next;
        self.next = (self.next * 2).min(self.max);
        wait
    }
}

struct Attempt {
    started: Instant,
    hung: bool,
}

type AttemptFuture = Pin<Box<dyn Future<Output = (u64, Result<Listed, VolumeError>)> + Send>>;

/// Who's asking, and where to report: what every attempt of one listing shares.
///
/// Reports go through plain callbacks, ❌ never `streaming::ListingEventSink`: the
/// streaming listing calls in here, so naming its sink would weld the two modules
/// into a cycle (`module-cycles`).
pub(crate) struct ListingRead<'a> {
    /// The read went `stall_after` without a new entry, with what the folder lives on.
    pub on_stalled: &'a (dyn Fn(StalledOn) + Sync),
    /// A read's running entry count, for the pane's "Loaded N files…" line.
    pub on_progress: Arc<dyn Fn(usize) + Send + Sync>,
    pub listing_id: &'a str,
    pub volume_id: &'a str,
    pub volume: &'a Arc<dyn Volume>,
    pub path: &'a Path,
    pub cancel: &'a CancellationToken,
}

/// Reads `path` until the volume answers, reporting a stall the moment the read
/// goes `stall_after` without a new entry.
///
/// Until the first stall this is one read, raced against `cancel`, exactly as
/// before. After it, the listing waits on: a stuck read's refusal is stale (it
/// spent the outage in the kernel), so it asks again at once; a prompt transient
/// refusal is retried with backoff; a prompt lasting refusal is the answer. On a
/// volume whose reads block in the kernel, one fresh probe runs beside the stuck
/// read, since the stuck one can stay stuck after the server is back.
///
/// ❌ Never aborts a read: a cancel or a winning sibling detaches it, and it runs
/// to its own end (an MTP transaction dropped mid-flight wedges the phone).
pub(crate) async fn read_until_answered(read: ListingRead<'_>, watch: &StallWatch) -> ReadOutcome {
    let policy = watch.policy;
    let beat = Arc::new(Beat::new());
    let probes = read.volume.backend_kind() == BackendKind::Local;
    let mut backoff = Backoff::new(&policy);
    let mut in_flight: FuturesUnordered<AttemptFuture> = FuturesUnordered::new();
    let mut attempts: HashMap<u64, Attempt> = HashMap::new();
    let mut stalled = false;
    let mut ever_stalled = false;
    let mut stalled_on: Option<StalledOn> = None;
    let mut next_attempt_at: Option<Instant> = None;
    // Every attempt's stop signal: the listing's own cancel, plus this function
    // returning. A read still waiting at the gate when a sibling answers would
    // otherwise start later for nobody.
    let attempts_cancel = read.cancel.child_token();
    let _stop_attempts_on_return = attempts_cancel.clone().drop_guard();

    let spawn_attempt = |in_flight: &mut FuturesUnordered<AttemptFuture>, attempts: &mut HashMap<u64, Attempt>| {
        let slot = watch.reads.register(read.volume_id);
        attempts.insert(
            slot.read_id,
            Attempt {
                started: Instant::now(),
                hung: false,
            },
        );
        in_flight.push(spawn_read(&read, slot, Arc::clone(&beat), attempts_cancel.clone()));
    };
    spawn_attempt(&mut in_flight, &mut attempts);

    loop {
        let last_moved = beat.last_moved();
        let stall_check = attempts
            .values()
            .filter(|a| !a.hung)
            .map(|a| a.started.max(last_moved) + policy.stall_after)
            .min();
        let max_in_flight = if ever_stalled && probes {
            MAX_READS_PER_STALLED_LISTING
        } else {
            1
        };
        let may_start = in_flight.len() < max_in_flight;

        tokio::select! {
            biased;
            () = read.cancel.cancelled() => return ReadOutcome::Cancelled,
            Some((read_id, result)) = in_flight.next() => {
                let was_hung = attempts.remove(&read_id).is_some_and(|a| a.hung);
                let error = match result {
                    Ok(listed) => return ReadOutcome::Listed(listed),
                    Err(error) => error,
                };
                if !ever_stalled || !(was_hung || is_transient(&error, read.path)) {
                    return ReadOutcome::Refused(error);
                }
                log::debug!(
                    "listing {} on volume {}: a {} read answered {error}, asking again",
                    read.listing_id,
                    read.volume_id,
                    if was_hung { "stuck" } else { "retried" },
                );
                let wait = if was_hung { Duration::ZERO } else { backoff.step() };
                next_attempt_at = Some(next_attempt_at.map_or(Instant::now() + wait, |at| at.min(Instant::now() + wait)));
            }
            () = beat.moved.notified() => {
                // The volume answered: nothing is hung any more, and the pane's
                // progress line has already replaced the stalled notice.
                for (read_id, attempt) in attempts.iter_mut().filter(|(_, a)| a.hung) {
                    attempt.hung = false;
                    watch.reads.set_hung(read.volume_id, *read_id, false);
                }
                stalled = false;
            }
            () = sleep_until_opt(stall_check), if stall_check.is_some() => {
                let now = Instant::now();
                let last_moved = beat.last_moved();
                for (read_id, attempt) in attempts.iter_mut().filter(|(_, a)| !a.hung) {
                    if now >= attempt.started.max(last_moved) + policy.stall_after {
                        attempt.hung = true;
                        watch.reads.set_hung(read.volume_id, *read_id, true);
                    }
                }
                if !stalled && !attempts.is_empty() && attempts.values().all(|a| a.hung) {
                    stalled = true;
                    ever_stalled = true;
                    log::warn!(
                        "listing {} on volume {} stalled: no answer for {:?}, still trying",
                        read.listing_id,
                        read.volume_id,
                        policy.stall_after,
                    );
                    // Classified once per listing, and only once it stalls: the
                    // answer can't change mid-read, and a listing that never
                    // stalls never pays for it.
                    let on = match stalled_on {
                        Some(on) => on,
                        None => {
                            let on = super::stalled_on::stalled_on(read.volume, read.path).await;
                            stalled_on = Some(on);
                            on
                        }
                    };
                    (read.on_stalled)(on);
                    if probes && next_attempt_at.is_none() {
                        next_attempt_at = Some(now + backoff.step());
                    }
                }
            }
            () = sleep_until_opt(next_attempt_at), if next_attempt_at.is_some() && may_start => {
                next_attempt_at = None;
                spawn_attempt(&mut in_flight, &mut attempts);
            }
        }
    }
}

async fn sleep_until_opt(at: Option<Instant>) {
    match at {
        Some(at) => tokio::time::sleep_until(at).await,
        None => std::future::pending().await,
    }
}

/// Whether retrying might help, by the same typed classification the pane renders
/// from (`ErrorCategory::Transient`).
fn is_transient(error: &VolumeError, path: &Path) -> bool {
    listing_error_from_volume_error(error, path).category == ErrorCategory::Transient
}

/// One read, in its own task so a cancel or a sibling's answer detaches it.
fn spawn_read(read: &ListingRead<'_>, slot: ReadSlot, beat: Arc<Beat>, cancel: CancellationToken) -> AttemptFuture {
    let read_id = slot.read_id;
    let volume = Arc::clone(read.volume);
    let path: PathBuf = read.path.to_path_buf();
    let report_progress = Arc::clone(&read.on_progress);
    let listing_id = read.listing_id.to_string();
    let task = tokio::spawn(async move {
        // Stall-probe: marker logged as the FIRST executable line inside the spawned task.
        // If `read_directory_with_progress: entry` fires but this `task started` line doesn't,
        // the tokio runtime didn't schedule this task (worker starvation). If both fire but
        // `list_directory_core` doesn't follow, the Volume's list_directory itself is blocked.
        // Info-level (matches the other `stall_probe::*` lifecycle markers); always lands in
        // the prod file chain for organic-repro triage.
        log::info!(
            target: "stall_probe::listing_task",
            "task started: listing_id={}, path={}",
            listing_id,
            path.display(),
        );
        // The slot rides here, so it frees when THIS read ends, however long after
        // the listing gave up on it.
        let slot = slot;
        if !slot.reads.admit(&slot, &cancel).await {
            return Err(VolumeError::Cancelled(
                "listing cancelled before its read started".to_string(),
            ));
        }
        let on_progress = |p: ListingProgress| {
            // A cancelled listing keeps running until the backend reaches a safe
            // boundary, but its listing_id is spent: the caller already emitted
            // `listing-cancelled` and the pane moved on. Stay quiet so a superseded
            // listing can't post progress against an id the frontend has retired.
            if cancel.is_cancelled() {
                return;
            }
            beat.report(p.entries());
            // Streaming listing UI shows "Loaded N entries…", so it wants total
            // entry count, not just files. `ListingProgress::entries()` sums
            // files + dirs for that.
            report_progress(p.entries());
        };
        // A pane can ask by a spelling its volume doesn't store (typed, restored,
        // carried over from the kernel mount); this lands it on the stored one.
        list_as_stored(volume.as_ref(), &path, Some(&on_progress), Some(&cancel)).await
    });
    Box::pin(async move {
        let result = task.await.unwrap_or_else(|e| {
            Err(VolumeError::IoError {
                message: format!("Directory listing task failed: {}", e),
                raw_os_error: None,
            })
        });
        (read_id, result)
    })
}
