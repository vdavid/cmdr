# File system listing details

Depth and rationale for the listing module. `CLAUDE.md` holds the must-knows that prevent silent breakage; the
narrative, data flow, and decision rationale live here. For profiling listing performance, see
`docs/guides/benchmarking-file-loading.md`.

## Data flow

```
Frontend                          Backend
   |                                   |
   |--- listDirectoryStart ----------->| (returns immediately)
   |<-- { listingId, status: loading } |
   |                                   |
   |                            [background task spawns]
   |<--- listing-opening event --------| (just before read_dir)
   |<--- listing-progress event -------| (every 200ms, { listingId, loadedCount })
   |<--- listing-read-complete event --| (when read_dir finishes, { listingId, totalCount })
   |                            [sorting + caching; watcher arm dispatched, not awaited]
   |<--- listing-complete event -------| (ready, { listingId, totalCount, volumeRoot })
   |                                   |
   |-- getFileRange(listingId, ...) -->| (on-demand fetching)
   |<-- [FileEntry, FileEntry, ...]    |
```

`listing-complete` is what commits the listing in the pane, so nothing slow may sit in front of it. Arming the FSEvents
watch used to, and no longer does: `start_watching_detached` hands it to the blocking pool. Why arming is slow, what
that cost, and the two rules that keep it cheap: `../DETAILS.md` § "Arming a listing watch is detached".

## Local listing progress

`listing-progress` is the only thing the user sees during a big folder's read: without it the pane sits on "Opening
folder..." from the first keystroke to `listing-read-complete`, however long the stat loop takes. `streaming.rs` builds
the callback and passes it down through `Volume::list_directory`, so a backend that ignores its `on_progress` argument
turns that state off for every folder it serves. Nothing about that fails loudly (the symptom is a UI state that stops
appearing), which is why `a_local_directory_read_emits_progress_events` in `streaming_test.rs` drives the whole chain
against a real `LocalPosixVolume`: the rest of that suite runs on `InMemoryVolume` and can't see a backend drop it.

The local backend can't take the callback where the work happens. `on_progress` is `Sync` but not `Send`, and
`LocalPosixVolume` runs its stat loop on `spawn_blocking`, which demands `Send + 'static`. So the two halves are split:

- `list_directory_core_with_tally` publishes into a `ListingTally` (`reading.rs`) as it stats, one relaxed atomic bump
  per entry, unthrottled. That's free next to the `stat` it accompanies.
- `LocalPosixVolume::list_directory` samples the tally every `PROGRESS_SAMPLE_INTERVAL` from a `tokio::select!` against
  the `JoinHandle`, and calls `on_progress` from there. The callback never leaves the async task that owns it.

Two details the loop depends on. The `select!` is `biased` so a listing that finished during a tick returns its real
result rather than spending another sample on an approximate count. And a snapshot of zero is dropped rather than
emitted, because the blocking pool may not have picked the task up yet and "Loaded 0 files..." is worse than the
"Opening folder..." it would replace.

Putting the throttle in the sampler rather than the stat loop means one place decides how often the number changes,
and it's the place that knows it's driving a UI. `PROGRESS_SAMPLE_INTERVAL` is 200 ms in production and 1 ms under
test, so `listing_a_local_directory_reports_progress_while_it_reads` can pin the wiring against a 5,000-entry scratch
dir instead of needing one big enough to outlast a real interval.

SMB and MTP wire `on_progress` through their own listing loops directly, having no `spawn_blocking` hop to cross.

## Caching

- **`LISTING_CACHE`**: global `RwLock<HashMap<String, CachedListing>>`, keyed by `listing_id` (UUID per navigation).
- **`CachedListing`**: `{ volume_id, path, entries, visible_rows, path_index, sort_by, sort_order,
  directory_sort_mode, sequence, created_at, last_accessed_ms }`. `entries` is private: `entries()` reads,
  `entries_mut()` / `set_entries()` change it and drop BOTH maps on the way, `rows(include_hidden)` is what every read
  accessor asks, and `index_of_path` / `indices_of_paths` are what every by-path caller asks. See § "Row numbers" and
  § "Entries by path".
- **Focused-pane reads**: `get_cached_listing(volume_id, path)` clones the newest matching cached listing without
  requiring watcher coverage. Agent reads use it for the already-open pane, including SMB and MTP, and never start a
  new filesystem listing.
- **`caching::snapshot_listings()`**: lightweight summary of every active listing (id, volume, path, entry count, age).
  Used by `cmdr://state` so error reports surface orphan listings (started but not bound to a pane).
- **Concurrency**: multiple listings coexist (different panes, rapid navigation), each with a unique ID.

### Lifecycle

1. `list_directory_start_streaming()` receives the listing ID from the frontend, spawns a task.
2. The background task reads the directory, sorts, stores in the cache.
3. Frontend calls `get_file_range()` for visible entries (on-demand).
4. Frontend calls `find_file_indices()` to batch-resolve file names to indices (selection adjustment during operations).
5. Frontend calls `get_paths_at_indices()` / `get_files_at_indices()` for batch selection lookups (transfer dialogs,
   delete dialog, drag, clipboard).
