//! Network-enrichment core tests (data-safety + risky-path TDD targets), all
//! over a scripted fake fetcher + fake backend + a real writer — no real mount, no
//! FFI, no clock. They pin the conservative-fetch decision, resumability across an
//! unmount (completed rows survive, no false `Failed`), GC-never-on-disconnect, the
//! "always index" override, and offline search after unmount.

use std::cell::RefCell;
use std::collections::HashMap;
use std::time::Duration;

use crate::media_index::backend::fake::FakeVisionBackend;
use crate::media_index::predicate::MediaKind;
use crate::media_index::progress::NoopProgressSink;
use crate::media_index::read::MediaIndex;
use crate::media_index::scheduler::enrich::ImageEntry;
use crate::media_index::store::{EnrichmentState, MediaStatusRow, MediaStore, media_db_path};
use crate::media_index::writer::MediaWriter;

use super::budget::ByteBudget;
use super::enrich::{NetworkEnrichCtx, NetworkPassOutcome, PauseReason, enrich_network_and_gc};
use super::fetch::FakeByteFetcher;
use super::policy::ConservativeFetchPolicy;

const MOUNT: &str = "/Volumes/naspi";

/// These tests drive the ONE-worker (sequential) network path, so the extra-worker seams
/// are never exercised: `never_make` panics if reached, `one_worker` pins the count to 1,
/// and `TEST_BUDGET` is an unused (but valid) prefetch budget. The parallel path has its own
/// coverage in `budget.rs`, `pool/tests.rs`, and the parallel-specific cases below.
fn never_make() -> std::sync::Arc<dyn crate::media_index::backend::VisionBackend> {
    unreachable!("the one-worker network path never builds an extra backend")
}
fn one_worker() -> usize {
    1
}
static TEST_BUDGET: std::sync::LazyLock<ByteBudget> = std::sync::LazyLock::new(|| ByteBudget::new(64 * 1024 * 1024));

/// Open a fresh media store + writer for a scratch volume.
fn media_writer(dir: &std::path::Path, volume_id: &str) -> MediaWriter {
    let db_path = media_db_path(dir, volume_id);
    MediaStore::open(&db_path).expect("open media store");
    MediaWriter::spawn(&db_path, volume_id).expect("media writer")
}

/// A qualifying image entry keyed on its index-relative path (as the walk produces).
fn image(rel_path: &str, mtime: u64, size: u64) -> ImageEntry {
    ImageEntry {
        path: rel_path.to_string(),
        mtime: Some(mtime),
        size: Some(size),
        kind: MediaKind::Image,
    }
}

/// The OS path the fetcher is scripted on (mount root + index-relative path).
fn os(rel_path: &str) -> String {
    format!("{MOUNT}{rel_path}")
}

fn always_enrich(_os_path: &str) -> bool {
    true
}

/// The default "nothing is excluded" privacy veto for tests not exercising exclusion.
fn never_excluded(_os_path: &str) -> bool {
    false
}

fn no_sleep(_d: Duration) {}

// ── The conservative-fetch decision (idle gate), over a fake idle signal ────

#[test]
fn defers_when_not_idle_enriches_nothing() {
    let dir = tempfile::tempdir().expect("temp");
    let writer = media_writer(dir.path(), "smb-vol");
    let images = [image("/DCIM/a.jpg", 1, 10)];
    let fetcher = FakeByteFetcher::new().with_bytes(os("/DCIM/a.jpg"), b"jpeg".to_vec());
    let backend = FakeVisionBackend::new();
    let policy = ConservativeFetchPolicy::default();

    // Not idle ⇒ the pass defers before fetching anything.
    let not_idle = || false;
    let cancel = || false;
    let ctx = NetworkEnrichCtx {
        volume_id: "smb-vol",
        mount_root: MOUNT,
        images: &images,
        statuses: &HashMap::new(),
        backend: &backend,
        make: &never_make,
        workers: &one_worker,
        budget: &TEST_BUDGET,
        fetcher: &fetcher,
        writer: &writer,
        policy: &policy,
        is_idle: &not_idle,
        should_enrich: &always_enrich,
        is_excluded: &never_excluded,
        cancel: &cancel,
        sleep: &no_sleep,
        progress: &NoopProgressSink,
        clip_stamp: None,
    };
    let outcome = enrich_network_and_gc(&ctx).expect("pass");
    assert!(
        matches!(
            outcome,
            NetworkPassOutcome::Paused {
                reason: PauseReason::NotIdle,
                ..
            }
        ),
        "a non-idle app defers the network pass"
    );
    let store = MediaStore::open(&media_db_path(dir.path(), "smb-vol")).expect("reopen");
    assert!(
        store.status_for("/DCIM/a.jpg").expect("read").is_none(),
        "nothing enriched"
    );
    writer.shutdown();
}

