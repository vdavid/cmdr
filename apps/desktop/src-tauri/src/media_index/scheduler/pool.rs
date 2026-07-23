//! The parallel enrichment worker pool (plan M2): drive the decode + inference stage of
//! a local pass across N workers instead of one, funneling every write back to the
//! single SQLite writer.
//!
//! ## Why this shape
//!
//! The real Vision backend confines its `!Send` CoreFoundation objects to ONE dedicated
//! 8 MB-stack thread and serializes calls on it (`backend/vision/mod.rs`). Parallelism
//! is therefore N INDEPENDENT backends — each its own thread, stack, autoreleasepool,
//! and request handlers — not concurrent calls into one. This module owns that fan-out:
//! N scoped worker threads, worker 0 on the scheduler's long-lived `representative`
//! backend and workers 1..N on backends built on demand from `make`, each pulling image
//! indices off a shared atomic cursor so no path is ever handed to two workers (the
//! no-double-enrichment invariant is structural, not a lock).
//!
//! Writes stay single: the [`MediaWriter`] handle is cloneable and already funnels every
//! `upsert` through ONE writer thread, so N workers calling it concurrently is safe and
//! preserves the single-writer invariant (parallelize compute, never DB writes).
//!
//! ## Live-apply + thermal
//!
//! `workers()` returns the CURRENT effective worker count — the user's
//! `mediaIndex.parallelism` capped by live thermal pressure — re-read between images. A
//! SHRINK retires the now-excess worker slots within the running batch (they just stop
//! pulling); a GROW ends the batch so the outer loop re-spawns with more threads. Either
//! way the pass never restarts and the cursor is never rewound, so a mid-pass slider move
//! or a thermal event takes effect within about one image.
//!
//! ## Cancellation
//!
//! The same between-images `cancel` hook the serial pass used (memory watchdog OR master
//! toggle off) is checked by every worker before each image; the first worker to see it
//! sets the shared stop flag and all workers drain out promptly, GC is skipped, and the
//! pass reports `cancelled` (rows kept — stopping is never erasing).

use std::collections::HashSet;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::thread;

use crate::ignore_poison::IgnorePoison;
use crate::media_index::backend::{ImageInput, VisionBackend, VisionError};
use crate::media_index::progress::{EnrichProgress, EnrichProgressSink};
use crate::media_index::store::{EnrichmentState, needs_clip, needs_enrichment};
use crate::media_index::writer::MediaWriter;

use super::enrich::{
    EnrichGates, GcScope, ImageEntry, PassSummary, apply_media_upsert, enrichable_totals, gc_targets, parent_dir,
    status_row,
};

/// Build a fresh independent backend for an extra worker slot (workers 1..N). In
/// production this is `|| Arc::new(VisionOcrBackend::new())` — one dedicated Vision
/// thread each; in the concurrency tests it clones the ONE shared fake so every worker's
/// calls land in the same recorder. Worker 0 always reuses the scheduler's long-lived
/// `representative` backend, so a steady N=1 pass builds NOTHING here (byte-for-byte
/// today's single-worker behavior).
pub(crate) type MakeBackend<'a> = dyn Fn() -> Arc<dyn VisionBackend> + Sync + 'a;

/// The effective worker count, re-read between images: the user's chosen parallelism
/// capped by live thermal pressure. A `dyn Fn` seam so tests script it (and change it
/// mid-pass to exercise live-apply and thermal backoff) without touching globals or FFI.
pub(crate) type WorkerCount<'a> = dyn Fn() -> usize + Sync + 'a;

/// The shared, thread-safe state every worker reads and updates during a pass. The
/// counters are atomics (each worker snapshots its own monotonic view for progress); the
/// first per-image writer error is captured here and turns the whole pass into an `Err`
/// after the batch drains (mirroring the serial `?`).
struct PassState<'a> {
    statuses: &'a std::collections::HashMap<String, crate::media_index::store::MediaStatusRow>,
    gates: &'a EnrichGates<'a>,
    writer: &'a MediaWriter,
    progress: &'a dyn EnrichProgressSink,
    stamp: &'a str,
    total: u64,
    bytes_total: u64,
    done: AtomicU64,
    bytes_done: AtomicU64,
    enriched: AtomicUsize,
    /// Set when any worker should stop: a genuine cancel (watchdog / toggle off) OR a
    /// writer error. Disambiguated after the pass by `first_error`.
    stop: AtomicBool,
    /// The first per-image writer error, if any — the pass returns it as `Err` (a data
    /// write failing is fatal to the pass, exactly as the serial `?` made it).
    first_error: Mutex<Option<String>>,
}

