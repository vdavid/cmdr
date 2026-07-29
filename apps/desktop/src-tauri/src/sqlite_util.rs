//! Small SQLite helpers shared by every store: the process-wide page-cache slab
//! and the connection factories that install it, the per-connection page-cache
//! budget, the per-thread connection cache the read paths keep, and freelist
//! reclamation for the writer threads.

use std::ffi::{c_int, c_void};
use std::path::Path;
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
    Installed { slot_bytes: usize, slots: usize },
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
    #[cfg(test)]
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
                "shared page cache installed: {slots} slots x {slot_bytes} B = {} MiB",
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
    Connection::open(db_path)
}

/// Open a read-only connection, installing the shared page cache first. See
/// [`open`] for why every open funnels through this module.
pub fn open_read_only(db_path: &Path) -> rusqlite::Result<Connection> {
    ensure_shared_page_cache();
    Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
}

/// Open an in-memory database, installing the shared page cache first. See
/// [`open`] for why every open funnels through this module. Test-only today:
/// production always opens a file.
#[cfg(test)]
pub fn open_in_memory() -> rusqlite::Result<Connection> {
    ensure_shared_page_cache();
    Connection::open_in_memory()
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

/// The connection's page-cache budget in KiB. `PRAGMA cache_size` echoes back the
/// negative KiB form [`apply_page_cache`] sets, so flip the sign.
#[cfg(test)]
pub fn page_cache_kib(conn: &Connection) -> i64 {
    let raw: i64 = conn
        .pragma_query_value(None, "cache_size", |row| row.get(0))
        .expect("read cache_size");
    assert!(raw < 0, "cache_size should be set in negative-KiB form, got {raw}");
    -raw
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
