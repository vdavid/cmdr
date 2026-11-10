# Search arena snapshot: map the index instead of loading it

**Status**: deferred, not started. Tracked as GitHub #114. The cheap half of this shipped separately (see "What already
shipped"); what's parked here is the memory-mapped arena itself. If it gets built it ships as ONE change, built by
parallel agents and merged at the end.

**The one question that gates it**: is roughly a second of wait on every search-dialog reopen past the 30-second idle
window, plus a few seconds on the first open of each session, worth ~3,000-4,500 lines, ~271 MB of disk per volume, and
permanent ownership of a file format with a crash-recovery path? Everything below is what "yes" costs.

## The problem

Opening the search dialog on a large boot volume costs tens of seconds before anything is searchable.

Field evidence, `ERR-S76V3` (`getcmdr/cmdr-reports#18`, 0.46.1, macOS 27.0, Mac15,9, 64 GB RAM, app up 312 s, first
search of the session). Log timestamps are local:

```
00:13:37.940  FE:user-action  search.open
00:14:09.547  search::index   Search index loaded: 5388928 entries, generation 213044, took 31.604403875s
00:14:12.390  search          importance weights loaded for 'root': 198946 scored folders
00:14:12.613  search::engine  Search completed: "SONY ILCE-5000.dcp" in "$HOME" → 1 matches, took 50.052875ms
```

**34.7 s of waiting for a 50 ms search.** The index writer burned 15 ms of CPU across that window and 16 cores sat idle,
so this is neither contention nor compute: it is one thread issuing ~200,000 serial 4 KiB `pread`s against a cold 830 MB
`index-root.db`, then decoding 5.39 M rows into a ~466 MB heap arena, then reading a second database.

Four costs stacked in the loader as it was then (`search/index/load.rs::load_search_index` and
`search/volumes.rs::load_volume_blocking` today); § "What already shipped" says which are fixed:

1. **One thread, no read-ahead.** One connection, one statement, ~14-26 MB/s effective against an SSD that does GB/s. No
   `PRAGMA mmap_size` is set anywhere in the repo, so the kernel never reads ahead either.
2. **The scan reads far more bytes than it uses.** `entries` carries `name_folded` (a full second copy of every
   filename, which this loader deliberately does not read), plus `physical_size`, `is_symlink`, `inode`, `listed_epoch`,
   and `unreadable_cause`. A table scan pays for every one of them.
3. **`COUNT(*)` is a second full b-tree traversal.** The comment calling it "a cheap b-tree count" is true warm and
   false cold. It exists only to size a `Vec`.
4. **The importance weights load runs on the line after the arena**, same thread, different database: 2.84 s.

## The shape of the fix

Stop loading the index and start **mapping** it. A read-optimized, columnar, memory-mapped snapshot file sits beside
each volume's index database. The engine scans it in place. An append-only mutation journal, written by the index writer
and replayed at dialog open, keeps it fresh.

### What already shipped

The cheap half landed on its own and moved the baseline, so read the rest of this spec against **that** rather than
against the `ERR-S76V3` numbers above. It kept the heap arena and made building one cheap:

- The loader is a parallel rowid-range scan instead of one thread.
- The row estimate comes from `dir_stats(ROOT_ID)` rather than a second full b-tree traversal.
- `load_weights` runs concurrently with the arena rather than on the line after it.
- `id_to_index` is gone: rows arrive in rowid order, so `index_of_id` binary-searches them. ~143 MB saved.
- `prepare_search_index` takes a volume id, so selecting a volume warms it.
- `IDLE_TIMEOUT` is 30 s rather than 5 min.

So the arena is already dropped when the dialog is down, and the ~5 MB-when-idle requirement is **already met**.

### Why this shape and not another

❗ **The memory argument is NOT what justifies this.** An earlier draft claimed the mapping was the only shape that
could hold a ~5 MB idle ceiling. That was wrong: the arena has always been dialog-scoped, loaded on open and dropped
after an idle timer, so idle footprint was never the problem. What the mapping actually buys, over the shipped cheap
version:

- **The wait disappears rather than shrinking.** The cheap version still rebuilds ~320 MB of heap arena whenever the
  idle window has passed. Mapping makes reopening free, forever, including the first open after launch.
