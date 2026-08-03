# Cutting Cmdr's idle CPU and RAM

**Status**: SPECCED, not started. **Owner**: David. **Date**: 2026-08-03. **Baseline**: prod v0.37.0, repo at
`3d4c9816f`.

Prod v0.37.0, measured 2026-08-03 on David's machine: **110 minutes of CPU over 9.1 hours** (about 20% of one core,
sustained), **1.78 GB physical footprint** (2.8 GB peak), and **141,072 log lines / 28 MB in six hours**. The app was
idle throughout. Principle 5 says respect the user's resources; this is a long way from that.

**Read these first** (an executing agent should not start without them):
`crates/cmdr-index/src/indexing/writer/CLAUDE.md`, `crates/cmdr-index/src/indexing/store/CLAUDE.md`,
`crates/cmdr-index/src/indexing/reconcile/CLAUDE.md` and its `DETAILS.md`, `crates/cmdr-index/src/media_index/scheduler/`,
`crates/cmdr-fs/src/sqlite_util.rs` module docs, `docs/tooling/memory-debugging.md`, `docs/tooling/logging.md`,
`docs/notes/idle-cpu-indexing-streamlining-2026-07.md`, `docs/specs/later/sealed-subtrees-plan.md`.

## M0: where the CPU actually goes (DONE, and it reordered the plan)

An earlier draft named the reconcile drain as "the one that moves the CPU number", inferred from log volume rather than
from CPU. That inference was wrong, and it is recorded here so nobody re-derives it.

Method: `sample 69717 20` on the live prod process, then per-thread attribution counting samples whose leaf frame is NOT
a blocking syscall (`psynch_cvwait`, `kevent`, `mach_msg2_trap`, `semaphore_timedwait`, and friends). 7,330
running-samples across 71 threads.

- `index-writer`: **45.0%** of busy CPU
- `cmdr-sync-status-0..3` (four threads): **41.7%** combined
- everything else (24 named plus unnamed): 13.3%

**The reconcile drain does not appear.** Neither does the media live tick, in this window. Two threads nobody was
looking at account for **87% of busy CPU**, and inside each one a single avoidable call dominates:

- **`index-writer`**: 1,828 of ~3,398 running samples (**54% of the writer**, ~24% of app-wide busy CPU) sit in
  `insert_entry_v2_with_id` → `Connection::execute` → `prepare_with_flags` → `sqlite3LockAndPrepare` → `sqlite3Prepare`
  → `sqlite3RunParser` → `yy_reduce` → `sqlite3Insert` → `sqlite3GenerateConstraintChecks` → `sqlite3MPrintf`. That is
  **SQL statement parsing, not execution**: the same `INSERT` re-parsed from text on every row. Actual execution
  (`sqlite3_step`) is 182 samples on that path.
- **`cmdr-sync-status`**: 745 of 758 running samples per thread (~42% of app-wide busy CPU across the four) are two
  `stat` calls per probed path: 418 via `sync_status::probe::ubiquitous_bool` → `NSURL::fileURLWithPath` →
  `initFileURLWithPath` → `_NSFileExists` → `stat`, and 327 in `sync_status_for`'s own direct `stat`.

Both are confirmed in code (M1 and M2). Roughly **half of all busy CPU is addressable by about 30 lines**, which is why
the milestone order changed.

A caveat that qualifies every number here: this machine runs six Cmdr worktrees with active cargo builds. That is a
heavy case, not an unrepresentative one, but the fixes need a quiet-machine sanity check so we do not tune for one
workload. See § Definition of done.

## The evidence for the rest

- **7,007,762 rows** in the root index; **974,485 (14%) under `.claude/worktrees/`**. 26,536 `node_modules` dirs, 131
  `target` dirs.
- **3,704 distinct rescan anchors in eight hours, 3,438 (93%) under `.claude/worktrees/*/target`**, across 27,617 queue
  events. The hot ones are `target/debug/incremental/<crate>-<hash>/s-<hash>-<hash>-<hash>`.
- Churn windows crossing budget today: 108 s, 156 s, 79 s, 72 s, 44 s, 7 s of walking per 15 minutes. Six windows over
  eight hours, so roughly **466 s of reported walking**; unreported windows are each under the 60 s budget. This is wall
  time, largely IO wait, and `docs/notes/reanchor-cost-spike.md` measured this class of walk at 16–23% CPU. The drain is
  real, and it is not the 110 minutes.
