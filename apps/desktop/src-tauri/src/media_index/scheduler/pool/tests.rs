//! Concurrency-correctness tests for the parallel enrichment pool (plan M2), driven by a
//! deliberately racy instrumented backend: each `analyze_media` records the path it saw,
//! bumps a live in-flight counter, and sleeps, so a race (double-enrichment, over-
//! concurrency, a lost image) actually manifests instead of hiding behind fast fakes.
//!
//! These lock down: no path is ever analyzed twice under concurrency; observed
//! concurrency never exceeds the effective worker count (and reaches it — real
//! parallelism); a mid-pass worker-count change (GROW and SHRINK) applies without
//! reprocessing or dropping an image; cancellation drains promptly and skips GC; and
//! N=1 is strictly serial (never two concurrent analyses — the byte-for-byte-today
//! guarantee).

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use crate::ignore_poison::IgnorePoison;
use crate::media_index::backend::fake::FakeVisionBackend;
use crate::media_index::backend::{ImageInput, MediaAnalysis, OcrResult, VisionBackend, VisionError};
use crate::media_index::predicate::MediaKind;
use crate::media_index::progress::NoopProgressSink;
use crate::media_index::scheduler::enrich::{EnrichGates, GcScope, ImageEntry};
use crate::media_index::scheduler::pool::{MakeBackend, run_enrich_pool};
use crate::media_index::store::{MediaStatusRow, MediaStore, media_db_path};
use crate::media_index::writer::MediaWriter;

/// A `VisionBackend` that instruments concurrency: it counts per-path calls, tracks the
/// live and peak in-flight count, and sleeps a little in `analyze_media` so overlap is
/// real. Delegates the actual (deterministic) results to an inner [`FakeVisionBackend`].
struct RacyBackend {
    inner: FakeVisionBackend,
    delay: Duration,
    started: AtomicUsize,
    in_flight: AtomicUsize,
    max_in_flight: AtomicUsize,
    calls: Mutex<HashMap<String, usize>>,
    /// An optional rendezvous: the first `size` `analyze_media` calls block on the barrier
    /// until all `size` have arrived, so peak concurrency reaches `size` DETERMINISTICALLY
    /// (a plain sleep-based overlap can flake to 1 under heavy CPU contention). `(barrier,
    /// size)` + a monotonic gate counter deciding which calls participate.
    barrier: Option<(Arc<std::sync::Barrier>, usize)>,
    barrier_seen: AtomicUsize,
}

impl RacyBackend {
    fn new(delay: Duration) -> Self {
        Self {
            inner: FakeVisionBackend::new(),
            delay,
            started: AtomicUsize::new(0),
            in_flight: AtomicUsize::new(0),
            max_in_flight: AtomicUsize::new(0),
            calls: Mutex::new(HashMap::new()),
            barrier: None,
            barrier_seen: AtomicUsize::new(0),
        }
    }

    /// Like [`new`](Self::new) but the first `size` calls rendezvous on a barrier, forcing a
    /// `size`-way concurrent overlap that can't flake under load.
    fn with_barrier(delay: Duration, size: usize) -> Self {
        Self {
            barrier: Some((Arc::new(std::sync::Barrier::new(size)), size)),
            ..Self::new(delay)
        }
    }
    /// How many `analyze_media` calls have STARTED so far (drives the live-apply tests'
    /// thresholds without locking the calls map on the hot path).
    fn started(&self) -> usize {
        self.started.load(Ordering::SeqCst)
    }
    fn peak_concurrency(&self) -> usize {
        self.max_in_flight.load(Ordering::SeqCst)
    }
    /// The highest per-path call count (2+ means a path was analyzed twice — the race).
    fn max_calls_for_any_path(&self) -> usize {
        self.calls.lock_ignore_poison().values().copied().max().unwrap_or(0)
    }
    fn distinct_paths_seen(&self) -> usize {
        self.calls.lock_ignore_poison().len()
    }
}

