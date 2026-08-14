//! When to offer ground Cmdr gave up on back to the walks.
//!
//! A directory marked [`UnreadableCause::Abandoned`] is out of the coverage
//! frontier, so no walk will ever list it, so nothing will ever clear its cause on
//! its own. Something has to reopen it, or a mount that came back stays invisible
//! until the next full rescan.
//!
//! ## Why a backoff and not a timer
//!
//! Clearing the cause puts the whole subtree back in the frontier, and the next
//! walk over that scope pays the full failing read again — 15 s of stall timeout
//! per directory on a wedged mount, and that mount is usually still wedged. A flat
//! retry would re-pay that on every cycle, which is the bug this whole mechanism
//! exists to stop, just at a slower cadence. So the window grows: **1 h, then 4 h,
//! then 24 h**, per volume, persisted in that volume's `meta`.
//!
//! ## Armed by the mark, disarmed by success
//!
//! The window exists only while there is something to retry. `MarkDirsUnreadable`
//! arms it the first time a walk condemns anything; a retry that finds nothing left
//! to clear disarms it, so a volume with no abandoned ground pays exactly one `meta`
//! read per maintenance tick and never touches `entries` (the column carries no
//! index, so a speculative clear would be a full scan of every row on the volume).
//!
//! Re-arming is deliberately a no-op while armed: a walk that fails again after a
//! retry must not restart the window at the fast step, or the backoff never grows.
//!
//! ## What a cleared cause actually buys, today
//!
//! The ground goes back in the frontier, so the next search over that scope walks
//! it. ❌ Nothing here enqueues a walk of its own — the component that would (a
//! phase machine driving coverage on its own schedule) doesn't exist yet, and
//! adding a walk trigger to a maintenance tick would put background disk work
//! behind a clock nobody asked. A successful listing anywhere clears the cause
//! immediately regardless (`mark_dirs_listed`), which is the same contract
//! `Denied` heals under.

use std::time::Duration;

use rusqlite::Connection;

use crate::indexing::store::{IndexStore, IndexStoreError, UnreadableCause};

/// `meta` key: when the current retry window started (unix seconds). Absent means
/// nothing is waiting to be retried.
const RETRY_AT_KEY: &str = "abandoned_retry_at";
/// `meta` key: which step of [`BACKOFF`] the current window is on.
const RETRY_STEP_KEY: &str = "abandoned_retry_step";

/// How long to wait before each successive retry. The last entry repeats forever.
///
/// The first window is short because the common case that heals is a mount coming
/// back within the hour (a phone reconnected, a NAS woken up); the last is long
/// because the common case that does NOT heal is a File Provider domain for a
/// device that isn't coming back this week, and each retry over one of those costs
/// a stall timeout per directory.
const BACKOFF: [Duration; 3] = [
    Duration::from_secs(60 * 60),
    Duration::from_secs(4 * 60 * 60),
    Duration::from_secs(24 * 60 * 60),
];

/// One volume's retry state: when the current window opened, and how long it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RetryWindow {
    /// Unix seconds when this window started.
    opened_at: u64,
    /// Index into [`BACKOFF`], saturated at its last entry.
    step: usize,
}

impl RetryWindow {
    /// This window's length.
    fn length(self) -> Duration {
        BACKOFF[self.step.min(BACKOFF.len() - 1)]
    }

    /// Whether the window has elapsed at `now`.
    ///
    /// A window that opened in the FUTURE counts as elapsed: a backwards clock jump
    /// or an index carried over from a machine with a skewed clock must not wedge
    /// retries shut for years, and the cost of being wrong is one extra attempt.
    fn is_due(self, now: u64) -> bool {
        now < self.opened_at || now - self.opened_at >= self.length().as_secs()
    }

    /// The window after a retry that cleared `cleared` rows, or `None` to disarm.
    ///
    /// Nothing cleared means nothing is waiting any more (a successful listing
    /// healed it, or a truncating rescan wiped the rows), so the volume goes quiet
    /// and the next mark re-arms at the fast first step. Anything cleared means the
    /// ground is back in the frontier and might fail again, so the next window is
    /// the longer one.
    fn advanced(self, now: u64, cleared: usize) -> Option<Self> {
        (cleared > 0).then_some(Self {
            opened_at: now,
            step: (self.step + 1).min(BACKOFF.len() - 1),
        })
    }
}

/// Arm the retry window for this volume, unless one is already open.
///
/// Called when a walk's `MarkDirsUnreadable` commits with
/// [`UnreadableCause::Abandoned`]. ❌ Never restart an open window: a walk that
/// re-condemns the same ground right after a retry would otherwise pin the backoff
/// at its first step forever, and every retry re-pays the mount's timeouts.
pub(super) fn arm(conn: &Connection, now: u64) -> Result<(), IndexStoreError> {
    if read_window(conn)?.is_some() {
        return Ok(());
    }
    write_window(
        conn,
        RetryWindow {
            opened_at: now,
            step: 0,
        },
    )
}

/// Clear every `Abandoned` cause if this volume's retry window has elapsed,
/// returning how many rows that reopened (`None` when nothing was due).
///
/// The whole decision runs here, on the writer thread, because all of its inputs
/// live in this database: reading the window on another thread would race the
/// writes that move it, and a stale read means a retry that shouldn't have happened.
pub(super) fn clear_if_due(conn: &Connection, now: u64) -> Result<Option<usize>, IndexStoreError> {
    let Some(window) = read_window(conn)? else {
        return Ok(None);
    };
    if !window.is_due(now) {
        return Ok(None);
    }
    let cleared = IndexStore::clear_unreadable_cause(conn, UnreadableCause::Abandoned)?;
    match window.advanced(now, cleared) {
        Some(next) => write_window(conn, next)?,
        None => disarm(conn)?,
    }
    Ok(Some(cleared))
}

fn read_window(conn: &Connection) -> Result<Option<RetryWindow>, IndexStoreError> {
    let Some(opened_at) = IndexStore::get_meta(conn, RETRY_AT_KEY)?.and_then(|v| v.parse().ok()) else {
        return Ok(None);
    };
    let step = IndexStore::get_meta(conn, RETRY_STEP_KEY)?
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    Ok(Some(RetryWindow { opened_at, step }))
}

fn write_window(conn: &Connection, window: RetryWindow) -> Result<(), IndexStoreError> {
    IndexStore::update_meta(conn, RETRY_AT_KEY, &window.opened_at.to_string())?;
    IndexStore::update_meta(conn, RETRY_STEP_KEY, &window.step.to_string())
}

fn disarm(conn: &Connection) -> Result<(), IndexStoreError> {
    IndexStore::delete_meta(conn, RETRY_AT_KEY)?;
    IndexStore::delete_meta(conn, RETRY_STEP_KEY)
}

#[cfg(test)]
mod tests;
