//! Small SQLite helpers shared by every store: the process-wide page-cache slab
//! and the connection factories that install it, the per-connection page-cache
//! budget, the per-thread connection cache the read paths keep, and freelist
//! reclamation for the writer threads.
//!
//! It lives here rather than in the app because the slab is exactly ONE per
//! process and five stores share it — the three index DBs plus the agent's and the
//! operation log's — so the helpers belong in the crate all of them depend on.
//!
//! ❌ The test-only items and the open counter are gated on
//! `any(test, feature = "testing")`, never bare `cfg(test)`: a consumer compiles
//! this crate as a plain dependency, where `cfg(test)` is NOT set, so the counter
//! would silently stop recording inside their suites. See `../CLAUDE.md`.

use std::ffi::{c_int, c_void};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use rusqlite::{Connection, OpenFlags, ffi};

// ── Process-wide shared page cache ───────────────────────────────────

/// The whole process's SQLite page-cache budget, in bytes (64 MiB).
///
/// SQLite serves every cached database page out of this one slab
/// (`SQLITE_CONFIG_PAGECACHE`), so total page memory is THIS number no matter
/// how many connections exist. That's the property the per-connection
/// `cache_size` budgets below can't have: read connections are thread-local and
/// live as long as their thread, so their count tracks tokio's blocking-thread
/// pool rather than anything semantic (156 were open in a profiled prod session:
/// v0.36.2, ~10 h uptime, macOS 26.5.2, `lsof` + `footprint -s`, 2026-07-28).
///
/// 64 MiB holds a whole `wal_autocheckpoint` window for two concurrently
/// scanning volumes ([`WRITE_PAGE_CACHE_KIB`] each) plus every hot DB's upper
/// b-tree levels and a real leaf working set, and it's ~5x below the 310 MB
/// ceiling the per-connection budgets alone imposed. Rationale, the fixed-cost
/// tradeoff, and the alternatives weighed:
/// `indexing/store/DETAILS.md` § "SQLite page memory is one process-wide slab".
pub const SHARED_PAGE_CACHE_BYTES: usize = 64 * 1024 * 1024;

/// The database page size every store runs (SQLite's default). The slab's slot
/// size is this plus SQLite's own per-page header, so one slot holds one page.
const DB_PAGE_BYTES: usize = 4096;

/// The outcome of handing SQLite its shared page-cache slab. Recorded once per
/// process by [`ensure_shared_page_cache`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SharedPageCache {
    /// The slab is installed: `slots` cache lines of `slot_bytes` each.
    Installed {
        /// Bytes per cache slot: one database page plus SQLite's per-page header.
        slot_bytes: usize,
        /// How many slots the slab holds.
        slots: usize,
    },
    /// SQLite was already initialized, so it refused the slab. Something opened
    /// a connection outside this module's factories and won the race; the
    /// process falls back to per-connection budgets alone.
    TooLate,
    /// `sqlite3_config` rejected the slab for some other reason (the raw code).
    Rejected(c_int),
}

impl SharedPageCache {
    /// Total bytes SQLite will serve page-cache allocations from, or `0` when
    /// the slab isn't installed.
    #[cfg(any(test, feature = "testing"))]
    pub fn bytes(&self) -> usize {
        match self {
            Self::Installed { slot_bytes, slots } => slot_bytes * slots,
            Self::TooLate | Self::Rejected(_) => 0,
        }
    }
}

static SHARED_PAGE_CACHE: LazyLock<SharedPageCache> = LazyLock::new(install_shared_page_cache);

/// Install the process-wide page-cache slab if it isn't installed yet, and
/// report the outcome. Idempotent and cheap after the first call.
///
/// `sqlite3_config` only works BEFORE SQLite initializes itself, and SQLite
/// initializes on the first connection open. So this runs from [`open`],
/// [`open_read_only`], and [`open_in_memory`] — the only ways the app opens a
/// connection — which makes "the slab is installed first" true by construction
/// rather than by an ordering someone has to remember. The
/// `desktop-rust-sqlite-open-direct` check keeps it that way.
pub fn ensure_shared_page_cache() -> SharedPageCache {
    *SHARED_PAGE_CACHE
}