- Media live tick, every 60 s: `live tick 'root': 0 enriched, 0 GC'd across 5,492 touched dir(s)`. An earlier sample
  caught 586/3,425 samples in `run_live_tick_blocking`; the later 20 s sample caught none, so the tick is **bursty** and
  its true average is unmeasured.
- **132 open SQLite connections** across 71 threads: 60 × `index-root.db`, 28 × `index-smb-…naspi.db`, 24 ×
  `importance-root.db`, 13 × `importance-smb`, 5 × `media-root.db`, 2 × `operation-log.db`.
- Footprint: Rust heap (mimalloc, shown as `IOAccelerator`) 947 MB dirty plus 725 MB reclaimable; `MALLOC_LARGE` 643 MB
  (regions of 9 MB and 2.25 MB) plus `MALLOC_SMALL` 152 MB. WebKit and compositor under 4 MB.
- Log volume over six hours: 32,479 `rescan`, 19,705 `reconciler`, 15,405 `sync_status`, 13,133 `writer`, 8,590
  `smb2::client::tree`, 7,694 `stall_probe`, 6,514 `space_poller`, 5,911 SMB `WARN`.

## The constraint that shapes M3

**No denylists. No path-shaped exclusions for build output.** David's call, and it is right: a user may run any tool that
churns hard, and the app must recognize and throttle that rather than carry a list of the ones we thought of. Every
mechanism here works having never heard of cargo.

`importance/classify.rs`'s `is_denylisted` stays where it is and keeps flooring a folder's *ranking*. It does not grow a
second job.

---

## M1: stop re-parsing the writer's hot INSERT (~24% of busy CPU)

`IndexStore::insert_entry_v2_with_id` (`crates/cmdr-index/src/indexing/store/entries.rs:453`) calls `conn.execute(...)`
with a literal SQL string. `rusqlite`'s `execute` prepares a fresh statement every call, so SQLite re-runs its parser per
inserted row. **The same file already uses `prepare_cached` in 21 other places**; this hot path is the exception, not a
considered choice.

`sqlite3GenerateConstraintChecks` → `sqlite3MPrintf` inside the parse is SQLite building constraint-violation message
strings at prepare time. It is pure waste on the success path and it goes away with the statement cache.

**The change**: `prepare_cached` on this path, plus an audit of `writer/entries.rs` and `writer/delta.rs` (zero
`prepare_cached` uses between them) for the same pattern.

**Why it is safe**: `prepare_cached` changes no SQL and no semantics; it caches the compiled statement per connection.
The writer is single-threaded per DB (`writer/CLAUDE.md`), so there is no cross-thread cache concern.

**M1b, land separately**: `propagate_delta_by_id` (`writer/delta.rs`) is 918 samples, of which 628 are `sqlite3VdbeHalt`
→ `vdbeCommit` → `sqlite3BtreeCommitPhaseOne` → `pagerWalFrames` → `walWriteOneFrame` → `pwrite`. That is a **transaction
commit and WAL write per delta**. Batching deltas into one transaction is the fix, but the `dir_stats` ledger's crash
semantics are the writer's whole contract, so this is correctness-sensitive. Keep it out of the statement-cache commit so
a regression stays bisectable.

**Tests**: a performance fix with no behavior change, so the honest instrument is a benchmark, not a unit assertion.
`benches/index_benchmarks.rs` already covers the enrichment and dir-stats hot paths; add or extend an insert-throughput
bench and record before and after. Existing writer correctness tests must stay green unchanged; if any of them change,
the fix is wrong. M1b, being correctness-sensitive, gets a real TDD cycle against crash semantics.

**Docs**: a one-line guardrail in `store/CLAUDE.md` (hot write paths use `prepare_cached`; `execute` with a literal
re-parses per call), plus a Decision/Why in the nearest `DETAILS.md` for M1b's batching semantics.

**Checks**: `pnpm check rust`, then `pnpm check -q`.

---

## M2: stop stat-ing every path twice for sync status (~23% of busy CPU)

`ubiquitous_bool` (`apps/desktop/src-tauri/src/file_system/sync_status/probe.rs:56`) builds its URL with
`NSURL::fileURLWithPath(&ns_path)`. The single-argument form has to determine whether the path is a directory, so
Foundation calls `_NSFileExists` → `stat` internally. `fileURLWithPath:isDirectory:` takes that answer as a parameter and
skips the syscall.

