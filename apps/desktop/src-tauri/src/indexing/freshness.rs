//! Per-volume index freshness: the "admittedly stale" state model.
//!
//! Local disk gets freshness for free from FSEvents' historical journal: on
//! launch we replay from `last_event_id`, so "the app was off" self-heals to
//! Fresh. SMB and MTP have **no journal** — `CHANGE_NOTIFY` / PTP events only
//! arrive while connected and watching, and any gap (app off, disconnect, wifi
//! blip, notify overflow) loses them irrecoverably. So for these volumes
//! freshness is binary: continuously watched since the last scan ⇒ we know
//! what's where (Fresh); any break ⇒ we can't know what drifted (Stale).
//!
//! ## The UI states and where they live
//!
//! The UX surfaces five colors; four are `Freshness` variants — gray is the
//! *absence* of a running index, mirroring the registry's "disabled = no key"
//! model (see `state.rs`):
//!
//! - **Gray (disabled / not-indexed)**: no `IndexInstance` for the volume, OR a
//!   scan was interrupted and its partial discarded (D-interrupted). Not a
//!   `Freshness` value; it's `get_volume_index_status` returning `enabled:
//!   false`.
//! - **Blue (scanning)** → [`Freshness::Scanning`].
//! - **Green (fresh)** → [`Freshness::Fresh`].
//! - **Yellow (stale)** → [`Freshness::Stale`].
//! - **Red (indexing stopped)** → [`Freshness::Failed`]: the DB died with a fatal
//!   storage error. Unlike Stale it is NOT browsable and does NOT self-heal on a
//!   watcher event; the index is torn down and the only way out is a
//!   rebuild-from-scratch rescan. See `state.rs`'s `IndexPhase::Failed` and
//!   `failure.rs`.
//!
//! ## The transition table (this module is the single source of truth)
//!
//! [`Freshness::on`] is a pure function from `(current, event)` to the next
//! state. Some transitions are scan-driven (`ScanStarted`, `ScanCompleted`, and
//! `ScanFailed` — fired by the scan completion handlers); the watcher-driven
//! ones (`WatcherDied`, `OverflowUnrecoverable`) are fired from the
//! watcher-lifetime layer (`smb_index` / `mtp_index`), which just calls
//! `on(WatcherDied)` — the call sites live there, never in this state machine.
//!
//! ## Persistence
//!
//! Freshness is NOT persisted as a value. `meta.scan_completed_at` (already
//! written by the scan-completion handler) is the only durable signal: its
//! presence proves a scan finished. On launch, [`initial_freshness_on_launch`]
//! derives the starting state from it — and for SMB/MTP a finished scan loads
//! as **Stale**, never Fresh, because the app wasn't watching while off. That's
//! correct and honest, not a bug to fix.

use serde::{Deserialize, Serialize};

/// One volume's index freshness. Carried by a `Running` index instance; gray /
/// not-indexed is the absence of an instance, not a variant here (see module
/// docs and `state.rs`'s "disabled = no key" model).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum Freshness {
    /// A full scan (initial or rescan) is in progress. Blue.
    Scanning,
    /// Scan complete AND the watch has been unbroken since. Authoritative. Green.
    Fresh,
    /// An index exists, but watch continuity broke (app restart, disconnect,
    /// dead watcher, notify overflow). Browsable, clearly marked, one-click
    /// rescan. Yellow.
    Stale,
    /// The index DB died with a fatal storage error (`SQLITE_IOERR`, corruption, a
    /// full or read-only disk, …). Indexing STOPPED for this volume — its writer
    /// thread and watcher are torn down, so there are no sizes and no live updates
    /// until the user retries (a rebuild-from-scratch). Distinct from Stale (which
    /// is still browsable and self-heals on rescan) and from gray/disabled (a
    /// deliberate off state). Red. Terminal in this table: only an explicit rescan
    /// (`ScanStarted`) leaves it, so a concurrent scan-completion handler can't
    /// downgrade a dead index back to Stale/Fresh. See [`IndexFailure`](super::store::IndexFailure)
    /// for the typed reason carried alongside on the `Failed` phase.
    Failed,
}

