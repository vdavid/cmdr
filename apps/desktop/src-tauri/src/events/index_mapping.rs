//! The wire format for everything the index subsystems report.
//!
//! The drive index, media index, and importance subsystems emit a typed
//! `IndexEvent` into an injected `EventSink` and name no wire format. This module
//! is the other half: the Tauri payload structs the frontend subscribes to, the
//! `TauriEventSink` that fills them in, and the two host-side destinations that
//! aren't frontend events at all (the error-report pipeline and the
//! restricted-path set).
//!
//! Every payload derives `tauri_specta::Event` with a pinned kebab `event_name`
//! (the `…Event` suffix wouldn't kebab-case to the wire string) and is registered
//! in `ipc.rs`'s `collect_events!`. Add a new event in both places or the frontend
//! never sees it.
//!
//! The data types the payloads carry (`ScanRunKind`, `RescanReason`,
//! `ActivityPhase`, `Freshness`, `AggregationPhase`, `MediaEnrichTerminalReason`,
//! `IndexFailure`, `MemoryWatchdogAction`) stay with the subsystems: a schema
//! derive on a value is fine there, a presentation decision isn't. Only the
//! envelope lives here.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use tauri_specta::Event;

use cmdr_index::AggregationPhase;
use cmdr_index::Freshness;
use cmdr_index::media_index::events::MediaEnrichTerminalReason;
use cmdr_index::{
    ActivityPhase, Diagnostic, EventSink, IndexErrorReport, IndexEvent, IndexEventKind, MemoryWatchdogAction,
    RescanReason, ScanRunKind,
};

use walk_announcer::WalkAnnouncer;

mod walk_announcer;

#[cfg(test)]
mod tests;

// ── Drive-index payloads ─────────────────────────────────────────────

/// A volume's full scan started.
///
/// Carries the static per-scan calibration, so the frontend's calibrated-vs-rough
/// progress tier is a pure function of this one event and the 500 ms progress
/// event can stay counters-only.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[tauri_specta(event_name = "index-scan-started")]
#[serde(rename_all = "camelCase")]
pub struct IndexScanStartedEvent {
    /// The volume being scanned.
    pub volume_id: String,
    /// What kind of run this is, so the run-kind header and its per-step copy
    /// state what's happening instead of inferring it.
    pub scan_run_kind: ScanRunKind,
    /// The previous completed scan OF THIS RUN'S KIND: its final entry count, the
    /// tier-1 (calibrated) progress denominator. Falls back to the last scan of
    /// any kind, then to `None` (no calibration yet).
    pub prior_total_entries: Option<u64>,
    /// The same prior scan's wall-clock duration, used to seed the tier-1 ETA
    /// before the sliding window has samples. `None` when there's no calibration.
    pub prior_scan_duration_ms: Option<u64>,
    /// The scanned volume's used bytes at scan start, the tier-2 (rough,
    /// first-scan) progress denominator. `None` when the space-info fetch failed.
    pub volume_used_bytes: Option<u64>,
    /// Whether this run covers the drive branch by branch rather than walking it
    /// whole. It decides what the per-folder size hourglass reads: a whole-volume
    /// walk puts every folder on the drive in flux for the run's whole length,
    /// while a phased run puts only the ground the branch events name in flux.
    pub covered_in_phases: bool,
}

/// A branch of a drive went under the walker, and the folder sizes inside it (and
/// above it, since the roll-up repairs the ancestor chain) can move until it's
/// done.
///
/// Held back by one second app-side, so a walk that finishes inside a second never
/// flashes anything (`walk_announcer.rs`). Only a run that announced itself with
/// `coveredInPhases` sends these.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[tauri_specta(event_name = "index-coverage-branch-started")]
#[serde(rename_all = "camelCase")]
pub struct IndexCoverageBranchStartedEvent {
    /// The volume being covered.
    pub volume_id: String,
    /// The roots under the walker, absolute in the volume's own path space.
    pub roots: Vec<String>,
}

