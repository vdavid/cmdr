//! `ScanProgressReporter`: the 500 ms progress + mid-scan partial-aggregation
//! tick loop, shared by EVERY scan path (local guarded-walker fresh/reconcile, SMB/MTP
//! trait fresh/reconcile).
//!
//! Each scan path spawns one of these alongside its scan thread so the
//! coordinator reads as "dispatch scanner → await completion → spawn live loop"
//! without an inlined polling loop. Each 500 ms tick emits an
//! `index-scan-progress` event and, on every `PARTIAL_AGG_TICK_INTERVAL`-th tick
//! (and only while the writer isn't backed up), asks the writer to compute a
//! bounded subset of partial directory sizes so visible listings show growing
//! numbers during the scan. The partial-aggregation `source` is chosen by the
//! caller per scan kind (`Maps` for a fresh scan whose accumulator maps are
//! populated, `Sql` for a reconcile rescan where they're empty). The
//! send-decision and hot-path math live in [`super::partial_agg`]; this is the
//! dumb caller. The loop ends when the completion handler sets `scan_done`, so
//! partial passes are structurally scoped to the full-scan window.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use tauri::AppHandle;
use tauri::async_runtime::JoinHandle;
use tauri_specta::Event;

use super::events::IndexScanProgressEvent;
use super::partial_agg;
use super::routing;
use super::scanner::ScanProgress;
use super::writer::{IndexWriter, PartialAggSource, WriteMessage};
use crate::file_system::listing::caching;

/// Drives the periodic scan-progress events and mid-scan partial aggregation for
/// one full scan, on any scan path. Construct with [`ScanProgressReporter::new`],
/// then [`spawn`](ScanProgressReporter::spawn) the background loop.
///
/// The handles are kept by value (cloned in by the caller) so the spawned loop is
/// fully self-contained. `AppHandle` stays here rather than being abstracted
/// behind an emit closure: emitting `index-scan-progress` is the reporter's whole
/// reason to exist, and the genuinely pure decision logic already lives — and is
/// unit-tested — in `partial_agg`.
pub(super) struct ScanProgressReporter {
    /// The scan's live progress counters, snapshotted each tick.
    progress: Arc<ScanProgress>,
    /// Writer handle: the queue-depth gate plus the `ComputePartialAggregates` sink.
    writer: IndexWriter,
    /// App handle for emitting `index-scan-progress` (and, indirectly, the
    /// partial-aggregation `index-dir-updated` refresh the writer fires).
    app: AppHandle,
    /// The scanned volume's id: rides every event payload and filters hot paths.
    volume_id: String,
    /// Which source mid-scan partial aggregation computes from, chosen by the
    /// caller per scan kind: `Maps` for a fresh scan (accumulator maps populated
    /// by `InsertEntriesV2`), `Sql` for a reconcile rescan (maps empty, recompute
    /// from committed rows).
    partial_agg_source: PartialAggSource,
    /// Tick counter; gates partial-aggregation passes via `partial_agg`.
    tick: u64,
}

impl ScanProgressReporter {
    /// Create a reporter for a scan of `volume_id`. The writer and app handles are
    /// cloned in by the caller so the spawned loop owns everything it needs.
    /// `partial_agg_source` picks the mid-scan partial-aggregation source by scan
    /// kind (`Maps` fresh / `Sql` reconcile).
    pub(super) fn new(
        progress: Arc<ScanProgress>,
        writer: IndexWriter,
        app: AppHandle,
        volume_id: String,
        partial_agg_source: PartialAggSource,
    ) -> Self {
        Self {
            progress,
            writer,
            app,
            volume_id,
            partial_agg_source,
            tick: 0,
        }
    }

    /// Do one tick's work: emit a progress event, then (on an interval tick with a
    /// shallow writer queue) fire a partial-aggregation pass. The gating and
    /// hot-path collection live in the tested `partial_agg` helpers; this body
    /// just snapshots and sends.
    fn tick(&mut self) {
        let snap = self.progress.snapshot();
        let _ = IndexScanProgressEvent {
            volume_id: self.volume_id.clone(),
            entries_scanned: snap.entries_scanned,
            dirs_found: snap.dirs_found,
            bytes_scanned: snap.bytes_scanned,
        }
        .emit(&self.app);

        // Mid-scan partial aggregation: on the interval-th tick (and only when the
        // writer isn't backed up), ask the writer to compute and write a bounded
        // subset of partial dir sizes so visible listings show growing numbers
        // during the scan. The whole block sits behind the gate so skipped ticks
        // do zero extra work — which also makes disabling the feature a single
        // call-site toggle.
        self.tick += 1;
        if partial_agg::should_send_partial_agg(self.tick, self.writer.queue_depth()) {
            // Take the cheap, owned listing snapshot first and let its read lock
            // drop before any path work; don't hold a cross-subsystem lock through
            // normalization.
            let listings = caching::snapshot_listings();
            let hot_paths = partial_agg::collect_hot_paths(&listings, &self.volume_id);
            // Map the firmlink-normalized absolute hot paths into the volume's
            // index-relative space so the writer's `resolve_path_under(ROOT_ID, ..)`
            // resolves them: a pass-through for the local `root` (its index is
            // rooted at `/`), a mount-relative strip for SMB, and a scheme strip
            // for MTP. Reuses `routing::index_read_path` — the SAME transform
            // enrichment and the dir-stats queries use (single-source), so a
            // network hot path resolves identically here and on the read side. A
            // path that doesn't map (unknown volume, outside the mount) is dropped,
            // exactly like an unindexed listing.
            let hot_paths: Vec<String> = hot_paths
                .iter()
                .filter_map(|abs| routing::index_read_path(&self.volume_id, abs))
                .collect();
            match self.writer.try_send(WriteMessage::ComputePartialAggregates {
                hot_paths,
                source: self.partial_agg_source,
            }) {
                Ok(true) => {}
                Ok(false) => log::debug!("Partial aggregation pass dropped: writer channel full"),
                Err(e) => log::debug!("Partial aggregation send failed (writer gone): {e}"),
            }
        }
    }

    /// Spawn the 500 ms reporter loop. It ticks until `scan_done` is set by the
    /// scan-completion handler (or the task is aborted at shutdown). Uses
    /// `tauri::async_runtime::spawn` because a scan can start from the synchronous
    /// Tauri `setup()` hook where no Tokio runtime context exists.
    pub(super) fn spawn(mut self, scan_done: Arc<AtomicBool>) -> JoinHandle<()> {
        tauri::async_runtime::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_millis(500)).await;
                if scan_done.load(Ordering::Relaxed) {
                    break;
                }
                self.tick();
            }
        })
    }
}
