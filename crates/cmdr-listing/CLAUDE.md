# cmdr-listing

The directory listing engine: streaming reads, the listing cache, sorting, pane rows, and diffs, for 100k+ entries.
Tauri-free; Cmdr hosts it from `apps/desktop/src-tauri/src/listing_host/`.

## Module map

- **lib.rs** (`Inner`, the engine's state, and the API re-exports), **engine/** (`Listings`, one file per caller group),
  **host/** (the seams, see `src/host/CLAUDE.md`), **testing/** (fixtures).
- Read: **streaming.rs**, **stall.rs**, **pipeline.rs**, **reread.rs**, **refresh.rs**, **foreign_path.rs**. Cache:
  **cached_listing.rs**, **caching.rs** (lookups, change intake), **mutate.rs**, **authoritative.rs**,
  **operations.rs**. Rows: **visible_rows.rs**, **path_index.rs**, **quick_filter.rs**, **diff.rs**,
  **diff_emitter.rs**. Sort: **sorting.rs** (names collate through `cmdr-collation`).

## Must-knows

- **The host is a value.** New state is an `Inner` field, ❌ never a `static`. An app concern arrives through a `host`
  seam, called per read or mutation (never per entry) and never under an engine lock. `with_rows` is the one closure run
  under the cache lock; it never re-enters `Listings`. Background loops hold `Inner` weakly, so a dropped engine ends.
- **Spawn, sleep, and read the clock through `crate::rt`**: a bare `tokio::spawn` panics on a watcher's OS thread
  (banned in `clippy.toml`), and a direct `tokio::time` or `std::time::Instant` panics in the browser build.
  `DETAILS.md` § "The browser build".
- **Gate test scaffolding on `any(test, feature = "testing")`**, ❌ never `cfg(test)` alone: host tests need it.
- **Neither a row number nor a path indexes `entries`.** `CachedListing::rows` is the ONLY filter point; by-path callers
  use `index_of_path` / `indices_of_paths`, and a mutating caller resolves BEFORE `entries_mut`, which drops both maps.
- **Only `Publish` (borrowed from `write_cache`) enqueues diffs.** Reconcile scratch drift before an entry mutation or a
  guarded index read, then check the expected revision; quick-filter changes share that revision. `DETAILS.md` § "Diff
  event coalescing".
- **No full re-read lands after a newer one** (§ "Read generations"). Overlays fold in only through `Prepared`, and
  observers hear a close AFTER the cache removal.
- ❌ **Never abort a read**: cancel detaches (an abort wedges an MTP phone), and a stall reports and keeps waiting.
- **ONE comparator**, `entry_comparator`; a sort change bumps the frontend's `cacheGeneration`. Names collate through
  `cmdr-collation`, whose must-knows apply here (`crates/cmdr-collation/CLAUDE.md`).
- **A listing stores the REQUESTED column; only `CachedListing::effective()` feeds a comparator.** A date the volume
  doesn't record sorts by Modified (`EffectiveSort::resolve`), ❌ never written back as the request. `DETAILS.md` §
  "Requested and effective sort".
- **A listing's path is a `ListingPath`** from `ListingPath::on_volume`; ❌ never compare a raw path to it (MTP).
- **Only a pane open, a Finder drop/paste, or Ask Cmdr's live `list_dir` resolves a foreign spelling**, ❌ never a
  walker, scan, or refresh.
- **The orphan reaper keys on `last_accessed_ms`**, which index-size writes never bump.
- **Finder tags and macOS-reported packages are deferred**: every modify path calls `carry_forward_deferred` BEFORE
  storing.

Data flow, decisions, the capped surface, and every rule's why: `DETAILS.md`. Read it before any non-trivial work here:
editing, planning, reorganizing, or advising.