/// A branch stopped being walked, on every exit path (covered, left to another
/// walk, or cancelled). Never held back, so a row can't keep an hourglass for a
/// walk that stopped.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[tauri_specta(event_name = "index-coverage-branch-ended")]
#[serde(rename_all = "camelCase")]
pub struct IndexCoverageBranchEndedEvent {
    /// The volume being covered.
    pub volume_id: String,
    /// The roots that were under the walker.
    pub roots: Vec<String>,
}

/// The running scan's moving counters, on the reporter's 500 ms tick.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[tauri_specta(event_name = "index-scan-progress")]
#[serde(rename_all = "camelCase")]
pub struct IndexScanProgressEvent {
    /// The volume being scanned.
    pub volume_id: String,
    /// Entries walked so far.
    pub entries_scanned: u64,
    /// Directories found so far.
    pub dirs_found: u64,
    /// Resolved post-dedup physical bytes scanned so far, the tier-2 progress
    /// numerator (apples-to-apples with `volume_used_bytes`).
    pub bytes_scanned: u64,
}

/// A scan finished cleanly.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[tauri_specta(event_name = "index-scan-complete")]
#[serde(rename_all = "camelCase")]
pub struct IndexScanCompleteEvent {
    /// The volume that finished scanning.
    pub volume_id: String,
    /// Entries in the finished index.
    pub total_entries: u64,
    /// Directories in the finished index.
    pub total_dirs: u64,
    /// How long the scan took.
    pub duration_ms: u64,
}

/// A scan ended WITHOUT completing: a network (SMB/MTP) scan that disconnected,
/// was canceled, timed out, or otherwise aborted.
///
/// Unlike `index-scan-complete`, this writes no completion facts (the partial
/// isn't a finished index). It exists purely so the frontend clears the volume's
/// live activity, so an aborted scan doesn't leave a stuck "scanning" row in the
/// corner indicator or the breadcrumb badge tooltip.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[tauri_specta(event_name = "index-scan-aborted")]
#[serde(rename_all = "camelCase")]
pub struct IndexScanAbortedEvent {
    /// The volume whose scan ended.
    pub volume_id: String,
}

/// These directories' recursive sizes changed, so any listing showing them is
/// stale.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[tauri_specta(event_name = "index-dir-updated")]
#[serde(rename_all = "camelCase")]
pub struct IndexDirUpdatedEvent {
    /// Absolute paths, in the listing's path space.
    pub paths: Vec<String>,
}

/// Journal replay is working through the backlog.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[tauri_specta(event_name = "index-replay-progress")]
#[serde(rename_all = "camelCase")]
pub struct IndexReplayProgressEvent {
    /// The volume being replayed.
    pub volume_id: String,
    /// Events applied so far.
    pub events_processed: u64,
    /// An approximate total (not every ID in the range belongs to this volume).
    pub estimated_total: Option<u64>,
}

/// Journal replay finished.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[tauri_specta(event_name = "index-replay-complete")]
#[serde(rename_all = "camelCase")]
pub struct IndexReplayCompleteEvent {
    /// The volume that finished replaying.
    pub volume_id: String,
    /// How long replay took.
    pub duration_ms: u64,
}

/// A full rescan was triggered instead of incremental replay, so the UI can show
/// a transparent toast.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[tauri_specta(event_name = "index-rescan-notification")]
#[serde(rename_all = "camelCase")]
pub struct IndexRescanNotificationEvent {
    /// The volume being rescanned.
    pub volume_id: String,
    /// The typed trigger; the frontend maps it to its own copy.
    pub reason: RescanReason,
    /// Details for logs, never rendered. The frontend handler reads `reason` and
    /// resolves a message key from it; this field rides along for the console and
    /// for support transcripts.
    pub details: String,
}

/// A writer's aggregation pass moved through one of its phases.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[tauri_specta(event_name = "index-aggregation-progress")]
#[serde(rename_all = "camelCase")]
pub struct AggregationProgressEvent {
    /// The volume whose writer is aggregating. The writer is spawned per volume,
    /// so the frontend can attribute progress to the right drive even when two
    /// volumes aggregate concurrently.
    pub volume_id: String,
    /// One of `aggregation_phase_name`'s outputs: `saving_entries` | `loading` |
    /// `sorting` | `computing` | `writing`.
    pub phase: String,
    /// Units done in this phase.
    pub current: u64,
    /// Units total in this phase.
    pub total: u64,
}

