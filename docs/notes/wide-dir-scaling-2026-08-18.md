# The 60,000-child stall: it was the ancestor roll-up, and it was quadratic

`preemption-2026-08-18.md` stumbled into a first index that did not finish in 600 s over one directory of 60,000
children, where 12,000 finished in about two, and left the question open. This note answers it, and § 7 carries the fix
that shipped for it and the curve it replaced the quadratic with.

**The finding.** Covering a wide directory is fine. Covering it AFTER something stopped the walk is `O(width²)`: each
unwalked child becomes a frontier root of its own, each root's walk ends with a `ComputeSubtreeAggregates`, and that
handler recomputes the wide parent from all of its children — once per root. At 60,000 children that is ~73 ms of
database work per root, 60,000 times.

**It is slow rather than wedged.** The reproduction was still making forward progress at 9.7 roots per second when it
hit the budget, and would have finished in roughly another 68 minutes.

**A real user reaches it by opening a folder while the drive indexes**, which is the most ordinary thing they do.

**✅ Fixed.** The writer coalesces the roll-up per burst instead of per subtree, which takes the piecemeal cost from
`O(width²)` to linear: 60,000 children now settle in 29.3 s where the same bench measured a curve heading for 73
minutes. § 7 has both curves. §§ 1–6 are the diagnosis as it stood, kept because the mechanism and the ~750-child
break-even are what make the fix's shape make sense.

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

## 6. What the fix had to answer

**The target**: roll the ancestor up once per BURST of subtree aggregates, rather than once per subtree. The residual
per-root cost with the roll-up removed is ~0.6 ms and flat, so 60,000 roots would settle in well under a minute instead
of 73. (Measured after: ~490 µs and flat, 29.3 s at 60,000. § 7.)

**❌ There is no cheaper incremental version of the current call.** Sizes and counts could ride `propagate_delta_by_id`
at `O(depth)`, but `recursive_has_symlinks` and `min_subtree_epoch` are recomputed from a directory's children in every
path there is, so ANY per-root ancestor update pays `O(width)` at the wide parent. Coalescing is the only shape that
removes the quadratic.

