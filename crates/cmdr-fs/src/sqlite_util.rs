//! Small SQLite helpers shared by every store: the process-wide page-cache slab
//! and the connection factories that install it, the per-connection page-cache
//! budgets that have to ADD UP to that slab, the runtime reading of what the slab
//! actually holds (which the app's `get_memory_diagnostics` folds in, since the
//! slab is otherwise an anonymous 64 MiB of the Rust heap), the per-thread
//! connection cache the read paths keep (and the live count that watches it
//! against its budget), and freelist reclamation for the writer threads.
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
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

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
/// 64 MiB is the sum of the two things that can fill it: a whole
/// `wal_autocheckpoint` window each for the
/// [`CONCURRENTLY_SCANNING_WRITERS`] it's sized for, plus the entire
/// [`READ_CONNECTION_BUDGET`] at [`READ_PAGE_CACHE_KIB`] apiece. The const
/// assertion beside those keeps the arithmetic honest.
///
/// ⚠️ A slab SMALLER than what the budgets add up to isn't a cap, it's a
/// treadmill: it runs permanently full, `pcache1`'s under-pressure flag latches
/// on, and every fetch recycles from the global LRU under the `PGroup` mutex
/// (measured: 132 read connections scanning continuously held 63 of the 64 MiB
/// at the old 8 MiB read budget, 17 MiB at the current one —
/// `sqlite_util/page_cache_probe.rs`, release build, 2026-08-21).
///
/// Rationale, the fixed-cost tradeoff, and the alternatives weighed:
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

// ── Page-cache accounting ────────────────────────────────────────────

/// What SQLite's process-wide page memory looks like right now.
///
/// The slab is one leaked Rust allocation, so it lands INSIDE the app's
/// mimalloc total with nothing naming it: a memory reading that stops at
/// "the Rust heap holds X" leaves 64 MiB of X anonymous. These counters name
/// it, and say how much of it is real — the slab is handed out zeroed and only
/// the slots SQLite actually takes become dirty pages.
///
/// Read alongside [`live_read_connections`]: `used_bytes` pegged at
/// `slab_bytes` with the connection count past [`READ_CONNECTION_BUDGET`] is
/// the treadmill [`SHARED_PAGE_CACHE_BYTES`] describes, not a healthy cache.
// DEFAULT-OK: all zeros is the truthful reading for a process whose SQLite never
// initialized — no slab, no connections, so no pages held anywhere. It's the one
// failure path [`query_page_cache_usage`] has.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PageCacheUsage {
    /// The installed slab's size in bytes, or `0` when it isn't installed
    /// (`SharedPageCache::TooLate` or `Rejected`), in which case every page
    /// below is coming from the heap.
    pub slab_bytes: u64,
    /// Slab bytes currently holding database pages.
    pub used_bytes: u64,
    /// The high-water mark of `used_bytes` over the process's life. A peak at
    /// `slab_bytes` means the slab ran full at least once, even if it isn't now.
    pub peak_used_bytes: u64,
    /// Page-cache bytes SQLite took from the HEAP because the slab couldn't
    /// serve them. Expected to be zero; anything else means page memory is
    /// growing outside the budget.
    pub overflow_bytes: u64,
    /// The high-water mark of `overflow_bytes`.
    pub peak_overflow_bytes: u64,
}

/// Read SQLite's own page-cache accounting, converted to bytes.
///
/// Cheap: two `sqlite3_status64` calls, each taking a static mutex briefly. Safe
/// to call before any connection exists — it installs the slab first, so reading
/// the counters can't be what defeats it.
pub fn query_page_cache_usage() -> PageCacheUsage {
    let (slot_bytes, slab_bytes) = match ensure_shared_page_cache() {
        SharedPageCache::Installed { slot_bytes, slots } => (slot_bytes as u64, (slot_bytes * slots) as u64),
        SharedPageCache::TooLate | SharedPageCache::Rejected(_) => (0, 0),
    };

    // The counters live behind a static mutex SQLite allocates during
    // `sqlite3_initialize`, so reading them initializes the library. That's the
    // same door a connection open goes through, and the slab above has already
    // gone through `sqlite3_config` by now, so the one ordering that matters
    // (config, then initialize) holds either way.
    // SAFETY: `sqlite3_initialize` takes no arguments, is idempotent, and is
    // documented as callable from any thread.
    if unsafe { ffi::sqlite3_initialize() } != ffi::SQLITE_OK {
        return PageCacheUsage::default();
    }

    // `PAGECACHE_USED` counts SLOTS, `PAGECACHE_OVERFLOW` counts BYTES. Mixing
    // those up is a 4,000× reading.
    let (used_slots, peak_used_slots) = status_counter(ffi::SQLITE_STATUS_PAGECACHE_USED);
    let (overflow_bytes, peak_overflow_bytes) = status_counter(ffi::SQLITE_STATUS_PAGECACHE_OVERFLOW);

    PageCacheUsage {
        slab_bytes,
        used_bytes: used_slots * slot_bytes,
        peak_used_bytes: peak_used_slots * slot_bytes,
        overflow_bytes,
        peak_overflow_bytes,
    }
}

