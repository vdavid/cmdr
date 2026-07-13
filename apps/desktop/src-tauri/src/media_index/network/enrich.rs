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
use crate::media_index::scheduler::enrich::{ImageEntry, PassSummary, gc_targets, status_row};
use crate::media_index::store::{EnrichmentState, MediaStatusRow, needs_enrichment};
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
    /// Whether an image at this OS path should enrich (override / importance gate).
    pub(crate) should_enrich: &'a dyn Fn(&str) -> bool,
    /// The emergency-stop check (memory watchdog), checked between images.
    pub(crate) cancel: &'a dyn Fn() -> bool,
    /// The bandwidth-throttle sleep (real `thread::sleep` in production; a recorder /
    /// no-op in tests so the pass never actually sleeps).
    pub(crate) sleep: &'a dyn Fn(Duration),
}

/// Run one conservative network enrichment pass. See the module docs for the
/// data-safety contract.
pub(crate) fn enrich_network_and_gc(ctx: &NetworkEnrichCtx) -> Result<NetworkPassOutcome, String> {
    let engine = ctx.backend.engine_version();
    // The GC "current" set is the FULL walked identity set (not just freshly enriched),
    // so a present-but-deferred image is never GC'd.
    let current: HashSet<String> = ctx.images.iter().map(|i| i.path.clone()).collect();
    let mut summary = PassSummary::default();

    for image in ctx.images {
        // Yield promptly to the memory watchdog and to foreground activity.
        if (ctx.cancel)() {
            return finish_paused(ctx, summary, PauseReason::Cancelled);
        }
        if !(ctx.is_idle)() {
            return finish_paused(ctx, summary, PauseReason::NotIdle);
        }

        let os_path = os_join(ctx.mount_root, &image.path);
        // Override / importance gate: a low-importance NAS folder defers unless the
        // user marked it "always index".
        if !(ctx.should_enrich)(&os_path) {
            continue;
        }
        // Path-keyed staleness: unchanged images (same mtime/size/engine) are skipped.
        if !needs_enrichment(ctx.statuses.get(&image.path), image.mtime, image.size, &engine) {
            continue;
        }

        match ctx.fetcher.fetch(&os_path, ctx.policy.read_timeout) {
            Ok(bytes) => {
                let fetched = bytes.len() as u64;
                let input = ImageInput {
                    path: image.path.clone(),
                    kind: image.kind,
                    bytes: Some(bytes),
                };
                match ctx.backend.ocr(&input) {
                    Ok(result) => {
                        ctx.writer
                            .upsert(status_row(image, EnrichmentState::Done, &engine), Some(result.text))
                            .map_err(|e| e.to_string())?;
                        summary.enriched += 1;
                    }
                    Err(e) => {
                        // A GOOD read but a bad decode/OCR ⇒ a genuinely bad file ⇒
                        // `Failed` (same as the local pass). NOT a disconnect.
                        log::warn!(target: "media_index", "network OCR failed for '{}': {e}", image.path);
                        ctx.writer
                            .upsert(status_row(image, EnrichmentState::Failed, &engine), None)
                            .map_err(|e| e.to_string())?;
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
                // completed scan's GC collects it). Never `Failed`.
                continue;
            }
            Err(FetchError::TooLarge) => {
                log::debug!(target: "media_index", "network enrichment skips oversized '{}'", image.path);
                continue;
            }
        }
    }

    // Completed the walk ⇒ deletion-driven GC is safe here (see module docs). A paused
    // pass returns above and NEVER reaches this.
    let targets = gc_targets(ctx.statuses.keys(), &current);
    summary.gc_count = targets.len();
    ctx.writer.gc_paths(targets).map_err(|e| e.to_string())?;
    ctx.writer.flush_blocking().map_err(|e| e.to_string())?;
    Ok(NetworkPassOutcome::Completed(summary))
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
