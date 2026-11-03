# Journal replay against a full reconcile (#807)

What a cold-start FSEvents replay costs per delivered event and per processed path, against what the full reconcile it
falls back to costs per walked entry, and the `ReplayBudget` weights that came out of it. Canonical home of the
decision: `crates/cmdr-index/src/indexing/watch/DETAILS.md` § "Replay cost, not event count".

## Setup

- **Machine**: MacBook Pro, Apple M3 Max, 64 GB, macOS 27.0.1 (26A434), APFS boot volume (926 GiB, 621 GiB used, 7.8 M
  inodes). Idle on purpose: the only other processes were David's idle prod and dev Cmdr. 2026-11-03, 04:00–05:00 local.
- **Build**: release profile, on top of `9313f09438`.
- **Index**: an APFS clone (`cp -c`) of prod's `index-root.db` and its `-wal`, 8,082,261 rows, `last_event_id`
  1,827,771,087.
- **Harness**: `crates/cmdr-index/src/indexing/watch/event_loop/replay_cost_bench.rs` (`#[ignore]`d, env-gated; its
  module docs say how to run it). The replay arm reads the real journal through `DriveWatcher::start("/", since)`, once
  counting only and once draining every 100,000-path window through the production `flush_replay_batch` inside one
  transaction, with no budget. The walk arm runs `start_local_reconcile` on `/`, what a `ReplayOverflow` fallback runs
  on a populated, completed index. CPU is whole-process `getrusage` (writer thread included); fseventsd's CPU is a
  `ps -o time` delta on its pid.

## Numbers

**Journal delivery is the bottleneck, and fseventsd is what's busy.** The harness's queue held 3–26 events throughout,
so the consumer waited on delivery the whole time.

- From id 1 (oldest retained event: id 506,889): 1 M events at 91.9 s, 2 M at 182.1 s, 3 M at 366.7 s, covering ids up
  to 143 M. 122 µs of wall clock per event.
- From id 1,700,000,000 (recent history), two runs: 3 M events in 249.2 s and 236.8 s. 79–83 µs per event.
- From id 1,760,000,000: 1 M events in 109.7 s (110 µs per event), with **fseventsd at 90.4 s of CPU** (82% of a core,
  ~90 µs per event). The harness itself spent 6–7 µs of CPU per event.
- During the 192 s walk, fseventsd spent 5 s of CPU: its baseline here.

**Dedup with the production 100,000-path window**: 3 M raw events charged 2,476,537 paths (1.21 events per path),
649,593 distinct over the whole range; from id 1, 2,618,836 charged, 957,429 distinct (1.15 per path). 29% (recent) to
45% (old) of distinct paths are excluded and skip processing after a string check; the budget still charges them, and
the per-path costs below average them in.

**Processing** (3 M recent events, 2,476,442 paths, 25 windows): 51.5 s inside the drains (20.8 µs per path, 13.9–26.0
per window), 0.6 s for the commit and flush. Process CPU was 95.3 s against the counting pass's 17.0 s, so processing
cost ~78 s: **31.6 µs of CPU per path**. End to end the pass took 266.8 s, 107.7 µs of wall clock per path. It ran
against a nearly current index, so most upserts changed nothing and ~380,000 stale rows were deleted.

**Full reconcile** of `/` on the same copy: 7,542,487 entries in 744,036 dirs, **192.6 s wall, 90.5 s CPU**, 582 MB peak
RSS: **25.5 µs of wall and 12.0 µs of CPU per entry**. Prod's own log agrees: 191.1 s (2026-11-01) and 202.2 s
(2026-10-31) on a quiet machine, 498.5 s (2026-11-02) under build load. A fresh parallel scan is faster (87.2 s for 6.97
M entries, from the index's meta), but a populated, completed index falls back to the reconcile.

## What each unit of replay costs, in walked entries

- **A delivered event**: 79–122 µs of wall (3.1–4.8 entries) and ~97 µs of CPU, fseventsd's included (~8 entries).
- **A processed path**: 20.8 µs of wall (0.8 entries) and 31.6 µs of CPU (2.6 entries).

## Decision

**The old 1:1 paths-vs-entries budget was 4–12× too generous**, and it missed most of the cost. At its limit on this Mac
(~7.5 M paths, ~9 M events) a replay ran ~15 minutes of delivery plus ~2.6 of processing against a 3.2-minute walk, and
burned ~12× the walk's CPU counting fseventsd. Worse, it charged nothing for an event that dedups away, so a build-heavy
journal that rewrites the same files could deliver tens of millions of events while charging a few hundred thousand
paths.

**The budget now charges both, in walked-entry units, against the index's entry count**: `REPLAY_EVENT_COST` = 5 per
delivered event, `REPLAY_PATH_COST` = 2 per processed path, each between its wall-clock and CPU parity above (the path
rounded up, since this index was nearly current and a stale one does real upserts). A typical path (1.2 events) costs ~8
units, so a replay may process about an eighth of the index's entry count. At that limit it takes ~0.65× the walk's wall
clock and ~1.5× its CPU: neither cost runs past the walk's by more than half.

- For this index (7.5 M entries) that's ~940,000 paths or ~1.1 M events, about 100 s of replay.
- #751's case still replays: 14 M ids in 90 minutes is ~0.4–0.5 M events at the 29–36 ids per event measured here, ~3 M
  units, ~40–60 s.
- **Floor**: `REPLAY_BUDGET_FLOOR` = 800,000 units, the old floor of 100,000 paths at ~8 units each: ~10 s of replay. It
  only bites below an 800,000-entry index, where both routes take seconds.

## Limits of this measurement

- **The journal didn't reach 10–50 M events in a measurable window.** Reading it all from id 1 didn't finish in 20
  minutes (counting only). At 30–45 ids per event, the 1,828 M ids it spans are plausibly 40–60 M events, which would
  take 1–2 hours to deliver at these rates. The runs above cover 1–3 M events; the per-event rate held steady across
  them (linear within ±25%), so the weights should hold at 10–50 M, but that's an extrapolation.
- One idle M3 Max, one index. Under load the walk slowed 2.6× (prod's 498.5 s); fseventsd under load wasn't measured.
- Processing ran against a nearly current index. A stale one does more real upserts; `REPLAY_PATH_COST` rounds up for
  that, and delivery dominates either way.
