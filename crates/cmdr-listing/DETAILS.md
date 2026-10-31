# cmdr-listing details

Read this before any non-trivial work in the listing engine: editing, planning, reorganizing, or advising. `CLAUDE.md`
holds the must-knows; the data flow, decisions, and rationale live here. The seams have their own docs
(`src/host/DETAILS.md`), and so does Cmdr's side of them (`apps/desktop/src-tauri/src/listing_host/DETAILS.md`). For
profiling listing performance, see `docs/guides/benchmarking-file-loading.md`.

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

`listing-complete` is what commits the listing in the pane, so nothing slow may sit in front of it, and an observer's
`listing_opened` runs right before it. Cmdr's change watch therefore detaches its arm to the blocking pool
(`start_watching_detached`). Why arming is slow, what an inline arm cost, and the rules that keep it cheap:
`apps/desktop/src-tauri/src/file_system/DETAILS.md` § "Arming a listing watch is detached".

## Driving the engine without Cmdr

A host hands over the two seams nothing works without and takes the no-op answer for the rest:

```rust
let listings = Listings::builder(Arc::new(MyVolumes), Arc::new(MyEvents))
    .runtime(tokio::runtime::Handle::current()) // optional: else cmdr-fs's fallback runtime
    .index(Arc::new(MyIndex))                   // optional, like leases / locale / mounts
    .overlay(Arc::new(MyOverlay))               // optional lists, in registration order
    .observer(Arc::new(MyObserver))
    .build();
listings.start_orphan_reaper();
let id = ListingId::new("pane-1-nav-7");
listings.start(ListingRequest {
    id: id.clone(),
    volume_id: "root".into(),
    path: "/tmp".into(),
    include_hidden: false,
    sort_by: SortColumn::Name,
    sort_order: SortOrder::Ascending,
    dir_sort_mode: DirectorySortMode::LikeFiles,
});
// `MyEvents::finished` hears `ListingEnd::Complete`; then read rows by pane index
// (`file_range`, `file_at`, …), and close it:
listings.end(&id);
```

Change intake (`directory_changed`, `observe_children`, `refresh`) is how the host keeps an open listing current; a host
wires its storage backends' `cmdr_fs` `ListingHost` to those methods. `Listings::detached()` defaults all eight seams,
for tools and tests; the crate's own tests build a `testing::TestEngine` instead (§ "Tests own their engine").

## The overlay step

Every read that builds or replaces a listing runs ONE step between the volume's answer and the sort: `pipeline.rs`'s
`Inner::prepare`, the index enrich and then the overlays, which fold in rows a PANE sees that the volume doesn't hold.
It returns a `Prepared`, which nothing else can build, and `CachedListing::new` and `update_listing_entries` take
nothing else, so a read path that skips the overlays doesn't compile. Five paths go through it, each pinned by
`overlay_read_paths_test.rs`: the streaming read, the test-only sync start (`operations.rs`), a backend's `FullRefresh`
(`refresh.rs`'s `notify_full_refresh_locked`), a backend swap's re-read (`foreign_path::respell_listing`), and the
watcher's full re-read, also ⌘R (`Listings::refresh`, `reread.rs`). Its `std::fs` fallback, with no volume to decorate
through, keeps the count the listing had (`OverlayRows::Unchanged`). A device backend's `Replaced` hand-over is
synchronous and can't wait on the overlays' blocking gather, so it asks only their zero-I/O `applies_to`, over the
handed-over entries (`Inner::prepare_handed_over`): none applies (every device backend today) and it records zero
contributed rows; one applies and the arm drops the backend's entries and re-reads the directory as a `FullRefresh`
does.

Today's one contributor is the git portal, which puts the six virtual category rows into a repo's `.git/` listing.
Placement matters three ways:

- **After enrich**: a contributed row has no drive-index entry, so there is nothing to look up for it.
- **Before the sort**: the rows land wherever the pane's own sort puts them, with no ordering privilege of their own.
- **Before the cache insert**: the pane reads its rows out of the cache, so a row that isn't there isn't shown.

`CachedListing` records how many rows came from an overlay, and `try_get_authoritative_listing` declines any listing
carrying some. A decorated listing is a PANE view, never a picture of a directory a delete walker or a copy scan may
reuse. ❌ Never let one answer that oracle: the rows have no inode behind them, and a walker that meets one stops
mid-operation. The seam and the reasoning: `host/overlays.rs`, `volume/DETAILS.md` § "Architecture".

## Local listing progress

`listing-progress` is the only thing the user sees during a big folder's read: without it the pane sits on "Opening
folder..." from the first keystroke to `listing-read-complete`, however long the stat loop takes. `streaming.rs` builds
the callback and passes it down through `Volume::list_directory`, so a backend that ignores its `on_progress` argument
turns that state off for every folder it serves. Nothing about that fails loudly (the symptom is a UI state that stops
appearing), which is why `a_local_directory_read_emits_progress_events` in `streaming_test.rs` drives the whole chain
against a real `LocalPosixVolume`: the rest of that suite runs on `InMemoryVolume` and can't see a backend drop it.

How `LocalPosixVolume` feeds the callback from a `spawn_blocking` stat loop: `crates/cmdr-local/DETAILS.md` § "The local
reader".

## Caching

The record is `cached_listing.rs`; the lookups and change intake are `caching.rs`, the single-entry patches `mutate.rs`,
the full-refresh turnstile `refresh.rs`, the fresh-listing oracle `authoritative.rs`, and the six-hour backstop that
reclaims a leaked listing `orphan_reaper.rs`.

- **The engine** (`engine/mod.rs`): `Listings`, a cheap-to-clone `Arc<Inner>`, built by
  `Listings::builder(volumes, events)` (the other seams optional) or `Listings::detached()`. `Inner` (defined in
  `lib.rs`) holds the seams the host handed over and every piece of engine state: the cache, the in-flight reads' cancel
  tokens, the pending diffs, the refresh turnstiles, the hung-read registry, the collator per locale, the epoch
  `last_accessed_ms` counts from, and the scratch settings. Two engines share nothing. Each module adds its operations
  as an `impl Inner` block. The app builds one in `listing_host::install` and hands it out as
  `listing_host::listings()`.

  **Decision:** `Inner` lives at the crate root, not in `engine/`. **Why:** every module implements its part of `Inner`
  and `Inner` holds every module's state, so wherever it's defined, that module and the implementers point at each
  other. At the root, the root is their parent, which `module-cycles` reads as one hub; any sibling home makes a
  16-module tangle. The same reason keeps the layout flat: grouping into `cache/` and `read/` puts `cached_listing` and
  `pipeline` in sibling groups that use each other.

- **The public API** is `lib.rs`'s `pub use` list: `Listings`' methods (in `engine/`, one file per caller group:
  `open.rs`, `rows.rs`, `settings.rs`, `intake.rs`, `oracle.rs`, `housekeeping.rs`, `reread.rs`) and the types they take
  and return. Every submodule but `host` is private, so the compiler keeps callers on that list. A listing id crosses it
  as a `ListingId`; only IPC sees a `String`. Methods return owned data; `with_rows` is the one closure the engine runs
  under its cache lock, and it must never call back into the engine. A scratch-visibility or collator read happens
  BEFORE the cache lock and travels down as a value; `CachedListing` keeps no reference to its engine (it copies the
  epoch).
- **The cache** (`Inner::cache`): `RwLock<HashMap<String, CachedListing>>`, keyed by `listing_id` (UUID per navigation).
- **`CachedListing`**:
  `{ volume_id, path, entries, visible_rows, path_index, sort_by, sort_order, directory_sort_mode, supported_dates, sequence, created_at, last_accessed_ms, overlay_rows }`.
  The four sort fields are private: `sort_by` is the REQUESTED column and `effective()` derives the order the rows are
  in (§ "Requested and effective sort"). `entries` is private: `entries()` reads, `entries_mut()` / `set_entries()`
  change it and drop BOTH maps on the way, `rows(include_hidden)` is what every read accessor asks, and `index_of_path`
  / `indices_of_paths` are what every by-path caller asks. See § "Row numbers" and § "Entries by path".
- **Focused-pane reads**: `Listings::cached_entries(volume_id, path)` clones the newest matching cached listing without
  requiring watcher coverage. Agent reads use it for the already-open pane, including SMB and MTP, and never start a new
  filesystem listing.
- **`Listings::summaries()`**: lightweight summary of every active listing (id, volume, path, entry count, age). Used by
  `cmdr://state` so error reports surface orphan listings (started but not bound to a pane).
- **Concurrency**: multiple listings coexist (different panes, rapid navigation), each with a unique ID.

### Lifecycle

The method names below are `Listings`'; the app's IPC commands of the old names (`list_directory_start_streaming`,
`get_file_range`, …) are pass-throughs to them.

1. `start(ListingRequest)` registers the cancel token, spawns the read on the host runtime, and returns at once.
2. The read lists the directory, runs the pipeline (§ "The overlay step"), sorts, stores in the cache, tells the
   observers, and reports `finished(Complete)`.
3. The pane reads visible rows on demand (`file_range`), batch-resolves names to rows (`find_file_indices`, selection
   adjustment during operations), and batch-reads selections only under the revision guard (`selection_snapshot` /
   `selection_entries` / `selection_names`: transfer dialogs, delete, drag, clipboard, the context menu, Services,
   multi-rename). There's no unchecked read by row number on purpose: a watcher insert renumbers rows, and an old
   selection read against the new rows names the neighbor.
4. `end(id)` removes the listing from the cache and drops its pending diffs, then tells the observers.

Both ends tell the observers (`host/observers.rs`), the seam for subsystems that keep something alive while a pane is
showing a directory, outside every engine lock and in registration order (Cmdr's order and why:
`apps/desktop/src-tauri/src/listing_host/DETAILS.md` § "The observers"). ❗ The close call comes AFTER the cache
removal: an observer's own detached arm reconciles against listing-cache membership, so releasing while the entry is
still there lets a racing arm re-take what was just released, and a change event landing between the removal and a
watch's disarm finds no listing to patch. An observer that keeps a record per open listing asks
`OpenedListing::still_open` before keeping one: a close can race the open and reach every observer first.

### Backstop reaper

`Listings::start_orphan_reaper` (the app calls it in its `setup()`) sweeps every `REAPER_SWEEP_INTERVAL` (30 min) and
tears down any listing idle past `ORPHAN_IDLE_WINDOW` (6 h) via the same `list_directory_end` path, so a leaked listing
(close IPC never delivered) can't pin its entry vector and OS watcher for the whole session. Its loop holds the engine
weakly and ends once the last handle is dropped. Pure, clock-injectable seam: `orphan_ids(now_ms, window_ms, …)` and
`reap_orphaned_listings_at_for(now_ms, window_ms, only)`. Mirrors the search index's idle/backstop timers and the file
viewer's window-`Destroyed` net.

It keys on `last_accessed_ms`, NOT `created_at`. `created_at` is stamped once and never refreshed, so an age-based
reaper keyed on it would evict a pane open all session. `last_accessed_ms` (an `AtomicU64` of ms since the engine's
epoch) is bumped by every operation that proves the listing still backs a live pane: the read accessors
(`get_file_range`, `get_file_at`, `get_file_beside`, `get_listing_stats`, the index/path/batch lookups),
`resort_listing`, and every watcher/notify cache patch (`insert_entry_sorted` / `remove_entries_by_paths` /
`remove_entry_by_name` / `update_entry_sorted` / `update_listing_entries`). `AtomicU64` so read accessors stamp it
lock-free under a shared cache read lock. The 6 h window is deliberately generous: we'd rather never evict a live
listing than aggressively reclaim. `refresh_listing_index_sizes` intentionally does NOT touch it: it's driven by
background indexing, not user/FS activity, so touching there could keep a truly-orphaned listing alive indefinitely.

**The pane heartbeat is what proves a listing live**, not access. A pane left on a quiet folder overnight makes no reads
and gets no FS events, so access alone let the reaper take an on-screen `~/Downloads` listing after six idle hours: the
pane kept stale rows, its watcher was gone (new downloads never appeared), and every F3–F6 failed on the missing
listing. Every 30 min the frontend's `file-explorer/pane/listing-liveness.ts` names each LANDED listing through
`keep_listings_alive`, which touches the ones cached and returns the ones it no longer holds. A leaked listing stops
being named and is reaped as before.

**A lookup on a missing listing is a typed `ListingLookupError::Gone { listing_id }`** (every accessor in
`operations.rs`, and their commands). It's the accessors' only failure: they take the cache lock with `*_ignore_poison`.
The frontend funnels every such refusal (`#lib/tauri-commands` `listing-gone.ts`), and every id the heartbeat returns,
to the liveness registry, and the owning pane re-lists the same folder with the cursor on the same entry. A pane
registers its listing only once it has landed, so a read racing a navigation (which also answers `Gone`) never triggers
a re-list. The clipboard and drag commands still flatten it into their `String` errors; the pane recovers through its
next read or heartbeat.

#### Tests own their engine

Every test in this crate builds its own engine (`testing/`): `TestEngine` is a `Listings` over one fake per seam it
reads back (`FakeVolumes`, `RecordingEvents`, `RecordingLeases`, a fixed `en` locale, plus `FakeMountTable`,
`StaticOverlay`, and `RecordingObserver` on request), and `TestListing::insert(&engine, tag)` puts a listing straight
into its cache. Two engines share nothing, so a test asserts on engine-wide state (every listing on a path, an orphan
sweep, scratch visibility) without seeing anyone else's, and needs no lock to flip a setting. A test reaches engine
internals through `engine.inner`, which every module in the crate can see.

The `testing` feature makes the same fixtures `pub` for the host's tests. Cmdr's half, which shares one engine and so
keeps unique ids: `apps/desktop/src-tauri/src/listing_host/DETAILS.md` § "Tests".

**Decision:** a test builds the engine it asserts on, rather than isolating tests on one shared cache. **Why:** a shared
cache needed unique ids, scoped orphan sweeps, a teardown guard, and a process-wide lock serializing every
scratch-visibility test, and a test asserting a cache-wide count still depended on what else was running. A handle
threaded through its callers needs none of it.

## Row numbers (visible_rows.rs)

A pane numbers its rows over what it is SHOWING, so row 7 is the seventh visible entry and not `entries[7]`. Two things
leave an entry out: `FileEntry::is_hidden` (when `include_hidden` is off) and scratch a running operation owns
(`cmdr_fs::staging`).

**Answering "which entry is row N" by walking and counting is what wedged the app.** The MCP pane mirror fetches ~100
rows per sync, each fetch was one full walk, and at the bottom of a 74,144-entry directory that came to ~7.4 M predicate
evaluations per index event on the main thread — IPC stopped being answered at all. Evidence and the before/after:
`docs/notes/listing-row-fetch-quadratic-2026-08-22.md`.

**Decision: materialize the row map once per `(listing, include_hidden)`, and split it in two.** `settled` is a
`Vec<u32>` of entry indices nothing can hide any more; `candidates` holds the scratch-NAMED entries with the count of
settled rows ahead of each. Rows merge candidates using the listing's committed exact-path `ScratchProjection`, never
live settings or ownership. A lookup is an array index plus a binary search over a list that is almost always empty.
Reads check only those candidates for drift before using the map; publication is described in § "Diff event coalescing".

**Why the split rather than an invalidation hook.** The `is_hidden` half is stable, but the scratch half is not: an
operation settling un-hides its leftover with no change to the listing and nothing to notify anyone, because the
ownership signal is a `Weak` that simply stops upgrading (`cmdr_fs::staging`). Hunting for every event that could flip
it is exactly the kind of invariant that rots; re-asking about the few names it could apply to cannot. What makes it
sound is that `visible_rows::is_hidden_from_listings` is GATED on the pure `could_be_hidden_from_listings`, so a name
outside that set is settled by construction.

**Two slots, one per `include_hidden`**, so a pane toggling hidden files — or two readers disagreeing about the flag
mid-toggle — can never be handed the map built for the other answer.

**Validity needs no version counter.** Every accessor holds the cache READ lock, and every mutation needs the WRITE
lock, so `entries` cannot move under a reader. `entries_mut()` drops both maps as it hands the vector out, which is why
no mutation path has to remember anything.

**What it fixed beyond the wedge**, all three found by making `entries` private and following the compile errors:
type-to-jump filtered dotfiles but not scratch, so its index space and `getFileAt`'s disagreed and the cursor landed a
row off during a copy; Brief-mode column widths were sized around names the pane never draws; and the streaming
listing's `totalCount` counted dotfiles only.

**Cost.** One `Vec<u32>` per listing per `include_hidden` actually used: ~300 KB against a 74k listing whose entries are
themselves ~15 MB.

**The one case that got slower**, said plainly: reading row 0 right after a mutation used to short-circuit after one
entry and now rebuilds the whole map. It doesn't matter in practice, because the reads that accompany it
(`get_listing_stats` and mutation publication counts) share that one pass — but a future caller that reads a single
shallow row per mutation and nothing else is the shape to watch.

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
  `includeHidden` values to match the committed settings. Scratch drift is reconciled before taking that read lock, so
  stable shown scratch and stable owned hidden temps are valid settled states. The frontend also requires its applied
  revisions to match; a queued event is safe because its revision was already committed. See § "Diff event coalescing"
  for publication and guarded consumption.
- **Names match as the Mac does**: the exact spelling first, else a name that folds to the same key
  (`cmdr_fs::name_fold`, case and Unicode form). A folded match counts only when the key is unique on BOTH sides, so the
  two directions always agree: `Report` and `report` (a case-sensitive volume) against `REPORT` pair nothing.
- **Runs off the IPC thread with a 10 s deadline** (`blocking_typed_result_with_timeout`), answering a typed
  `CompareDirectoriesError` (`gone` / `timedOut` / `internal`).
- **Two seconds apart is the same time** (`SAME_TIME_TOLERANCE_SECS`): FAT and many shares store time in 2 s steps. An
  unknown time or size never marks a copy; only a difference we can see does.
- No content comparison: like TC's ⇧F2, it reads metadata only. Byte comparison belongs to Synchronize directories.

## Quick filter (name_filter.rs, quick_filter.rs)

The pane's "type to narrow" mode (Total Commander's quick filter). The pattern lives on the `CachedListing`
(`set_name_filter`) and is one more input to the row predicate (`visible_rows::shows`), so it is NOT a second filter
point: counts, ranges, selection, type-to-jump, and `directory-diff` rows all speak the filtered row space.
`name_filter.rs` is the pure matcher (`NameFilter`), which the row map and the diff read; `quick_filter.rs` is the pane
setting that swaps it under the cache lock. Split so the matcher depends on nothing in the cache, which is what keeps
`cached_listing` → `visible_rows` → `name_filter` from cycling back through `operations`.

