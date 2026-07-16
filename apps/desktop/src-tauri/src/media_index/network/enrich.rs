//! The network enrichment core: fetch each opted-in image's bytes off the mount and
//! OCR them, conservatively (idle-gated, bandwidth-bounded, override-gated) and
//! crash/unmount-safely (resumable, disconnect-paused). Split out of the scheduler so
//! this I/O-shaped logic is directly testable with a fake fetcher + fake backend, no
//! real mount and no FFI (mirroring the local [`super::super::scheduler::enrich`]).
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
use std::time::Duration;

use crate::media_index::backend::{ImageInput, VisionBackend};
use crate::media_index::progress::{EnrichProgress, EnrichProgressSink};
use crate::media_index::scheduler::enrich::{ImageEntry, PassSummary, apply_media_upsert, gc_targets, status_row};
use crate::media_index::store::{EnrichmentState, MediaStatusRow, needs_clip, needs_enrichment};
use crate::media_index::writer::MediaWriter;

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
    /// The OCR backend (real Vision, or a fake in tests).
    pub(crate) backend: &'a dyn VisionBackend,
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
    /// upsert (the in-flight-analyze TOCTOU close).
    pub(crate) is_excluded: &'a dyn Fn(&str) -> bool,
    /// The emergency-stop check (memory watchdog), checked between images.
    pub(crate) cancel: &'a dyn Fn() -> bool,
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

/// Run one conservative network enrichment pass. See the module docs for the
/// data-safety contract.
pub(crate) fn enrich_network_and_gc(ctx: &NetworkEnrichCtx) -> Result<NetworkPassOutcome, String> {
    let stamp = ctx.backend.analysis_stamp();
    // The GC "current" set is the FULL walked identity set (not just freshly enriched),
    // so a present-but-deferred image is never GC'd.
    let current: HashSet<String> = ctx.images.iter().map(|i| i.path.clone()).collect();
    let mut summary = PassSummary::default();

    // The honest progress denominator: the enrichable subset (images passing
    // the coverage gates over their OS path), never the full walked set — a NAS archive
    // mostly deferred below the slider would otherwise stick the bar. `done` counts every
    // subset image the pass finishes handling (enriched, already-current, vanished, or
    // oversized skips), so a completed pass reaches `total`.
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
        // Yield promptly to the memory watchdog and to foreground activity.
        if (ctx.cancel)() {
            return finish_paused(ctx, summary, PauseReason::Cancelled);
        }
        if !(ctx.is_idle)() {
            return finish_paused(ctx, summary, PauseReason::NotIdle);
        }

        let os_path = os_join(ctx.mount_root, &image.path);
        // Privacy veto (LIVE hard veto, beats coverage): an excluded image is deferred,
        // so it stays in `current` and GC never wipes it. Not in the enrichable subset.
        if (ctx.is_excluded)(&os_path) {
            continue;
        }
        // Coverage gate: a low-importance NAS folder defers unless the user marked it
        // "always index". Not in the subset.
        if !(ctx.should_enrich)(&os_path) {
            continue;
        }
        // In the enrichable subset ⇒ count it as processed no matter the outcome.
        done += 1;
        bytes_done += image.size.unwrap_or(0);

        // Two-part path-keyed staleness: fetch + analyze when the Vision side OR the CLIP
        // side is stale; unchanged images are skipped but still count as processed.
        let stored = ctx.statuses.get(&image.path);
        let want_vision = needs_enrichment(stored, image.mtime, image.size, &stamp);
        let want_clip = needs_clip(stored, ctx.clip_stamp);
        if want_vision || want_clip {
            match ctx.fetcher.fetch(&os_path, ctx.policy.read_timeout) {
                Ok(bytes) => {
                    let fetched = bytes.len() as u64;
                    let input = ImageInput {
                        path: image.path.clone(),
                        kind: image.kind,
                        bytes: Some(bytes),
                    };
                    // ONE decode runs the requested side(s) from the fetched bytes.
                    let analysis = ctx.backend.analyze_media(&input, want_vision, want_clip);
                    // Re-check the LIVE veto AFTER the slow analyze: an exclusion landing
                    // during it must not persist a row (the in-flight-analyze TOCTOU). The
                    // bytes were already fetched, so still throttle below for honest
                    // bandwidth accounting; only the upsert is skipped.
                    if !(ctx.is_excluded)(&os_path) {
                        match analysis {
                            Ok(media) => {
                                if apply_media_upsert(ctx.writer, image, &stamp, ctx.clip_stamp, want_vision, media)? {
                                    summary.enriched += 1;
                                }
                            }
                            Err(e) => {
                                // A GOOD read but a bad decode/analysis ⇒ a genuinely bad
                                // file ⇒ `Failed` (same as the local pass). NOT a disconnect.
                                log::warn!(target: "media_index", "network analysis failed for '{}': {e}", image.path);
                                if want_vision {
                                    ctx.writer
                                        .upsert(status_row(image, EnrichmentState::Failed, &stamp), None)
                                        .map_err(|e| e.to_string())?;
                                }
                                if want_clip
                                    && let Some(clip_stamp) = ctx.clip_stamp
                                {
                                    ctx.writer
                                        .upsert_clip(image.path.clone(), clip_stamp.to_string(), None)
                                        .map_err(|e| e.to_string())?;
                                }
                            }
                        }
                    }
                    // Bandwidth throttle: hold the sustained fetch rate under the cap.
                    (ctx.sleep)(throttle_delay(fetched, ctx.policy.max_bytes_per_sec));
                }
                Err(FetchError::Disconnected(msg)) => {
                    // The mount went away: PAUSE. Keep completed rows, write no `Failed`
                    // for this image, and skip GC entirely (a disconnect is not a
                    // completed-scan deletion — plan Decision 3).
                    log::info!(
                        target: "media_index",
                        "network enrichment of '{}' paused (disconnected): {msg}", ctx.volume_id
                    );
                    return finish_paused(ctx, summary, PauseReason::Disconnected);
                }
                Err(FetchError::NotFound) => {
                    // The source vanished between the walk and the fetch: skip it (a
                    // completed scan's GC collects it). Never `Failed`. Already counted.
                }
                Err(FetchError::TooLarge) => {
                    log::debug!(target: "media_index", "network enrichment skips oversized '{}'", image.path);
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

    // Completed the walk ⇒ deletion-driven GC is safe here (see module docs). A paused
    // pass returns above and NEVER reaches this.
    let targets = gc_targets(ctx.statuses.keys(), &current);
    summary.gc_count = targets.len();
    ctx.writer.gc_paths(targets).map_err(|e| e.to_string())?;
    ctx.writer.flush_blocking().map_err(|e| e.to_string())?;
    Ok(NetworkPassOutcome::Completed(summary))
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