The caller `sync_status_for` already has directory-ness available (it does its own `stat`, 327 samples), so the
information is on hand and is not passed down. Fix both halves together: pass `is_directory` into the URL
construction, and stat the path once per probe rather than twice.

**Verify the premise before changing code.** `_NSFileExists` in the stack is strong evidence, but confirm that
`fileURLWithPath:isDirectory:` actually removes the syscall on this macOS version rather than deferring it, and record
the result with an evidence anchor (`(verified on macOS 26.5.2, sample, 2026-08-03)`) per `.claude/rules/docs.md`.

**The volume question this exposes.** 15,405 `sync_status` log lines in six hours, from a `log::debug!` that fires **once
per batch** (`service.rs:172-177`), means roughly **43 sync-status batches per minute on an idle app**. That is its own
finding. Before optimizing per-path cost, ask why an idle app probes sync status 43 times a minute at all, and whether
the answer is a missing cache or a subscription. Halving a syscall that should not be happening is the smaller win.

**Tests**: test-first is awkward for a syscall count, so assert the observable contract (sync status still reported
correctly for iCloud, Dropbox, and plain-local paths) and measure the syscall reduction with `sample` or `dtruss`,
stating the number. The batch-rate question gets its own test once its cause is known.

**Docs**: `sync_status/DETAILS.md`, a Decision/Why on the `isDirectory:` form with the evidence anchor.

**Checks**: `pnpm check rust desktop`.

---

## M3: bound the reconcile drain's ARRIVAL rate

Demoted from "the one" to "real but secondary" by M0. Still worth doing: it is unbounded by construction, it scales with
the user's workload rather than with anything we control, and it feeds M1's writer.

### Why the existing throttle does not fire, stated correctly

An earlier draft said "with N distinct anchors, aggregate cost is N/30". **That is wrong, and the `30 ×` factor never
enters.** `RescanThrottle::is_eligible` returns `true` unconditionally for an anchor with no completion record
(`rescan_throttle.rs:168-171`, the leading edge). Cargo's anchors are **one-shot**: each is walked once and never
consults its window. So the cost is `arrival_rate × walk_cost`, and the per-anchor throttle contributes nothing to it.

The failure is not "the per-anchor bound is too loose". It is **that nothing rate-limits arrivals**. That distinction is
the argument for a global shape and against a per-path one.

`reconcile/DETAILS.md:484` already names this class for an Electron updater: *"its signal is REPETITION, and every path
is unique, so no anchor ever reaches a second strike."* The settle delay answered it then only because those directories
vanished before settling. Cargo's persist, settle, and get walked.

### Inherited evidence: do not re-derive this

`docs/specs/later/sealed-subtrees-plan.md` targets this exact problem under this exact constraint, and carries two
completed spikes whose results this milestone must use:

- **Spike B (`docs/notes/churn-observability-spike.md`) already answers the depth question, negatively.** A climb rule
  based on churn share **over-climbs on real data**: it selects `~/Library/Containers` and `~/Library/Caches` rather than
  the actual culprit, because *"churn share alone cannot distinguish 'this parent is entirely churny' from 'this parent's
  churn is dominated by one child right now.'"* Its resolution, combining churn share with a **content ratio** (entries
  or bytes below the candidate versus below its parent), is the attribution rule this milestone needs, and it is already
  paid for. Spike B also measured that `target/` becomes classifiable within **~31 s**.
- **Spike A** established "schedule on a cost budget, not a fixed clock".
- `watch/churn_monitor.rs` is the ancestor-rollup instrument built for this, gated behind `CMDR_CHURN_SPIKE`. Turning it
  on is cheaper than inventing a new measurement.

**State the relationship explicitly**: does this plan supersede sealing, complement it, or defer to it? A sealed subtree
and a budget-refused subtree are two mechanisms deciding the same thing about the same directory, and one has to be
authoritative.

### Decisions made here, not delegated

