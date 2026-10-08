//! The event seam between the index subsystems and whoever hosts them.
//!
//! The subsystems describe what happened as a typed [`IndexEvent`] and hand it to
//! an injected [`EventSink`]. They never name a wire format, an event name, or a
//! sentence a human reads: the host owns all three (`events/index_mapping.rs`
//! renders them into the Tauri payloads the frontend subscribes to).
//!
//! Two implementations ship: the app's `TauriEventSink` (production) and
//! `RecordingSink` (tests, in `test_sinks.rs`, which assert on the event stream instead of standing
//! up an app). [`NoopEventSink`] covers the handle-free callers that have nothing
//! to emit to.
//!
//! Modeled on `file_system/write_operations/event_sinks.rs`, with one difference:
//! that sink has a method per event because its payloads are app types; this one
//! has a single `emit` because the payload is a crate enum, which is what keeps
//! the wire format out of here.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::indexing::aggregator::AggregationPhase;
use crate::indexing::lifecycle::freshness::Freshness;
use crate::indexing::store::IndexFailure;

use super::payload::{
    ActivityPhase, CoveragePhase, FolderChangeRollup, MemoryWatchdogAction, RescanReason, ScanRunKind,
};

/// Why a volume's enrichment pass ended. A typed discriminant, never a string:
/// the frontend clears the indicator row on `Completed` /
/// `Cancelled` / `Failed` and re-voices it paused on the two pause reasons.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", rename_all_fields = "camelCase", tag = "kind")]
pub enum MediaEnrichTerminalReason {
    /// The pass enriched every eligible image and GC'd vanished rows.
    Completed {
        /// Images the pass enriched.
        enriched: u64,
        /// Rows it removed for images that no longer exist.
        gc_count: u64,
    },
    /// A network pass paused because the app is in use (resumes when idle again).
    PausedWaitingForIdle,
    /// A network pass paused because the volume disconnected (resumes on reconnect).
    PausedDisconnected,
    /// The memory watchdog stopped the pass (resumes on the next scan / re-enable).
    Cancelled,
    /// The pass bubbled an error (e.g. a writer-send failure). The row must still clear.
    Failed,
}

/// English prose the index produces for logs and error reports, never for the UI.
///
/// The newtype is the point: a bare `String` field leaves the next reader to guess
/// whether a human ever sees it, and the answer decides whether it needs
/// translating. Everything wrapped here is diagnostic, so it stays English and the
/// host is free to log it verbatim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic(pub String);

impl Diagnostic {
    /// The message, for a host that wants to log or attach it.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<String> for Diagnostic {
    fn from(s: String) -> Self {
        Self(s)
    }
}

/// A failure worth an error report, described by what broke rather than by the
/// sentence someone would write about it.
///
/// The host renders each variant into the line it logs and ships; keeping the
/// structure here is what lets it group, throttle, or re-word them without the
/// index knowing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IndexErrorReport {
    /// The memory watchdog crossed its safety limit and acted on it.
    MemoryWatchdog {
        /// What the watchdog did.
        action: MemoryWatchdogAction,
        /// `phys_footprint` at the time, in bytes.
        phys_footprint_bytes: u64,
        /// The limit that was crossed, in bytes.
        limit_bytes: u64,
        /// How far memory climbed after the stop, present only on an escalation.
        growth_since_stop_bytes: Option<u64>,
        /// Which escalation this is, present only on an escalation.
        escalation: Option<u32>,
        /// The full memory breakdown, when it could be captured.
        snapshot: Option<Diagnostic>,
    },
    /// A volume's index DB hit a fatal storage error, so its indexing stopped.
    StorageFailed {
        /// The typed SQLite reason (result code plus extended code).
        failure: IndexFailure,
        /// Which operation was running.
        context: Diagnostic,
        /// The underlying store error.
        detail: Diagnostic,
    },
    /// The live event loop couldn't open its read connection, so live indexing
    /// is off for the session.
    LiveEventLoopUnavailable {
        /// The connection error, after retries.
        detail: Diagnostic,
    },
    /// A directory-walk worker thread failed to spawn, so the walk runs with
    /// less parallelism.
    WalkWorkerSpawnFailed {
        /// The spawn error.
        detail: Diagnostic,
    },
}