#[test]
fn proceeds_when_idle_and_enriches_over_the_fetched_bytes() {
    let dir = tempfile::tempdir().expect("temp");
    let writer = media_writer(dir.path(), "smb-vol");
    let images = [image("/DCIM/a.jpg", 1, 10)];
    let fetcher = FakeByteFetcher::new().with_bytes(os("/DCIM/a.jpg"), b"jpeg".to_vec());
    let backend = FakeVisionBackend::new().with_text("/DCIM/a.jpg", "a beach at sunset");
    let policy = ConservativeFetchPolicy::default();

    let idle = || true;
    let cancel = || false;
    let ctx = NetworkEnrichCtx {
        volume_id: "smb-vol",
        mount_root: MOUNT,
        images: &images,
        statuses: &HashMap::new(),
        backend: &backend,
        make: &never_make,
        workers: &one_worker,
        budget: &TEST_BUDGET,
        fetcher: &fetcher,
        writer: &writer,
        policy: &policy,
        is_idle: &idle,
        should_enrich: &always_enrich,
        is_excluded: &never_excluded,
        cancel: &cancel,
        sleep: &no_sleep,
        progress: &NoopProgressSink,
        clip_stamp: None,
    };
    let outcome = enrich_network_and_gc(&ctx).expect("pass");
    assert!(matches!(outcome, NetworkPassOutcome::Completed(s) if s.enriched == 1));

    // The bytes were fetched off the "mount" and OCR'd; the text is searchable.
    let index = MediaIndex::open(dir.path(), "smb-vol");
    let hits = index.search_ocr("beach", 10).expect("search");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].path, "/DCIM/a.jpg");
    writer.shutdown();
}

#[test]
fn bandwidth_throttle_is_invoked_per_fetched_image() {
    let dir = tempfile::tempdir().expect("temp");
    let writer = media_writer(dir.path(), "smb-vol");
    let images = [image("/DCIM/a.jpg", 1, 4), image("/DCIM/b.jpg", 1, 4)];
    let fetcher = FakeByteFetcher::new()
        .with_bytes(os("/DCIM/a.jpg"), vec![0u8; 1_000_000])
        .with_bytes(os("/DCIM/b.jpg"), vec![0u8; 1_000_000]);
    let backend = FakeVisionBackend::new();
    // 1 MB/s ⇒ each 1 MB fetch should ask to sleep ~1 s.
    let policy = ConservativeFetchPolicy {
        max_bytes_per_sec: 1_000_000,
        ..ConservativeFetchPolicy::default()
    };

    let idle = || true;
    let cancel = || false;
    let recorded: RefCell<Vec<Duration>> = RefCell::new(Vec::new());
    let record = |d: Duration| recorded.borrow_mut().push(d);
    let ctx = NetworkEnrichCtx {
        volume_id: "smb-vol",
        mount_root: MOUNT,
        images: &images,
        statuses: &HashMap::new(),
        backend: &backend,
        make: &never_make,
        workers: &one_worker,
        budget: &TEST_BUDGET,
        fetcher: &fetcher,
        writer: &writer,
        policy: &policy,
        is_idle: &idle,
        should_enrich: &always_enrich,
        is_excluded: &never_excluded,
        cancel: &cancel,
        sleep: &record,
        progress: &NoopProgressSink,
        clip_stamp: None,
    };
    enrich_network_and_gc(&ctx).expect("pass");
    let delays = recorded.borrow();
    assert_eq!(delays.len(), 2, "one throttle per fetched image");
    assert_eq!(delays[0], Duration::from_secs(1), "1 MB at 1 MB/s ⇒ 1 s");
    writer.shutdown();
}

