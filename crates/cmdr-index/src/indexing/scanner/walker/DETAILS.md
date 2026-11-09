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

- **The workers.** Each walk runs `num_threads` workers (8 MB-stack OS threads, never rayon — File Provider reads
  descend deep XPC override chains that overflow rayon's 2 MB stack) that pull directory-read tasks from the walk's own
  queue and call `readdir` directly. A blocking `readdir` can't time itself out, so a **watchdog** caps each read from
  outside: every in-flight read carries an `Arc<AtomicU8>` state (`READING → COMPLETED`, won by the worker; or
  `READING → ABANDONED`, won by the watchdog), and whoever wins the compare-and-swap owns the outcome exactly once. A
  read the watchdog condemns is **abandoned**: reported as a read error (subtree pruned, dir left unmarked), a
  replacement worker is started to restore capacity, and the stuck worker is left parked in the syscall (its job ends
  when the File Provider layer finally errors). Only genuinely-hung _frontier_ dirs reach this, each pruning its
  subtree, so the parked-worker cost is bounded and self-clearing. Workers are NOT waited for (an abandoned one would
  block forever); the walk returns when the outstanding-task count hits zero. The reader is an injected `ReadDirFn`
  (production `bulk_read_dir` on macOS, `std_read_dir` elsewhere, tests a mock that blocks or trickles), so hang /
  big-but-healthy / honest-skip / parallel-correctness are unit-tested with no real hung mount.
- **Where the threads come from.** A worker is one job on `WalkConfig::threads`, production's process-wide
  `WALK_THREADS` (`cmdr_fs::utility_pool::UtilityPool`: `Utility` QoS, 8 MB stacks, a 60 s keep-alive). The pool hands
  each job an idle thread or spawns one, and never queues a job behind a busy thread, so every walk still gets its full
  `num_threads` at once and the concurrency bound is the walks' own, exactly as with dedicated threads. The watchdog
  loop runs on the caller's thread, which `walk` blocks anyway. **Why pooled:** a thread per worker per walk plus a
  watchdog each came to up to ~19,000 thread creations in half an hour on a busy machine; pooled, it's 40–124. It's
  hygiene: measured to change neither memory nor CPU, so ❌ don't cite it as a slack fix
  (`docs/notes/performance/walker-thread-pool-2026-09-27.md`). Test: `tests.rs::a_run_of_walks_reuses_its_threads`.
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

## The test-only park point

`park.rs` lets a test hold a walk between directories, so it can change the volume under a walk that's provably not
reading it. It's `#[cfg(test)]`: absent from release builds and from every build of the app, and reached from outside
the walker only through a `#[cfg(test)]` re-export in `../mod.rs`, for the real-image vanish pin
(`../../tests/vanish_tests.rs`) and the tests that hold a walk to prove it holds its volume. Nothing of it is on
`lib.rs`.

- **Arming.** `ParkHandle::arm(root, after_dirs)` registers a park for the next walk of exactly `root`; `walk` picks it
  up at start by that path, so no other walk sees it. Dropping the handle releases and unregisters it.
- **Where it parks.** In `Engine::run_worker`, after a task is popped (and after the give-up prune check), before the
  heartbeat, the throttle, and the read. Once the walk has read `after_dirs` directories, a worker reaching that point
  waits. A worker holds nothing on the volume there: the reader opens its directory and closes the fd before returning
  (`bulk_read`'s `DirFd` drop), and the task is only a path.
- **"Parked" means nobody is touching the volume.** Each worker counts itself busy from the park point until its task is
  fully handled, the visitor's per-child work included, and `wait_until_parked(timeout)` answers only once the park has
  tripped and that count is zero. Workers already past the point when it trips finish their task first, so a walk parks
  after at most `after_dirs` plus one directory per worker.
- **Why between reads.** A volume detached while the walk is parked has vanished between reads: every later read fails
  on a path that's gone. A detach under an open directory fd is a different, riskier shape, and nothing here pins it.
- **The watchdog ignores it.** A parked worker has no in-flight read in its slot, so it's never abandoned. A cancel
  doesn't wake a parked worker; `release()` or dropping the handle does.
- Tests: `tests.rs::a_parked_walk_holds_between_directories_with_nothing_in_flight_until_released` (nothing in flight
  while parked, every directory read once released, nothing abandoned) and
  `a_park_armed_for_another_root_leaves_a_walk_alone`.

## The walk holds its volume

`walk` takes the walk's own `VolumeWork` (a `WalkerWorker` child of its caller's, which `run_scan` mints) and the engine
keeps it. Every worker job holds the engine, so the share drops only once `walk` has returned AND the last worker is out
of its read. A pooled thread drops the job, engine included, before it goes idle. An abandoned worker parked in a hung
syscall keeps the volume held on purpose: a removable stop then answers `StillReleasing` instead of unmounting under a
read in flight (`../../hold.rs`). Test: `tests.rs::an_abandoned_worker_holds_its_volume_until_its_read_returns`.

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

## The bulk reader

`bulk_read.rs` is the macOS reader: one `getattrlistbulk` call per batch, its own loop (open, give up on `EINTR`,
publish each batch through `ReadProgress` before decoding it). The record layout is NOT here: it asks for `REQUEST` (an
`attr_buf::Request`) and decodes through `cmdr_fs::attr_buf::records`, the one decoder the local lister uses too
(`crates/cmdr-fs/DETAILS.md` § "`attr_buf`: the packed attribute layout"). ❌ No offset table, `attrlist`, or `ATTR_*`
constant in this crate: two hand-rolled tables of one packed ABI is how the returned-vs-requested offset bug shipped
once. What the index does with a decoded record is its own policy, in `decode_batch` / `entry_from_record`:

- **Degrade, don't drop.** A record with a name and a type is always an entry. Its `stat` is `None` (the caller pays one
  `symlink_metadata`) when an attribute the stat needs wasn't returned (the inode, or for a file any of link count,
  allocated size, logical size), or when the type isn't file / dir / symlink and carries no inline size (fifo, socket,
  device node). ❌ Never report a value the decoder answered `None` for.
