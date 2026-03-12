# Fix: enrichment lock contention

## Problem

`enrich_entries_with_index()` uses `try_lock()` on the `INDEXING` mutex. When any other operation holds
this mutex, enrichment silently skips — entries go to the frontend with `recursive_size: None` and the
UI shows `<dir>`. There is no automatic recovery. Sizes only reappear if an unrelated index event later
triggers `refreshIndexSizes()` on the frontend.

The root cause: a single `std::sync::Mutex<IndexPhase>` guards both state machine transitions (rare,
exclusive) and read-only DB access (frequent, concurrent). These have opposite access patterns.

### Contention sources (biggest to smallest)

| Operation | Lock type | Hold time |
|-----------|-----------|-----------|
| `verify_affected_dirs` Phase 1 | `INDEXING.lock()` | Seconds (bulk DB reads for all affected paths) |
| `run_background_verification` dir-stat reads | `INDEXING.lock()` | Milliseconds |
| `get_dir_stats_batch` IPC | `INDEXING.lock()` | Milliseconds (path resolution + batch query) |
| `get_status` IPC | `INDEXING.lock()` | Microseconds |
| State transitions | `INDEXING.lock()` | Microseconds |

Any of these overlapping with a `get_file_range` call causes the enrichment `try_lock()` to fail.

### Constraint

`rusqlite::Connection` is `Send` but not `Sync`. Multiple threads cannot share `&Connection`. This is
why the original code used `Mutex` instead of `RwLock`.

## Design: separate the read store from the state machine

Split the global state into two pieces:

1. **`INDEXING` mutex** (unchanged) — guards lifecycle transitions only. Never held during DB reads.
2. **`READ_POOL` static** (new) — provides independent read connections to any thread. No lock
   contention for reads.

SQLite WAL mode allows unlimited concurrent read connections. Each thread opens its own connection and
caches it in thread-local storage.

### New type: `ReadPool`

```rust
struct ReadPool {
    db_path: PathBuf,
    /// Incremented on shutdown/clear. Thread-local connections check this to detect staleness.
    generation: AtomicU64,
}

thread_local! {
    static THREAD_CONN: RefCell<Option<(PathBuf, u64, Connection)>> = RefCell::new(None);
}

impl ReadPool {
    fn new(db_path: PathBuf) -> Result<Self, IndexStoreError> {
        let _ = IndexStore::open_read_connection(&db_path)?; // Validate openable
        Ok(Self { db_path, generation: AtomicU64::new(0) })
    }

    /// Invalidate all thread-local connections. Next `with_conn` call reopens.
    fn invalidate(&self) {
        self.generation.fetch_add(1, Ordering::Release);
    }

    /// Run `f` with a thread-local read connection.
    ///
    /// SAFETY constraint: must be called from synchronous code only. In async contexts,
    /// tasks can migrate between threads at .await points, which would make the thread-local
    /// connection unreliable. All current callers (enrich_entries_with_index,
    /// verify_affected_dirs Phase 1) are synchronous.
    fn with_conn<T>(&self, f: impl FnOnce(&Connection) -> T) -> Result<T, String> {
        let gen = self.generation.load(Ordering::Acquire);
        THREAD_CONN.with(|cell| {
            let mut slot = cell.borrow_mut();
            // Reuse if same path + same generation; otherwise reopen
            let needs_reopen = match slot.as_ref() {
                Some((p, g, _)) => p != &self.db_path || *g != gen,
                None => true,
            };
            if needs_reopen {
                let conn = IndexStore::open_read_connection(&self.db_path)
                    .map_err(|e| format!("{e}"))?;
                *slot = Some((self.db_path.clone(), gen, conn));
            }
            Ok(f(&slot.as_ref().unwrap().2))
        })
    }
}
```

Cost: ~50-100μs to open a connection, paid once per thread per generation (cached thereafter). With
Tauri's default thread pool (~4-8 threads), this adds ~8-16MB of page cache memory. Negligible.

Thread-local connections are implicit read transactions per query (no explicit `BEGIN`). Each query
sees the latest committed data. Long-running explicit transactions should be avoided — they pin
WAL snapshots.

### New static: `READ_POOL`

```rust
static READ_POOL: LazyLock<std::sync::Mutex<Option<Arc<ReadPool>>>> =
    LazyLock::new(|| std::sync::Mutex::new(None));

/// Clone the pool Arc. Lock held for nanoseconds — just an Arc clone.
fn get_read_pool() -> Option<Arc<ReadPool>> {
    READ_POOL.lock().ok()?.as_ref().cloned()
}
```

Alternative: `ArcSwap` from the `arc-swap` crate would avoid even this brief mutex. Not worth the
dependency — the inner lock is held for nanoseconds.

## Changes

### `indexing/mod.rs`

