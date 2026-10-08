//! The per-thread read-connection cache, and the one way a thread is asked to
//! let go of what it caches.
//!
//! Its own file because it is its own mechanism with its own state (the live
//! count, the retirement epoch and table). Everything here is re-exported from
//! `sqlite_util`, which is the path callers use.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{LazyLock, Mutex};

use rusqlite::Connection;

use super::{READ_CONNECTION_BUDGET, READ_PAGE_CACHE_KIB, SHARED_PAGE_CACHE_BYTES};
use crate::ignore_poison::IgnorePoison;

/// A small per-thread LRU of open read connections, keyed by db path plus the
/// caller's invalidation generation.
///
/// Both read paths (`indexing::read::enrichment`'s `ReadPool` and
/// `ImportanceIndex`) keep their connections in a thread-local so enrichment
/// never takes a lock on the hot path. Holding ONE slot made that lock-freedom
/// expensive in the ordinary two-pane case: a thread alternating between the
/// left pane's volume and the right pane's closed and reopened on every
/// alternation, re-running the pragmas and the collation registration and
/// throwing away the connection's whole `prepare_cached` statement cache —
/// recompiling those statements is the expensive part. A handful of slots is
/// affordable because [`READ_PAGE_CACHE_KIB`] is sized against
/// [`READ_CONNECTION_BUDGET`] rather than against one connection, so a slot costs
/// 128 KiB of the page ceiling rather than 8 MiB of it.
///
/// Every entry is counted in [`live_read_connections`], including the ones this
/// cache evicts and the ones it takes down with a dying thread, so the budget is
/// observable rather than hoped for.
///
/// Not thread-safe by design: it lives in a `thread_local!` `RefCell`, so there
/// is no lock. ❌ Don't wrap it in a mutex. That is also why nothing can close
/// another thread's connections: [`retire_read_connections`] asks, and each cache
/// answers on its own thread's next use.
pub struct ThreadConnCache {
    /// Most-recently-used first. Never longer than `capacity`.
    entries: Vec<CachedConn>,
    capacity: usize,
    /// The [`RETIRE_EPOCH`] this cache last swept its entries against.
    swept_at: u64,
}

/// One cached read connection.
struct CachedConn {
    db_path: PathBuf,
    /// The caller's invalidation generation it was opened under.
    generation: u64,
    /// The [`RETIRE_EPOCH`] its thread had seen when it was opened, which is what
    /// tells a connection a retirement is aimed at from one opened after it.
    opened_at: u64,
    conn: Connection,
}

/// Slots per thread. Two covers the ordinary two-pane case (left pane on the
/// boot disk, right pane on a NAS share); the third absorbs a background reader
/// (search, the importance scheduler) landing on the same blocking thread
/// without evicting either pane.
pub const THREAD_CONN_SLOTS: usize = 3;

/// Read connections the process's [`ThreadConnCache`]s hold right now.
static LIVE_READ_CONNECTIONS: AtomicUsize = AtomicUsize::new(0);

/// Latched the first time the count passes [`READ_CONNECTION_BUDGET`], so the
/// warning is one line rather than one per open from then on.
static READ_BUDGET_EXCEEDED: AtomicBool = AtomicBool::new(false);

/// How many read connections the process's [`ThreadConnCache`]s hold right now,
/// across every thread.
///
/// That is the DURABLE read population and the term the sizing is about: those
/// connections live as long as their thread, so their count tracks tokio's
/// blocking pool. Multiply by [`READ_PAGE_CACHE_KIB`] for their share of SQLite's
/// global ceiling on retained pages, and read it against
/// [`READ_CONNECTION_BUDGET`], which is what the page cache is sized for.
///
/// ⚠️ NOT every read connection in the process. The media, agent, and
/// operation-log stores open a read connection per call and drop it, so they
/// never enter this count; they add [`READ_PAGE_CACHE_KIB`] apiece to
/// `pGroup->nMaxPage` only for the life of the call.
pub fn live_read_connections() -> usize {
    LIVE_READ_CONNECTIONS.load(Ordering::Relaxed)
}

/// Count one newly opened read connection, and say so once if that puts the
/// process past the budget its page cache was sized for.
fn count_read_connection_opened() {
    let live = LIVE_READ_CONNECTIONS.fetch_add(1, Ordering::Relaxed) + 1;
    if live > READ_CONNECTION_BUDGET && !READ_BUDGET_EXCEEDED.swap(true, Ordering::Relaxed) {
        let ceiling_mib = (live * READ_PAGE_CACHE_KIB as usize) / 1024;
        log::warn!(
            target: "sqlite",
            "{} open, past the {READ_CONNECTION_BUDGET} the page cache is sized for; their share of SQLite's global page ceiling is now ~{ceiling_mib} MiB against a {} MiB slab, so the slab can run permanently full",
            crate::pluralize::pluralize(live as u64, "read connection"),
            SHARED_PAGE_CACHE_BYTES / (1024 * 1024)
        );
    }
}

/// Count `n` read connections going away (evicted, retired, or dropped with
/// their thread).
fn count_read_connections_closed(n: usize) {
    if n > 0 {
        LIVE_READ_CONNECTIONS.fetch_sub(n, Ordering::Relaxed);
    }
}

impl ThreadConnCache {
    /// An empty cache holding at most `capacity` connections.
    pub const fn new(capacity: usize) -> Self {
        Self {
            entries: Vec::new(),
            capacity,
            swept_at: 0,
        }
    }