// ── Resumability across unmount: completed rows kept, NO false Failed ────────

#[test]
fn disconnect_mid_pass_keeps_completed_rows_and_writes_no_failure() {
    let dir = tempfile::tempdir().expect("temp");
    let writer = media_writer(dir.path(), "smb-vol");
    let images = [image("/DCIM/a.jpg", 1, 10), image("/DCIM/b.jpg", 1, 10)];
    // a.jpg fetches fine; b.jpg's fetch disconnects (unmount mid-pass).
    let fetcher = FakeByteFetcher::new()
        .with_bytes(os("/DCIM/a.jpg"), b"jpeg".to_vec())
        .disconnect_on(os("/DCIM/b.jpg"));
    let backend = FakeVisionBackend::new().with_text("/DCIM/a.jpg", "first photo");
    let policy = ConservativeFetchPolicy::default();

    let idle = || true;
    let cancel = || false;
    let ctx = NetworkEnrichCtx {
        volume_id: "smb-vol",
        mount_root: MOUNT,
        images: &images,
        statuses: &HashMap::new(),
        backend: &backend,
        make: &never_make,
        workers: &one_worker,
        budget: &TEST_BUDGET,
        fetcher: &fetcher,
        writer: &writer,
        policy: &policy,
        is_idle: &idle,
        should_enrich: &always_enrich,
        is_excluded: &never_excluded,
        cancel: &cancel,
        sleep: &no_sleep,
        progress: &NoopProgressSink,
        clip_stamp: None,
    };
    let outcome = enrich_network_and_gc(&ctx).expect("pass");
    assert!(
        matches!(
            outcome,
            NetworkPassOutcome::Paused {
                reason: PauseReason::Disconnected,
                ..
            }
        ),
        "a mid-pass unmount pauses, it doesn't complete"
    );

    let store = MediaStore::open(&media_db_path(dir.path(), "smb-vol")).expect("reopen");
    // The completed image's row survives.
    let a = store.status_for("/DCIM/a.jpg").expect("read a");
    assert!(a.is_some(), "the completed image's row is kept across the unmount");
    // The disconnected image has NO row at all — crucially NOT a `Failed` row (a
    // disconnect is not a bad file).
    assert!(
        store.status_for("/DCIM/b.jpg").expect("read b").is_none(),
        "a disconnected image is never marked Failed"
    );
    writer.shutdown();
}

// ── GC must NOT fire on a mere disconnect (data-safety) ──────────────────────

#[test]
fn gc_does_not_fire_on_a_disconnect() {
    let dir = tempfile::tempdir().expect("temp");
    let writer = media_writer(dir.path(), "smb-vol");
    // Pre-seed a stored row whose source is absent from THIS walk. A completed pass
    // would GC it — but a disconnected pass must NOT.
    writer
        .upsert(
            MediaStatusRow {
                path: "/DCIM/old.jpg".to_string(),
                mtime: Some(1),
                size: Some(2),
                media_kind: MediaKind::Image,
                state: EnrichmentState::Done,
                engine_version: "fake-vision-1".to_string(),
                clip_stamp: String::new(),
            },
            Some(crate::media_index::writer::UpsertAnalysis::ocr_only(
                "kept across disconnect",
            )),
        )
        .expect("seed");
    writer.flush_blocking().expect("flush");

    // The walk has only b.jpg, whose fetch disconnects before completing.
    let images = [image("/DCIM/b.jpg", 1, 10)];
    let fetcher = FakeByteFetcher::new().disconnect_on(os("/DCIM/b.jpg"));
    let backend = FakeVisionBackend::new();
    let policy = ConservativeFetchPolicy::default();
    let statuses = crate::media_index::scheduler::enrich::load_statuses(dir.path(), "smb-vol");

    let idle = || true;
    let cancel = || false;
    let ctx = NetworkEnrichCtx {
        volume_id: "smb-vol",
        mount_root: MOUNT,
        images: &images,
        statuses: &statuses,
        backend: &backend,
        make: &never_make,
        workers: &one_worker,
        budget: &TEST_BUDGET,
        fetcher: &fetcher,
        writer: &writer,
        policy: &policy,
        is_idle: &idle,
        should_enrich: &always_enrich,
        is_excluded: &never_excluded,
        cancel: &cancel,
        sleep: &no_sleep,
        progress: &NoopProgressSink,
        clip_stamp: None,
    };
    let outcome = enrich_network_and_gc(&ctx).expect("pass");
    assert!(matches!(
        outcome,
        NetworkPassOutcome::Paused {
            reason: PauseReason::Disconnected,
            ..
        }
    ));

    let store = MediaStore::open(&media_db_path(dir.path(), "smb-vol")).expect("reopen");
    assert!(
        store.status_for("/DCIM/old.jpg").expect("read").is_some(),
        "a disconnect must NOT GC a volume's existing coverage"
    );
    writer.shutdown();
}

