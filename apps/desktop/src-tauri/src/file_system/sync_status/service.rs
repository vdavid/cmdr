//! Batching, caching, and cancellation around the per-path sync-status probe.
//!
//! ## The lifetime problem this exists to solve
//!
//! `commands::sync_status` gives the frontend a 2 s answer, but a File Provider XPC
//! call has no deadline of its own and `spawn_blocking` work cannot be cancelled.
//! So the old shape returned an empty map at 2 s while its `std::thread::scope` kept
//! a tokio blocking thread plus ~11 fresh 8 MB-stack OS threads alive until the
//! provider replied, and the frontend's retry started another set. Two rounds were
//! in flight when the 2026-07-31 incident was sampled, 21-23 threads between them.
//!
//! Three things fix it, and all three matter:
//!
//! 1. **The deadline bounds the caller's wait, never the work.** Timing out drops a
//!    waiter, nothing else. The batch keeps running on the [`Pool`], and its answers
//!    land in the [`Cache`], so the frontend's next poll gets them for free instead
//!    of re-asking the provider.
//! 2. **One batch in flight at a time.** A second request either joins the running
//!    batch (same paths: the retry case) or supersedes it (different paths: the user
//!    scrolled or navigated). It never starts a parallel fan-out.
//! 3. **A superseded batch is cancelled**, so its not-yet-started paths cost
//!    nothing. Only the paths already inside a synchronous XPC call have to finish;
//!    that is the one thing no cancellation can take back.

use super::SyncStatus;
use super::cache::{Cache, Ttls};
use super::pool::{Pool, PoolConfig};
use cmdr_fs::ignore_poison::IgnorePoison;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::watch;

/// The per-path probe. Injected so the batching, caching, and cancellation
/// behaviour can be tested without a File Provider (and without XPC in a unit test).
pub(super) type Probe = Arc<dyn Fn(&Path) -> SyncStatus + Send + Sync>;

pub(super) struct Service {
    pool: Pool,
    shared: Arc<Shared>,
    inflight: Mutex<Option<Arc<Batch>>>,
    batches_started: AtomicUsize,
}

/// The half of the service a queued job needs, so jobs own an `Arc` instead of
/// borrowing the service.
struct Shared {
    cache: Cache,
    probe: Probe,
}

struct Batch {
    /// What this batch was created to resolve. A later request joins only when
    /// every path it still needs is in here.
    paths: HashSet<String>,
    cancelled: AtomicBool,
    /// Jobs not yet finished. The one that takes it to zero marks the batch done.
    pending: AtomicUsize,
    done: watch::Sender<bool>,
    started: Instant,
}

impl Batch {
    fn new(paths: &[String]) -> Self {
        Self {
            paths: paths.iter().cloned().collect(),
            cancelled: AtomicBool::new(false),
            pending: AtomicUsize::new(paths.len()),
            done: watch::channel(false).0,
            started: Instant::now(),
        }
    }

    fn is_done(&self) -> bool {
        *self.done.borrow()
    }

    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }

    fn covers(&self, paths: &[String]) -> bool {
        paths.iter().all(|path| self.paths.contains(path))
    }

    /// Stops the batch and releases anyone waiting on it. Paths already inside a
    /// synchronous provider call still have to come back on their own.
    fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
        let _ = self.done.send(true);
    }

    fn finish_one(&self) {
        if self.pending.fetch_sub(1, Ordering::SeqCst) == 1 {
            let _ = self.done.send(true);
        }
    }

    async fn wait(&self) {
        let mut done = self.done.subscribe();
        while !*done.borrow_and_update() {
            if done.changed().await.is_err() {
                break;
            }
        }
    }
}

impl Service {
    pub(super) fn new(probe: Probe, pool: PoolConfig, cache_capacity: usize, ttls: Ttls) -> Self {
        Self {
            pool: Pool::new(pool),
            shared: Arc::new(Shared {
                cache: Cache::new(cache_capacity, ttls),
                probe,
            }),
            inflight: Mutex::new(None),
            batches_started: AtomicUsize::new(0),
        }
    }