fn install_shared_page_cache() -> SharedPageCache {
    // Ask SQLite for its own per-page header size instead of guessing: it's
    // build- and version-dependent, and a slot too small for `page + header`
    // fails silently — `pcache1Alloc` simply never takes the slab and every
    // allocation falls through to the heap.
    let mut header_bytes: c_int = 0;
    // SAFETY: `sqlite3_config` is variadic; `SQLITE_CONFIG_PCACHE_HDRSZ` takes
    // exactly one `int *` out-parameter, and `header_bytes` is a live, aligned
    // `c_int` for the whole call. Called before any connection exists, so no
    // other thread is inside SQLite.
    let rc = unsafe { ffi::sqlite3_config(ffi::SQLITE_CONFIG_PCACHE_HDRSZ, &raw mut header_bytes) };
    if rc != ffi::SQLITE_OK {
        return report(classify(rc));
    }

    let slot_bytes = (DB_PAGE_BYTES + header_bytes.max(0) as usize).next_multiple_of(8);
    let slots = SHARED_PAGE_CACHE_BYTES / slot_bytes;

    // `u64` elements give the 8-byte alignment SQLite requires of `pMem`; a
    // `Vec<u8>` only promises alignment 1.
    let mut slab: Box<[u64]> = vec![0u64; (slot_bytes * slots).div_ceil(8)].into_boxed_slice();
    // SAFETY: `sqlite3_config` is variadic; `SQLITE_CONFIG_PAGECACHE` takes
    // exactly `(void *pBuf, int sz, int N)`. `pBuf` is 8-byte aligned (a `u64`
    // allocation) and at least `sz * N` bytes long by construction above, and
    // it outlives SQLite because it's leaked on the success path below. Called
    // before any connection exists, so no other thread is inside SQLite.
    let rc = unsafe {
        ffi::sqlite3_config(
            ffi::SQLITE_CONFIG_PAGECACHE,
            slab.as_mut_ptr().cast::<c_void>(),
            slot_bytes as c_int,
            slots as c_int,
        )
    };
    if rc != ffi::SQLITE_OK {
        // SQLite kept no pointer, so the slab drops here rather than leaking.
        return report(classify(rc));
    }
    // SQLite holds this pointer for the life of the process (there's no
    // un-configure), so the slab must never be freed. Leaking it IS the
    // lifetime.
    let _slab: &'static mut [u64] = Box::leak(slab);

    report(SharedPageCache::Installed { slot_bytes, slots })
}

fn classify(rc: c_int) -> SharedPageCache {
    if rc == ffi::SQLITE_MISUSE {
        SharedPageCache::TooLate
    } else {
        SharedPageCache::Rejected(rc)
    }
}

/// Log the outcome once (this runs inside the `LazyLock`, so exactly once) and
/// pass it through. A missing slab isn't fatal — SQLite falls back to the heap
/// and the per-connection budgets still cap each connection — but it silently
/// restores the old memory profile, so it's worth a loud line.
fn report(outcome: SharedPageCache) -> SharedPageCache {
    match outcome {
        SharedPageCache::Installed { slot_bytes, slots } => {
            log::debug!(
                target: "sqlite",
                "shared page cache installed: {} x {slot_bytes} B = {} MiB",
                crate::pluralize::pluralize(slots as u64, "slot"),
                (slot_bytes * slots) / (1024 * 1024)
            );
        }
        SharedPageCache::TooLate => {
            log::warn!(
                target: "sqlite",
                "shared page cache not installed: SQLite was already initialized (a connection was opened outside `sqlite_util`); page memory now scales with connection count again"
            );
        }
        SharedPageCache::Rejected(rc) => {
            log::warn!(target: "sqlite", "shared page cache rejected by sqlite3_config (rc={rc})");
        }
    }
    outcome
}