// ── The "always index" override enriches a low-importance folder ─────────────

#[test]
fn override_enriches_a_low_importance_folder_while_the_rest_defers() {
    let dir = tempfile::tempdir().expect("temp");
    let writer = media_writer(dir.path(), "smb-vol");
    // Two folders; only /Photos is overridden. A synthetic importance signal scores
    // BOTH low (below threshold), so without the override neither would enrich.
    let images = [image("/Photos/keep.jpg", 1, 10), image("/Misc/skip.jpg", 1, 10)];
    let fetcher = FakeByteFetcher::new()
        .with_bytes(os("/Photos/keep.jpg"), b"jpeg".to_vec())
        .with_bytes(os("/Misc/skip.jpg"), b"jpeg".to_vec());
    let backend = FakeVisionBackend::new()
        .with_text("/Photos/keep.jpg", "the overridden photo")
        .with_text("/Misc/skip.jpg", "should be deferred");
    let policy = ConservativeFetchPolicy::default();

    // The gate the scheduler composes: override OR importance>=threshold. Here the
    // synthetic importance is always 0.1 (low), threshold 0.5, and only /Photos is
    // override-covered — so the pure `should_enrich_image` says yes for /Photos only.
    let threshold = 0.5f32;
    let synthetic_importance = |_os: &str| Some(0.1f32);
    let overridden_folder = format!("{MOUNT}/Photos");
    let gate = |os_path: &str| {
        let covered = super::config::path_is_within(os_path, &overridden_folder);
        super::policy::should_enrich_image(covered, synthetic_importance(os_path), threshold)
    };
    let idle = || true;
    let cancel = || false;
    let ctx = NetworkEnrichCtx {
        volume_id: "smb-vol",
        mount_root: MOUNT,
        images: &images,
        statuses: &HashMap::new(),
        backend: &backend,
        make: &never_make,
        workers: &one_worker,
        budget: &TEST_BUDGET,
        fetcher: &fetcher,
        writer: &writer,
        policy: &policy,
        is_idle: &idle,
        should_enrich: &gate,
        is_excluded: &never_excluded,
        cancel: &cancel,
        sleep: &no_sleep,
        progress: &NoopProgressSink,
        clip_stamp: None,
    };
    let outcome = enrich_network_and_gc(&ctx).expect("pass");
    assert!(matches!(outcome, NetworkPassOutcome::Completed(s) if s.enriched == 1));

    let store = MediaStore::open(&media_db_path(dir.path(), "smb-vol")).expect("reopen");
    assert!(
        store.status_for("/Photos/keep.jpg").expect("read").is_some(),
        "the overridden low-importance folder DOES enrich"
    );
    assert!(
        store.status_for("/Misc/skip.jpg").expect("read").is_none(),
        "a non-overridden low-importance folder defers"
    );
    writer.shutdown();
}

// ── Offline search after unmount (Decision 8) ───────────────────────────────

