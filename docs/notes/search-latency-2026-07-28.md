# Why search felt slow (2026-07-28)

Investigation of "the search is extremely slow". Measured on David's machine (M-series Mac, root index
6.96 M entries / 1.1 GB, importance map 161 k scored folders), with the harness in
`apps/desktop/src-tauri/src/search/bench.rs`.

## The short version

Two costs dominated, and neither was the actual scan (the rayon filter runs in 100–400 ms even on a
7 M-entry index):

1. **The arena reloaded in front of nearly every search.** Root's in-memory arena stamps
   `WRITER_GENERATION` at load, and any mismatch meant "rebuild before answering" — a 2.6 s pass over
   6.3 M rows (warm page cache; 6–18 s cold). The root writer bumps that counter on EVERY mutation, and
   a live-watched boot disk mutates several times a second, so the stamp was stale within moments of
   every load.
2. **Ranking touched every match, allocating.** The importance blend reconstructed a parent path
   `String` per candidate (and `classify_match` allocated two more), then fully sorted all of them to
   show 30. A one-letter query matches 4.6 M of 7 M entries, so this was ~11 s of work per keystroke
   burst.

Both are fixed; numbers below.

## Field evidence (prod log, before the fix)

From `~/Library/Logs/com.veszelovszki.cmdr/cmdr.log.2`, one dialog session:

```
20:31:28.875 FE:user-action  search.open
20:31:32.349 search::index   Search index loaded: 6524858 entries, generation 543485, took 3.551102s
20:31:35.236 search::index   Search index loaded: 6524882 entries, generation 543527, took 2.760654917s
20:31:35.371 search::engine  Search completed: … → 1 matches (returning 1), took 53.425625ms
20:31:46.329 search::index   Search index loaded: 13541261 entries, generation 543560, took 10.955199875s
20:31:47.164 search::engine  Search completed: … → 0 matches (returning 0), took 114.477417ms
```

Read it as: 3.5 s to open, then the FIRST search threw that arena away and rebuilt it (2.8 s) because 42
writer mutations had landed in the meantime, then scanned in 53 ms. The 11 s line is the unscoped
fan-out loading a second volume's index.

A later session (`cmdr.log`, 2026-07-28) shows generation 167897 → 168406 in 89 s: **~5.7 writer bumps
per second while idle**, so the "is it fresh?" check essentially never passed.

## Bench: the engine's phases

`search_ranked` timed three ways per query: `count_only` (scan alone), full run with an EMPTY weight map
(scan + rank + materialize), full run with the REAL map (adds the importance blend).

Real root index, 6.96 M entries, 161 k scored folders (before → after):

- rare literal, 0 matches: 181 ms → 176 ms
- `report`, 5,243 matches: 244 ms → 141 ms
- `*.pdf`, 14,053 matches: 231 ms → 135 ms
- `e` (one letter), 4,607,821 matches: **11.88 s → 0.41 s** (29×)

Synthetic 3 M-entry index (deterministic, `bench_synthetic`), same shape:

- `report`, 168,750 matches: 325 ms → 191 ms
- `*.pdf`, 328,140 matches: 445 ms → 171 ms
- `e`, 2,193,749 matches: 3.23 s → 0.53 s

Phase split before the fix, on the one-letter query against the real index: scan 0.36 s, rank
2.74 s, importance blend 8.77 s. The blend was ~75% of the whole search.

## What changed

- **`ranking::rank_decorated`**: per-thread folder→weight memo (matches cluster hard by folder), the
  folder path hashed straight off the parent chain via `PathHasher` instead of being materialized as a
  `String`, an allocation-free `classify_match` for ASCII names, a rayon-parallel decorate pass, and
  `select_nth_unstable_by` + sort of the top-k instead of a full sort of every match. Ordering is
  unchanged: the comparator is a total order (unique entry id tiebreak), so the top-k set and its order
  are unique.
- **`search::volumes::get_loaded`**: a stale-but-warm arena now SERVES the search and refreshes in the
  background, at most once per `REFRESH_MIN_INTERVAL` (30 s). The arena was always a snapshot; the old
  policy paid seconds to shrink staleness from "seconds" to "milliseconds" in front of a
  keystroke-debounced dialog.

## Still open (not fixed here)

- **Unscoped search fans out to EVERY indexed volume**, loading each one's arena synchronously inside
  the search. The dialog pre-loads only root, so the first unscoped query pays the NAS load (10.9 s in
  the log above) and its RAM. Options: pre-load the other volumes in the background on dialog open, load
  the volumes in parallel, or make an unscoped search root-only unless the user asks for more (the
  frontend docs already describe filename search as root-only, so the backend and the docs disagree).
- **The same NAS is indexed twice**, once per address it was mounted from
  (`index-smb-100-127-48-122-445-naspi.db` and `index-smb-192-168-1-111-445-naspi.db`, 2.6 M entries and
  ~525 MB each). An unscoped search scans both and merges duplicate hits.
- **`QueryDialog.executeQuery` swallows search errors** (`catch {}` around `config.runQuery()`), so the
  engine's "Query too broad. Add a filename pattern, size, date, or type filter" rejection renders as a
  plain empty result. Worth surfacing.
- **Cold arena load is 6–18 s** from a cold page cache for a 1.1 GB index DB (2.6 s warm). Nothing
  reloads it now, but the first dialog open of a session still waits on it.
