# Sentinel-encoding the search arena's row (2026-08-06)

`SearchEntry` went from 56 bytes to 40 by encoding its two `Option<u64>` fields into eight bytes each
(`search/index.rs`, `OptU64`). This is the before-and-after, and the method, so a future "did the arena get bigger
again?" is a re-run rather than a re-derivation.

## What was measured, and on what

One snapshot of the real root index, `.backup`-copied from the live file so both builds read byte-identical data:
**6,045,549 rows**, 934,793 of them with a NULL `logical_size` (hardlink dedup, 15.5%) and 1 with a NULL `modified_at`.
Largest `logical_size` on the volume is 994,663,481,856 and the largest `modified_at` is 1786028164 — both nine-plus
orders of magnitude short of the `u64::MAX` sentinel.

Both figures come from `search::bench::bench_arena_bytes`, run under `--release`. ⚠️ The test binary uses
`test_support`'s `System`-backed counting allocator, not the mimalloc the app ships, so the DIFFERENCE is what carries;
the absolute numbers aren't the app's footprint.

|                                 | before    | after     | Δ                  |
| ------------------------------- | --------- | --------- | ------------------ |
| `size_of::<SearchEntry>()`      | 56 B      | 40 B      | −16 B              |
| arena heap held                 | 689.5 MiB | 597.2 MiB | −92.3 MiB (−13.4%) |
| of which the entries `Vec`      | 322.9 MiB | 230.6 MiB | −92.3 MiB (−28.6%) |
| names arena + `id_to_index`     | 366.6 MiB | 366.6 MiB | unchanged          |
| process RSS, one arena resident | 705.0 MiB | 613.0 MiB | −92.0 MiB (−13.1%) |

**The two instruments agree with each other and with the arithmetic** (6,045,549 × 16 B = 92.25 MiB), which is the point
of running both: the heap figure is thread-local and exact, and the RSS figure says it reaches real memory. Both
reproduce to 0.1 MB across runs.

## Latency: no measurable change

`search::bench::bench_arena_scan`, count-only (the rayon pass alone, no ranking or path materialization), one-letter
pattern so millions of rows reach the predicates. Seven interleaved before/after rounds, best-of-9 or best-of-15 each.

- **name only** (every row pays the `Candidate` build, which decodes both fields): before min 79.7–162.9 ms across
  rounds, after 91.0–176.0 ms.
- **+ size and date bounds** (the predicates read the decoded values): before 88.1–141.2 ms, after 83.7–136.2 ms.

**The spread within one build is far wider than any gap between them**, so the honest reading is "unchanged": this
machine runs several agents at once and a round-to-round swing of 60–80 ms swamps the two extra compares per row. Four
of seven rounds favoured the smaller struct and three the larger, which is what noise looks like. There is no sign of
the regression an extra branch per field read could in principle cause, and a smaller row packing 1.6 rows per cache
line instead of 1.14 plausibly pays for it.

**Match counts were byte-identical on every one of the 14 runs** (4,141,266 name-only, 438,669 with the size and date
bounds), which is a correctness check the memory numbers can't give: the size and date filters return exactly the same
answer over 6.0 M real rows, the 934,793 NULL-size ones included.

## The method, which is the reusable part

- **Compare two BINARIES, not two runs.** `cargo test --release` overwrites the same test binary, so numbers taken
  minutes apart on a shared machine are comparing the machine's mood. Build one side,
  `cp target/release/deps/cmdr_lib-<hash>` aside, build the other, then run the two alternately.
- **Build both sides' bench from the SAME source.** The pre-change side was produced by restoring `search/` to the base
  commit and copying the new bench file over it, with only its three `OptU64` lines reverted — so the harness is
  identical and only the thing under test differs.
- **Snapshot the index.** The boot volume's index gains rows by the second; a row count that moved between runs is a
  difference the arena shape didn't cause. `sqlite3 "file:index-root.db?mode=ro" ".backup '/tmp/snap.db'"` — ❌ not
  `VACUUM INTO`, which fails on this schema with `no such collation sequence: platform_case`.
- **Warm up before reading RSS**, or the delta is mostly the DB's page cache. `bench_arena_bytes` loads once, drops it,
  then measures.
- **Run `bench_arena_bytes` with `--exact`.** The harness runs tests in parallel in one process, so a sibling bench
  holding its own arena lands in the same RSS reading.

## What's still on the table

`id_to_index` is the larger remaining item and is **not** a free win — see `size-only-subtrees-rejected-2026-08-06.md` §
The search arena for why it has to be measured on the latency axis first. The names arena plus `id_to_index` is 366.6
MiB of the 597.2 MiB an arena now costs, so that's where the next real lever is.