#[test]
fn search_answers_offline_after_the_volume_unmounts() {
    let dir = tempfile::tempdir().expect("temp");
    let writer = media_writer(dir.path(), "smb-vol");
    let images = [image("/DCIM/a.jpg", 1, 10)];
    let fetcher = FakeByteFetcher::new().with_bytes(os("/DCIM/a.jpg"), b"jpeg".to_vec());
    let backend = FakeVisionBackend::new().with_text("/DCIM/a.jpg", "lighthouse by the sea");
    let policy = ConservativeFetchPolicy::default();
    let idle = || true;
    let cancel = || false;
    let ctx = NetworkEnrichCtx {
        volume_id: "smb-vol",
        mount_root: MOUNT,
        images: &images,
        statuses: &HashMap::new(),
        backend: &backend,
        make: &never_make,
        workers: &one_worker,
        budget: &TEST_BUDGET,
        fetcher: &fetcher,
        writer: &writer,
        policy: &policy,
        is_idle: &idle,
        should_enrich: &always_enrich,
        is_excluded: &never_excluded,
        cancel: &cancel,
        sleep: &no_sleep,
        progress: &NoopProgressSink,
        clip_stamp: None,
    };
    enrich_network_and_gc(&ctx).expect("pass");
    // Simulate unmount: drop the writer (the mount/volume is gone). The read API opens
    // `media.db` directly, keyed by volume_id, so it still answers.
    writer.shutdown();

    let index = MediaIndex::open(dir.path(), "smb-vol");
    let hits = index.search_ocr("lighthouse", 10).expect("offline search");
    assert_eq!(
        hits.len(),
        1,
        "the NAS's photos stay searchable with the volume unplugged"
    );
    assert_eq!(hits[0].path, "/DCIM/a.jpg");
}

#[test]
fn exclusion_landing_during_network_analyze_writes_no_row() {
    // The network mirror of the local in-flight-analyze TOCTOU: an image passes the
    // filter veto (false), the exclude lands DURING analyze, and the pre-upsert
    // re-check (the second `is_excluded` call) drops the row. Modeled by a stateful
    // veto that flips false → true across its two calls.
    let dir = tempfile::tempdir().expect("temp");
    let writer = media_writer(dir.path(), "smb-vol");
    let images = [image("/DCIM/a.jpg", 1, 10)];
    let fetcher = FakeByteFetcher::new().with_bytes(os("/DCIM/a.jpg"), b"jpeg".to_vec());
    let backend = FakeVisionBackend::new();
    let policy = ConservativeFetchPolicy::default();

    let idle = || true;
    let cancel = || false;
    // `AtomicU32` (not `Cell`) because `is_excluded` is now `+ Sync`; this test runs the
    // one-worker sequential path, so the veto still flips false → true across its two calls.
    let calls = std::sync::atomic::AtomicU32::new(0);
    let excluded = |_: &str| calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst) >= 1;
    let ctx = NetworkEnrichCtx {
        volume_id: "smb-vol",
        mount_root: MOUNT,
        images: &images,
        statuses: &HashMap::new(),
        backend: &backend,
        make: &never_make,
        workers: &one_worker,
        budget: &TEST_BUDGET,
        fetcher: &fetcher,
        writer: &writer,
        policy: &policy,
        is_idle: &idle,
        should_enrich: &always_enrich,
        is_excluded: &excluded,
        cancel: &cancel,
        sleep: &no_sleep,
        progress: &NoopProgressSink,
        clip_stamp: None,
    };
    let outcome = enrich_network_and_gc(&ctx).expect("pass");
    assert!(matches!(outcome, NetworkPassOutcome::Completed(s) if s.enriched == 0));
    assert!(
        calls.load(std::sync::atomic::Ordering::SeqCst) >= 2,
        "the veto is re-checked before the upsert"
    );
    let store = MediaStore::open(&media_db_path(dir.path(), "smb-vol")).expect("reopen");
    assert!(
        store.status_for("/DCIM/a.jpg").expect("read").is_none(),
        "nothing persisted for the mid-analyze network exclusion"
    );
    writer.shutdown();
}

// ── The parallel (N-worker) network path (plan M2) ──────────────────────────

/// An instrumented backend shared by every parallel worker: records per-path call counts
/// and the peak simultaneous `analyze_media`, and sleeps a little so overlap is real.
/// Delegates results to an inner [`FakeVisionBackend`].
struct ParallelProbe {
    inner: FakeVisionBackend,
    in_flight: std::sync::atomic::AtomicUsize,
    peak: std::sync::atomic::AtomicUsize,
    calls: std::sync::Mutex<HashMap<String, usize>>,
    delay: Duration,
}