**`enrich_entries_with_index()`** — the core fix:

```rust
pub fn enrich_entries_with_index(entries: &mut [FileEntry]) {
    let pool = match get_read_pool() {
        Some(p) => p,
        None => return, // Indexing not initialized
    };

    // ... same directory detection logic as today ...

    if let Err(e) = pool.with_conn(|conn| {
        enrich_via_parent_id_on(entries, conn, &parent_path)
    }).and_then(|r| r) {
        log::debug!("Enrichment fast path failed: {e}, trying fallback");
        let _ = pool.with_conn(|conn| {
            enrich_via_individual_paths_on(entries, conn)
        });
    }
}
```

No `try_lock`. No skipping. No recovery needed.

The double-Result from `with_conn` (connection-open error wrapping the inner function error) is
flattened via `.and_then(|r| r)`. `enrich_via_individual_paths_on` returns `()`, so its call is
simpler — just `let _ = pool.with_conn(...)`. No changes to inner function return types needed.

**`enrich_via_parent_id`** — rename to `enrich_via_parent_id_on`, accept `&Connection` instead of
`&IndexStore`. Replace `store.read_conn()` with the passed-in `conn`. The internal calls
(`store::resolve_path(conn, ...)`, `IndexStore::list_child_dir_ids_and_names(conn, ...)`,
`IndexStore::get_dir_stats_batch_by_ids(conn, ...)`) already take `&Connection`.

**`enrich_via_individual_paths`** — same treatment: rename to `enrich_via_individual_paths_on`,
accept `&Connection`. Replace `store.read_conn()` with the passed-in `conn`.

Both functions have test call sites in `mod.rs` `#[cfg(test)]` that pass `&IndexStore` — update
those to pass `store.read_conn()` instead.

**`start_indexing()`** — create and install the pool before transitioning to Initializing:

```rust
let pool = Arc::new(ReadPool::new(db_path.to_path_buf())?);
*READ_POOL.lock().unwrap() = Some(pool);
```

The `Initializing` phase and its temporary `IndexStore` are kept as-is — `get_status()` and other
IPC commands still use `INDEXING.lock()` to check the phase enum. `ReadPool` only replaces the
enrichment and verification read paths, not the state machine queries.

**`stop_indexing()`** — invalidate and clear the pool:

```rust
if let Some(pool) = READ_POOL.lock().unwrap().take() {
    pool.invalidate(); // Bump generation so thread-local connections are discarded
}
```

**`clear_index()`** — same pattern (invalidate before deleting DB files):

```rust
if let Some(pool) = READ_POOL.lock().unwrap().take() {
    pool.invalidate();
}
// ... then delete DB files ...
```

**`verify_affected_dirs` Phase 1** (line ~1850) — the single `INDEXING.lock()` at the top of this
function is removed entirely. The entire Phase 1 loop (resolve paths + list children → build
`HashMap<String, (i64, Vec<EntryRow>)>` snapshot) moves into a single `pool.with_conn()` closure:

```rust
let pool = match get_read_pool() {
    Some(p) => p,
    None => return VerifyResult::default(),
};

let snapshot = pool.with_conn(|conn| {
    let mut map = HashMap::new();
    for parent_path in affected_paths {
        if let Some(parent_id) = store::resolve_path(conn, parent_path)? {
            let children = IndexStore::list_children_on(parent_id, conn)?;
            map.insert(parent_path.to_string(), (parent_id, children));
        }
    }
    Ok(map)
});
```

Specific call replacements:
- `store::resolve_path(conn, parent_path)` — same call, `conn` now comes from `pool.with_conn`
- `store.list_children(parent_id)` → `IndexStore::list_children_on(parent_id, conn)` (the `_on`
  variant that takes `&Connection` already exists in store.rs)

Phase 2 (filesystem I/O, write messages) is unchanged — it already runs without the lock.

`verify_affected_dirs` must remain a synchronous function (no `.await` points) for thread-local
safety. This is already the case today and must not change.

**`run_background_verification` dir-stat reads** (line ~1755) — the second `INDEXING.lock()`
acquisition after `verify_affected_dirs` returns. This resolves new directory paths to IDs and
fetches their stats. Replace with `get_read_pool()` + `pool.with_conn()`. The three DB reads per
new directory all already accept `&Connection`:

```rust
let deltas = pool.with_conn(|conn| {
    let mut deltas = Vec::new();
    for dir_path in &verify_result.new_dir_paths {
        if let Some(entry_id) = store::resolve_path(conn, dir_path)? {
            let parent_id = IndexStore::get_parent_id(conn, entry_id)?;
            let stats = IndexStore::get_dir_stats_by_id(conn, entry_id)?;
            deltas.push((parent_id, stats));
        }
    }
    Ok(deltas)
});
```