**Where it goes**: the writer already has the right seam. `writer_loop` drains its deferred `dir_stats` repairs at the
caught-up point — `queue_depth == 0` and `conn.is_autocommit()` — for exactly the reason this needs ("with nothing
queued behind us every committed row is final, so a recompute-from-children sees the whole truth"). A routine roll-up
queue drained at the same point turns W repairs of one wide parent into one.

**What made it more than a patch**, and what the shipped version answers (§ 7):

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

## 7. What shipped, and the curve it bought

**The change**: `handle_compute_subtree_aggregates` queues the ancestor instead of walking it, and `writer_loop` drains
the queue at its caught-up point, where a whole burst is one walk. Mechanism, the race argument, and the durability
answer live with the code (`crates/cmdr-index/src/indexing/writer/DETAILS.md` § "The routine roll-up is coalesced per
burst"); this section is the measurement.

**The bench had to be corrected first.** `wide_dir_rollup_cost` drove each frontier root through a `cover` that blocks
on a flush before returning. The phase machine does the opposite ("the writer drains once per phase, not once per
root"), and the difference decides the answer: a flush per root stops the walker and the writer overlapping at all, so
nothing can ever coalesce and the arm reports a quadratic the machine does not have. Both arms now leave the drain to
the caller and end the timed region at a settle. **Every number below comes from that corrected bench, before and
after**, so the two tables are directly comparable — and the "before" column is NOT the § 3 table, which was taken with
the flush per root.

Method: same MacBook, 2026-08-18, on `worktree-rollup-burst` (`f437c2dbc` for the tables, re-verified on the branch tip
once the delete guards landed), release build, tree in `/private/tmp`.

⚠️ **The machine was not idle again**, and this bench is sensitive to that in a way worth knowing before anyone reads a
future run as a regression. A repeat taken at load average ~41 (two Cmdr instances building and running) reported 1.48
ms and 1.19 ms a root at 12,000 and 20,000 — three times the numbers below — while 30,000 to 60,000 barely moved. Two
repeats once the load dropped landed at 477/482 µs and 473/469 µs, back on the line. **The small widths are the noisy
ones**: they finish inside seconds, so one scheduling episode owns a large share of the run. Read the ratios and the
per-root SHAPE rather than the absolute times, and re-run a surprising small width before believing it.

**Before** (the roll-up inside the handler):

| width  | one walk | per child | ratio | per root |
| ------ | -------- | --------- | ----- | -------- |
| 500    | 19.7 ms  | 655.8 ms  | 33x   | 1.31 ms  |
| 1,000  | 32.5 ms  | 1.8 s     | 56x   | 1.82 ms  |
| 2,000  | 61.9 ms  | 5.7 s     | 92x   | 2.86 ms  |
| 4,000  | 120.8 ms | 21.2 s    | 175x  | 5.30 ms  |
| 8,000  | 230.8 ms | 84.1 s    | 364x  | 10.51 ms |
| 12,000 | 368.2 ms | 194.0 s   | 527x  | 16.16 ms |

**After** (coalesced per burst):

| width  | one walk | per child | ratio | per root |
| ------ | -------- | --------- | ----- | -------- |
| 500    | 17.0 ms  | 250.2 ms  | 15x   | 500.3 µs |
| 1,000  | 31.9 ms  | 492.2 ms  | 15x   | 492.2 µs |
| 2,000  | 65.3 ms  | 961.4 ms  | 15x   | 480.7 µs |
| 4,000  | 110.5 ms | 2.0 s     | 18x   | 505.4 µs |
| 12,000 | 359.9 ms | 6.2 s     | 17x   | 518.2 µs |
| 20,000 | 667.2 ms | 9.6 s     | 14x   | 479.9 µs |
| 30,000 | 1.1 s    | 14.7 s    | 14x   | 491.6 µs |
| 40,000 | 1.7 s    | 19.5 s    | 12x   | 488.0 µs |
| 60,000 | 3.0 s    | 29.3 s    | 10x   | 488.2 µs |

**The per-root column is the finding. It grows 12× across the "before" widths and does not move at all across the
"after" ones** — 480 to 518 µs from 500 children to 60,000, which is the § 6 prediction (the residual cost with the
roll-up ablated away) landing on the nose. The ratio against covering the same ground whole now FALLS as the directory
gets wider, where it used to climb without limit.

**Above 12,000 the "before" column is extrapolated, not measured.** The 12,000 arm alone took 194 s and 60,000 would
have taken about 73 minutes. The § 3 fit, `per root ≈ 0.9 ms + 1.2 µs × width`, predicts 10.5 ms at 8,000 and 15.3 ms at
12,000 against 10.51 ms and 16.16 ms measured, so it holds: 500 s at 20,000, 1,110 s at 30,000, 1,960 s at 40,000, and
4,380 s at 60,000. **Against 29.3 s measured, that is a 150× improvement at 60,000, and the multiple keeps growing with
the width because the shape changed rather than the constant.**

**End to end, the original reproduction.** `preemption_bench::preemption_cost` at `CMDR_PREEMPTION_BENCH_DIRS=60000` is
the run § 2 caught stalling: it took 1,332 s and its second arm gave up with
`timed out after 600.0s waiting for the phases to finish`. The same run now finishes in **74.2 s** with no arm timing
out, and the arm that stalled — the folder somebody opened while the big sibling walks — reports **891.7 ms** from
opened to covered, against the 2.38 s it would wait without preemption. Handing ground over stayed where
`preemption-2026-08-18.md` measured it (median 133 ms, worst 141 ms over five rounds). So the wide directory no longer
costs the phase machine anything a user would notice, and preemption's own numbers are unchanged by the fix.

**The coalescing factor, counted rather than timed.** The writer counts the roll-up walks it runs, so the scaling guard
asserts on the mechanism: 400 frontier roots under one parent cost **1** ancestor roll-up, against exactly 400 before
(`writer/aggregation/tests.rs::a_burst_of_roots_under_one_parent_costs_a_handful_of_rollups`, which fails at 400 if
anyone puts the inline repair back).

### The one thing it cost, and how to spot the next one

Deferring the roll-up moved WHEN an ancestor is credited: from inside the `ComputeSubtreeAggregates` handler to the
writer's caught-up point, one hook run after `flush_blocking` replies. Any test that read a `dir_stats` row back on the
heels of a flush therefore reads a not-yet-credited ancestor. The roll-up work fixed the tests it knew about, in
`writer/aggregation/`; it missed
`indexing::reconcile::verifier::tests::verify_new_dir_credits_ancestors_exactly_once`, two subsystems away, which is
the only test outside the writer that reads an ANCESTOR's totals back after a subtree scan.

Why nothing caught it (verified on the `rust-tests-linux` container and macOS, 2026-08-18):

- **macOS never loses the race.** 40 runs of the failing test on the Mac, 40 passes. The same binary in the Linux
  container failed 8 of 20. So `pnpm check rust-tests` and CI's native Linux `desktop-rust-tests` lane both read green
  while the bug was live; the Docker lane is where it surfaces, and it is `IsSlow`, so it runs on the
  `--include-slow` cadence rather than every wrap.
- **Load HIDES it, it doesn't cause it.** A full-package parallel `cargo nextest run -p cmdr-index` passed 3/3 with the
  bug in place: the test thread gets descheduled after the flush, which is exactly the pause the writer needs. Run
  alone on a quiet machine, it fails. That inverts the usual reading of a re-run-alone classifier, which calls "fails
  alone" real and "fails only under load" contention.
- **The symptom is an UNDER-credit under a test named for double-crediting**, and it is often TORN rather than absent
  (a credited parent under an uncredited grandparent), so it surfaces at `check_db_consistency`'s recompute oracle as
  readily as at the test's own assertion. Two different panic sites, one cause.

❌ Don't read an under-credit here as a lost roll-up. Measured: with nothing further sent to the writer, the stale row
converged in 1.3-1.7 ms. The queue is unbounded, the drain is idempotent, and `Shutdown` and channel-close both settle
on the way out, so the ledger is eventually consistent by design. The fix is `writer::tests::settle_the_writer` at the
read site, never a change to the writer.

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