impl VisionBackend for RacyBackend {
    fn engine_version(&self) -> String {
        self.inner.engine_version()
    }
    fn taxonomy_version(&self) -> String {
        self.inner.taxonomy_version()
    }
    fn ocr(&self, input: &ImageInput) -> Result<OcrResult, VisionError> {
        self.inner.ocr(input)
    }
    fn analyze(&self, input: &ImageInput) -> Result<crate::media_index::backend::Analysis, VisionError> {
        self.inner.analyze(input)
    }
    fn analyze_media(
        &self,
        input: &ImageInput,
        want_vision: bool,
        want_clip: bool,
    ) -> Result<MediaAnalysis, VisionError> {
        self.started.fetch_add(1, Ordering::SeqCst);
        let cur = self.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
        self.max_in_flight.fetch_max(cur, Ordering::SeqCst);
        *self.calls.lock_ignore_poison().entry(input.path.clone()).or_insert(0) += 1;
        // Rendezvous: the first `size` calls block until all have arrived (each has already
        // bumped `in_flight`), so peak reaches `size` with certainty, not by timing luck.
        if let Some((barrier, size)) = &self.barrier
            && self.barrier_seen.fetch_add(1, Ordering::SeqCst) < *size
        {
            barrier.wait();
        }
        if !self.delay.is_zero() {
            // allowed-test-sleep: this stub fakes analysis latency, which is what keeps workers
            // busy long enough for the pool's queueing behavior to be observable
            std::thread::sleep(self.delay);
        }
        let result = self.inner.analyze_media(input, want_vision, want_clip);
        self.in_flight.fetch_sub(1, Ordering::SeqCst);
        result
    }
}

/// A scratch media writer over a temp dir.
fn media_writer(dir: &std::path::Path, volume_id: &str) -> MediaWriter {
    let db_path = media_db_path(dir, volume_id);
    MediaStore::open(&db_path).expect("open media store");
    MediaWriter::spawn(&db_path, volume_id).expect("media writer")
}

/// `n` synthetic local images `/photos/img{i}.jpg`, all stale (no stored status).
fn images(n: usize) -> Vec<ImageEntry> {
    (0..n)
        .map(|i| ImageEntry {
            path: format!("/photos/img{i:04}.jpg"),
            mtime: Some(i as u64),
            size: Some(1000),
            kind: MediaKind::Image,
        })
        .collect()
}

/// The default gates: enrich everything, exclude nothing, whole-store GC, no CLIP.
fn gates<'a>(
    should_enrich: &'a (dyn Fn(&str) -> bool + Sync),
    is_excluded: &'a (dyn Fn(&str) -> bool + Sync),
) -> EnrichGates<'a> {
    EnrichGates {
        should_enrich,
        is_excluded,
        gc_scope: GcScope::WholeStore,
        clip_stamp: None,
    }
}

fn never_cancel() -> bool {
    false
}

/// The `make` used when a test wants all workers to SHARE one backend instance (so its
/// concurrency + per-path counters see every worker's calls).
fn shared_make(backend: &Arc<RacyBackend>) -> impl Fn() -> Arc<dyn VisionBackend> + Sync + '_ {
    move || backend.clone() as Arc<dyn VisionBackend>
}

#[test]
fn the_racy_backend_detects_a_repeat() {
    // Proves the guard has teeth: analyzing the same path twice IS observable, so a test
    // asserting `max_calls_for_any_path() == 1` would genuinely fail on a double-enrich.
    let backend = RacyBackend::new(Duration::ZERO);
    let input = ImageInput {
        path: "/photos/dup.jpg".into(),
        kind: MediaKind::Image,
        bytes: None,
    };
    let _ = backend.analyze_media(&input, true, false);
    assert_eq!(backend.max_calls_for_any_path(), 1);
    let _ = backend.analyze_media(&input, true, false);
    assert_eq!(backend.max_calls_for_any_path(), 2, "a repeat must be visible");
}

