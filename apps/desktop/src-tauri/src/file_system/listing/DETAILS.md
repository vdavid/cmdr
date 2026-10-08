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
   |<--- listing-stalled event --------| (only if the read goes quiet for 8 s; the listing keeps going)
   |<--- listing-progress event -------| (every 200ms, { listingId, loadedCount })
   |<--- listing-read-complete event --| (when read_dir finishes, { listingId, totalCount })
   |                            [overlay rows folded in; sorting + caching; watcher arm dispatched, not awaited]
   |<--- listing-complete event -------| (ready, { listingId, totalCount, volumeRoot, storedPath })
   |                                   |
   |-- getFileRange(listingId, ...) -->| (on-demand fetching)
   |<-- [FileEntry, FileEntry, ...]    |
```

`listing-complete` is what commits the listing in the pane, so nothing slow may sit in front of it. Arming the FSEvents
watch used to, and no longer does: `start_watching_detached` hands it to the blocking pool. Why arming is slow, what
that cost, and the two rules that keep it cheap: `../DETAILS.md` § "Arming a listing watch is detached".

## The overlay step

Between the index enrich and the sort, `read_directory_with_progress` calls `crate::listing_overlays::decorate`, which
folds in rows a PANE sees that the volume doesn't hold. `listing/operations.rs` (the sync sibling) and
`caching::notify_full_refresh_locked` (the watcher-driven re-read) do the same, in the same place, so a refresh can't
quietly strip them.

Today's one contributor is the git portal, which puts the six virtual category rows into a repo's `.git/` listing.
Placement matters three ways:

- **After enrich**: a contributed row has no drive-index entry, so there is nothing to look up for it.
- **Before the sort**: the rows land wherever the pane's own sort puts them, with no ordering privilege of their own.
- **Before the cache insert**: the pane reads its rows out of the cache, so a row that isn't there isn't shown.

`CachedListing` records how many rows came from an overlay, and `try_get_authoritative_listing` declines any listing
carrying some. A decorated listing is a PANE view, never a picture of a directory a delete walker or a copy scan may
reuse. ❌ Never let one answer that oracle: the rows have no inode behind them, and a walker that meets one stops
mid-operation. The seam and the reasoning: `src/listing_overlays.rs`, `volume/DETAILS.md` § "Architecture".

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

The record and the map are `cached_listing.rs`, everything that patches one or notifies about it is `caching.rs`, and
the six-hour backstop that reclaims a leaked one is `orphan_reaper.rs`.

- **`LISTING_CACHE`**: global `RwLock<HashMap<String, CachedListing>>`, keyed by `listing_id` (UUID per navigation).
- **`CachedListing`**: `{ volume_id, path, entries, visible_rows, path_index, sort_by, sort_order,
  directory_sort_mode, sequence, created_at, last_accessed_ms, overlay_rows }`. `entries` is private: `entries()` reads,
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

Both ends also tell `crate::listing_lifecycle`, the seam for subsystems that keep something alive while a pane is
showing a directory (today: the git portal's per-repo watcher, which a virtual path can't arm through the FSEvents
watcher). ❗ The close call comes AFTER the cache removal: an observer's own detached arm reconciles against
listing-cache membership, so releasing while the entry is still there lets a racing arm re-take what was just released.

### Backstop reaper

`start_orphan_listing_reaper` (spawned in `lib.rs` setup) sweeps every `REAPER_SWEEP_INTERVAL` (30 min) and tears down
any listing idle past `ORPHAN_IDLE_WINDOW` (6 h) via the same `list_directory_end` path, so a leaked listing (close IPC
never delivered) can't pin its entry vector and OS watcher for the whole session. Pure, clock-injectable seam:
`orphan_ids(now_ms, window_ms, …)` and `reap_orphaned_listings_at(now_ms, window_ms)`. Mirrors the search index's
idle/backstop timers and the file viewer's window-`Destroyed` net.

It keys on `last_accessed_ms`, NOT `created_at`. `created_at` is stamped once and never refreshed, so an age-based reaper
keyed on it would evict a pane open all session. `last_accessed_ms` (an `AtomicU64` of ms-since-a-process-epoch) is
bumped by every operation that proves the listing still backs a live pane: the read accessors (`get_file_range`,
`get_file_at`, `get_file_beside`, `get_listing_stats`, the index/path/batch lookups), `resort_listing`, and every
watcher/notify cache patch (`insert_entry_sorted` / `remove_entries_by_paths` / `remove_entry_by_name` /
`update_entry_sorted` / `update_listing_entries`). `AtomicU64` so read accessors stamp it lock-free under a shared `LISTING_CACHE.read()`. The
6 h window is deliberately generous: we'd rather never evict a live listing than aggressively reclaim.
`refresh_listing_index_sizes` intentionally does NOT touch it: it's driven by background indexing, not user/FS activity,
so touching there could keep a truly-orphaned listing alive indefinitely.

**The pane heartbeat is what proves a listing live**, not access. A pane left on a quiet folder overnight makes no reads
and gets no FS events, so access alone let the reaper take an on-screen `~/Downloads` listing after six idle hours: the
pane kept stale rows, its watcher was gone (new downloads never appeared), and every F3–F6 failed on the missing listing.
Every 30 min the frontend's `file-explorer/pane/listing-liveness.ts` names each LANDED listing through
`keep_listings_alive`, which touches the ones cached and returns the ones it no longer holds. A leaked listing stops
being named and is reaped as before.

**A lookup on a missing listing is a typed `ListingLookupError::Gone { listing_id }`** (every accessor in
`operations.rs`, and their commands). It's the accessors' only failure: they take the cache lock with
`*_ignore_poison`. The frontend funnels every such refusal (`$lib/tauri-commands` `listing-gone.ts`), and every id the
heartbeat returns, to the liveness registry, and the owning pane re-lists the same folder with the cursor on the same entry. A pane registers its
listing only once it has landed, so a read racing a navigation (which also answers `Gone`) never triggers a re-list. The
clipboard and drag commands still flatten it into their `String` errors; the pane recovers through its next read or
heartbeat.

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
leave an entry out: `FileEntry::is_hidden` (when `include_hidden` is off) and scratch a running operation owns
(`file_system::staging`).

**Answering "which entry is row N" by walking and counting is what wedged the app.** The MCP pane mirror fetches ~100
rows per sync, each fetch was one full walk, and at the bottom of a 74,144-entry directory that came to ~7.4 M predicate
evaluations per index event on the main thread — IPC stopped being answered at all. Evidence and the before/after:
`docs/notes/listing-row-fetch-quadratic-2026-08-22.md`.

**Decision: materialize the row map once per `(listing, include_hidden)`, and split it in two.** `settled` is a
`Vec<u32>` of entry indices nothing can hide any more; `candidates` holds the scratch-NAMED entries with the count of
settled rows ahead of each. Rows merge candidates using the listing's committed exact-path `ScratchProjection`,
never live settings or ownership. A lookup is an array index plus a binary search over a list that is almost always
empty. Reads check only those candidates for drift before using the map; publication is described in § "Diff event
coalescing".

**Why the split rather than an invalidation hook.** The `is_hidden` half is stable, but the scratch half is not: an
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
(`get_listing_stats` and mutation publication counts) share that one pass — but a future
caller that reads a single shallow row per mutation and nothing else is the shape to watch.

**What is still O(rows), and can still be re-multiplied by a caller that loops it**: `find_file_index`,
`get_file_beside`, and `get_listing_stats`. They are a name search and a full sum, so linear is their floor, not an
accident. `find_file_indices` is the batch form of the first, and `get_file_beside` exists so a caller wanting a
neighbour doesn't compose two calls; reach for those instead of a loop.

## Compare directories (compare.rs)

Total Commander's ⇧F2: each pane marks the files the other pane lacks plus, per `CompareDirectoriesMode`, the newer
copies (`newerAndMissing`, TC's default), nothing more (`missing`), or both copies of a file whose size differs
(`sizeAndMissing`). Folders are left alone, and a folder never counts as a file's counterpart.

- **Read off both cached listings under ONE lock, in each pane's row space** (`CachedListing::rows`), so the answer is a
  ready selection and a row the pane doesn't show is never marked.
- **The answer names the committed visible revision**, read under that same lock. `settled` requires both requested
  `includeHidden` values to match the committed settings. Scratch drift is reconciled before taking that read lock,
  so stable shown scratch and stable owned hidden temps are valid settled states.
  The frontend also requires its applied revisions to match; a queued event is safe because its revision was already
  committed. See § "Diff event coalescing" for publication and guarded consumption.
- **Names match as the Mac does**: the exact spelling first, else a name that folds to the same key
  (`cmdr_fs::name_fold`, case and Unicode form). A folded match counts only when the key is unique on BOTH sides, so
  the two directions always agree: `Report` and `report` (a case-sensitive volume) against `REPORT` pair nothing.
- **Runs off the IPC thread with a 10 s deadline** (`blocking_typed_result_with_timeout`), answering a typed
  `CompareDirectoriesError` (`gone` / `timedOut` / `internal`).
- **Two seconds apart is the same time** (`SAME_TIME_TOLERANCE_SECS`): FAT and many shares store time in 2 s steps. An
  unknown time or size never marks a copy; only a difference we can see does.
- No content comparison: like TC's ⇧F2, it reads metadata only. Byte comparison belongs to Synchronize directories.
## Quick filter (name_filter.rs)

The pane's "type to narrow" mode (Total Commander's quick filter). The pattern lives on the `CachedListing`
(`set_name_filter`) and is one more input to the row predicate (`visible_rows::shows`), so it is NOT a second filter
point: counts, ranges, selection, type-to-jump, and `directory-diff` rows all speak the filtered row space.

- **Decision: the filter is the listing's, not a per-call argument like `include_hidden`.** Why: every pane-index IPC
  already carries `include_hidden`; threading a pattern through all of them would touch every caller for no gain, since
  only the pane showing the listing ever filters it. The cost: the filter is not a slot key of `VisibleRowsCache`, so a
  change drops both slots.
- **`set_listing_name_filter` swaps the row space under ONE write lock** and answers with the new count plus where the
  cursor's file and the selected files landed (the `resort_listing` shape). A selected file the filter hides drops out
  of the selection, so no operation acts on a row the user can't see. A change drops the queued diffs, like a hidden
  toggle, and answers the diff `sequence` the new row space starts at (bumped under the same lock).
- **Matching cost**: the wildcard-free path folds each name once (`fold_name` allocates only when folding changes it)
  and does one `contains`; a wildcard pattern walks chars with backtracking only to the last `*`, so no recursion
  and no blowup on `*a*a*a*`. Measured by the review on a 50k-name folder: fast enough to type into.
- **Typing narrows down to the last match, never past it.** A growing pattern is sent with `refuse_empty`; one that
  matches no entry is refused under the same lock (`accepted: false`, old filter kept) and the frontend drops the
  keystroke. The check walks every entry, not the current rows: an edited pattern needn't narrow the old one.
- Matching: substring anywhere in the name, folded by `cmdr_fs::name_fold` (case and Unicode form), `*` / `?` as
  wildcards with an implied `*` on both ends. A new listing starts unfiltered; the frontend side is
  `apps/desktop/src/lib/file-explorer/pane/DETAILS.md` § Quick filter.

### The quick filter and in-flight diffs

A filter change is a committed visible revision, using the same `sequence` identity as sort, hidden visibility,
scratch reconciliation, selection snapshots, and comparison. There is no independent filter epoch.

- The guarded setter reconciles scratch drift, then checks the caller's expected revision and hidden setting BEFORE
  interpreting selection indices. A typed `Changed` refusal leaves the filter untouched; the pane drains buffered
  transitions and retries with fresh indices.
- A successful change advances the revision and drops superseded queued transitions under the cache write lock
  (cache then queue). A new-space watcher mutation cannot enqueue until that lock is released, so the drop cannot
  erase it. An already-drained old batch is harmless: its revision precedes the response barrier.
- Watcher and overlay replacements sort, compare committed projected rows, replace, and publish under that same
  write lock. An async directory read crossing a filter change therefore derives its diff in the current row space,
  without an externally computed diff or an enqueue after unlocking.
- Filter requests use the pane's `pane-row-state.ts` reconfiguration gate. Pending diffs wait for the response;
  synchronous installation establishes the response revision before buffered batches drain. Reconfiguration also
  invalidates old async selection continuations and gates compare and destructive-operation snapshots.

`name_filter_test.rs` covers old-space pending drops, fresh publication after the setter unlocks, replacements
crossing a filter change, guarded selection refusal, and scratch drift before filtering. The post-unlock test injects
a real watcher mutation and holds the flush timer; no sleeps or scheduler timing are needed.

## Diffs speak the pane's rows

A `directory-diff` index is a row of the pane showing the listing, the same space `get_file_range` reads, ❌ never an
index into `entries`. The pane's rows are `CachedListing::pane_rows()`: the row map at the listing's own
`include_hidden`, recorded at `list_directory_start_streaming` and updated by `set_listing_include_hidden`, which the pane calls
first thing in `hidden-files-resync.ts` (after every load and every toggle). It's per listing, and each pane holds its
own listing, so two panes on one folder each get their own rows.

**The rule: the cache takes every change; the pane hears only the rows it shows.** An entry the pane shows on neither
side of a change (a dotfile, `~/Library`'s `UF_HIDDEN` flag, in-flight scratch) updates the cache and emits nothing, so
turning hidden files on later shows fresh data at once. One it shows on one side only arrives as a remove (it turned
hidden, or went) or an add (it turned visible, or came). One it shows on both is a modify or a move. A change with no
visible part queues nothing, so no event and no sequence bump.

**Why.** Two costs of reporting entry indices. The idle one: on a pane on `~`, dotfile writes produce a diff about
every 10 s, in bursts, and each one cost the webview a count, range, stats, and column-width refetch with nothing
visible to change, even with hidden files off (most of WebContent's idle CPU after the index-size work, GitHub #92).
The correctness one: the pane reads the index as a row, so with hidden entries sorted above a change, the cursor and
selection slid a row off on every add or remove (`pane_diff_test::a_removal_reports_the_row_it_left` reported entry 3
for row 1).

**How.**

- The single-entry helpers (`insert_entry_sorted`, `remove_entries_by_paths`, `remove_entry_by_name`,
  `update_entry_sorted`) return `PaneRows { before, after }`, read off the row map as it stood BEFORE their own patch and
  under the same write lock (`VisibleRows::rows_before` / `row_of_entry`, two binary searches). ❗ Reading after the patch
  would rebuild the map per patch, once per add in a burst. `DiffChange::for_pane` turns the pair into a change or none.
- Batch re-reads (`publish_replacement`, the watcher's `handle_directory_change`) call `update_listing_entries`,
  which diffs committed projected rows through `CachedListing::replace_entries` under the cache write lock.
  `compute_diff(old, new, include_hidden, name_filter)` is a test-only helper.
- `CachedListing::shows` combines the filter with captured scratch decisions and hidden visibility, matching its row map.
- `is_entry_modified` counts `is_hidden`: with hidden files shown the row dims, and a `chflags hidden` reaching a full
  re-read would otherwise leave the cache holding the old flag.
- A toggle drops what's queued for the listing: it's numbered in the old row space, the pane re-reads its count and
  cursor right after, and the cache already holds every change.

**Everything else a hidden change could reach, checked:** the status bar's counts and sizes sum the pane's rows; the
`..` row and folder sizes come over `listing-index-sizes-changed`, not this event; Brief column widths measure the
pane's rows; the cursor and selection sit on shown rows, and an entry that turns hidden reaches them as a removal.

**Known gap**: the Ask Cmdr bulk-rename review listens to every `directory-diff` by filename to recheck clashes. A
hidden destination name (a rename to a dotfile) appearing or vanishing externally, in a folder whose pane hides hidden
files, no longer triggers that recheck; the write engine's exclusive final rename still refuses the clash.

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

## Sorting

`sorting.rs` holds the comparator every list in the app is ordered by. `entry_comparator(sort_by, sort_order,
dir_sort_mode)` is generic over the `SortableEntry` trait, which names the seven fields ordering reads: name,
is_directory, size, modified_at, created_at, and the two recursive-size fields the Size column's directory rule needs.

Two implementors. `FileEntry` answers from its own fields. `commands::search::SearchSortRow` is a search-results pane's
row, and it answers `None` for the creation time and recursive size a search result doesn't carry, which lands it on the
comparator's existing unknown-value arms rather than on a second set of rules: ordering such rows by Created falls back
to the name, and under Size the directories are all unknown and sort by name among themselves.

**Why a trait and not a conversion.** Building a fabricated `FileEntry` per search row would be a second place where the
mapping from "a row" to "what orders it" is decided, and that mapping is the thing that must not drift. The trait makes
the shared fields the contract and the generic monomorphizes, so the listing's hot path pays nothing.

**`DirectorySortMode` decides whether directories lead.** `LikeFiles` and `AlwaysByName` put them first (by the column,
or A→Z by name whatever the direction, except on the Name column itself), and `MixedWithFiles` ("Show folders first" off, #291) drops that step: `compare_mixed` ranks a directory
among the files by the same column. Size is the one column where the two kinds read different fields (a directory's
`known_dir_size`, a file's `size`), so the mixed sort uses one rule for both, an unknown size LAST whatever the order,
to stay transitive: per-kind rules (a file's unknown first, a directory's last) would cycle once mixed. The mode
arrives per listing from the frontend, which folds its two settings into it (`apps/desktop/src/lib/file-explorer/DETAILS.md`
§ Sorting), and every sort path reads it off the `CachedListing`, so the watcher's re-sorts, archive panes, and every
volume follow it.

`sort_search_results` (`commands/search.rs`) is the frontend's way in: it answers with the input indices in sorted
order, and the caller re-orders the rows it already holds. The frontend deliberately has NO comparator of its own; the
snapshot store's sort round-trips through this command. `apps/desktop/src/lib/search/DETAILS.md` § "The snapshot pane's
row order".

### Collating names

`collation.rs` owns the one answer to "how do two names rank". It wraps an ICU4X `Collator` built for the locale
`intl::active_locale()` reports, with numeric ordering on.

**Why a collator and not code-point order.** macOS stores whatever bytes a writer hands it, so a single folder holds
both spellings of an accented name: Cocoa writers emit NFD (`a` + U+0301), browsers and most others NFC (U+00E1).
Comparing code points decides at that character and orders by the SPELLING, which put `…Dávid…-signed.pdf` above
`…Dáviad…-araw.pdf` in a real `~/Downloads` (5 of its names were NFD, 7 NFC). Code points also rank by number rather
than by category, which stranded every accented letter after `z` and every `_name` below every `2026-…` name, where
Finder, Total Commander, and Explorer all put `_` first.

**One order, two readings.** `NameCollator::compare` collates a pair live; `NameCollator::key` serializes one name's
weights to bytes that sort the same way. The bulk `sort_entries` builds a key per row and sorts on those, so a 100k
listing collates 100k times instead of ~1.7M; the incremental `insert_entry_sorted` compares live, because
`partition_point` only probes ~17 rows to place one entry. `SortableEntry::compare_name` is where the two meet: the
default asks the collator, and the private `Keyed` wrapper overrides it to read prebuilt keys.

Prebuilt keys are what keep the collator affordable: collating live on every comparison measured ~4x SLOWER than the
code-point comparator it replaced, and keying once per row lands ahead of it instead. On 100k synthetic names, sorting
by Name, release build, M-series laptop, 2026-09-08: **105 ms, against 228 ms** for the previous
`to_lowercase` + `alphanumeric-sort` comparator on the same rows. The new figure covers the whole of `sort_entries`
(key building, the index sort, and the permutation).
`collation_test::the_bulk_sort_and_the_live_comparator_agree_on_unicode_names` and
`collation::tests::a_key_ranks_names_the_way_compare_does` pin that they never diverge.

Sorting is covered by three suites, split along the questions they answer rather than by size: `sorting_test` (which
column, which direction, and the edge cases), `sorting_dir_mode_test` (`DirectorySortMode`, so how a directory ranks
against the files beside it, including the honest-size coverage flags), and `collation_test` (the order names
themselves come out in). `sorting_test_support` holds the two builders more than one of them needs; a builder used by
one suite stays in that suite.

The fixture each suite needs is what marks the seam. `collation_test` pins the process-wide reading language under a
lock shared with `intl::native_strings`, which is why case-insensitive ordering is tested THERE and not beside the
other column tests: case folding is a collation question, so `to_lowercase` would answer it wrongly (see the
raw-bytes-tiebreak note above). `sorting_dir_mode_test` owns the honest-size directory builder. Each file's header
names its two neighbours, so the boundary is legible from whichever one you open first.

**Both readings end with a raw-bytes tiebreak**, because the NFC and NFD spellings of one name carry identical
collation weights and would otherwise compare `Equal`. Without a total order the watcher's re-read could order such a
pair either way between two passes, and `compute_diff` would report a `DiffChangeType::Move` for a row that never
moved.

**❌ Never persist a `NameKey`.** The bytes are ICU4X's internal encoding, and a library or CLDR update may re-tune
them. That is harmless while every key in a comparison came from the running binary, and silently wrong the moment a
stored key meets a fresh one.

**On the `unstable` feature.** `write_sort_key_to` sits behind `icu_collator`'s `unstable` flag, which ICU4X exempts
from semver (it may change in a minor release). Every shape that change can take is a compile error, since the only
surface used is that one call and `Vec<u8>`'s upstream `CollationKeySink` impl; `Cargo.lock` is committed, so a break
arrives as a red Renovate PR rather than on `main`. What type checking cannot see is a change to the key BYTES, which
is what the never-persist rule and the ordering tests above cover.

**Locale changes are pulled, not pushed.** `active_collator` caches one collator per locale tag and re-reads
`intl::active_locale()` on each call, so the next sort after an OS language switch uses the new language. Listings
already sorted keep their order until something re-reads them, exactly as they do when the sort column changes.

`sort_entries` sorts indices and then permutes the rows in place by cycle-following, rather than building a second
`Vec<FileEntry>`: a row is large enough that cloning 100k of them would cost more than the sort. Note the direction
flip before `apply_permutation` — `order[i]` names the row that belongs AT `i`, and the applier wants where the row
currently at `i` GOES. Applying the un-inverted permutation scrambles the whole listing without failing anything
loudly, so `sorting::tests::apply_permutation_moves_each_row_to_its_destination` covers each cycle shape.

## Decisions

- **Streaming with a background task, not chunked IPC**: chunked needs multiple IPC calls and complex state tracking.
  Streaming spawns a `tokio::spawn` task and emits events; the frontend stays responsive (Tab works, ESC cancels).
- **Cancellation via `AtomicBool` checked per-entry**: network folders iterate slowly (seconds per entry); a per-entry
  check keeps ESC responsive (cancel within ~100 ms).
- **Three-stage progress (opening → progress → read-complete → complete)**: `listing-opening` (about to start slow
  I/O), `listing-progress` (loaded N, every 200 ms via `list_directory_core_with_progress`), `listing-read-complete`
  (all read, sorting now), `listing-complete` (ready to render).
- **Sort after read, before caching**: the frontend expects sorted order, and the sort runs in the background task
  after all entries are collected. Cost is ~105 ms per 100k entries (§ "Collating names"), well under the I/O it
  follows.
- **Enrichment at cache-write time, not on `get_file_range`**: every path that stores entries (streaming, watcher
  update, re-sort) enriches first. Index freshness is event-driven: `src-tauri/src/listing_index_sizes/` writes the rows
  an index update moved straight into the cache (`CachedListing::update_index_sizes_by_path`, which keeps both maps)
  before it tells the pane, so `get_listing_stats` stays read-only and sees up-to-date `recursive_size`. A whole-volume
  update runs the full `refresh_listing_index_sizes` re-enrich instead.
- **Hidden-file filtering in Rust, not the frontend**: visible count is unknown until all files are read. APIs accept
  `include_hidden: bool` and read through the listing's row map (§ "Row numbers"); `directory-diff` events use the
  listing's own recorded setting (§ "Diffs speak the pane's rows").
- **The listing read commands are `async`**: a sync `#[tauri::command]` runs on the MAIN thread in Tauri 2, so one slow
  accessor stops the app answering IPC at all, which is principle 2's "never block the main thread" broken at the IPC
  layer. `refresh_listing_index_sizes` goes one further onto the blocking pool, because it runs two indexed SQLite
  queries and an index storm fires it once per event per pane.