- **Budget: 3% of wall clock per volume for the drain.** Move it only with a measurement.
- **Fairness: the foreground pane's anchor and its ancestors are exempt from the budget**; everything else is FIFO within
  the remainder. The architectural cost is real: `cmdr-index` does not know which pane the user is looking at, so this
  needs a new `indexing/host/` seam (`indexing/CLAUDE.md`: "❌ Anything the app must answer arrives here"). That seam is
  part of this milestone, not a footnote. Without it, `pick_and_collapse_rescan` (`reconciler/rescan.rs:328`) is
  `min_by_key(depth)` over `HashSet` iteration order, so among 3,704 same-depth anchors the winner is effectively random
  and "eligible again later" does not mean "picked later".
- **Anchoring unit**: constrained by Spike B above. Do not adopt a bare fixed depth without engaging it.

### Two corrections to assumptions in the earlier draft

1. **`rescan_churn.rs` does NOT roll up an ancestor chain.** It is flat per-anchor with a 64-entry cap and
   cheapest-eviction (`MAX_TRACKED_ANCHORS`; the `64+ anchors` in the log line is that cap, not a count). The rollup is
   in `watch/churn_monitor.rs`.
2. **`cost_budget.rs:37` argues explicitly AGAINST charging cost up the whole ancestor chain**: *"per-directory fractions
   would be noise… the unit refused would become 'whichever depth tripped first', which is neither predictable nor
   explainable."* Its answer is one accumulator at a fixed depth. Any design must argue past this.

### Constraints the design must honor

- **Composes as a further eligibility gate**, like settle and window: whichever says "not yet" wins, and every gate is an
  absolute deadline that passes on its own.
- **The pure, clock-injected discipline holds.** No filesystem, clock, or logging calls inside the engine.
- **A volume-wide budget on an EXTERNAL volume is a correctness regression.** `rescan_route.rs:58-70` is explicit: the
  per-navigation verifier is root-scoped and "bails inert on a mount-rooted volume", which is why external volumes get a
  45 s interval where the boot disk gets 24 h, because *"a 24-hour blind window there would be a pure correctness
  regression on the one volume kind with zero verifier cover."* A duty-cycle budget is that blind window renamed. Scope
  it to the boot disk, or give external volumes a much looser budget.
- **A volume-wide gate makes the hourglass flicker volume-wide.** Under a global budget, eligibility flips for every
  queued anchor at once, and `reconcile_with_eligibility` (`rescan_hold.rs:96-117`) re-derives every queued anchor's hold
  on the ~1 s sweep. A held root drags its ancestor chain to `/`. So `/` and `~` would blink "size updating" at the
  duty-cycle period, a regression in the exact property `rescan_hold.rs` exists to protect. A per-subtree shape only
  flickers the offending chain.
- **`record_held_back` must be fed by the new gate.** Its one call site (`rescan.rs:155`) is gated on
  `!throttle.is_eligible(...)`. A governor living outside `is_eligible` will not increment it, and the churn line's
  `held_back` field goes to zero during heavy churn, which `rescan_churn.rs:77-78` designates as *the* regression signal.
- **`gc` measures each record against its OWN window.** New state needs the same discipline and the same bounding.
- **A fourth shape to evaluate before adopting a budget.** High anchor cardinality is the same signal as
  `MustScanSubDirs` on `/`: the OS saying it can no longer track this incrementally. `rescan_route.rs` already answers
  that by routing to the visible scanner with a persisted once-a-day window and a green badge, reasoning that the anchor
  path "carries no diagnostic information" and the signal means *"this index is now SUSPECT."* Applying that disposition
  to a cardinality storm touches the hourglass invariant **not at all** (routed anchors leave `pending_rescans` entirely,
  so they hold nothing by construction), reuses `SweepRecord` and the existing user-facing story, is path-shape-blind,
  and carries the external-volume distinction for free. Its cost is that a full sweep is expensive
  (`rescan_route.rs:46-48` measured 1,309 s), trading many small walks for one big one. Argue it before choosing.

### Correctness and transparency when the governor refuses

Principle 4 is protect the user's data, and `docs/design-principles.md` calls for radical transparency. A refused subtree
means the index is knowingly behind: folder sizes go stale and search misses recent files. Decide, in
`reconcile/DETAILS.md`:

- **Does refusal touch `recursive_size_complete` or `min_subtree_epoch`?** Almost certainly it must not: the
  `absorbing_min_epoch` trap zeroes every ancestor up to `/`. Confirm a refused anchor stays queued and never stamps
  `listed_epoch`, per `cost_budget.rs:45-47`. Say so, so nobody rediscovers it the hard way.