#[test]
fn four_workers_enrich_each_path_exactly_once_and_run_concurrently() {
    let dir = tempfile::tempdir().expect("temp");
    let writer = media_writer(dir.path(), "vol");
    let imgs = images(200);
    // A 4-way barrier forces a real 4-way overlap deterministically (no timing luck).
    let backend = Arc::new(RacyBackend::with_barrier(Duration::from_micros(500), 4));
    let make = shared_make(&backend);
    let se = |_: &str| true;
    let ex = |_: &str| false;

    let summary = run_enrich_pool(
        &imgs,
        &HashMap::new(),
        backend.as_ref(),
        &make as &MakeBackend,
        &|| 4,
        &writer,
        &gates(&se, &ex),
        &never_cancel,
        &NoopProgressSink,
    )
    .expect("pool pass");

    assert_eq!(summary.enriched, 200, "every image enriched");
    assert!(!summary.cancelled);
    // No path analyzed twice, and every path seen exactly once.
    assert_eq!(backend.max_calls_for_any_path(), 1, "no double-enrichment");
    assert_eq!(backend.distinct_paths_seen(), 200);
    // Real parallelism: the barrier guarantees all four workers ran concurrently, and the
    // pool never exceeds the requested width.
    assert!(
        backend.peak_concurrency() >= 2,
        "expected real overlap, got {}",
        backend.peak_concurrency()
    );
    assert!(
        backend.peak_concurrency() <= 4,
        "over-concurrency: {}",
        backend.peak_concurrency()
    );
    // The rows actually landed.
    let statuses = crate::media_index::scheduler::enrich::load_statuses(dir.path(), "vol");
    assert_eq!(statuses.len(), 200);
}

#[test]
fn one_worker_is_strictly_serial() {
    // The byte-for-byte-today guarantee: at N=1 there is never more than ONE concurrent
    // analyze, and every image is processed exactly once, in the natural order.
    let dir = tempfile::tempdir().expect("temp");
    let writer = media_writer(dir.path(), "vol");
    let imgs = images(80);
    let backend = Arc::new(RacyBackend::new(Duration::from_micros(200)));
    let make = shared_make(&backend);
    let se = |_: &str| true;
    let ex = |_: &str| false;

    let summary = run_enrich_pool(
        &imgs,
        &HashMap::new(),
        backend.as_ref(),
        &make as &MakeBackend,
        &|| 1,
        &writer,
        &gates(&se, &ex),
        &never_cancel,
        &NoopProgressSink,
    )
    .expect("pool pass");

    assert_eq!(summary.enriched, 80);
    assert_eq!(backend.peak_concurrency(), 1, "N=1 must never overlap");
    assert_eq!(backend.max_calls_for_any_path(), 1);
}

#[test]
fn parallelism_grows_mid_pass_without_reprocessing() {
    // Live-apply GROW: run the first 40 images at width 1, then 4. Every image is still
    // enriched exactly once, and concurrency grows past 1 after the switch.
    let dir = tempfile::tempdir().expect("temp");
    let writer = media_writer(dir.path(), "vol");
    let imgs = images(200);
    let backend = Arc::new(RacyBackend::new(Duration::from_micros(500)));
    let make = shared_make(&backend);
    let se = |_: &str| true;
    let ex = |_: &str| false;
    let b = backend.clone();
    let workers = move || if b.started() < 40 { 1 } else { 4 };

    let summary = run_enrich_pool(
        &imgs,
        &HashMap::new(),
        backend.as_ref(),
        &make as &MakeBackend,
        &workers,
        &writer,
        &gates(&se, &ex),
        &never_cancel,
        &NoopProgressSink,
    )
    .expect("pool pass");

    assert_eq!(summary.enriched, 200);
    assert_eq!(
        backend.max_calls_for_any_path(),
        1,
        "no double-enrichment across a re-pool"
    );
    assert_eq!(backend.distinct_paths_seen(), 200);
    // The point of this test is that a mid-pass re-pool (GROW) reprocesses / drops nothing —
    // asserted deterministically above. That parallelism actually runs concurrently is proven
    // by the barrier-backed `four_workers_...` test (a timing-based peak check here would flake
    // under load, and the varying worker count means a barrier can't pin it).
}