/// Everything the index subsystems tell their host about.
///
/// Progress and lifecycle variants become frontend events; [`Error`](Self::Error)
/// and [`PathAccessDenied`](Self::PathAccessDenied) are the two that reach the
/// host's own machinery instead (error reporting and the restricted-path set),
/// which is why they're events rather than direct calls: neither is reachable
/// from a crate that can't name the app.
#[derive(Debug, Clone, PartialEq)]
pub enum IndexEvent {
    /// A volume's full scan started, with the calibration the frontend needs to
    /// pick and seed a progress tier.
    ScanStarted {
        /// The volume being scanned.
        volume_id: String,
        /// What kind of run this is.
        run_kind: ScanRunKind,
        /// The previous completed scan of this kind's entry count.
        prior_total_entries: Option<u64>,
        /// The previous completed scan of this kind's wall-clock duration.
        prior_scan_duration_ms: Option<u64>,
        /// The volume's used bytes at scan start.
        volume_used_bytes: Option<u64>,
        /// Whether this run covers the volume branch by branch rather than
        /// walking it whole, so a host knows to follow
        /// [`CoverageBranchStarted`](Self::CoverageBranchStarted) for which
        /// ground is under the walker instead of treating the whole volume as
        /// in flux for the run's whole length.
        covered_in_phases: bool,
        /// The remembered time left once the walk is done, from the last completed
        /// run of this kind. `None` when a later step has no such history, which
        /// is how a host knows to show no overall figure. See
        /// `IndexStatusResponse::left_after_find_files_ms` for the whole set.
        left_after_find_files_ms: Option<u64>,
        /// The remembered time left once the file list is saved.
        left_after_save_ms: Option<u64>,
        /// The remembered time left once folder sizes are computed.
        left_after_compute_ms: Option<u64>,
        /// The remembered time left once the catch-up step is done.
        left_after_catch_up_ms: Option<u64>,
    },
    /// A branch of a volume is being walked, and it is the walker's for as long
    /// as this holds.
    ///
    /// Paired with [`CoverageBranchEnded`](Self::CoverageBranchEnded), and only
    /// ever emitted by a run that announced itself with `covered_in_phases`. The
    /// pair brackets one walk; how long a host waits before believing it is the
    /// host's to decide.
    CoverageBranchStarted {
        /// The volume being covered.
        volume_id: String,
        /// The roots under the walker, absolute in the volume's own path space.
        roots: Vec<String>,
    },
    /// A branch stopped being walked, whether it was covered, left to another
    /// walk, or cancelled. Always emitted, so nothing can stay marked in flux.
    CoverageBranchEnded {
        /// The volume being covered.
        volume_id: String,
        /// The roots that were under the walker.
        roots: Vec<String>,
    },
    /// A phase of a drive's first index started: the folders this user cares
    /// about, then the rest of their home folder, then the rest of the drive.
    ///
    /// ⚠️ Fires on TRANSITIONS only, and again when a visited-root interlude
    /// hands the outer phase back, so the same phase can arrive twice in a row.
    /// A host that joined mid-run reads the running phase off
    /// [`IndexStatusResponse::coverage_phase`](super::IndexStatusResponse::coverage_phase)
    /// instead.
    CoveragePhaseStarted {
        /// The volume being covered.
        volume_id: String,
        /// Which phase it is. ❌ Don't re-derive this from `root`: an app-side
        /// home path can disagree with `IndexPathSpace` about firmlinks, which
        /// works on one machine and mislabels on another.
        phase: CoveragePhase,
        /// The root of the phase, absolute in the volume's own path space.
        root: String,
    },
    /// The user's home folder on this volume stopped needing a walk, which is
    /// the moment their own files become searchable and sizeable.
    ///
    /// A REPORT, ❌ not a second driver: the marker behind it still drives
    /// exactly one subscriber inside the crate (the early media and importance
    /// kick). This exists so a host can time the moment, which is the whole
    /// user-facing claim covering a drive in phases makes.
    HomeCovered {
        /// The volume whose home folder is covered.
        volume_id: String,
    },
    /// The running scan's moving counters, emitted on the reporter's tick.
    ScanProgress {
        /// The volume being scanned.
        volume_id: String,
        /// Entries walked so far.
        entries_scanned: u64,
        /// Directories found so far.
        dirs_found: u64,
        /// Post-dedup physical bytes walked so far.
        bytes_scanned: u64,
    },
    /// A scan finished cleanly.
    ScanComplete {
        /// The volume that finished scanning.
        volume_id: String,
        /// Entries in the finished index.
        total_entries: u64,
        /// Directories in the finished index.
        total_dirs: u64,
        /// How long the scan took.
        duration_ms: u64,
    },
    /// A scan ended without completing (disconnect, cancel, timeout, or an
    /// unlistable root), so the volume's live activity must be cleared.
    ScanAborted {
        /// The volume whose scan ended.
        volume_id: String,
    },
    /// These directories' sizes changed, so any listing showing them is stale.
    DirsUpdated {
        /// Absolute paths, in the listing's path space.
        paths: Vec<String>,
    },
    /// Journal replay is working through the backlog.
    ReplayProgress {
        /// The volume being replayed.
        volume_id: String,
        /// Events applied so far.
        events_processed: u64,
        /// An approximate total, when the journal offers one.
        estimated_total: Option<u64>,
    },
    /// Journal replay finished.
    ReplayComplete {
        /// The volume that finished replaying.
        volume_id: String,
        /// How long replay took.
        duration_ms: u64,
    },
    /// A full rescan was chosen over incremental replay, and why.
    RescanScheduled {
        /// The volume being rescanned.
        volume_id: String,
        /// The typed trigger, which the frontend maps to its own copy.
        reason: RescanReason,
        /// The specifics, for the log only.
        details: Diagnostic,
    },
    /// A writer's aggregation pass moved through one of its phases.
    AggregationProgress {
        /// The volume whose writer is aggregating.
        volume_id: String,
        /// Which phase of the pass.
        phase: AggregationPhase,
        /// Units done in this phase.
        current: u64,
        /// Units total in this phase.
        total: u64,
    },
    /// A full-scan aggregation pass finished, so the progress overlay can go.
    AggregationComplete {
        /// The volume that finished aggregating.
        volume_id: String,
    },
    /// The memory watchdog acted, with the figures a shipped report needs.
    ///
    /// Byte-precise on purpose: whole-GB rounding turned a 16.9 GB reading into
    /// "16 GB" in shipped reports, and that lost detail is what an incident needs.
    MemoryWarning {
        /// `phys_footprint`, the machine-pressure metric the thresholds key on.
        phys_footprint_bytes: u64,
        /// Resident set size, which counts mappings `phys_footprint` excludes.
        resident_bytes: u64,
        /// The global allocator the two figures below come from.
        global_allocator: cmdr_fs::process_memory::GlobalAllocator,
        /// Bytes the global allocator holds for the Rust heap (committed, or reserved).
        rust_heap_bytes: u64,
        /// Bytes the other malloc zones hold, disjoint from the figure above.
        system_malloc_bytes: u64,
        /// `phys_footprint` minus both allocators.
        untracked_bytes: u64,
        /// What the watchdog did.
        action: MemoryWatchdogAction,
    },
    /// A volume's freshness moved to a new value.
    FreshnessChanged {
        /// The volume whose badge changes.
        volume_id: String,
        /// The new freshness.
        freshness: Freshness,
    },
    /// A volume moved to a new top-level pipeline phase.
    PhaseChanged {
        /// The volume that changed phase.
        volume_id: String,
        /// The phase it moved to.
        phase: ActivityPhase,
    },
    /// An image-enrichment pass reported progress over its enrichable subset.
    MediaEnrichProgress {
        /// The volume being enriched.
        volume_id: String,
        /// Images processed so far.
        done: u64,
        /// Images in the enrichable subset.
        total: u64,
        /// Bytes processed so far.
        bytes_done: u64,
        /// Bytes across the enrichable subset.
        bytes_total: u64,
    },
    /// An image-enrichment pass ended, on any exit path.
    MediaEnrichTerminal {
        /// The volume whose pass ended.
        volume_id: String,
        /// Why it ended.
        reason: MediaEnrichTerminalReason,
    },
    /// Something failed in a way that deserves an error report.
    ///
    /// The host raises this through its own error-reporting path, so a failure
    /// inside indexing reaches the same pipeline an app-side failure does.
    Error {
        /// What broke.
        report: IndexErrorReport,
    },
    /// A path was unreadable because the OS denied access.
    ///
    /// The host decides whether that's worth surfacing; the index only reports
    /// the observation.
    PathAccessDenied {
        /// The path that couldn't be read.
        path: PathBuf,
    },
    /// What one live batch changed, rolled up per folder over the CORRECTED
    /// stream (after rename detection and removal-storm coalescing).
    ///
    /// ⚠️ **This is the one variant where a dropped event costs SIGNAL rather
    /// than a UI update.** The trait's fire-and-forget contract below says a
    /// drop never costs correctness, and that still holds — nothing downstream
    /// of this is a record anyone is owed — but a consumer reading it learns
    /// slightly less about what the user has been doing. That is acceptable:
    /// the folder will change again. Say it here rather than letting the
    /// contract look silently violated.
    FolderActivity {
        /// The volume the batch belongs to.
        volume_id: String,
        /// The batch's own instant, unix seconds. ❌ Never a window start: any
        /// coalescing window is the HOST's policy, and a field named for one
        /// here would lie about who decided it.
        observed_at: u64,
        /// One rollup per folder the batch touched.
        folders: Vec<FolderChangeRollup>,
    },
    /// This volume's index may have lost rows to a drive that went away, so it
    /// has been marked for a rebuild and the next start will walk it from
    /// scratch.
    ///
    /// Fired once per marker write, ❌ never per launch: the marker is
    /// persisted, and a host that announced it again on every start would keep
    /// apologizing for one disconnection. Nothing is asked of the host — the
    /// rebuild is the crate's own and already arranged; this exists so a person
    /// who just pulled a drive can be told why its folder sizes are about to be
    /// recomputed.
    IndexNeedsFreshScan {
        /// The volume whose index is marked.
        volume_id: String,
    },
}