- **Is staleness user-visible, or silent?** "Silent, and here is why that is acceptable" is a fine answer. No answer is
  not. Compare `sealed-subtrees-plan.md`, which faces the same debt directly: *"We knowingly decline to enumerate and
  still claim exact… radical transparency says own that debt."*
- **Repair coverage is asymmetric.** The per-navigation verifier repairs what the user looks at on the boot disk and is
  inert on SMB, MTP, and external, so the same budget buys very different staleness per volume kind.

### Tests

- **The real red is the integration test**: drive N unique anchors through the drain against current code and watch
  aggregate walk time blow the budget. A unit test of an engine that does not exist yet is not a red step; per
  `tdd-red-green.md`, do not label it TDD. Write the engine's unit tests alongside the engine and say so.
- Note that `disable_rescan_throttle_for_test` (`rescan.rs:293`) is consumed today only by
  `indexing/tests/stress_tests_concurrency.rs:853`, not by `reconciler/tests/live_events.rs`.
- Unit: budget refusal expires on its own; a refused anchor holds no hourglass; `gc` bounds the new map; the foreground
  anchor is never starved; `record_held_back` increments.
- **Regression anchor** named for this bug's shape, so a future tuning pass cannot reintroduce cardinality blindness.

**Checks**: `pnpm check rust`, `pnpm check -q`, `pnpm check --include-slow`.

---

## M4: gate the media tick before it walks, without breaking scoped GC

`run_live_tick_blocking` (`media_index/scheduler/live.rs:124`) walks at `:138` and computes its coverage gates at
`:151-152`, so thousands of ineligible dirs cost a `resolve_path` plus a `list_children_on` each before being rejected.

**The data-safety invariant this milestone lives or dies on.** Filtering only the walk input is a data-loss bug. The same
`touched_dirs` set is used three times:

- `:138`, the walk
- `:197`, `GcScope::TouchedDirs(touched_dirs)`, which deletes every stored row whose parent dir is in the set and absent
  from the walk
- `:224`, `patch_touched_dirs(...)`, which patches coverage counts from the same pairing

Filter the walk but not the GC scope and you delete every media row (OCR text, Vision tags, CLIP embeddings) in every dir
the filter removed. That is the trap `media_index/scheduler/CLAUDE.md` warns about, reached from the other side, and it
contradicts `media_index/CLAUDE.md`'s "Uncovered rows STAY: narrowing a setting deletes nothing."

**So: compute one filtered set once and thread it to all three.** That is the milestone's named invariant and its first
test.

The gate is implementable: `local_should_enrich` (`scheduler/lifecycle.rs:44-54`) is
`config.covers(volume_id, path) || scores.contains_key(parent_dir(path))`, and `covers` is a prefix test
(`media_index/network/config.rs:48-51`), so a dir-level pre-filter is sound.

**Measure the gate before reordering around it.** `:151-152` already calls `gate::importance_threshold()` plus
`folder_scores()` unconditionally every tick, which is an `ImportanceIndex::open` plus an `above_threshold` that
materializes a map over 90,308 folders and 161,094 weights (`idle-memory-profile-2026-07-28.md`), **every 60 s forever**.
That may cost more than the walk being moved behind it. Step one is measuring it, not reordering.

**Test idiom correction**: `CountingOpener` (`sqlite_util/tests.rs:91-108`) counts connection **opens**, not statement
executions, and `walk_image_entries_in_dirs` takes a `&Connection`, so there is no seam there. Test the filtered set as a
pure function, and assert on the returned `images` plus the GC scope.

**Tests**: test-first on the invariant (a filtered-out dir loses no rows), then on the pure filter, then that an eligible
dir still enriches, so the gate cannot be trivially "correct" by gating everything out.

**Docs**: `media_index/scheduler/DETAILS.md`, Decision/Why on gate-before-walk and the one-set invariant.

**Checks**: `pnpm check rust`.

---

## M5: make the SQLite page-cache bound real, and find the actual 643 MB

