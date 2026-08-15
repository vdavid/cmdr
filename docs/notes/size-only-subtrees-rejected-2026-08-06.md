# Rejected: size-only subtrees (2026-08-06)

**Decision: not doing it.** The proposal was to stop storing a row per file under `CACHEDIR.TAG`-marked subtrees
(`target/`, package caches) and keep only the folder's totals, to cut index write CPU under build churn. Measurement
killed the case. The plan doc is deleted; this is what's left of it.

This exists so nobody re-proposes it from the same premises. If you want to revisit it, the § "What a revival has to
solve first" is the bar.

## The measured case against

**CPU, the whole argument, is ~1%.** The plan's central number was "the live write path costs 34.1 µs per row". That is
an UNOPTIMIZED-build figure. Re-running the plan's own probe (`writer_upsert_throughput`) reproduces 34.072 µs under
`cargo test` and **7.375 µs under `cargo test --release`**, which is what Cmdr ships. At the release rate, the row
writes this feature would have deleted are **~1% of the CPU the app spends under real build churn** (1.3 s of 129.8 s
over 20 minutes, 63 workspace rebuilds). The rest goes to receiving events, coalescing, `stat`ing, and discarding, none
of which the feature removes.

> **The transferable lesson: benchmark in release.** A debug-build µs figure is worthless for arguing about shipped CPU,
> and here it was wrong by 4.6x in the direction that made a feature look worth building. The 34.1 µs originated in the
> earlier resource-use work and the plan inherited it in good faith; nobody invented it to win an argument. Which is the
> point: a stale number propagates by citation, so re-measure the load-bearing one yourself.

**Disk is ~120 MB of a 987 MB index (~12%),** measured by deleting exactly those rows from a `VACUUM INTO` snapshot and
vacuuming again. Real, small, and never the reason to do this.

**RAM is transient, not steady-state.** `IDLE_TIMEOUT` (`apps/desktop/src-tauri/src/search/volumes.rs`) drops every
loaded arena five minutes after the search dialog closes, so the rows cost resident memory only while someone is
actively searching.

## The hardlink finding, which is load-bearing for any revival

On the real index: **81 681 multi-link inode families, and 92.2% of them have partners in a DIFFERENT directory.**

The index's dedup rule is a global one: an `entries` + `idx_inode` lookup that stores `logical_size = NULL` for a repeat
inode, so the bytes are counted once across the whole volume. **Per-directory summing cannot reproduce that**, because
for nine families in ten the other names live somewhere else entirely. Any future design that deletes file rows has to
solve this first, and "sum within the folder" is not the solution.

The cheaper alternative was checked and also fails: **55.3% of file rows under marked subtrees are multi-link members**,
so "keep rows only for multi-link files" keeps most of the rows and saves close to nothing.

## Two ideas worth inheriting

**1. Mark on observed CHURN, not on a filename convention.** The marked set turned out to be much broader than build
output: of 46 `CACHEDIR.TAG` roots, 31 weren't build output at all, and `~/.cargo/registry` plus `~/.cache/uv` plus a
`.venv` are ~189 000 rows of dependency SOURCE, where "nobody wants to search this" is a weak claim. Churn is the honest
signal: it excludes write-once caches like `~/.cargo/registry` by construction, and it matches the founding constraint
of the whole resource effort, which is recognize and throttle. The machinery already exists and only logs today:
`crates/cmdr-index/src/indexing/reconcile/reconciler/rescan/churn.rs`, and
`crates/cmdr-index/src/indexing/watch/churn_monitor.rs` already rolls up the ancestor chain.

**2. `CACHEDIR.TAG` needs a PREFIX test, not first-line equality.** The standard specifies the first 43 bytes. Of 31
real tag files on this machine, **6 carry the signature twice with no newline between**, so an equality test on the
first line rejects them. Anything that ever detects these tags has to read the first 43 bytes and compare a prefix.

## The search arena: the most valuable surviving lead

`load_search_index` (`apps/desktop/src-tauri/src/search/index.rs`) loads all ~6M rows unfiltered into an arena.
Shrinking what a row costs there is a better lever than not storing the row at all. **All figures below are struct
arithmetic, not measurement.**

- **✅ Done: sentinel-encoding the two `Option<u64>` in `SearchEntry`.** The row is 40 bytes, the arena is 92 MiB
  cheaper, and the scan is no slower. Measurements and the A/B method: `search-arena-row-2026-08-06.md`.
- **⚠️ Removing `id_to_index: HashMap<i64, usize>` is NOT a free win.** It has four production call sites in
  `search/engine.rs` (lines 70, 98, 498, 540), all ancestor walking for scope and path building, so it's hit once per
  ancestor per candidate inside an interactive loop. Binary search trades ~144 MB for ~23 cache-missing comparisons per
  lookup and has to be measured on BOTH axes before anyone calls it a win.
- **The better first experiment on that map**: entries already arrive rowid-ordered (`id INTEGER PRIMARY KEY`), so a
  sorted `Vec<(i64, u32)>` at 12 B/row (or an offset table exploiting id contiguity) likely gets most of the memory
  while staying O(1)-ish.

## APFS clones over-count sizes. A finding, NOT a plan.

Recorded because it's real and user-visible. **David has decided against this complexity for now**; this is not a
backlog item.

`physical_size` over-counts clones today. `bulk_read.rs` requests `ATTR_FILE_ALLOCSIZE`, which reports the FULL
allocation for every clone, so three clones of a 200 MB file each report `209715200` and `du -sh -c` says 600M for 200M
of real data.

**The blast radius is user-facing, not internal**: `physicalSize` reaches `SelectionInfo.svelte`, the size column via
`measure-column-widths.ts`, and the `sizeDisplayMode` switch in `full-list-utils.ts` including `recursivePhysicalSize`
for directories. And **Finder's Duplicate uses `clonefile` on APFS**, so ordinary users hit this by duplicating a
folder. It is not an artifact of one developer having five `target/` copies. Hardlink attribution is arbitrary in the
same family of ways: one folder shows the bytes, its siblings show zero.

Reference material if anyone picks it up:

- `ATTR_CMNEXT_CLONEID` and `ATTR_CMNEXT_CLONE_REFCNT` ride along free in the existing `getattrlistbulk` call.
- `CLONE_REFCNT` is a `u32`, not a `u64`.
- `ATTR_CMNEXT_PRIVATESIZE` costs +120% on the scan, so it stays off the scan path.
- **Totals dedup must stay keyed on INODE, never on clone id.** A clone is a distinct file that legitimately holds its
  own logical bytes; deduping on clone id would under-count real data.

## What a revival has to solve first

1. A CPU argument that isn't ~1%, measured in RELEASE, against the whole cost of churn rather than the row writes alone.
2. Global hardlink dedup without per-file rows, given 92.2% cross-directory partners.
3. A marking signal that doesn't silently swallow dependency source people search.

## The instruments, which outlived the plan

Both are committed and generic, so a re-measurement is a re-run:

- `index-size-probe`: rows, bytes, fan-out, size distribution, and vacuum-reclaim, all safe against the live index.
  Usage is in `docs/tooling/index-query.md`. It carries no trace of this proposal: any subtree it reports on is named
  with `--scope`, so re-measuring the slice above means passing those paths rather than re-deriving them from tags.
- `scripts/churn-baseline`: CPU, memory, rows written, and log volume across an idle control and a churn phase, with the
  root writer thread's own CPU read off its heartbeat. It carries no trace of this proposal either: the subtrees whose
  rows it counts separately are named with `-scope-roots` (a file of paths, one per line), so reproducing the split
  above means listing those paths.
