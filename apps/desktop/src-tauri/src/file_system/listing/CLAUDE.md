# File system listing module

Backend directory reading, caching, sorting, and streaming: 100k+ entries, non-blocking, with progress.

## Module map

- Read: **reading.rs**, **streaming.rs**, **stall.rs**. Cache and mutate: **cached_listing.rs**, **caching.rs**,
  **operations.rs**. Project and publish: **visible_rows.rs**, **path_index.rs**, **name_filter.rs**, **diff.rs**,
  **diff_emitter.rs**. Sort: **sorting.rs**, **collation.rs**. `FileEntry` comes from `cmdr-fs`.

## Invariants and gotchas

- **Neither a row number nor a path indexes `entries`.** Rows drop hidden, filtered-out, and scratch entries, so
  `CachedListing::rows` is the ONLY filter point, on READ; by-path callers go through `index_of_path` /
  `indices_of_paths`. ❗ A MUTATING caller resolves BEFORE `entries_mut`, which drops both maps. `entries` stays
  private: accessors that grew their own filter were each a row off.
- **Mutation owners allocate revisions and enqueue stamped batches under the cache write lock**, cache then queue.
  The emitter coalesces transport, never transition boundaries or revisions. Indices are pane rows, not entry indices.
  Rows use committed exact-path scratch decisions. Reconcile and publish scratch drift BEFORE an entry mutation or
  guarded index consumption, then check the expected revision. `DETAILS.md` § "Diff event coalescing".
- **Quick-filter changes share that revision** and drop superseded pending batches under the cache write lock,
  never after unlocking. Guard selection remapping before changing the filter. `DETAILS.md` § "The quick filter and in-flight diffs".
- **Refreshes of ONE directory stay serialized** (`notify_full_refresh`), or an older read lands last.
- **`listing_overlays::decorate` folds in rows no volume holds**, between enrich and the sort, in all THREE read paths
  (`streaming.rs`, `operations.rs`, the watcher's full refresh); miss one and a refresh strips them.
- **A close notifies `crate::listing_lifecycle` AFTER the cache removal**, ❌ never before: an observer's detached arm
  reconciles against cache membership.
- **The orphan reaper keys on `last_accessed_ms`**, bumped by reads, cache patches, and the panes'
  `keep_listings_alive` heartbeat (idle panes make no reads). ❌ Never from `refresh_listing_index_sizes`.
- **`read_directory_with_progress` holds a `priority::foreground` lease** for its whole body: ❌ never bind it to `_`.
- ❌ **A listing never aborts a read**, on cancel or when a retry wins: detaching lets it unwind; aborting wedges an
  MTP phone. A read quiet for `stall_after` emits `listing-stalled` and keeps waiting, ❌ never a deadline that ends
  it. `DETAILS.md` § "Stalled listings".
- **FullRefresh goes through `caching::spawn_full_refresh`**: on a watcher's OS thread a bare `tokio::spawn` panics.
- **Sorting has ONE comparator**, `entry_comparator` over `SortableEntry` (`FileEntry` plus a search-results row). ❌
  Never add a second. A sort change invalidates the frontend's cached range, so bump `cacheGeneration`.
- **Names rank by Unicode collation (`collation.rs`), ❌ never code points or `to_lowercase`** (macOS holds NFC and NFD
  side by side). Both readings, live `compare` and prebuilt `key`, end with a raw-bytes tiebreak, else two spellings of
  one name tie and the watcher sees a phantom `Move`. ❌ Never persist a `NameKey`.
- **A listing's path is a `ListingPath`, built only by `ListingPath::on_volume`** (the volume's one spelling,
  `Volume::listing_path`). ❌ Never compare a raw path to it: MTP reports `mtp://…` where the pane holds `/DCIM`.
- **Only a pane open or a Finder drop/paste resolves a foreign spelling** (`list_as_stored`, `stored_spellings`): ❌
  never a walker, scan, or refresh, where a miss means gone and a resolve returns a look-alike twin. `DETAILS.md` § "A
  pane path the volume stores another way".
- **New listing state hangs off a struct, not a `static`**; fixtures use `caching_test_support::TestListing`.
- **Finder tags are deferred**: `list_directory_core` never reads them, and every modify path calls
  `carry_forward_tags` BEFORE storing, else an mtime touch blanks a file's dots. ❌ Never route enrich through it.

Read `DETAILS.md` before non-trivial work here.