impl ParallelProbe {
    fn new(delay: Duration) -> std::sync::Arc<Self> {
        std::sync::Arc::new(Self {
            inner: FakeVisionBackend::new(),
            in_flight: std::sync::atomic::AtomicUsize::new(0),
            peak: std::sync::atomic::AtomicUsize::new(0),
            calls: std::sync::Mutex::new(HashMap::new()),
            delay,
        })
    }
    fn peak(&self) -> usize {
        self.peak.load(std::sync::atomic::Ordering::SeqCst)
    }
    fn max_calls_for_any_path(&self) -> usize {
        use crate::ignore_poison::IgnorePoison;
        self.calls.lock_ignore_poison().values().copied().max().unwrap_or(0)
    }
}

impl crate::media_index::backend::VisionBackend for ParallelProbe {
    fn engine_version(&self) -> String {
        self.inner.engine_version()
    }
    fn taxonomy_version(&self) -> String {
        self.inner.taxonomy_version()
    }
    fn ocr(
        &self,
        input: &crate::media_index::backend::ImageInput,
    ) -> Result<crate::media_index::backend::OcrResult, crate::media_index::backend::VisionError> {
        self.inner.ocr(input)
    }
    fn analyze(
        &self,
        input: &crate::media_index::backend::ImageInput,
    ) -> Result<crate::media_index::backend::Analysis, crate::media_index::backend::VisionError> {
        self.inner.analyze(input)
    }
    fn analyze_media(
        &self,
        input: &crate::media_index::backend::ImageInput,
        want_vision: bool,
        want_clip: bool,
    ) -> Result<crate::media_index::backend::MediaAnalysis, crate::media_index::backend::VisionError> {
        use crate::ignore_poison::IgnorePoison;
        use std::sync::atomic::Ordering;
        let cur = self.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
        self.peak.fetch_max(cur, Ordering::SeqCst);
        *self.calls.lock_ignore_poison().entry(input.path.clone()).or_insert(0) += 1;
        std::thread::sleep(self.delay);
        let r = self.inner.analyze_media(input, want_vision, want_clip);
        self.in_flight.fetch_sub(1, Ordering::SeqCst);
        r
    }
}

#[test]
fn parallel_network_enriches_each_path_once_and_the_byte_budget_bounds_concurrency() {
    let dir = tempfile::tempdir().expect("temp");
    let writer = media_writer(dir.path(), "smb-vol");
    // 40 images, each 10 bytes on the wire; the pass reserves `image.size` (10) per file.
    let images: Vec<ImageEntry> = (0..40).map(|i| image(&format!("/DCIM/img{i:03}.jpg"), 1, 10)).collect();
    let mut fetcher = FakeByteFetcher::new();
    for img in &images {
        fetcher = fetcher.with_bytes(os(&img.path), vec![0u8; 10]);
    }
    let backend = ParallelProbe::new(Duration::from_micros(300));
    let make_backend = backend.clone();
    let make = move || make_backend.clone() as std::sync::Arc<dyn crate::media_index::backend::VisionBackend>;
    let four = || 4usize;
    // A budget of 25 bytes admits at most TWO 10-byte reservations at once, so the pass
    // never has more than two images fetched-and-computing — the byte budget, not the four
    // workers, is the bound. Proves byte-bounded prefetch end to end (no deadlock, no
    // over-buffering).
    let budget = ByteBudget::new(25);
    let idle = || true;
    let cancel = || false;
    let ctx = NetworkEnrichCtx {
        volume_id: "smb-vol",
        mount_root: MOUNT,
        images: &images,
        statuses: &HashMap::new(),
        backend: backend.as_ref(),
        make: &make,
        workers: &four,
        budget: &budget,
        fetcher: &fetcher,
        writer: &writer,
        policy: &ConservativeFetchPolicy::default(),
        is_idle: &idle,
        should_enrich: &always_enrich,
        is_excluded: &never_excluded,
        cancel: &cancel,
        sleep: &no_sleep,
        progress: &NoopProgressSink,
        clip_stamp: None,
    };
    let outcome = enrich_network_and_gc(&ctx).expect("pass");
    assert!(matches!(outcome, NetworkPassOutcome::Completed(s) if s.enriched == 40));
    assert_eq!(
        backend.max_calls_for_any_path(),
        1,
        "no double-enrichment across workers"
    );
    assert!(
        backend.peak() <= 2,
        "byte budget should bound concurrency to 2, saw {}",
        backend.peak()
    );
    // Every row landed through the single writer.
    let statuses = crate::media_index::scheduler::enrich::load_statuses(dir.path(), "smb-vol");
    assert_eq!(statuses.len(), 40);
    writer.shutdown();
}