// ── Connection factories ─────────────────────────────────────────────

/// Open a read-write connection (creating the file if missing), installing the
/// shared page cache first.
///
/// ❌ Don't call `rusqlite::Connection::open*` directly anywhere else: the first
/// connection in the process initializes SQLite, and after that the slab can no
/// longer be installed. Enforced by `desktop-rust-sqlite-open-direct`.
pub fn open(db_path: &Path) -> rusqlite::Result<Connection> {
    ensure_shared_page_cache();
    #[cfg(any(test, feature = "testing"))]
    record_open(db_path);
    Connection::open(db_path)
}

/// Open a read-only connection, installing the shared page cache first. See
/// [`open`] for why every open funnels through this module.
pub fn open_read_only(db_path: &Path) -> rusqlite::Result<Connection> {
    ensure_shared_page_cache();
    #[cfg(any(test, feature = "testing"))]
    record_open(db_path);
    Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
}

/// Open an in-memory database, installing the shared page cache first. See
/// [`open`] for why every open funnels through this module. Test-only today:
/// production always opens a file.
#[cfg(any(test, feature = "testing"))]
pub fn open_in_memory() -> rusqlite::Result<Connection> {
    ensure_shared_page_cache();
    Connection::open_in_memory()
}

/// Test-only: how many times this module opened a connection to `db_path`. Keyed
/// by path so a test on its own temp DB is unaffected by connections other tests
/// open in parallel. The reopen counter the thread-local cache's reuse tests
/// assert on (identity via timing would only flake).
#[cfg(any(test, feature = "testing"))]
pub fn open_count_for(db_path: &Path) -> u64 {
    OPEN_COUNTS
        .lock_ignore_poison()
        .get(db_path)
        .copied()
        .unwrap_or_default()
}

#[cfg(any(test, feature = "testing"))]
use crate::ignore_poison::IgnorePoison;

#[cfg(any(test, feature = "testing"))]
static OPEN_COUNTS: LazyLock<std::sync::Mutex<std::collections::HashMap<PathBuf, u64>>> =
    LazyLock::new(|| std::sync::Mutex::new(std::collections::HashMap::new()));

#[cfg(any(test, feature = "testing"))]
fn record_open(db_path: &Path) {
    *OPEN_COUNTS
        .lock_ignore_poison()
        .entry(db_path.to_path_buf())
        .or_default() += 1;
}

// ── Page cache budget (per connection) ───────────────────────────────

/// Page cache for a WRITE connection, in KiB (16 MiB), applied as the negative
/// `PRAGMA cache_size` form.
///
/// Coupled to `wal_autocheckpoint = 4000` (~16 MiB of 4 KiB pages, set in
/// `indexing::store::apply_pragmas`): the cache is sized to hold the pages a
/// whole autocheckpoint window dirties, so a big write batch commits without
/// evicting pages it's about to touch again. Change one, reconsider the other.
///
/// There is at most ONE write connection per DB (a single writer thread), so
/// this budget is claimed a handful of times process-wide, and it fits inside
/// [`SHARED_PAGE_CACHE_BYTES`] several times over.
pub const WRITE_PAGE_CACHE_KIB: i64 = 16_384;

/// Page cache for a READ-ONLY connection, in KiB (8 MiB), applied as the
/// negative `PRAGMA cache_size` form.
///
/// With the shared slab installed this is an upper bound per connection, NOT a
/// reservation: pages come out of [`SHARED_PAGE_CACHE_BYTES`], SQLite runs one
/// global LRU across every cache (the bundled build defines
/// `SQLITE_ENABLE_MEMORY_MANAGEMENT`, so all caches share one `PGroup`), and a
/// connection that never runs a query costs nothing. So a hot enrichment
/// connection gets a generous working set while a hundred idle ones don't add
/// up — which is exactly what the per-connection-only model couldn't do.
///
/// Still smaller than [`WRITE_PAGE_CACHE_KIB`]: reads never commit or
/// checkpoint, so there's no dirty-page window to hold, and 8 MiB comfortably
/// covers the upper interior levels of the hot b-trees plus a directory's worth
/// of leaves (the enrichment path's point lookups and range scans).
pub const READ_PAGE_CACHE_KIB: i64 = 8_192;