    /// Every path's status, waiting at most `deadline` for the ones that aren't
    /// cached. The bool is "we ran out of time", so the frontend can tell a slow
    /// provider from a folder with no cloud files in it.
    pub(super) async fn statuses_within(
        &self,
        paths: Vec<String>,
        deadline: Duration,
    ) -> (HashMap<String, SyncStatus>, bool) {
        let requested = paths.len();
        let mut resolved = HashMap::with_capacity(requested);
        let mut misses = Vec::new();
        for path in paths {
            match self.shared.cache.get(Path::new(&path)) {
                Some(status) => {
                    resolved.insert(path, status);
                }
                None => misses.push(path),
            }
        }

        if misses.is_empty() {
            // allowed-pluralize-noun: diagnostic log line; a one-path batch reading "1 paths" costs a reader nothing.
            log::debug!(target: "sync_status", "{requested} paths, all cached");
            return (resolved, false);
        }

        let batch = self.join_or_start(&misses);
        let ran_out_of_time = tokio::time::timeout(deadline, batch.wait()).await.is_err();

        for path in misses {
            if let Some(status) = self.shared.cache.get(Path::new(&path)) {
                resolved.insert(path, status);
            }
        }

        let missing = requested - resolved.len();
        if ran_out_of_time {
            // The batch is still running on the pool, and its answers will be
            // cached by the time the pane asks again. This warning is the trace
            // the incident's 45 MB log did not have.
            log::warn!(
                target: "sync_status",
                // allowed-pluralize-noun: diagnostic log line; the counts are what matter, not their grammar.
                "{requested} paths: gave up waiting after {deadline:?} with {missing} unanswered; \
                 the batch keeps running on {} pool threads and its results will be cached",
                self.pool.worker_count()
            );
        } else {
            log::debug!(
                target: "sync_status",
                // allowed-pluralize-noun: diagnostic log line; the counts are what matter, not their grammar.
                "{requested} paths resolved in {:?} ({missing} unknown to the provider)",
                batch.started.elapsed()
            );
        }

        (resolved, ran_out_of_time || batch.is_cancelled())
    }

    /// One path, for callers that have no async context (the native context menu).
    /// Bounded by `deadline` the same way, and the answer still populates the cache
    /// even when the wait expires first.
    pub(super) fn status_within_blocking(&self, path: &str, deadline: Duration) -> SyncStatus {
        if let Some(status) = self.shared.cache.get(Path::new(path)) {
            return status;
        }
        let (answer_tx, answer_rx) = std::sync::mpsc::channel();
        let shared = Arc::clone(&self.shared);
        let path = path.to_string();
        self.pool.submit(Box::new(move || {
            let status = shared.probe_and_cache(&path);
            let _ = answer_tx.send(status);
        }));
        answer_rx.recv_timeout(deadline).unwrap_or_default()
    }

    pub(super) fn invalidate_dir(&self, dir: &Path) {
        self.shared.cache.invalidate_dir(dir);
    }

    pub(super) fn invalidate_path(&self, path: &Path) {
        self.shared.cache.invalidate_path(path);
    }

    /// How many fan-outs this service has ever started. The whole point of M4.2 is
    /// that this stays put while the frontend re-asks for the same paths.
    /// Threads the probe pool holds. The bench reports it; the incident's `sample`
    /// runs are what it is compared against.
    #[cfg(test)]
    pub(super) fn pool_worker_count(&self) -> usize {
        self.pool.worker_count()
    }

    #[cfg(test)]
    fn batches_started(&self) -> usize {
        self.batches_started.load(Ordering::SeqCst)
    }

    /// Joins the running batch when it already covers `misses`, otherwise cancels
    /// it and starts one. Exactly one batch is ever in flight.
    fn join_or_start(&self, misses: &[String]) -> Arc<Batch> {
        let mut inflight = self.inflight.lock_ignore_poison();

        if let Some(current) = inflight.as_ref()
            && !current.is_done()
            && current.covers(misses)
        {
            log::debug!(
                target: "sync_status",
                "joining the batch already in flight for {} of these paths",
                misses.len()
            );
            return Arc::clone(current);
        }

        if let Some(previous) = inflight.take() {
            if !previous.is_done() {
                log::debug!(
                    target: "sync_status",
                    "superseding an in-flight batch of {} paths after {:?}",
                    previous.paths.len(),
                    previous.started.elapsed()
                );
            }
            previous.cancel();
        }

        let batch = Arc::new(Batch::new(misses));
        self.batches_started.fetch_add(1, Ordering::SeqCst);
        let wedged = self.pool.wedged_count();
        if wedged > 0 {
            log::warn!(
                target: "sync_status",
                "{wedged} of {} pool threads are still inside a File Provider call that never answered",
                self.pool.worker_count()
            );
        }
        log::debug!(
            target: "sync_status",
            "starting a batch of {} paths ({} pool threads, {} queued)",
            misses.len(),
            self.pool.worker_count(),
            self.pool.queue_len()
        );

        for path in misses {
            let shared = Arc::clone(&self.shared);
            let batch = Arc::clone(&batch);
            let path = path.clone();
            self.pool.submit(Box::new(move || {
                // The cancellation check that makes an abandoned batch free: a
                // superseded path never reaches the provider at all.
                if !batch.is_cancelled() {
                    shared.probe_and_cache(&path);
                }
                batch.finish_one();
            }));
        }

        *inflight = Some(Arc::clone(&batch));
        batch
    }
}

