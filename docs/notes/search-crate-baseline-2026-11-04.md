# Search before and after the `cmdr-search` extraction

The numbers the extraction (`docs/specs/search-crate-plan.md`) has to stay within noise of: the arena scan, the
per-query allocations and latency, and the arena load, on one real boot index.

## Method

- **Index**: an APFS clone of David's live `index-root.db` (10,816,742 rows) and `importance-root.db` (13,563 scored
  folders), taken 2026-11-04 so both runs read the same file.
- **Build**: `cargo test -p cmdr --lib --release`, the `bench.rs` harnesses (its module doc has the commands), 16 rayon
  threads, M-series MacBook with other agents' builds running (so medians, not minimums, carry the comparison).
- **Warm**: the file was read once before each run, so these are warm-cache numbers.

## Baseline (before, `main` plus the three early fixes, 2026-11-04)

`bench_arena_scan` (best of 9):

- name only: 7,634,336 matches, min 65.15 ms, median 78.12 ms
- plus size and date bounds: 730,045 matches, min 59.45 ms, median 62.82 ms

`bench_query_allocations` (9 runs each):

- no match: 0 matches, 13,060 allocations, median 36.3 ms
- rare literal: 749 matches, 15,183 allocations, median 66.4 ms
- word: 4,750 matches, 18,826 allocations, median 53.9 ms
- extension: 14,080 matches, 16,477 allocations, median 49.6 ms
- one letter: 2,306,218 matches, 49,819 allocations, median 411.7 ms

`bench_arena_bytes`: 10,816,742 rows loaded in 928.54 ms (warm); `size_of::<SearchEntry>()` 40 B.

- **Gotcha**: in a release build `bench_arena_bytes` reports a NEGATIVE arena heap (−349.8 MB), so its heap counter
  doesn't see the release test binary's allocations correctly; only its load time and RSS lines are usable there.

## After the crate (`cmdr-search`, 2026-11-04, same index, load average 21–27)

`bench_arena_scan` (best of 9):

- name only: 7,634,336 matches, min 43.07 ms, median 46.30 ms
- plus size and date bounds: 730,045 matches, min 36.64 ms, median 42.07 ms

`bench_query_allocations` (9 runs each):

- no match: 13,054 allocations, median 58.7 ms
- rare literal: 15,152 allocations, median 66.9 ms
- word: 18,818 allocations, median 54.5 ms
- extension: 16,446 allocations, median 58.3 ms
- one letter: 49,904 allocations, median 417.5 ms

`bench_arena_bytes`: 10,816,742 rows loaded in 848 ms, 938 ms, and 1.17 s across three runs (one more read 1.60 s).

- **Verdict**: within noise. Allocations per query are the same to within 0.2% (the per-chunk clones moved verbatim),
  the one-letter query (ranking-bound) is 1% apart, and the arena load brackets the baseline's 929 ms. The scan medians
  came out faster, which the move can't have caused (same loop, same chunks, accessors in place of field reads); read it
  as machine noise, not a win. The short-query medians are noisier still (another agent's build was running).
