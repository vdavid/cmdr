# Where Cmdr's idle CPU goes, and the four answers that were wrong

**What this settles:** how a 110-minutes-of-CPU-over-9.1-hours idle problem was attributed, mis-attributed four times,
and what finally held. Kept because every wrong answer here was reached by a reasonable method, three of them share one
root cause, and that same method is what anybody re-opening this question will pick up first.

**The problem, prod v0.37.0 on David's machine, 2026-08-03**: 110 minutes of CPU over 9.1 hours (~20% of one core,
sustained), 1.78 GB physical footprint (2.8 GB peak), 141,072 log lines / 28 MB in six hours. The app was idle
throughout. ⚠️ The CPU baseline covers 9.1 hours, the log count 6 hours, and the churn count 8 hours, so ❌ never divide
one by another. The machine also ran six Cmdr worktrees with active cargo builds: a heavy case rather than an
unrepresentative one, but anything measured on it wants a quiet-machine sanity check as well.

## The method, and the bias built into it

`sample <pid> 20` on the live process, then per-thread attribution counting samples whose leaf frame is NOT a blocking
syscall (`psynch_cvwait`, `kevent`, `mach_msg2_trap`, `semaphore_timedwait`, and friends). 7,330 running-samples across
71 threads.

⚠️ **That blocking-frame list is a list of SCHEDULER waits. It does not include `stat`, `pread`, `pwrite`, `open`, or
`read`.** So a sample parked in a file-IO syscall scores as busy CPU, and the bias is not uniform: it lands hardest on
exactly the threads whose stacks can't tell the two apart. `reanchor-cost-spike.md` had already recorded this error
class for this repo ("the cost is IO wait, not syscalls; CPU time is 16-23% of wall on the big directories"), and it was
applied to the reconcile drain and to nothing else.

⚠️ **And 20 seconds cannot attribute 9.1 hours.** It can't tell "45% sustained" from "one burst that happened to be
running", which is why the media live tick read 586/3,425 samples in one window and zero in another.

**The correction that works**: a 180 s sample classifying leaves into THREE buckets — userspace, file-IO syscall, and
parked — reporting the first two separately, so the gap between them is the size of the error. `ps -M <pid>` is the
cheaper companion: per-thread cumulative CPU since launch, integrating every burst instead of sampling one (it reports
no thread names, so take it alongside a `sample`).

## The four wrong answers

**1. "The reconcile drain is the one that moves the CPU number."** Inferred from LOG VOLUME (32,479 `rescan` lines,
19,705 `reconciler`) rather than from CPU. Refuted by the 20 s attribution, where the drain does not appear at all. Real
but secondary: ~466 s of reported walking over eight hours, largely IO wait, at the 16-23% CPU `reanchor-cost-spike.md`
measured for this class of walk.

**2. "`index-writer` is 45% of busy CPU and `cmdr-sync-status` 41.7%."** The 20 s window's headline: 87% of busy CPU in
two threads nobody was looking at. Refuted by the 180 s window on the same idle process, where `index-writer` is **not
in the top 12 at all** and the `sqlite3RunParser` prepare path that dominated the first window is absent. Two windows on
one idle process disagreeing about which thread dominates is itself the finding: **the workload is bursty at a period
longer than either window**, so ❌ don't order work off a single `sample`.

**3. "The sync-status probe is ~23% of CPU."** Refuted by the three-bucket sample: `cmdr-sync-status` x4 is 3.4% of busy
but **0.2% of userspace CPU**, with 1,964 of 2,037 samples per thread inside the `stat` itself. It is syscall and
provider-latency time. The 43 sync-status batches a minute an idle app runs are still real waste (syscalls, IO, and load
on `fileproviderd`), so that work is defensible on those grounds; ❌ it never had a claim on CPU.