/// One `sqlite3_status64` counter as `(current, highwater)`, or zeros if SQLite
/// rejects the op. A diagnostic reading is never worth an abort.
fn status_counter(op: c_int) -> (u64, u64) {
    let mut current: i64 = 0;
    let mut highwater: i64 = 0;
    // SAFETY: `sqlite3_status64` writes two `sqlite3_int64` out-parameters, both
    // live `i64`s here, and takes no ownership. Callable from any thread once
    // SQLite is initialized, which the caller above guarantees.
    let rc = unsafe { ffi::sqlite3_status64(op, &raw mut current, &raw mut highwater, 0) };
    if rc == ffi::SQLITE_OK {
        (current.max(0) as u64, highwater.max(0) as u64)
    } else {
        (0, 0)
    }
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

/// Page cache for a READ-ONLY connection, in KiB, applied as the negative
/// `PRAGMA cache_size` form.
///
/// NOT a reservation, and barely a per-connection cap. Under the unified `PGroup`
/// the bundled build runs (`SQLITE_ENABLE_MEMORY_MANAGEMENT`), a cache that has
/// reached its own `nMax` RECYCLES the global LRU's tail instead of allocating,
/// and that tail may belong to any connection — so a hot enrichment connection
/// still grows a real working set, by taking pages back from idle peers.
///
/// What the number really buys is process-wide. SQLite's ONE ceiling on retained
/// pages is `pGroup->nMaxPage`, the SUM of every open connection's `cache_size`;
/// past it an unpinned page is freed outright rather than kept. Read connections
/// therefore contribute `count × this number`, with `count` tracking tokio's
/// blocking-thread pool. So this is sized so that [`READ_CONNECTION_BUDGET`] of
/// them fit inside [`SHARED_PAGE_CACHE_BYTES`] beside the writers, which is
/// asserted below — not sized for what one connection would enjoy.
///
/// ⚠️ Safe to be this small ONLY for a read-only connection: `pcache.c` keeps
/// `eCreate == 2` while a cache has no dirty pages, so `pcache1FetchStage2`'s
/// "abort when the cache is nearly full" step (which tests `nMax * 9/10`) can
/// never fire on one. A writer has dirty pages, hence [`WRITE_PAGE_CACHE_KIB`].
///
/// ⚠️ `cache_size` ALSO sets the sorter's in-memory budget (`vdbesort.c`), whose
/// floor is 1 MiB, so a read connection running a big `ORDER BY` sorts against
/// that floor now. Which query, and why it's affordable: the `DETAILS.md` section
/// below.
///
/// Mechanism, the amalgamation references, and the sizing:
/// `indexing/store/DETAILS.md` § "SQLite page memory is one process-wide slab".
pub const READ_PAGE_CACHE_KIB: i64 = 128;

/// How many read connections the sizing budgets for, process-wide.
///
/// Nothing bounds the real count structurally: read connections are thread-local
/// and live as long as their thread (`indexing/read/enrichment.rs`'s
/// `THREAD_CONNS`, `ImportanceIndex`'s `READ_CONNS`), each holding up to
/// [`THREAD_CONN_SLOTS`], so the count tracks tokio's blocking-thread pool. A
/// profiled prod session had **156 open** across 69 blocking threads (v0.36.2,
/// ~10 h uptime, macOS 26.5.2, `lsof` + `footprint -s`, 2026-07-28); 256 covers
/// that with room for the three-slot cache to hold more per thread.
///
/// So it's a budget, and it is WATCHED rather than trusted:
/// [`live_read_connections`] counts what the caches actually hold, and the first
/// crossing logs a `warn!` naming the ceiling it implies.
pub const READ_CONNECTION_BUDGET: usize = 256;

/// How many write connections can be filling their page cache at once: the two
/// concurrently scanning volumes [`SHARED_PAGE_CACHE_BYTES`] is sized for.
///
/// More write connections than this exist (one per open database), but a writer
/// that isn't scanning holds nothing, and two volumes scanning at once is the
/// busiest shape the app produces.
const CONCURRENTLY_SCANNING_WRITERS: usize = 2;

const _: () = assert!(
    READ_PAGE_CACHE_KIB < WRITE_PAGE_CACHE_KIB,
    "read connections must stay cheaper than the single write connection"
);

/// The whole design in one line: SQLite's own ceiling on retained pages
/// (`pGroup->nMaxPage` = `Σ cache_size`) never exceeds the memory we handed it.
///
/// Break it and the 64 MiB stops describing anything: the slab runs permanently
/// full, `pcache1`'s under-pressure flag latches on, and every page fetch recycles
/// from the global LRU under the `PGroup` mutex instead of the cheap path — with
/// nothing ever shrinking back at idle, because the "free this page outright"
/// branch in `pcache1Unpin` only fires above `nMaxPage`.
const _: () = assert!(
    CONCURRENTLY_SCANNING_WRITERS * (WRITE_PAGE_CACHE_KIB as usize)
        + READ_CONNECTION_BUDGET * (READ_PAGE_CACHE_KIB as usize)
        <= SHARED_PAGE_CACHE_BYTES / 1024,
    "the writers the slab is sized for plus the whole read-connection budget must fit inside the slab"
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
                let held = self.entries.len();
                self.entries.retain(|(p, _, _)| p != db_path);
                count_read_connections_closed(held - self.entries.len());
                let conn = open(db_path)?;
                if self.entries.len() >= self.capacity {
                    self.entries.pop();
                    count_read_connections_closed(1);
                }
                self.entries.insert(0, (db_path.to_path_buf(), generation, conn));
                count_read_connection_opened();
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

impl Drop for ThreadConnCache {
    /// A thread dying takes its connections with it, so the process-wide count
    /// has to follow. Tokio retires an idle blocking thread after ten seconds,
    /// so this runs routinely rather than only at shutdown.
    fn drop(&mut self) {
        count_read_connections_closed(self.entries.len());
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
mod page_cache_probe;
#[cfg(test)]
mod tests;
