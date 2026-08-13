# Cutting Cmdr's idle CPU and RAM

**Status**: SPECCED, not started. **Owner**: David. **Date**: 2026-08-03. **Baseline**: prod v0.37.0, repo at
`3d4c9816f`.

Prod v0.37.0, measured 2026-08-03 on David's machine: **110 minutes of CPU over 9.1 hours** (about 20% of one core,
sustained), **1.78 GB physical footprint** (2.8 GB peak), and **141,072 log lines / 28 MB in six hours**. The app was
idle throughout. Principle 5 says respect the user's resources; this is a long way from that.

**Read these first** (an executing agent should not start without them):
`crates/cmdr-index/src/indexing/writer/CLAUDE.md`, `crates/cmdr-index/src/indexing/store/CLAUDE.md`,
`crates/cmdr-index/src/indexing/reconcile/CLAUDE.md` and its `DETAILS.md`,
`crates/cmdr-index/src/media_index/scheduler/`, `crates/cmdr-fs/src/sqlite_util.rs` module docs,
`docs/tooling/memory-debugging.md`, `docs/tooling/logging.md`, `docs/notes/idle-cpu-indexing-streamlining-2026-07.md`,
`later/indexing/sealed-subtrees-plan.md`.

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

Both are confirmed in code (M1 and M2).

### What this method does and does not measure

**The blocking-frame list above is a list of SCHEDULER waits. It does not include `stat`, `pread`, `pwrite`, `open`, or
`read`.** So a sample parked in a file-IO syscall is scored as busy. That bias is not uniform, and it lands exactly on
the milestones whose stacks cannot distinguish the two:

- **M1's 1,828 samples are pure userspace codegen** (`sqlite3RunParser` and friends, zero syscalls). **Trustworthy as
  CPU.**
- **M2's 745 samples are `stat`.** On plain local paths that is mostly CPU; on iCloud, Dropbox, or FileProvider paths it
  is provider latency. `probe.rs:6-11` and `apps/desktop/src-tauri/CLAUDE.md` both warn that these calls can wait 30 to
  120 seconds. **Treat M2's share as an upper bound on CPU, not a measurement of it.**
- **M1b's 628 samples are `walWriteOneFrame` → `pwrite`.** That is disk IO. It is still Principle 5, but it is a
  different claim and must not be reported as a CPU share.

`docs/notes/reanchor-cost-spike.md:62` already records this error class for this repo: _"The cost is IO wait, not
syscalls. CPU time is 16–23% of wall on the big directories."_ This plan applies that deflator to the reconcile drain
and must apply it consistently to M2 and M1b.

**The window is also short**: 20 seconds used to attribute 110 minutes over 9.1 hours. It cannot distinguish "45%
sustained" from "one burst that happened to be running", which is exactly why the media tick reads 586/3,425 in one
window and zero in another.

**Before committing to the M1-then-M2 order, re-measure**, cheapest first:

1. **`ps -M <pid>`** gives per-thread cumulative CPU since launch, integrating the whole 9.1 hours with no heuristic. It
   reports no thread names, so correlate by taking it alongside a `sample`.
2. **A longer `sample`** (180 s or more) classifying leaves into three buckets, not two: userspace, file-IO syscall, and
   parked. Report the first two separately; the gap is the size of the correction.
3. **Instruments Time Profiler** (`xctrace record --template 'Time Profiler'`) samples on-CPU threads natively rather
   than approximating with a frame list.

### The re-measurement was run, and it does not agree with the 20 s window

A 180 s sample with three-bucket leaf attribution (userspace / file-IO syscall / parked), same process:

- `cmdr-sync-status` ×4: 3.4% of busy, but **0.2% of userspace CPU**. 1,964 of 2,037 samples per thread are the `stat`
  itself. **The prediction held: M2's "23% of CPU" was wrong.** It is syscall and provider-latency time, not CPU.
- `index-writer` **does not appear in the top 12 at all** in this window. The `sqlite3RunParser` prepare path that
  dominated the 20 s window is absent.
- The tokio pool holds 95.7% of userspace CPU, and its hot leaves are SQLite b-tree traversal (`sqlite3BtreeTableMoveto`
  8.8%, `sqlite3VdbeExec` 5.8%, plus `getPageNormal`, `sqlite3GetVarint`, `pcache1Fetch`, `pcache1Unpin`, `moveToRoot`,
  roughly 20% of userspace CPU together) and `DirTree::path_at_into` (6.3%), reached through
  `indexing::watch::event_loop::live::process_live_batch` and `importance::scheduler`'s incremental rescore walk.

**So M0 is NOT settled, and this plan must not commit to a milestone order on it.** Two windows on the same idle process
disagree about which thread dominates, which means the workload is bursty at a period longer than either window. What
survives both:

- **M1's defect is real independent of sampling**, because it was confirmed by reading the code: `conn.execute` with a
  literal re-parses per row, and the same file uses `prepare_cached` 21 times. Its _share_ is unknown; its _wrongness_
  is not. Fix it on those grounds, not on a percentage.
- **M2's CPU claim is refuted.** The 43 sync-status batches per minute are still real waste (syscalls, IO, and load on
  `fileproviderd`), so M2a keeps its value, but M2 must be re-argued as an IO and provider-load win, **not** as ~23% of
  CPU. It has no claim on being second.
- **A third candidate appeared that no milestone covers**: the importance incremental rescore walking index folders
  inside the live event batch, with SQLite b-tree traversal underneath. `idle-memory-profile-2026-07-28.md` § "Cause 2"
  reports that treadmill as FIXED. Either the fix is incomplete or something re-armed it. **Investigate before
  sequencing anything after M1.**

