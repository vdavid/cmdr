//! The network enrichment core: fetch each opted-in image's bytes off the mount and
//! OCR them, conservatively (idle-gated, bandwidth-bounded, override-gated) and
//! crash/unmount-safely (resumable, disconnect-paused). Split out of the scheduler so
//! this I/O-shaped logic is directly testable with a fake fetcher + fake backend, no
//! real mount and no FFI (mirroring the local [`super::super::scheduler::enrich`]).
//!
//! ## Sequential vs parallel (plan M2)
//!
//! At one worker (the default) the pass is a plain sequential loop: fetch one image,
//! analyze it, write it, throttle, repeat — byte-for-byte the pre-M2 behavior. Above one
//! worker it becomes a producer/consumer: ONE fetcher thread keeps every conservative
//! fetch-side decision (idle-gate, disconnect→pause, bandwidth throttle, gates) and hands
//! fetched bytes to N compute workers (each its own Vision backend) over a channel, with
//! admission bounded by BYTES ([`ByteBudget`]) so the prefetch buffer can't blow the memory
//! ceiling on a RAW-heavy corpus. The wire is serialized on the one smb2 session either
//! way; the win is overlapping that serialized fetch with parallel compute.
//!
//! ## Data-safety lines (plan Decision 3 + Cross-cutting § Cancellation)
//!
//! - A **disconnect** (fetch timeout / I/O error) is NOT a bad file: the pass returns
//!   [`NetworkPassOutcome::Paused`], keeps every completed row, writes NO `Failed` row
//!   for the in-flight image, and does NOT GC. The volume resumes on reconnect.
//! - Only a pass that **ran to completion** GCs (a completed index scan drove it, so
//!   the tree is whole). A paused/cancelled pass never reaches GC — a mere disconnect
//!   can't wipe a volume's coverage.
//! - A genuinely bad file (a good read but a decode/OCR failure) DOES mark `Failed`,
//!   exactly as the local pass does — that's a real, per-file outcome, not a transport
//!   fault.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

use crate::ignore_poison::IgnorePoison;
use crate::media_index::backend::{ImageInput, VisionBackend};
use crate::media_index::progress::{EnrichProgress, EnrichProgressSink};
use crate::media_index::scheduler::enrich::{ImageEntry, PassSummary, apply_media_upsert, gc_targets, status_row};
use crate::media_index::scheduler::pool::{MakeBackend, WorkerCount};
use crate::media_index::store::{EnrichmentState, MediaStatusRow, needs_clip, needs_enrichment};
use crate::media_index::writer::MediaWriter;

use super::budget::ByteBudget;
use super::fetch::{ByteFetcher, FetchError, os_join};
use super::policy::{ConservativeFetchPolicy, throttle_delay};

/// Why a network pass stopped before finishing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PauseReason {
    /// The user became active; yield the wire and retry when idle again.
    NotIdle,
    /// The mount went away mid-pass (unmount). Resume on reconnect.
    Disconnected,
    /// The memory watchdog's emergency stop fired. Resume on the next scan.
    Cancelled,
}

/// The outcome of a network enrichment pass.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum NetworkPassOutcome {
    /// The pass fetched every eligible image and GC'd vanished rows.
    Completed(PassSummary),
    /// The pass stopped early. Completed rows are persisted; NO GC ran.
    Paused { summary: PassSummary, reason: PauseReason },
}