/// A full-scan aggregation pass finished and the UI can dismiss the progress
/// overlay. Carries the `volume_id` so the frontend clears the right drive's
/// aggregation row (two volumes can aggregate concurrently).
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[tauri_specta(event_name = "index-aggregation-complete")]
#[serde(rename_all = "camelCase")]
pub struct IndexAggregationCompleteEvent {
    /// The volume that finished aggregating.
    pub volume_id: String,
}

/// The memory watchdog stopped indexing to avoid a system crash, or memory kept
/// climbing after that stop. Drives a user-visible toast.
///
/// Byte-precise on purpose: whole-GB rounding turned a 16.9 GB reading into
/// "16 GB" in shipped reports, and that lost detail is exactly what an incident
/// needs. The two allocator figures are disjoint and neither is "the heap" on its
/// own; `cmdr_fs::process_memory` explains why.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[tauri_specta(event_name = "index-memory-warning")]
#[serde(rename_all = "camelCase")]
pub struct IndexMemoryWarningEvent {
    /// `phys_footprint` at the time, in bytes: the machine-pressure metric the
    /// watchdog thresholds key on (what Activity Monitor shows, what jetsam
    /// watches).
    pub phys_footprint_bytes: u64,
    /// Resident set size (RSS) at the time, in bytes. Counts graphics and shared
    /// mappings `phys_footprint` excludes, so it's context, not the trigger.
    pub resident_bytes: u64,
    /// Bytes mimalloc (our global allocator, so all Rust allocation including
    /// indexing) has committed.
    pub rust_heap_bytes: u64,
    /// Bytes the system malloc zones hold: WebKit, Objective-C, and C libraries.
    /// Does NOT include the Rust heap above.
    pub system_malloc_bytes: u64,
    /// `phys_footprint` minus both allocators: graphics surfaces, mapped files,
    /// thread stacks, and anything neither allocator accounts for.
    pub untracked_bytes: u64,
    /// What the watchdog did.
    pub action: MemoryWatchdogAction,
}

/// A volume's freshness changed to a NEW value (blue/green/yellow transitions).
///
/// Drives the per-drive freshness UX: the always-visible badge refreshes, and the
/// frontend's one-time stale dialog fires on the exact Fresh→Stale edge. Emitted
/// only when the value actually changes, so the frontend can subscribe rather
/// than poll.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[tauri_specta(event_name = "index-freshness-changed")]
#[serde(rename_all = "camelCase")]
pub struct IndexFreshnessChangedEvent {
    /// The volume whose badge changes.
    pub volume_id: String,
    /// The new freshness.
    pub freshness: Freshness,
}

/// A volume's top-level indexing phase changed (a step in the
/// `Scanning → Aggregating → Reconciling → Live` pipeline, plus `Replaying` and
/// `Idle`).
///
/// This is the PER-VOLUME counterpart to the global debug phase timeline, which
/// records ONE app-wide journal for the debug window and can't attribute a phase
/// to a drive when two volumes index at once. This event carries the `volumeId`,
/// so the frontend's per-volume step checklist can advance the right drive.
///
/// It fires only on TRANSITIONS, so a frontend that joins mid-scan (a window
/// reload) can't learn the current phase from it. The frontend backfills the
/// observable steps from the scan/aggregation activity instead; the reconcile step
/// is the one transition with no other signal, so it's briefly unobservable after
/// a reload that lands mid-reconcile (an accepted, rare gap — see the frontend
/// `indexing/DETAILS.md`).
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[tauri_specta(event_name = "index-phase-changed")]
#[serde(rename_all = "camelCase")]
pub struct IndexPhaseChangedEvent {
    /// The volume that changed phase.
    pub volume_id: String,
    /// The phase it moved to.
    pub phase: ActivityPhase,
}

// ── Media-index payloads ─────────────────────────────────────────────