**Still required before the order is trusted**: `ps -M <pid>` for cumulative per-thread CPU over the whole session (it
integrates every burst instead of sampling one), or repeated samples across hours. Until then, treat every share in this
section as provisional and the ordering below as a hypothesis.

Two further caveats on the numbers here. The CPU baseline covers 9.1 hours, the log counts 6 hours, and the churn counts
8 hours, so do not divide one by another. And this machine runs six Cmdr worktrees with active cargo builds: a heavy
case, not an unrepresentative one, but the fixes need a quiet-machine sanity check so we do not tune for one workload.
See § Definition of done.

## MT: the 60-second rescore treadmill is back (found last, ranks first)

Found while chasing M0's "third candidate". It is a regression of a fix `docs/notes/idle-memory-profile-2026-07-28.md` §
"Cause 2" records as shipped, it runs forever on an idle app, and it matches the 180 s sample's hot stacks exactly. **Do
this before anything else.**

Every ~60 seconds, without a user touching the app, the log shows the same three lines:

```
21:21:27  DEBUG importance  incremental rescore takes the full walk: the changed subtrees cover too much of the volume
21:21:42  DEBUG importance  incremental rescore of 'root' updated 52071 folders
21:21:42  DEBUG search      importance weights loaded for 'root': 160718 scored folders
```

842 rescore lines and 474 weight loads in the current log. The folder count is pinned at 52,071 and the weight count at
160,718, so **nothing is changing and it rewrites and reloads all of it anyway.**

### The mechanism, end to end

1. `try_scoped_walk` (`importance/scheduler/scoped_walk.rs`) gives up when the changed subtrees hold more than
   `SCOPED_WALK_MAX_DIRS = 20_000` directories, returning `FullWalkReason::SubtreesTooLarge` (`:76`, `:94`).
2. `walk_for_incremental` (`scheduler/recompute.rs:296-300`) then runs the **full O(dirs) walk** over the whole 6.5
   M-row root index and escalates the scope to `RescoreScope::WithAncestors`.
3. That rewrites 52,071 folder rows and wakes the search weight reload, which reads all 160,718 weights.

The full walk is measured at ~9 µs per folder (5.5 s over 611,699 folders, `scheduler/DETAILS.md:253`). Running it once
a minute forever is the shape `idle-memory-profile-2026-07-28.md` describes as the treadmill, and the 180 s sample's top
stacks (`importance::scheduler::spawn_incremental` → `recompute::walk_for_incremental` → `walk::walk_index_folders`,
over `sqlite3BtreeTableMoveto` and `DirTree::path_at_into`) are exactly this path.

### Why the 2026-07-28 fix does not hold here

That fix added `sanitize_incremental_batch`, which drops paths that floor by path (`target/`, `Library/Caches`,
dot-directories) before the walk. It reduces the batch; it does not bound the **subtree size** the surviving origins
cover. On this machine the changed subtrees still exceed 20,000 directories every minute, so the `SubtreesTooLarge`
escape hatch fires and the pass takes the full walk anyway. **The escape hatch has become the default path.**

### This is the same root cause as M3, seen from the other end

Both are "too much churn arrives, so a bounded mechanism gives up and does the expensive thing". Worth stating in the
design, because a fix for one may be the fix for both, and because `SCOPED_WALK_MAX_DIRS` is exactly the kind of
cardinality cliff M3 exists to stop falling off.

### INVESTIGATED 2026-08-04. Three of the premises above are wrong.

Full write-up with the measurements: `docs/notes/importance-treadmill-2026-08-04.md`. Corrections first, because the
framing above misled once already:

- **The counts are not "pinned".** Across 681 rescores the folder count takes hundreds of values (`52071` 12 times,
  `51920` 28, also `17`, `4`, `0`). Nearly stable, not pinned, and that distinction is why a naive diff fails.
- **It is not "every 60 s forever".** An incremental pass runs every 60 s (681 in 10.5 h), but the FULL walk fired 330
  times, in bursts, and stopped entirely after 22:42.
- **Q4 was already fixed on `main`, just unreleased.** v0.37.0 was tagged at 07:37 on 2026-08-03; the delta reload
  landed at 11:25 the same day (`8d8118132`). `git merge-base --is-ancestor 8d8118132 v0.37.0` says NO. The running
  binary had no delta path, hence 616 `loaded` lines and zero `patched`.

**Q1, the actual trigger, and it is one path.** `origin_dir` (`reconcile/reconciler.rs:1616`) is the PARENT of the
changed file, so any file written directly in `~` makes `$HOME` an origin. `$HOME` does not floor, and it covers
**574,007 of the volume's 694,963 directories (83%)**, so its subtree instantly blows the 20,000 cap. The writers are
ordinary dotfile churn: `~/.claude.json` (Claude Code, constantly), `~/.zsh_history`, `~/.zcompdump`. The clincher: the
last full-walk fallback in the whole log is **22:42:27**, and the last write to `~/.claude.json` is **22:42:19**. Eight
seconds apart, and the treadmill never fires again. Only 19 non-floored dirs on the volume have >20,000-dir subtrees,
and every one except `$HOME` / `/Users` / `/` is static.

**Q2, refuted by measurement. Do NOT raise `SCOPED_WALK_MAX_DIRS`.** With the cap lifted to 2,000,000 against a copy of
the real 7.0M-row index: `$HOME` scoped walk **6.02 s** versus the full walk's **4.9 s**, so raising the cap makes the
one origin that actually fires _worse_. The general reasoning ("subtree is 20k, volume is 600k") is sound, and the
crossover is ~440,000 dirs, but `$HOME` sits past it. The abandoned probe costs 31 ms, which is not worth optimizing.