/// Inputs that drive a volume's freshness transitions.
///
/// The first three are scan-driven and need no live watcher. The last two are
/// fired from the watcher-lifetime layer: `WatcherDied` when the SMB session
/// drops / the watcher task returns, `OverflowUnrecoverable` when a
/// `CHANGE_NOTIFY` overflow can't be repaired by a targeted subtree rescan. All
/// the call sites live there, never as new state-machine arms.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FreshnessEvent {
    /// A full scan started (initial or rescan). ⇒ `Scanning`.
    ScanStarted,
    /// A full scan completed without cancellation. ⇒ `Fresh`.
    ///
    /// Only valid for an UNinterrupted scan: an interrupted/disconnected
    /// mid-scan discards the partial and the volume goes gray (no instance), so
    /// it never reaches this event (D-interrupted, handled in `state.rs`).
    ScanCompleted,
    /// The live watcher for this volume reported the watched session is gone
    /// (disconnect, SMB session drop, MTP device unplug). ⇒ `Stale`.
    ///
    /// Fired from the watcher-lifetime layer (`smb_index` / `mtp_index`).
    WatcherDied,
    /// A `CHANGE_NOTIFY` overflow could not be repaired by a targeted subtree
    /// rescan, so the index may have drifted. ⇒ `Stale`.
    ///
    /// The SMB watcher keeps watching on overflow and emits a `FullRefresh`; the
    /// watcher-lifetime layer decides overflow policy (rescan-subtree vs. signal
    /// Stale) and fires this only for the unrecoverable case.
    OverflowUnrecoverable,
    /// A full scan/reconcile failed or panicked; the prior index stays visible
    /// but is marked stale so the UI is honest and a rescan is offered. ⇒ `Stale`.
    ///
    /// Fired by the LOCAL completion handler (`manager::start_scan`) on both the
    /// `Ok(Err(_))` (e.g. `ScanError::EmptyRoot`, or a `catch_unwind`-converted
    /// reconcile-walk `ScanError::Panicked`) and the `Err(_)` (thread-join panic)
    /// arms. Without it, `ScanStarted` having already moved the badge to
    /// `Scanning` would strand it on a perpetual spinner until relaunch.
    ScanFailed,
    /// The index DB died with a FATAL storage error and indexing has stopped for
    /// this volume. ⇒ `Failed` (terminal). Fired ONCE by the failure supervisor
    /// (`state::spawn_failure_supervisor`) when the writer trips its
    /// [`IndexFailureSignal`](super::failure::IndexFailureSignal), NOT from the scan
    /// path. Distinct from `ScanFailed` (a scan/reconcile that failed but left a
    /// browsable, self-healing Stale index): here the storage itself is unusable.
    StorageFailed,
}

impl Freshness {
    /// Pure transition: the next freshness given the current state and an event.
    ///
    /// Total and deterministic — every `(state, event)` pair has a defined
    /// result, so the table is the single source of truth and the watcher-layer
    /// call sites can't introduce an undefined transition. A scan can start from any
    /// state (re-scan a Fresh or Stale volume), and a watcher death / overflow
    /// only matters once we'd otherwise claim Fresh, but defining them from
    /// every state keeps the function total and the seam trivial.
    #[must_use]
    pub fn on(self, event: FreshnessEvent) -> Self {
        // `Failed` is TERMINAL: a dead-storage index only leaves it by an explicit
        // rescan (the rebuild-from-scratch recovery). Guarding here first keeps a
        // concurrent scan-completion handler (which may still fire `ScanFailed` or
        // even `ScanCompleted` as the torn-down scan unwinds) from downgrading a
        // dead index back to Stale/Fresh.
        if self == Freshness::Failed && event != FreshnessEvent::ScanStarted {
            return Freshness::Failed;
        }
        match event {
            // A scan (re)starts from anywhere: Fresh/Stale rescan, a fresh initial
            // scan, or the recovery rescan of a Failed volume. While scanning we are
            // neither Fresh nor Stale.
            FreshnessEvent::ScanStarted => Freshness::Scanning,
            // A clean scan completion is the only path to Fresh.
            FreshnessEvent::ScanCompleted => Freshness::Fresh,
            // Continuity broke, or a scan/reconcile failed. From Scanning a
            // watcher death is unusual (the scan path handles mid-scan disconnect
            // by discarding to gray), but a `ScanFailed` from Scanning is the
            // normal failure exit — `ScanStarted` already moved us here, so
            // landing Stale keeps the badge honest and offers a rescan.
            FreshnessEvent::WatcherDied | FreshnessEvent::OverflowUnrecoverable | FreshnessEvent::ScanFailed => {
                Freshness::Stale
            }
            // The storage itself died: stop and fail, distinct from a browsable
            // Stale. Terminal (see the guard above).
            FreshnessEvent::StorageFailed => Freshness::Failed,
        }
    }

    /// Whether reads on this volume's index are authoritative (Fresh only).
    /// Stale and Scanning are browsable but explicitly not authoritative.
    #[must_use]
    pub fn is_authoritative(self) -> bool {
        matches!(self, Freshness::Fresh)
    }
}

