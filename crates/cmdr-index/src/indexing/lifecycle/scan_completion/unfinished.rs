//! A scan that neither finished nor was cancelled: a typed failure, or a walker
//! thread that panicked outright.
//!
//! Split from the completion flow next door so that flow reads as one path, and so
//! a cancelled walk can't slip in here: it arrives as an `Err` but must NOT be
//! reported as a failure, and the caller peels it off before this is ever reached.

use crate::indexing::events::{ActivityPhase, EventSink, IndexEvent, set_phase_for};
use crate::indexing::lifecycle::{freshness, signals};
use crate::indexing::scanner::{ScanError, ScanSummary};
use cmdr_fs::log_error::LogError;

/// Whether a failed local scan should emit `index-scan-aborted`: only when the
/// volume VANISHED (its root became unlistable), never for a legitimately empty
/// root or a walk panic. The abort event clears the frontend's stuck "scanning"
/// row; an empty root and a panic keep the prior index visible-stale without an
/// abort. Pure so the decision is unit-testable without an `AppHandle`.
pub(super) fn scan_failure_is_vanished_volume(err: &ScanError) -> bool {
    matches!(err, ScanError::RootUnlistable)
}

/// Clear the live activity of a volume whose drive went away: the scan it was
/// running will never finish on its own, so the frontend's "scanning" row has to
/// come off something other than a completion.
///
/// Shared with the completion gate next door (`../scan_completion.rs`), which meets
/// the same drive one step later: a walk can end `Ok` and its drive still be gone
/// by the time anything would be stamped. ❌ Freshness is the CALLER's, because the
/// two callers reach here with it in different states.
pub(super) fn report_a_vanished_drive(events: &dyn EventSink, volume_id: &str) {
    set_phase_for(
        events,
        volume_id,
        ActivityPhase::Idle,
        "local scan aborted (volume vanished)",
    );
    events.emit(IndexEvent::ScanAborted {
        volume_id: volume_id.to_string(),
    });
}

/// Report a scan that neither finished nor was cancelled. Both outcomes reset
/// freshness to Stale, and a vanished root also clears the frontend's stuck
/// "scanning" row.
pub(super) fn report_unfinished_scan(
    result: &std::thread::Result<Result<ScanSummary, ScanError>>,
    events: &dyn EventSink,
    volume_id: &str,
    freshness_slot: &std::sync::Mutex<Option<freshness::Freshness>>,
) {
    match result {
        Ok(Err(e)) => {
            log::warn!("Volume scan failed: {}", LogError(e));
            // The scan/reconcile bailed (e.g. `EmptyRoot`, `RootUnlistable`, or a
            // `catch_unwind`-converted reconcile-walk `Panicked`). The prior index
            // is untouched and stays visible, but `ScanStarted` already moved
            // freshness to Scanning, so reset it to Stale — honest "rescan
            // available" instead of a stuck spinner. Fire through the cloned handle,
            // never the registry (no re-lock).
            signals::apply_freshness_event_on(freshness_slot, events, volume_id, freshness::FreshnessEvent::ScanFailed);

            // If the failure is a VANISHED volume (its root went unlistable —
            // a yanked external drive), the scan will never complete on its own, so
            // clear the frontend's live activity and go Idle — mirroring the network
            // disconnect arm (`lifecycle/manager/network_scan.rs`). A legitimately empty root
            // (`EmptyRoot`) or a panic is NOT a vanished volume, so it does not
            // abort. No `scan_completed_at` was written (the meta writes live in the
            // clean-completion arm only), so the index heals to a rescan on remount.
            if scan_failure_is_vanished_volume(e) {
                report_a_vanished_drive(events, volume_id);
            }
        }
        Err(_) => {
            log::warn!("Volume scan thread panicked");
            // The walker thread itself panicked (the reconcile walk is
            // `catch_unwind`-wrapped, so this is the residual guarded-walker/thread
            // case). Same honest reset as the `Ok(Err(_))` arm above.
            signals::apply_freshness_event_on(freshness_slot, events, volume_id, freshness::FreshnessEvent::ScanFailed);
        }
        // The caller routes finished and cancelled walks itself and never gets
        // here; matching exhaustively keeps that split visible rather than
        // silently absorbing a future outcome into "failed".
        Ok(Ok(_)) => debug_assert!(false, "a completed scan must not reach the failure reporter"),
    }
}