**Q3, confirmed defect and the biggest single cost.** There is no diff anywhere: `rescore_rows` (`recompute.rs:498`)
scores every folder unconditionally and `apply_incremental` (`writer.rs:373`) clears each subtree and re-inserts every
row. Measured against a fresh recompute over the same snapshot, of 51,081 rows in the `$HOME` subtree:

- **identical `signals_json`: 51,021 (99.88%)**
- identical `score`: **17 (0.03%)**

**So `signals_json` is the equality key and `score` is not.** `recency()` (`scorer/mod.rs:214`) reads `now_secs`, so
every score drifts ~2e-6 per pass; `FolderSignals` is entirely clock-free. A score diff would find nothing to skip.

Cost, from the log's own timestamps over 330 full-walk passes: **median 15.3 s, mean 20.1 s, max 363 s, total 6,639 s ,
17.6% of the 10.5-hour log spent inside an importance pass.** Uncontended measurements put the walk at 4.9 s and the
write at 558 ms, so ~10 s of each pass is contention with six concurrent cargo builds. That decomposition is inference,
not measurement, and is flagged as such.

### Decisions

- **Landing now: skip rows whose signals blob is unchanged.** Correct on its own merits, worth 99.88% of the write, and
  semantics-preserving (the stored data ends up identical). Guarded by the clear/insert agreement invariant and the eval
  suite.
