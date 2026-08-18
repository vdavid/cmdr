# What preemption buys, and what stopping a walk costs

The first index covers a volume one frontier group at a time, and until this change the machine consulted its visit
queue only BETWEEN those walks. A folder somebody opened therefore waited out whatever walk was running, and a frontier
root is not a small unit: `~/projects-git` is 1.58M entries on David's machine with 97% of it under a single child, and
no stitch depth splits it (`phased-vs-bulk-index-2026-08-14.md` § depth 1 against depth 2).

`docs/specs/ground-ownership-plan.md` § M7 named two reasons preemption had been ruled out and asked for both to be
measured rather than assumed. This note answers them, and adds a third finding nobody asked for: **preemption has a
break-even, and below it the machine is slightly slower.**

## Method

`crates/cmdr-index/src/indexing/lifecycle/phases/tests/preemption_bench.rs`, `#[ignore]`d, release build, driving the
REAL phase machine over a synthetic tree:

```sh
CMDR_PHASES_TEST_TREE_DIR=/private/tmp \
  cargo test -p cmdr-index --release --lib -- --ignored --nocapture --exact \
  indexing::lifecycle::phases::tests::preemption_bench::preemption_cost
```

`CMDR_PREEMPTION_BENCH_DIRS` overrides the 12,000 default (the cancel-to-join rows below were taken at 12,000, 60,000,
and 200,000).

- **Hardware**: David's MacBook (Apple silicon, APFS, macOS 25.5), 2026-08-18, on `worktree-preemption`.
- **Tree**: a `big` folder of N directories with one file each, plus a `zzz-visited` folder that sorts last. Flat on
  purpose, so one frontier root holds all of it. The run builds three trees of that size, so N dominates its wall time.
- ⚠️ **Past ~12,000 children in one folder, the fixture stops measuring preemption and starts measuring what a STOPPED
  walk of a huge directory costs.** At 12,000 the whole bench runs in 8.9 s. At 60,000 and at 200,000 the machine does
  not finish covering the tree inside a 600 s budget. That cost is the writer's ancestor roll-up going quadratic
  (`wide-dir-scaling-2026-08-18.md`) rather than preemption's own, though preemption is what triggers it here. Arm 1 is
  unaffected (it times one walk, not a machine run); arm 2 therefore only has the 12,000 row.
- ⚠️ **Machine load moves the walk numbers a lot, though not the handover.** A first attempt at N = 60,000 ran while
  several agents were hammering the same disk: building one tree took ~35 minutes (17 ms per entry) and the phase
  machine blew the 600 s patience budget covering it, while its cancel-to-join figures came out within 2 ms of the quiet
  run's. Compare a re-run's arms against each other, not against the absolute values here.

## 1. Cancel-to-join, the bound the handoff does NOT fix

The claim table's atomic handoff answers "the freed ground goes straight to the waiter". It says nothing about WHEN the
previous holder lets go, and `CoverWalk::finish()` joins the walk thread, so preemption latency is floored by
cancel-to-join whatever the table does. Measured from `cancel()` to `finish()` returning, five rounds, each over its own
virgin root, with the walk already 4,000 entries deep and its flush left to the caller:

| N (per round)    | median    | worst     |
| ---------------- | --------- | --------- |
| 12,000 (2,400)   | 88.86 ms  | 91.55 ms  |
| 60,000 (12,000)  | 135.98 ms | 140.68 ms |
| 200,000 (40,000) | 151.40 ms | 214.12 ms |

The 60,000 row reproduced to within 2 ms on a machine that was otherwise saturated (134.15 ms / 139.42 ms), so this arm
is one of the few things here that machine load does not move.

**The bound is real and it is small, and it grows much slower than the ground does.** The local walker checks its token
between directories, so the wait is one directory's read plus the parallel walker's own drain plus the flush a stopped
walk owes. A 17× bigger root bought a 1.7× longer wait, and the spread inside a run is wide (24 ms to 214 ms at the
largest size) because what is actually being waited for is however much the walker had in flight at that instant.
`YIELD_WAIT` is 750 ms, 3.5× the worst measured, which leaves room for a share's listing round trip; a holder that
overruns it costs the waiting walk that budget and then the answer a plain `Claim::take` would have given.