- **Font metrics in a Rust binary cache, not frontend canvas measurement**: measuring 50k filenames in JS is slow. The
  frontend measures each code point's width once via Canvas and ships the table to Rust; later text-width queries are
  hash lookups in the cached `.bin` table. `calculate_max_width_with_suffixes()` is the entry point, used by
  `brief_columns::compute_brief_column_text_widths` to size each Brief column to its widest filename (plus a per-row
  trailing suffix that reserves room for the Finder tag-dot cluster).
- **Revision on `CachedListing`, not `WatchedDirectory`**: it describes the committed pane view on every volume,
  including SMB and MTP, which have no FSEvents `WatchedDirectory`. See § "Diff event coalescing".
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
- `try_get_authoritative_listing(volume_id, path)`: the fresh-listing oracle for write-op pre-flight scans and the
  look-alike check before a new name (`write_operations/look_alike.rs`). Returns
  `Some(entries)` when a cached listing exists for `(volume_id, path)` and `listing_watch_coverage(path) == WatchCoverage::EveryWriter`
  (delegated to the backend via the `Volume` trait), else `None`. When multiple listings exist for the same pair (two
  panes), picks the most-recently-updated one deterministically: highest `sequence` (an `AtomicU64`), ties broken by
  latest `created_at`. Entries are cloned out under the cache `RwLock`, then the lock is released before the volume call
  (cheap clone for a flat `Vec<FileEntry>`, < 5 ms for 15k entries; matters because otherwise the volume call holds the
  cache lock across an await and blocks pane navigation). See the freshness-contract section in `volume/CLAUDE.md` for
  per-backend debounce windows callers must tolerate.