const _: () = assert!(
    READ_PAGE_CACHE_KIB < WRITE_PAGE_CACHE_KIB,
    "read connections must stay cheaper than the single write connection"
);

const _: () = assert!(
    (WRITE_PAGE_CACHE_KIB as usize) * 1024 * 2 <= SHARED_PAGE_CACHE_BYTES,
    "the shared slab must hold two concurrent writers' autocheckpoint windows"
);

/// Apply the page-cache budget for this connection's role: [`READ_PAGE_CACHE_KIB`]
/// when `readonly`, [`WRITE_PAGE_CACHE_KIB`] otherwise.
///
/// Every store's `apply_pragmas` calls this, so the split is set in ONE place and
/// a new store can't quietly hand read connections the writer's budget.
pub fn apply_page_cache(conn: &Connection, readonly: bool) -> rusqlite::Result<()> {
    let kib = if readonly {
        READ_PAGE_CACHE_KIB
    } else {
        WRITE_PAGE_CACHE_KIB
    };
    // Negative = KiB of memory; positive would mean a page COUNT, which varies
    // with `page_size` and is not what any of these budgets mean.
    conn.execute_batch(&format!("PRAGMA cache_size = -{kib};"))
}

/// Prepared statements a WRITE connection may keep compiled.
///
/// `rusqlite`'s own default is 16 (`STATEMENT_CACHE_DEFAULT_CAPACITY`), an LRU keyed
/// by SQL text. The index store alone holds **35** distinct `prepare_cached` sites
/// (`entries.rs` 25, `dir_stats.rs` 6, `meta.rs` 2, `mod.rs` 1, `connection.rs` 1),
/// so a writer working across them evicts and silently RE-COMPILES statements it is
/// about to use again — the exact cost `prepare_cached` is there to remove, with no
/// error and no failing test to show for it.
///
/// 64 leaves headroom for the store to grow without anyone noticing this ceiling.
/// The cost is one compiled VDBE program per slot on the handful of write
/// connections (one per DB), not per read connection, so it is a rounding error
/// against [`WRITE_PAGE_CACHE_KIB`].
///
/// ⚠️ Raise this when the store's `prepare_cached` count approaches it. A statement
/// cache smaller than the working set is worse than none: it pays the lookup and
/// still re-compiles.
pub const WRITE_STATEMENT_CACHE_CAPACITY: usize = 64;

/// Size the connection's prepared-statement cache for its role.
///
/// READ connections keep `rusqlite`'s default: they are thread-local and there are
/// many of them (132 open in a profiled prod session), so a large per-connection
/// statement cache there multiplies by a number nothing controls — the same
/// reasoning as [`READ_PAGE_CACHE_KIB`]. The single writer per DB gets
/// [`WRITE_STATEMENT_CACHE_CAPACITY`].
///
/// Called from every store's `apply_pragmas`, beside [`apply_page_cache`], so the
/// split is set in ONE place.
pub fn apply_statement_cache(conn: &Connection, readonly: bool) {
    if !readonly {
        conn.set_prepared_statement_cache_capacity(WRITE_STATEMENT_CACHE_CAPACITY);
    }
}

/// The connection's page-cache budget in KiB. `PRAGMA cache_size` echoes back the
/// negative KiB form [`apply_page_cache`] sets, so flip the sign.
#[cfg(any(test, feature = "testing"))]
pub fn page_cache_kib(conn: &Connection) -> i64 {
    let raw: i64 = conn
        .pragma_query_value(None, "cache_size", |row| row.get(0))
        .expect("read cache_size");
    assert!(raw < 0, "cache_size should be set in negative-KiB form, got {raw}");
    -raw
}