    /// Run `f` against the connection for `(db_path, generation)`, opening one
    /// through `open` when this thread holds no live match.
    ///
    /// A hit moves the entry to the front; a miss evicts the least recently used
    /// entry once the cache is full. An entry for `db_path` at a DIFFERENT
    /// generation is dropped before the reopen, so a caller's `invalidate()`
    /// still retires the stale connection rather than leaving two around.
    ///
    /// Every call first closes whatever [`retire_read_connections`] has asked for
    /// since this thread's last one, for ANY database: that is the only moment a
    /// retirement can reach a thread-local cache.
    pub fn with<T, E>(
        &mut self,
        db_path: &Path,
        generation: u64,
        open: impl FnOnce(&Path) -> Result<Connection, E>,
        f: impl FnOnce(&Connection) -> T,
    ) -> Result<T, E> {
        let epoch = RETIRE_EPOCH.load(Ordering::Acquire);
        if epoch != self.swept_at {
            self.close_retired(epoch);
        }
        match self
            .entries
            .iter()
            .position(|entry| entry.db_path == db_path && entry.generation == generation)
        {
            Some(0) => {}
            Some(hit) => {
                let entry = self.entries.remove(hit);
                self.entries.insert(0, entry);
            }
            None => {
                // Retire a same-path entry at a stale generation: the caller
                // invalidated it, so it must not linger behind the new one.
                let held = self.entries.len();
                self.entries.retain(|entry| entry.db_path != db_path);
                count_read_connections_closed(held - self.entries.len());
                let conn = open(db_path)?;
                if self.entries.len() >= self.capacity {
                    self.entries.pop();
                    count_read_connections_closed(1);
                }
                self.entries.insert(
                    0,
                    CachedConn {
                        db_path: db_path.to_path_buf(),
                        generation,
                        opened_at: epoch,
                        conn,
                    },
                );
                count_read_connection_opened();
            }
        }
        let entry = self
            .entries
            .first()
            .expect("the MRU entry exists: every branch above leaves a match at index 0");
        Ok(f(&entry.conn))
    }

    /// Close every connection a retirement was aimed at, and note the epoch swept
    /// against so the next call is one atomic load again.
    ///
    /// The connections close AFTER the table's lock is released: closing one can
    /// checkpoint, and a retiring thread must never wait on that.
    fn close_retired(&mut self, epoch: u64) {
        let retired: Vec<CachedConn> = {
            let retired_at = RETIRED_AT.lock_ignore_poison();
            let (kept, retired) = std::mem::take(&mut self.entries)
                .into_iter()
                .partition(|entry| retired_at.get(&entry.db_path).is_none_or(|at| entry.opened_at >= *at));
            self.entries = kept;
            retired
        };
        count_read_connections_closed(retired.len());
        self.swept_at = epoch;
    }

    /// Test-only: the generation this thread holds for `db_path`, or `None` when
    /// it holds no connection to it.
    #[cfg(any(test, feature = "testing"))]
    pub fn generation_for(&self, db_path: &Path) -> Option<u64> {
        self.entries
            .iter()
            .find(|entry| entry.db_path == db_path)
            .map(|entry| entry.generation)
    }

    /// Test-only: how many connections this thread currently holds.
    #[cfg(any(test, feature = "testing"))]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Test-only: whether this thread holds no connections at all. Paired with
    /// [`len`](Self::len) because clippy won't take one without the other.
    #[cfg(any(test, feature = "testing"))]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl Drop for ThreadConnCache {
    /// A thread dying takes its connections with it, so the process-wide count
    /// has to follow. Tokio retires an idle blocking thread after ten seconds,
    /// so this runs routinely rather than only at shutdown.
    fn drop(&mut self) {
        count_read_connections_closed(self.entries.len());
    }
}

// ── Retiring cached read connections ─────────────────────────────────

/// Bumped by every [`retire_read_connections`]. A [`ThreadConnCache`] compares it
/// to the epoch it last swept at, so a thread with nothing retired pays one atomic
/// load per use.
static RETIRE_EPOCH: AtomicU64 = AtomicU64::new(0);

/// The epoch each retired database was last retired at. One entry per path ever
/// retired, so it grows by one per database the process deletes or lets go of.
static RETIRED_AT: LazyLock<Mutex<HashMap<PathBuf, u64>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// Ask every thread to close the read connections it caches to `db_path`.
///
/// For a database that is going away (deleted, or its volume stopped), so the
/// caches stop holding it open. It matters most for a delete: an unlinked file
/// keeps its blocks allocated until the last handle on it closes.
///
/// ⚠️ **A request, not a close.** A [`ThreadConnCache`] is thread-local, so only
/// its own thread can drop its connections, and it does so the next time it uses
/// the cache for any database. A thread that never reads again keeps its
/// connections until it exits, which tokio does to an idle blocking thread after
/// ten seconds. So the honest bound is "the owning thread's next read, or its
/// death", ❌ never "closed when this returns".
///
/// Connections opened AFTER the call are untouched, so a database recreated under
/// the same path is cached like any other. A delete calls it on both sides of the
/// unlink ([`delete_database`](super::delete_database) does), so a reader that opened in between isn't left
/// holding the unlinked file.
pub fn retire_read_connections(db_path: &Path) {
    // Under the lock, so the table and the epoch can't be seen disagreeing.
    let mut retired_at = RETIRED_AT.lock_ignore_poison();
    let epoch = RETIRE_EPOCH.fetch_add(1, Ordering::AcqRel) + 1;
    retired_at.insert(db_path.to_path_buf(), epoch);
}