6. `list_directory_end()` stops the watcher and removes from the cache (primary, fast eviction).

### Backstop reaper

`start_orphan_listing_reaper` (spawned in `lib.rs` setup) sweeps every `REAPER_SWEEP_INTERVAL` (30 min) and tears down
any listing idle past `ORPHAN_IDLE_WINDOW` (6 h) via the same `list_directory_end` path, so a leaked listing (close IPC
never delivered) can't pin its entry vector and OS watcher for the whole session. Pure, clock-injectable seam:
`orphan_ids(now_ms, window_ms, …)` and `reap_orphaned_listings_at(now_ms, window_ms)`. Mirrors the search index's
idle/backstop timers and the file viewer's window-`Destroyed` net.

It keys on `last_accessed_ms`, NOT `created_at`. `created_at` is stamped once and never refreshed, so an age-based reaper
keyed on it would evict a pane open all session. `last_accessed_ms` (an `AtomicU64` of ms-since-a-process-epoch) is
bumped by every operation that proves the listing still backs a live pane: the read accessors (`get_file_range`,
`get_total_count`, `get_file_at`, `get_file_beside`, `get_listing_stats`, the index/path/batch lookups), `resort_listing`, and every
watcher/notify cache patch (`insert_entry_sorted` / `remove_entries_by_paths` / `remove_entry_by_name` /
`update_entry_sorted` / `update_listing_entries`). `AtomicU64` so read accessors stamp it lock-free under a shared `LISTING_CACHE.read()`. The
6 h window is deliberately generous: we'd rather never evict a live listing than aggressively reclaim.
`refresh_listing_index_sizes` intentionally does NOT touch it: it's driven by background indexing, not user/FS activity,
so touching there could keep a truly-orphaned listing alive indefinitely.

#### Test isolation for `LISTING_CACHE`

`cargo test` runs the crate's tests as threads in ONE process, so `LISTING_CACHE` is shared by every listing test at
once (`cargo nextest` gets isolation free from process-per-test, but the module-run command and CI's lib run don't).
Three failure modes follow, and `caching_test_support.rs` closes all three:

- **Colliding keys.** Two tests picking the same literal listing id clobber each other. `TestListing::insert(tag)` mints
  a process-unique id (`unique_test_id`: tag + pid + counter).
- **Leaks on a failed assertion.** A hand-rolled `cache.remove(...)` placed after the assertions never runs when one
  fails, so the entry stays visible to every later test. `TestListingGuard`'s `Drop` tears down through the production
  `list_directory_end` (entry, watcher, and pending coalesced diff together), and `Drop` runs on unwind.