/// The variants of [`IndexEvent`] without their payloads.
///
/// Exists so tests can assert on the SHAPE of an event stream ("started, then
/// progress, then complete") without spelling out every field, and so the host's
/// mapping can be checked for completeness against [`ALL`](Self::ALL).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IndexEventKind {
    /// [`IndexEvent::ScanStarted`].
    ScanStarted,
    /// [`IndexEvent::CoverageBranchStarted`].
    CoverageBranchStarted,
    /// [`IndexEvent::CoverageBranchEnded`].
    CoverageBranchEnded,
    /// [`IndexEvent::CoveragePhaseStarted`].
    CoveragePhaseStarted,
    /// [`IndexEvent::HomeCovered`].
    HomeCovered,
    /// [`IndexEvent::ScanProgress`].
    ScanProgress,
    /// [`IndexEvent::ScanComplete`].
    ScanComplete,
    /// [`IndexEvent::ScanAborted`].
    ScanAborted,
    /// [`IndexEvent::DirsUpdated`].
    DirsUpdated,
    /// [`IndexEvent::ReplayProgress`].
    ReplayProgress,
    /// [`IndexEvent::ReplayComplete`].
    ReplayComplete,
    /// [`IndexEvent::RescanScheduled`].
    RescanScheduled,
    /// [`IndexEvent::AggregationProgress`].
    AggregationProgress,
    /// [`IndexEvent::AggregationComplete`].
    AggregationComplete,
    /// [`IndexEvent::MemoryWarning`].
    MemoryWarning,
    /// [`IndexEvent::FreshnessChanged`].
    FreshnessChanged,
    /// [`IndexEvent::PhaseChanged`].
    PhaseChanged,
    /// [`IndexEvent::MediaEnrichProgress`].
    MediaEnrichProgress,
    /// [`IndexEvent::MediaEnrichTerminal`].
    MediaEnrichTerminal,
    /// [`IndexEvent::Error`].
    Error,
    /// [`IndexEvent::PathAccessDenied`].
    PathAccessDenied,
    /// [`IndexEvent::FolderActivity`].
    FolderActivity,
    /// [`IndexEvent::IndexNeedsFreshScan`].
    IndexNeedsFreshScan,
}