- `insert_entry_sorted(listing_id, entry)`: inserts in sorted position, returns the pane rows (`PaneRows`, § "Diffs
  speak the pane's rows"). All four patch helpers here answer in pane rows.
- `remove_entries_by_paths(listing_id, paths)`: removes by exact file-path match, returning `(pre-removal pane rows,
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
  returns the pane rows before and after; a pair that differs is a `Move`.
- `has_entry(listing_id, path)`: whether a path exists in the cached listing (classifies watcher events add vs modify).
- `get_listing_path(listing_id)`: the directory path for a listing (filters watcher events to direct children).

## One spelling per directory (`ListingPath`)

`CachedListing::path` is a `ListingPath`, and `ListingPath::on_volume(volume_id, path)` is the only way to make one: it
asks the registered volume for its spelling (`Volume::listing_path`, identity by default). Every store
(`CachedListing::new`) and every path-keyed lookup (`find_listings_for_path_on_volume`, `get_cached_listing`,
`try_get_authoritative_listing`, the archive refresh) builds its key through it BEFORE taking `LISTING_CACHE`, since it
reads the volume registry.

**Decision:** canonicalize in the cache, not at each caller. **Why:** a pane path enters from the frontend (navigation,
Enter on a row, go-to-path, history, tab restore), from a backend's change report, and from pre-flight oracle queries
built off entry paths. Only MTP spells one folder two ways today (the `mtp://` storage URL its reports and the volume
switcher use, and the inner `/DCIM` its rows carry), but a per-caller fix would have to find every route, and a verbatim
match there dropped every Cmdr-made delete on a pane the user had entered with Enter (field reports ERR-QW42X,
ERR-46A6B, v0.44.0). MTP canonicalizes to the inner spelling because its rows, the drive index, and the walkers already
use it. ADB, SFTP, and WebDAV rows carry their full app URL, and SMB, archive, and git-portal rows share their root's
spelling, so they keep the default. Pinned by `mtp_listing_path_test.rs`.