- **The footprint while the dialog is UP drops too**, from ~320 MB of anonymous heap to a few MB plus reclaimable page
  cache. David explicitly said he does not need this ("while the dialog is up, the user wants to focus on search, so
  it's okay"), so count it as a bonus, not a requirement. Clean file-backed pages do not count against macOS
  `phys_footprint`, which is what Activity Monitor's "Memory" column and our own VM-map diagnostic report. ⚠️ That claim
  is **unverified in this codebase**; verify it before committing to the design, because it is load-bearing for this
  bullet.
- **The long read transaction goes away.** Today's multi-second load holds a SQLite read open, which blocks WAL
  checkpointing (`ERR-S76V3` shows `wal_checkpoint partial (608 of 2777 pages, blocked by readers)` right after it).
- **The cost side**: ~271 MB against an 830 MB database, roughly a third on top (disk accepted by David on 2026-09-20),
  plus a file format and a journal to own forever.

**Rejected alternatives**, so nobody re-proposes them mid-flight:

- _A covering SQLite index on the six loader columns._ Same disk cost, still page-at-a-time reads, still full decode
  CPU. Strictly worse for the same money.
- _`PRAGMA mmap_size` plus a 32 KiB page size on the index database itself._ Helps cold I/O with no extra disk, but
  keeps the full decode, so the wait shrinks instead of disappearing and the heap arena stays. Worth trying on its own
  merits if this spec is never built; it is cheap and it stacks with what shipped.
- _A `changed_gen` column plus an index on `entries` as the delta source._ Adds a column and an index to a 5.4 M-row
  table, so more disk and more write cost on every mutation, to replace a file we can delete.
- _Folding the importance weights into the arena file._ Importance recomputes independently of `entries` (the root
  recompute subscriber swaps root's map without rebuilding the arena), so coupling their lifetimes is wrong.

### Why it is safe

The snapshot and the journal are **derived data and never authoritative**. The index database remains the source of
truth. Any doubt at all (missing file, version mismatch, bad checksum, torn record, generation gap, names blob past its
addressable limit) falls back to reading the database, which after this change is the parallel loader and is itself
several times faster than today. The failure mode is "as fast as the improved fallback", never "wrong results".

## Loud rules

These are the ones an agent gets wrong by default. Every milestone is bound by all of them.

1. ❌ **Never modify an arena file in place.** Build `*.arena.tmp`, fsync, atomically rename. A live mapping keeps the
   old inode alive, so a reader mid-scan is unaffected. Truncating or rewriting a mapped file in place is a SIGBUS that
   takes the process down.
2. ❌ **Never put a search type inside `cmdr-index`.** The one-way rule in `search/CLAUDE.md` stands. The arena format,
   the builder, the journal, and the mapped reader are index concerns (a serialization of `entries` and a log of
   mutations to it) and live in `cmdr-index`. Matching, ranking, and query vocabulary stay app-side.
3. ❌ **No per-row hash lookup in the hot scan.** Shadowing of snapshot rows by journal rows is a **bitset over snapshot
   indices**, resolved once at map time. A `HashSet<i64>` consulted 5.4 M times per query is a regression dressed as
   correctness.
4. ❌ **Never let the arena answer a coverage question its journal tail cannot back.** `LoadedVolume::honors` keeps its
   current contract; the generation it compares is the **journal tail generation**, not the snapshot's own.
5. ❌ **Never leave an orphaned arena.** `clear_index`, forget-volume, and index-disabled all delete the arena and the
   journal alongside the database.
6. ❌ **Never build an arena in front of a user action.** The builder is background, idle-gated, disk-space-gated, and
   cancelable. A first run with no arena serves from the fallback loader and builds behind it.
7. ❌ **Never hand-edit a check allowlist number.** Run the check, commit the rewrite (`scripts/check/checks/DETAILS.md`
   § "Allowlist consent").
8. **Results must be bit-identical across backends.** The heap and mapped backends answer the same query over the same
   database with the same rows in the same order. This is pinned by a differential test, not by inspection.

## The contracts

Both are defined here so that agents in different workstreams can code against them in parallel and merge.

### Contract A: the on-disk arena format

One file per volume, `index-{volume_id}.arena`, beside `index-{volume_id}.db` in the app data dir. Little-endian
throughout. Sizes below are for the 5,388,928-row root index from `ERR-S76V3`.

**Header** (fixed 128 bytes, 8-byte aligned):

- `magic: [u8; 8]` = `CMDRARNA`
- `format_version: u32` (starts at 1; a reader that does not recognize it falls back, never guesses)
- `id_width: u8` (4 or 8, chosen at build time from `MAX(id)`; 4 while max id < `u32::MAX`, which it is at ~28.5 M
  today)
- `row_count: u64`
- `names_len: u64`
- `snapshot_generation: u64` (the index `search_generation` the builder read at)
- `coverage_token: u64`
- `built_at_unix: u64`
- `section_offsets: [u64; 8]` and `section_lens: [u64; 8]`
- `checksum: u64` over the header and every section

**Sections**, each 8-byte aligned, all sorted by `id` ascending (the builder scans in rowid order, so this is free):

| Section        | Type                                            | Bytes at 5.39 M rows |
| -------------- | ----------------------------------------------- | -------------------- |
| `ids`          | `u32` or `u64` per `id_width`                   | 21.6 MB              |
| `parent_ids`   | same width as `ids`                             | 21.6 MB              |
| `name_offset`  | `u32` into the names blob                       | 21.6 MB              |
| `name_len`     | `u16`                                           | 10.8 MB              |
| `is_directory` | bitset, one bit per row                         | 0.7 MB               |
| `modified_at`  | `u64` seconds since epoch, `u64::MAX` = unknown | 43.1 MB              |
| `logical_size` | `u64`, `u64::MAX` = absent                      | 43.1 MB              |
| `names`        | UTF-8, concatenated, not NUL-terminated         | ~108 MB              |

**Total ~271 MB.** Columnar is load-bearing, not cosmetic: a name match touches `name_offset`, `name_len`,
`is_directory`, and `names` (~141 MB, sequential, kernel-read-ahead friendly). `logical_size`, `modified_at`, and
`parent_ids` are touched for the surviving ~30 rows. An unused column costs disk and nothing else.

Guards the builder owns:

- `names_len > u32::MAX` means `name_offset` cannot address the blob. Refuse to build, log, leave the fallback in place.
  (At ~20 bytes per name this needs ~215 M rows, so it is a guard, not a plan.)
- `MAX(id) >= u32::MAX` selects `id_width = 8` rather than failing.
- `None` in `logical_size` is **meaningful** (a NULL is a hardlink-deduped row, not a zero-byte file) and must survive
  exactly, in both directions. Same sentinel discipline as today's `OptU64`, and the same reason.

❌ **Don't narrow `modified_at` to `u32`.** It saves 21.6 MB and costs correctness: `u32` seconds ends at 2106, and any
value past that would have to clamp, which silently changes what a date filter returns for a file carrying a far-future
or garbage mtime (a real thing off SMB and MTP, where the value is the server's rather than ours). The rule this change
holds itself to is that **every filter reads the same bytes it reads today**, so the width stays `u64` and the sentinel
stays `u64::MAX`. Negative values can't reach here: the scanner already maps a pre-1970 `tv_sec` to `None`
(`crates/cmdr-index/src/indexing/scanner/walker/bulk_read.rs`), which matters because `rusqlite`'s `FromSql for u64` is
a fallible `try_into` and a negative would abort the whole load.

### Contract B: the `SearchIndex` accessor API

`SearchIndex` stops being a struct with public fields and becomes a facade over two backends plus an overlay. Every
consumer goes through these methods; no caller indexes `entries` or touches `id_to_index` again.

```rust
pub struct SearchIndex { /* backend: Heap | Mapped, overlay, shadow bitset, generation */ }

impl SearchIndex {
    pub fn len(&self) -> usize;                       // snapshot rows + overlay rows
    pub fn is_empty(&self) -> bool;
    pub fn id(&self, idx: usize) -> i64;
    pub fn parent_id(&self, idx: usize) -> i64;
    pub fn name(&self, idx: usize) -> &str;
    pub fn is_directory(&self, idx: usize) -> bool;
    pub fn size(&self, idx: usize) -> Option<u64>;
    pub fn modified_at(&self, idx: usize) -> Option<u64>;
    pub fn is_shadowed(&self, idx: usize) -> bool;    // a bit test; skip in the scan
    pub fn index_of_id(&self, id: i64) -> Option<usize>;
    pub fn generation(&self) -> u64;                  // journal tail, not snapshot
}
```

Indices `0..snapshot_len` address the mapping (or the heap rows); `snapshot_len..len()` address the overlay tail. The
scan iterates the whole range and skips `is_shadowed`. `index_of_id` checks the overlay map first, then binary-searches
the mapped `ids` (which is sorted, which is why `id_to_index` disappears and takes ~143 MB with it).

`matcher.rs` currently takes a `&SearchEntry` plus its name slice; it takes `(&SearchIndex, idx)` instead. No matching
or folding logic changes: ❌ do not re-derive case folding or NFD normalization while you are in there.

### Where each filter's data comes from, before and after

The rule: **every filter reads the same bytes it reads today, from a different container.** Nothing moves between
sources, so nothing changes answers. Pinned by the differential test in M4.

| Filter                                             | Today                                                                          | After                                             |
| -------------------------------------------------- | ------------------------------------------------------------------------------ | ------------------------------------------------- |
| Name (substring, glob, regex, fuzzy)               | arena `names` + `name_offset`/`name_len`                                       | the `names` / `name_offset` / `name_len` columns  |
| Folders only / files only                          | arena `is_directory`                                                           | the `is_directory` bitset                         |
| **File** size (`min_size`, `max_size`)             | arena `logical_size`, NULL meaning unknown                                     | the `logical_size` column, same sentinel          |
| **Folder** size                                    | ❗ **not the arena**: `dir_sizes_for` reads `dir_stats` from SQLite per search | **unchanged**, still SQLite                       |
| Modified before / after                            | arena `modified_at`                                                            | the `modified_at` column, same width and sentinel |
| Scope (search under this folder)                   | ancestor walk over `parent_id` via `id_to_index`                               | same walk, `index_of_id` over the sorted `ids`    |
| Exclusions                                         | ancestor-id walk, same chain                                                   | same                                              |
| Ranking (importance blend, recency, match quality) | `hash_path` off the parent chain + the weights map                             | same chain, weights now mapped (M8)               |

The folder-size row is the one worth knowing: a directory's recursive size was never in the arena, so that filter
already goes to the database on every search, and this change does not touch it. The `C.md` guardrail stands unchanged:
a directory's size filter applies BEFORE ranking, and ❌ a read error never falls back to "no map", because the engine
reads that as "no filter".

### The journal

`index-{volume_id}.journal`, append-only, written by the index writer **after** the SQLite commit succeeds.

- Record: `len: u32`, `kind: u8` (upsert | move | delete), `generation: u64`, the row's fields for an upsert, `crc32`.
- Replay at map time builds the overlay (owned rows in a small arena) and the shadow bitset, by binary-searching each
  record's id in the mapped `ids`.
- **The staleness check that makes this safe**: the journal's tail generation must equal the index's current
  `search_generation`. If it does not (a hard crash between commit and append), the journal is not trustworthy, so the
  volume falls back to a full database load and a rebuild is scheduled. Rare, and it degrades to the improved fallback.
- A torn tail (short read, bad CRC) truncates the log at the last good record, which then trips the generation check.
- Observed write rate on David's live boot disk is roughly one row mutation per second (`ERR-S76V3` log:
  `Writer: +11 msgs (4 upserts, 1 delete, 6 others)` per 5-6 s), so ~86,000 records and ~5 MB a day.

**Rebuild threshold**: journal past 10% of `row_count` or past 64 MB, whichever comes first. Roughly daily on a busy
disk, which keeps write amplification honest: rebuilding 271 MB on a timer would be gigabytes of SSD writes a day, and
the resources principle exists to stop exactly that. Rebuild runs background, idle-gated, and only with free space to
spare.

## Milestones

Eight workstreams. They ship as one change, so ordering below is about dependencies and merge surface, not about
intermediate releases. Nothing here is thrown away.

Concurrency cap is ~3 agents in flight (`docs/guides/multi-agent-refactors.md`: a 14-agent burst gets rate-limited and
agents silently finish partial).

- **Wave 1**: M1, M2, M3
- **Wave 2**: M4, M5, M6
- **Wave 3**: M7, M8
- **Wave 4**: docs sweep, adversarial conformance review, integration

### M1. Characterization suite (no production code)

**Scope**: `apps/desktop/src-tauri/src/search/` tests only.

**Intentions**: pin what search answers _today_, so the backend swap is provably identical rather than hoped-for. Build
a synthetic index database (deterministic, a few hundred thousand rows, hardlink-deduped NULL sizes, unknown mtimes, NFD
names, deep trees, case-colliding names, excluded subtrees), run a battery of queries through the current engine, and
snapshot results including order.

**Landmines**: pin reality, not the plan's expectations. Verify the pins actually bite by breaking the engine on purpose
once and seeing them go red. The battery must include the shapes that exercise ranking ties, since order is part of the
contract.

**Test plan**: this milestone is the test plan. It must be green against `main`'s behavior before any other milestone
merges.

**DONE**: a query battery + result snapshots that later milestones run unchanged, plus a documented way to run the same
battery against an arbitrary backend.

### M2. Arena format, builder, journal, and mapped reader (`cmdr-index`)

**Scope**: new `crates/cmdr-index/src/indexing/arena/` (`format.rs`, `build.rs`, `journal.rs`, `map.rs`, `mod.rs`), plus
`CLAUDE.md` + `DETAILS.md` for it, plus the `Index` handle methods that expose it.

**Intentions**: own Contract A end to end. The builder does one parallel range-scan pass over `entries` (rowid ranges,
one read connection per worker, per-worker segments merged with name-offset rebasing), sizes itself from
`dir_stats(ROOT_ID)`'s `recursive_file_count + recursive_dir_count` rather than `COUNT(*)`, writes `*.arena.tmp`,
fsyncs, renames. `map.rs` opens, validates, and exposes typed column slices plus `madvise` helpers. `journal.rs` owns
the record format, the appender type, the replayer, and truncation on rebuild.

**Landmines**: rule 1 (atomic rename, never in place). Rule 2 (no search vocabulary in this crate). `id_width` selection
and the `names_len` guard. The `logical_size` NULL must round-trip as `None`, never `0`. `index-crate-isolation`
ceilings move here, and per `scripts/check/checks/DETAILS.md` § "Allowlist consent" that needs David's consent plus a
written rationale in `handle/DETAILS.md` and the commit body; the whole new surface gets designed and argued at once,
not bumped per method. `memmap2` is the mapping crate: 0.9.11, published 2026-06-22, 346 M downloads, repo alive (not
archived, last push 2026-08-20), Apache-2.0. Run `cargo deny check` when adding it.

**Test plan**: round-trip over a synthetic database; torn-tail truncation; bad CRC; wrong `format_version`; checksum
mismatch; `id_width` 4 and 8; empty index; a names blob near the `u32` limit (synthetic header, not 4 GB of data); NULL
`logical_size` survives; the builder's parallel merge produces byte-identical output to a single-threaded reference
build.

**DONE**: `cmdr-index` can build, map, validate, and journal an arena, with no app-side code involved.

### M3. The accessor seam (`search/`)

**Scope**: `crates/cmdr-search/src/arena.rs`, `crates/cmdr-search/src/engine.rs`, `crates/cmdr-search/src/ranking.rs`,
`crates/cmdr-search/src/matcher.rs`, `search/execute.rs`, `commands/search.rs`, `search/bench.rs`, and the tests that
build `SearchIndex` literals (`search/ranking/tests.rs`, `search/execute/tests.rs`, `search/volumes/tests.rs`,
`search/index/memory_tests.rs`).

**Intentions**: implement Contract B over the **existing heap backend only**, and migrate every caller to it. Behavior
identical, M1's battery green throughout. The public `entries` / `names` fields disappear. This is the seam every other
workstream merges into.

**Partly shipped**: `index_of_id` exists and `id_to_index` is already gone (binary search over the rowid-ordered rows),
so the four parent-chain call sites in `engine.rs` are done. What is left is the rest of the facade: `id`, `parent_id`,
`name`, `is_directory`, `size`, `modified_at`, `is_shadowed`, and retiring direct `entries[idx]` access.

**Landmines**: the comparator in the top-k sort tiebreaks on entry id and must stay a total order, or ranking order
changes. `bench.rs` builds synthetic indices by hand and needs a constructor rather than field access. `memory_tests.rs`
currently pins `size_of::<SearchEntry>() == 40`; that pin retires here and is replaced in M6 by a footprint pin on the
mapped path.

**Test plan**: M1's battery, unchanged, green. Existing search tests green with no assertion edits beyond construction.

**DONE**: nothing outside `crates/cmdr-search/src/arena.rs` knows how a row is stored.

### M4. Mapped backend and overlay (`search/`)

**Scope**: new `search/index/mapped.rs` and `search/index/overlay.rs`. Depends on M2's map API and M3's accessor API,
both defined above, so it can be written in parallel with them and merged.

**Intentions**: the `Mapped` backend over `cmdr_index`'s mapped columns, the overlay built from journal replay, and the
shadow bitset. `index_of_id` = overlay map, then binary search over the mapped `ids`.

**Landmines**: rule 3 (bitset, not a per-row hash lookup). Rule 4 (`generation()` is the journal tail). Widening `u32`
ids to `i64` on read, and comparing in the stored width during the binary search. The overlay's names are owned and live
in its own small arena; `name(idx)` dispatches on `idx < snapshot_len`. The shadow bitset is ~0.7 MB of anonymous memory
and exists only while the dialog is up.

**Test plan**: the **differential test** that rule 8 demands: for a given synthetic database, heap and mapped backends
answer M1's whole battery with identical rows in identical order. Plus overlay-specific cases: a row created after the
snapshot is findable; a deleted row is not; a moved row resolves under its new parent; a row updated twice replays to
its last state.

**DONE**: the mapped backend passes M1's battery and the differential test.

### M5. Parallel fallback loader (`search/`) — ✅ SHIPPED

Landed with the cheap version. Kept here because it is also this spec's fallback path: it is what runs when there is no
arena file, when the journal is rejected, or when a build has not finished. Nothing more to do; the notes below record
what it has to keep doing.

**Scope**: `search/index/load.rs`.

**Intentions**: the no-arena path (first run, post-upgrade, a rejected journal, a build that has not finished) is a
parallel rowid-range scan, not today's single thread. Row estimate from `dir_stats(ROOT_ID)` with a `COUNT(*)` fallback
only when that row is missing. Cancellation propagates across workers.

**Landmines**: the existing `CANCEL_CHECK_INTERVAL` semantics and the single-flight gate in `volumes.rs::ensure_volume`
must keep working. Per-worker name arenas merge with offset rebasing, same as M2's builder; the two are close enough
that sharing the merge helper is worth a look. A cancelled worker must not leave the others spinning.

**Test plan**: parallel and single-threaded loads produce identical `SearchIndex` contents; cancellation mid-load
returns promptly and leaks nothing; M1's battery green over the heap backend.

**DONE**: the fallback is fast enough that landing on it is a performance note, not an incident.

### M6. Journal write hook and rebuild scheduler (`cmdr-index`)

**Scope**: `crates/cmdr-index/src/indexing/writer/` call sites, plus the rebuild scheduler. M2 owns the journal's format
and types; this milestone owns _when_ they are called. Split that way so the two do not collide.

**Intentions**: append committed mutations after the SQLite commit succeeds, stamped with the generation. Detect the
rebuild threshold (10% of `row_count` or 64 MB) and schedule a background, idle-gated, disk-space-gated, cancelable
rebuild. Delete the arena and journal on `clear_index`, forget-volume, and index-disabled (rule 5).

**Landmines**: rule 6 (never in front of a user action). The append must not become a per-row fsync: batch it with the
writer's own commit cadence. A volume whose free space is tight skips the build and logs rather than filling the disk.
Non-root volumes (NAS, MTP) have writers too and get the same hook.

**Test plan**: a mixed upsert/move/delete workload leaves a journal that replays to exactly the database's state; a
simulated crash between commit and append is caught by the generation check; the threshold fires once, not repeatedly;
`clear_index` leaves no orphan files.

**DONE**: an arena stays fresh on a live volume without anyone opening the dialog.

### M7. Lifecycle, volume-scoped preload, and the wait UI

**Scope**: `search/volumes.rs`, `commands/search.rs`, `apps/desktop/src/lib/search/search-lifecycle.svelte.ts`,
`apps/desktop/src/lib/tauri-commands/search.ts`, the generated bindings, and `SearchDialog.svelte`.

**Partly shipped**: the volume-scoped `prepare_search_index`, the concurrent `load_weights`, the 30 s `IDLE_TIMEOUT`,
and the progress UI all landed with the cheap version.

**Intentions**: what is left here is the mapping's own lifecycle. Map on dialog open and on volume selection; unmap on
close. `IDLE_TIMEOUT` and `BACKSTOP_TIMEOUT` retire entirely in favour of unmap-on-close, since the page cache becomes
the warm-keeping mechanism. The progress UI stays, because the fallback path can still take seconds.

**Landmines**: `search-index-ready` already names its volume and the frontend already gates per volume; extend that, do
not duplicate it. The dialog-scoped lifecycle currently arms both timers on open and drops **all** arenas together;
unwinding it must not strand a run mid-walk. Cancelling a pre-load on dialog close (`cancel_active_loads`,
`CANCEL_EPOCH`) still has to work for the fallback path. The `loading` / `ready` contract of `prepare_search_index`
keeps its current meaning: `loading: false, ready: false` is the terminal "no index here".

**Test plan**: switching the focused pane's volume warms that volume's arena; closing the dialog unmaps; reopening
within seconds is instant; a machine with indexing off still gets the terminal answer and does not wait forever.
Footprint pin: with the dialog OPEN and a search running, the mapped path holds under 5 MB of anonymous memory. (The
dialog-closed half of that pin shipped with the cheap version.)

### M8. Importance weights, mapped

**Scope**: `crates/cmdr-index/src/importance/`, plus `search/volumes.rs`'s `load_weights`.

**Intentions**: the 2.84 s weights read is the new critical path once the arena maps in microseconds, so it gets the
same treatment at a much smaller scale: a mapped `importance-{volume_id}.weights` file (sorted `hash_path: u64` +
`score: f32`, ~2.4 MB at 198,946 scored folders), written by the importance writer on a full recompute and patched
incrementally, mapped by search. Kept **separate** from the arena file on purpose: importance recomputes independently
of `entries`.

**Landmines**: the reload contract in `crates/cmdr-index/src/importance/read/DETAILS.md` is canonical for what the
notices mean and this must not fork it. `for_each_nonzero_weight` filters `score > 0`; the mapped file should already
exclude zeros rather than making every reader re-filter. The existing "a full pass reloads, an incremental PATCHES"
split stays.

**Test plan**: mapped and SQLite-read weight maps are identical for the same database; a patch applied to the mapped
file matches a full rebuild; a missing or stale weights file falls back to the SQLite read.

## Invariants register

Checked by the end-of-phase conformance review, one agent, adversarial, against this list.

1. Heap and mapped backends answer identically, in the same order (M1 battery + M4 differential test).
2. `None` in `logical_size` and `modified_at` survives every hop as `None`, never `0`.
3. No arena file is ever modified in place.
4. `search/` holds no search vocabulary inside `cmdr-index`, and `cmdr-index` carries no `tauri`.
5. The hot scan does no per-row hash lookup.
6. `LoadedVolume::honors` compares the journal tail generation.
7. `clear_index` / forget / disable leaves no arena or journal behind.
8. With the dialog closed, search holds under 5 MB of anonymous memory (already true; must stay true), and with it OPEN
   the mapped path holds under 5 MB of anonymous memory too (the new part).
9. A corrupt, stale, or missing arena degrades to the fallback loader and never to a wrong answer.
10. No check allowlist number was hand-edited; every loosening beyond `file-length` / `claude-md-length` / `jscpd-*`
    carries David's explicit consent (`index-crate-isolation` is the one this change needs).

## Checks

Per `.claude/rules/check-scope-matches-change.md`, scope per milestone, not a blanket suite:

- Rust-only milestones (M1-M6, M8): `pnpm check rust` plus `pnpm check clippy`. Clippy is not in `--fast` and a lint
  error there blocks every other session.
- M7 touches the frontend: add the svelte and typecheck lanes, and the generated bindings.
- Per milestone: `pnpm check --fast` while iterating, plain `pnpm check` at the milestone.
- Before the merge back to `main`: `pnpm check --include-slow`, plus `desktop-e2e-linux` since M7 touches its surface.
- ❌ Never tail or truncate the checker's output.

## Docs to update

- New: `crates/cmdr-index/src/indexing/arena/CLAUDE.md` + `DETAILS.md`.
- Rewrite: `apps/desktop/src-tauri/src/search/CLAUDE.md` (the 40-byte row must-know retires; the storage model, the
  shadow bitset, and the fallback rule replace it) and `search/DETAILS.md` (§ "Waiting for a cold arena", § "Per-volume
  load", § "The arena row").
- `crates/cmdr-index/src/indexing/handle/DETAILS.md`: the `index-crate-isolation` ceiling rationale.
- `docs/architecture.md`: a pointer for the new module (map only, never how).
- `docs/notes/`: the `ERR-S76V3` measurement and the arena sizing land here as the evidence anchor.
- `docs/specs/DETAILS.md` § "The specs": this spec's entry.