/// The freshness a persisted index loads as at app launch.
///
/// `scan_completed_at_present` is whether `meta.scan_completed_at` is set (a
/// finished scan). `journaled` is whether the volume self-heals continuity from
/// an event journal on launch — `true` for local disk (FSEvents replay),
/// `false` for SMB/MTP (no journal).
///
/// - No completed scan ⇒ `None` (the caller starts a fresh scan; gray until it
///   begins, then Scanning).
/// - Completed scan + journaled (local) ⇒ `Fresh` (replay restores continuity).
/// - Completed scan + NOT journaled (SMB/MTP) ⇒ `Stale`. We weren't watching
///   while the app was off, so we can't claim Fresh. This is the core of the
///   "admittedly stale" model — correct and honest, not a limitation.
#[must_use]
pub fn initial_freshness_on_launch(scan_completed_at_present: bool, journaled: bool) -> Option<Freshness> {
    if !scan_completed_at_present {
        return None;
    }
    if journaled {
        Some(Freshness::Fresh)
    } else {
        Some(Freshness::Stale)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── on(): the transition table ───────────────────────────────────────

    #[test]
    fn scan_started_always_goes_to_scanning() {
        for from in [Freshness::Scanning, Freshness::Fresh, Freshness::Stale] {
            assert_eq!(
                from.on(FreshnessEvent::ScanStarted),
                Freshness::Scanning,
                "a (re)scan from {from:?} must enter Scanning"
            );
        }
    }

    #[test]
    fn scan_completed_makes_it_fresh() {
        // The ONLY path to Fresh: a clean scan completion. From Scanning is the
        // normal case; from Stale would be a rescan that finished.
        assert_eq!(Freshness::Scanning.on(FreshnessEvent::ScanCompleted), Freshness::Fresh);
        assert_eq!(Freshness::Stale.on(FreshnessEvent::ScanCompleted), Freshness::Fresh);
    }

    #[test]
    fn watcher_death_makes_a_fresh_volume_stale() {
        // The headline Fresh→Stale transition. The watcher-lifetime layer wires
        // the call site; the transition itself must already hold.
        assert_eq!(Freshness::Fresh.on(FreshnessEvent::WatcherDied), Freshness::Stale);
    }

    #[test]
    fn overflow_unrecoverable_makes_a_fresh_volume_stale() {
        assert_eq!(
            Freshness::Fresh.on(FreshnessEvent::OverflowUnrecoverable),
            Freshness::Stale
        );
    }

    #[test]
    fn scan_failure_always_goes_to_stale() {
        // A failed/panicked full scan or reconcile leaves the prior index visible
        // but marks it Stale so the badge is honest and a rescan is offered — from
        // any prior state. Critically Scanning→Stale: `ScanStarted` already moved
        // the badge to Scanning, so without this it would stick on the spinner.
        for from in [Freshness::Scanning, Freshness::Fresh, Freshness::Stale] {
            assert_eq!(
                from.on(FreshnessEvent::ScanFailed),
                Freshness::Stale,
                "a failed scan from {from:?} must land Stale"
            );
        }
    }

    #[test]
    fn stale_stays_stale_under_continuity_breaks() {
        // Idempotent: a second disconnect on an already-stale volume is a no-op.
        assert_eq!(Freshness::Stale.on(FreshnessEvent::WatcherDied), Freshness::Stale);
        assert_eq!(
            Freshness::Stale.on(FreshnessEvent::OverflowUnrecoverable),
            Freshness::Stale
        );
    }

    #[test]
    fn only_fresh_is_authoritative() {
        assert!(Freshness::Fresh.is_authoritative());
        assert!(!Freshness::Stale.is_authoritative());
        assert!(!Freshness::Scanning.is_authoritative());
        assert!(!Freshness::Failed.is_authoritative());
    }

    #[test]
    fn storage_failure_goes_to_failed_from_any_state() {
        // A fatal storage death stops indexing from wherever the volume was.
        for from in [Freshness::Scanning, Freshness::Fresh, Freshness::Stale] {
            assert_eq!(
                from.on(FreshnessEvent::StorageFailed),
                Freshness::Failed,
                "a storage failure from {from:?} must land Failed"
            );
        }
    }

    #[test]
    fn failed_is_terminal_except_an_explicit_rescan() {
        // The core stickiness guarantee: once the storage is dead, a concurrent
        // scan-completion handler firing ScanFailed/ScanCompleted (or another
        // watcher-death) must NOT downgrade the badge off Failed — only the user's
        // explicit rebuild (ScanStarted) leaves it.
        for event in [
            FreshnessEvent::ScanCompleted,
            FreshnessEvent::WatcherDied,
            FreshnessEvent::OverflowUnrecoverable,
            FreshnessEvent::ScanFailed,
            FreshnessEvent::StorageFailed,
        ] {
            assert_eq!(
                Freshness::Failed.on(event),
                Freshness::Failed,
                "Failed must stay Failed under {event:?}"
            );
        }
        // The one escape hatch: a rebuild-from-scratch rescan.
        assert_eq!(
            Freshness::Failed.on(FreshnessEvent::ScanStarted),
            Freshness::Scanning,
            "an explicit rescan recovers a Failed volume"
        );
    }

    // ── initial_freshness_on_launch(): the load rule ─────────────────────

    #[test]
    fn launch_without_completed_scan_is_not_indexed() {
        // No prior completed scan → gray (None); the caller starts a fresh scan.
        assert_eq!(initial_freshness_on_launch(false, true), None);
        assert_eq!(initial_freshness_on_launch(false, false), None);
    }

    #[test]
    fn launch_journaled_volume_loads_fresh() {
        // Local disk replays its FSEvents journal → continuity restored → Fresh.
        assert_eq!(initial_freshness_on_launch(true, true), Some(Freshness::Fresh));
    }

    #[test]
    fn launch_non_journaled_volume_loads_stale() {
        // SMB/MTP: a completed scan exists, but we weren't watching while off,
        // so we CAN'T claim Fresh. This is the whole point of the feature.
        assert_eq!(initial_freshness_on_launch(true, false), Some(Freshness::Stale));
    }
}