/// Run one local enrichment pass across up to `workers()` parallel workers and return
/// what it did. Enriches the stale, covered images (skipping excluded / deferred ones,
/// re-checking the live exclusion veto after the slow analyze) and GCs vanished rows on a
/// clean completion. This is the ONE core: the serial `enrich_and_gc_scoped` delegates
/// here with `workers = || 1` and an unreachable `make`, so a steady N=1 pass is a single
/// worker pulling in cursor order — identical observable behavior to the pre-pool loop.
///
/// `representative` is worker 0's backend (the scheduler's long-lived one, or the test
/// fake); `make` builds workers 1..N; `workers` is the live effective count.
#[allow(
    clippy::too_many_arguments,
    reason = "the pass's inputs (images, statuses, backend, factory, worker count, writer, gates, cancel, progress) are each a distinct seam; bundling them into a struct would just move the arg list, not shrink it"
)]
pub(crate) fn run_enrich_pool(
    images: &[ImageEntry],
    statuses: &std::collections::HashMap<String, crate::media_index::store::MediaStatusRow>,
    representative: &dyn VisionBackend,
    make: &MakeBackend,
    workers: &WorkerCount,
    writer: &MediaWriter,
    gates: &EnrichGates,
    cancel: &(dyn Fn() -> bool + Sync),
    progress: &dyn EnrichProgressSink,
) -> Result<PassSummary, String> {
    let stamp = representative.analysis_stamp();
    let current: HashSet<String> = images.iter().map(|i| i.path.clone()).collect();

    let (total, bytes_total) = enrichable_totals(images, gates.should_enrich, gates.is_excluded);
    let state = PassState {
        statuses,
        gates,
        writer,
        progress,
        stamp: &stamp,
        total,
        bytes_total,
        done: AtomicU64::new(0),
        bytes_done: AtomicU64::new(0),
        enriched: AtomicUsize::new(0),
        stop: AtomicBool::new(false),
        first_error: Mutex::new(None),
    };
    // The pass-start tick, so the indicator row appears immediately at 0 / total.
    progress.report(EnrichProgress {
        done: 0,
        total,
        bytes_done: 0,
        bytes_total,
    });

    let cursor = AtomicUsize::new(0);

    // Batched so a GROW can add threads: each batch runs at a fixed width `n`; a worker
    // retires when its slot exceeds the live count (SHRINK) or ends the batch when the
    // live count exceeds `n` (GROW → re-spawn wider). The cursor persists across batches,
    // so no image is reprocessed or skipped.
    loop {
        if state.stop.load(Ordering::Acquire) || cursor.load(Ordering::Acquire) >= images.len() {
            break;
        }
        // No `cancel()` check here: the workers below check it once per image (exactly as
        // the serial loop did), and set `stop` — checking it in the outer loop too would
        // call a STATEFUL cancel hook an extra time per batch and shift its count.
        let n = workers().max(1);
        // Extra independent backends for workers 1..n (empty at n == 1: worker 0 reuses
        // the representative, so N=1 builds nothing). Owned by this stack frame and
        // borrowed by the scoped threads; dropped at batch end, so a SHRINK's excess
        // Vision threads exit promptly.
        let extra: Vec<Arc<dyn VisionBackend>> = (1..n).map(|_| make()).collect();

        thread::scope(|scope| {
            for id in 0..n {
                let backend: &dyn VisionBackend = if id == 0 {
                    representative
                } else {
                    extra[id - 1].as_ref()
                };
                let state = &state;
                let cursor = &cursor;
                scope.spawn(move || worker_loop(id, n, backend, state, cursor, workers, cancel, images));
            }
        });
        // `extra` drops here → the batch's extra Vision threads shut down.
    }

    // A writer error anywhere fails the whole pass (as the serial `?` did).
    if let Some(err) = state.first_error.lock_ignore_poison().take() {
        return Err(err);
    }
    let cancelled = state.stop.load(Ordering::Acquire);

    // Deletion-driven GC — only on a clean completion (never when cancelled: an emergency
    // stop yields fully; vanished rows collect on the next completed scan). Single-
    // threaded here, after every worker has drained.
    let gc_count = if cancelled {
        0
    } else {
        let targets: Vec<String> = match gates.gc_scope {
            GcScope::WholeStore => gc_targets(statuses.keys(), &current),
            GcScope::TouchedDirs(dirs) => statuses
                .keys()
                .filter(|p| dirs.contains(parent_dir(p)) && !current.contains(*p))
                .cloned()
                .collect(),
        };
        let n = targets.len();
        writer.gc_paths(targets).map_err(|e| e.to_string())?;
        n
    };

    writer.flush_blocking().map_err(|e| e.to_string())?;
    Ok(PassSummary {
        enriched: state.enriched.load(Ordering::Acquire),
        gc_count,
        cancelled,
    })
}