#[test]
fn parallel_network_disconnect_drains_workers_and_skips_gc() {
    let dir = tempfile::tempdir().expect("temp");
    let writer = media_writer(dir.path(), "smb-vol");
    // Seed a stored row NOT in this walk: a clean completion would GC it; a disconnect must not.
    let mut statuses = HashMap::new();
    statuses.insert(
        "/DCIM/gone.jpg".to_string(),
        MediaStatusRow {
            path: "/DCIM/gone.jpg".into(),
            mtime: Some(1),
            size: Some(1),
            media_kind: MediaKind::Image,
            state: EnrichmentState::Done,
            engine_version: "old".into(),
            clip_stamp: String::new(),
        },
    );
    let images: Vec<ImageEntry> = (0..20).map(|i| image(&format!("/DCIM/img{i:03}.jpg"), 1, 10)).collect();
    let mut fetcher = FakeByteFetcher::new();
    for img in &images {
        fetcher = fetcher.with_bytes(os(&img.path), vec![0u8; 10]);
    }
    // The 11th image disconnects (unmount mid-pass).
    fetcher = fetcher.disconnect_on(os("/DCIM/img010.jpg"));
    let backend = ParallelProbe::new(Duration::from_micros(200));
    let make_backend = backend.clone();
    let make = move || make_backend.clone() as std::sync::Arc<dyn crate::media_index::backend::VisionBackend>;
    let four = || 4usize;
    let budget = ByteBudget::new(100);
    let idle = || true;
    let cancel = || false;
    let ctx = NetworkEnrichCtx {
        volume_id: "smb-vol",
        mount_root: MOUNT,
        images: &images,
        statuses: &statuses,
        backend: backend.as_ref(),
        make: &make,
        workers: &four,
        budget: &budget,
        fetcher: &fetcher,
        writer: &writer,
        policy: &ConservativeFetchPolicy::default(),
        is_idle: &idle,
        should_enrich: &always_enrich,
        is_excluded: &never_excluded,
        cancel: &cancel,
        sleep: &no_sleep,
        progress: &NoopProgressSink,
        clip_stamp: None,
    };
    let outcome = enrich_network_and_gc(&ctx).expect("pass");
    let NetworkPassOutcome::Paused { summary, reason } = outcome else {
        panic!("a mid-pass disconnect must pause, not complete");
    };
    assert_eq!(reason, PauseReason::Disconnected);
    assert_eq!(summary.gc_count, 0, "a paused pass never GCs");
    // The pre-disconnect images that were fetched were still computed (drained), and none
    // was double-enriched.
    assert!(summary.enriched >= 1, "workers drained fetched jobs before pausing");
    assert_eq!(backend.max_calls_for_any_path(), 1);
    // `gc_count == 0` (asserted above) is the data-safety guarantee: a paused pass runs no
    // deletions at all, so a mount blip can't wipe coverage. The seeded stale row is only in
    // the in-memory `statuses` (never persisted), so a DB-survival check would be vacuous.
    writer.shutdown();
}

// ── Per-file unreadable errors: skip-and-count, never a pause (plan M1) ──────