**4. "The search arena is never dropped, and is the memory culprit."** ~600 MB of the 947 MB Rust heap rested on it. It
was a **bad grep**: the search for "Search index dropped/unloaded/released/evicted" missed the message the code actually
emits, "Search indices dropped (all volumes)". The arena loaded at 16:15 and was gone by 16:53. ⚠️ The cheapest lead in
the whole effort, and it cost a day of hypothesis because nobody checked the string against the source.

## What survived, and what moved the number

- **The writer's hot INSERT re-parsed per row** (`conn.execute` with a literal, in a file using `prepare_cached` 21
  other times). Confirmed by READING THE CODE, which is why it was worth fixing whatever the sampling said: its share
  was unknown, its wrongness was not. ⚠️ Shipping it needed the statement-cache CAPACITY raised too — rusqlite's default
  cache holds 16 entries against the writer connection's 31 distinct `prepare_cached` sites, so the LRU would have
  evicted and silently re-prepared, looking fixed in a microbenchmark and doing nothing in production
  (`crates/cmdr-fs/src/sqlite_util.rs`, `WRITE_STATEMENT_CACHE_CAPACITY`).
- **The live write path committed per message.** In autocommit every row pays its own COMMIT plus a WAL frame write.
  `crates/cmdr-index/src/indexing/writer/DETAILS.md` § "Implicit write batching" holds the design and the numbers. ⚠️
  Quote that win as a RATIO, ❌ never as microseconds: the probes behind it are DEBUG builds, so the absolute figures
  mean nothing off that machine while the ratio survives.
- **The importance rescore treadmill**, the biggest single cost, which is its own note:
  `importance-treadmill-2026-08-04.md`. Its trigger is the one line worth carrying anywhere this comes up again:
  `origin_dir` is the PARENT of the changed file, so any write directly in `~` makes `$HOME` an origin, and `$HOME`
  covers 83% of the volume's directories.

## Still open, and nothing here explains them

- **643 MB `MALLOC_LARGE`**, in regions of 9 MB and 2.25 MB. ⚠️ It is **not** SQLite page cache: with
  `SQLITE_ENABLE_MEMORY_MANAGEMENT` defined `pcache1.separateCache = 0`, so there is no bulk allocation and every
  overflow page is an individual ~4.1 KB `sqlite3Malloc`, which macOS routes to the SMALL zone (the large threshold is
  127 KB). Page-cache overflow can therefore only ever appear in `MALLOC_SMALL`. The corroborating before-and-after: the
  shared page-cache slab moved `MALLOC_SMALL` 405 -> 152 MB (-62%) while `MALLOC_LARGE` moved 730 -> 643 MB (-12%).
- **947 MB Rust heap** (mimalloc, reported as `IOAccelerator`), plus 725 MB reclaimable.
  `docs/tooling/memory-debugging.md` notes that a collapsing balloon is usually mimalloc decommitting rather than an
  allocation leak, so part of this may be a purge-tuning question and not a culprit at all. Prior art:
  `memory-runaway-rust-heap-2026-07-25.md`.
- **132 open SQLite connections** across 71 threads (60 x `index-root.db` alone). The count tracks tokio's blocking-pool
  size, ❌ nothing semantic. `idle-memory-profile-2026-07-28.md` § "Cause 1" flagged it and it is still unresolved.

## The rules this leaves behind

- ❌ **Never rank work off one `sample` window.** Two windows on one idle process disagreed about the top thread. Take
  `ps -M` or repeated samples across hours before an ordering decision, and say which of the two you have.
- ❌ **Never report a share as CPU when the leaf is a syscall.** Split userspace from file-IO explicitly, report both,
  and treat the syscall half as an upper bound on CPU rather than a measurement of it.
- ❌ **Never infer CPU from log volume.** It named the wrong milestone first, and cost the effort its ordering.
- **A defect confirmed by reading the code outranks a percentage.** Two of the three fixes that shipped were found that
  way, and the percentages pointing at them were wrong in both directions.
- **Grep for the message the source emits**, not the one you would have written.
