# Which primitive covers a search frontier, 2026-08-05

The cover-walk work in `docs/specs/unindexed-search-plan.md` had one real decision in it: whether a search-driven walk
over a coverage frontier should run on the **parallel guarded walker** (`scanner::scan_subtree`) or the **serial
reconcile** (`reconcile::reconcile_subtree`). The plan said to decide it by measurement on a representative frontier
rather than by either of the two published full-volume numbers. This is that measurement.

**Answer: the parallel walker, at every size, with no row shortfall.** It is 3.2–5.8x faster and wrote exactly the same
number of rows as the serial reconcile on all four trees.

## Method

`indexing::lifecycle::cover::bench::measure_cover_primitives`, `#[ignore]`d, run in release:

```sh
CMDR_COVER_BENCH_ROOT=/Applications \
  cargo test -p cmdr-index --release --lib -- --ignored --nocapture measure_cover_primitives
```

Each primitive gets a **fresh empty index** with only the ancestor chain down to the walk root seeded, which is the
state a real frontier node is in. The timer covers the walk **plus `flush_blocking`**, because both are writing the same
rows and a walk that only fills a queue faster hasn't covered anything yet. A plain recursive `read_dir` pass runs first
so neither primitive pays for the other's cold page cache.

A frontier node is all-new ground by definition, so this is a bulk add, never an incremental diff — which is why it
needed its own measurement rather than the rescan numbers.

Machine: Apple M3 Max, 16 cores, macOS 26.5.2, otherwise idle. Branch `worktree-david+unindexed-search-exec`.

## Results

| tree                                     | entries   | parallel walker | serial reconcile | speedup | rows written |
| ---------------------------------------- | --------- | --------------- | ---------------- | ------- | ------------ |
| `apps/desktop/node_modules` (small)      | 368       | 3.92 ms         | 12.40 ms         | 3.2x    | identical    |
| `~/Library/Application Support`          | 220,038   | 1.84 s          | 9.90 s           | 5.4x    | identical    |
| `/Applications`                          | 300,656   | 2.56 s          | 14.72 s          | 5.8x    | identical    |
| `~/projects-git/vdavid/cmdr` (worktrees) | 1,202,613 | 19.76 s         | 74.93 s          | 3.8x    | identical    |

"Rows written" compares `COUNT(*)` in the two indexes afterwards; the listed-directory counts matched too (50,502 on
`/Applications`, 30,872 on Application Support, 69,137 on the repo).

## The caveat the plan told us to check, and what it turned out to be

`reconcile/DETAILS.md` warns that the parallel walk "buys part of its speed by giving up", citing a boot-disk run that
came out ~10% short (6,001,637 rows against 6,663,048), and attributes the loss to abandonment "under rayon contention".

Two things are wrong with leaning on that here:

- **The walker doesn't use rayon.** Its workers are dedicated 8 MB-stack OS threads, and `scanner/CLAUDE.md`'s "Never
  rayon" must-know says why (File Provider reads descend deep XPC override chains that overflow rayon's 2 MB stack).
  Reading `docs/notes/indexing-benchmarks-2026-07-21.md` itself, the abandonment was the walker's stall timeout and
  32-consecutive-failure give-up budget firing inside a MacDroid phone's File Provider mount — a genuinely unresponsive
  mount, not thread contention. `reconcile/DETAILS.md` has been corrected.
- **A frontier walk is scoped, so the pathology is out of scope by construction.** The shortfall came from walking `/`
  whole, which drags in every File Provider mount on the machine. A search walks the folders a coverage answer named.

Zero directories were abandoned across all four trees here, including the 1.2M-entry repo with cargo `target/`
directories across every worktree.

**And under the coverage model an abandoned directory is no longer a silent loss.** It is never marked listed
(`scanner/CLAUDE.md` § "Honest-stale, never false-complete"), so it stays `listed_epoch = 0`, stays in the frontier, and
the next search walks it again. That is Accepted difference 9 in the plan, and it degrades to "walked again later"
rather than "missing forever".

## What the measurement caught on the way

The first run of the small tree read **1.01 s for 368 entries**, against 12 ms for the serial reconcile — the parallel
walker losing badly at small sizes. That was not thread-spawn cost: `walk` joins its watchdog before returning, and the
watchdog slept a flat `watchdog_interval` (1 s in production) before checking whether the walk was done. Every walk paid
one interval of dead time, however small it was.

Invisible on a full volume scan, ruinous for a search covering a run of small frontier nodes one after another. The
watchdog now waits on a condvar that `signal_done` wakes, ordered so the wake can't be missed in the check-then-wait
window. Same tree: **3.92 ms**. Pinned by
`scanner::walker::tests::a_tiny_walk_returns_without_waiting_out_the_watchdog`.

## When to revisit

- **If the frontier ever includes a File Provider mount root.** That is the one shape that produced the published
  shortfall. The walk survives it (that is what the guarded walker exists for), but the subtree stays frontier and
  re-enters every search until the mount responds. The walk's exclusion policy and the branch watch both touch this.
- **If the serial reconcile stops being the repair path.** It is still used, for a frontier node the index already holds
  rows under, where the parallel walker's fresh ids would collide (`lifecycle/cover/`). If that case is ever closed
  another way, the serial path leaves the cover story entirely.
- **On network volumes.** Neither number here transfers: SMB and MTP walk over the `Volume` trait, where the wire is the
  bottleneck, and the parallelism question is a different one.

## What the volume-boundary probe costs (added 2026-08-05)

The search walk stays on the device it started on (Decision 4: a search targets one volume), and the batched macOS read
carries no `ATTR_CMN_DEVID`, so the check is one `symlink_metadata` per directory the walk discovers. Measured by
`scanner::policy_tests::measure_boundary_probe`, which walks the same real tree twice into a fresh index each time —
once with the pin on, once with a probe that reports no device at all — so the difference IS the syscall:

| Tree                                | Dirs   | No pin  | Pinned  | Delta |
| ----------------------------------- | ------ | ------- | ------- | ----- |
| `/Applications`                     | 50 501 | 3.11 s  | 3.20 s  | +2.9% |
| `/Applications` (rerun)             | 50 501 | 3.08 s  | 3.25 s  | +5.6% |
| the `cmdr` repo, worktrees included | 69 303 | 35.59 s | 36.68 s | +3.1% |

**About 2–3 µs per directory, and 3–6% of wall clock.** It runs on the walker's worker threads alongside the directory
read that follows it, which is why the amortized cost is far below a serialized `lstat`. Files are never probed, so the
cost scales with directory count rather than entry count (~7% of entries here).

Rejected alternative: **adding `ATTR_CMN_DEVID` to the batched `getattrlistbulk` read**, which would make the check free
on the hot path. It changes a hand-written packed-record parser for every walk in the app (a full boot scan included) to
buy back 3-6% on the one walk that needs it, and the attribute packing order is bit-ordered, so the new field lands in
the middle of the existing parse. Worth revisiting only if the probe ever shows up in a profile.

Also rejected: **reading the mount table** (`getmntinfo`) once per walk and cutting at any path in it. Cheaper still,
but it's a snapshot — a drive mounted mid-walk would be walked into this volume's index — and it answers a different
question from the one that matters ("is this a mount point _right now_").