/// The M1 classification contract at the pass level: file k of n being unreadable
/// (permission denied and friends) skips THAT file — the pass still completes,
/// enriches the other n−1, counts the skip honestly, and writes NO row for the
/// unreadable file (`Failed` is reserved for a good read with a bad decode; a
/// pause is reserved for a typed disconnect).
#[test]
fn an_unreadable_file_skips_and_counts_and_never_pauses_the_pass() {
    let dir = tempfile::tempdir().expect("temp");
    let writer = media_writer(dir.path(), "smb-vol");
    let images = [
        image("/DCIM/a.jpg", 1, 10),
        image("/DCIM/locked.jpg", 1, 10),
        image("/DCIM/c.jpg", 1, 10),
    ];
    let fetcher = FakeByteFetcher::new()
        .with_bytes(os("/DCIM/a.jpg"), b"jpeg".to_vec())
        .unreadable_on(os("/DCIM/locked.jpg"))
        .with_bytes(os("/DCIM/c.jpg"), b"jpeg".to_vec());
    let backend = FakeVisionBackend::new();
    let policy = ConservativeFetchPolicy::default();

    let idle = || true;
    let cancel = || false;
    let ctx = NetworkEnrichCtx {
        volume_id: "smb-vol",
        mount_root: MOUNT,
        images: &images,
        statuses: &HashMap::new(),
        backend: &backend,
        make: &never_make,
        workers: &one_worker,
        budget: &TEST_BUDGET,
        fetcher: &fetcher,
        writer: &writer,
        policy: &policy,
        is_idle: &idle,
        should_enrich: &always_enrich,
        is_excluded: &never_excluded,
        cancel: &cancel,
        sleep: &no_sleep,
        progress: &NoopProgressSink,
        clip_stamp: None,
    };
    let outcome = enrich_network_and_gc(&ctx).expect("pass");
    let NetworkPassOutcome::Completed(summary) = outcome else {
        panic!("a per-file unreadable must not pause the pass, got {outcome:?}");
    };
    assert_eq!(summary.enriched, 2, "the readable neighbors still enrich");
    assert_eq!(summary.skipped_unreadable, 1, "the skip is counted, not hidden");

    // No row at all for the unreadable file: not `Failed` (bad decode), not `Done`.
    let store = MediaStore::open(&media_db_path(dir.path(), "smb-vol")).expect("reopen");
    assert!(
        store.status_for("/DCIM/locked.jpg").expect("read").is_none(),
        "an unreadable file must leave no row behind"
    );
    assert!(store.status_for("/DCIM/a.jpg").expect("read").is_some());
    assert!(store.status_for("/DCIM/c.jpg").expect("read").is_some());
    writer.shutdown();
}

/// The same contract on the parallel path: the fetcher-side skip must count and
/// continue (releasing the file's byte-budget reservation), never stop production.
#[test]
fn parallel_unreadable_files_skip_and_count_without_pausing() {
    let dir = tempfile::tempdir().expect("temp");
    let writer = media_writer(dir.path(), "smb-vol");
    let images: Vec<ImageEntry> = (0..12).map(|i| image(&format!("/DCIM/img{i:03}.jpg"), 1, 10)).collect();
    let mut fetcher = FakeByteFetcher::new();
    for (i, img) in images.iter().enumerate() {
        // Every third file is unreadable; the rest fetch fine.
        if i % 3 == 1 {
            fetcher = fetcher.unreadable_on(os(&img.path));
        } else {
            fetcher = fetcher.with_bytes(os(&img.path), vec![0u8; 10]);
        }
    }
    let backend = ParallelProbe::new(Duration::from_micros(200));
    let make_backend = backend.clone();
    let make = move || make_backend.clone() as std::sync::Arc<dyn crate::media_index::backend::VisionBackend>;
    let four = || 4usize;
    // A small budget, so a leaked reservation for a skipped file would deadlock the
    // pass (the regression this test also guards).
    let budget = ByteBudget::new(25);
    let idle = || true;
    let cancel = || false;
    let ctx = NetworkEnrichCtx {
        volume_id: "smb-vol",
        mount_root: MOUNT,
        images: &images,
        statuses: &HashMap::new(),
        backend: backend.as_ref(),
        make: &make,
        workers: &four,
        budget: &budget,
        fetcher: &fetcher,
        writer: &writer,
        policy: &ConservativeFetchPolicy::default(),
        is_idle: &idle,
        should_enrich: &always_enrich,
        is_excluded: &never_excluded,
        cancel: &cancel,
        sleep: &no_sleep,
        progress: &NoopProgressSink,
        clip_stamp: None,
    };
    let outcome = enrich_network_and_gc(&ctx).expect("pass");
    let NetworkPassOutcome::Completed(summary) = outcome else {
        panic!("per-file unreadable files must not pause the parallel pass, got {outcome:?}");
    };
    assert_eq!(summary.enriched, 8);
    assert_eq!(summary.skipped_unreadable, 4);
    writer.shutdown();
}