impl IndexEventKind {
    /// Every kind, in declaration order.
    ///
    /// The private `slot_of` below is what keeps this list complete: its match is
    /// exhaustive, so a new variant doesn't compile until it has a slot, and the
    /// slot doesn't compile until this array has room for it. That's what makes
    /// the host's completeness test meaningful.
    pub const ALL: [Self; 23] = [
        Self::ScanStarted,
        Self::CoverageBranchStarted,
        Self::CoverageBranchEnded,
        Self::CoveragePhaseStarted,
        Self::HomeCovered,
        Self::ScanProgress,
        Self::ScanComplete,
        Self::ScanAborted,
        Self::DirsUpdated,
        Self::ReplayProgress,
        Self::ReplayComplete,
        Self::RescanScheduled,
        Self::AggregationProgress,
        Self::AggregationComplete,
        Self::MemoryWarning,
        Self::FreshnessChanged,
        Self::PhaseChanged,
        Self::MediaEnrichProgress,
        Self::MediaEnrichTerminal,
        Self::Error,
        Self::PathAccessDenied,
        Self::FolderActivity,
        Self::IndexNeedsFreshScan,
    ];

    /// Where this kind sits in [`ALL`](Self::ALL).
    ///
    /// Every arm wraps its index in a `const` block, which the compiler
    /// evaluates whether or not the arm ever runs. A kind whose slot is past the
    /// end of `ALL` is therefore a compile error, and a new variant with no arm
    /// at all fails the exhaustiveness check above it. Adding a kind means
    /// touching both lists or the crate doesn't build.
    const fn slot_of(self) -> usize {
        match self {
            Self::ScanStarted => const { Self::slot(0) },
            Self::CoverageBranchStarted => const { Self::slot(1) },
            Self::CoverageBranchEnded => const { Self::slot(2) },
            Self::CoveragePhaseStarted => const { Self::slot(3) },
            Self::HomeCovered => const { Self::slot(4) },
            Self::ScanProgress => const { Self::slot(5) },
            Self::ScanComplete => const { Self::slot(6) },
            Self::ScanAborted => const { Self::slot(7) },
            Self::DirsUpdated => const { Self::slot(8) },
            Self::ReplayProgress => const { Self::slot(9) },
            Self::ReplayComplete => const { Self::slot(10) },
            Self::RescanScheduled => const { Self::slot(11) },
            Self::AggregationProgress => const { Self::slot(12) },
            Self::AggregationComplete => const { Self::slot(13) },
            Self::MemoryWarning => const { Self::slot(14) },
            Self::FreshnessChanged => const { Self::slot(15) },
            Self::PhaseChanged => const { Self::slot(16) },
            Self::MediaEnrichProgress => const { Self::slot(17) },
            Self::MediaEnrichTerminal => const { Self::slot(18) },
            Self::Error => const { Self::slot(19) },
            Self::PathAccessDenied => const { Self::slot(20) },
            Self::FolderActivity => const { Self::slot(21) },
            Self::IndexNeedsFreshScan => const { Self::slot(22) },
        }
    }