- **Cache-wide assertions.** `find_listings_for_path`, `find_listings_on_volume`, and the orphan sweep all scan the
  whole map, so a shared path or volume id makes a count assertion depend on what else is running. Those tests derive a
  unique path / volume id per test. For the sweep, `reap_orphaned_listings_at_for(now, window, only)` restricts the
  wired teardown to the ids the test owns; production keeps calling the unrestricted `reap_orphaned_listings_at`.
  `TestListing` also stamps `last_accessed_ms` at NOW (a live pane's value) rather than 0, so a fixture isn't
  orphan-eligible under someone else's sweep in the first place. Pinned by
  `caching_reaper_test::a_reaper_sweep_leaves_a_sibling_tests_listing_alone`.

The guard mirrors `indexing::tests::stress_test_helpers::TestInstanceGuard` (over `INDEX_REGISTRY`),
`write_operations::test_support::TestOperationGuard` (over `WRITE_OPERATION_STATE`), and
`volume::manager::test_support::TestVolumeRegistration` (over the global `VolumeManager`); knowing one is knowing all
four.

**New subsystem state hangs off a struct, not a `static`.** These guards are the retrofit cost of a process-global; a
handle threaded through its callers needs none of it.


## Row numbers (visible_rows.rs)

A pane numbers its rows over what it is SHOWING, so row 7 is the seventh visible entry and not `entries[7]`. Two things
leave an entry out: the dotfile filter (when `include_hidden` is off) and scratch a running operation owns
(`file_system::staging`).

**Answering "which entry is row N" by walking and counting is what wedged the app.** The MCP pane mirror fetches ~100
rows per sync, each fetch was one full walk, and at the bottom of a 74,144-entry directory that came to ~7.4 M predicate
evaluations per index event on the main thread — IPC stopped being answered at all. Evidence and the before/after:
`docs/notes/listing-row-fetch-quadratic-2026-08-22.md`.

**Decision: materialize the row map once per `(listing, include_hidden)`, and split it in two.** `settled` is a
`Vec<u32>` of entry indices nothing can hide any more; `candidates` holds the scratch-NAMED entries with the count of
settled rows ahead of each. A read re-asks `is_hidden_from_listings` about the candidates only — a handful, usually
none — and merges them back by row number, so a lookup is an array index plus a binary search over a list that is
almost always empty.

**Why the split rather than an invalidation hook.** The dotfile half is stable, but the scratch half is not: an
operation settling un-hides its leftover with no change to the listing and nothing to notify anyone, because the
ownership signal is a `Weak` that simply stops upgrading (`cmdr_fs::staging`). Hunting for every event that could flip
it is exactly the kind of invariant that rots; re-asking about the few names it could apply to cannot. What makes it
sound is that `staging::is_hidden_from_listings` is GATED on the pure `could_be_hidden_from_listings`, so a name outside
that set is settled by construction.

**Two slots, one per `include_hidden`**, so a pane toggling hidden files — or two readers disagreeing about the flag
mid-toggle — can never be handed the map built for the other answer.

**Validity needs no version counter.** Every accessor holds the `LISTING_CACHE` READ lock, and every mutation needs the
WRITE lock, so `entries` cannot move under a reader. `entries_mut()` drops both maps as it hands the vector out, which
is why no mutation path has to remember anything.

**What it fixed beyond the wedge**, all three found by making `entries` private and following the compile errors:
type-to-jump filtered dotfiles but not scratch, so its index space and `getFileAt`'s disagreed and the cursor landed a
row off during a copy; Brief-mode column widths were sized around names the pane never draws; and the streaming
listing's `totalCount` counted dotfiles only.

**Cost.** One `Vec<u32>` per listing per `include_hidden` actually used: ~300 KB against a 74k listing whose entries are
themselves ~15 MB.

**The one case that got slower**, said plainly: reading row 0 right after a mutation used to short-circuit after one
entry and now rebuilds the whole map. It doesn't matter in practice, because the reads that accompany it
(`get_total_count`, `get_listing_stats`) were already walking the listing and now share that one pass — but a future
caller that reads a single shallow row per mutation and nothing else is the shape to watch.

**What is still O(rows), and can still be re-multiplied by a caller that loops it**: `find_file_index`,
`get_file_beside`, and `get_listing_stats`. They are a name search and a full sum, so linear is their floor, not an
accident. `find_file_indices` is the batch form of the first, and `get_file_beside` exists so a caller wanting a
neighbour doesn't compose two calls; reach for those instead of a loop.

## Entries by path (path_index.rs)

The other index space a caller arrives with. A row number comes from the pane; a PATH comes from anything that read the
listing earlier and now wants to change one row — Finder-tag enrichment above all, which sends 500 paths per call and
sweeps a whole directory that way.

**The same defect as § "Row numbers", one caller further on.** Each path was found by walking `entries`, under the
cache's WRITE lock, once per path. Measured on a release build (M1 Max, 2026-08-22, synthetic entries with a 63-character
mean path): one 500-path chunk costs 20 ms at 20,000 entries, 64 ms at 75,000, and 418 ms at 300,000 — times
`entries / 500` chunks to cover the directory, so a 300,000-entry listing spent over four minutes of write-locked
walking on tags nobody asked to wait for. It is the "just opened" step in the release curve
(`docs/notes/listing-wedge-impact-2026-08-22.md` § 2).

**Decision: materialize `(path hash, entry index)` pairs, sorted by hash.** A lookup hashes once, binary-searches, and
compares the real path of the entries whose hash matches. Same build-once-and-index shape as the row map, and the same
validity argument: `entries_mut()` drops it, and every accessor holds the `LISTING_CACHE` lock, so no version counter is
needed.

**Why hashes and not a `HashMap<String, usize>`.** Twelve bytes per entry against 100+ for a map that owns a second copy
of every path: 3.6 MB against a 300,000-entry listing whose entries are themselves ~65 MB, where the map would add
~30 MB. It also builds in one sequential pass plus an integer sort, so its cost doesn't depend on the pane's sort order,
where an index sorted BY PATH degrades 6× on anything but a name sort (measured: 9.4 ms name-sorted against 58.5 ms
shuffled, 300,000 entries). Colliding hashes land adjacent and the lookup compares the real path, so a collision costs
one extra comparison and nothing else — it is resolved, not assumed away.

**Decision: one build decision per BATCH, at `BUILD_FROM_BATCH_SIZE` (32).** Both halves are linear in the listing —
building costs ~65 ns per entry, one scan ~3.4 ns per entry examined — so `k` scans reach the build's price at
`k ≈ 2 × 65 / 3.4 ≈ 38`, a constant, because the listing size cancels. Under that, a batch is cheaper scanning; over it,
the map wins on the batch alone before any reuse. A context-menu tag toggle on one right-clicked file must not walk
300,000 entries into a map it uses once. An existing map is always used, whatever the batch size.

