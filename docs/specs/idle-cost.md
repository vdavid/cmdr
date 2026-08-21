# Cmdr should cost almost nothing while you're not using it

**Problem**: a idle prod build burned 110 minutes of CPU over 9.1 hours (about 20% of a core, sustained) at a 1.78 GB
footprint, writing 141,072 log lines in six hours. A file manager sitting in the background is competing with the work
the user actually opened their laptop for, which is principle 5 ("respect the user's resources") failing in the most
visible way there is: a fan.

**Size**: three to four days of build work, plus one design decision that gates the largest item.

**Read first**: `docs/notes/idle-cpu-attribution-2026-08-03.md`. Four successive hypotheses about where the CPU goes
were refuted by measurement, and each refutation is recorded there. ❗ Do not re-derive a number from a stack profile
here without reading it: the method's own bias (blocking frames count scheduler waits, so `stat` and `pwrite` score as
busy CPU) produced three of the four wrong answers.

Already shipped and deliberately not repeated here: the rescore treadmill fix (origin bound plus demotion, `0271855aa`),
the signals-blob write skip (`234bd2aec`), writer statement caching with its capacity guard (`e63e06b37`), implicit
write batching (`3313aabfd`), and most of the log-volume reduction.

## The work

Ordered so the cheap, unblocked items come first.

1. **Stop re-probing paths that are not cloud files.** Half a day. An idle app runs about 43 sync-status batches a
   minute learning "still not a cloud file", because `TTLS.stable` (`sync_status/mod.rs:66`) expires the NEGATIVE answer
   after 60 seconds while `notify_directory_changed` already invalidates explicitly. Give `Unknown` a much longer TTL,
   or none until invalidated, plus a test that invalidation still forces a re-probe: the whole change is trusting
   invalidation over expiry, so that test is the change. Then drop the second of two `stat`s by passing
   `metadata.is_dir()` into `fileURLWithPath:isDirectory:` (`sync_status/probe.rs:56`). ⚠️ Verify on the current macOS
   that this removes the syscall rather than deferring it to the first resource-value read, and pass the value from that
   same `metadata` call, never a guess: a wrong `isDirectory:` changes the URL's canonical form, which the FileProvider
   path reads. ⚠️ Sized honestly, this is an IO-and-provider-load win, not a CPU one. Its CPU case was refuted. It also
   closes the last remaining line of the log-volume work for free, because that line fires once per batch.

2. **Gate the media tick before it walks.** Half a day. `run_live_tick_blocking` walks `touched_dirs` and computes its
   coverage gates afterwards, so thousands of ineligible directories each cost a `resolve_path` plus a
   `list_children_on` before being rejected. Compute one filtered set and thread it to all three consumers: the walk,
   `GcScope::TouchedDirs`, and `patch_touched_dirs`. ❌ **This is a data-loss trap, not a perf tweak.** Filter the walk
   but not the GC scope and you delete every OCR text, Vision tag, and CLIP embedding in every directory the filter
   removed, contradicting `media_index/CLAUDE.md`'s "uncovered rows STAY". Step one is measuring
   `gate::importance_threshold()` and `folder_scores()`, which already run unconditionally every 60 seconds over 90,308
   folders and may cost more than the walk being moved behind them.

3. **Name the 643 MB.** A day, mostly collection under load. The largest unattributed block after the 947 MB Rust heap,
   and it is **not** SQLite page cache: with `SQLITE_ENABLE_MEMORY_MANAGEMENT` defined, `pcache1.separateCache = 0`, so
   every overflow page is an individual ~4.1 KB allocation, below macOS's 127 KB large-zone threshold, and can therefore
   only appear in `MALLOC_SMALL`. The corrected attribution is in `docs/notes/idle-memory-profile-2026-07-28.md`. ❌
   Don't proceed to a fix before naming it. Needs a dev diagnostic surface, so it is an app-side IPC command plus
   `bindings.ts`, not a `cmdr-fs`-only change.

4. **Make the page-cache bound real.** A few hours, gated on item three. The docstring is now honest
   (`cmdr-fs/src/sqlite_util.rs`: an upper bound per connection, not a reservation), but `Σ nMax` is still 132
   connections × 8 MiB against a 64 MiB slab. Cut the multiplier: either `READ_PAGE_CACHE_KIB`, or the connection count,
   which tracks tokio's blocking-thread pool and nothing semantic. ❌ Don't reach for `sqlite3_soft_heap_limit64`. It is
   advisory, so a test asserting the bound is flaky-green, and under one unified `PGroup` steady limit pressure means
   global LRU eviction under a contended mutex with 71 threads in SQLite, which can evict a hot volume's working set to
   serve a cold one.

5. **Bound the reconcile drain's arrival rate.** Several days, and half of it is design. **Blocked: see below.** 3,704
   distinct rescan anchors in eight hours, 93% of them under `.claude/worktrees/*/target`, and nothing rate-limits
   arrivals. The per-anchor throttle contributes nothing, because cargo's anchors are one-shot and `is_eligible` returns
   `true` unconditionally on the leading edge. This is unbounded by construction and scales with the user's workload
   rather than with anything Cmdr controls.

## The decision that gates item five

Four mutually exclusive shapes, and three of them collide with something already shipped or with David's standing rule
that there are no denylists and no path-shaped exclusions:

- **(a) A volume-wide duty-cycle budget** (about 3% of wall clock). Makes the hourglass flicker volume-wide, because
  `rescan_hold.rs` re-derives every queued anchor's hold on the roughly one-second sweep and a held root drags its chain
  to `/`. Also a blind window on external volumes, the one kind with no verifier cover.
- **(b) A per-subtree budget at a fixed depth.** `cost_budget.rs:37` argues explicitly against charging up an ancestor
  chain, and `ANCHOR_DEPTH = 5` anchors at `~/projects-git/vdavid/cmdr`, which is exactly the subtree that
  `a_subtree_with_a_low_slow_read_fraction_is_never_refused_however_large_it_grows` exists to protect.
- **(c) Spike B's churn-share plus content-ratio climb.** The spike's own authors wanted a `~/Library/Containers` and
  `~/Library/Caches` hard-stop list to make it safe, which is the denylist that was rejected, so the over-climb risk is
  unmitigated and needs its own answer.
- **(d) Treat high anchor cardinality the way `rescan_route.rs` already treats `MustScanSubDirs` on `/`**: route to the
  visible scanner with a once-a-day window and a green badge.

**Recommendation: (d).** It reuses a shipped mechanism and a shipped user-facing story, and it is the only one of the
four that fights neither `rescan_hold.rs` nor `cost_budget.rs`.

Two smaller calls ride along: whether a budget-refused subtree's staleness is visible to the user or silent, and how
this relates to `later/indexing/sealed-subtrees-plan.md`, since a sealed subtree and a budget-refused subtree decide the
same thing about the same directory and one of them has to be authoritative.

## Sequencing

Items one through four are independent. Item five changes item two's input volume, so run them in either order but ❌
don't measure them together.
