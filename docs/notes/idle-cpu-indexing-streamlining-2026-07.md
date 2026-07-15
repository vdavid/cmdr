# Idle-CPU + indexing streamlining (2026-07, #37)

Issue [#37](https://github.com/vdavid/cmdr/issues/37): Cmdr burned CPU constantly while idle on macOS. Investigation
found the idle CPU was NOT one cause but a stack, and RSS was mostly a red herring. This note records what each change
achieved with numbers, so the wins aren't misattributed.

## Root-cause stack (measured, not guessed)

Cmdr live-watches the entire boot volume with file-level FSEvents. On any running Mac, background apps write constantly
(caches, app-state SQLite DBs, prefs, temp/lock files), so the reconciler + writer never idle. On top of that firehose
sat two amplifiers:

1. **Importance full-recompute loop** (fixed earlier in `cc6681c00`, before this effort): every live `dir-changed` batch
   carries the bare root `/` (universal ancestor), and the incremental treated `/` as a full-refresh sentinel → a full
   ~750k-folder re-score every ~2 min, back-to-back, pegging a core. Fixed by dropping `/` at the incremental boundary +
   throttling incrementals to ≤1/60 s.
2. **Importance incremental subtree-clear** (this effort, L1): even after `cc6681c00`, each throttled incremental still
   pegged a core inside one SQL statement. See below.

The per-event index-write pipeline itself was measured at ~99.9% idle (writer `time_in_processing_ms=3` per 5 s), so it
was never the idle-CPU driver — but it does redundant disk work, addressed by M1.

**RSS:** the reporter's ~1.4 GB was mostly WebView `IOAccelerator` GPU surfaces (~662 MB) + shared library pages; the
indexing engine's real `phys_footprint` was ~250 MB. Not an indexing leak. Out of scope here; flagged for a separate
WebView-GPU-on-background look.

## L1 — importance folded-key (the idle-CPU headline for this effort)

The incremental's subtree-clear `DELETE FROM weights WHERE path = ?1 OR path LIKE ?2 || '/%'` ran against a
`WITHOUT ROWID` table whose PK was `path TEXT PRIMARY KEY COLLATE platform_case` (a custom NFD+case-folding collation).
A custom collation on the key defeats SQLite's b-tree range/LIKE optimization → the DELETE full-scanned ~166k rows and
re-ran the NFD-folding comparison on each, per changed prefix. CPU profiling (`sample`) put the entire pegged core here
(`sqlite3MemCompare` → `unicode_normalization::decompose_canonical`).

Fix: a precomputed `path_folded` BINARY primary key (`= normalize_for_comparison(path)`, the same fold the collation
applied) + verbatim `path` kept as a plain column. The subtree-clear became an index-served range.

**Measured (release, synthetic 118,682-dir index, 106,622 weight rows):**

- `EXPLAIN QUERY PLAN` of the subtree-clear: `SCAN weights` (full scan) → `SEARCH weights USING PRIMARY KEY`
  (index-served). This is the crisp proof.
- One `apply_incremental` (index-served DELETE + re-insert + flush): **~0.015 ms** (leaf and mid-subtree alike), down
  from the multi-second full-scan the profile caught. ~5 orders of magnitude.

No migration (importance DB is a disposable cache; `SCHEMA_VERSION` 2→3 delete-and-recreates only `importance-*.db`).
Ranking unchanged (score is pure Rust; row identity byte-identical; search ranker uses verbatim-path `HashMap` lookup).

## L2 — targeted subtree walk: DEFERRED (measured, not worth it)

The incremental also walks all ~166k dirs before rescoping (`walk_index_folders`), on a `spawn_blocking` worker the
profile never showed hot. Measured to decide, not assume.

**Measured (release, 118,682 dirs):** the full walk is **~284 ms**, but it fires at most **once per 60 s** (the
throttle), i.e. **~0.7% of one core** averaged — and L1 already made the co-located write free. That matches the profile
(walk thread never hot). Replacing it with a targeted subtree walk carries real cross-boundary correctness risk (a
floored ancestor above the changed subtree, a marker deep below), which isn't worth shaving an already-invisible
sub-1%-duty-cycle cost. Deferred; revisit only if a very large volume (≈1M dirs) makes the walk a visible fraction.

## M1 — live per-file index-write throttle (resource-respect, not the CPU headline)

Not an idle-CPU fix (the write layer was already idle). It cuts redundant index DISK writes for the "same file rewritten
rapidly" pattern (WALs, logs, app-state DBs) and tames Cmdr's own DB/log self-write feedback loop (its data dir sits
inside the watched, indexed tree). Leading + trailing throttle (not debounce), 60 s window per file, 2% + 512 KiB
significant-jump bypass, `~/Downloads` exempt. Backend-only (a listed file's pane size comes from the live `lstat`, not
the index, so there's nothing to mark).

**Measured (integration test, real reconciler over a temp index):** 1 create + 50 rapid in-window rewrites → **1
in-window index write** (the leading edge; all 50 suppressed) + 1 trailing flush at window end = **2 writes/window vs 51
unthrottled (~25×)**. Each avoided write also skips its ~8-10-level ancestor `dir_stats` propagation, so the WAL-churn
saving is larger than the raw write ratio.

## M2 — search-index load arena right-sizing

`load_search_index` pre-allocated a fixed `String::with_capacity(100_000_000)` (~100 MB) + 5M-slot `Vec` on every load,
regardless of index size. Now count-driven (`SELECT COUNT(*)`, clamped to a 512 MiB arena ceiling). A small index
allocates a few KB instead of ~100 MB; correctness unchanged (both still grow if the estimate runs low).

## Net

Idle CPU: the importance loop (`cc6681c00`) + L1 remove the two core-pegs; the residual walk is <1% duty cycle (L2
deferred with numbers). Disk: M1 cuts the rapid-rewrite write firehose ~25×. Memory: M2 drops a fixed ~100 MB per-load
allocation; the reporter's large RSS is a separate WebView-GPU matter, not indexing.