**A tag write deliberately bypasses `entries_mut`** (`CachedListing::set_tags_by_path`). A tag is not part of a name, a
sort key, or a path, so neither map can go stale from one; routing it through `entries_mut` would drop the row map on
every enrichment chunk and make the next pane read rebuild all of it, which is a second quadratic riding on the first.
❗ If a tag ever becomes a sort column or a visibility input, this has to go back through `entries_mut`.
`path_index_test::a_tag_update_leaves_the_row_map_standing` pins it.

**The write lock was left alone, deliberately.** The hold is now the batch plus one build per listing (~0.05 ms per
500-path chunk at 75,000 entries, after a ~4.9 ms build), so moving the resolve to a read lock would save one build and
buy a torn window: entries can move between dropping a read lock and taking the write lock, and every index would need
re-verifying against its path before it could be trusted. Not worth it at these numbers.

**Every by-path lookup goes through the map**, in one of two forms. `CachedListing::indices_of_paths` takes a BATCH and
makes one build decision for it: tag enrichment and the watcher's removals. `CachedListing::index_of_path`
(`PathIndexCache::resolve_one`) takes ONE path, for `carry_forward_tags`, `has_entry`, `update_entry_sorted`, and
`insert_entry_sorted`'s duplicate guard. ❗ The single-path form rides a map that already exists and **never builds
one**: one lookup is far under `BUILD_FROM_BATCH_SIZE`, so a modify on an untouched 300,000-entry listing must not pay
~20 ms for a map it uses once and (mutating) drops on the way out. What it buys is the sweep's map, so while enrichment
is walking a big directory every watcher event landing in it is a hash rather than a walk.

❗ **A mutating caller resolves BEFORE it takes `entries_mut`.** That is the whole reason `update_entry_sorted` and the
removals can reach a map at all: `entries_mut` drops both maps as it hands the vector out, so a lookup after it can only
walk. The `LISTING_CACHE` write lock is held across both, so the index is still true when the mutation lands, and the
invariant is untouched — `path_index_test::a_mutation_still_drops_the_map_it_rode` pins that they still drop it. One
side effect worth having: a modify or removal that takes no row leaves both maps standing rather than dropping them for a
row it never touched, which matters because an add-only watcher event calls the removal batch with an empty path list.

**The watcher's removals were the second quadratic**, and the only one of these callers with a measured win rather than
a latent one. `handle_directory_change_incremental` resolved each removal's index with its own full walk (the diff needs
the PRE-removal index) and then walked again inside the removal itself, and one coalesced watcher event carries up to
500 paths — a directory emptied, a `git checkout` across a big tree, an unpack over a folder. Measured by the counting
probe: a 500-path removal from a 20,000-entry listing examined **9,981,000 entries**, and now examines **20,500** (one
walk to build the map, plus one lookup per path). `remove_entries_by_paths` is now the only by-path removal, since its
caller is always a batch. Resolving and removing under ONE write lock also makes the emitted indices true: the two-lock
shape it replaced let another writer move a row between the lookup and the removal, and the `directory-diff` would have
named a row that had shifted.

**What is still linear per removal**: `Vec::remove` per doomed row, so dropping `k` rows from an `n`-entry listing
memmoves about `k × n / 2` entries. The lookup fix doesn't touch it, and the incremental path's 500-event cap bounds it.
The one-pass rebuild that would fix it needs a transient second copy of `entries` (~65 MB at 300,000 rows), which is not
a trade to take without measuring first.

**What is still O(entries), by path**: nothing. The single-path callers walk only on a listing that has no map yet,
which is what the threshold deliberately buys.

## Decisions

- **Streaming with a background task, not chunked IPC**: chunked needs multiple IPC calls and complex state tracking.
  Streaming spawns a `tokio::spawn` task and emits events; the frontend stays responsive (Tab works, ESC cancels).
- **Cancellation via `AtomicBool` checked per-entry**: network folders iterate slowly (seconds per entry); a per-entry
  check keeps ESC responsive (cancel within ~100 ms).
- **Three-stage progress (opening → progress → read-complete → complete)**: `listing-opening` (about to start slow
  I/O), `listing-progress` (loaded N, every 200 ms via `list_directory_core_with_progress`), `listing-read-complete`
  (all read, sorting now), `listing-complete` (ready to render).
- **Sort after read, before caching**: the frontend expects sorted order. Sorting 50k entries takes ~15 ms, done in the
  background task after all entries are collected.
- **Enrichment at cache-write time, not on `get_file_range`**: every path that stores entries (streaming, watcher
  update, re-sort) enriches first. Index freshness is event-driven: `index-dir-updated` → `refreshIndexSizes` →
  `refresh_listing_index_sizes` (write-locks the cache, re-enriches entries). This keeps `get_listing_stats` read-only
  while it sees up-to-date `recursive_size`. The frontend calls `refreshListingIndexSizes` before `fetchListingStats`.
- **Hidden-file filtering in Rust, not the frontend**: visible count is unknown until all files are read. APIs accept
  `include_hidden: bool` and read through the listing's row map (§ "Row numbers").