/// Throttled progress for one volume's image-enrichment pass.
///
/// `total` / `bytes_total` are the ENRICHABLE-subset denominators (images passing
/// the coverage gates), NEVER the full walked set: a raw walked-set denominator
/// rebuilds the never-finishes bug inside the indicator.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[tauri_specta(event_name = "media-enrich-progress")]
#[serde(rename_all = "camelCase")]
pub struct MediaEnrichProgressEvent {
    /// The volume being enriched.
    pub volume_id: String,
    /// Subset images processed so far (enriched, already-current, or quietly
    /// skipped).
    pub done: u64,
    /// Total images in the enrichable subset (the honest denominator).
    pub total: u64,
    /// Bytes processed so far.
    pub bytes_done: u64,
    /// Total bytes across the enrichable subset.
    pub bytes_total: u64,
}

/// An image-enrichment pass ended. EVERY pass exit emits exactly one, so the
/// indicator row never sticks at "enriching".
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[tauri_specta(event_name = "media-enrich-terminal")]
#[serde(rename_all = "camelCase")]
pub struct MediaEnrichTerminalEvent {
    /// The volume whose pass ended.
    pub volume_id: String,
    /// Why it ended.
    pub reason: MediaEnrichTerminalReason,
}

// ── The mapping ──────────────────────────────────────────────────────

/// Where one [`IndexEvent`] ends up.
///
/// Returned by [`route`] rather than looked up in a second table, so the wire
/// name a test reads is literally the name the payload carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Destination {
    /// A Tauri event under this wire name.
    Frontend(&'static str),
    /// The error-report pipeline (`log_error!` → the auto-dispatcher).
    ErrorReport,
    /// The restricted-path set behind the sidebar's "limited by macOS" styling.
    RestrictedPaths,
    /// Nothing but the anonymous first-index measurements, which the sink takes
    /// off this same stream before routing (`analytics/first_index.rs`). The
    /// frontend has no use for it: what it renders is the size that appears, not
    /// the marker behind it.
    AnalyticsOnly,
}

/// Emit `payload` if there's an app to emit it to, and report its wire name.
fn to_frontend<E>(app: Option<&AppHandle>, payload: E) -> Destination
where
    E: Event + Serialize + Clone,
{
    if let Some(app) = app {
        let _ = payload.emit(app);
    }
    Destination::Frontend(E::NAME)
}

/// The wire name of an aggregation phase. Pinned strings: the frontend's
/// "compute folder sizes" step keys off them.
fn aggregation_phase_name(phase: AggregationPhase) -> &'static str {
    match phase {
        AggregationPhase::SavingEntries => "saving_entries",
        AggregationPhase::LoadingDirectories => "loading",
        AggregationPhase::Sorting => "sorting",
        AggregationPhase::Computing => "computing",
        AggregationPhase::Writing => "writing",
    }
}