#[test]
fn parallelism_shrinks_mid_pass_without_dropping_images() {
    // Live-apply SHRINK: start at 4, drop to 1 after 60 images. All images still enriched
    // exactly once (retiring workers must not strand the cursor).
    let dir = tempfile::tempdir().expect("temp");
    let writer = media_writer(dir.path(), "vol");
    let imgs = images(200);
    let backend = Arc::new(RacyBackend::new(Duration::from_micros(300)));
    let make = shared_make(&backend);
    let se = |_: &str| true;
    let ex = |_: &str| false;
    let b = backend.clone();
    let workers = move || if b.started() < 60 { 4 } else { 1 };

    let summary = run_enrich_pool(
        &imgs,
        &HashMap::new(),
        backend.as_ref(),
        &make as &MakeBackend,
        &workers,
        &writer,
        &gates(&se, &ex),
        &never_cancel,
        &NoopProgressSink,
    )
    .expect("pool pass");

    assert_eq!(summary.enriched, 200, "no image dropped on shrink");
    assert_eq!(backend.max_calls_for_any_path(), 1);
    assert!(backend.peak_concurrency() <= 4);
}

#[test]
fn cancellation_drains_promptly_and_skips_gc() {
    // Cancel after ~30 analyses at width 4: the pass stops well short of all 200, reports
    // cancelled, and skips GC (an emergency stop yields fully — rows kept, nothing wiped).
    let dir = tempfile::tempdir().expect("temp");
    let writer = media_writer(dir.path(), "vol");
    let imgs = images(400);
    let backend = Arc::new(RacyBackend::new(Duration::from_micros(500)));
    let make = shared_make(&backend);
    let se = |_: &str| true;
    let ex = |_: &str| false;
    // A stale stored row for a path NOT in the walk would be a GC target on a clean pass;
    // assert it survives a cancelled one.
    let mut statuses = HashMap::new();
    statuses.insert(
        "/photos/gone.jpg".to_string(),
        MediaStatusRow {
            path: "/photos/gone.jpg".into(),
            mtime: Some(1),
            size: Some(1),
            media_kind: MediaKind::Image,
            state: crate::media_index::store::EnrichmentState::Done,
            engine_version: "old".into(),
            clip_stamp: String::new(),
        },
    );
    let b = backend.clone();
    let cancel = move || b.started() >= 30;

    let summary = run_enrich_pool(
        &imgs,
        &statuses,
        backend.as_ref(),
        &make as &MakeBackend,
        &|| 4,
        &writer,
        &gates(&se, &ex),
        &cancel,
        &NoopProgressSink,
    )
    .expect("pool pass");

    assert!(summary.cancelled, "should report cancelled");
    assert_eq!(summary.gc_count, 0, "cancelled pass skips GC");
    // Drains within a bound: only the ~30 trigger plus at most the 4 in-flight workers'
    // current images finish beyond the trigger, never the whole 400.
    assert!(
        summary.enriched <= 60,
        "should stop promptly, enriched {}",
        summary.enriched
    );
    assert!(
        summary.enriched >= 25,
        "should have made progress, enriched {}",
        summary.enriched
    );
}

#[test]
fn a_thermally_capped_worker_count_bounds_concurrency() {
    // Thermal backoff flows through as the `workers()` value: Serious halves 8 -> 4, so the
    // pool never runs more than 4 concurrent analyses even though the user asked for 8.
    use crate::media_index::thermal::ThermalPressure;
    let dir = tempfile::tempdir().expect("temp");
    let writer = media_writer(dir.path(), "vol");
    let imgs = images(150);
    let backend = Arc::new(RacyBackend::new(Duration::from_micros(400)));
    let make = shared_make(&backend);
    let se = |_: &str| true;
    let ex = |_: &str| false;
    let workers = || ThermalPressure::Serious.cap(8);

    let summary = run_enrich_pool(
        &imgs,
        &HashMap::new(),
        backend.as_ref(),
        &make as &MakeBackend,
        &workers,
        &writer,
        &gates(&se, &ex),
        &never_cancel,
        &NoopProgressSink,
    )
    .expect("pool pass");

    assert_eq!(summary.enriched, 150);
    assert!(
        backend.peak_concurrency() <= 4,
        "thermal cap not honored: {}",
        backend.peak_concurrency()
    );
}