// ── Per-thread connection cache ──────────────────────────────────────

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
/// recompiling those statements is the expensive part. A handful of slots costs
/// nothing now that [`SHARED_PAGE_CACHE_BYTES`] decouples memory from connection
/// count.
///
/// Not thread-safe by design: it lives in a `thread_local!` `RefCell`, so there
/// is no lock. ❌ Don't wrap it in a mutex.
pub struct ThreadConnCache {
    /// Most-recently-used first. Never longer than `capacity`.
    entries: Vec<(PathBuf, u64, Connection)>,
    capacity: usize,
}

/// Slots per thread. Two covers the ordinary two-pane case (left pane on the
/// boot disk, right pane on a NAS share); the third absorbs a background reader
/// (search, the importance scheduler) landing on the same blocking thread
/// without evicting either pane.
pub const THREAD_CONN_SLOTS: usize = 3;

impl ThreadConnCache {
    /// An empty cache holding at most `capacity` connections.
    pub const fn new(capacity: usize) -> Self {
        Self {
            entries: Vec::new(),
            capacity,
        }
    }

    /// Run `f` against the connection for `(db_path, generation)`, opening one
    /// through `open` when this thread holds no live match.
    ///
    /// A hit moves the entry to the front; a miss evicts the least recently used
    /// entry once the cache is full. An entry for `db_path` at a DIFFERENT
    /// generation is dropped before the reopen, so a caller's `invalidate()`
    /// still retires the stale connection rather than leaving two around.
    pub fn with<T, E>(
        &mut self,
        db_path: &Path,
        generation: u64,
        open: impl FnOnce(&Path) -> Result<Connection, E>,
        f: impl FnOnce(&Connection) -> T,
    ) -> Result<T, E> {
        match self
            .entries
            .iter()
            .position(|(p, g, _)| p == db_path && *g == generation)
        {
            Some(0) => {}
            Some(hit) => {
                let entry = self.entries.remove(hit);
                self.entries.insert(0, entry);
            }
            None => {
                // Retire a same-path entry at a stale generation: the caller
                // invalidated it, so it must not linger behind the new one.
                self.entries.retain(|(p, _, _)| p != db_path);
                let conn = open(db_path)?;
                if self.entries.len() >= self.capacity {
                    self.entries.pop();
                }
                self.entries.insert(0, (db_path.to_path_buf(), generation, conn));
            }
        }
        let (_, _, conn) = self
            .entries
            .first()
            .expect("the MRU entry exists: every branch above leaves a match at index 0");
        Ok(f(conn))
    }

    /// Test-only: the generation this thread holds for `db_path`, or `None` when
    /// it holds no connection to it.
    #[cfg(any(test, feature = "testing"))]
    pub fn generation_for(&self, db_path: &Path) -> Option<u64> {
        self.entries.iter().find(|(p, _, _)| p == db_path).map(|(_, g, _)| *g)
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

// ── Freelist reclamation ─────────────────────────────────────────────

/// Reclaim freed pages via `PRAGMA incremental_vacuum`, stepping until the pragma
/// is exhausted.
///
/// SQLite compiles `incremental_vacuum` into a loop that frees ONE page per
/// `sqlite3_step()`, yielding a result row after each page. `execute_batch` steps
/// a statement exactly once, so it frees a single page no matter the cap — which
/// strands the freelist, draining it one page per tick. Prepare the pragma and
/// step it to completion instead.
///
/// `cap` bounds how many pages to reclaim; `None` drains the whole freelist.
pub fn run_incremental_vacuum(conn: &Connection, cap: Option<i64>) -> rusqlite::Result<()> {
    let sql = match cap {
        Some(n) => format!("PRAGMA incremental_vacuum({n});"),
        None => "PRAGMA incremental_vacuum;".to_string(),
    };
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query([])?;
    while rows.next()?.is_some() {}
    Ok(())
}

#[cfg(test)]
mod tests;
