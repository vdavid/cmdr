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

## After

Recorded by the extraction's crate milestone, beside these.