`find_listings_for_path(parent_path)` has no volume id and compares verbatim; only local callers use it, and a local
path has one spelling.

## A pane path the volume stores another way (`foreign_path.rs`)

`ListingPath` folds spellings a backend itself hands out. A different problem is a path that came from OUTSIDE the
volume's listings (typed, pasted, a restored tab, a favorite, MCP `nav_to_path`, or a pane carried over from the macOS
kernel mount, which decomposes every name) on a backend that matches names byte-for-byte. SMB is one: an accented
folder the share stores composed answers the kernel's decomposed spelling with `STATUS_OBJECT_PATH_NOT_FOUND`
(ERR-VETBX, `crates/cmdr-smb/DETAILS.md` § "SMB names are opaque bytes").

- **The pane seam lists as stored.** `read_directory_with_progress` lists through `list_as_stored`: as given first
  (free on the happy path), and only on `NotFound` asks `Volume::find_stored_spelling`, then lists what it answers. From
  there on the listing IS the stored spelling: `CachedListing::path`, the watch, the overlays, the index enrich, and
  every entry path. `listing-complete` carries `storedPath` (only when it differs), and the pane re-spells its tab and
  current history entry in place (`navigate.ts::adoptStoredSpelling`), so there's no Back step to the other spelling and
  a pinned tab doesn't fork. Favorites stay as the user wrote them: we don't silently edit something a person
  authored, and the cost is one resolve per click, remembered per share until the folder changes.