/// One worker's loop within a batch of width `n`: pull the next image off the shared
/// cursor and process it until the pass stops, the batch's width changes, or the images
/// run out. `id` is the worker's slot (0-based); worker 0 rides the representative
/// backend, so it never retires on a shrink to 1.
#[allow(
    clippy::too_many_arguments,
    reason = "a worker needs its id, batch width, backend, shared state, cursor, live worker count, cancel hook, and the image slice; all distinct, no natural grouping"
)]
fn worker_loop(
    id: usize,
    n: usize,
    backend: &dyn VisionBackend,
    state: &PassState,
    cursor: &AtomicUsize,
    workers: &WorkerCount,
    cancel: &(dyn Fn() -> bool + Sync),
    images: &[ImageEntry],
) {
    loop {
        if state.stop.load(Ordering::Acquire) {
            break;
        }
        // The between-images cancel hook (memory watchdog OR master toggle off): the
        // first worker to see it stops the whole pass promptly (everything cancelable).
        if cancel() {
            state.stop.store(true, Ordering::Release);
            break;
        }
        // Live-apply: re-read the effective width. A SHRINK retires this slot; a GROW ends
        // the batch so the outer loop re-spawns wider.
        let cur = workers().max(1);
        if id >= cur || cur > n {
            break;
        }
        let i = cursor.fetch_add(1, Ordering::AcqRel);
        let Some(image) = images.get(i) else {
            break;
        };
        if let Err(err) = process_image(image, backend, state) {
            *state.first_error.lock_ignore_poison() = Some(err);
            state.stop.store(true, Ordering::Release);
            break;
        }
    }
}

/// Process ONE image: the exact per-image body of the pass (coverage + privacy gates,
/// two-part staleness, the single-decode analyze, the post-analyze TOCTOU veto re-check,
/// and the upsert / fail / quiet-skip outcomes), updating the shared counters and
/// reporting progress. Shared by every worker (and thus by the serial N=1 path), so the
/// enrich policy lives in ONE place. Returns `Err` only on a writer failure (fatal to the
/// pass); a bad image is a per-file outcome, never an error.
fn process_image(image: &ImageEntry, backend: &dyn VisionBackend, state: &PassState) -> Result<(), String> {
    let is_excluded = state.gates.is_excluded;
    let should_enrich = state.gates.should_enrich;

    // Privacy veto (LIVE, beats coverage) then coverage: a vetoed/deferred image is
    // skipped here but stays in the GC `current` set, and is NOT in the enrichable subset
    // (so it doesn't count toward done/total).
    if is_excluded(&image.path) || !should_enrich(&image.path) {
        return Ok(());
    }

    // In the enrichable subset ⇒ count it as processed no matter the outcome, so the bar
    // reaches `total` on completion.
    let done = state.done.fetch_add(1, Ordering::AcqRel) + 1;
    let bytes_done = state.bytes_done.fetch_add(image.size.unwrap_or(0), Ordering::AcqRel) + image.size.unwrap_or(0);

    let stored = state.statuses.get(&image.path);
    let want_vision = needs_enrichment(stored, image.mtime, image.size, state.stamp);
    let want_clip = needs_clip(stored, state.gates.clip_stamp);
    if want_vision || want_clip {
        let input = ImageInput {
            path: image.path.clone(),
            kind: image.kind,
            // Local volume: the backend reads the real on-disk path itself.
            bytes: None,
        };
        // ONE decode runs the requested side(s) (plan M3 Q5).
        let analysis = backend.analyze_media(&input, want_vision, want_clip);
        // Re-check the LIVE veto AFTER the slow analyze: an exclusion that landed during it
        // must not persist a row (the in-flight-analyze TOCTOU close).
        if !is_excluded(&image.path) {
            match analysis {
                Ok(media) => {
                    if apply_media_upsert(
                        state.writer,
                        image,
                        state.stamp,
                        state.gates.clip_stamp,
                        want_vision,
                        media,
                    )? {
                        state.enriched.fetch_add(1, Ordering::AcqRel);
                    }
                }
                // A VANISHED source (ENOENT-class): skip QUIETLY (DEBUG, no row); a later
                // completed pass's GC collects any stale row. Already counted toward `done`.
                Err(VisionError::Missing(msg)) => {
                    log::debug!(target: "media_index", "skipping vanished image '{}': {msg}", image.path);
                }
                // A present-but-bad file: a real per-file failure. Mark Vision `Failed` (if
                // attempted) and stamp CLIP (embedding `None`) so a bad file isn't re-decoded
                // for CLIP every pass.
                Err(e) => {
                    log::warn!(target: "media_index", "analysis failed for '{}': {e}", image.path);
                    if want_vision {
                        state
                            .writer
                            .upsert(status_row(image, EnrichmentState::Failed, state.stamp), None)
                            .map_err(|e| e.to_string())?;
                    }
                    if want_clip && let Some(clip_stamp) = state.gates.clip_stamp {
                        state
                            .writer
                            .upsert_clip(image.path.clone(), clip_stamp.to_string(), None)
                            .map_err(|e| e.to_string())?;
                    }
                }
            }
        }
    }
    state.progress.report(EnrichProgress {
        done,
        total: state.total,
        bytes_done,
        bytes_total: state.bytes_total,
    });
    Ok(())
}

#[cfg(test)]
mod tests;
