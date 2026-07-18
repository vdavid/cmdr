//! FSEvents/inotify event processing for the LOCAL index, split into three
//! non-calling responsibilities plus the primitives they share:
//!
//! - [`live`]: the real-time live event loop (`run_live_event_loop`,
//!   `process_live_batch`, the rename pre-pass).
//! - [`replay`]: cold-start journal replay (`run_replay_event_loop`), boot disk
//!   only, which hands off to live mode and spawns verification.
//! - [`verification`]: post-replay bidirectional readdir diff.
//! - [`storm`]: removal-storm coalescing helpers used by `process_live_batch`.
//!
//! This root file keeps only what more than one loop shares: `merge_fs_events`
//! (dedup with flag priority), `open_read_conn_with_retry` (read-connection
//! open used at each loop's start), `ReplayConfig` (the manager→replay bridge),
//! and the cross-loop flush/gap/channel constants. Replay-only bounded-buffer
//! constants live in [`replay`].

use std::path::Path;
use std::time::Duration;

use rusqlite::Connection;

use super::IndexPathSpace;
use super::store::{self, IndexStore};
use super::watcher;

mod live;
mod replay;
mod storm;
mod verification;

// Re-export the loop entry points so external callers keep using the stable
// `event_loop::…` paths (`manager.rs`, `scan_completion.rs`, and the indexing
// stress tests) after the internal split.
pub(in crate::indexing) use live::run_live_event_loop;
pub(in crate::indexing) use replay::run_replay_event_loop;
// Only the indexing stress tests reach `process_live_batch` directly; gate the
// re-export so the non-test build doesn't see it as an unused import.
#[cfg(test)]
pub(in crate::indexing) use live::process_live_batch;

// ── Shared constants ─────────────────────────────────────────────────

/// Flush interval for live event batching. Events are deduplicated by
/// normalized path during this window before being processed. Longer
/// windows reduce allocations during event storms (for example, multiple
/// agents writing simultaneously) at the cost of slightly delayed UI
/// updates.
pub(super) const LIVE_FLUSH_INTERVAL_MS: u64 = 1000;

/// How often the live loop sweeps the reconciler's per-file throttle for keys
/// whose window elapsed, applying their last-seen size. Runs on the existing
/// `tokio::select!` (no new thread). ~1 s keeps trailing flushes prompt relative
/// to the 60 s window while staying negligible work when idle.
pub(super) const THROTTLE_SWEEP_INTERVAL_MS: u64 = 1000;

/// Threshold for detecting a journal gap. If the first event ID received is
/// more than this many IDs ahead of the stored `since_event_id`, we consider
/// the journal unavailable and fall back to a full scan.
pub(super) const JOURNAL_GAP_THRESHOLD: u64 = 10_000_000;

/// Capacity of the watcher→event loop channel. Provides backpressure to
/// FSEvents/inotify when the event loop can't keep up, preventing unbounded
/// memory growth. Each event is ~300 bytes, so 20K ≈ 6 MB worst case.
pub(super) const WATCHER_CHANNEL_CAPACITY: usize = 20_000;

/// Configuration for a replay event loop.
pub(super) struct ReplayConfig {
    pub(super) volume_id: String,
    /// The volume's path space. Journal replay only runs for a `has_event_journal()`
    /// volume (the boot disk today), so this is `root` in practice; it's threaded
    /// rather than hardcoded so the replay resolves in the same space as the live
    /// loop that follows it, and it's ready if a journaled mount-rooted kind appears.
    pub(super) space: IndexPathSpace,
    pub(super) since_event_id: u64,
    pub(super) estimated_total: Option<u64>,
    /// Whether to run the one-shot ledger heal after replay's initial phase.
    /// Set by `resume_or_scan` when this DB has never healed. Replay itself runs
    /// no full aggregate (only backfill), so it enqueues the heal's own
    /// `ComputeAllAggregates { source: Sql }` after the entries table is fully
    /// replayed. See `indexing/DETAILS.md` § "The dir_stats ledger".
    pub(super) heal_after_replay: bool,
}

// ── Shared helpers ───────────────────────────────────────────────────

/// Merge two `FsChangeEvent`s for the same normalized path, keeping the
/// most significant flags. Priority: `must_scan_sub_dirs` always wins,
/// then `item_removed`, then `item_created`, then `item_modified`.
/// The higher `event_id` is always kept.
pub(super) fn merge_fs_events(
    existing: &watcher::FsChangeEvent,
    incoming: &watcher::FsChangeEvent,
) -> watcher::FsChangeEvent {
    use watcher::FsEventFlags;

    let event_id = existing.event_id.max(incoming.event_id);

    // must_scan_sub_dirs always wins -- it triggers a subtree rescan
    if incoming.flags.must_scan_sub_dirs || existing.flags.must_scan_sub_dirs {
        return watcher::FsChangeEvent {
            path: existing.path.clone(),
            event_id,
            flags: FsEventFlags {
                must_scan_sub_dirs: true,
                ..existing.flags.clone()
            },
        };
    }

    // removed wins over everything else
    if incoming.flags.item_removed || existing.flags.item_removed {
        return watcher::FsChangeEvent {
            path: existing.path.clone(),
            event_id,
            flags: FsEventFlags {
                item_removed: true,
                item_is_file: incoming.flags.item_is_file || existing.flags.item_is_file,
                item_is_dir: incoming.flags.item_is_dir || existing.flags.item_is_dir,
                ..Default::default()
            },
        };
    }

    // created > modified
    if incoming.flags.item_created || existing.flags.item_created {
        return watcher::FsChangeEvent {
            path: existing.path.clone(),
            event_id,
            flags: FsEventFlags {
                item_created: true,
                item_is_file: incoming.flags.item_is_file || existing.flags.item_is_file,
                item_is_dir: incoming.flags.item_is_dir || existing.flags.item_is_dir,
                ..Default::default()
            },
        };
    }

    // Otherwise keep the incoming event (newer) with the higher event_id
    watcher::FsChangeEvent {
        path: existing.path.clone(),
        event_id,
        flags: incoming.flags.clone(),
    }
}

/// Open a read-only DB connection with a bounded retry. Used by the live
/// and replay event loops at startup. With `busy_timeout` already set in
/// `apply_pragmas` the first attempt almost always succeeds; the retry is
/// here so a one-off open error can't permanently disable live indexing.
pub(super) async fn open_read_conn_with_retry(db_path: &Path) -> Result<Connection, store::IndexStoreError> {
    match IndexStore::open_read_connection(db_path) {
        Ok(c) => Ok(c),
        Err(e) => {
            log::warn!("Open read connection failed, retrying in 100ms: {e}");
            tokio::time::sleep(Duration::from_millis(100)).await;
            IndexStore::open_read_connection(db_path)
        }
    }
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests;