/// Everything one network pass needs. Bundled into a struct to keep the call honest
/// (and dodge `clippy::too_many_arguments`); the `dyn Fn` seams (`is_idle`,
/// `should_enrich`, `cancel`, `sleep`) keep the core testable over fakes with no clock
/// or globals.
pub(crate) struct NetworkEnrichCtx<'a> {
    /// The volume being enriched (for logging).
    pub(crate) volume_id: &'a str,
    /// The volume's OS mount root (`/Volumes/<share>`), prepended to each index-
    /// relative path to reach the real file. `/` for a local/root volume.
    pub(crate) mount_root: &'a str,
    /// The qualifying images from the (completed) index walk, index-relative.
    pub(crate) images: &'a [ImageEntry],
    /// The already-loaded stored statuses (path → row) for staleness + GC.
    pub(crate) statuses: &'a HashMap<String, MediaStatusRow>,
    /// The OCR backend (real Vision, or a fake in tests). Worker 0 / the sequential path
    /// use this; extra parallel workers build their own from [`make`](Self::make).
    pub(crate) backend: &'a dyn VisionBackend,
    /// Builds an INDEPENDENT backend for each extra parallel worker (plan M2). Never called
    /// at one worker (the sequential path uses [`backend`](Self::backend) only).
    pub(crate) make: &'a MakeBackend<'a>,
    /// The effective parallel worker count (the user's `mediaIndex.parallelism` capped by
    /// thermal pressure), read once at pass start to pick sequential vs parallel.
    pub(crate) workers: &'a WorkerCount<'a>,
    /// The byte-bounded prefetch admission gate: a fetcher acquires an image's byte size
    /// before reading and a worker releases it after the decode, so the prefetch buffer
    /// never exceeds the budget (plan M2 — bound by BYTES, not file count).
    pub(crate) budget: &'a ByteBudget,
    /// The byte fetcher (real `std::fs`-on-mount, or a scripted fake in tests).
    pub(crate) fetcher: &'a dyn ByteFetcher,
    /// The volume's media writer.
    pub(crate) writer: &'a MediaWriter,
    /// The conservative-fetch knobs.
    pub(crate) policy: &'a ConservativeFetchPolicy,
    /// Whether the app is idle enough to fetch right now (checked between images).
    pub(crate) is_idle: &'a dyn Fn() -> bool,
    /// Whether an image at this OS path is COVERED (override / importance gate,
    /// snapshot-based).
    pub(crate) should_enrich: &'a dyn Fn(&str) -> bool,
    /// The LIVE privacy veto at this OS path (read fresh, not snapshot): a hard veto
    /// that beats coverage, checked before enriching AND again immediately before the
    /// upsert (the in-flight-analyze TOCTOU close). `+ Sync` because parallel workers call
    /// it from their own threads.
    pub(crate) is_excluded: &'a (dyn Fn(&str) -> bool + Sync),
    /// The emergency-stop check (memory watchdog), checked between images. `+ Sync` for the
    /// parallel workers and the byte-budget wait.
    pub(crate) cancel: &'a (dyn Fn() -> bool + Sync),
    /// The bandwidth-throttle sleep (real `thread::sleep` in production; a recorder /
    /// no-op in tests so the pass never actually sleeps).
    pub(crate) sleep: &'a dyn Fn(Duration),
    /// The throttled progress sink (the top-right indicator's second publisher).
    /// A no-op in tests that don't assert progress.
    pub(crate) progress: &'a dyn EnrichProgressSink,
    /// The installed CLIP model's provenance stamp, or `None` when no model is installed —
    /// the CLIP half of two-part staleness (an opted-in NAS's photos become semantically
    /// searchable too). `None` ⇒ Vision-only, exactly like the local pass.
    pub(crate) clip_stamp: Option<&'a str>,
}

/// Run one conservative network enrichment pass. Dispatches to the sequential loop at one
/// worker (byte-for-byte the pre-M2 behavior) or the byte-bounded parallel producer/consumer
/// above one. See the module docs for the data-safety contract.
pub(crate) fn enrich_network_and_gc(ctx: &NetworkEnrichCtx) -> Result<NetworkPassOutcome, String> {
    if (ctx.workers)().max(1) == 1 {
        run_sequential(ctx)
    } else {
        run_parallel(ctx)
    }
}

