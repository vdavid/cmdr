# Listing post-read pipeline runs sort + index enrichment on the async runtime

**Severity:** medium
**Lens:** B — Concurrency
**Confidence:** medium

## Location
`apps/desktop/src-tauri/src/file_system/listing/streaming.rs:472-510` (approximate; see `read_directory_with_progress`)

## What
After `volume.list_directory(...).await` completes (which runs the actual I/O on a `tokio::spawn`'d sub-task), the rest of the pipeline runs **inline on the calling task** without `spawn_blocking`:

1. `crate::indexing::enrich_entries_with_index(&mut entries)` — synchronous SQLite calls against `ReadPool`, plus per-entry HashMap lookups.
2. `crate::indexing::trigger_verification(&path.to_string_lossy())` — also acquires a mutex via `VERIFIER_STATE.lock()`.
3. `sort_entries(&mut entries, sort_by, sort_order, dir_sort_mode)` — pure CPU sort.
4. `if let Ok(mut cache) = LISTING_CACHE.write()` — a global `std::sync::RwLock` writer acquired on the async task.

For 100k-entry directories on a slow drive, the sort alone can take tens of ms; enrichment plus sort on a path like a project root with deep `dir_stats` lookups can land in the 50-100 ms range. This runs on the tokio worker that called `list_directory_start_streaming`, which is the IPC thread pool.

## Why it matters
The architecture's intent is that the directory listing pipeline never wedges the runtime. The actual I/O is correctly off-worker (`tokio::spawn`'d, `spawn_blocking` inside `LocalPosixVolume::list_directory`). But the post-read finalize phase reverts to running on the same async worker that the IPC handler used.

Concrete consequences:

- A 100k-file listing's sort + enrich runs for tens of ms inline; during that window, the tokio worker can't poll other IPC handlers. With a small worker pool (default Tauri uses very few), one slow finalize can stall *every other in-flight IPC* on that worker.
- `LISTING_CACHE.write()` is a `std::sync::RwLock`. Holding it across `sort_entries` (which it doesn't; sort happens before the lock) is fine, but the lock is taken on the async worker too — if another async task is also trying to read `LISTING_CACHE` (which `get_file_range` and friends do constantly), it parks the *worker*, not just the task.
- The architecture-patterns doc explicitly calls out "I/O runs on a separate OS thread; the main task polls via `mpsc::channel` every 100 ms" as the design — but only for the read syscall. The CPU+SQLite work that follows breaks the same guarantee for large listings.

## Evidence
- `apps/desktop/src-tauri/src/file_system/listing/streaming.rs:419-456`: the I/O is correctly `tokio::spawn`'d and `select!`-ed against the cancel notify.
- Lines 458-510 (after the `select!`): no second `spawn_blocking` — `enrich_entries_with_index`, `trigger_verification`, `sort_entries`, and the `LISTING_CACHE.write()` insert all run inline.
- `indexing/CLAUDE.md` describes enrichment as "two indexed queries total" but does not promise it runs off-thread.
- `architecture-patterns.md:97-99` "Known gap: On stuck network mounts, the OS read_dir syscall blocks the I/O thread" — the gap is acknowledged for the syscall but not for the finalize phase.

## Suggested fix
Wrap the post-read finalize block in a single `tokio::task::spawn_blocking`:
```rust
let (mut entries, cache_inserted) = tokio::task::spawn_blocking(move || {
    crate::indexing::enrich_entries_with_index(&mut entries);
    sort_entries(&mut entries, sort_by, sort_order, dir_sort_mode);
    // ... cache.write().insert(...) inside here too
    (entries, true)
}).await?;
```
`trigger_verification` itself spawns a task internally, so it's safe to leave on the async path. The pure-CPU sort + the SQLite reads + the global RwLock write all move to the blocking pool, where they're free to take 100 ms without affecting IPC responsiveness.

The cancellation check between enrich and sort, and the one before `cache.insert`, both need to be preserved inside the spawn_blocking block (the `state` Arc is `Send + Sync`).

## Notes
Lower priority than the cloud / space-poller findings because most directories are small enough that this is invisible. But the moment a user opens a 100k-photo iCloud cache or a `node_modules` deep tree, this becomes the visible latency.