**The earlier draft's central claim was wrong.** It treated the whole ~795 MB system C heap as SQLite page cache. With
`SQLITE_ENABLE_MEMORY_MANAGEMENT` defined, `pcache1.separateCache = 0`, so `nInitPage = 0` and `pcache1InitBulk` returns
immediately: **there is no bulk allocation**, and every overflow page is an individual `sqlite3Malloc(szPage + hdr)` of
about 4.1 KB. macOS routes that to the *small* zone (the large threshold is 127 KB). **Page-cache overflow can therefore
only appear in `MALLOC_SMALL` (152 MB), never in `MALLOC_LARGE` (643 MB)**, and the plan's own evidence, regions of 9 MB
and 2.25 MB, confirms those are something else.

The prior note corroborates: `idle-memory-profile-2026-07-28.md:15-16` recorded `MALLOC_LARGE 730 MB / MALLOC_SMALL
405 MB` and asserted "for us, ~all SQLite". After the slab shipped, `MALLOC_SMALL` fell 405 → 152 (−62%) while
`MALLOC_LARGE` moved 730 → 643 (−12%). The slab did exactly what it should to page cache and barely touched
`MALLOC_LARGE`. **That note's "~all SQLite" line is the error this plan inherited, and it must be corrected there too.**

- **M5a: identify the 643 MB.** Now the primary unknown, and the largest unattributed block after the 947 MB Rust heap.
  It is not page cache. Do not proceed to a fix before naming it.
