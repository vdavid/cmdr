# The 60,000-child stall: it is the ancestor roll-up, and it is quadratic

`preemption-2026-08-18.md` stumbled into a first index that did not finish in 600 s over one directory of 60,000
children, where 12,000 finished in about two, and left the question open. This note answers it.

**The finding.** Covering a wide directory is fine. Covering it AFTER something stopped the walk is `O(width²)`: each
unwalked child becomes a frontier root of its own, each root's walk ends with a `ComputeSubtreeAggregates`, and that
handler recomputes the wide parent from all of its children — once per root. At 60,000 children that is ~73 ms of
database work per root, 60,000 times.

**It is slow rather than wedged.** The reproduction was still making forward progress at 9.7 roots per second when it
hit the budget, and would have finished in roughly another 68 minutes.

**A real user reaches it by opening a folder while the drive indexes**, which is the most ordinary thing they do.

## Method

Hardware: David's MacBook (Apple silicon, APFS, macOS 25.5), 2026-08-18, on `worktree-wide-dir-scaling` at `9191903a5` —
the same commit `preemption-2026-08-18.md` was taken on, so the two are directly comparable. ⚠️ **The machine was not
idle**: two Cmdr builds sat at ~99% CPU throughout. That inflates the absolute numbers and can't explain any of the
ratios below, which are taken between arms of the same run.

- `crates/cmdr-index/src/indexing/lifecycle/phases/tests/wide_dir_bench.rs`, `#[ignore]`d, release build.
- `crates/cmdr-index/src/indexing/lifecycle/phases/tests/preemption_bench.rs` for the original reproduction.
- `sample(1)` against the running process, and `sqlite3` against the wedged run's own index.

## 1. The uninterrupted first index is fine, at every width tried

`wide_dir_cost`, one directory of N subdirectories each holding one file, driven through `Index::start_volume` — the
real activation, writer, watcher, and phase machine.

| N      | build | machine  | walk     | tail     | entries |
| ------ | ----- | -------- | -------- | -------- | ------- |
| 12,000 | 1.1 s | 349.7 ms | 236.6 ms | 113.1 ms | 24,002  |
| 20,000 | 1.7 s | 748.1 ms | 519.9 ms | 228.2 ms | 40,002  |
| 30,000 | 2.8 s | 1.4 s    | 1.0 s    | 352.1 ms | 60,002  |
| 40,000 | 3.7 s | 2.3 s    | 1.8 s    | 495.0 ms | 80,002  |
| 50,000 | 4.6 s | 2.5 s    | 1.9 s    | 571.4 ms | 100,002 |
| 60,000 | 5.6 s | 3.2 s    | 2.4 s    | 750.6 ms | 120,002 |

**60,000 children in one directory costs 3.2 s, and the curve is close to linear** (29 µs a child at 12,000, 53 µs at
60,000). So the walk, the writer, and the branch-watch registration are all cleared: none of them is what the open
question found.

## 2. What the open question actually found

Re-running the preemption bench at `CMDR_PREEMPTION_BENCH_DIRS=60000` reproduces the failure exactly (1,332 s, then
`timed out after 600.0s waiting for the phases to finish`). **The arm that blows the budget is the SECOND one, not the
first**, so `preemption-2026-08-18.md`'s "with no preemption involved" was wrong: `lsof` on the running process showed
`index-preemption-bench-after.db` as the only index open, and the handover arm's table had already printed.

The arm that stalls is the one where somebody opens a folder. That is the whole difference, and it is what turns the
12,000-against-60,000 gap into a **cliff rather than a curve**: the machine hears about an open folder on the progress
reporter's 500 ms tick, so preemption can only land mid-walk once the wide directory takes longer than that to walk. On
this hardware the walk of 12,000 children is ~180 ms and never gets stopped; 60,000 is ~2.4 s and does. The threshold
sits somewhere around 12,000–15,000 children, and it is a property of the timing rather than of the width.

**What preemption leaves behind is the actual problem**, and preemption is only one of several ways to arrive at it (§
5).

## 3. Which stage owns it

Three independent lines, all naming the same one.