- **Decision: the filter is the listing's, not a per-call argument like `include_hidden`.** Why: every pane-index IPC
  already carries `include_hidden`; threading a pattern through all of them would touch every caller for no gain, since
  only the pane showing the listing ever filters it. The cost: the filter is not a slot key of `VisibleRowsCache`, so a
  change drops both slots.
- **`set_listing_name_filter` swaps the row space under ONE write lock** and answers with the new count plus where the
  cursor's file and the selected files landed (the `resort_listing` shape). A selected file the filter hides drops out
  of the selection, so no operation acts on a row the user can't see. A change drops the queued diffs, like a hidden
  toggle, and answers the diff `sequence` the new row space starts at (bumped under the same lock).
- **Matching cost**: the wildcard-free path folds each name once (`fold_name` allocates only when folding changes it)
  and does one `contains`; a wildcard pattern walks chars with backtracking only to the last `*`, so no recursion and no
  blowup on `*a*a*a*`. Measured by the review on a 50k-name folder: fast enough to type into.
- **Typing narrows down to the last match, never past it.** A growing pattern is sent with `refuse_empty`; one that
  matches no entry is refused under the same lock (`accepted: false`, old filter kept) and the frontend drops the
  keystroke. The check walks every entry, not the current rows: an edited pattern needn't narrow the old one.
- Matching: substring anywhere in the name, folded by `cmdr_fs::name_fold` (case and Unicode form), `*` / `?` as
  wildcards with an implied `*` on both ends. A new listing starts unfiltered; the frontend side is
  `apps/desktop/src/lib/file-explorer/pane/DETAILS.md` § Quick filter.

### The quick filter and in-flight diffs

A filter change is a committed visible revision, using the same `sequence` identity as sort, hidden visibility, scratch
reconciliation, selection snapshots, and comparison. There is no independent filter epoch.

- The guarded setter reconciles scratch drift, then checks the caller's expected revision and hidden setting BEFORE
  interpreting selection indices. A typed `Changed` refusal leaves the filter untouched; the pane drains buffered
  transitions and retries with fresh indices.
- A successful change advances the revision and drops superseded queued transitions under the cache write lock (cache
  then queue). A new-space watcher mutation cannot enqueue until that lock is released, so the drop cannot erase it. An
  already-drained old batch is harmless: its revision precedes the response barrier.
- Watcher and overlay replacements sort, compare committed projected rows, replace, and publish under that same write
  lock. An async directory read crossing a filter change therefore derives its diff in the current row space, without an
  externally computed diff or an enqueue after unlocking.
