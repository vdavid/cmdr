//! The backstop that reclaims a listing whose close IPC never arrived.
//!
//! A pane's listing is normally released by the explicit `list_directory_end`
//! IPC. This catches the ones that leaked instead: a thrown frontend handler,
//! an `$effect` teardown that never ran, a future code path that forgot the
//! call. It runs on a coarse timer and tears each orphan down through exactly
//! the same path the close IPC uses, so the cache entry, its watcher, and any
//! pending diff go together.

use std::sync::atomic::Ordering;
use std::time::Duration;

use crate::file_system::listing::cached_listing::{LISTING_CACHE, epoch_millis_now};

/// Idle window after which an untouched listing is treated as orphaned and reaped.
///
/// **Deliberately generous (six hours).** A listing legitimately lives for the entire
/// time a pane shows its directory, which can be the whole multi-day session. The
/// primary, fast eviction path is the explicit `list_directory_end` IPC; this backstop
/// only catches listings that genuinely leaked (a thrown FE handler skipped the close
/// IPC, an `$effect` teardown that threw, a future code path that forgot the call).
/// Every read accessor that proves a pane is still alive (`get_file_range`,
/// `get_total_count`, `get_file_at`, `get_listing_stats`, resort, watcher-diff patches,
/// …) refreshes `last_accessed_ms`, so a pane the user is interacting with — or that is
/// receiving FS-change diffs — is never six continuous hours idle. We err strongly
/// toward NOT evicting: six hours of zero interaction AND zero FS activity on a path is
/// overwhelmingly a leak, not a pane the user is actively using.
pub(crate) const ORPHAN_IDLE_WINDOW: Duration = Duration::from_secs(6 * 60 * 60);

/// How often the backstop reaper task wakes up to scan for orphaned listings.
///
/// Coarse on purpose: the reaper is defense-in-depth for a multi-day session, not a
/// hot-path reclaimer, so a 30-minute cadence keeps it effectively free while still
/// bounding orphan accumulation well under a day.
pub(crate) const REAPER_SWEEP_INTERVAL: Duration = Duration::from_secs(30 * 60);

/// Pure helper: given the current time, the idle window, and an iterator of
/// `(listing_id, last_accessed_ms)`, returns the IDs whose idle time meets or exceeds
/// `window_ms`.
///
/// Split out from the cache walk so the reaper logic is deterministically testable
/// without sleeping or touching the real (process-start-relative) clock: feed it a
/// synthetic `now_ms`, `window_ms`, and a list of stamps.
pub(crate) fn orphan_ids<'a>(
    now_ms: u64,
    window_ms: u64,
    listings: impl Iterator<Item = (&'a str, u64)>,
) -> Vec<String> {
    listings
        .filter(|(_, last_ms)| now_ms.saturating_sub(*last_ms) >= window_ms)
        .map(|(id, _)| id.to_string())
        .collect()
}

/// Scans the cache for orphaned listings (idle past `ORPHAN_IDLE_WINDOW`) and tears each
/// down via the same path as the explicit `list_directory_end` IPC: cache entry removed,
/// `WATCHER_MANAGER` watcher dropped, pending coalesced diff dropped.
///
/// This is the backstop reaper. The fast, primary eviction is still the FE-fired
/// `list_directory_end`; this only catches listings whose close IPC was never delivered.
/// Returns the IDs it reaped (empty in the common case), for logging/tests.
pub(crate) fn reap_orphaned_listings() -> Vec<String> {
    reap_orphaned_listings_at(epoch_millis_now(), ORPHAN_IDLE_WINDOW.as_millis() as u64)
}

/// `reap_orphaned_listings` with the clock and idle window injected, so tests can
/// simulate "6 hours idle" deterministically (the real epoch clock starts at process
/// launch, so a real 6 h gap can't be produced in a unit test without sleeping).
pub(crate) fn reap_orphaned_listings_at(now_ms: u64, window_ms: u64) -> Vec<String> {
    reap_orphaned_listings_scoped(now_ms, window_ms, None)
}

/// `reap_orphaned_listings_at` restricted to the listing ids in `only`.
///
/// The sweep walks the process-global cache, and under `cargo test` that cache is
/// shared by every concurrently-running listing test, so an unrestricted sweep
/// evicts other tests' fixtures (cache entry AND watcher) mid-assertion. The
/// reaper's own tests scope the sweep to the ids they own; production passes
/// `None` and sweeps everything.
#[cfg(test)]
pub(crate) fn reap_orphaned_listings_at_for(now_ms: u64, window_ms: u64, only: &[&str]) -> Vec<String> {
    reap_orphaned_listings_scoped(now_ms, window_ms, Some(only))
}

fn reap_orphaned_listings_scoped(now_ms: u64, window_ms: u64, only: Option<&[&str]>) -> Vec<String> {
    let ids = {
        let cache = match LISTING_CACHE.read() {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };
        orphan_ids(
            now_ms,
            window_ms,
            cache
                .iter()
                .filter(|(id, _)| only.is_none_or(|ids| ids.contains(&id.as_str())))
                .map(|(id, listing)| (id.as_str(), listing.last_accessed_ms.load(Ordering::Relaxed))),
        )
    };

    for id in &ids {
        // Reuse the exact teardown the explicit close IPC uses, so the cache entry AND
        // its watcher (and any pending diff) are released together.
        crate::file_system::listing::operations::list_directory_end(id);
        log::warn!(
            target: "listing_cache",
            "Reaped orphaned listing `{id}`: no access for >= {} min. Its `list_directory_end` IPC was likely never delivered (a skipped FE cleanup).",
            ORPHAN_IDLE_WINDOW.as_secs() / 60,
        );
    }

    ids
}

/// Spawns the periodic backstop reaper task. Call once during app setup.
///
/// Wakes every `REAPER_SWEEP_INTERVAL` and calls `reap_orphaned_listings`. Runs on
/// Tauri's async runtime so it survives for the process lifetime; the task ends only
/// when the runtime shuts down at app exit.
pub(crate) fn start_orphan_listing_reaper() {
    tauri::async_runtime::spawn(async {
        loop {
            tokio::time::sleep(REAPER_SWEEP_INTERVAL).await;
            let reaped = reap_orphaned_listings();
            if !reaped.is_empty() {
                log::info!(
                    target: "listing_cache",
                    "Orphan-listing reaper swept {} leaked listing(s)",
                    reaped.len(),
                );
            }
        }
    });
}