- **Every record is accounted for.** A record with no name or no type, and a `BadRecord` (the record itself can't be
  trusted, so nothing after it in the batch can be located) together with every record after it, count in
  `BulkDirRead::unusable`. Entries plus unusable always equal the kernel's count.
- **A short read is never used.** The walk's `bulk_read_dir` re-reads a directory with any unusable record through
  `std_read_dir` (`complete_or_reread`), so a scan never stamps a directory listed while leaving a child out; the serial
  reconcile does the same in `read_fs_children` (`../../reconcile/DETAILS.md`), since its diff DELETES rows a listing
  lacks.
- **Decision/Why a truncated file record now re-reads the directory.** A file or symlink record too short for its file
  group (`BadRecord::Truncated`) is unusable, where the index's own parser once kept it with `stat: None` and walked on
  by its length. The decoder can't locate anything past a record it doesn't believe, and a `read_dir` re-read is
  complete and correct, so the conservative answer costs only speed on a record no measured filesystem writes.
- **Decision/Why `EINTR` re-reads the directory whole, ❌ never resumes the fd** (David, 2026-10-28,
  [#708](https://github.com/getcmdr/cmdr/issues/708)). The man page doesn't say where an interrupted call leaves the
  directory stream: if the kernel advanced the fd's offset past a batch before the interrupt surfaced, a retry on the
  same fd continues after it, and that batch silently goes missing. For the walk that's an unlisted child; for the
  reconcile it's worse, since `diff_dir_against_db` DELETES every row the listing lacks. So the read comes back
  `interrupted` with no entries, `BulkDirRead::doubt` reports it alongside an unusable count, and both callers re-read
  through `read_dir` exactly as for an unusable record. This matches the pane lister
  (`crates/cmdr-local/src/reading/bulk.rs`, `Restart::Errno`). Rejected: proving the retry safe from XNU source
  (evidence we'd have to re-verify per macOS release), and restarting the bulk read on a fresh fd a bounded number of
  times (more code for a rare path whose `read_dir` fallback costs nothing measurable).

The batch source is a parameter (`bulk_read_dir_with`, production passes `kernel_batch`), which is the seam the
interrupted path is tested through.

Tests (`bulk_read.rs`): `bulk_matches_symlink_metadata` (decoded values equal `symlink_metadata` field for field over a
rich real tree), the synthetic-record cells for each branch above (`build_record` packs a record in `REQUEST`'s layout
with attributes withheld), `a_truncated_record_and_everything_after_it_are_unusable`,
`the_walk_rereads_a_directory_with_an_unusable_record`,
`an_interrupted_bulk_read_is_marked_in_doubt_and_keeps_no_entries`, and `the_walk_rereads_an_interrupted_directory`.