    /// One slot index, bounds-checked against [`ALL`](Self::ALL) at compile time.
    const fn slot(index: usize) -> usize {
        assert!(
            index < Self::ALL.len(),
            "a new `IndexEventKind` needs its own entry in `IndexEventKind::ALL`"
        );
        index
    }
}

/// `ALL` holds every kind exactly once, in declaration order.
///
/// `ALL[i].slot_of() == i` leaves no room for a duplicate, a stray, or a gap, so
/// the array a host iterates is the enum itself, not a list somebody remembered
/// to update.
const _: () = {
    let mut index = 0;
    while index < IndexEventKind::ALL.len() {
        assert!(
            IndexEventKind::ALL[index].slot_of() == index,
            "`IndexEventKind::ALL` must list every kind once, in declaration order"
        );
        index += 1;
    }
};

impl IndexEvent {
    /// Which variant this is.
    #[must_use]
    pub fn kind(&self) -> IndexEventKind {
        match self {
            Self::ScanStarted { .. } => IndexEventKind::ScanStarted,
            Self::CoverageBranchStarted { .. } => IndexEventKind::CoverageBranchStarted,
            Self::CoverageBranchEnded { .. } => IndexEventKind::CoverageBranchEnded,
            Self::CoveragePhaseStarted { .. } => IndexEventKind::CoveragePhaseStarted,
            Self::HomeCovered { .. } => IndexEventKind::HomeCovered,
            Self::ScanProgress { .. } => IndexEventKind::ScanProgress,
            Self::ScanComplete { .. } => IndexEventKind::ScanComplete,
            Self::ScanAborted { .. } => IndexEventKind::ScanAborted,
            Self::DirsUpdated { .. } => IndexEventKind::DirsUpdated,
            Self::ReplayProgress { .. } => IndexEventKind::ReplayProgress,
            Self::ReplayComplete { .. } => IndexEventKind::ReplayComplete,
            Self::RescanScheduled { .. } => IndexEventKind::RescanScheduled,
            Self::AggregationProgress { .. } => IndexEventKind::AggregationProgress,
            Self::AggregationComplete { .. } => IndexEventKind::AggregationComplete,
            Self::MemoryWarning { .. } => IndexEventKind::MemoryWarning,
            Self::FreshnessChanged { .. } => IndexEventKind::FreshnessChanged,
            Self::PhaseChanged { .. } => IndexEventKind::PhaseChanged,
            Self::MediaEnrichProgress { .. } => IndexEventKind::MediaEnrichProgress,
            Self::MediaEnrichTerminal { .. } => IndexEventKind::MediaEnrichTerminal,
            Self::Error { .. } => IndexEventKind::Error,
            Self::PathAccessDenied { .. } => IndexEventKind::PathAccessDenied,
            Self::FolderActivity { .. } => IndexEventKind::FolderActivity,
            Self::IndexNeedsFreshScan { .. } => IndexEventKind::IndexNeedsFreshScan,
        }
    }