/// The sequential (one-worker) pass: fetch, analyze, write, throttle, repeat. This IS the
/// pre-M2 behavior — every existing network test exercises it.
fn run_sequential(ctx: &NetworkEnrichCtx) -> Result<NetworkPassOutcome, String> {
    let stamp = ctx.backend.analysis_stamp();
    let cctx = ComputeCtx::from(ctx);
    let current: HashSet<String> = ctx.images.iter().map(|i| i.path.clone()).collect();
    let mut summary = PassSummary::default();

    let (total, bytes_total) = network_enrichable_totals(ctx);
    let mut done = 0u64;
    let mut bytes_done = 0u64;
    ctx.progress.report(EnrichProgress {
        done,
        total,
        bytes_done,
        bytes_total,
    });

    for image in ctx.images {
        if (ctx.cancel)() {
            return finish_paused(ctx, summary, PauseReason::Cancelled);
        }
        if !(ctx.is_idle)() {
            return finish_paused(ctx, summary, PauseReason::NotIdle);
        }

        let os_path = os_join(ctx.mount_root, &image.path);
        if (ctx.is_excluded)(&os_path) || !(ctx.should_enrich)(&os_path) {
            continue;
        }
        done += 1;
        bytes_done += image.size.unwrap_or(0);

        let stored = ctx.statuses.get(&image.path);
        let want_vision = needs_enrichment(stored, image.mtime, image.size, &stamp);
        let want_clip = needs_clip(stored, ctx.clip_stamp);
        if want_vision || want_clip {
            match ctx.fetcher.fetch(&os_path, image.size, ctx.policy.read_timeout) {
                Ok(bytes) => {
                    let fetched = bytes.len() as u64;
                    if compute_and_write(&cctx, ctx.backend, image, bytes, want_vision, want_clip, &stamp)? {
                        summary.enriched += 1;
                    }
                    // Bandwidth throttle: hold the sustained fetch rate under the cap.
                    (ctx.sleep)(throttle_delay(fetched, ctx.policy.max_bytes_per_sec));
                }
                Err(FetchError::Disconnected(msg)) => {
                    log::info!(
                        target: "media_index",
                        "network enrichment of '{}' paused (disconnected): {msg}", ctx.volume_id
                    );
                    return finish_paused(ctx, summary, PauseReason::Disconnected);
                }
                Err(FetchError::NotFound) => {}
                Err(FetchError::TooLarge) => {
                    log::debug!(target: "media_index", "network enrichment skips oversized '{}'", image.path);
                }
                // A per-file read failure (permission denied and friends) skips and
                // counts, NEVER pauses (that's reserved for a typed disconnect) and
                // never writes a row (`Failed` is for a good read with a bad decode).
                Err(FetchError::Unreadable(msg)) => {
                    summary.skipped_unreadable += 1;
                    log::debug!(target: "media_index", "network enrichment skips unreadable '{}': {msg}", image.path);
                }
            }
        }
        ctx.progress.report(EnrichProgress {
            done,
            total,
            bytes_done,
            bytes_total,
        });
    }

    // Completed the walk ⇒ deletion-driven GC is safe here (see module docs).
    let targets = gc_targets(ctx.statuses.keys(), &current);
    summary.gc_count = targets.len();
    ctx.writer.gc_paths(targets).map_err(|e| e.to_string())?;
    ctx.writer.flush_blocking().map_err(|e| e.to_string())?;
    Ok(NetworkPassOutcome::Completed(summary))
}

/// The minimal, all-`Sync` slice of the pass a COMPUTE worker needs: enough to analyze a
/// fetched image and persist it, without the non-`Sync` fetch-side seams (`is_idle`,
/// `should_enrich`, `sleep`) that stay on the fetcher thread. Lets the parallel workers
/// borrow only what's thread-safe. The sequential path builds one too, so
/// [`compute_and_write`] has ONE signature.
struct ComputeCtx<'a> {
    mount_root: &'a str,
    is_excluded: &'a (dyn Fn(&str) -> bool + Sync),
    writer: &'a MediaWriter,
    clip_stamp: Option<&'a str>,
}

impl<'a> ComputeCtx<'a> {
    fn from(ctx: &NetworkEnrichCtx<'a>) -> Self {
        Self {
            mount_root: ctx.mount_root,
            is_excluded: ctx.is_excluded,
            writer: ctx.writer,
            clip_stamp: ctx.clip_stamp,
        }
    }
}

/// One fetched image handed from the fetcher to a compute worker.
struct FetchedJob {
    image: ImageEntry,
    bytes: Vec<u8>,
    want_vision: bool,
    want_clip: bool,
}