impl Shared {
    /// Probes one path and caches the answer.
    fn probe_and_cache(&self, path: &str) -> SyncStatus {
        let status = (self.probe)(Path::new(path));
        self.cache.put(Path::new(path), status);
        status
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::wait_until_async;
    use std::sync::mpsc;

    const WAIT: Duration = Duration::from_secs(5);

    const POOL: PoolConfig = PoolConfig {
        name: "test-sync-status",
        target_workers: 4,
        max_workers: 8,
        wedged_after: Duration::from_secs(30),
    };

    const TTLS: Ttls = Ttls {
        stable: Duration::from_secs(60),
        transitional: Duration::from_secs(2),
    };

    /// A probe that counts how many paths actually reached it, and can be held
    /// open so a test controls when it answers.
    struct FakeProbe {
        probed: Arc<Mutex<Vec<String>>>,
        gate: Option<Arc<Mutex<mpsc::Receiver<()>>>>,
    }

    impl FakeProbe {
        fn instant() -> (Probe, Arc<Mutex<Vec<String>>>) {
            let probed = Arc::new(Mutex::new(Vec::new()));
            let fake = FakeProbe {
                probed: Arc::clone(&probed),
                gate: None,
            };
            (Arc::new(move |path: &Path| fake.call(path)), probed)
        }

        /// The probe blocks until the returned sender is dropped: our stand-in for
        /// a provider that hasn't replied yet.
        fn held() -> (Probe, Arc<Mutex<Vec<String>>>, mpsc::Sender<()>) {
            let probed = Arc::new(Mutex::new(Vec::new()));
            let (release, gate) = mpsc::channel();
            let fake = FakeProbe {
                probed: Arc::clone(&probed),
                gate: Some(Arc::new(Mutex::new(gate))),
            };
            (Arc::new(move |path: &Path| fake.call(path)), probed, release)
        }

        fn call(&self, path: &Path) -> SyncStatus {
            self.probed
                .lock_ignore_poison()
                .push(path.to_string_lossy().into_owned());
            if let Some(gate) = &self.gate {
                let _ = gate.lock_ignore_poison().recv();
            }
            SyncStatus::Synced
        }
    }

    fn paths(dir: &str, count: usize) -> Vec<String> {
        (0..count).map(|n| format!("{dir}/file{n}.txt")).collect()
    }

    fn probed_count(probed: &Arc<Mutex<Vec<String>>>) -> usize {
        probed.lock_ignore_poison().len()
    }

    fn probed_under(probed: &Arc<Mutex<Vec<String>>>, dir: &str) -> usize {
        probed
            .lock_ignore_poison()
            .iter()
            .filter(|path| path.starts_with(dir))
            .count()
    }

    #[tokio::test]
    async fn answers_every_requested_path() {
        let (probe, probed) = FakeProbe::instant();
        let service = Service::new(probe, POOL, 1024, TTLS);

        let (statuses, timed_out) = service.statuses_within(paths("/cloud", 20), WAIT).await;

        assert!(!timed_out);
        assert_eq!(statuses.len(), 20);
        assert_eq!(probed_count(&probed), 20);
    }

    /// M4.4: a second identical request costs no provider calls at all. Pre-fix,
    /// the 3 s idle poll re-queried every visible path forever.
    #[tokio::test]
    async fn a_repeat_request_is_served_from_cache() {
        let (probe, probed) = FakeProbe::instant();
        let service = Service::new(probe, POOL, 1024, TTLS);

        let requested = paths("/cloud", 30);
        service.statuses_within(requested.clone(), WAIT).await;
        assert_eq!(probed_count(&probed), 30);

        let (statuses, timed_out) = service.statuses_within(requested, WAIT).await;
        assert!(!timed_out);
        assert_eq!(statuses.len(), 30);
        assert_eq!(probed_count(&probed), 30, "the second round asked the provider nothing");
    }

    /// M4.2: a second ask for paths already in flight joins that batch instead of
    /// starting a parallel fan-out. This is the frontend's retry-after-timeout,
    /// which is how the incident had two rounds of threads live at once.
    #[tokio::test]
    async fn a_second_request_for_in_flight_paths_joins_instead_of_fanning_out() {
        let (probe, probed, release) = FakeProbe::held();
        let service = Service::new(probe, POOL, 1024, TTLS);
        let requested = paths("/cloud", POOL.target_workers);

        // The held probe keeps the batch in flight past the deadline.
        let (_, timed_out) = service
            .statuses_within(requested.clone(), Duration::from_millis(50))
            .await;
        assert!(timed_out);
        assert_eq!(service.batches_started(), 1);
        wait_until_async(WAIT, "the batch reached the provider", || probed_count(&probed) > 0).await;

        let (_, timed_out) = service
            .statuses_within(requested.clone(), Duration::from_millis(50))
            .await;
        assert!(timed_out);
        assert_eq!(
            service.batches_started(),
            1,
            "the retry joined the running batch rather than starting a second fan-out"
        );

        drop(release);
        wait_until_async(WAIT, "the batch finished", || {
            probed_count(&probed) == POOL.target_workers
        })
        .await;
        let (statuses, timed_out) = service.statuses_within(requested, WAIT).await;
        assert!(!timed_out);
        assert_eq!(statuses.len(), POOL.target_workers);
        assert_eq!(
            probed_count(&probed),
            POOL.target_workers,
            "one round of provider calls served all three asks"
        );
    }

    /// M4.1: a caller that gave up doesn't hold anything, and the work it started
    /// still lands in the cache, so the next poll is free rather than a fresh
    /// fan-out into the same unresponsive provider.
    #[tokio::test]
    async fn a_timed_out_request_keeps_its_results() {
        let (probe, probed, release) = FakeProbe::held();
        let service = Service::new(probe, POOL, 1024, TTLS);

        let requested = paths("/cloud", 2);
        let (statuses, timed_out) = service
            .statuses_within(requested.clone(), Duration::from_millis(50))
            .await;
        assert!(timed_out, "the held provider outlived the deadline");
        assert!(statuses.is_empty());

        drop(release);
        wait_until_async(WAIT, "the abandoned batch finished anyway", || {
            probed_count(&probed) == 2
        })
        .await;

        let (statuses, timed_out) = service.statuses_within(requested, WAIT).await;
        assert!(!timed_out);
        assert_eq!(statuses.len(), 2);
        assert_eq!(
            probed_count(&probed),
            2,
            "the retry reused the abandoned batch's answers instead of re-asking"
        );
    }

    /// M4.1: the user scrolls, so the paths still queued for the old visible range
    /// must never reach the provider.
    #[tokio::test]
    async fn superseding_a_batch_stops_its_queued_paths() {
        let (probe, probed, release) = FakeProbe::held();
        let service = Service::new(probe, POOL, 1024, TTLS);

        // Far more paths than the pool has workers, so most of them stay queued.
        let (_, timed_out) = service
            .statuses_within(paths("/old", 200), Duration::from_millis(50))
            .await;
        assert!(timed_out);
        wait_until_async(WAIT, "the first batch reached the provider", || {
            probed_count(&probed) > 0
        })
        .await;

        // A different visible range supersedes it. `join_or_start` runs before the
        // first await, so the old batch is already cancelled here.
        let wanted = paths("/new", 3);
        let second = service.statuses_within(wanted, Duration::from_millis(50));
        let (_, timed_out) = second.await;
        assert!(timed_out, "the workers are still held by the old batch");
        assert_eq!(service.batches_started(), 2);

        drop(release);
        wait_until_async(WAIT, "the new visible range was probed", || {
            probed_under(&probed, "/new/") == 3
        })
        .await;

        let abandoned = probed_under(&probed, "/old/");
        assert!(
            abandoned <= POOL.max_workers,
            "{abandoned} of 200 superseded paths reached the provider; only the ones already inside \
             a call should have (at most {} workers)",
            POOL.max_workers
        );
    }

    /// The blocking entry point the native context menu uses is bounded too, and it
    /// reads the cache the batch path fills.
    #[tokio::test(flavor = "multi_thread")]
    async fn the_blocking_entry_point_honours_its_deadline() {
        let (probe, probed, release) = FakeProbe::held();
        let service = Service::new(probe, POOL, 1024, TTLS);

        let status = service.status_within_blocking("/cloud/a.txt", Duration::from_millis(50));
        assert_eq!(
            status,
            SyncStatus::Unknown,
            "an unanswered probe falls back, it doesn't hang"
        );

        drop(release);
        wait_until_async(WAIT, "the probe finished behind the deadline", || {
            probed_count(&probed) == 1
        })
        .await;
        assert_eq!(
            service.status_within_blocking("/cloud/a.txt", Duration::from_millis(50)),
            SyncStatus::Synced,
            "the answer it waited for is cached for the next ask"
        );
    }

    /// Invalidation reaches the provider again rather than serving the old answer.
    #[tokio::test]
    async fn invalidation_forces_a_re_probe() {
        let (probe, probed) = FakeProbe::instant();
        let service = Service::new(probe, POOL, 1024, TTLS);

        let requested = paths("/cloud", 3);
        service.statuses_within(requested.clone(), WAIT).await;
        assert_eq!(probed_count(&probed), 3);

        service.invalidate_dir(Path::new("/cloud"));
        service.statuses_within(requested.clone(), WAIT).await;
        assert_eq!(probed_count(&probed), 6, "the whole directory was re-probed");

        service.invalidate_path(Path::new("/cloud/file1.txt"));
        service.statuses_within(requested, WAIT).await;
        assert_eq!(probed_count(&probed), 7, "only the invalidated path was re-probed");
    }
}