- **The listing read commands are `async`**: a sync `#[tauri::command]` runs on the MAIN thread in Tauri 2, so one slow
  accessor stops the app answering IPC at all, which is principle 2's "never block the main thread" broken at the IPC
  layer. `refresh_listing_index_sizes` goes one further onto the blocking pool, because it runs two indexed SQLite
  queries and an index storm fires it once per event per pane.
- **Font metrics in a Rust binary cache, not frontend canvas measurement**: measuring 50k filenames in JS is slow. The
  frontend measures each code point's width once via Canvas and ships the table to Rust; later text-width queries are
  hash lookups in the cached `.bin` table. `calculate_max_width_with_suffixes()` is the entry point, used by
  `brief_columns::compute_brief_column_text_widths` to size each Brief column to its widest filename (plus a per-row
  trailing suffix that reserves room for the Finder tag-dot cluster).
- **Sequence counter on `CachedListing`, not `WatchedDirectory`**: SMB and MTP volumes don't use FSEvents
  (`can_watch_listings() == false`), so they have no `WatchedDirectory`. With the sequence on the watcher,
  `increment_sequence` returned `None` and `directory-diff` events never fired for those volumes. The `AtomicU64` on
  `CachedListing` works for all volume types; the FSEvents path uses the same counter.
- **`ListingEventSink` trait decouples streaming from Tauri** (same pattern as `OperationEventSink`):
  `read_directory_with_progress` emits events, but `tauri::AppHandle` can't be created in tests.
  `CollectorListingEventSink` captures events for assertions. `Arc<dyn ListingEventSink>` (not `&dyn`) because the sink
  is cloned into `tokio::spawn` for progress callbacks.
- **Watcher starts AFTER listing-complete**: watcher diffs rely on cached entries; starting before the cache is
  populated would miss initial state.
- **Incremental watcher path with fallback to full re-read**: most FS changes touch a few files. The incremental path
  stats each changed path, classifies add/remove/modify against the cache, and patches in-place via
  `insert_entry_sorted` / `remove_entries_by_paths` / `update_entry_sorted`. Falls back to full `handle_directory_change`
  when events exceed 500 or contain unknown kinds (`Any` / `Other`), which can't be reliably classified.
- **Synthetic diff for entry creation (`emit_synthetic_entry_diff`)**: `create_directory` / `create_file` return before
  the watcher fires; without it the new entry wouldn't appear until the next debounce (~200 ms). The command handler
  stats the new entry, inserts into all affected listings, and emits a `directory-diff` immediately. The watcher's later
  duplicate is prevented by `has_entry`.

## Cache helpers (caching.rs)

Used by the watcher's incremental path and synthetic mkdir to patch listings without full re-reads:

- `find_listings_for_path(path)`: all listing IDs whose directory matches the path (multiple panes/tabs may show the
  same directory).
- `find_listings_for_path_on_volume(volume_id, path)`: same, also filtered by volume ID. Prevents false matches when two
  volumes serve overlapping paths.
- `try_get_authoritative_listing(volume_id, path)`: the fresh-listing oracle for write-op pre-flight scans. Returns
  `Some(entries)` when a cached listing exists for `(volume_id, path)` and `listing_watch_coverage(path) == WatchCoverage::EveryWriter`
  (delegated to the backend via the `Volume` trait), else `None`. When multiple listings exist for the same pair (two
  panes), picks the most-recently-updated one deterministically: highest `sequence` (an `AtomicU64`), ties broken by
  latest `created_at`. Entries are cloned out under the cache `RwLock`, then the lock is released before the volume call
  (cheap clone for a flat `Vec<FileEntry>`, < 5 ms for 15k entries; matters because otherwise the volume call holds the
  cache lock across an await and blocks pane navigation). See the freshness-contract section in `volume/CLAUDE.md` for
  per-backend debounce windows callers must tolerate.
- `insert_entry_sorted(listing_id, entry)`: inserts in sorted position, returns the insertion index.
- `remove_entries_by_paths(listing_id, paths)`: removes by exact file-path match, returning `(pre-removal index,
  entry)` highest-index-first. Used by the local FSEvents incremental path, where the event path shares the entries'
  path space. ❗ There is no single-path form: that caller is always a batch, and looping one was a quadratic. See
  § "Entries by path".
- `remove_entry_by_name(listing_id, name)`: removes by file NAME within the listing (its directory, so names are
  unique). This is what the `Removed` change patch uses, so it works even when the listing's stored entry paths use a
  different path space than the notifier's resolved parent. That's the case for MTP: `MtpVolume` stores each entry's
  `path` as the storage-relative inner form (`/Documents/notes.txt`) while `notify_mutation` resolves the parent to the
  absolute `mtp://…` URL, so a full-path match never matched and `notify_mutation(Deleted)` silently no-oped (moved or
  deleted MTP files lingered in the source pane until a manual refresh).
- `update_entry_sorted(listing_id, entry)`: updates an existing entry (remove + re-insert if sort position changed),
  returns `ModifyResult`.