Note: `run_background_verification` is an async function, but the `pool.with_conn()` closure
contains no `.await` points — it's a synchronous block. This is safe for thread-local storage
because the task cannot migrate threads mid-closure. Add a comment at the call site documenting
this invariant.

### Operations that stay on `INDEXING.lock()`

These need the full `IndexManager` (mutable `PathResolver`, state machine access, etc.):

- `get_dir_stats()` / `get_dir_stats_batch()` — need `&mut PathResolver` for LRU cache. Contention
  is low (IPC calls, throttled to 2s cooldown on the frontend). Can be migrated later if needed.
- `get_status()` — reads cached state, no DB queries. Fast.
- `prioritize_dir()` / `cancel_nav_priority()` / `force_scan()` / `stop_scan()` — infrequent.

### Files with no changes

- `store.rs` — `open_read_connection()` and `list_children_on()` already exist.
- `operations.rs` / `streaming.rs` — call sites (`enrich_entries_with_index(&mut entries)`)
  unchanged; the function signature stays the same.
- Frontend — no changes. The backend enrichment path now reliably returns sizes on
  the first `get_file_range` call, so the frontend rarely needs the `refreshIndexSizes()` path.

## Enrichment path after the fix

```
get_file_range() / list_directory_start()
  └── enrich_entries_with_index(&mut entries)
        ├── get_read_pool()              → clone Arc (nanoseconds, no contention)
        ├── pool.with_conn(|conn| ...)   → thread-local connection (no lock)
        │     ├── resolve_path()          (one indexed query)
        │     ├── list_child_dir_ids_and_names()
        │     └── get_dir_stats_batch_by_ids()
        └── apply stats to entries
```

## Risks

1. **Stale thread-local connections after `clear_index()`**: When the DB file is deleted, thread-local
   connections still reference the old file. The generation counter handles this: `clear_index()` calls
   `pool.invalidate()` which bumps the generation. On the next `with_conn` call, the generation
   mismatch triggers a fresh connection open. Between invalidation and the next call, the old
   connection holds an open file descriptor to the deleted file (the OS reclaims disk space only when
   all fds close). This is acceptable — the thread pool is small and the connections are replaced on
   next use.

2. **Thread-local + async safety**: `with_conn` uses `thread_local!` storage, which is thread-affine.
   Async tasks can migrate between threads at `.await` points. All current callers
   (`enrich_entries_with_index`, `verify_affected_dirs` Phase 1) are synchronous — no `.await` points
   exist between obtaining the pool reference and completing the closure. Future callers must maintain
   this invariant, documented with a safety comment on `with_conn`.

3. **WAL checkpoint pressure**: More concurrent readers can slightly delay WAL checkpointing. With
   enrichment queries taking <1ms and threads being few, this is negligible.

4. **Pragma and collation setup**: `ReadPool` relies on `IndexStore::open_read_connection()` which
   sets `synchronous = NORMAL`, `cache_size = -16384` (16 MB), and registers the `platform_case`
   collation. No additional pragma setup is needed in `ReadPool`. WAL mode is a database-level
   property set by the write connection — all read connections inherit it automatically. The
   validation connection in `ReadPool::new()` is dropped immediately; thread-local connections open
   lazily on first `with_conn` call, by which time the write connection has established WAL mode.

## Testing

1. **Enrichment under contention**: Spawn a thread that holds `INDEXING.lock()` for 2 seconds. On
   another thread, call `enrich_entries_with_index()`. Assert sizes are populated. This test would
   fail today and pass after the fix.
2. **Thread-local connection reuse**: Call `pool.with_conn()` twice from the same thread, verify the
   connection is reused (no second `open_read_connection` call).
3. **Generation invalidation**: Call `pool.invalidate()`, then `pool.with_conn()`. Verify a fresh
   connection is opened (the old cached one is discarded).
4. **Cross-thread reads**: Spawn N threads, each calling `pool.with_conn()`. Assert all succeed
   concurrently.
5. **Shutdown**: Clear `READ_POOL`, verify `enrich_entries_with_index` returns early without panic.
6. **Existing tests**: `cargo nextest run indexing` — all must pass unchanged.
7. **Manual**: Launch app, navigate to a large directory during startup (when `verify_affected_dirs`
   would have held the lock). Verify sizes appear on first load.

## CLAUDE.md updates

Update `indexing/CLAUDE.md`:
- Replace references to `GLOBAL_INDEX_STORE` (stale name) with the actual architecture.
- Update the "Global read-only store uses `std::sync::Mutex`" gotcha to describe `ReadPool`.
- Update the `enrich_entries_with_index` doc comment (remove `try_lock` references).
- Add a gotcha about `with_conn` being sync-only (no `.await` between pool access and closure).
