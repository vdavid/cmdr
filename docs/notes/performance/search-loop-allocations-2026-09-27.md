# The search loop allocated per row, and what that cost (2026-09-27)

**What this settles:** follow-up #3 in `README.md` ("the search loop probably allocates per entry"). **Verdict: it did,
in three places, and fixing them made every query 2–10× faster and closed the system allocator's search penalty.** The
mechanism now lives in `apps/desktop/src-tauri/src/search/DETAILS.md` § "Scan cost: allocations per query, never per
row"; this note is the evidence. Raw numbers: `search-loop-allocations-2026-09-27.csv` (one row per round, binary, and
query).

## Where the allocations were

The dialog (`start_live`) and MCP (`run_live_collected`, `run_blocking`) both reach the same loop:
`execute.rs::search_covered_half`, then `engine::search_ranked`, then a rayon scan over the arena, then ranking.

1. **The regex cache pool, per scanned row.** The scan shared one `regex::Regex` across all rayon workers. Every
   `is_match` fetches a cache from the regex's pool. Only the first thread to use it gets the lock-free owner slot;
   every other worker goes through eight mutex-guarded stacks keyed by `thread_id % 8`, and one that loses its single
   `try_lock` builds a whole fresh cache and throws it away (`regex_automata::util::pool::Pool` in 0.4.16, its
   `get_slow` and `put_value`; read on 2026-09-27). With 16 workers on eight stacks, pairs collide constantly: **1.07 M
   allocations (mimalloc) and 1.28 M (system) for a query matching nothing**, varying 0.4–1.3 M run to run because it's
   contention. This is what made a no-match query slow down under the system allocator.