/// The parallel (N-worker) pass: one fetcher thread makes every conservative fetch-side
/// decision and feeds fetched bytes to N compute workers over a bounded channel, admission
/// gated by the byte budget. The fetcher owns pause/disconnect/idle/throttle and the
/// progress denominator; workers own analyze + the TOCTOU veto + the write, and release the
/// byte budget once the decode is done. On any early stop (disconnect / not-idle / cancel)
/// the fetcher stops producing; workers drain what's already fetched (that work is valid and
/// kept), and NO GC runs. GC runs only after a clean, fully-fetched completion.
fn run_parallel(ctx: &NetworkEnrichCtx) -> Result<NetworkPassOutcome, String> {
    let stamp = ctx.backend.analysis_stamp();
    let cctx = ComputeCtx::from(ctx);
    let budget = ctx.budget;
    let current: HashSet<String> = ctx.images.iter().map(|i| i.path.clone()).collect();
    let n = (ctx.workers)().max(1);

    let (total, bytes_total) = network_enrichable_totals(ctx);
    ctx.progress.report(EnrichProgress {
        done: 0,
        total,
        bytes_done: 0,
        bytes_total,
    });

    // A small bounded channel: the byte budget is the real backpressure, this just caps the
    // handful of decoded-but-unprocessed jobs.
    let (tx, rx) = std::sync::mpsc::sync_channel::<FetchedJob>(n * 2);
    let rx = Mutex::new(rx);
    let enriched = AtomicUsize::new(0);
    // Only the fetcher thread observes fetch errors, but the count outlives the
    // thread scope, so it rides an atomic like `enriched`.
    let skipped_unreadable = AtomicUsize::new(0);
    // The first per-image writer error; fails the whole pass (as the sequential `?` does).
    let first_error: Mutex<Option<String>> = Mutex::new(None);
    // Workers set this on a writer error so the fetcher stops promptly.
    let worker_error = AtomicBool::new(false);

    // Extra backends for workers 1..n (worker 0 rides `ctx.backend`); owned here, borrowed by
    // the scoped worker threads.
    let extra: Vec<Arc<dyn VisionBackend>> = (1..n).map(|_| (ctx.make)()).collect();

    let pause_reason: Mutex<Option<PauseReason>> = Mutex::new(None);

    std::thread::scope(|scope| {
        // Compute workers.
        for id in 0..n {
            let backend: &dyn VisionBackend = if id == 0 { ctx.backend } else { extra[id - 1].as_ref() };
            let rx = &rx;
            let enriched = &enriched;
            let first_error = &first_error;
            let worker_error = &worker_error;
            let stamp = &stamp;
            let cctx = &cctx;
            scope.spawn(move || {
                loop {
                    // Take one job (releasing the lock before the slow analyze).
                    let job = {
                        let guard = rx.lock_ignore_poison();
                        guard.recv()
                    };
                    let Ok(job) = job else { break }; // fetcher dropped tx and the queue drained
                    let fetched = job.bytes.len() as u64;
                    let result = compute_and_write(
                        cctx,
                        backend,
                        &job.image,
                        job.bytes,
                        job.want_vision,
                        job.want_clip,
                        stamp,
                    );
                    // Release the byte budget once the decode has consumed the bytes.
                    budget.release(fetched);
                    match result {
                        Ok(true) => {
                            enriched.fetch_add(1, Ordering::AcqRel);
                        }
                        Ok(false) => {}
                        Err(e) => {
                            *first_error.lock_ignore_poison() = Some(e);
                            worker_error.store(true, Ordering::Release);
                            break;
                        }
                    }
                }
            });
        }

        // The fetcher (this thread): every conservative fetch-side decision stays here.
        let mut done = 0u64;
        let mut bytes_done = 0u64;
        let mut reason: Option<PauseReason> = None;
        for image in ctx.images {
            if worker_error.load(Ordering::Acquire) {
                break;
            }
            if (ctx.cancel)() {
                reason = Some(PauseReason::Cancelled);
                break;
            }
            if !(ctx.is_idle)() {
                reason = Some(PauseReason::NotIdle);
                break;
            }
            let os_path = os_join(ctx.mount_root, &image.path);
            if (ctx.is_excluded)(&os_path) || !(ctx.should_enrich)(&os_path) {
                continue;
            }
            done += 1;
            bytes_done += image.size.unwrap_or(0);

            let stored = ctx.statuses.get(&image.path);
            let want_vision = needs_enrichment(stored, image.mtime, image.size, &stamp);
            let want_clip = needs_clip(stored, ctx.clip_stamp);
            if want_vision || want_clip {
                // Admit this image's bytes against the prefetch budget before reading; a stop
                // wakes the wait so a stopping pass never blocks here.
                if !ctx.budget.acquire(image.size.unwrap_or(0), ctx.cancel) {
                    reason = Some(PauseReason::Cancelled);
                    break;
                }
                match ctx.fetcher.fetch(&os_path, image.size, ctx.policy.read_timeout) {
                    Ok(bytes) => {
                        let fetched = bytes.len() as u64;
                        // Reconcile the reservation (`image.size`) with the ACTUAL bytes read,
                        // so the worker's release balances exactly.
                        reconcile_budget(ctx.budget, image.size.unwrap_or(0), fetched, ctx.cancel);
                        (ctx.sleep)(throttle_delay(fetched, ctx.policy.max_bytes_per_sec));
                        if tx
                            .send(FetchedJob {
                                image: image.clone(),
                                bytes,
                                want_vision,
                                want_clip,
                            })
                            .is_err()
                        {
                            // All workers gone (a writer error): stop fetching.
                            ctx.budget.release(fetched);
                            break;
                        }
                    }
                    Err(FetchError::Disconnected(msg)) => {
                        ctx.budget.release(image.size.unwrap_or(0));
                        log::info!(
                            target: "media_index",
                            "network enrichment of '{}' paused (disconnected): {msg}", ctx.volume_id
                        );
                        reason = Some(PauseReason::Disconnected);
                        break;
                    }
                    Err(FetchError::NotFound) => {
                        ctx.budget.release(image.size.unwrap_or(0));
                    }
                    Err(FetchError::TooLarge) => {
                        ctx.budget.release(image.size.unwrap_or(0));
                        log::debug!(target: "media_index", "network enrichment skips oversized '{}'", image.path);
                    }
                    // Per-file read failure: skip-and-count, never a pause (see the
                    // sequential arm). Only the fetcher observes fetch errors, so a
                    // plain local counter is race-free.
                    Err(FetchError::Unreadable(msg)) => {
                        ctx.budget.release(image.size.unwrap_or(0));
                        skipped_unreadable.fetch_add(1, Ordering::Relaxed);
                        log::debug!(target: "media_index", "network enrichment skips unreadable '{}': {msg}", image.path);
                    }
                }
            }
            ctx.progress.report(EnrichProgress {
                done,
                total,
                bytes_done,
                bytes_total,
            });
        }
        // Drop tx so workers finish the drained queue and exit; the scope joins them.
        drop(tx);
        *pause_reason.lock_ignore_poison() = reason;
    });

    if let Some(err) = first_error.lock_ignore_poison().take() {
        return Err(err);
    }

    let mut summary = PassSummary {
        enriched: enriched.load(Ordering::Acquire),
        gc_count: 0,
        cancelled: false,
        skipped_unreadable: skipped_unreadable.load(Ordering::Relaxed),
    };
    let reason = *pause_reason.lock_ignore_poison();
    if let Some(reason) = reason {
        // A paused pass keeps its completed rows and NEVER GCs (a disconnect can't wipe
        // coverage). Flush what the workers wrote.
        ctx.writer.flush_blocking().map_err(|e| e.to_string())?;
        return Ok(NetworkPassOutcome::Paused { summary, reason });
    }

    // Clean completion ⇒ deletion-driven GC is safe (the whole index walk drove it).
    let targets = gc_targets(ctx.statuses.keys(), &current);
    summary.gc_count = targets.len();
    ctx.writer.gc_paths(targets).map_err(|e| e.to_string())?;
    ctx.writer.flush_blocking().map_err(|e| e.to_string())?;
    Ok(NetworkPassOutcome::Completed(summary))
}