- `has_entry(listing_id, path)`: whether a path exists in the cached listing (classifies watcher events add vs modify).
- `get_listing_path(listing_id)`: the directory path for a listing (filters watcher events to direct children).

## Change notification API (caching.rs)

`notify_directory_changed(volume_id, parent_path, change)`: unified entry point for notifying the listing system that a
directory changed on a volume. `DirectoryChange` variants:

- `Added(FileEntry)`: single add, patches via `insert_entry_sorted`.
- `Removed(String)`: single remove by name, patches via `remove_entry_by_name` (name match, not full path — see above).
- `Modified(FileEntry)`: single modify, patches via `update_entry_sorted`.
- `Renamed { old_name, new_entry }`: same-dir rename (remove old + insert new).
- `FullRefresh`: re-reads via the Volume trait, computes a diff against the cache.

All variants enrich entries with index data and queue `directory-diff` events through `diff_emitter::enqueue_diff`.
A re-stat whose sort-relevant fields changed re-inserts the entry at its new sorted position and reports one
`DiffChangeType::Move` (`../DETAILS.md` § "Reordered rows"), which is what lets the pane cursor follow the row.
Natural deduplication: `insert_entry_sorted` returns `None` for duplicates, `remove_entry_by_name` returns `None` if
already removed. Callers: `Volume::notify_mutation()` (after each successful create/delete/rename on all volume types)
and the `rename_file` command (local FS renames). `emit_synthetic_entry_diff` remains a legacy fallback for
`create_file` / `create_directory` on volumes where `supports_local_fs_access()` is `true`.

`refresh_archive_listings(volume_id, archive_path)` is a sibling entry point for the archive content watch: it
`FullRefresh`es every open listing at or inside a changed `.zip` (parent drive id + full path) WITHOUT the drive-index
sync `notify_directory_changed` runs, since an archive-inner path isn't a real filesystem path. Rationale and the watch
that drives it: `crates/cmdr-archive/src/watch/DETAILS.md`. What a refresh DOES to this cache is
`archive_watch_integration_test.rs`, here: a refresh through `AppListings` reflected in an open listing while an outside
listing is untouched, a truncated mid-write keeping the previous listing, and LRU eviction releasing the watch. No
FSEvents timing lives in it; the backend's half of the seam is `cmdr-archive`'s `watch/host_seam_test.rs`.

`smb_pane_close_watch_integration_test.rs` is the other cell here whose other half is a backend: closing a pane's
listing (`list_directory_end`) drops a cache entry and its FSEvents `WatchedDirectory`, and must not reach the volume's
own watcher, which the index depends on with no pane open. It runs over a real `cmdr-smb` session because that watcher
is the one at stake, and it takes its fixture from `write_operations::smb_test_support`.

## Diff event coalescing (diff_emitter.rs)

All `directory-diff` emit paths funnel through `diff_emitter::enqueue_diff(listing_id, changes)` instead of calling
`app.emit` directly. The module buffers changes per listing and flushes one combined event after a 50 ms trailing
window. Producers: `caching::notify_added` / `notify_removed` / `notify_modified`; `caching::notify_full_refresh`
(SMB `STATUS_NOTIFY_ENUM_DIR` re-reads); `watcher::handle_directory_change_incremental`;
`watcher::handle_directory_change` (full re-read fallback); `commands::file_system::write_ops::emit_synthetic_entry_diff`
(`create_file` / `create_directory`); `mtp::connection::event_loop::compute_and_emit_diffs`.

**Why**: a 5k-file bulk delete used to fire one `directory-diff` per file. The frontend handler in `FilePane.svelte`
runs ~5 IPC calls per event (`getTotalCount`, `refetchColumnWidths`, `fetchEntryUnderCursor`, `fetchListingStats`, plus
a virtual-list re-fetch), so the source pane flickered heavily (the brief view's columns collapsed to width-of-name on
every recompute). Coalescing into one event per 50 ms caps the FE work at ≤ 20 emits/sec/listing and the flicker goes
away.

**Why it's safe**: only the IPC emit is deferred. Cache mutations stay synchronous and inline at the call site, so
`get_file_range` always sees the latest entries. Per-change `index` values stay correct because each producer computes
them against the cache state at the moment it mutates.

**Cleanup**: `list_directory_end` calls `diff_emitter::drop_pending(listing_id)` so an in-flight buffer for a closed
listing doesn't fire a trailing event. The E2E `flush_all_watchers` helper (`#[cfg(feature = "playwright-e2e")]`) also
calls `flush_all_pending()` so tests don't have to wait out the 50 ms window.

## File metadata tiers

Tiers 1-2 are fetched eagerly (stat + uid→name), tiers 3-4 deferred. With 50k+ files, each metadata piece has a
different cost: Tier 1 (name, size, dates, permissions) is free from a single `stat()`; Tier 2 (owner name, symlink
target) is ~1 μs and cacheable; Tier 3 (macOS Spotlight/NSURL metadata) costs ~50-100 μs/file; Tier 4 (EXIF, PDF) costs
1-100 ms+ and reads file content. See [full tier table](../../../../../../docs/notes/file-metadata-tiers.md).