/// Turn one index event into whatever the host does with it, and say where it
/// went.
///
/// `app` is `None` in tests, which suppresses the Tauri emit and nothing else, so
/// a test still exercises the real routing (including the error-report and
/// restricted-path arms) without standing up an app.
pub(crate) fn route(event: IndexEvent, app: Option<&AppHandle>) -> Destination {
    match event {
        IndexEvent::ScanStarted {
            volume_id,
            run_kind,
            prior_total_entries,
            prior_scan_duration_ms,
            volume_used_bytes,
            covered_in_phases,
        } => to_frontend(
            app,
            IndexScanStartedEvent {
                volume_id,
                scan_run_kind: run_kind,
                prior_total_entries,
                prior_scan_duration_ms,
                volume_used_bytes,
                covered_in_phases,
            },
        ),
        IndexEvent::CoverageBranchStarted { volume_id, roots } => {
            to_frontend(app, IndexCoverageBranchStartedEvent { volume_id, roots })
        }
        IndexEvent::CoverageBranchEnded { volume_id, roots } => {
            to_frontend(app, IndexCoverageBranchEndedEvent { volume_id, roots })
        }
        IndexEvent::ScanProgress {
            volume_id,
            entries_scanned,
            dirs_found,
            bytes_scanned,
        } => to_frontend(
            app,
            IndexScanProgressEvent {
                volume_id,
                entries_scanned,
                dirs_found,
                bytes_scanned,
            },
        ),
        IndexEvent::ScanComplete {
            volume_id,
            total_entries,
            total_dirs,
            duration_ms,
        } => to_frontend(
            app,
            IndexScanCompleteEvent {
                volume_id,
                total_entries,
                total_dirs,
                duration_ms,
            },
        ),
        IndexEvent::HomeCovered { .. } => Destination::AnalyticsOnly,
        IndexEvent::ScanAborted { volume_id } => to_frontend(app, IndexScanAbortedEvent { volume_id }),
        IndexEvent::DirsUpdated { paths } => to_frontend(app, IndexDirUpdatedEvent { paths }),
        IndexEvent::ReplayProgress {
            volume_id,
            events_processed,
            estimated_total,
        } => to_frontend(
            app,
            IndexReplayProgressEvent {
                volume_id,
                events_processed,
                estimated_total,
            },
        ),
        IndexEvent::ReplayComplete { volume_id, duration_ms } => {
            to_frontend(app, IndexReplayCompleteEvent { volume_id, duration_ms })
        }
        IndexEvent::RescanScheduled {
            volume_id,
            reason,
            details,
        } => to_frontend(
            app,
            IndexRescanNotificationEvent {
                volume_id,
                reason,
                details: details.0,
            },
        ),
        IndexEvent::AggregationProgress {
            volume_id,
            phase,
            current,
            total,
        } => to_frontend(
            app,
            AggregationProgressEvent {
                volume_id,
                phase: aggregation_phase_name(phase).to_string(),
                current,
                total,
            },
        ),
        IndexEvent::AggregationComplete { volume_id } => to_frontend(app, IndexAggregationCompleteEvent { volume_id }),
        IndexEvent::MemoryWarning {
            phys_footprint_bytes,
            resident_bytes,
            rust_heap_bytes,
            system_malloc_bytes,
            untracked_bytes,
            action,
        } => to_frontend(
            app,
            IndexMemoryWarningEvent {
                phys_footprint_bytes,
                resident_bytes,
                rust_heap_bytes,
                system_malloc_bytes,
                untracked_bytes,
                action,
            },
        ),
        IndexEvent::FreshnessChanged { volume_id, freshness } => {
            to_frontend(app, IndexFreshnessChangedEvent { volume_id, freshness })
        }
        IndexEvent::PhaseChanged { volume_id, phase } => to_frontend(app, IndexPhaseChangedEvent { volume_id, phase }),
        IndexEvent::MediaEnrichProgress {
            volume_id,
            done,
            total,
            bytes_done,
            bytes_total,
        } => to_frontend(
            app,
            MediaEnrichProgressEvent {
                volume_id,
                done,
                total,
                bytes_done,
                bytes_total,
            },
        ),
        IndexEvent::MediaEnrichTerminal { volume_id, reason } => {
            to_frontend(app, MediaEnrichTerminalEvent { volume_id, reason })
        }
        IndexEvent::Error { report } => {
            raise(&report);
            Destination::ErrorReport
        }
        IndexEvent::PathAccessDenied { path } => {
            crate::restricted_paths::record_denial(&path);
            Destination::RestrictedPaths
        }
    }
}