/// Balance the byte budget when the ACTUAL fetched size differs from the reserved size
/// (`image.size` can be stale or absent): release an over-reservation, or acquire the extra
/// if the file grew. Keeps the budget's accounting exact so it neither leaks nor blocks.
fn reconcile_budget(budget: &ByteBudget, reserved: u64, actual: u64, cancel: &(dyn Fn() -> bool + Sync)) {
    match actual.cmp(&reserved) {
        std::cmp::Ordering::Less => budget.release(reserved - actual),
        std::cmp::Ordering::Greater => {
            // The bytes are already read, so acquire what we can without waiting past a stop.
            let _ = budget.acquire(actual - reserved, cancel);
        }
        std::cmp::Ordering::Equal => {}
    }
}

/// Analyze one fetched image and persist the result: run the requested side(s) over the
/// fetched bytes on `backend`, re-check the LIVE exclusion veto AFTER the slow analyze (the
/// in-flight-analyze TOCTOU close), and upsert on success or mark `Failed` on a
/// present-but-bad file. Returns whether a row was persisted (for the `enriched` counter).
/// Shared by the sequential and parallel paths so the per-image policy lives in ONE place.
fn compute_and_write(
    cctx: &ComputeCtx,
    backend: &dyn VisionBackend,
    image: &ImageEntry,
    bytes: Vec<u8>,
    want_vision: bool,
    want_clip: bool,
    stamp: &str,
) -> Result<bool, String> {
    let os_path = os_join(cctx.mount_root, &image.path);
    let input = ImageInput {
        path: image.path.clone(),
        kind: image.kind,
        bytes: Some(bytes),
    };
    // ONE decode runs the requested side(s) from the fetched bytes.
    let analysis = backend.analyze_media(&input, want_vision, want_clip);
    // Re-check the LIVE veto AFTER the slow analyze: an exclusion landing during it must not
    // persist a row (the in-flight-analyze TOCTOU).
    if (cctx.is_excluded)(&os_path) {
        return Ok(false);
    }
    match analysis {
        Ok(media) => apply_media_upsert(cctx.writer, image, stamp, cctx.clip_stamp, want_vision, media),
        Err(e) => {
            // A GOOD read but a bad decode/analysis ⇒ a genuinely bad file ⇒ `Failed` (same
            // as the local pass). NOT a disconnect.
            log::warn!(target: "media_index", "network analysis failed for '{}': {e}", image.path);
            if want_vision {
                cctx.writer
                    .upsert(status_row(image, EnrichmentState::Failed, stamp), None)
                    .map_err(|e| e.to_string())?;
            }
            if want_clip && let Some(clip_stamp) = cctx.clip_stamp {
                cctx.writer
                    .upsert_clip(image.path.clone(), clip_stamp.to_string(), None)
                    .map_err(|e| e.to_string())?;
            }
            Ok(false)
        }
    }
}

/// The ENRICHABLE-subset denominators for a network pass: the count and total bytes of
/// images passing the coverage gates over their OS path (`should_enrich` AND not
/// `is_excluded`), the honest progress denominator (never the full walked set).
fn network_enrichable_totals(ctx: &NetworkEnrichCtx) -> (u64, u64) {
    let mut total = 0u64;
    let mut bytes_total = 0u64;
    for image in ctx.images {
        let os_path = os_join(ctx.mount_root, &image.path);
        if !(ctx.is_excluded)(&os_path) && (ctx.should_enrich)(&os_path) {
            total += 1;
            bytes_total += image.size.unwrap_or(0);
        }
    }
    (total, bytes_total)
}

/// Flush the completed rows (so a pause never loses finished work) and return the
/// paused outcome. Deliberately skips GC.
fn finish_paused(
    ctx: &NetworkEnrichCtx,
    summary: PassSummary,
    reason: PauseReason,
) -> Result<NetworkPassOutcome, String> {
    ctx.writer.flush_blocking().map_err(|e| e.to_string())?;
    Ok(NetworkPassOutcome::Paused { summary, reason })
}