**Sampled.** `sample(1)` on the running process, ~5 minutes into the stalled arm: the `index-writer` thread is 2,247 of
2,250 samples inside `writer_loop → handle_compute_subtree_aggregates → repair_dir_stats_upward`, split between
`recompute_recursive_has_symlinks` (911), the per-level `SUM` recompute (716), and `recompute_min_subtree_epoch` (618).
Every `index-cover` thread is parked in `send_blocking_with_depth`: the walkers are blocked on a full writer channel, so
**the writer is the bottleneck and the roll-up is the writer**.

**Counted.** `sqlite3` against the stalled run's own index, read-only, two probes 123 s apart: 41,152 → 39,962
directories still at `listed_epoch = 0`. That is **9.7 frontier roots a second**, against ~20,000 entries a second in
arm 1, and it answers the hung-or-slow question — it was going to finish, in about another 68 minutes.

**Ablated.** `wide_dir_rollup_cost` covers a wide directory's children one frontier root at a time (the state a stopped
walk leaves, reproduced deterministically with a stitch) against covering the same ground in one walk:

| width | one walk | per child | ratio | per root | per root, roll-up off |
| ----- | -------- | --------- | ----- | -------- | --------------------- |
| 500   | 19.7 ms  | 849.7 ms  | 43x   | 1.70 ms  | 1.67 ms               |
| 1,000 | 32.1 ms  | 2.2 s     | 68x   | 2.18 ms  | 1.13 ms               |
| 2,000 | 62.6 ms  | 6.6 s     | 105x  | 3.30 ms  | 633 µs                |
| 4,000 | 124.4 ms | 22.5 s    | 181x  | 5.63 ms  | 591 µs                |

The last column is the same bench with the one call
`handle_compute_subtree_aggregates → repair_dir_stats_upward(parent)` skipped. **With the roll-up in, the per-root cost
grows with the width; with it out, it flattens** — and the width-4,000 arm goes from 22.5 s to 2.4 s. Nothing else in
the per-root path depends on how wide the parent is.

Fitting the two columns: **per root ≈ 0.9 ms + 1.2 µs × width**. The quadratic term passes the fixed per-call cost at
about 750 children and owns everything past a few thousand. Extrapolated to 60,000 it predicts 73 ms a root and about 73
minutes overall, which is what the live run was doing at 9.7 roots a second.

## 4. Why it is quadratic

