# File system listing module

Backend directory reading, caching, sorting, and streaming for the file explorer. Handles 100k+ file directories with
non-blocking I/O and progress events.

## Module map

- **reading.rs**: low-level disk I/O. **streaming.rs**: async streaming with progress + cancellation, via the
  `ListingEventSink` trait. **operations.rs**: sync frontend-facing API (lifecycle, cache accessors).
- **caching.rs**: `LISTING_CACHE`, `CachedListing`, incremental patch helpers, `notify_directory_changed`.
  **mutation.rs**: what `Volume::notify_mutation` means for a local-FS backend. **diff.rs**: the `DiffChange`
  vocabulary plus `compute_diff` for the full re-read path. **diff_emitter.rs**: coalesces `directory-diff` emits into
  one event per 50 ms trailing window. **visible_rows.rs**: what a pane's row number means, materialized once per
  listing so every accessor indexes instead of walking.
- **listing_host.rs**: `AppListings`, what a storage backend in its own crate asks instead of reaching `caching`.
- **sorting.rs**, **brief_columns.rs** (Brief-mode column widths), **fuzzy_jump.rs** (type-to-jump). `FileEntry` lives
  in `crates/cmdr-fs/src/entry.rs`, aliased here as `listing::metadata`.

## Invariants and gotchas

- **A pane's row number is not an index into `entries`** (dotfiles and in-flight scratch drop out, `file_system::staging`).
  `CachedListing::rows` materializes the mapping once and is the ONLY filter point, on READ, never on cache fill
  (`staging_temps_test.rs` pins it). `entries` is private and `entries_mut` drops the map, so an accessor can't grow its
  own filter or leave a stale one — three had, each a row off. Re-deriving the sequence per row is what wedged a 74k
  directory; `visible_rows_test.rs` pins the scan count. `DETAILS.md` § "Row numbers".
- **Listing read commands are `async`.** A sync `#[tauri::command]` runs on the main thread in Tauri 2, so one slow
  accessor stops the app answering IPC at all.
- **Watcher diffs must update the cache AND emit an event.** Miss either and you get stale data or no UI update.
- **The full re-read watcher path re-sorts `new_entries` before `compute_diff`** (looks like a double-sort, isn't):
  `list_directory_core` always returns Name/Asc, so without it add/remove indices come out wrong.
- **A row that jumped its sorted position is one `move` change, not a remove plus an add**, so the pane can ride its
  cursor and selection along. `../DETAILS.md` § "Reordered rows".
- **All `directory-diff` emits go through `diff_emitter::enqueue_diff`, never `app.emit`.** Direct emits bypass the
  50 ms coalescing and re-introduce per-file flicker. Cache mutations stay synchronous; only the emit is deferred.
- **The orphan reaper keys on `last_accessed_ms`, not `created_at`.** Every read accessor and cache patch must bump it
  or the 6 h reaper can evict a live pane. Not from `refresh_listing_index_sizes` (background, not user activity).
- **Listing cancellation sets both `AtomicBool` and `tokio::sync::Notify`.** ❌ The `select!` cancel arm must never
  `listing_task.abort()`: returning detaches a safely-unwinding task, aborting wedges an MTP phone mid-round-trip.
  `DETAILS.md` § "Cancelling a listing detaches, never aborts".
- **Watcher callbacks run on OS threads, not the tokio runtime.** Use `tauri::async_runtime::spawn`; bare
  `tokio::spawn` panics and aborts the app. All FullRefresh dispatch funnels through `caching::spawn_full_refresh`,
  covering every producer (FSEvents, git, SMB, MTP, archive). The incremental path stays sync.
- **Sequence counter lives on `CachedListing`, not `WatchedDirectory`.** SMB/MTP have none.
- **A sort change invalidates the frontend's cached range.** Bump `cacheGeneration` to re-fetch.
- **New listing state hangs off a struct, not a `static`.** Fixtures go through `caching_test_support::TestListing`.
  `DETAILS.md` § "Test isolation".
- **Finder tags are deferred and must survive re-stats.** `list_directory_core` never reads tags; `enrich_tags` fills
  them visible-range-first. Every modify path calls `carry_forward_tags` BEFORE storing/emitting, else an mtime touch
  blanks a file's dots. ❌ Don't route the enrich path through it (that would block real removals).

Data flow, caching lifecycle, the orphan reaper, decisions, cache-helper and notification catalogs, diff coalescing,
metadata tiers, and the full tags story: `DETAILS.md`. Read it before any non-trivial work here: editing,
planning, reorganizing, or advising.
