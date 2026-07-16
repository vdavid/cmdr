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

use super::enrich::{NetworkEnrichCtx, NetworkPassOutcome, PauseReason, enrich_network_and_gc};
use super::fetch::FakeByteFetcher;
use super::policy::ConservativeFetchPolicy;

const MOUNT: &str = "/Volumes/naspi";

/// Open a fresh media store + writer for a scratch volume.
fn media_writer(dir: &std::path::Path, volume_id: &str) -> MediaWriter {
    let db_path = media_db_path(dir, volume_id);
    MediaStore::open(&db_path).expect("open media store");
    MediaWriter::spawn(&db_path).expect("media writer")
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
    use std::cell::Cell;
    let dir = tempfile::tempdir().expect("temp");
    let writer = media_writer(dir.path(), "smb-vol");
    let images = [image("/DCIM/a.jpg", 1, 10)];
    let fetcher = FakeByteFetcher::new().with_bytes(os("/DCIM/a.jpg"), b"jpeg".to_vec());
    let backend = FakeVisionBackend::new();
    let policy = ConservativeFetchPolicy::default();

    let idle = || true;
    let cancel = || false;
    let calls = Cell::new(0u32);
    let excluded = |_: &str| {
        let n = calls.get();
        calls.set(n + 1);
        n >= 1
    };
    let ctx = NetworkEnrichCtx {
        volume_id: "smb-vol",
        mount_root: MOUNT,
        images: &images,
        statuses: &HashMap::new(),
        backend: &backend,
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
    assert!(calls.get() >= 2, "the veto is re-checked before the upsert");
    let store = MediaStore::open(&media_db_path(dir.path(), "smb-vol")).expect("reopen");
    assert!(
        store.status_for("/DCIM/a.jpg").expect("read").is_none(),
        "nothing persisted for the mid-analyze network exclusion"
    );
    writer.shutdown();
}
