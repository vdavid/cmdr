# Local guarded walker details

The hang-tolerant engine behind every local walk, and the macOS bulk reader it enumerates with. `mod.rs` holds the
caller-facing types and readers, `engine.rs` the walk driver and its scheduling machinery, `bulk_read.rs` the macOS
`getattrlistbulk` reader. Read this before any non-trivial work here: editing, planning, reorganizing, or advising. The
scan driver that calls it, and the exclusion policy it applies, are `../CLAUDE.md` and `../DETAILS.md`.

## The engine

The LOCAL scan (both the fresh `scan_volume`/`scan_subtree` and the serial reconcile rescan in `../../reconcile/`) must
survive a hung `readdir`: a disconnected macOS File Provider mount (Dropbox / Google Drive / MacDroid under
`~/Library/CloudStorage`, iCloud under `~/Library/Mobile Documents`) blocks a `readdir` indefinitely when the provider
is offline, which froze the whole scan.

- **The pool.** A persistent pool of 8 MB-stack worker threads (dedicated OS threads, never rayon — File Provider reads
  descend deep XPC override chains that overflow rayon's 2 MB stack) pull directory-read tasks from a shared queue and
  call `readdir` directly. A blocking `readdir` can't time itself out, so a **watchdog** thread caps each read from
  outside: every in-flight read carries an `Arc<AtomicU8>` state (`READING → COMPLETED`, won by the worker; or
  `READING → ABANDONED`, won by the watchdog), and whoever wins the compare-and-swap owns the outcome exactly once. A
  read the watchdog condemns is **abandoned**: reported as a read error (subtree pruned, dir left unmarked), a
  replacement worker is spawned to restore pool capacity, and the stuck worker is left parked in the syscall (it exits
  on its own when the File Provider layer finally errors). Only genuinely-hung _frontier_ dirs reach this, each pruning
  its subtree, so the parked-worker cost is bounded and self-clearing. Workers are NOT joined (an abandoned one would
  block forever); the walk returns when the outstanding-task count hits zero. The reader is an injected `ReadDirFn`
  (production `bulk_read_dir` on macOS, `std_read_dir` elsewhere, tests a mock that blocks or trickles), so hang /
  big-but-healthy / honest-skip / parallel-correctness are unit-tested with no real hung mount.
- **Per-subtree give-up budget.** The per-dir watchdog abandons ONE hung dir at a time, so a dead mount that fails on
  every read (a disconnected File Provider returning `ETIMEDOUT`/`os error 60` per descendant, e.g. a MacDroid phone's
  `/proc/*/task/*/fd`) still cost one abandon PER DESCENDANT — hundreds/thousands of probes and a log flood. The give-up
  budget bounds that structurally: every read carries a `SubtreeBudget` (`engine.rs`) shared by the children of ONE
  successfully-listed directory. Each failed read (timeout OR IO error) increments it; any successful sibling read
  resets it; when it reaches `WalkConfig::give_up_after` (`DEFAULT_GIVE_UP_AFTER = 32`, mirroring the network scanner's
  `CONSECUTIVE_FAILURE_ABORT`) the budget is **given up** (sticky) — the trip is logged ONCE (subtree path + count), and
  every still-queued sibling sharing that budget is pruned unread by a pre-read check in `run_worker` (no probe, no
  per-dir log). A successfully-listed dir mints a FRESH budget for its own children, so the bound is ~N probes per level
  of a dead subtree instead of N-per-descendant. It's **throttle, not exclude**: purely structural, so a healthy
  provider (reads succeed → counter resets) is fully indexed, and only a genuinely-dead subtree is abandoned — no
  path/CloudStorage denylist. Under concurrency "consecutive" is loose (up to `num_threads` reads can be in flight
  against one budget), the same caveat the network scanner notes. **Honest-stale, never false-complete:** a pruned dir
  is never marked listed (never added to `listed_ids`), so it stays `listed_epoch = 0` (unknown size) — its `EntryRow`
  still exists (its parent listed it), but its subtree is left unknown, not zeroed and not `scan_completed_at`-marked.
  Honest-stale is NOT silent, though: the prune calls `DirVisitor::visit_pruned`, which is the only mention a pruned
  directory ever gets anywhere. Without it a visitor recording ground nothing can read would miss exactly the pruned
  majority the budget exists to avoid probing, and every one of them would sit in the coverage frontier forever. ❌
  Don't log in that hook — killing the per-descendant log flood is what the budget is for.
  `WalkStats.subtrees_abandoned` counts the trips; `run_scan` logs a one-line scan-wide summary. This MIRRORS (not
  shares) the network scanner's counter: that one is a single global `usize` over a serial BFS that aborts the WHOLE
  walk; this is a per-subtree parallel `Arc<Atomic>` tree that prunes one subtree — different granularity and
  control-flow, and the shared logic is a trivial threshold compare, so a helper would be an inelegant abstraction.
  Test: `walker/tests.rs::gives_up_on_a_dead_subtree_and_keeps_walking_a_healthy_sibling` (synthetic dead subtree, no
  real mount).
- **Parent attribution needs no path→id map.** Each read task carries the directory's own id; the `InsertVisitor`
  attributes children to their parent via that carried id (`dir.id`) and allocates fresh child ids from the shared
  `Arc<AtomicI64>` counter — so the whole-volume `HashMap<PathBuf,i64>` the old fresh scanner kept is gone for the local
  path. (It survives only in the network scanner's `ScanContext`, whose serial BFS still resolves parents by path.)
  `std_read_dir` classifies each child from the dirent (`d_type`, no extra syscall on APFS); the visitor does its own
  per-child `symlink_metadata` for sizes/mtime.

## The walker's progress timeout

**Elapsed time cannot tell a BIG directory from a BROKEN one, so the walker doesn't measure it.** Every read publishes
what it has delivered through a `ReadProgress` handle (`mod.rs`), and the watchdog judges that (`Engine::verdict`,
`engine.rs`). Two rules, either of which abandons the read:

- **Stalled** — nothing delivered for `WalkConfig::stall_timeout` (production `LOCAL_LIST_TIMEOUT`, 15 s). This is the
  hung-mount rule, and it applies at any point in a read: a mount that drops after delivering a million entries is
  abandoned as promptly as one that never answers.
- **Over allowance** — total time past `stall_timeout` plus `WalkConfig::per_entry_allowance`
  (`DEFAULT_PER_ENTRY_ALLOWANCE`, 1 ms) per entry delivered. The floor under the stall rule: without it a read trickling
  one entry every 14 s would never stall and never finish. It's ~500× the measured `getattrlistbulk` per-entry cost and
  10× the reconcile cost budget's per-entry threshold for calling a read _pathological_, so a healthy read clears it by
  orders of magnitude.

**Why it changed.** A total-duration cap of 15 s made the 2026-07-21 fresh scan report "complete" with 6,001,637
entries; the reconcile that followed added **661,411 rows** it had silently dropped. All five abandoned directories were
flat and merely large (200,000 / 179,523 / 102,929 / 100,000 / 74,024 entries), and the serial reconcile read every one
of them in 10.8 s or less. They only exceeded 15 s in the parallel scan, which runs one read per core, so the constant
was being asked a question its own doc comment never claimed to answer ("an online cloud dir lists in well under a
second"). Measurements: `docs/notes/indexing-benchmarks-2026-07-21.md`. Same class of mistake, same week, as the
reconcile cost budget's cumulative-time metric (see `../../reconcile/DETAILS.md`), and the same fix shape: score the
work done, not the clock.

**A reader that can't report progress is still bounded.** With `entries` stuck at 0, both rules collapse to the plain
total-duration cap the walker always had — which is the honest verdict, since a read we can't observe is
indistinguishable from one that has produced nothing. That covers the serial reconcile's `GuardedReader` (it awaits a
whole `Vec` on a helper thread and reuses `LOCAL_LIST_TIMEOUT` as a total cap) and any future reader added without
progress plumbing. Both production readers do publish: `bulk_read_dir` per `getattrlistbulk` batch, `std_read_dir` per
entry.

**What did NOT change.** The abandon/replace protocol, the subtree give-up budget's accounting (a timeout is still one
`record_failure` against the subtree budget, an IO error still resets on a successful sibling), and the honest-stale
contract (an abandoned dir is never marked listed, so it stays `listed_epoch = 0`). Fewer false timeouts simply means
`DEFAULT_GIVE_UP_AFTER` trips less often on healthy volumes.

Tests (`tests.rs`, all millisecond-scale with a mock reader): `a_read_that_keeps_delivering_is_never_abandoned`,
`a_read_that_stops_delivering_is_abandoned_promptly`, `a_reader_that_cannot_report_progress_is_still_bounded`,
`a_trickling_read_is_abandoned_by_the_per_entry_allowance`.