- Filter requests use the pane's `pane-row-state.ts` reconfiguration gate. Pending diffs wait for the response;
  synchronous installation establishes the response revision before buffered batches drain. Reconfiguration also
  invalidates old async selection continuations and gates compare and destructive-operation snapshots.

`name_filter_test.rs` covers old-space pending drops, fresh publication after the setter unlocks, replacements crossing
a filter change, guarded selection refusal, and scratch drift before filtering. The post-unlock test injects a real
watcher mutation and holds the flush timer; no sleeps or scheduler timing are needed.

## Diffs speak the pane's rows

A `directory-diff` index is a row of the pane showing the listing, the same space `get_file_range` reads, ❌ never an
index into `entries`. The pane's rows are `CachedListing::pane_rows()`: the row map at the listing's own
`include_hidden`, recorded at `list_directory_start_streaming` and updated by `set_listing_include_hidden`, which the
pane calls first thing in `hidden-files-resync.ts` (after every load and every toggle). It's per listing, and each pane
holds its own listing, so two panes on one folder each get their own rows.

**The rule: the cache takes every change; the pane hears only the rows it shows.** An entry the pane shows on neither
side of a change (a dotfile, `~/Library`'s `UF_HIDDEN` flag, in-flight scratch) updates the cache and emits nothing, so
turning hidden files on later shows fresh data at once. One it shows on one side only arrives as a remove (it turned
hidden, or went) or an add (it turned visible, or came). One it shows on both is a modify or a move. A change with no
visible part queues nothing, so no event and no sequence bump.

**Why.** Two costs of reporting entry indices. The idle one: on a pane on `~`, dotfile writes produce a diff about every
10 s, in bursts, and each one cost the webview a count, range, stats, and column-width refetch with nothing visible to
change, even with hidden files off (most of WebContent's idle CPU after the index-size work, GitHub #92). The
correctness one: the pane reads the index as a row, so with hidden entries sorted above a change, the cursor and
selection slid a row off on every add or remove (`pane_diff_test::a_removal_reports_the_row_it_left` reported entry 3
for row 1).

**How.**

- The single-entry helpers (`insert_entry_sorted`, `remove_entries_by_paths`, `remove_entry_by_name`,
  `update_entry_sorted`) return `PaneRows { before, after }`, read off the row map as it stood BEFORE their own patch
  and under the same write lock (`VisibleRows::rows_before` / `row_of_entry`, two binary searches). ❗ Reading after the
  patch would rebuild the map per patch, once per add in a burst. `DiffChange::for_pane` turns the pair into a change or
  none.
- Batch re-reads (`publish_replacement`, `Listings::refresh`) call `update_listing_entries`, which diffs committed
  projected rows through `CachedListing::replace_entries` under the cache write lock.
  `compute_diff(old, new, include_hidden, name_filter, visibility)` is a test-only helper.
- `CachedListing::shows` combines the filter with captured scratch decisions and hidden visibility, matching its row
  map.
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
cache's WRITE lock, once per path. Measured on a release build (M1 Max, 2026-08-22, synthetic entries with a
63-character mean path): one 500-path chunk costs 20 ms at 20,000 entries, 64 ms at 75,000, and 418 ms at 300,000 —
times `entries / 500` chunks to cover the directory, so a 300,000-entry listing spent over four minutes of write-locked
walking on tags nobody asked to wait for. It is the "just opened" step in the release curve
(`docs/notes/listing-wedge-impact-2026-08-22.md` § 2).

**Decision: materialize `(path hash, entry index)` pairs, sorted by hash.** A lookup hashes once, binary-searches, and
compares the real path of the entries whose hash matches. Same build-once-and-index shape as the row map, and the same
validity argument: `entries_mut()` drops it, and every accessor holds the cache lock, so no version counter is needed.

**Why hashes and not a `HashMap<String, usize>`.** Twelve bytes per entry against 100+ for a map that owns a second copy
of every path: 3.6 MB against a 300,000-entry listing whose entries are themselves ~65 MB, where the map would add ~30
MB. It also builds in one sequential pass plus an integer sort, so its cost doesn't depend on the pane's sort order,
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
(`PathIndexCache::resolve_one`) takes ONE path, for `carry_forward_deferred`, `has_entry`, `update_entry_sorted`, and
`insert_entry_sorted`'s duplicate guard. ❗ The single-path form rides a map that already exists and **never builds
one**: one lookup is far under `BUILD_FROM_BATCH_SIZE`, so a modify on an untouched 300,000-entry listing must not pay
~20 ms for a map it uses once and (mutating) drops on the way out. What it buys is the sweep's map, so while enrichment
is walking a big directory every watcher event landing in it is a hash rather than a walk.

❗ **A mutating caller resolves BEFORE it takes `entries_mut`.** That is the whole reason `update_entry_sorted` and the
removals can reach a map at all: `entries_mut` drops both maps as it hands the vector out, so a lookup after it can only
walk. The cache write lock is held across both, so the index is still true when the mutation lands, and the invariant is
untouched — `path_index_test::a_mutation_still_drops_the_map_it_rode` pins that they still drop it. One side effect
worth having: a modify or removal that takes no row leaves both maps standing rather than dropping them for a row it
never touched, which matters because an add-only watcher event calls the removal batch with an empty path list.

**The watcher's removals were the second quadratic**, and the only one of these callers with a measured win rather than
a latent one. The incremental path (`Listings::observe_children`) once resolved each removal's index with its own full
walk (the diff needs the PRE-removal index) and then walked again inside the removal itself, and one coalesced watcher
event carries up to 500 paths — a directory emptied, a `git checkout` across a big tree, an unpack over a folder.
Measured by the counting probe: a 500-path removal from a 20,000-entry listing examined **9,981,000 entries**, and now
examines **20,500** (one walk to build the map, plus one lookup per path). `remove_entries_by_paths` is now the only
by-path removal, since its caller is always a batch. Resolving and removing under ONE write lock also makes the emitted
indices true: the two-lock shape it replaced let another writer move a row between the lookup and the removal, and the
`directory-diff` would have named a row that had shifted.

**What is still linear per removal**: `Vec::remove` per doomed row, so dropping `k` rows from an `n`-entry listing
memmoves about `k × n / 2` entries. The lookup fix doesn't touch it, and the incremental path's 500-event cap bounds it.
The one-pass rebuild that would fix it needs a transient second copy of `entries` (~65 MB at 300,000 rows), which is not
a trade to take without measuring first.

**What is still O(entries), by path**: nothing. The single-path callers walk only on a listing that has no map yet,
which is what the threshold deliberately buys.

## Sorting

`sorting.rs` holds the comparator every list in the app is ordered by. `entry_comparator(collator, EffectiveSort)` is
generic over the `SortableEntry` trait, which names the eight fields ordering reads: name, is_directory, size,
modified_at, created_at, added_at, and the two recursive-size fields the Size column's directory rule needs.

Two implementors. `FileEntry` answers from its own fields. `commands::search::SearchSortRow` is a search-results pane's
row, and it answers `None` for the dates and recursive size a search result doesn't carry. Its dates are never read for
ordering (search resolves as a Modified-only volume, below); under Size its directories are all unknown and sort by name
among themselves.

### Requested and effective sort

A pane REQUESTS a column; the rows are in the EFFECTIVE one. They differ only for a date the listed volume doesn't
record (`Volume::supported_dates`, the matrix in `crates/cmdr-fs/DETAILS.md` § "Which dates each volume lists"): a pane
asking for Added on an smbfs mount gets Modified order.

- **Decision: `EffectiveSort` is a type whose only constructor is
  `EffectiveSort::resolve(requested, order, dir_mode, supported)`, and `entry_comparator` and `sort_entries` take
  nothing else.** Why: the fallback rule lives in ONE function, and a sort site that forgets it doesn't compile. Rule:
  Created or Added that `supported` lacks becomes Modified; every other column, the order, and the directory mode stay
  as asked.
- **Decision: `CachedListing` stores the requested column plus the reading volume's `SupportedDates`, and `effective()`
  derives the column.** Why: no stored effective column can drift from its inputs, and nothing holding a resolved column
  can hand it back as the next request or persist it. Only `CachedListing` holds sort state (the cache lookups return
  ids, never a sort).
- **The dates travel on `Prepared`** (`pipeline::ListedDates`). `prepare(ReadBy::Volume { .. })` asks the RESOLVED
  volume (a routed archive answers for itself), so the first read and every re-read refresh them, including the respell
  after a backend swap. `ReadBy::StdFs` and a backend's handed-over entries carry `Unchanged` and keep the listing's.
  `update_listing_entries` writes them BEFORE it sorts; when that moves the effective column, its `DirectoryDiffBatch`
  carries `effective_sort_by: Some(column)` and publishes even with no row changed, or a one-row folder never hears that
  its displayed date moved. Batches aren't merged, so the last `Some` a pane applies wins.
- **Every answer names the effective column**: `ListingEnd::Complete` (`listing-complete`), `ResortResult` (resort AND
  the hidden-files toggle), the diff batch above, and `sort_search_results`, which resolves with `SupportedDates::NONE`
  since the drive index stores neither date. The frontend never computes it.
- **What re-places a row**: `sort_key_changed(old, new, &EffectiveSort)`, beside `compare_by_column`, reads the same
  accessors the comparator reads for the effective column, plus kind and name (Size includes a directory's recursive
  size and its completeness). A false positive costs a harmless re-place; a false negative is a misplaced row.
  Separately, `diff::is_entry_modified` counts every date whatever the sort, since the row's tooltip shows them all.

**Why a trait and not a conversion.** Building a fabricated `FileEntry` per search row would be a second place where the
mapping from "a row" to "what orders it" is decided, and that mapping is the thing that must not drift. The trait makes
the shared fields the contract and the generic monomorphizes, so the listing's hot path pays nothing.

**`DirectorySortMode` decides whether directories lead.** `LikeFiles` and `AlwaysByName` put them first (by the column,
or A→Z by name whatever the direction, except on the Name column itself), and `MixedWithFiles` ("Show folders first"
off, #291) drops that step: `compare_mixed` ranks a directory among the files by the same column. Size is the one column
where the two kinds read different fields (a directory's `known_dir_size`, a file's `size`), so the mixed sort uses one
rule for both, an unknown size LAST whatever the order, to stay transitive: per-kind rules (a file's unknown first, a
directory's last) would cycle once mixed. The mode arrives per listing from the frontend, which folds its two settings
into it (`apps/desktop/src/lib/file-explorer/DETAILS.md` § Sorting), and every sort path reads it off the
`CachedListing`, so the watcher's re-sorts, archive panes, and every volume follow it.

`sort_search_results` (`commands/search.rs`) is the frontend's way in: it answers with the input indices in sorted order
(plus the effective column), and the caller re-orders the rows it already holds. The frontend deliberately has NO
comparator of its own; the snapshot store's sort round-trips through this command.
`apps/desktop/src/lib/search/DETAILS.md` § "The snapshot pane's row order".

### Collating names

Names rank by `cmdr-collation`'s `NameCollator`, built for the locale the host's `ReadingLocale` seam reports (Cmdr's
answer: `intl::active_locale()`). Why a collator and not code points, the options it's built with, the raw-bytes
tiebreak both readings end with, the `unstable` ICU feature, and ❌ never persisting a `NameKey`:
`crates/cmdr-collation/DETAILS.md`. This crate re-exports `NameCollator`, so a host names it as
`cmdr_listing::NameCollator`.

**One order, two readings.** `NameCollator::compare` collates a pair live; `NameCollator::key` serializes one name's
weights to bytes that sort the same way. The bulk `sort_entries` builds a key per row and sorts on those, so a 100k
listing collates 100k times instead of ~1.7M; the incremental `insert_entry_sorted` compares live, because
`partition_point` only probes ~17 rows to place one entry. `SortableEntry::compare_name` is where the two meet: the
default asks the collator, and the private `Keyed` wrapper overrides it to read prebuilt keys.

Prebuilt keys are what keep the collator affordable: collating live on every comparison measured ~4x SLOWER than the
code-point comparator it replaced, and keying once per row lands ahead of it instead. On 100k synthetic names, sorting
by Name, release build, M-series laptop, 2026-09-08: **105 ms, against 228 ms** for the previous `to_lowercase` +
`alphanumeric-sort` comparator on the same rows. The new figure covers the whole of `sort_entries` (key building, the
index sort, and the permutation). `collation_test::the_bulk_sort_and_the_live_comparator_agree_on_unicode_names` and
`cmdr-collation`'s `tests::a_key_ranks_names_the_way_compare_does` pin that they never diverge.

Sorting is covered by four suites, split along the questions they answer rather than by size: `sorting_test` (which
column, which direction, and the edge cases), `sorting_dir_mode_test` (`DirectorySortMode`, so how a directory ranks
against the files beside it, including the honest-size coverage flags), `sorting_effective_test` (the fallback rule, the
Added column, and `sort_key_changed`), and `collation_test` (the order names themselves come out in). A whole listing in
a fallback is `effective_sort_test`; `update_entry_sorted` is `caching_update_test`. `sorting_test_support` holds the
two builders more than one of them needs; a builder used by one suite stays in that suite.

The fixture each suite needs is what marks the seam. `collation_test` fixes the reading language per engine
(`testing::FixedLocale`), which is why case-insensitive ordering is tested THERE and not beside the other column tests:
case folding is a collation question, so `to_lowercase` would answer it wrongly (`crates/cmdr-collation/DETAILS.md` §
"The options"). `sorting_dir_mode_test` owns the honest-size directory builder. Each file's header names its two
neighbours, so the boundary is legible from whichever one you open first.

**The tiebreak keeps diffs honest.** Without the raw-bytes tiebreak's total order, the watcher's re-read could order the
NFC and NFD spellings of one name either way between two passes, and `compute_diff` would report a
`DiffChangeType::Move` for a row that never moved.

**Locale changes are pulled, not pushed.** `Inner::collator` caches one collator per locale tag and re-asks the host's
`ReadingLocale` on each call, so the next sort after an OS language switch uses the new language. Listings already
sorted keep their order until something re-reads them, exactly as they do when the sort column changes.

`sort_entries` sorts indices and then permutes the rows in place by cycle-following, rather than building a second
`Vec<FileEntry>`: a row is large enough that cloning 100k of them would cost more than the sort. Note the direction flip
before `apply_permutation` — `order[i]` names the row that belongs AT `i`, and the applier wants where the row currently
at `i` GOES. Applying the un-inverted permutation scrambles the whole listing without failing anything loudly, so
`sorting::tests::apply_permutation_moves_each_row_to_its_destination` covers each cycle shape.

## The public surface is capped

`index-crate-isolation` holds this crate to 38 root promises, 47 `Listings` methods, one public module (`host`), and 20
public items inside it. The numbers are exactly what the crate exposes, with no headroom, so an addition has to be
argued for. Everything counted exists for a named caller:

- **The handle and its vocabulary** (root): `Listings`, `ListingsBuilder` (the constructors, `builder` and `detached`,
  are two of the 47 methods), `ListingId`, `ListingRequest`, and the `host` module.
- **Named arguments** (root): `PaneMarks` (a pane's cursor and selection, for `resort`, `set_include_hidden`, and
  `set_name_filter_guarded`), `RowSpan` (`file_range`'s start and count), and `OnNoMatch` (the quick filter's refusal).
  Each replaces a confusable positional `usize` or `bool` pair; the IPC commands keep their wire params and build them
  at the boundary, so `bindings.ts` doesn't move.
- **A pane opening, reading, and closing a listing** (the IPC pass-throughs in `commands/file_system/listing.rs`): the
  `start` / `cancel` / `end` / `keep_alive` methods, the 13 row-space reads (`file_range` through `with_rows`, plus
  `compare_directories` and `fuzzy_find_first_match`), the five pane settings (`resort`, `set_include_hidden`,
  `set_name_filter_guarded`, `scratch_visibility`, `set_scratch_visibility`), and every type they answer in:
  `VisibleRows`, `ListingStats`, `ResortResult`, `RowBeside`, `SelectionSnapshot`, `NameFilterResult`,
  `ListingLookupError`, `FuzzyJumpError`, `CompareDirectoriesMode` / `Error` / `Result`, `StalledOn`.
- **Change intake** (the app's composer, the watcher, git wiring, the index-size worker): `directory_changed`,
  `observe_children`, `refresh` / `begin_read` / `refresh_from_entries` with `RefreshOutcome` and `ReadTicket`,
  `refresh_archive_listings`, `respell_on_volume`, `apply_row_facts` with `RowFacts`, `update_index_sizes` /
  `refresh_index_sizes` with `IndexSizes`, and the diff vocabulary the events seam carries (`DirectoryDiffBatch`,
  `DiffChange`, `DiffChangeType`). `RowFacts` (the 38th root promise) folds the deferred visible-row passes into one
  method: Finder tags and the packages macOS reports share `apply_row_facts` instead of a method each, so the next such
  pass is a variant, not a 48th method.
- **Lookups and the fresh-listing oracle** (write-operation walkers, MCP, multi-rename, the index-size worker, the NFS
  poller): the ten oracle methods (`authoritative_listing` through `summaries`) and `ListingSummary`.
  `open_listing_paths` is the NFS poller's: the directories one volume's panes show, a typed question rather than a read
  of the `summaries` triage snapshot.
- **Sorting outside a listing** (search results, the local reader): `SortColumn`, `SortOrder`, `DirectorySortMode`,
  `EffectiveSort`, `SortableEntry`, `entry_comparator`, `sort_entries`, `NameCollator`, and `Listings::collator`.
  `EffectiveSort` is the one input both sort functions take, and `resolve` its only constructor, so a caller outside a
  listing can't sort by a date without stating which dates its rows carry.
- **Housekeeping**: `start_orphan_reaper`, `flush_pending_diffs` (the `playwright-e2e` feature only), and
  `stored_spellings` (a Finder drop's foreign spelling, resolved once), and `entries_as_stored` (a typed folder path
  opened the way a pane opens it, for Ask Cmdr's live `list_dir`, which keeps no listing).
- **The seams** (`host`, the 20 items): the eight traits, the six `No*` defaults (the two list seams need none), and the
  values they exchange (`ListingEnd`, `OpenedListing`, `ForegroundHold`, `MountFacts`, `Unregistered`), plus
  `ScratchVisibility`. The seam files themselves are `pub(crate)`; a host names a seam only as `cmdr_listing::host::X`.

`testing` is gated behind the `testing` feature and counts apart: it's the host's test surface, never a promise to a
shipped build.

## Decisions

- **Streaming with a background task, not chunked IPC**: chunked needs multiple IPC calls and complex state tracking.
  Streaming spawns a task on the host runtime and reports through `ListingEvents`; the frontend stays responsive (Tab
  works, ESC cancels).
- **Cancellation via `AtomicBool` checked per-entry**: network folders iterate slowly (seconds per entry); a per-entry
  check keeps ESC responsive (cancel within ~100 ms).
- **Three-stage progress (opening → progress → read-complete → complete)**: `listing-opening` (about to start slow I/O),
  `listing-progress` (loaded N, every 200 ms via `list_directory_core_with_progress`), `listing-read-complete` (all
  read, sorting now), `listing-complete` (ready to render).
- **Sort after read, before caching**: the frontend expects sorted order, and the sort runs in the background task after
  all entries are collected. Cost is ~105 ms per 100k entries (§ "Collating names"), well under the I/O it follows.
- **Enrichment at cache-write time, not on `get_file_range`**: every path that stores entries (streaming, watcher
  update, re-sort) enriches first. Index freshness is event-driven: the app's index-size worker
  (`apps/desktop/src-tauri/src/listing_index_sizes/`) writes the rows an index update moved straight into the cache
  (`Listings::update_index_sizes`, data as `IndexSizes`, through `CachedListing::update_index_sizes_by_path`, which
  keeps both maps) before it tells the pane, so `get_listing_stats` stays read-only and sees up-to-date
  `recursive_size`. A whole-volume update runs the full `Listings::refresh_index_sizes` re-enrich instead (§ "Index
  sizes off the lock"). `IndexSizes` is data and not a `&mut FileEntry` closure so no app code can touch a name or path
  under the cache write lock (a renamed entry would desync the path index).
- **Hidden-file filtering in Rust, not the frontend**: visible count is unknown until all files are read. APIs accept
  `include_hidden: bool` and read through the listing's row map (§ "Row numbers"); `directory-diff` events use the
  listing's own recorded setting (§ "Diffs speak the pane's rows").
- **The engine's methods are synchronous; the app's read commands are `async`**: a sync `#[tauri::command]` runs on the
  MAIN thread in Tauri 2, so one slow accessor stops the app answering IPC at all. The app wraps them (and their
  deadlines) in `commands/file_system/listing.rs`; the engine never sees a deadline. `refresh_index_sizes` runs on the
  app's blocking pool, because it asks the index two SQLite queries and an index storm fires it once per event per pane.
- **Brief column widths stay app-side** (`font_metrics/brief_columns.rs`, reading rows through `with_rows`): they're
  pane layout over font metrics. Why the metrics live in a Rust cache:
  `apps/desktop/src-tauri/src/font_metrics/DETAILS.md`.
- **Revision on `CachedListing`, not `WatchedDirectory`**: it describes the committed pane view on every volume,
  including SMB and MTP, which have no FSEvents `WatchedDirectory`. See § "Diff event coalescing".
- **The engine reports facts through the `ListingEvents` seam** (`host/events.rs`): `opening`, `progress`,
  `finished(ListingEnd)`, `diff`, … The app's `listing_host::events::TauriListingEvents` owns every wire payload and the
  policy on them (error classification, the restricted-empty-root hint, analytics, the MCP ring, the TCC set), so the
  engine names no Tauri type. A read reports its own complete or cancel; a refused one is reported by its task
  (`streaming::report_failure`) after the cancel token is gone. Tests record facts with `testing::RecordingEvents`; the
  app policy's matrix (`listing_host/tests/streaming_reaction_test.rs`) collects wire events with `WireCollector`.
  `Arc<dyn ListingEvents>` (not `&dyn`) because it's cloned into the progress callback. **Why facts and not wire
  events:** the restricted-empty-root hint, error classification, analytics, the MCP ring, and TCC bookkeeping are
  product policy, and wire payloads, their `tauri_specta` derives, and every English word belong with the product. The
  engine derives only `serde` and `specta::Type` on the answers it returns (`index-crate-isolation` forbids
  `tauri-specta` here).
- **Every other app question is a seam too** (`host/mod.rs`): which volume serves a path (`volumes`, including the one
  `unregistered` question that folds mount adoption and "why is it missing"), what the drive index knows (`index`), the
  foreground hold (`leases`), the reading language (`locale`), and the mount table (`mounts`). What each one is, and the
  no-op it degrades to: `src/host/DETAILS.md`. The two scratch-visibility settings are engine state the host sets
  (`Listings::set_scratch_visibility`, which the app's settings commands call).
- **Watcher starts AFTER listing-complete**: watcher diffs rely on cached entries; starting before the cache is
  populated would miss initial state.
- **Incremental watcher path with fallback to full re-read**: most FS changes touch a few files. The incremental path
  stats each changed path, classifies add/remove/modify against the cache, and patches in-place via
  `insert_entry_sorted` / `remove_entries_by_paths` / `update_entry_sorted`. Falls back to full
  `handle_directory_change` when events exceed 500 or contain unknown kinds (`Any` / `Other`), which can't be reliably
  classified.
- **Instant row for entry creation** (the app's `crates/cmdr-operations/src/create.rs::patch_listings_after_create`):
  `create_directory` / `create_file` and the paste writer return before the watcher fires; without it the new entry
  wouldn't appear until the next debounce (~200 ms). The create hands the local backend's mutation patch a
  `MutationEvent::Created` for `new_path`'s file name, which stats the entry and sends an `Added` through the app's
  composer (index, badges, panes), like every other local mutation. `notify_added` upserts, so a row the watcher added
  first gets the create's fresher stat, and the watcher's later duplicate is a modify. Upsert is safe because no create
  overwrites (mkdir and mkfile refuse a taken name, paste retries `pasted (N)` on `O_EXCL`), so a row already at the
  path is the watcher's row for this file or a stale one; `insert_entry_sorted` re-checks the path under the write lock,
  so a race can't duplicate. The index half is an upsert too, pinned idempotent by `cmdr-index`'s
  `the_same_added_applied_twice_counts_once`.
- **The engine is a handle, `Listings`**, not crate statics. **Why:** a static cache made every test share one cache
  (unique ids, scoped reaper sweeps, a process-wide lock serializing every scratch-visibility test) and kept the cache
  global for any other consumer. The app holds its one engine in a `OnceLock` (`listing_host::listings()`). Two
  process-wide registries the engine reads stay where they are, because neither is listing state: `cmdr_fs::staging`'s
  in-flight scratch set and the ICU collation data behind `NameCollator`.
- **`Listings::builder(volumes, events)` takes the two seams a consumer can't do without as arguments**; the other six
  default to no-ops, and `Listings::detached()` defaults all eight. **Why:** an engine with no volumes lists nothing and
  one with no events leaves every pane on "loading", so forgetting either is a compile error, not a degraded mode. The
  no-op defaults are public unit types so a caller can pass one explicitly (the app's test engine passes `NoEvents`).
- **A listing id is a `ListingId` in every Rust signature**; only the IPC boundary sees a `String`. **Why:** a listing
  id and a volume id were both `&str` side by side in a dozen signatures, the confusable-positional-params shape
  `AGENTS.md` bans. It's not a `specta::Type`, since an exported type would change the generated bindings. Volume ids
  stay `&str`: they're `cmdr_fs` vocabulary.
- **Nothing long-lived holds the engine strongly.** Background loops (the orphan reaper) and anything the engine hands
  out that outlives a call hold `Inner` weakly and give up, or answer the no-op, once the last `Listings` is dropped.
  **Why:** a strong handle in a loop leaks one engine per test, and one held by a volume closes a cycle (`Listings` →
  volumes → a volume → its `VolumeHost` → `Listings`) for any host that hands volumes over as values. Per-event tasks (a
  refresh, the 50 ms diff flush) hold it strongly, because they end.
- **The engine is never a backend's `ListingHost` itself.** A host wires its backends' `cmdr_fs` `ListingHost` to its
  own composer (Cmdr's: `listing_host/DETAILS.md` § "The adapters"). **Why:** a backend's `directory_changed` must reach
  the index and the badges before the panes (`index → badges → panes`), and an engine-built host would patch the panes
  alone.
- **What isn't the engine:** the local backend's `std::fs` reader and mutation patch (`crates/cmdr-local/`), Brief
  column widths, the FSEvents watcher's OS half, and every wire event struct. They answer to the engine's API from the
  app.
- **A poisoned engine lock recovers** (`cmdr_fs`'s `*_ignore_poison`), ❌ never `.expect`: a panic would turn one
  poisoned cache into a crash on every later IPC, and skipping the work (the shape it replaced) silently dropped a
  listing while still reporting it complete.

## Cache helpers (`caching.rs`, `mutate.rs`, `authoritative.rs`)

The `Inner` methods the watcher's incremental path (`observe_children`) and the change intake patch listings with,
without full re-reads. Callers outside the crate reach them through `Listings`' lookups and intake methods:

- `find_listings_for_path_on_volume(volume_id, path)`: all listing IDs whose directory matches the path on that volume
  (multiple panes/tabs may show the same directory). `None` matches verbatim across every volume (below).
- `try_get_authoritative_listing(volume_id, path)` (`authoritative.rs`, public as `Listings::authoritative_listing`):
  the fresh-listing oracle for write-op pre-flight scans and the look-alike check before a new name
  (`crates/cmdr-operations/src/look_alike.rs`). Returns `Some(entries)` when a cached listing exists for
  `(volume_id, path)` and `listing_watch_coverage(path) == WatchCoverage::EveryWriter` (delegated to the backend via the
  `Volume` trait), else `None`. When multiple listings exist for the same pair (two panes), picks the
  most-recently-updated one deterministically: highest `sequence` (an `AtomicU64`), ties broken by latest `created_at`.
  Entries are cloned out under the cache `RwLock`, then the lock is released before the volume call (cheap clone for a
  flat `Vec<FileEntry>`, < 5 ms for 15k entries; matters because otherwise the volume call holds the cache lock across
  an await and blocks pane navigation). See the freshness-contract section in `volume/CLAUDE.md` for per-backend
  debounce windows callers must tolerate.
- `insert_entry_sorted(listing_id, entry)`: inserts in sorted position, returns the pane rows (`PaneRows`, § "Diffs
  speak the pane's rows"). All four patch helpers here answer in pane rows.
- `remove_entries_by_paths(listing_id, paths)`: removes by exact file-path match, returning
  `(pre-removal pane rows, entry)` highest-index-first. Used by the local FSEvents incremental path, where the event
  path shares the entries' path space. ❗ There is no single-path form: that caller is always a batch, and looping one
  was a quadratic. See § "Entries by path".
- `remove_entry_by_name(listing_id, name)`: removes by file NAME within the listing (its directory, so names are
  unique). This is what the `Removed` change patch uses, so it works even when the listing's stored entry paths use a
  different path space than the notifier's resolved parent. That's the case for MTP: `MtpVolume` stores each entry's
  `path` as the storage-relative inner form (`/Documents/notes.txt`) while `notify_mutation` resolves the parent to the
  absolute `mtp://…` URL, so a full-path match never matched and `notify_mutation(Deleted)` silently no-oped (moved or
  deleted MTP files lingered in the source pane until a manual refresh).
- `update_entry_sorted(listing_id, entry)`: updates an existing entry (remove + re-insert if sort position changed),
  returns the pane rows before and after; a pair that differs is a `Move`.
- `has_entry(listing_id, path)`: whether a path exists in the cached listing (classifies watcher events add vs modify).
- `Listings::location(id)`: a listing's volume id and directory (the watcher filters its events to direct children).

## One spelling per directory (`ListingPath`)

`CachedListing::path` is a `ListingPath`, and `ListingPath::on_volume(volume_id, path)` is the only way to make one: it
asks the registered volume for its spelling (`Volume::listing_path`, identity by default). Every store
(`CachedListing::new`) and every path-keyed lookup (`find_listings_for_path_on_volume`, `get_cached_listing`,
`try_get_authoritative_listing`, the archive refresh) builds its key through it BEFORE taking the cache, since it asks
the host's volume seam.

**Decision:** canonicalize in the cache, not at each caller. **Why:** a pane path enters from the frontend (navigation,
Enter on a row, go-to-path, history, tab restore), from a backend's change report, and from pre-flight oracle queries
built off entry paths. Only MTP spells one folder two ways today (the `mtp://` storage URL its reports and the volume
switcher use, and the inner `/DCIM` its rows carry), but a per-caller fix would have to find every route, and a verbatim
match there dropped every Cmdr-made delete on a pane the user had entered with Enter (field reports ERR-QW42X,
ERR-46A6B, v0.44.0). MTP canonicalizes to the inner spelling because its rows, the drive index, and the walkers already
use it. ADB, SFTP, and WebDAV rows carry their full app URL, and SMB, archive, and git-portal rows share their root's
spelling, so they keep the default. Pinned by `listing_host/tests/mtp_listing_path_test.rs`.

`Listings::listings_for_local_path(path)` (`find_listings_for_path_on_volume(None, …)` inside) has no volume id and
compares verbatim across EVERY volume, so a remote pane showing the same path string matches too. Only
`LocalPosixVolume::listing_watch_coverage` relies on it, because the volume doesn't know its own id. A caller that knows
its volume uses `listings_for_path_on_volume`: a local create's instant row once landed in an SFTP pane showing a remote
`/tmp` beside a local one.

## A pane path the volume stores another way (`foreign_path.rs`)

`ListingPath` folds spellings a backend itself hands out. A different problem is a path that came from OUTSIDE the
volume's listings (typed, pasted, a restored tab, a favorite, MCP `nav_to_path`, or a pane carried over from the macOS
kernel mount, which decomposes every name) on a backend that matches names byte-for-byte. SMB is one: an accented folder
the share stores composed answers the kernel's decomposed spelling with `STATUS_OBJECT_PATH_NOT_FOUND` (ERR-VETBX,
`crates/cmdr-smb/DETAILS.md` § "SMB names are opaque bytes").

- **The pane seam lists as stored.** `read_directory_with_progress` lists through `list_as_stored`: as given first (free
  on the happy path), and only on `NotFound` asks `Volume::find_stored_spelling`, then lists what it answers. From there
  on the listing IS the stored spelling: `CachedListing::path`, the watch, the overlays, the index enrich, and every
  entry path. `listing-complete` carries `storedPath` (only when it differs), and the pane re-spells its tab and current
  history entry in place (`navigate.ts::adoptStoredSpelling`), so there's no Back step to the other spelling and a
  pinned tab doesn't fork. Favorites stay as the user wrote them: we don't silently edit something a person authored,
  and the cost is one resolve per click, remembered per share until the folder changes.
- **A backend swap respells what's open.** When `network/smb_upgrade.rs` hands a share from the kernel mount to a direct
  connection, `respell_listings_on_volume` re-reads each open listing on it through the new backend, holding the
  directory's refresh turn (`refresh.rs`'s `refresh_turn`). A listing whose path changed spelling is re-keyed, its
  entries are written whole (the diff matches rows by name, so it can't see every path changing), and the pane adopts
  the path from `listing-respelled` and refetches its rows. That sweep sees only listings already cached, and a pane
  landing on the share starts the upgrade the moment it starts listing (`network/smb_pane_upgrade.rs`), so the swap
  often lands mid-read. So `read_directory_with_progress`, right after its cache insert, re-reads its own listing when
  the registry no longer serves the id with the backend that read it (`respell_if_read_by_a_replaced_backend`, pointer
  identity): a swap before the insert is caught there, one after it by the sweep.
- **Files from outside Cmdr respell where they enter.** A Finder drag-in and a paste of files copied in Finder hand over
  kernel-mount paths (`/Volumes/<share>/…`), which route to the direct SMB volume when the share has one
  (`resolveSourceVolumeId`). The frontend asks `stored_spellings`
  (`apps/desktop/src-tauri/src/commands/file_system/stored_spelling.rs` → `foreign_path::stored_spellings`) once, right
  after resolving the source volume, so the scan preview, the conflict check, the transfer, and the journal (which a
  rollback replays) all carry one spelling. All-ASCII paths skip the round trip (one Unicode form, and the kernel keeps
  the server's case); a path with no other spelling, two look-alikes, or a deadline past 10 s goes as given, so an
  operation still means exact bytes and never a guessed twin.
- **Decision: only these seams resolve.** **Why:** a resolve can't tell "another spelling of this folder" from "a
  different folder sharing its folded name, the one named having vanished". A delete walker, a copy scan, a watcher
  refresh, or an existence probe holds a path that came out of a listing, so a miss there means it's gone, and resolving
  would hand the walker a look-alike twin. ❌ Don't route those through `list_as_stored`.
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

`Listings::directory_changed(volume_id, parent_path, change)` (`caching.rs`'s `notify_directory_changed`): applies a
directory change on a volume to the panes, and only the panes. ❌ The engine never applies it to the drive index itself:
the host composes the subsystems that hear a change around this call, in its own order, and Cmdr's is
`apps/desktop/src-tauri/src/listing_host/DETAILS.md` § "The composer". It runs live from the first call, with or without
anyone listening: the events seam drops what nobody hears. `DirectoryChange` variants:

- `Added(FileEntry)`: single add, patches via `insert_entry_sorted`.
- `Removed(String)`: single remove by name, patches via `remove_entry_by_name` (name match, not full path — see above).
- `Modified(FileEntry)`: single modify, patches via `update_entry_sorted`.
- `Renamed { old_name, new_entry }`: same-dir rename (remove old + insert new).
- `Replaced(Vec<FileEntry>)`: the backend already re-read the directory and hands the contents over; the host sorts them
  the listing's way, diffs, stores, and publishes (`publish_replacement`). Where an overlay applies, it re-reads like
  `FullRefresh` instead (§ "The overlay step").
- `FullRefresh`: re-reads via the Volume trait, computes a diff against the cache.

`Replaced` and `FullRefresh` differ only in who does the read, and both end in the same `publish_replacement`. Report
`Replaced` when the entries are already in hand (a device event loop that has to invalidate its own path cache and
re-list anyway); report `FullRefresh` when the host should go and get them, which also gets the volume-wide fallback
when no listing matches the exact path. ❗ Sorting before the diff is load-bearing, not a double sort: a backend answers
in its protocol's order (MTP by object handle), so a diff computed against that order carries indices pointing at the
wrong rows in a pane sorted any other way.

All variants enrich entries with index data and queue `directory-diff` events through `diff_emitter::Publish`. A re-stat
whose sort key changed (`sort_key_changed`, § "Requested and effective sort") re-inserts the entry at its new sorted
position and reports one `DiffChangeType::Move` (`apps/desktop/src-tauri/src/file_system/DETAILS.md` § "Reordered
rows"), which is what lets the pane cursor follow the row. Natural deduplication: `insert_entry_sorted` returns `None`
for duplicates, `remove_entry_by_name` returns `None` if already removed. Callers: `Volume::notify_mutation()` (after
each successful create/delete/rename on all volume types) and the `rename_file` command (local FS renames), both through
the app's composer. A create on a volume where `supports_local_fs_access()` is `true` goes through the same local
mutation patch (§ "Decisions", the instant row).

`Listings::refresh_archive_listings(volume_id, archive_path)` is a sibling entry point for the archive content watch: it
`FullRefresh`es every open listing at or inside a changed `.zip` (parent drive id + full path) WITHOUT the drive-index
sync the composer runs, since an archive-inner path isn't a real filesystem path. Rationale and the watch that drives
it: `crates/cmdr-archive/src/watch/DETAILS.md`. What a refresh DOES to this cache is
`listing_host/tests/archive_watch_integration_test.rs`: a refresh through `AppListings` reflected in an open listing
while an outside listing is untouched, a truncated mid-write keeping the previous listing, and LRU eviction releasing
the watch. No FSEvents timing lives in it; the backend's half of the seam is `cmdr-archive`'s `watch/host_seam_test.rs`.

`listing_host/tests/smb_pane_close_watch_integration_test.rs` is the other cell whose other half is a backend: closing a
pane's listing (`Listings::end`) drops a cache entry and its FSEvents `WatchedDirectory`, and must not reach the
volume's own watcher, which the index depends on with no pane open. It runs over a real `cmdr-smb` session because that
watcher is the one at stake, and it takes its fixture from `operations_host::backend_suites::smb_test_support`.

## Diff event coalescing (diff_emitter.rs)

Mutation owners (`mutate.rs`'s patch helpers and tag writes, `reread.rs` full replacements) allocate the listing's
visible `sequence` and enqueue under ONE cache write lock. Initial revision is zero; hidden-only patches do not advance
it. Full replacements arrive enriched (`pipeline.rs`) and sort first, then `diff_rows` compares the committed old
projection with the pinned replacement projection and stores the final replacement under that lock. An identical raw
replacement still reconciles visibility drift. Callers never enqueue the same patch again. Lock order is cache then
queue, and that is a type: the queue's only writer is `diff_emitter::Publish`, which `CacheWrite::get_mut`
(`Inner::write_cache`) hands out borrowed from the cache's write guard, so nothing enqueues or drops a listing's batches
without holding the lock; `CacheWrite::remove` drops a closed listing's queue in the same acquisition. The emitter never
reads the cache to stamp a batch.

**Scratch visibility belongs to the revision.** `CachedListing` captures exact path-keyed decisions at construction, so
revision zero's count and later rows agree. Ordinary directories have an empty projection. Read boundaries keep the
shared-lock fast path when there is no drift; otherwise they reacquire the write lock, capture and recheck live
decisions once, diff old/new projected rows, commit the projection, and publish its revision and pinned count before
consuming indices. Stable scratch does not block guarded sorting, hidden toggles, selection snapshots, or comparison. If
ownership or either advanced scratch setting changed, an old guarded revision returns `Changed`; applying the transition
and retrying succeeds. A flip after validation cannot change rows at that revision: rows never resample. An unobserved
ABA exposes no intermediate rows; an observed flip and flip back allocate separate revisions.

Entry mutations reconcile and publish scratch drift FIRST, then derive their own old/new coordinates using that pinned
projection. For `[b,c]` with hidden `a.cmdr-tmp-*`, ownership expiry plus removal of `b` publishes an add at zero, count
three (`r0→r1`), then a remove at one, count two (`r1→r2`). New scratch paths are sampled once when admitted; the same
decision supplies their diff and count. Removed candidates are forgotten. A projection-only change affecting no
currently shown rows still publishes an empty batch linking revisions (for example, a scratch dotfile with hidden files
off). Reconciliation has no ownership notifications and does not clone the whole listing on ordinary reads; both
visible-row caches remain valid across projection changes.

Wire payload: `{ listingId, batches: [{ fromSequence, sequence, totalCount, changes }] }`. Each batch is one old/new row
space; one multi-path removal is one batch, successive mutations are separate batches. `totalCount` is the final visible
count for that transition. A 50 ms window coalesces transport without flattening coordinate spaces. Events may arrive
out of order; clients chain batches by revision rather than treating an event's changes as one transition.

`resort_listing` accepts a final `expected_sequence: Option<u64>` and returns `ResortResult` with `sequence`,
`totalCount`, `effectiveSortBy`, `newCursorIndex`, and `newSelectedIndices`. It refuses stale revisions before
interpreting selected rows, commits a revision even for identical ordering, and discards queued old-space batches under
the cache write lock. Already drained batches retain their older stamps, so the client can discard them.
`set_listing_include_hidden` takes the expected revision plus optional cursor and selection, remaps exact surviving
identities from the OLD visibility to the NEW visibility, and returns the same result. An unchanged setting does not
allocate a revision.

`get_selection_snapshot(listing_id, include_hidden, selected_indices, expected_sequence)` consumes backend-space rows
under one reconciled read lock, returning `{ paths, fileCount, folderCount }`. Revision or visibility mismatch and
invalid rows return typed `ListingLookupError::Changed` (`type: "changed"`); a missing listing remains `Gone`. This
guards row-to-path consumption, not subsequent filesystem operations.

Ending a listing drops the queue under the cache write lock. The app's E2E `flush_all_watchers` helper also drains
queues through `Listings::flush_pending_diffs` (the `playwright-e2e` feature) without waiting for the transport window.

## File metadata tiers

What the local reader fetches eagerly and what it defers: `crates/cmdr-local/DETAILS.md` § "The local reader".

## Finder tags

`FileEntry.tags` holds macOS Finder tags (`com.apple.metadata:_kMDItemUserTags`), which the app reads and writes in
`apps/desktop/src-tauri/src/file_system/tags_io.rs`, through the volume. Each tag is `(name, color)` where color `0` =
none (a colorless named tag), `1` grey, `2` green, `3` purple, `4` blue, `5` yellow, `6` red, `7` orange. The per-file
xattr is the display source of truth — Finder rewrites every file's xattr on a recolor, so we never read the system tag
registry.

**Why deferred, visible-range-first.** A `getxattr` for tags costs **~15 µs/file** (benchmarked 2026-06-28, synthetic
200k-file dir, warm), ≈6× the per-entry `lstat` the core listing already pays (the `_kMDItemUserTags` namespace isn't
free). So `list_directory_core` never touches tags; the frontend calls `enrich_tags(listing_id, paths)` for the visible
range (mirroring the custom-folder-icon prefetch), and a background sweep backfills the rest. Visible range (~100 rows)
≈ 1.5 ms; a full 200k sweep ≈ 3 s, off the render path. On a direct SMB share the same read is a network round trip,
pipelined over smb2: about 1,100 files a second (`docs/notes/smb-finder-tags-2026-10-31.md`).

**Flow.** The app's `enrich_tags` command reads tags for the batch and calls `Listings::apply_row_facts` with
`RowFacts::Tags` (`mutate.rs`'s `apply_tags_to_listing`), which mutates entries in place (tags are sort-irrelevant — no
reorder), replaces **unconditionally** (clearing to empty so an external removal propagates), and emits one coalesced
`modify` diff for the rows that actually changed (so re-enriching an unchanged visible range is silent). It's
timeout-guarded, and a path whose read failed stays out of the update, so its dots stay until a read answers. The whole
batch resolves against the listing's path map in one pass, and the write it does leaves both maps standing: § "Entries
by path".

**Packages macOS reports** ride the same deferred shape: the app asks macOS about the visible dotted folders the package
list doesn't know (`enrich_packages`) and hands the answers over as `RowFacts::Packages` (`apply_packages_to_listing`).
Unlike tags, a package has an extension, so it can move a row under the Ext sort (`sort_key_changed` compares
`name_kind`): each changed row goes through `update_entry_sorted`. Only rows the list can't answer are touched, so a
failed lookup never unmarks a listed `.app`, and a `false` revokes an earlier detection.

**Carry-forward.** A watcher re-stat builds entries via `get_single_entry`, which reads no xattr (empty tags) and knows
only the package list. Every modify path (`notify_modified`, `observe_children`) calls `mutate.rs`'s
`carry_forward_deferred` BEFORE storing and emitting, copying the cached entry's tags, and a package macOS reported,
onto the re-stat'd one; otherwise any unrelated Modify event (content edit, mtime touch, chmod) would blank a file's
dots, or turn a detected package back into a folder, until the next enrich. It only ever restores (no-op when the
incoming entry already has tags; a package only onto a same-named non-symlink folder), so it never masks a real change;
clearing flows solely through the enrich paths.

**Write path, the context menu, and the toggle** (the app's `tags.rs`):
`apps/desktop/src-tauri/src/file_system/DETAILS.md` § "Finder tags".

## The foreground lease a listing holds

`read_directory_with_progress` takes a foreground hold on the volume (the `leases` seam, which Cmdr answers with a
`priority::foreground` lease) as its FIRST statement, inside a `ReadSession` (`streaming.rs`), and holds it for the
whole body. That is what tells a background SMB upload and the index scan that the user is waiting on this share right
now, for however long the folder actually takes to come back. The command entry point
(`commands/file_system/listing.rs`) still stamps the volume's timestamp on the way in: it covers the non-streaming path
and seeds the debounce, and the lease covers the listing itself. Design, both halves, and what bounds a held lease:
`priority/DETAILS.md`.

**It is RAII and nothing else.** Every exit gives it back with no code on the path: the error return, the three
cancellation returns, the restricted-empty-root return, a panic inside the task, and the task's future being dropped
when the runtime shuts down. The hold is a private field of `ReadSession`, which also carries the read's cancel token
and stall watch, and the read's body takes the session by value and reaches the token only through it: there is no guard
to bind to `_`, and the hold drops exactly when the body ends. ❌ Don't add a manual release beside the drop.

**Cancel releases it, deliberately.** The `select!` cancel arm returns while the detached backend task is still
unwinding, so the lease goes back before the wire work has finished. That is the right answer, not a leak: the pane has
already moved on, so nobody is waiting on that listing any more, and the transfer it was holding off should resume.

**The lease keys on the volume id the frontend asked with**, so a `.zip` opened on a share leases the SHARE (the
archive's own volume is resolved below this point and contends for nothing). Pinned by
`streaming_test::{a_listing_holds_a_foreground_lease_for_its_whole_duration, a_listing_that_fails_gives_its_lease_back, two_concurrent_listings_on_one_volume_both_have_to_finish, dropping_the_listing_task_mid_flight_gives_the_lease_back}`.

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

Pinned by `streaming_test::test_cancel_unwinds_the_listing_instead_of_aborting_it`, which drives a fake volume that only
ends when its token flips and fails if its future is dropped first.

## Stalled listings (`stall.rs`)

**The failure.** A pane navigating into an OS-mounted share whose server stopped answering (NFS or `smbfs`) showed a
spinner for as long as the kernel held the read, and `cmdr://state` showed the pane on the new path with
`totalFiles: 0`, no rows, and no error: indistinguishable from an empty folder. `LocalPosixVolume` reads inside
`spawn_blocking`, and `read_dir` on a silent mount blocks in the kernel with no timeout of ours anywhere above it. What
the stuck call answers when the server comes back differs by filesystem, so recovery can't count on either:

- **`smbfs`**: blocks for as long as the server is silent (116 s observed, with no sign of ending), then answers
  `ENOTCONN` the instant the server is back, and the kernel drops the mount (macOS 27.0, `docker pause` on a Samba
  fixture mounted with `mount_smbfs`, `ls` timed from a second shell, 2026-10-02).
- **NFS (hard mount)**: blocks until the server answers, then completes, or refuses with `EPERM` if it comes back with
  stricter exports (the NAS in the issue rewrote its exports as `secure` on restart). Reasoned from GitHub issue #305,
  not reproduced here.

**The watch.** `read_until_answered` races each read against `StallPolicy::stall_after` (8 s) of silence: no entry count
growth, since a local read stuck on one `stat` repeats the same number every progress tick. 8 s clears the 0.3–6 s a
busy NAS holds a single request for (`listing_done_level`), so a slow answer never reads as a dead server. A tick whose
`ListingProgress::phase` changed counts as movement too: a backup's index load (`cmdr-restic`) enumerates nothing for
seconds, and its bytes arriving are the read answering. Each tick reaches the host whole (`ListingEvents::progress`), so
the phase rides the `listing-progress` event for the pane to word. On a stall the listing emits `listing-stalled` and
keeps waiting; the frontend swaps the spinner for the "still waiting" screen, and any later progress, complete, error,
or cancelled event for the same listing replaces it. A read that resumes producing entries clears the stall, and a later
silence reports it again.

**After a stall, the listing answers for itself** until the volume does, so the pane recovers with no help:

- A STUCK read's refusal is stale (it spent the outage in the kernel, and the `ENOTCONN` above is exactly that), so the
  listing asks again at once.
- A PROMPT transient refusal (`ErrorCategory::Transient`, the same classification the error screen renders from) is
  retried after a backoff: 2 s doubling to 30 s.
- A PROMPT lasting refusal is the answer and ends the listing (a folder gone once the server is back walks the pane up
  as before). So does any refusal from a listing that never stalled: nothing changed for a healthy volume.
- On a `BackendKind::Local` volume (where reads block in the kernel), one fresh probe runs beside the stuck read once
  the first backoff passes, since a stuck read can stay stuck after the server is back, and the probe answers the moment
  it is. Connecting backends (SMB direct, SFTP, WebDAV, MTP, ADB) get no probe: their own session timeouts and the
  reconnect manager already turn silence into a typed answer, and the frontend's `live-retry` re-lists once the volume
  is live again.

The retries stop with the listing: cancel (the user navigating away, Esc, or Go back) wins every `select!`, and the
function's drop guard cancels the child token every read carries, so a read still waiting at the gate never starts.

**The thread bound.** Every read on a hung mount pins a blocking-pool thread until the kernel lets go, and the pool is
finite (`deadline::BlockingBudget`'s doc has the incident where it ran out). One listing holds at most two reads (the
stuck one and the probe). Across listings, `HungReads` keeps every volume's reads: once one is hung, at most
`MAX_READS_ON_A_HUNG_VOLUME` (4) may be in flight on that volume, and any further read waits at the gate as a future,
never a thread. That leaves a stalled listing's two reads plus a user's Retry room to land the moment the server is
back, and caps what mashing Retry or opening one folder after another can pin. A read's slot lives in its own task, so
it frees when the KERNEL lets go, however long after its listing moved on.

**What it's stalled on (`stalled_on.rs`).** The event carries `stalled_on: StalledOn` (`server` / `drive` / `unknown`),
which picks the screen's wording, so it says "server" or "drive" only when the mount proves it and keeps the combined
line otherwise. Classified once per listing, at its first stall:

- **Direct backends name themselves**: SMB direct, SFTP, and WebDAV are `server`; MTP and ADB are `unknown` (a USB cable
  for one, a cable or Wi-Fi for the other, so neither word fits both).
- **A filesystem path** (`Local`, and an archive or `.git` portal inside one) reads the mount it lies on off the
  kernel's table through the host's `MountTable` seam, ❌ never a `statfs` on the path, which blocks on the very mount
  that just stalled. Cmdr answers with the `getfsstat(MNT_NOWAIT)` snapshot on macOS
  (`volumes::mount_type_and_source_for`) and `/proc/mounts` on Linux (`linux_mounts::mount_entry_for_path`). Network
  types on an explicit allowlist (`smbfs`, `nfs`, `afpfs`, `webdav`, `ftp`; `cifs`, `smb3`, `nfs4`, `fuse.sshfs`,
  `fuse.rclone`, `fuse.s3fs`, …) are `server`; a known local disk (`cmdr_fs::filesystem_kind::mount_is_local_disk`) is
  `drive`; anything else (GVFS's `fuse.gvfsd-fuse`, which holds phones and shares alike, macFUSE, cloud clients' mounts,
  autofs, `9p`) is `unknown`.
- **The table lookup is lexical**, so a path that reads as a local disk is resolved through its symlinks first (`~/nas`
  → `/Volumes/nas`) on a blocking thread, bounded at 500 ms. A timeout means the probe hit the hung mount, which reads
  as `unknown`. That probe can pin one blocking thread per stalled listing for as long as the kernel holds it, outside
  the `HungReads` gate below; it only runs when the lexical answer was `drive`.
- Not followed: a symlink INSIDE a network share pointing back onto a local disk (still reads `server`).

**Decision: a gate that closes only on a hung volume, not a `BlockingBudget`.** A budget caps a family's reads always,
which would throttle the healthy concurrent listings of a busy pane pair, two tabs, and a refresh on the boot disk. The
gate costs nothing until a read on that volume has actually gone quiet. The bound is loose for the first 8 s (nobody
knows the volume is hung yet), which only a human mashing Escape and Enter can exploit.

**Decision: never a deadline that ENDS the listing.** Ending it would hand the pane an error while the server may be
seconds from answering, and the stuck read would keep its thread anyway. Waiting costs the same thread and lands on its
own.

Pinned by `stall_test` (a read that never answers stalls within the deadline; recovery from a stale `ENOTCONN`;
transient retries; a lasting refusal ends it; cancel stops retries; the per-volume bound, mutation-checked; a steady
read never stalls). The frontend half: `apps/desktop/src/lib/file-explorer/pane/DETAILS.md` § "A folder that stops
answering".

**Manual repro**, no NAS needed: run a private copy of the guest SMB fixture image
(`docker run -d --rm -p 127.0.0.1:<port>:445 smb-consumer-smb-consumer-guest`),
`mount_smbfs -N //guest@127.0.0.1:<port>/public <dir>`, open `<dir>` in a pane, `docker pause` the container, and open a
subfolder: the "still waiting" screen appears after 8 s. `docker unpause` and the pane lands, or walks up if the kernel
dropped the mount. Don't pause the shared fixture containers: other sessions' E2E runs use them.

## Serializing full refreshes

A `FullRefresh` re-reads the directory through the Volume trait, diffs the result against the cached listing, and then
REPLACES that listing wholesale. Read and write are two steps, so two refreshes of the same directory running at once
can finish in the opposite order to the one they started in, and the loser writes a snapshot the winner has already
superseded.

**Why that is worse than a flicker.** Nothing schedules a re-read afterwards. The listing keeps the older truth until
some unrelated event happens to touch the directory again, so entries that exist on disk are simply absent from the pane
— for as long as the folder stays quiet. Watched folders take bursts all the time (an unzip, a `git checkout`, an rsync,
a build writing output), and a burst is exactly what fires several refreshes at once.

`notify_full_refresh` therefore takes a per-directory turnstile, keyed on `(volume_id, parent_path)` — the pair that
identifies what a refresh re-reads — before doing anything. Holding it across the read makes read-then-write atomic
against other refreshes of that directory, which is the whole guarantee: a refresh that starts later reads later, so it
cannot answer with a staler directory.

**At most one runs and one waits.** A third arrival returns immediately rather than queueing: the refresh already queued
starts its read after the running one finishes, so it will observe everything the third arrival would have. That turns a
storm of N events into two reads instead of N, which matters most on the big directories where a refresh is expensive.
Both properties are pinned by `a_slow_refresh_cannot_overwrite_the_listing_a_newer_one_already_wrote` and
`a_storm_of_refreshes_costs_two_reads_of_the_directory_not_one_per_event`, which script a fake volume so the slow read
is the one that started first (otherwise the interleaving is timing-dependent and the test is itself a flake).

The turnstile map prunes entries whose only remaining reference is the map itself, so a long session browsing a wide
tree doesn't accumulate one per directory it has left.

❌ Don't "optimize" this back into concurrent refreshes. The cost it removes is real and the failure it prevents is
silent: the pane looks settled and correct, which is why it went unexplained through several E2E flakes before the
duration evidence pinned it.

## Read generations

**The re-read API** (`reread.rs`, the watcher's and ⌘R's half): `refresh(id)` re-reads through the volume and returns
only once its read landed or was superseded, which E2E's `flush_file_watcher` and ⌘R's 2 s `TimedOut` wait both rely on.
It answers a `RefreshOutcome`: `Landed`, `Superseded`, `DirectoryGone` (the host emits `directory-deleted` and stops the
watch), `NoVolume` (nothing serves the listing's volume id: the host's cue to `begin_read`, read the directory itself,
and hand the entries over with `refresh_from_entries`), `ListingGone`, or `Skipped` (`PermissionDenied` silently, any
other read error logged at warn). `observe_children(id, children)` is the incremental path: the engine classifies each
stat'd child as add, remove, or modify, enriches them in one call, carries tags forward, and publishes removes, then
adds, then modifies.

The turnstile orders `FullRefresh`es of one directory, and nothing else: ⌘R and the watcher's re-read
(`Listings::refresh`), the `std::fs` hand-over (`begin_read` + `refresh_from_entries`), a respell, and a backend's
`Replaced` never queue on it. So every full re-read also takes a `ReadGeneration` (`reread.rs`) from the engine's one
counter at the moment it STARTS reading, and `Inner::update_listing_entries`, the one step that stores a re-read, drops
it under the cache write lock when the listing already published a newer one (`RefreshOutcome::Superseded`).

When each one takes it, because a generation taken too early drops a FRESHER read:

- A `FullRefresh` after acquiring its turn, right before `list_directory`, never while queued.
- `refresh` and the respell after resolving the volume, right before the read.
- `begin_read` when the ticket is issued, before the app's own read, so it dates the read rather than the hand-over.
- `Replaced` on arrival. The backend's read predates that a little; accepted, since dating it needs a `cmdr_fs` change
  and the backend serializes its own re-reads.
- The streaming read's insert publishes `ReadGeneration::INSERT` (0), so any later re-read wins.

Equal generations both land: the respell writes one read twice when the spelling changed.

**Incremental patches take none and bump none.** A re-read that started before an `Added`, `Removed`, `Modified`, or
`observe_children` patch can still land after it and carry the older state, as it always could; bumping on every patch
would drop every re-read during a busy burst. Don't "fix" it silently.

**Why not put `refresh` through the turnstile.** Three breaks: `notify_full_refresh` returns early when the waiting
berth is taken, so an awaited ⌘R would return before its read landed and E2E's `flush_file_watcher` would go flaky; a
turn held by a read stalled on a dead mount is held forever (a read is never aborted), so ⌘R would queue behind it
instead of reading through the recovered mount; and the turnstile keys on the notifier's raw path while a listing keys
on its `ListingPath`, which differ for MTP. Pinned by `read_generation_test.rs`.

## Index sizes off the lock

`index_sizes.rs` refreshes a listing's folder sizes (`Listings::refresh_index_sizes`, once per index event per open
pane) with no engine lock held while the index answers, which is two indexed SQLite queries:

1. Under the READ lock, clone the non-symlink folders (the only rows `ListingIndex::enrich` writes, by the seam's
   contract) and the listing's PUBLISHED read generation. No folders, nothing to do.
2. Enrich the clones, no lock held.
3. Under the WRITE lock, drop the answers if the published generation moved: the re-read that replaced the rows asked
   the index again once they landed, later than this did. Otherwise write the seven `recursive_*` fields back by path as
   `IndexSizes` through `update_index_sizes_by_path`, which keeps both maps, doesn't re-sort, and doesn't bump
   `last_accessed_ms`, and publish every shown row whose sizes moved as a modify, under the same lock.

The publish keeps the pane on the cache's numbers. The app's index-size worker sends only what differs from the cache,
so a size written here silently never reached the pane: a re-read's diff carries the pipeline's sizes, the replace
step's newer ones landed in the cache alone, and the worker, comparing against them, saw nothing to send.

The published generation, never the last one handed out: a re-read in flight that then fails or is superseded must not
cost these sizes with nothing to replace them. By path is order-independent, so a re-sort in between is harmless; a row
added in between was enriched by its own insert, and one removed or renamed is skipped. A `Modified` patch in between
takes no generation, so it can get sizes one index event older written over its own enrich; the next event corrects it,
the same window two events always had.

The replace step (`update_listing_entries`) runs the same routine once its rows land, for a read the index holds rows
for, and enriches nothing under its lock. That second ask is what makes "the re-read enriched them later" true: the
pipeline's own enrich ran BEFORE the replace, so an index event between the two would otherwise lose its sizes to the
generation check. A routed re-read (an archive, a git snapshot) is never enriched, by the pipeline or by the replace.
`resort_listing` asks the index (`ask_index_sizes`) before it takes the cache lock, so a sort by size sees the index's
current sizes and no pane read waits behind the index, and writes the answers silently under its own lock after its
sequence check: the resort drops every pending diff and the pane re-fetches, and a publish would move the very sequence
the caller asked it to check. That makes the rule hold with no exception: no seam runs under an engine lock. Pinned by
`index_sizes_test.rs`.

## Where each crate invariant lives

The rules the engine keeps that this crate's extraction added, each with one home:

- **No `tauri`, `tauri-specta`, or `cmdr` in the tree, and a capped surface**: `index-crate-isolation` enforces both; §
  "The public surface is capped".
- **Every app concern arrives through a `host` seam that degrades to a no-op and has a `testing` fake; a seam is called
  per read or mutation, never per entry**: `src/host/CLAUDE.md`.
- **The engine reports facts; wire payloads, derives, and words live app-side, and the bindings don't move**: §
  "Decisions" (the `ListingEvents` bullet).
- **`index → badges → panes` is the order every directory change is applied in, owned by the host's composer, and the
  observers run in registration order**: Cmdr's are `apps/desktop/src-tauri/src/listing_host/DETAILS.md` § "The
  composer" and § "The observers".
- **Test scaffolding is gated on `any(test, feature = "testing")`**: `CLAUDE.md`.
- **No seam, observer, or overlay runs under an engine lock, with no exception; `with_rows` is the one closure run under
  the cache lock and it never re-enters `Listings`; everything else crosses as data**: `CLAUDE.md`, and § "Index sizes
  off the lock" for the index refresh, the replace step, and resort, which honor it by enriching off the lock.
- **Background loops hold the engine weakly**: `CLAUDE.md`, and § "Decisions".
- **A listing id is a `ListingId` in every Rust signature; only the IPC boundary sees a `String`**: § "Decisions". The
  same reasoning (no confusable positional params) shapes the row-space methods' arguments.
