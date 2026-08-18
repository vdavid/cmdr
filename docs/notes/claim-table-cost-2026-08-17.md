# What the claim table cost, and what it costs now

The claim table (`crates/cmdr-index/src/indexing/lifecycle/cover/live/mod.rs`) is how two walks over one volume stay off
each other's ground: a walk takes its frontier roots, and a later walk over ground somebody already holds doesn't take
it. It is a data-safety mechanism, not a performance one, but it sits on the CALLER's thread at the top of
`cover::start`, before any disk is read, so whatever it costs is latency a search pays before it can see a result.

Measured with `crates/cmdr-index/src/indexing/lifecycle/cover/live_bench.rs` (`claim_cost`, `#[ignore]`d) on 2026-08-17,
release build, M-series laptop, over synthetic frontier roots eight components deep sharing a long common prefix — the
same generator the branch set's bench uses, so the two are directly comparable.

Before: `HashMap<String, Vec<String>>`, one linear scan of everything held per root asked about. After:
`HashMap<String, BTreeMap<String, Mode>>`, an ancestor-chain lookup plus one sorted descendant range per root.

## Taking a frontier nobody holds, then releasing it

The common case: a cold-drive search or a phased group start takes the whole frontier.

| roots | take before   | take after  | speed-up | release before | release after |
| ----- | ------------- | ----------- | -------- | -------------- | ------------- |
| 100   | 847.21 µs     | 288.75 µs   | 2.9x     | 43.83 µs       | 40.83 µs      |
| 500   | 18.17 ms      | 401.25 µs   | 45x      | 223.75 µs      | 65.00 µs      |
| 1,000 | 71.67 ms      | 830.29 µs   | 86x      | 933.46 µs      | 117.58 µs     |
| 2,500 | **446.77 ms** | **2.23 ms** | **200x** | 4.63 ms        | 332.38 µs     |
| 5,000 | 1.81 s        | 4.70 ms     | 385x     | 21.88 ms       | 657.96 µs     |

The before numbers grow as the square of the width (8.9 µs a root at 100, 367 µs a root at 5,000): each root was checked
against every root the same call had already taken, so one `take` was quadratic in its own width. The after numbers are
flat at ~1.0 µs a root.

## Taking it again under a live walk

The Decision 11 case: a refined query re-asks while the first query's walk is still running, so every root defers
against a table at its widest.

| roots | before        | after         | speed-up   |
| ----- | ------------- | ------------- | ---------- |
| 100   | 710.58 µs     | 7.96 µs       | 89x        |
| 500   | 17.63 ms      | 45.29 µs      | 389x       |
| 1,000 | 72.26 ms      | 99.79 µs      | 724x       |
| 2,500 | **441.46 ms** | **267.42 µs** | **1,651x** |
| 5,000 | 1.81 s        | 594.08 µs     | 3,046x     |

This arm gains the most because a deferral is now decided by the first overlapping key the range finds, rather than by
reading the whole set.

## What this does and does not settle

**It does settle that the claim table was a real cost at frontier scale.** 446.77 ms at 2,503 roots, on the thread the
search is waiting on, before a single directory is listed. The plan that prompted this measurement guessed "plausibly
tens of milliseconds" on the grounds that these are plain string comparisons; that guess was low by more than an order
of magnitude, because the quadratic term dominates long before the per-comparison cost matters.

**It does not settle the 3.0 s** that `cover-no-ground-block-2026-08-15.md` § "What this note does NOT settle" leaves
unattributed. ~450 ms of it now has a name, alongside the 490.8 ms `branch-set-cost-2026-08-15.md` attributes to
`finish_covering`, so the two together account for most of a second. The rest is still unexplained, and ❌ nobody should
quote this note as having closed that question.

**Neither figure is a regression risk.** Both are one-time per walk, and both are now flat per root.

**⚠️ Quote the shape, ❌ not the exact ratios.** An independent re-run on the same machine right after a 10-minute check
suite came in 17–43% slower on the "after" column (2.60 ms and 382 µs at 2,500), and a standalone `rustc -O` replica of
the old algorithm measured ~1.9× the "before" column, most likely because a bare replica misses the workspace release
profile's LTO and codegen settings. What reproduces exactly is the **shape**: the old table quadratic (a ~46× per-root
rise across a 50× width rise), the new one flat at ~1 µs/root free and ~0.2 µs/root contended. So the honest headline is
**roughly two orders of magnitude, and roughly three for the contended arm**, ❌ never "200×" or "1,651×" as though they
were stable constants.

## Re-taking these numbers

```sh
cargo test -p cmdr-index --release --lib -- --ignored --nocapture claim_cost
```

Release matters: the debug build's constant factors swamp the shape being measured. The widths bracket 2,503, the real
frontier from one cold-drive search that prompted the branch-set work.