⚠️ **Not measured over SMB.** The share half of the handover is covered functionally
(`cover::network_tests::a_walk_somebody_waits_on_takes_ground_off_a_background_walk`) but not timed, and a share's
cancel-to-join is a listing round trip rather than a `readdir`. If preemption ever feels unresponsive on a NAS, this is
the first number to take.

## 2. Time to index a folder somebody opened

The folder is opened the instant the big sibling's walk is announced, which is the worst case: the whole of that walk is
still ahead. "Before" is the same tree with nobody opening anything, because that IS what the folder used to wait for —
the machine reached it in ordinary frontier order, after the sibling.

| N      | sibling's own walk | the folder's own walk | before    | after (opened → covered) |
| ------ | ------------------ | --------------------- | --------- | ------------------------ |
| 12,000 | 178.47 ms          | 517.33 µs             | 178.99 ms | **194.65 ms**            |

### The break-even, and why the small number is a loss

"After" carries a cost "before" does not: the machine hears where the user is looking on the progress reporter's 500 ms
tick (`visits.rs`, and ❌ nothing faster by that seam's own contract), so the poll latency is inside every "after"
number. What the user waits is therefore roughly:

```
after ≈ poll latency (≤ 500 ms) + cancel-to-join (~90 ms) + the folder's own walk
before ≈ the rest of the sibling's walk + the folder's own walk
```

**So preemption pays exactly when a frontier root takes longer to walk than the poll plus the join** — under a second on
this hardware, which the 12,000-directory root here does not reach. Below the line the machine loses the difference: it
took 16 ms longer to cover the folder than doing nothing would have. Above it the win is the whole remaining walk, which
is the case the feature exists for: `~/projects-git` at 1.58M entries is tens of seconds of it.

⚠️ **The above-the-line half is reasoned, not measured.** Every attempt to take it end to end ran into the fixture's own
wall rather than preemption's: at 60,000 and at 200,000 directories in one folder, the phase machine's pass over that
folder is what dominates (see the method notes), so the arms stop measuring what they are named for. The model above is
what the mechanism does, and the two terms in it are each measured; their sum at a large root is not. Whoever wants the
headline number should give the bench a nested tree first, so a big root is big without being one enormous directory.

That loss is worth taking. It is bounded by the poll tick, it lands only on drives whose roots are small (where nobody
is waiting long for anything), and the alternative — a size threshold before stopping a walk — needs a number nothing in
the index can answer, since a frontier root is virgin ground by definition (`phases/grouping.rs` exists because of
exactly that).

## What this does NOT settle

- **The SMB and MTP halves are untimed.** Both the cancel-to-join above and the poll-plus-join model assume a local
  `readdir`. A share's listing dominates both terms.
- **It does not measure the cost to the first index.** A stopped group leaves its ground on the frontier and the next
  pass re-asks for it, so the machine repeats the stitch and the coverage query for that root. Cheap per event, and
  nothing here bounds how often a browsing user can trigger it.
- **It says nothing about the search-side handover's latency.** `Claim::preempt` waits up to `YIELD_WAIT` for a
  background walk to let go; the arms above measure the holder's side of that wait, not a real search's end-to-end time
  to first result.
- ✅ **The question it stumbled into is answered: `wide-dir-scaling-2026-08-18.md`.** The stall is the writer's ancestor
  roll-up, and it is quadratic in the directory's width. Two corrections to what is written above: the arm that blows
  the budget is the one where somebody OPENS a folder, so "with no preemption involved" was wrong, and the run is slow
  rather than wedged (9.7 frontier roots a second, about 68 minutes from finishing). Covering 60,000 children
  uninterrupted takes 3.2 s; it is what a STOPPED walk leaves — every unwalked child its own frontier root, each one
  rolling the 60,000-child parent up again — that costs `O(width²)`. That also explains the shape of the gap in the
  warning above: it is a cliff, not a curve, and the cliff is where the walk finally lasts longer than the 500 ms visit
  poll.