`walk_subtree` sends exactly one `ComputeSubtreeAggregates` per frontier root it covers
(`crates/cmdr-index/src/indexing/scanner/mod.rs`). Its handler recomputes that subtree, then calls
`repair_dir_stats_upward` from the subtree root's PARENT (`crates/cmdr-index/src/indexing/writer/aggregation.rs`). Each
level of that walk is `O(children)` by design and says so (`crates/cmdr-index/src/indexing/writer/DETAILS.md` § "The
repair primitive") — four indexed passes over the directory's children, for sizes and counts, direct symlinks, subtree
symlinks, and `min_subtree_epoch`.

That is the right cost for one message. Nothing costed the case the phase machine routinely creates: **W frontier roots
that all share one parent holding W children.** W roots × `O(W)` per roll-up = `O(W²)`, and the short-circuit can't
help, because every root genuinely changes what its parent sums to.

The per-level cost is also worse than "scan an index": `entries` is indexed on `(parent_id, name_folded)`, and all four
queries need `is_directory`, `is_symlink`, or a size, so each child costs an index seek plus a row fetch.

## 5. Who can hit it, and how wide is wide

Anything that stops a walk after the wide directory has been listed but before its children are walked leaves this
state:

- **A folder somebody opened**, which stops the group on purpose (`phases/mod.rs`, `walk_group`). Confirmed: this is the
  arm that stalls.
- **A search walk taking the ground.** `WalkFor::TheUser` asks background walks to hand ground over.
- **A quit or a relaunch mid-walk**, and a master-switch or per-drive toggle cycle.
- **A resume pass** from `completion_retry.rs`.

Past ~1,000 children the roll-up costs more than the walk it follows; past a few thousand it is the whole cost.
`~/Downloads`, a Maildir, a camera dump, an Xcode `DerivedData`, `node_modules/.cache`, and `/usr/share/man/man3` all
reach that on ordinary machines. **So: yes, a real user can hit this**, and the trigger is opening a folder while their
drive is being indexed for the first time.

⚠️ **The end-to-end 600 s reproduction is over a synthetic tree, not the shipping app.** The mechanism is in product
code and the ablation attributes it, but nobody has driven the real app into it, and the width where preemption starts
landing mid-walk is hardware-dependent.

## 6. What a fix involves

**The target**: roll the ancestor up once per BURST of subtree aggregates, rather than once per subtree. The residual
per-root cost with the roll-up removed is ~0.6 ms and flat, so 60,000 roots would settle in well under a minute instead
of 73.

**❌ There is no cheaper incremental version of the current call.** Sizes and counts could ride `propagate_delta_by_id`
at `O(depth)`, but `recursive_has_symlinks` and `min_subtree_epoch` are recomputed from a directory's children in every
path there is, so ANY per-root ancestor update pays `O(width)` at the wide parent. Coalescing is the only shape that
removes the quadratic.

**Where it goes**: the writer already has the right seam. `writer_loop` drains its deferred `dir_stats` repairs at the
caught-up point — `queue_depth == 0` and `conn.is_autocommit()` — for exactly the reason this needs ("with nothing
queued behind us every committed row is final, so a recompute-from-children sees the whole truth"). A routine roll-up
queue drained at the same point turns W repairs of one wide parent into one.

**What makes it more than a patch**, and why this note stops here rather than shipping it:

- It moves a documented invariant. The ancestor repair is race-free today because it runs inside the same message;
  deferring it to the caught-up point means the wide parent's row is stale in between, and `flush_blocking` replies
  BEFORE that point (`writer/CLAUDE.md`: "`flush_blocking` ≠ settled"). The tests in `writer/aggregation/tests.rs` that
  flush and then assert on ancestors would have to wait on `idle_epoch()` instead.
- It has a durability edge. Today each repair commits with its message, so a crash mid-first-index leaves ancestors
  correct as of the last one. Coalesced, a crash inside a burst leaves the wide parent drifted with nothing remembering
  it, so the fix owes an answer there (the completion sequence's `BackfillMissingDirStats` and `PayLedgerIfUnpaid` heal
  a run that finishes, and neither runs for one that dies).
- ❌ It must not reuse `DeferredRepairs`. That queue is drift telemetry: it warns on its first entry, caps at 1,024 ids,
  and GIVES UP after five attempts. On the routine path those are all wrong — a dropped roll-up is a permanently wrong
  size, and the warning would fire on every first index.

**A smaller mitigation exists and is not a fix.** One `cover()` call takes up to `MAX_ROOTS_PER_CALL = 16` frontier
roots (`phases/grouping.rs`) and sends 16 separate aggregate messages; sending one message per CALL and repairing each
distinct parent once would cut the roll-ups 16x with no semantic change at all. Raising the cap is also safer than it
was, because a group can now be stopped mid-flight for a folder somebody opened, so a long call no longer costs
responsiveness. Together they would take 73 minutes to tens of seconds at 60,000 children — while leaving the quadratic
in place for a directory an order of magnitude wider.

## What this note does NOT settle

- **Nothing was measured over SMB or MTP.** A trait-scanned volume walks the same phase machine and sends the same
  aggregate messages, so the same shape should apply, with the listing round trip on top.
- **The width where preemption starts landing mid-walk is one machine's.** It follows from the reporter's 500 ms tick
  against the walk's length, so slower storage moves it down and a faster disk moves it up.
- **A stop followed immediately by a start covers nothing.** Building the reproduction turned this up: `stop_indexing`
  only CANCELS, and a `start_volume` before the dying walk has released its ground hands the new machine a frontier
  every root of which is still claimed, so it walks nothing, runs out of passes, and finishes with the frontier intact.
  `completion_retry` re-arms a minute later, so it self-heals, and a real relaunch is a new process. Worth a look,
  though not worth a fix on this evidence.