- **For David: bound the origin by subtree size.** `dir_stats.recursive_dir_count` is already populated and exact
  (verified: reads 574,006 against a computed 574,007) and is a single indexed PK lookup, so a pass can ask "how much of
  the volume does this origin cover?" in O(1), replacing the 31 ms probe. **This is cardinality-based, not path-shaped,
  which is exactly the mechanism M3 exists to provide.** What to do with an over-budget origin is the open question:
  drop it (staleness the next full pass heals) or demote it to "rescore the origin and its ancestors, not its subtree"
  (more honest, since a dotfile write in `~` genuinely cannot change any descendant's signals). The demotion is the
  recommendation. It is a semantic change, so the eval suite applies.
- Note that even once the delta reload ships, `MAX_DELTA_ROWS` is 10,000 while a `$HOME` pass writes 51,081, so this
  treadmill still forces `ReloadAll`. Fixing the origin bound removes that; leaving it means the delta does not help
  here.

### The original investigation list (kept for the questions it framed)

1. **Why do the changed subtrees exceed 20,000 dirs on an idle machine?** Log the origins and the descent count at the
   bail. If `sanitize_incremental_batch` is letting build output through, that is the bug and it is upstream of
   everything here.
2. **Should `SubtreesTooLarge` fall back to a full walk at all?** Falling back to the most expensive option when the
   cheap one is overloaded is backwards under load. The alternatives are to walk the subtrees in bounded chunks across
   passes, or to skip the pass and let the next one try, or to raise the cap. Each has a staleness cost; argue it.
3. **Why does an unchanged result rewrite 52,071 rows?** A pass that finds nothing changed should write nothing. If the
   rescore is not diffing against stored values, that is a second, independent defect.
4. **Why does the weight reload read all 160,718 weights?** The 2026-07-28 fix made it a delta
   (`importance/read/DETAILS.md` § "The reload contract"). A full reload every minute means the delta path is bypassed
   on the `WithAncestors` scope, which is precisely the scope this treadmill takes.

**Tests**: test-first. The characteristic case is an idle volume whose changed-subtree set exceeds the cap: assert the
pass does not run a full walk every window, and that a pass finding no changes writes no rows. There is a differential
oracle already (the full walk is the scoped walk's oracle), so correctness has a ready check.

**Docs**: `importance/scheduler/DETAILS.md`, plus a correction to `idle-memory-profile-2026-07-28.md` § "Cause 2", which
currently reports this fixed.

**Checks**: `pnpm check rust`, `pnpm check --include-slow`.

## RESOLVED, negative: the search arena IS dropped, and is not the memory culprit

Checked 2026-08-04 against the same prod log. The drop path works:

```
16:22:10  DEBUG search::volumes  Search idle timeout reached, dropping indices
16:22:10  DEBUG search::volumes  Search indices dropped (all volumes)
16:53:03  DEBUG search::volumes  Search idle timeout reached, dropping indices
16:53:03  DEBUG search::volumes  Search indices dropped (all volumes)
```

The earlier "no drop line in five hours" claim was a **bad grep**, not a finding: it searched for "Search index
dropped/unloaded/released/evicted" while the actual message is "Search indices dropped (all volumes)". The arena loaded
at 16:15 and 16:16 and was gone by 16:53, so the 947 MB Rust heap measured around 21:00 is something else.

That eliminates the largest single memory candidate for the cost of one grep, which is what made it worth doing first.
**The remaining hypothesis, and it is now the leading one: MT.** A full walk over ~600k folders plus a 160,718-entry
weight map, every 60 seconds forever, churns hundreds of MB per minute. That is exactly the shape that leaves mimalloc
holding large arenas, and it fits the measured "947 MB dirty plus 725 MB reclaimable" better than any allocation leak
would. Re-measure the footprint after MT lands before hunting further.

The stale text below is kept only for the second lead, which still stands.

### The original entry (its premise refuted above)

`apps/desktop/src-tauri/src/search/DETAILS.md:9` says the search index "loads lazily on dialog open and drops after idle
(5 min timer + 10 min backstop), **~600 MB resident while active**". The log shows it loaded twice:

```
16:15:38  DEBUG search::index  Search index loaded: 6562042 entries, generation 1340883, took 3.301279875s
16:16:39  DEBUG search::index  Search index loaded: 6562034 entries, generation 1341019, took 3.727664375s
```

and **no drop line in the five hours since**. Either the drop is not logged, or the arena is still resident. That is a
yes-or-no question worth ~600 MB of the 947 MB Rust heap, and it is the cheapest memory lead in this plan. **Answer it
before M6a's hunt for the 643 MB `MALLOC_LARGE`**, which is the smaller number.

Second lead on the same heap: the evidence line reads "947 MB dirty plus 725 MB reclaimable", and
`docs/tooling/memory-debugging.md:26-27` says a collapsing balloon is "usually mimalloc decommitting pages". So part of
the Rust heap may be mimalloc holding rather than using, which is a purge-tuning question, not an allocation hunt. Name
it so nobody spends a day looking for a culprit that does not exist.

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

**No denylists. No path-shaped exclusions for build output.** David's call, and it is right: a user may run any tool
that churns hard, and the app must recognize and throttle that rather than carry a list of the ones we thought of. Every
mechanism here works having never heard of cargo.

`importance/classify.rs`'s `is_denylisted` stays where it is and keeps flooring a folder's _ranking_. It does not grow a
second job.

---

## M1: stop re-parsing the writer's hot INSERT (~24% of busy CPU)

`IndexStore::insert_entry_v2_with_id` (`crates/cmdr-index/src/indexing/store/entries.rs:453`) calls `conn.execute(...)`
with a literal SQL string. `rusqlite`'s `execute` prepares a fresh statement every call, so SQLite re-runs its parser
per inserted row. **The same file already uses `prepare_cached` in 21 other places**; this hot path is the exception,
not a considered choice.

`sqlite3GenerateConstraintChecks` → `sqlite3MPrintf` inside the parse is SQLite materializing constraint-violation
message strings ("UNIQUE constraint failed: entries.parent_id, entries.name_folded" and the NOT NULL equivalents) as P4
operands in the VDBE program. `entries` has both a UNIQUE index and NOT NULLs, so there are several per prepare. **This
is not waste**: it is the normal, correct cost of _preparing_ an INSERT against a constrained table. The defect is
paying it **once per row instead of once per statement**.

**The change**: `prepare_cached` on this path, plus `insert_entry_v2` at `entries.rs:403`, **which has the identical
bug** (same `conn.execute` with a literal, no id) and must be fixed or explicitly excused. Then audit
`writer/entries.rs` and `writer/delta.rs` (zero `prepare_cached` uses between them) for the same pattern.

### ⚠️ The trap that would silently undo this

**rusqlite's statement cache holds 16 entries** (`rusqlite-0.40.1/src/lib.rs:168`,
`STATEMENT_CACHE_DEFAULT_CAPACITY = 16`), a per-connection LRU keyed by SQL text. The writer connection already runs
**31 distinct `prepare_cached` sites**: `store/entries.rs` 21, `store/dir_stats.rs` 6, `store/meta.rs` 2, `store/mod.rs`
1, `store/connection.rs` 1. That is nearly twice the cache, so the LRU evicts and `prepare_cached` silently re-prepares,
**reintroducing exactly the cost being removed, with no error and no failing test**. Adding this milestone's new sites
pushes it further past the ceiling.

**So M1 is not one line.** It must also call `set_prepared_statement_cache_capacity` (`rusqlite-0.40.1/src/cache.rs:48`)
on the writer connection, sized above the distinct-statement count, in the same place `apply_pragmas` runs so it holds
by construction, with a comment tying the number to the count. Without this, M1 is a coin flip that looks like it worked
in a microbenchmark (few distinct statements) and does nothing in production.

**Why it is otherwise safe**: `prepare_cached` changes no SQL and no semantics, and the writer is single-threaded per DB
(`writer/CLAUDE.md`), so there is no cross-thread cache concern. The connection is opened once at `writer/mod.rs:507`,
outside the loop, so the cache persists for the process. Watch one path: `insert_with_allocated_id`
(`writer/entries.rs:429-432`) calls the insert **twice** on a PRIMARY KEY conflict. That is fine with a
`CachedStatement` (it returns to the cache on drop), but the retry must stay covered by existing tests.

### What M1 actually optimizes, and what it compounds with

`insert_entry_v2_with_id` has exactly two non-test callers: `writer/entries.rs:415` (via `insert_with_allocated_id`, the
single-row `UpsertEntryV2` path) and `store/dir_tree.rs:201`, which is `#[cfg(test)]`. So **M1 optimizes the live
reconcile write path specifically**; the scan path already batches through `insert_entries_v2_batch` (`entries.rs:485`),
which gets `prepare_cached` _and_ a savepoint. Two consequences: point the benchmark at the live path, and note that
**M1 and M3 compound** (M1 makes each row cheaper, M3 reduces the rows).

**A third option worth evaluating rather than assuming**: route the live path through `insert_entries_v2_batch` too.
That removes the parse _and_ the per-row transaction in one change, which subsumes M1b. Weigh it against the latency
cost of batching a single-row live event.

**M1b, land separately**: `propagate_delta_by_id` (`writer/delta.rs`) is 918 samples, of which 628 are `sqlite3VdbeHalt`
→ `vdbeCommit` → `sqlite3BtreeCommitPhaseOne` → `pagerWalFrames` → `walWriteOneFrame` → `pwrite`. `writer/delta.rs:6`
says these run "inside whatever transaction the caller holds", and `vdbeCommit` appearing in the profile proves the live
caller holds **none**: autocommit, one transaction and one WAL frame write per delta.

**Measure M1b in WAL frames written per minute and fsyncs, not in CPU share.** Those 628 samples are disk IO, so M1b
belongs under "minimize disk thrash"; inheriting M0's CPU framing would over-promise.

**The precedent to copy, so this is not novel work**: the network scan path already made this exact trade.
`network_scanner/DETAILS.md:108` describes a time-boxed transaction (`SCAN_COMMIT_INTERVAL`, 2 s) via `begin_scan_tx` /
`commit_scan_tx`, with `insert_entries_v2_batch` savepointing inside it. That answers the crash-contract question by
precedent and gives the design its shape: **a commit window bounded by time, not an unbounded batch.** Say why the live
path's interval differs from the scan path's.

**Tests**: a performance fix with no behavior change, so the honest instrument is a benchmark, not a unit assertion.
`benches/index_benchmarks.rs` already covers the enrichment and dir-stats hot paths; add or extend an insert-throughput
bench and record before and after. Existing writer correctness tests must stay green unchanged; if any of them change,
the fix is wrong. M1b, being correctness-sensitive, gets a real TDD cycle against crash semantics.

**Docs**: a one-line guardrail in `store/CLAUDE.md` (hot write paths use `prepare_cached`; `execute` with a literal
re-parses per call), plus a Decision/Why in the nearest `DETAILS.md` for M1b's batching semantics.

**Checks**: `pnpm check rust`, then `pnpm check`.

---

## M2: stop re-probing paths that are not cloud files (up to ~23% of busy CPU)

**Read M0's method caveat first**: these samples are `stat` time, so the share is an upper bound on CPU, not a
measurement. Re-measure before ordering this ahead of M3. The work is still right either way; only its rank is at stake.

### M2a: the TTL, which is the actual driver

Of the four sync-status threads' 745 running samples per thread, the `getResourceValue` XPC round trip is about **13**.
The probed paths are therefore overwhelmingly **not cloud files**: the resource-value read returns immediately with
nothing. The app is running a full NSURL construction plus resource-value read on plain local files, thousands of times
a minute, to keep learning "not a cloud file".

**Why it repeats forever**: `TTLS.stable = Duration::from_secs(60)` (`sync_status/mod.rs:64`). Every cached answer
expires after 60 seconds and gets re-probed, **including the negative one**. Meanwhile `sync_status/CLAUDE.md` says
invalidation is already explicit ("`listing::caching::notify_directory_changed` already calls `invalidate_dir`"), so the
60 s TTL is belt-and-braces on top of a working invalidation path.

A path that probed as not-a-cloud-file cannot become one without being moved, and a move invalidates. **Giving the
negative case a much longer TTL, or no expiry until invalidated, is a one-constant change that plausibly removes most of
the 43 sync-status batches per minute** an idle app currently runs (15,405 log lines in six hours from a `log::debug!`
that fires once per batch, `service.rs:172-177`). That is strictly better than halving a syscall on probes that should
not be happening at all.

Do this first. An earlier draft told the implementer to go looking for "a missing cache or a subscription"; the answer
is neither, it is a TTL.

### M2b: the doubled stat

`ubiquitous_bool` (`sync_status/probe.rs:56`) builds its URL with `NSURL::fileURLWithPath(&ns_path)`. The
single-argument form consults the file system to determine directory-ness, so Foundation calls `_NSFileExists` → `stat`
internally (418 samples). `sync_status_for` has already stat'd the path itself (327 samples) and holds the `metadata`
(`probe.rs:24`), so `metadata.is_dir()` is free. `fileURLWithPath:isDirectory:` takes the answer as a parameter and
skips the syscall.

This **removes the second of two stats**; the first supplies `st_flags` and `is_dir` and stays.

**Correctness constraint**: pass the value derived from that same `metadata` call, never a guess. A wrong `isDirectory:`
changes the URL's trailing slash and therefore its canonical form, which the FileProvider resource-value path does look
at.

**Verify before changing code**: confirm `fileURLWithPath:isDirectory:` actually removes the syscall on this macOS
version rather than deferring it to the first resource-value read, which would make M2b a no-op. Record with an evidence
anchor (`(verified on macOS 26.5.2, sample, 2026-08-03)`) per `AGENTS.md` § Docs.

**Tests**: test-first is awkward for a syscall count, so assert the observable contract (sync status still reported
correctly for iCloud, Dropbox, and plain-local paths, and a moved file still re-probes) and measure the reduction with
`sample` or `dtruss`, stating the number. M2a needs a real test that an invalidation still forces a re-probe, since the
whole change is trusting invalidation over expiry.

**Docs**: `sync_status/DETAILS.md`, a Decision/Why on both the negative-case TTL (with the invalidation argument) and
the `isDirectory:` form (with the evidence anchor).

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

`reconcile/DETAILS.md:484` already names this class for an Electron updater: _"its signal is REPETITION, and every path
is unique, so no anchor ever reaches a second strike."_ The settle delay answered it then only because those directories
vanished before settling. Cargo's persist, settle, and get walked.

### Inherited evidence: do not re-derive this

`later/indexing/sealed-subtrees-plan.md` targets this exact problem under this exact constraint, and carries two
completed spikes whose results this milestone must use:

- **Spike B (`docs/notes/churn-observability-spike.md`) already answers the depth question, negatively.** A climb rule
  based on churn share **over-climbs on real data**: it selects `~/Library/Containers` and `~/Library/Caches` rather
  than the actual culprit, because _"churn share alone cannot distinguish 'this parent is entirely churny' from 'this
  parent's churn is dominated by one child right now.'"_ Spike B also measured that `target/` becomes classifiable
  within **~31 s**.

  ⚠️ **Only half of Spike B's resolution is usable here, and the plan must not pretend otherwise.**
  `churn-observability-spike.md:252-253` recommends combining churn share with a **content ratio** (entries or bytes
  below the candidate versus below its parent) _"and the hard-stop list should include `~/Library/Containers` and
  `~/Library/Caches` as belt-and-braces."_ **That hard-stop list is a path-shaped denylist, which is exactly what David
  rejected.** So: adopt the content ratio, drop the hard-stop list, and own the cost. The spike's own evidence is that
  the ratio rule over-climbs to those two directories, and its authors thought it needed belt-and-braces to be safe.
  Without the list, over-climb risk is unmitigated and **needs its own answer in the design pass**. Do not read "already
  paid for" and reimplement the denylist.

- **Spike A** established "schedule on a cost budget, not a fixed clock".
- `watch/churn_monitor.rs` is the ancestor-rollup instrument built for this, gated behind `CMDR_CHURN_SPIKE`. Turning it
  on is cheaper than inventing a new measurement.

**State the relationship explicitly**: does this plan supersede sealing, complement it, or defer to it? A sealed subtree
and a budget-refused subtree are two mechanisms deciding the same thing about the same directory, and one has to be
authoritative.

### Decisions made here, not delegated

- **Budget: 3% of wall clock per volume for the drain.** Move it only with a measurement.
- **Fairness: the foreground pane's anchor and its ancestors are exempt from the budget**; everything else is FIFO
  within the remainder. The architectural cost is real: `cmdr-index` does not know which pane the user is looking at, so
  this needs a new `indexing/host/` seam (`indexing/CLAUDE.md`: "❌ Anything the app must answer arrives here"). That
  seam is part of this milestone, not a footnote. Without it, `pick_and_collapse_rescan` (`reconciler/rescan.rs:328`) is
  `min_by_key(depth)` over `HashSet` iteration order, so among 3,704 same-depth anchors the winner is effectively random
  and "eligible again later" does not mean "picked later".
- **Anchoring unit**: constrained by Spike B above. Do not adopt a bare fixed depth without engaging it.

### Two corrections to assumptions in the earlier draft

1. **`rescan_churn.rs` does NOT roll up an ancestor chain.** It is flat per-anchor with a 64-entry cap and
   cheapest-eviction (`MAX_TRACKED_ANCHORS`; the `64+ anchors` in the log line is that cap, not a count). The rollup is
   in `watch/churn_monitor.rs`.
2. **`cost_budget.rs:37` argues explicitly AGAINST charging cost up the whole ancestor chain**: _"per-directory
   fractions would be noise… the unit refused would become 'whichever depth tripped first', which is neither predictable
   nor explainable."_ Its answer is one accumulator at a fixed depth. Any design must argue past this.

**And a fixed depth of 5 collides with `cost_budget`'s own protected case.** `cost_budget.rs:359-369` has a test named
`a_subtree_with_a_low_slow_read_fraction_is_never_refused_however_large_it_grows`, whose stated purpose is that _"a rule
that refuses it stops refreshing the folder David works in all day"_. `ANCHOR_DEPTH = 5` anchors at
`~/projects-git/vdavid/cmdr`, so a cost-in-window budget at that depth refuses precisely the subtree that test protects.
Different metric (a fraction of slow reads versus a total cost), so it is not a contradiction, but the design pass owes
one sentence on why the same anchor is safe to refuse on one axis and not the other.

**Engage the prior deferral.** `docs/notes/idle-cpu-indexing-streamlining-2026-07.md` § "L2 , targeted subtree walk:
DEFERRED (measured, not worth it)" is a measured verdict on adjacent work. Say in a line why L2's deferral does not
apply here.

### Constraints the design must honor

- **Composes as a further eligibility gate**, like settle and window: whichever says "not yet" wins, and every gate is
  an absolute deadline that passes on its own.
- **The pure, clock-injected discipline holds.** No filesystem, clock, or logging calls inside the engine.
- **A volume-wide budget on an EXTERNAL volume is a correctness regression.** `rescan_route.rs:58-70` is explicit: the
  per-navigation verifier is root-scoped and "bails inert on a mount-rooted volume", which is why external volumes get a
  45 s interval where the boot disk gets 24 h, because _"a 24-hour blind window there would be a pure correctness
  regression on the one volume kind with zero verifier cover."_ A duty-cycle budget is that blind window renamed. Scope
  it to the boot disk, or give external volumes a much looser budget.
- **A volume-wide gate makes the hourglass flicker volume-wide.** Under a global budget, eligibility flips for every
  queued anchor at once, and `reconcile_with_eligibility` (`rescan_hold.rs:96-117`) re-derives every queued anchor's
  hold on the ~1 s sweep. A held root drags its ancestor chain to `/`. So `/` and `~` would blink "size updating" at the
  duty-cycle period, a regression in the exact property `rescan_hold.rs` exists to protect. A per-subtree shape only
  flickers the offending chain.
- **`record_held_back` must be fed by the new gate.** Its one call site (`rescan.rs:155`) is gated on
  `!throttle.is_eligible(...)`. A governor living outside `is_eligible` will not increment it, and the churn line's
  `held_back` field goes to zero during heavy churn, which `rescan_churn.rs:77-78` designates as _the_ regression
  signal.
- **`gc` measures each record against its OWN window.** New state needs the same discipline and the same bounding.
- **A fourth shape to evaluate before adopting a budget.** High anchor cardinality is the same signal as
  `MustScanSubDirs` on `/`: the OS saying it can no longer track this incrementally. `rescan_route.rs` already answers
  that by routing to the visible scanner with a persisted once-a-day window and a green badge, reasoning that the anchor
  path "carries no diagnostic information" and the signal means _"this index is now SUSPECT."_ Applying that disposition
  to a cardinality storm touches the hourglass invariant **not at all** (routed anchors leave `pending_rescans`
  entirely, so they hold nothing by construction), reuses `SweepRecord` and the existing user-facing story, is
  path-shape-blind, and carries the external-volume distinction for free. Its cost is that a full sweep is expensive
  (`rescan_route.rs:46-48` measured 1,309 s), trading many small walks for one big one. Argue it before choosing.

### Correctness and transparency when the governor refuses

Principle 4 is protect the user's data, and `docs/design-principles.md` calls for radical transparency. A refused
subtree means the index is knowingly behind: folder sizes go stale and search misses recent files. Decide, in
`reconcile/DETAILS.md`:

- **Does refusal touch `recursive_size_complete` or `min_subtree_epoch`?** Almost certainly it must not: the
  `absorbing_min_epoch` trap zeroes every ancestor up to `/`. Confirm a refused anchor stays queued and never stamps
  `listed_epoch`, per `cost_budget.rs:45-47`. Say so, so nobody rediscovers it the hard way.
- **Is staleness user-visible, or silent?** "Silent, and here is why that is acceptable" is a fine answer. No answer is
  not. Compare `sealed-subtrees-plan.md`, which faces the same debt directly: _"We knowingly decline to enumerate and
  still claim exact… radical transparency says own that debt."_
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

**Checks**: `pnpm check rust`, `pnpm check`, `pnpm check --include-slow`.

---

## M4: gate the media tick before it walks, without breaking scoped GC

`run_live_tick_blocking` (`media_index/scheduler/live.rs:124`) walks at `:138` and computes its coverage gates at
`:151-152`, so thousands of ineligible dirs cost a `resolve_path` plus a `list_children_on` each before being rejected.

**The data-safety invariant this milestone lives or dies on.** Filtering only the walk input is a data-loss bug. The
same `touched_dirs` set is used three times:

- `:138`, the walk
- `:197`, `GcScope::TouchedDirs(touched_dirs)`, which deletes every stored row whose parent dir is in the set and absent
  from the walk
- `:224`, `patch_touched_dirs(...)`, which patches coverage counts from the same pairing

Filter the walk but not the GC scope and you delete every media row (OCR text, Vision tags, CLIP embeddings) in every
dir the filter removed. That is the trap `media_index/scheduler/CLAUDE.md` warns about, reached from the other side, and
it contradicts `media_index/CLAUDE.md`'s "Uncovered rows STAY: narrowing a setting deletes nothing."

**So: compute one filtered set once and thread it to all three.** That is the milestone's named invariant and its first
test.

The gate is implementable: `local_should_enrich` (`scheduler/lifecycle.rs:44-54`) is
`config.covers(volume_id, path) || scores.contains_key(parent_dir(path))`, and `covers` is a prefix test
(`media_index/network/config.rs:48-51`), so a dir-level pre-filter is sound.

**Measure the gate before reordering around it.** `:151-152` already calls `gate::importance_threshold()` plus
`folder_scores()` unconditionally every tick, which is an `ImportanceIndex::open` plus an `above_threshold` that
materializes a map over 90,308 folders and 161,094 weights (`idle-memory-profile-2026-07-28.md`), **every 60 s
forever**. That may cost more than the walk being moved behind it. Step one is measuring it, not reordering.

**Test idiom correction**: `CountingOpener` (`sqlite_util/tests.rs:91-108`) counts connection **opens**, not statement
executions, and `walk_image_entries_in_dirs` takes a `&Connection`, so there is no seam there. Test the filtered set as
a pure function, and assert on the returned `images` plus the GC scope.

**Tests**: test-first on the invariant (a filtered-out dir loses no rows), then on the pure filter, then that an
eligible dir still enriches, so the gate cannot be trivially "correct" by gating everything out.

**Already fixed while speccing this** (`scheduler/mod.rs:380-387`): `folder_scores`'s docstring claimed the `None` case
_"tells the local pass to fall back to 'enrich all'"_. `local_should_enrich` (`lifecycle.rs:50-51`) does the opposite,
`None` means override-only, and `scheduler/CLAUDE.md` forbids enrich-all emphatically ("❌ Never fall back to
enrich-all: … an enrich-all pass over-indexes the volume permanently"). The docstring described the exact behavior the
module bans. Corrected in place; no code change.

**Docs**: `media_index/scheduler/DETAILS.md`, Decision/Why on gate-before-walk and the one-set invariant.

**Checks**: `pnpm check rust`.

---

## M5: make the SQLite page-cache bound real, and find the actual 643 MB

**The earlier draft's central claim was wrong.** It treated the whole ~795 MB system C heap as SQLite page cache. With
`SQLITE_ENABLE_MEMORY_MANAGEMENT` defined, `pcache1.separateCache = 0`, so `nInitPage = 0` and `pcache1InitBulk` returns
immediately: **there is no bulk allocation**, and every overflow page is an individual `sqlite3Malloc(szPage + hdr)` of
about 4.1 KB. macOS routes that to the _small_ zone (the large threshold is 127 KB). **Page-cache overflow can therefore
only appear in `MALLOC_SMALL` (152 MB), never in `MALLOC_LARGE` (643 MB)**, and the plan's own evidence, regions of 9 MB
and 2.25 MB, confirms those are something else.

The prior note corroborates: `idle-memory-profile-2026-07-28.md:15-16` recorded
`MALLOC_LARGE 730 MB / MALLOC_SMALL 405 MB` and asserted "for us, ~all SQLite". After the slab shipped, `MALLOC_SMALL`
fell 405 → 152 (−62%) while `MALLOC_LARGE` moved 730 → 643 (−12%). The slab did exactly what it should to page cache and
barely touched `MALLOC_LARGE`. **That note's "~all SQLite" line is the error this plan inherited, and it must be
corrected there too.**

- **M5a: identify the 643 MB.** Now the primary unknown, and the largest unattributed block after the 947 MB Rust heap.
  It is not page cache. Do not proceed to a fix before naming it.
- **M5b: bound page-cache overflow, ceiling ~152 MB.** Evaluate in this order:
  1. **Cut the multiplier, not just the product.** The docstring's promise ("total page memory is THIS number no matter
     how many connections exist") holds only if `Σ nMax ≤ slab slots`; today that is 132 × 8 MiB against 64 MiB. Cutting
     `READ_PAGE_CACHE_KIB`, or cutting the connection count (60 connections to one DB is itself the anomaly, driven by
     `THREAD_CONN_SLOTS = 3` times tokio's blocking pool), is config-only, adds no FFI, adds no thrash risk, and makes
     the existing docstring true instead of bolting a second mechanism onto a false one. `idle-memory-profile` already
     flagged this as unresolved: the count _"tracks tokio's blocking-thread pool, not anything semantic"_. Under
     "elegance above all", bounding the multiplier is the fix and a second ceiling is the hack.
  2. **`sqlite3_soft_heap_limit64` as a backstop**, if still wanted after (1). Two things the earlier draft asserted
     past: the limit is **advisory** (`sqlite3.c:7697`: _"it will exceed the limit rather than generate an
     `SQLITE_NOMEM` error… advisory only"_), so a test asserting the bound holds is flaky-green rather than green; and
     under one unified `PGroup`, limit pressure runs the global LRU under `pcache1.mutex`, which `pcache1AllocPage` must
     drop and retake around `pcache1Alloc` (`sqlite3.c:58095-58108`). With 71 threads in SQLite, steady limit pressure
     means constant global eviction under a contended mutex, and it can evict a hot volume's working set to serve a cold
     one. Weigh that.
- **Fix the false docstring** at `sqlite_util.rs:26` and `:39` whichever fix lands, and say what the slab bounds versus
  what anything else bounds.

**Citation correction**: the workspace resolves `libsqlite3-sys` **0.38.1** (`Cargo.lock:5369`, via `rusqlite 0.40`),
not 0.37.0. The `-DSQLITE_ENABLE_MEMORY_MANAGEMENT` flag is at `0.38.1/build.rs:135`, so the premise survives.

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

**Name the tradeoff.** `docs/tooling/logging.md:42` makes the file sink unconditionally DEBUG _on purpose_, so
error-report bundles carry full context. **Every TRACE demotion trades field diagnosability for volume.** The reconcile
bullet mitigates that by folding a count into the summary; any demotion without such a mitigation silently removes those
lines from every crash bundle. Say which ones are which.

**Not mechanical**: the churn-line rework has to update exact-string assertions at `rescan_churn.rs:342-346`,
`:366-376`, `:489-492`, and `:506-511`, plus `docs/tooling/logging.md:196-209`.

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
  moved at least 1 MB every ~3 s. Decay would essentially never engage there, and would engage on the NAS, which
  produces almost none of the lines.
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

**Careful**: the low-disk-space hysteresis detector and the `volume-space-changed` stream behind the live toast both
ride this loop. Backing off must not make the toast's numbers stale while it is on screen. That is user-visible, so it
needs a test and David reviews it.

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

- **M1 and M2 together should remove a large share of busy CPU.** M0's headline is "roughly half", but M2's portion is
  `stat` time and therefore an upper bound on CPU (§ M0). Set the real target from the re-measurement, and verify by
  re-running the per-thread attribution to show that `index-writer` and `cmdr-sync-status` shares dropped. M1b's win is
  measured separately, in WAL frames and fsyncs, not CPU.
- Idle CPU under a stated percent of one core on a quiet machine, and under a stated percent under the harness.
- Footprint under a stated ceiling after eight hours, with M5's predicted contribution named separately.
- Log lines per hour, stated before and after.
- `pnpm check --include-slow` green.
- Colocated `CLAUDE.md` and `DETAILS.md` updated per `AGENTS.md` § Docs, with Decision/Why where a design choice was
  made, including the correction to `idle-memory-profile-2026-07-28.md`.

## Sequencing

**MT goes first.** It runs every 60 seconds forever on an idle app, it is a regression of a fix believed shipped, and it
is the only candidate whose cost is visible in the log rather than inferred from a sample. Everything else is sized
against a machine that has stopped doing it.

**Step zero alongside it: re-measure.** Run `ps -M` plus a longer, three-bucket `sample` (§ M0). The search-arena
residency question is one grep and belongs here too.

M1 is next regardless of what the measurement says, because its defect is confirmed by reading the code rather than by
sampling. What the re-measurement decides is whether M2 outranks M3.

1. **M1** (writer statement cache **plus the cache-capacity guard**, which is the part that makes it work), then **M1b**
   (time-boxed delta transaction, copying `network_scanner`'s `SCAN_COMMIT_INTERVAL` shape) separately.
2. **M2a** (negative-case TTL) before **M2b** (the doubled stat). M2a is the larger and simpler win, and it shrinks the
   population M2b operates on.
3. **Re-run M0's attribution.** Everything below is sized against numbers M1 and M2 will have changed.
4. **M5a** (identify the 643 MB) early if it needs hours of collection under load; it gates M5b.
5. **M3** (arrival-rate governor): the design pass, then the integration red.
6. **M4** (media tick), measuring `folder_scores` first.
7. **M6** (log volume), after M3 so the remaining shape is clear.
8. **M7** (space poller), only if its gate opens.

**Safe to parallelize**: M1, M2, and M5a touch disjoint trees (`indexing/store` plus `indexing/writer`, app-side
`sync_status`, `cmdr-fs` plus a dev IPC surface). M3 and M4 share no files either, but M3 changes M4's input volume, so
running them together muddies the _measurement_ rather than risking a conflict. That is a weaker reason than a merge
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