2. **The exclude fold, per ancestor of every match.** With the system/cache excludes on (the default) and case folding
   on (the macOS default), `ExcludeRules::excludes_dir_name` folded every ancestor's name into three fresh `String`s
   (`normalize_for_comparison`'s NFD collect and lowercase, then a second lowercase): **~48 allocations per match**, 92
   M for a one-letter query. The older benches never saw it, because they turn the excludes off.
3. **Ranking with importance weights, per folder.** `hash_path_from_index` collected a folder's path components into a
   fresh `Vec` on every memo miss, and `rank_indices` built its folder memo per rayon split with no floor on the split
   length, so the memos were many and cold: ~1.17 M allocations ranking a one-letter query.

## The fixes

1. The scan runs in 32,768-row chunks, each with its own clone of `CompiledQuery` and `ScopeFilter`. A cloned `Regex`
   gets a pool of its own, and the chunk's one thread owns it outright.
2. ASCII names fold on a stack buffer, byte-identical to the general fold (a test pins it). Non-ASCII names still take
   the general fold, on purpose (see "What's left").
3. Path components sit in a 64-deep stack buffer, and ranking splits are at least 8,192 matches.

## Method

- **Allocation counts**: `test_support::allocations_on_pool` runs the query on a fresh rayon pool as wide as the global
  one (16 threads) and counts every `alloc`, `alloc_zeroed`, and `realloc` its threads make. The counting allocator
  forwards to `System`; for the mimalloc binaries it was temporarily pointed at `mimalloc::MiMalloc` (the counter is
  identical either way).
- **Latency**: `search::bench::bench_query_allocations`, a full ranked search (limit 30) as the app asks: case folding
  at the platform default, system excludes on, and real importance weights (178,525 scored folders), on the global rayon
  pool. Seven timed runs per query per binary.
- **Data**: `sqlite3 .backup` snapshots of prod's `index-root.db` (**5,205,555 rows**) and `importance-root.db`,
  2026-09-27.
- **Four release test binaries** (thin LTO) built from the same bench source: before and after the fix (base
  `e9fd713ad`), each under mimalloc (the shipping allocator) and the system allocator.
- **Six interleaved rounds**, the binary order rotated each round. Load average 28–44 throughout (other agents
  building), on David's 16-core dev machine. Reported: the median of the six per-round medians, with the range of those
  medians.

## Results

Median ms [range across rounds]; allocations are the median per query.

| query               | matches   | before, mimalloc    | after, mimalloc         | before, system      | after, system           |
| ------------------- | --------- | ------------------- | ----------------------- | ------------------- | ----------------------- |
| no match            | 0         | 64.0 [56.7–67.0]    | **9.8** [8.6–10.6]      | 89.8 [62.2–101.1]   | **9.4** [9.1–10.3]      |
| rare literal        | 524       | 92.7 [85.6–94.8]    | **16.2** [14.7–16.6]    | 136.8 [81.7–149.6]  | **16.6** [15.5–17.9]    |
| word (`report`)     | 2,790     | 97.1 [91.6–104.7]   | **16.3** [14.4–17.3]    | 141.3 [92.7–150.8]  | **16.4** [15.0–17.4]    |
| extension (`*.pdf`) | 36,494    | 95.3 [84.5–104.4]   | **23.6** [22.2–25.2]    | 139.6 [92.4–152.1]  | **23.8** [22.6–24.9]    |
| one letter (`e`)    | 1,893,804 | 540.7 [515.8–546.4] | **277.3** [263.0–283.8] | 590.1 [575.5–639.4] | **273.6** [256.2–276.8] |

Allocations per full query:

| query        | before, mimalloc | after   |
| ------------ | ---------------- | ------- |
| no match     | 1,070,063        | 6,623   |
| rare literal | 1,406,401        | 7,702   |
| word         | 1,578,954        | 9,175   |
| extension    | 2,341,016        | 74,697  |
| one letter   | 93,035,144       | 272,072 |

- **Speedup under mimalloc**: 6.6× (no match), 5.7× (rare literal), 6.0× (word), 4.0× (extension), 1.9× (one letter).
- **The system allocator's search penalty is gone**: system ÷ mimalloc was 1.40, 1.48, 1.46, 1.46, and 1.09 before, and
  0.96, 1.02, 1.00, 1.01, and 0.99 after. That removes the throughput argument against the system allocator in
  `allocator-comparison-2026-09-23.md` (its "30–45% slower on search queries"). The transient-peak argument stands.
- **Most of the win was lock traffic, not allocation**: under mimalloc, where an allocation is cheap, the no-match query
  still went from 64 to 10 ms. Each `is_match` on a shared `Regex` took a mutex, and losing it built a cache.
- **After the fix, a query costs ~6,600 allocations regardless of arena size**: one regex cache per scan chunk (~160
  chunks at ~40 allocations each).

## Follow-up: a per-chunk memo of ancestor verdicts (2026-10-01, #322)

After the fixes above, two costs were left in the exclude check: non-ASCII ancestor names still folded through `String`s
(~1% of directory names; `*.pdf` 74,333 allocations with excludes against 6,983 without, one letter 216,000 against
8,700), and every match re-walked its ancestors (a binary search each) even when hundreds of matches shared the same
folders. An allocation-free fold was off the table (it would re-derive `normalize_for_comparison`'s rules in
`excludes.rs`, which `search/CLAUDE.md` forbids), so each scan chunk now keeps its own memo of folder verdicts
(`engine.rs`, `AncestorVerdicts`): each folder is judged once per chunk, and a walk stops at the first folder already
judged.

Method: `bench_query_allocations`, release test binaries before and after (mimalloc, the test binary's default), against
fresh `sqlite3 .backup` snapshots of prod's `index-root.db` (**5,611,289 rows**) and `importance-root.db` (174,379
scored folders), 2026-10-01. Four interleaved rounds, order alternating, seven timed runs per query each. Load average
38–46 throughout (other agents building). Median of the four per-round medians, ms:

| query               | matches   | before | after     | allocs before | allocs after | scan allocs before | scan allocs after |
| ------------------- | --------- | ------ | --------- | ------------- | ------------ | ------------------ | ----------------- |
| no match            | 0         | 10.4   | 10.5      | 7,077         | 7,077        | 7,075              | 7,077             |
| rare literal        | 531       | 18.2   | 17.8      | 8,174         | 8,357        | 7,826              | 8,005             |
| word (`report`)     | 2,704     | 17.7   | 16.8      | 9,350         | 10,257       | 8,969              | 9,878             |
| extension (`*.pdf`) | 13,885    | 19.1   | 17.5      | 34,843        | 9,866        | 34,510             | 9,536             |
| one letter (`e`)    | 1,857,548 | 300.0  | **174.8** | 133,443       | 44,635       | 107,899            | 19,108            |

- **One letter: 1.7× faster** (300 → 175 ms), and the scan's allocations with excludes on are now within ~10,000 of the
  scan with them off (19,108 against ~9,350), where they were ~98,500 over.
- **Small queries don't move**: within run-to-run noise. They pay ~200–900 extra allocations for the memo maps' growth
  (a few per chunk that sees a match), far below a single row's worth on any arena.
- The remaining one-letter time is ranking 1.9 M matches and the first walk per folder per chunk.

## What's left

- **The unit test can't reliably catch the regex regression.** In a debug build the pool's lock is rarely contended
  (231–1,388 allocations for the shared-`Regex` shape against ~225 for the chunked one), so the budget test pins the
  fold and ranking halves only. The release bench is the check for the regex half.
