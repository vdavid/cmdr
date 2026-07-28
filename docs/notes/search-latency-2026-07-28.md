# Why search felt slow (2026-07-28)

Investigation of "the search is extremely slow". Measured on David's machine (M-series Mac, root index 6.96 M entries /
1.1 GB, importance map 161 k scored folders), with the harness in `apps/desktop/src-tauri/src/search/bench.rs`.

## The short version

Two costs dominated, and neither was the actual scan (the rayon filter runs in 100–400 ms even on a 7 M-entry index):

1. **The arena reloaded in front of nearly every search.** Root's in-memory arena stamps `WRITER_GENERATION` at load,
   and any mismatch meant "rebuild before answering" — a 2.6 s pass over 6.3 M rows (warm page cache; 6–18 s cold). The
   root writer bumps that counter on EVERY mutation, and a live-watched boot disk mutates several times a second, so the
   stamp was stale within moments of every load.
2. **Ranking touched every match, allocating.** The importance blend reconstructed a parent path `String` per candidate
   (and `classify_match` allocated two more), then fully sorted all of them to show 30. A one-letter query matches 4.6 M
   of 7 M entries, so this was ~11 s of work per keystroke burst.

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

Read it as: 3.5 s to open, then the FIRST search threw that arena away and rebuilt it (2.8 s) because 42 writer
mutations had landed in the meantime, then scanned in 53 ms. The 11 s line is the unscoped fan-out loading a second
volume's index.

A later session (`cmdr.log`, 2026-07-28) shows generation 167897 → 168406 in 89 s: **~5.7 writer bumps per second while
idle**, so the "is it fresh?" check essentially never passed.

## Bench: the engine's phases

`search_ranked` timed three ways per query: `count_only` (scan alone), full run with an EMPTY weight map (scan + rank +
materialize), full run with the REAL map (adds the importance blend).

Real root index, 6.96 M entries, 161 k scored folders (before → after):

- rare literal, 0 matches: 181 ms → 176 ms
- `report`, 5,243 matches: 244 ms → 141 ms
- `*.pdf`, 14,053 matches: 231 ms → 135 ms
- `e` (one letter), 4,607,821 matches: **11.88 s → 0.41 s** (29×)

Synthetic 3 M-entry index (deterministic, `bench_synthetic`), same shape:

- `report`, 168,750 matches: 325 ms → 191 ms
- `*.pdf`, 328,140 matches: 445 ms → 171 ms
- `e`, 2,193,749 matches: 3.23 s → 0.53 s

Phase split before the fix, on the one-letter query against the real index: scan 0.36 s, rank 2.74 s, importance blend
8.77 s. The blend was ~75% of the whole search.

## What changed

- **`ranking::rank_decorated`**: per-thread folder→weight memo (matches cluster hard by folder), the folder path hashed
  straight off the parent chain via `PathHasher` instead of being materialized as a `String`, an allocation-free
  `classify_match` for ASCII names, a rayon-parallel decorate pass, and `select_nth_unstable_by` + sort of the top-k
  instead of a full sort of every match. Ordering is unchanged: the comparator is a total order (unique entry id
  tiebreak), so the top-k set and its order are unique.
- **`search::volumes::get_loaded`**: a stale-but-warm arena now SERVES the search and refreshes in the background, at
  most once per `REFRESH_MIN_INTERVAL` (30 s). The arena was always a snapshot; the old policy paid seconds to shrink
  staleness from "seconds" to "milliseconds" in front of a keystroke-debounced dialog.

## The fan-out cost (2026-07-29)

The follow-up: the search that stayed slow wasn't the scan or the rank, it was the LOAD. An unscoped search resolved
every `index-*.db` to a target and loaded each arena SYNCHRONOUSLY, one after another, inside the search call.

Measured with `bench_volume_fanout` (the same harness, `CMDR_SEARCH_BENCH_DBS`), over David's three real index DBs
(root 6.99 M entries / 1.1 GB, plus the two 2.64 M-entry / 525 MB NAS indexes). The machine was under heavy multi-agent
load (16 cores, load average ~44), so read these as an upper bound with a compressed parallel speedup, not a quiet-box
baseline:

- cold-ish first read: root **10.6 s**, NAS **2.37 s**, NAS **2.46 s**
- warm page cache, all three: serial **4.11 s** → parallel **2.54 s**

So the first unscoped query after opening the dialog waited on 4.8 s of NAS arena loading with a warm page cache, and
the 10.9 s single load in the prod log above when cold. Three things fix it, none of which narrows what search covers:

1. **A cold volume no longer blocks the dialog's search** (`ColdVolumePolicy::DeferColdVolumes`). The run answers from
   the arenas already in memory and returns the cold volumes in `RunOutcome::deferred_volumes`; `search_files` warms
   each behind the reply and emits `search-index-ready`, which the dialog's existing listener turns into a re-run. So
   the NAS's matches fold in ~2.4 s later instead of freezing the first keystroke for 5–11 s. MCP still passes `Wait`
   (one shot, no re-run).
2. **Whatever a run does wait for loads in parallel** — 4.11 s → 2.54 s on the three-volume set above, and better on a
   quiet box (parallel loses more than serial to contention).
3. **Loads are single-flighted per volume.** A search arriving while the dialog's root pre-load is still running used to
   start a SECOND full read of the same DB (the 3.55 s + 2.76 s pair in the prod log above is that shape); it now joins
   the in-flight load.

## One NAS, two indexes (2026-07-29)

`index-smb-100-127-48-122-445-naspi.db` and `index-smb-192-168-1-111-445-naspi.db` are the same QNAP reached over
Tailscale and over the LAN: 2,643,852 vs 2,643,898 entries, `total_physical_bytes` 5,553,242,434,008 vs
5,553,638,575,402 (0.007% apart), and BOTH stamping `volume_path = /Volumes/naspi`. An unscoped search scanned both,
merged duplicate hits, doubled the match count, and held ~260 MB of arena twice.

An SMB volume id is `smb_volume_id(server, port, share)` off `statfs`'s `f_mntfromname`, so the address is the key. The
fix is read-time, not on-disk: `volumes::distinct_mount_roots_in` keeps one index per mount root (live-registered wins,
else the newest `scan_completed_at`) and skips the other. Nothing is deleted, so the skipped DB wins straight back the
moment it's the one mounted. On David's box this drops the unscoped fan-out from three volumes to two: one fewer 2.4 s
arena load and ~260 MB less resident.

Re-keying the index on a stable server identity was investigated and rejected (the id is derived at mount-detection time
with no SMB session, the smb2 crate exposes no volume serial, and re-keying orphans every existing SMB index for a
multi-hour rescan). Full reasoning: `apps/desktop/src-tauri/src/search/DETAILS.md` § Why the index isn't keyed on a
stable server identity.

## Still open (not fixed here)

- **`QueryDialog.executeQuery` swallows search errors** (`catch {}` around `config.runQuery()`), so the engine's "Query
  too broad. Add a filename pattern, size, date, or type filter" rejection renders as a plain empty result. Worth
  surfacing.
- **Cold arena load is 6–18 s** from a cold page cache for a 1.1 GB index DB (2.6 s warm). Nothing reloads it now, and
  nothing blocks a search on a cold NON-root volume, but the first dialog open of a session still waits on root's.
- **No resident-memory budget across arenas.** An unscoped search still ends up holding every indexed volume's arena
  until the idle timer fires (root ~690 MB + ~260 MB per NAS on David's box). The mount-root dedupe removed one copy;
  an LRU cap over the total would be the next step if it bites.
- **Nothing tells the user a volume is still warming.** The deferred results self-heal via the re-run, but for those
  couple of seconds the count is understated with no visible signal. A "still loading N volumes" note would need a new
  typed field alongside `uncovered_scopes` / `unresolved_scopes` plus frontend work.
- **A duplicate index DB is left on disk** (~525 MB here). Search ignores it; nothing reclaims it short of the
  32-external-DB retention cap. A "forget this index" affordance is David's call.