/// Raise a subsystem failure through the app's error-report pipeline.
///
/// `log_error!` is a crate-root macro, so a subsystem that can't name the app
/// can't invoke it. Routing the failure here instead keeps it in the same
/// pipeline an app-side failure uses: an error line, a backtrace record, and
/// `auto_dispatcher::on_error_logged`. The backtrace is still the failure's, not
/// this function's — `emit` is a synchronous call from the failing code.
///
/// The wording lives here because it's the sentence a human reads in a report;
/// the subsystems ship the numbers.
fn raise(report: &IndexErrorReport) {
    match report {
        IndexErrorReport::MemoryWatchdog {
            action,
            phys_footprint_bytes,
            limit_bytes,
            growth_since_stop_bytes,
            escalation,
            snapshot,
        } => {
            let breakdown = snapshot.as_ref().map(Diagnostic::as_str).unwrap_or_default();
            match action {
                MemoryWatchdogAction::StoppedIndexing => crate::log_error!(
                    target: MEMORY_WATCHDOG_TARGET,
                    "Memory watchdog: phys_footprint {:.2} GB exceeded the {} GB safety limit. \
                     Stopping all indexing to prevent a system crash.\n{}",
                    gb(*phys_footprint_bytes),
                    limit_bytes / (1024 * 1024 * 1024),
                    breakdown,
                ),
                MemoryWatchdogAction::StillGrowingAfterStop => crate::log_error!(
                    target: MEMORY_WATCHDOG_TARGET,
                    "Memory watchdog: phys_footprint is STILL climbing {:.2} GB after all indexing was stopped \
                     (now {:.2} GB, escalation #{}). The stop didn't hold, so the growth is not (only) the index scan.\n{}",
                    gb(growth_since_stop_bytes.unwrap_or(0)),
                    gb(*phys_footprint_bytes),
                    escalation.unwrap_or(0),
                    breakdown,
                ),
            }
        }
        IndexErrorReport::StorageFailed {
            failure,
            context,
            detail,
        } => crate::log_error!(
            target: STORAGE_TARGET,
            "Index storage failed ({context}): SQLite code {}/{}, stopping this volume's index: {detail}",
            failure.code,
            failure.extended_code,
        ),
        IndexErrorReport::LiveEventLoopUnavailable { detail } => crate::log_error!(
            target: LIVE_EVENT_LOOP_TARGET,
            "Live event loop: failed to open read connection after retries, live indexing disabled: {detail}"
        ),
        IndexErrorReport::WalkWorkerSpawnFailed { detail } => {
            crate::log_error!(target: WALKER_TARGET, "failed to spawn walk worker: {detail}")
        }
    }
}

/// Report categories. The auto-dispatcher groups by these, so they're stable
/// strings rather than `module_path!()` (which would say "events::index_mapping"
/// for every index failure and collapse four distinct incidents into one).
const MEMORY_WATCHDOG_TARGET: &str = "cmdr::indexing::memory_watchdog";
const STORAGE_TARGET: &str = "cmdr::indexing::store";
const LIVE_EVENT_LOOP_TARGET: &str = "cmdr::indexing::watch::live";
const WALKER_TARGET: &str = "cmdr::indexing::scanner::walker";

/// Bytes as gigabytes, for the watchdog's prose.
fn gb(bytes: u64) -> f64 {
    bytes as f64 / (1024.0 * 1024.0 * 1024.0)
}

/// The production sink: renders each index event into its Tauri payload and
/// emits it, or routes it to the host machinery that isn't the frontend.
pub struct TauriEventSink {
    app: AppHandle,
    /// The one event pair this sink doesn't forward on sight. See
    /// `walk_announcer.rs` for why a walk waits a second before it's news.
    announcer: Arc<WalkAnnouncer>,
}

impl TauriEventSink {
    /// A sink emitting over `app`.
    pub fn new(app: AppHandle) -> Self {
        let announcer_app = app.clone();
        Self {
            app,
            announcer: WalkAnnouncer::new(Arc::new(move |event| {
                route(event, Some(&announcer_app));
            })),
        }
    }
}

impl EventSink for TauriEventSink {
    fn emit(&self, event: IndexEvent) {
        match event.kind() {
            // The debounced pair. `observe` forwards through `route` itself, on
            // its own schedule.
            IndexEventKind::CoverageBranchStarted | IndexEventKind::CoverageBranchEnded => {
                self.announcer.observe(event);
                return;
            }
            // A run's terminal events. The machine ends every branch it starts, so
            // this normally finds nothing; it's here so no path can leave a volume
            // holding an hourglass for a walk whose run is over. Before the
            // terminal event, because the frontend drops the volume's branch state
            // on it.
            IndexEventKind::ScanComplete | IndexEventKind::ScanAborted => {
                if let Some(volume_id) = event.volume_id() {
                    self.announcer.run_ended(volume_id);
                }
            }
            _ => {}
        }
        // What a phased first index actually delivers, timed off this same
        // stream. Before the routing, so an event the routing consumes is still
        // ours to read.
        crate::analytics::first_index::observe(&event);
        // allowed-discarded-outcome: `Destination` says where an event went, and exists so a test can assert it without an app. The production sink is the end of the line: nobody above it asks.
        route(event, Some(&self.app));
    }
}