    /// The volume this event is about, when it's about one.
    ///
    /// Per-volume isolation is the subsystem's one cross-area invariant, so a
    /// host (or a test) routing an event stream reads the id from here rather
    /// than matching each variant.
    #[must_use]
    pub fn volume_id(&self) -> Option<&str> {
        match self {
            Self::ScanStarted { volume_id, .. }
            | Self::CoverageBranchStarted { volume_id, .. }
            | Self::CoverageBranchEnded { volume_id, .. }
            | Self::CoveragePhaseStarted { volume_id, .. }
            | Self::HomeCovered { volume_id }
            | Self::ScanProgress { volume_id, .. }
            | Self::ScanComplete { volume_id, .. }
            | Self::ScanAborted { volume_id }
            | Self::ReplayProgress { volume_id, .. }
            | Self::ReplayComplete { volume_id, .. }
            | Self::RescanScheduled { volume_id, .. }
            | Self::AggregationProgress { volume_id, .. }
            | Self::AggregationComplete { volume_id }
            | Self::FreshnessChanged { volume_id, .. }
            | Self::PhaseChanged { volume_id, .. }
            | Self::MediaEnrichProgress { volume_id, .. }
            | Self::MediaEnrichTerminal { volume_id, .. }
            | Self::FolderActivity { volume_id, .. }
            | Self::IndexNeedsFreshScan { volume_id } => Some(volume_id),
            Self::DirsUpdated { .. }
            | Self::MemoryWarning { .. }
            | Self::Error { .. }
            | Self::PathAccessDenied { .. } => None,
        }
    }
}

/// Where the index subsystems send everything they have to say.
///
/// Fire-and-forget by contract: `emit` never fails and never blocks the caller on
/// the host. A dropped event costs a UI update, never correctness, so emit sites
/// stay call-and-continue.
pub trait EventSink: Send + Sync {
    /// Report one event. Called from scan threads, writer threads, and async
    /// tasks, so implementations must be cheap and must not panic.
    fn emit(&self, event: IndexEvent);
}

/// A sink that drops everything.
///
/// For the paths that run before a host is wired up, and for tests that exercise
/// something other than the event stream.
pub struct NoopEventSink;

impl NoopEventSink {
    /// A shared handle to it, for the many call sites that need an
    /// `Arc<dyn EventSink>` and have nothing to say.
    #[must_use]
    pub fn shared() -> std::sync::Arc<dyn EventSink> {
        static SHARED: std::sync::LazyLock<std::sync::Arc<NoopEventSink>> =
            std::sync::LazyLock::new(|| std::sync::Arc::new(NoopEventSink));
        SHARED.clone()
    }
}

impl EventSink for NoopEventSink {
    fn emit(&self, _event: IndexEvent) {}
}