macOS extended metadata (`addedAt`, `openedAt`) needs `listxattr()` / `getxattr()` beyond the fast
`fs::read_dir()` + `metadata()` path. Available via `get_extended_metadata_batch()` but not wired into the streaming
path yet.

## Finder tags

`FileEntry.tags` holds macOS Finder tags (`com.apple.metadata:_kMDItemUserTags`), parsed in `../tags.rs`. Each tag
is `(name, color)` where color `0` = none (a colorless named tag),
`1` grey, `2` green, `3` purple, `4` blue, `5` yellow, `6` red, `7` orange. The per-file xattr is the display source of
truth — Finder rewrites every file's xattr on a recolor, so we never read the system tag registry.

**Why deferred, visible-range-first.** A `getxattr` for tags costs **~15 µs/file** (benchmarked 2026-06-28, synthetic
200k-file dir, warm), ≈6× the per-entry `lstat` the core listing already pays (the `_kMDItemUserTags` namespace isn't
free). So `list_directory_core` never touches tags; the frontend calls `enrich_tags(listing_id, paths)` for the visible
range (mirroring the custom-folder-icon prefetch), and a background sweep backfills the rest. Visible range (~100 rows)
≈ 1.5 ms; a full 200k sweep ≈ 3 s, off the render path.

**Flow.** `enrich_tags` reads tags for the batch and calls `caching::apply_tags_to_listing`, which mutates entries in
place (tags are sort-irrelevant — no reorder), replaces **unconditionally** (clearing to empty so an external removal
propagates), and emits one coalesced `modify` diff for the rows that actually changed (so re-enriching an unchanged
visible range is silent). It's timeout-guarded and degrades to empty on non-local/hung paths. The whole batch resolves
against the listing's path map in one pass, and the write it does leaves both maps standing: § "Entries by path".

**Carry-forward.** A watcher re-stat builds entries via `get_single_entry`, which reads no xattr (empty tags). Every
modify path (`notify_modified`, the incremental watcher loop) calls `caching::carry_forward_tags` BEFORE storing and
emitting, copying the cached entry's tags onto the re-stat'd one — otherwise any unrelated Modify event (content edit,
mtime touch, chmod) would blank a file's dots until the next enrich. `carry_forward_tags` only ever restores (no-op when
the incoming entry already has tags), so it never masks a real change; clearing flows solely through the enrich path's
unconditional replace.

**Write path.** `tags.rs::set_tags(path, &[TagRef])` encodes the full desired set as a **binary** plist
(`plist::Value::to_writer_binary` — `plist` defaults to XML, which is NOT Finder-compatible) of `"Name\nN"` strings
(always with the `\nN` suffix, even color 0, matching Finder), and `xattr::set`s it. An empty set REMOVES the xattr
(matching Finder clearing all tags), guarded so an already-untagged file doesn't surface a spurious ENOATTR. The
encode↔decode round-trip is verified **semantically** (re-`read_tags` equals the input), not byte-for-byte against a
Finder reference — valid bplists differ in object-table ordering/dedup.

`tags.rs::toggle_color(paths, color)` is the higher-level op behind both triggers: it reads each path's current tags,
applies Finder's multi-file rule (if EVERY path already carries the color, remove it from all; otherwise add the
canonical system tag — `Red\n6`, …, `Gray\n1` — to every path that lacks it), preserves all other tags, skips rewriting
files already in the target state, and returns the new per-path sets. The `toggle_tags(listing_id, paths, color)` IPC
command wraps it in the 5 s write-timeout tier and feeds the result to `apply_tags_to_listing` so the panes refresh
immediately. A same-color *custom* tag counts as "applied" (no duplicate system tag is added; removing strips every tag
of that color).