- **M5b: bound page-cache overflow, ceiling ~152 MB.** Evaluate in this order:
  1. **Cut the multiplier, not just the product.** The docstring's promise ("total page memory is THIS number no matter
     how many connections exist") holds only if `Σ nMax ≤ slab slots`; today that is 132 × 8 MiB against 64 MiB. Cutting
     `READ_PAGE_CACHE_KIB`, or cutting the connection count (60 connections to one DB is itself the anomaly, driven by
     `THREAD_CONN_SLOTS = 3` times tokio's blocking pool), is config-only, adds no FFI, adds no thrash risk, and makes
     the existing docstring true instead of bolting a second mechanism onto a false one. `idle-memory-profile` already
     flagged this as unresolved: the count *"tracks tokio's blocking-thread pool, not anything semantic"*. Under
     "elegance above all", bounding the multiplier is the fix and a second ceiling is the hack.
  2. **`sqlite3_soft_heap_limit64` as a backstop**, if still wanted after (1). Two things the earlier draft asserted
     past: the limit is **advisory** (`sqlite3.c:7697`: *"it will exceed the limit rather than generate an
     `SQLITE_NOMEM` error… advisory only"*), so a test asserting the bound holds is flaky-green rather than green; and
     under one unified `PGroup`, limit pressure runs the global LRU under `pcache1.mutex`, which `pcache1AllocPage` must
     drop and retake around `pcache1Alloc` (`sqlite3.c:58095-58108`). With 71 threads in SQLite, steady limit pressure
     means constant global eviction under a contended mutex, and it can evict a hot volume's working set to serve a cold
     one. Weigh that.
- **Fix the false docstring** at `sqlite_util.rs:26` and `:39` whichever fix lands, and say what the slab bounds versus
  what anything else bounds.

**Citation correction**: the workspace resolves `libsqlite3-sys` **0.38.1** (`Cargo.lock:5369`, via `rusqlite 0.40`), not
0.37.0. The `-DSQLITE_ENABLE_MEMORY_MANAGEMENT` flag is at `0.38.1/build.rs:135`, so the premise survives.

**M5 is not "independent, touches only `cmdr-fs`"**: M5a needs a dev diagnostic surface, so an app-side IPC command plus
`bindings.ts`.

**Test hazard**: `sqlite3_status64` counters are process-global and Rust tests run in parallel in one process, so a
shared-global-plus-reset fixture is a race, exactly as `rescan_route.rs:183-185` documents ("it flaked exactly that
way"). Installing a process-wide limit inside the test build also changes every other test's behavior.

**Predict the yield.** State what footprint M5 is expected to deliver, so M5a's readout can confirm or refute it. Note
that the 947 MB Rust heap is the single largest item and no milestone here targets it;
`docs/notes/memory-runaway-rust-heap-2026-07-25.md` is the on-topic prior.

**Checks**: `pnpm check rust`. No new `file-length` or `claude-md-length` allowlist entries without David's consent.

---

## M6: halve idle log volume and make what is left informative

**Retitled from "~0.1% of current", which was off by roughly 500×.** The targets below cover 74,103 of 141,072 lines
(53%). The remainder (13,133 `writer`, 8,590 `smb2` and 5,911 SMB `WARN` both out of scope, 7,694 `stall_probe`, plus
~31,600 unattributed) is untouched. Best case is roughly half.

- `reconciler.rs:1193` and `:1265`: `reconcile: can't read {path}`, 19,705 lines. The expected race with a compiler
  deleting files mid-walk, not a diagnosis. Count per walk, fold the count into the reconcile summary, individual lines
  to TRACE.
- `rescan.rs`: `MustScanSubDirs for {path} queued`, 32,479 lines. Already dedups per exact path, which unique paths
  defeat. Replace with a per-window counter on the churn line: "queued 27,617 signals across 3,704 anchors". Strictly
  more informative, far less volume.
- `sync_status/service.rs:175`: 15,405 lines. **Do not simply demote this.** Per M2 it fires once per batch, so it is
  measuring a 43-batches-per-minute idle rate that is itself a finding. Fix the rate and the line goes quiet on its own.
  Check `:146`, `:232`, `:242`, `:256`, and `:262` before attributing all 15,405 to one site.
- `space_poller.rs` per-tick lines: 6,514. To TRACE.

**Name the tradeoff.** `docs/tooling/logging.md:42` makes the file sink unconditionally DEBUG *on purpose*, so
error-report bundles carry full context. **Every TRACE demotion trades field diagnosability for volume.** The reconcile
bullet mitigates that by folding a count into the summary; any demotion without such a mitigation silently removes those
lines from every crash bundle. Say which ones are which.

**Not mechanical**: the churn-line rework has to update exact-string assertions at `rescan_churn.rs:342-346`, `:366-376`,
`:489-492`, and `:506-511`, plus `docs/tooling/logging.md:196-209`.

**Verification**: run the app and count lines per hour. State the actual before and after numbers.

**Checks**: `pnpm check rust`, plus the doc checks.

---

## M7: space poller, gated on being worth it

**Gate**: M0 puts `space_poller` well outside the top 20 threads, so unless a measurement contradicts that, do only the
cheap half (drop the per-tick log to TRACE, which M6 covers) and stop. **Skip the adaptive-decay policy entirely.**
Building a clock-injected decay engine with hysteresis to save a fraction of a percent of a core is the
cost-exceeds-value case in this plan.

An `fs_info` round trip to a NAS every 5 s forever is still wrong per "subscribe, don't poll", so if it does proceed:

- **The adaptive-decay idea targets the wrong volume.** The poller only logs on `emit()` (`:433`), which fires only when
  a change exceeds the ~1 MB threshold (`:400-416`). So the 6,514 lines mean the **boot volume's** free space genuinely
  moved at least 1 MB every ~3 s. Decay would essentially never engage there, and would engage on the NAS, which produces
  almost none of the lines.
- **"Don't poll a volume no pane is showing" is already true.** `poll_loop` iterates `WATCHED` (`:226-237`), which holds
  pane registrations plus the one permanent boot watcher. That bullet is a no-op.
- Giving the boot watcher its own slow cadence breaks the documented dedup at `space_poller.rs:17-19` (a pane watching
  the boot volume shares one `statfs` per tick with the permanent watcher). Not fatal, but do not trade a stated design
  property silently.
- **Expect a negative on subscriptions for BOTH local and SMB.** DiskArbitration and `NSWorkspace` volume notifications
  are mount, unmount, and eject level; `volumeAvailableCapacityKey` is a pull. Deriving free space from watched files is
  wrong anyway (sparse files, compression, snapshots, purgeable space, other processes). SMB2 has no `fs_info`
  subscription. Record the negative as a finding with an evidence anchor so it does not read as a failure.
- If pausing while hidden or unfocused, **refresh on focus** is what makes it safe. Say it.
- **Linux**: Cmdr has a Linux lane and DiskArbitration is macOS-only. Provide a counterpart or note explicitly that this
  is a macOS-only optimization.
- The poller tries `volume.get_space_info().await` first and falls back to `crate::volumes::get_volume_space`
  (`:260-270`).

**Careful**: the low-disk-space hysteresis detector and the `volume-space-changed` stream behind the live toast both ride
this loop. Backing off must not make the toast's numbers stale while it is on screen. That is user-visible, so it needs a
test and David reviews it.

**Checks**: `pnpm check rust desktop`.

---

## Risks

- **The M3 governor is too aggressive and the index goes stale in a way users notice.** Mitigation: ship an env kill
  switch (`CMDR_DISABLE_RESCAN_BUDGET=1`). There is a test-only `disable_rescan_throttle_for_test` and no field
  equivalent; a bad release should be a support reply, not a hotfix.
- **Tuned for one workload.** Every number here comes from one machine running six worktrees with active cargo builds.
  Mitigation: the synthetic harness below, plus a quiet-machine sanity check.
- **M1b's delta batching changes crash semantics.** The `dir_stats` ledger's crash behavior is the writer's contract.
  Land it separately from the statement cache so a regression is bisectable.
- **M5b's soft heap limit thrashes instead of reclaiming**, turning memory pressure into IO. Assert that query latency
  did not regress, not only that the bound held. `docs/notes/search-latency-2026-07-28.md` has a baseline.
- **No field observability.** `AGENTS.md` says anonymous analytics to PostHog is live. This plan measures on one machine
  while claiming to fix a mechanism that must work against tools nobody anticipated. Add at least: anchors refused per
  session, peak budget utilization, page-cache overflow high-water. Without it we have fixed David's laptop, not Cmdr.

## Definition of done

**A repeatable harness, not "a comparable idle period".** The baseline came from a machine with six worktrees and active
builds; measuring the after on a quiet afternoon would show an 80% improvement from the weather. Build a small tool
(precedent: `scripts/reanchor-cost`) that mints N unique deep directories at a known rate (say 200 unique anchors per
minute at depth 8, files touched then left in place) and measures CPU over 15 minutes. It is denylist-free by
construction, reproduces the exact failure mode, and doubles as M3's integration red.

Targets, to be sharpened once M1 and M2 land and the attribution is re-run:

- **M1 and M2 together should remove roughly half of busy CPU.** Verify by re-running M0's per-thread attribution and
  showing that `index-writer` and `cmdr-sync-status` shares dropped.
- Idle CPU under a stated percent of one core on a quiet machine, and under a stated percent under the harness.
- Footprint under a stated ceiling after eight hours, with M5's predicted contribution named separately.
- Log lines per hour, stated before and after.
- `pnpm check --include-slow` green.
- Colocated `CLAUDE.md` and `DETAILS.md` updated per `.claude/rules/docs.md`, with Decision/Why where a design choice was
  made, including the correction to `idle-memory-profile-2026-07-28.md`.

## Sequencing

M1 and M2 first: they are the measured majority of the cost, the smallest changes in the plan, and independent of
everything else and of each other.

1. **M1** (writer statement cache), then **M1b** (delta batching) separately.
2. **M2** (sync-status double stat), including the batch-rate question.
3. **Re-run M0's attribution.** Everything below is sized against numbers M1 and M2 will have changed.
4. **M5a** (identify the 643 MB) early if it needs hours of collection under load; it gates M5b.
5. **M3** (arrival-rate governor): the design pass, then the integration red.
6. **M4** (media tick), measuring `folder_scores` first.
7. **M6** (log volume), after M3 so the remaining shape is clear.
8. **M7** (space poller), only if its gate opens.

**Safe to parallelize**: M1, M2, and M5a touch disjoint trees (`indexing/store` plus `indexing/writer`, app-side
`sync_status`, `cmdr-fs` plus a dev IPC surface). M3 and M4 share no files either, but M3 changes M4's input volume, so
running them together muddies the *measurement* rather than risking a conflict. That is a weaker reason than a merge
hazard, and M4 is correct regardless.

## Out of scope, tracked elsewhere

The SMB `ChangeNotify` long-poll liveness bound and the 5,911 sweeper `WARN`s are owned by a dedicated agent in
`~/projects-git/vdavid/smb2`, serving that library's interests first. Cmdr consumes the resulting release afterwards.
Leave the seam: do not work around the warning volume on Cmdr's side.

Diagnosis for reference: `is_long_poll(ChangeNotify)` (`connection.rs:341`) correctly exempts notifies from the request
deadline, and `:335` says they are "bounded by the connection instead of by themselves". Every liveness verdict is
connection-level, so when the connection is healthy and only the long-poll is dead, that means bounded by nothing.
Measured: `fs_info` round-tripping in 4 ms and Echo `msg_id` climbing steadily while two `ChangeNotify`s sat unanswered
for 6,186 seconds.