- **A backend swap respells what's open.** When `network/smb_upgrade.rs` hands a share from the kernel mount to a
  direct connection, `respell_listings_on_volume` re-reads each open listing on it through the new backend, holding the
  directory's refresh turn (`caching::refresh_turn`). A listing whose path changed spelling is re-keyed, its entries are
  written whole (the diff matches rows by name, so it can't see every path changing), and the pane adopts the path from
  `listing-respelled` and refetches its rows. That sweep sees only listings already cached, and a pane landing on the
  share starts the upgrade the moment it starts listing (`network/smb_pane_upgrade.rs`), so the swap often lands
  mid-read. So `read_directory_with_progress`, right after its cache insert, re-reads its own listing when the registry
  no longer serves the id with the backend that read it (`respell_if_read_by_a_replaced_backend`, pointer identity): a
  swap before the insert is caught there, one after it by the sweep.
- **Files from outside Cmdr respell where they enter.** A Finder drag-in and a paste of files copied in Finder hand
  over kernel-mount paths (`/Volumes/<share>/…`), which route to the direct SMB volume when the share has one
  (`resolveSourceVolumeId`). The frontend asks `stored_spellings` (`apps/desktop/src-tauri/src/commands/file_system/stored_spelling.rs` →
  `foreign_path::stored_spellings`) once, right after resolving the source volume, so the scan preview, the conflict
  check, the transfer, and the journal (which a rollback replays) all carry one spelling. All-ASCII paths skip the
  round trip (one Unicode form, and the kernel keeps the server's case); a path with no other spelling, two look-alikes,
  or a deadline past 10 s goes as given, so an operation still means exact bytes and never a guessed twin.
- **Decision: only these seams resolve.** **Why:** a resolve can't tell "another spelling of this folder" from "a
  different folder sharing its folded name, the one named having vanished". A delete walker, a copy scan, a watcher
  refresh, or an existence probe holds a path that came out of a listing, so a miss there means it's gone, and
  resolving would hand the walker a look-alike twin. ❌ Don't route those through `list_as_stored`.
- **Entry points deliberately left alone.** MCP tools never hand a volume a raw file path (they go through pane
  navigation or row names). The file viewer reads a direct share through the kernel mount (`paths_are_os_visible`), and
  only reaches `SmbVolume` when that mount is gone. An approved agent proposal replays paths from the drive index, which
  on a direct share holds the server's bytes; its fingerprint binding refuses a path that no longer opens, and its
  per-op reporting is keyed on the proposal's own paths, so a respell there would unhook it.
- The `AmbiguousName` refusal (two look-alikes, neither exact) reaches the pane as its own listing error.
- **The cursor lands on a look-alike name.** `find_file_index` (a restored cursor, MCP `move_cursor`, a reveal from
  Finder, `selectName` after a foreign landing) asks `VisibleRows::row_of_any_spelling`: the exact name first, else the
  ONE row whose name folds the same (`cmdr_fs::name_fold`), else nothing. Placement only; every row a command ACTS on
  still comes by exact name.

Pinned by `foreign_path_test.rs`, `streaming_test.rs::a_foreign_spelling_lands_the_listing_on_the_stored_path`, the
Docker cell `network/smb_upgrade_respell_test.rs`, and on the frontend the drop and paste suites
(`drag-drop-controller.svelte.test.ts`, `clipboard-operations.test.ts`).

## Change notification API (caching.rs)

`notify_directory_changed(volume_id, parent_path, change)`: unified entry point for notifying the listing system that a
directory changed on a volume. `DirectoryChange` variants:

- `Added(FileEntry)`: single add, patches via `insert_entry_sorted`.
- `Removed(String)`: single remove by name, patches via `remove_entry_by_name` (name match, not full path — see above).
- `Modified(FileEntry)`: single modify, patches via `update_entry_sorted`.
- `Renamed { old_name, new_entry }`: same-dir rename (remove old + insert new).
- `Replaced(Vec<FileEntry>)`: the backend already re-read the directory and hands the contents over; the host sorts them
  the listing's way, diffs, stores, and publishes (`publish_replacement`).
- `FullRefresh`: re-reads via the Volume trait, computes a diff against the cache.

`Replaced` and `FullRefresh` differ only in who does the read, and both end in the same `publish_replacement`. Report
`Replaced` when the entries are already in hand (a device event loop that has to invalidate its own path cache and
re-list anyway); report `FullRefresh` when the host should go and get them, which also gets the volume-wide fallback
when no listing matches the exact path. ❗ Sorting before the diff is load-bearing, not a double sort: a backend answers
in its protocol's order (MTP by object handle), so a diff computed against that order carries indices pointing at the
wrong rows in a pane sorted any other way.

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
is the one at stake, and it takes its fixture from `write_operations::backend_suites::smb_test_support`.

## Diff event coalescing (diff_emitter.rs)

Mutation owners (`caching.rs` patch helpers and tag writes, `operations.rs` full replacements) allocate the listing's
visible `sequence` and enqueue under ONE cache write lock. Initial revision is zero; hidden-only patches do not
advance it. Full replacements enrich and sort first, then `diff_rows` compares the committed old projection with the
pinned replacement projection and stores the final replacement under that lock. An identical raw replacement still
reconciles visibility drift. Callers never enqueue the same patch again. Lock order is cache then queue; the emitter
never reads the cache to stamp a batch.

**Scratch visibility belongs to the revision.** `CachedListing` captures exact path-keyed decisions at construction,
so revision zero's count and later rows agree. Ordinary directories have an empty projection. Read boundaries keep
the shared-lock fast path when there is no drift; otherwise they reacquire the write lock, capture and recheck live
decisions once, diff old/new projected rows, commit the projection, and publish its revision and pinned count before
consuming indices. Stable scratch does not block guarded sorting, hidden toggles, selection snapshots, or comparison.
If ownership or either advanced scratch setting changed, an old guarded revision returns `Changed`; applying the
transition and retrying succeeds. A flip after validation cannot change rows at that revision: rows never resample.
An unobserved ABA exposes no intermediate rows; an observed flip and flip back allocate separate revisions.

Entry mutations reconcile and publish scratch drift FIRST, then derive their own old/new coordinates using that
pinned projection. For `[b,c]` with hidden `a.cmdr-tmp-*`, ownership expiry plus removal of `b` publishes an add at
zero, count three (`r0→r1`), then a remove at one, count two (`r1→r2`). New scratch paths are sampled once when admitted;
the same decision supplies their diff and count. Removed candidates are forgotten. A projection-only change affecting
no currently shown rows still publishes an empty batch linking revisions (for example, a scratch dotfile with hidden
files off). Reconciliation has no ownership notifications and does not clone the whole listing on ordinary reads;
both visible-row caches remain valid across projection changes.

Wire payload: `{ listingId, batches: [{ fromSequence, sequence, totalCount, changes }] }`. Each batch is one old/new
row space; one multi-path removal is one batch, successive mutations are separate batches. `totalCount` is the final
visible count for that transition. A 50 ms window coalesces transport without flattening coordinate spaces. Events may
arrive out of order; clients chain batches by revision rather than treating an event's changes as one transition.

`resort_listing` accepts a final `expected_sequence: Option<u64>` and returns `ResortResult` with `sequence`,
`totalCount`, `newCursorIndex`, and `newSelectedIndices`. It refuses stale revisions before interpreting selected rows,
commits a revision even for identical ordering, and discards queued old-space batches under the cache write lock.
Already drained batches retain their older stamps, so the client can discard them. `set_listing_include_hidden` takes
the expected revision plus optional cursor and selection, remaps exact surviving identities from the OLD visibility
to the NEW visibility, and returns the same result. An unchanged setting does not allocate a revision.

`get_selection_snapshot(listing_id, include_hidden, selected_indices, expected_sequence)` consumes backend-space rows
under one reconciled read lock, returning `{ paths, fileCount, folderCount }`. Revision or visibility mismatch and
invalid rows return typed `ListingLookupError::Changed` (`type: "changed"`); a missing listing remains `Gone`.
This guards row-to-path consumption, not subsequent filesystem operations.

Ending a listing drops the queue under the cache write lock. The E2E `flush_all_watchers` helper also drains queues
through `flush_all_pending()` without waiting for the transport window.

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

**Where the menu offers tags.** Only on rows that are real OS paths (`PaneContextMenuFacts.can_tag`, the frontend's
`rowIsOsVisible`, the same reading `Share…` takes). Decision/Why: hiding an action that can't work beats explaining
after the click, and the write is an `xattr::set` through the path, so a phone, an ADB device, an SFTP or WebDAV server,
an archive's insides, or a `.git`-portal row takes the click and stores nothing. Keyed on the capability, ❌ never a
list of backends, so a new protocol-only backend (S3) hides them for free. A share Cmdr talks to directly over smb2
keeps them: its share stays mounted by macOS, its rows are `/Volumes/…` paths, and the xattr goes through that mount.
A macOS-mounted filesystem that can't store xattrs still shows them; only trying can tell. Reading tags
(`enrich_tags`) still runs everywhere, since an empty read is harmless.

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
`select!` arm in `stall::read_until_answered` that races the reads, and (as a child token) the backend's cooperative
cancel token via `Volume::list_directory_with_cancel`. One token means the "is it cancelled?" checks and the "wake up"
signal can't disagree, and `cancel_listing()` is one call. By the time the cancel arm runs, the backend has necessarily
already been told to stop: the same cancellation woke it.

The listing then emits `listing-cancelled` and RETURNS, dropping each read's `JoinHandle`. Dropping a `JoinHandle`
detaches the task; it does not cancel it. So the backend keeps running for exactly as long as it needs to reach its own
safe boundary, while the user sees an instant cancel. A stalled listing whose retry wins detaches its other read the
same way, after cancelling its child token so a backend that listens unwinds.

❌ Never `abort()` a read's task there. Abort drops the listing future at whatever await point it's sitting on. For MTP
that's mid-PTP-transaction: the device is left expecting bytes nobody will send, and it wedges until the user replugs
the phone; the guardrail is in `crates/cmdr-mtp/src/connection/CLAUDE.md`. MTP bails between per-handle `GetObjectInfo`
round trips, so cooperative cancel costs at most one round trip of latency.

Backends that ignore the token (local, in-memory, SMB today) run their listing to completion in the detached task.
That's not a regression: local listings run inside `spawn_blocking`, which `abort()` never interrupted either.

Pinned by `streaming_test::test_cancel_unwinds_the_listing_instead_of_aborting_it`, which drives a fake volume that
only ends when its token flips and fails if its future is dropped first.

## Stalled listings (`stall.rs`)

**The failure.** A pane navigating into an OS-mounted share whose server stopped answering (NFS or `smbfs`) showed a
spinner for as long as the kernel held the read, and `cmdr://state` showed the pane on the new path with
`totalFiles: 0`, no rows, and no error: indistinguishable from an empty folder. `LocalPosixVolume` reads inside
`spawn_blocking`, and `read_dir` on a silent mount blocks in the kernel with no timeout of ours anywhere above it. What
the stuck call answers when the server comes back differs by filesystem, so recovery can't count on either:

- **`smbfs`**: blocks for as long as the server is silent (116 s observed, with no sign of ending), then answers
  `ENOTCONN` the instant the server is back, and the kernel drops the mount (macOS 27.0, `docker pause` on a Samba
  fixture mounted with `mount_smbfs`, `ls` timed from a second shell, 2026-10-02).
- **NFS (hard mount)**: blocks until the server answers, then completes, or refuses with `EPERM` if it comes back
  with stricter exports (the NAS in the issue rewrote its exports as `secure` on restart). Reasoned from GitHub issue #305, not reproduced here.

**The watch.** `read_until_answered` races each read against `StallPolicy::stall_after` (8 s) of silence: no entry
count growth, since a local read stuck on one `stat` repeats the same number every progress tick. 8 s clears the
0.3–6 s a busy NAS holds a single request for (`listing_done_level`), so a slow answer never reads as a dead server. On
a stall the listing emits `listing-stalled` and keeps waiting; the frontend swaps the spinner for the "still waiting"
screen, and any later progress, complete, error, or cancelled event for the same listing replaces it. A read that
resumes producing entries clears the stall, and a later silence reports it again.

**After a stall, the listing answers for itself** until the volume does, so the pane recovers with no help:

- A STUCK read's refusal is stale (it spent the outage in the kernel, and the `ENOTCONN` above is exactly that), so the
  listing asks again at once.
- A PROMPT transient refusal (`ErrorCategory::Transient`, the same classification the error screen renders from) is
  retried after a backoff: 2 s doubling to 30 s.
- A PROMPT lasting refusal is the answer and ends the listing (a folder gone once the server is back walks the pane up
  as before). So does any refusal from a listing that never stalled: nothing changed for a healthy volume.
- On a `BackendKind::Local` volume (where reads block in the kernel), one fresh probe runs beside the stuck read once
  the first backoff passes, since a stuck read can stay stuck after the server is back, and the probe answers the
  moment it is. Connecting backends (SMB direct, SFTP, WebDAV, MTP, ADB) get no probe: their own session timeouts and
  the reconnect manager already turn silence into a typed answer, and the frontend's `live-retry` re-lists once the
  volume is live again.

The retries stop with the listing: cancel (the user navigating away, Esc, or Go back) wins every `select!`, and the
function's drop guard cancels the child token every read carries, so a read still waiting at the gate never starts.

**The thread bound.** Every read on a hung mount pins a blocking-pool thread until the kernel lets go, and the pool is
finite (`deadline::BlockingBudget`'s doc has the incident where it ran out). One listing holds at most two reads (the
stuck one and the probe). Across listings, `HungReads` keeps every volume's reads: once one is hung, at most
`MAX_READS_ON_A_HUNG_VOLUME` (4) may be in flight on that volume, and any further read waits at the gate as a future,
never a thread. That leaves a stalled listing's two reads plus a user's Retry room to land the moment the server is
back, and caps what mashing Retry or opening one folder after another can pin. A read's slot lives in its own task,
so it frees when the KERNEL lets go, however long after its listing moved on.

**What it's stalled on (`stalled_on.rs`).** The event carries `stalled_on: StalledOn` (`server` / `drive` /
`unknown`), which picks the screen's wording, so it says "server" or "drive" only when the mount proves it and keeps the
combined line otherwise. Classified once per listing, at its first stall:

- **Direct backends name themselves**: SMB direct, SFTP, and WebDAV are `server`; MTP and ADB are `unknown` (a USB
  cable for one, a cable or Wi-Fi for the other, so neither word fits both).
- **A filesystem path** (`Local`, and an archive or `.git` portal inside one) reads the mount it lies on off the kernel's
  table, ❌ never a `statfs` on the path, which blocks on the very mount that just stalled. macOS uses the
  `getfsstat(MNT_NOWAIT)` snapshot (`volumes::mount_type_and_source_for`), Linux `/proc/mounts`
  (`linux_mounts::mount_entry_for_path`). Network types on an explicit allowlist (`smbfs`, `nfs`, `afpfs`, `webdav`,
  `ftp`; `cifs`, `smb3`, `nfs4`, `fuse.sshfs`, `fuse.rclone`, `fuse.s3fs`, …) are `server`; a known local disk
  (`index_provider::mount_is_local_disk`) is `drive`; anything else (GVFS's `fuse.gvfsd-fuse`, which holds phones and
  shares alike, macFUSE, cloud clients' mounts, autofs, `9p`) is `unknown`.
- **The table lookup is lexical**, so a path that reads as a local disk is resolved through its symlinks first
  (`~/nas` → `/Volumes/nas`) on a blocking thread, bounded at 500 ms. A timeout means the probe hit the hung mount,
  which reads as `unknown`. That probe can pin one blocking thread per stalled listing for as long as the kernel holds
  it, outside the `HungReads` gate below; it only runs when the lexical answer was `drive`.
- Not followed: a symlink INSIDE a network share pointing back onto a local disk (still reads `server`).

**Decision: a gate that closes only on a hung volume, not a `BlockingBudget`.** A budget caps a family's reads always,
which would throttle the healthy concurrent listings of a busy pane pair, two tabs, and a refresh on the boot disk. The
gate costs nothing until a read on that volume has actually gone quiet. The bound is loose for the first 8 s (nobody
knows the volume is hung yet), which only a human mashing Escape and Enter can exploit.

**Decision: never a deadline that ENDS the listing.** Ending it would hand the pane an error while the server may be
seconds from answering, and the stuck read would keep its thread anyway. Waiting costs the same thread and lands on
its own.

Pinned by `stall_test` (a read that never answers stalls within the deadline; recovery from a stale `ENOTCONN`;
transient retries; a lasting refusal ends it; cancel stops retries; the per-volume bound, mutation-checked; a steady
read never stalls). The frontend half: `apps/desktop/src/lib/file-explorer/pane/DETAILS.md` § "A folder that stops
answering".

**Manual repro**, no NAS needed: run a private copy of the guest SMB fixture image
(`docker run -d --rm -p 127.0.0.1:<port>:445 smb-consumer-smb-consumer-guest`), `mount_smbfs -N
//guest@127.0.0.1:<port>/public <dir>`, open `<dir>` in a pane, `docker pause` the container, and open a subfolder:
the "still waiting" screen appears after 8 s. `docker unpause` and the pane lands, or walks up if the kernel dropped the
mount. Don't pause the shared fixture containers: other sessions' E2E runs use them.

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