**D11 — never touch `com.apple.FinderInfo`.** The write path touches ONLY `_kMDItemUserTags`. That 32-byte
`FinderInfo` blob carries `kHasCustomIcon` (`0x0400` at offset 8, see `icons/per_path.rs`) plus type/creator codes;
zeroing it would destroy custom folder icons and break `has_custom_folder_icon`. Modern Finder reads tags straight from
`_kMDItemUserTags`, so the dot/color shows without the legacy label bits — verified to survive in
`tags.rs::write_tests::tagging_preserves_finder_info_custom_icon_flag`. `setxattr` is atomic per attribute, so a single
file is never half-written; a multi-file toggle that fails mid-loop leaves earlier files updated and propagates the
error (the IPC command logs it rather than surfacing a hard failure — tags are low-stakes and the panes still reflect
what's on disk).

## The foreground lease a listing holds

`read_directory_with_progress` takes a `priority::foreground` lease on the volume as its FIRST statement and holds it
for the whole body. That is what tells a background SMB upload and the index scan that the user is waiting on this
share right now, for however long the folder actually takes to come back. The command entry point
(`commands/file_system/listing.rs`) still stamps the volume's timestamp on the way in: it covers the non-streaming path
and seeds the debounce, and the lease covers the listing itself. Design, both halves, and what bounds a held lease:
`priority/DETAILS.md`.

**It is RAII and nothing else.** Every exit gives it back with no code on the path: the error return, the three
cancellation returns, the restricted-empty-root return, a panic inside the task, and the task's future being dropped
when the runtime shuts down. The two things that would break it are binding the guard to `_` (which drops it
immediately) and adding a manual release beside the drop.

**Cancel releases it, deliberately.** The `select!` cancel arm returns while the detached backend task is still
unwinding, so the lease goes back before the wire work has finished. That is the right answer, not a leak: the pane has
already moved on, so nobody is waiting on that listing any more, and the transfer it was holding off should resume.

**The lease keys on the volume id the frontend asked with**, so a `.zip` opened on a share leases the SHARE (the
archive's own volume is resolved below this point and contends for nothing). Pinned by
`streaming_test::{a_listing_holds_a_foreground_lease_for_its_whole_duration, a_listing_that_fails_gives_its_lease_back,
two_concurrent_listings_on_one_volume_both_have_to_finish, dropping_the_listing_task_mid_flight_gives_the_lease_back}`.

## Cancelling a listing detaches, never aborts

`StreamingListingState.cancel` is ONE `CancellationToken` serving three roles: the sync cancellation checks, the
`select!` arm that races the read, and the backend's cooperative cancel token via `Volume::list_directory_with_cancel`.
It used to be a flag plus a `Notify`; one token means the "is it cancelled?" checks and the "wake up" signal can't
disagree, and `cancel_listing()` is one call. By the time `read_directory_with_progress`'s `select!` cancel arm runs,
the backend has necessarily already been told to stop — the same cancellation woke it.

That arm then emits `listing-cancelled` and RETURNS, dropping the listing task's `JoinHandle`. Dropping a `JoinHandle`
detaches the task; it does not cancel it. So the backend keeps running for exactly as long as it needs to reach its own
safe boundary, while the user sees an instant cancel.

❌ Never `listing_task.abort()` there. Abort drops the listing future at whatever await point it's sitting on. For MTP
that's mid-PTP-transaction: the device is left expecting bytes nobody will send, and it wedges until the user replugs
the phone (`mtp/connection/CLAUDE.md` § the dropping-timeout guardrail). MTP bails between per-handle `GetObjectInfo`
round trips, so cooperative cancel costs at most one round trip of latency.

Backends that ignore the token (local, in-memory, SMB today) run their listing to completion in the detached task.
That's not a regression: local listings run inside `spawn_blocking`, which `abort()` never interrupted either.

Pinned by `streaming_test::test_cancel_unwinds_the_listing_instead_of_aborting_it`, which drives a fake volume that
only ends when its token flips and fails if its future is dropped first.

## Serializing full refreshes

A `FullRefresh` re-reads the directory through the Volume trait, diffs the result against the cached listing, and then
REPLACES that listing wholesale. Read and write are two steps, so two refreshes of the same directory running at once
can finish in the opposite order to the one they started in, and the loser writes a snapshot the winner has already
superseded.

**Why that is worse than a flicker.** Nothing schedules a re-read afterwards. The listing keeps the older truth until
some unrelated event happens to touch the directory again, so entries that exist on disk are simply absent from the
pane — for as long as the folder stays quiet. Watched folders take bursts all the time (an unzip, a `git checkout`, an
rsync, a build writing output), and a burst is exactly what fires several refreshes at once.

`notify_full_refresh` therefore takes a per-directory turnstile, keyed on `(volume_id, parent_path)` — the pair that
identifies what a refresh re-reads — before doing anything. Holding it across the read makes read-then-write atomic
against other refreshes of that directory, which is the whole guarantee: a refresh that starts later reads later, so it
cannot answer with a staler directory.

**At most one runs and one waits.** A third arrival returns immediately rather than queueing: the refresh already
queued starts its read after the running one finishes, so it will observe everything the third arrival would have. That
turns a storm of N events into two reads instead of N, which matters most on the big directories where a refresh is
expensive. Both properties are pinned by
`a_slow_refresh_cannot_overwrite_the_listing_a_newer_one_already_wrote` and
`a_storm_of_refreshes_costs_two_reads_of_the_directory_not_one_per_event`, which script a fake volume so the slow read
is the one that started first (otherwise the interleaving is timing-dependent and the test is itself a flake).

The turnstile map prunes entries whose only remaining reference is the map itself, so a long session browsing a wide
tree doesn't accumulate one per directory it has left.

❌ Don't "optimize" this back into concurrent refreshes. The cost it removes is real and the failure it prevents is
silent: the pane looks settled and correct, which is why it went unexplained through several E2E flakes before the
duration evidence pinned it.
