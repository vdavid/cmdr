# Local FS watch

Watch the boot disk (and `LocalExternal` drives) to keep the index live between full scans: the drive-level watcher plus
the event loop that turns its stream into index writes.

## Module map

- **watcher.rs** — the drive watcher: macOS FSEvents via `cmdr-fsevent-stream` (event ids + `sinceWhen` replay), Linux
  inotify via `notify`. `supports_event_replay()` gates replay.
- **branches.rs** — `WatchScope` + `BranchWatch`: how much of a volume its loop answers for, and the buffer that keeps a
  cover walk and a live loop off each other's ground.
- **event_loop.rs + event_loop/** — three non-calling responsibilities plus shared primitives: `live.rs`
  (`run_live_event_loop`, `process_live_batch`), `replay.rs` (cold-start journal replay), `verification.rs` +
  `verify_guard.rs` (post-replay diff), `storm.rs` (removal-storm coalescing), `tests/`.
- **churn_monitor.rs (+churn_monitor/)** — off-by-default per-subtree churn rollup (`CMDR_CHURN_SPIKE`).

## Must-knows

- **The watcher→loop channel is UNBOUNDED** (created in `lifecycle/manager.rs`). ❌ Don't re-bound it: a bounded channel
  backpressured a slow replay drain into an FSEvents overflow → a forced full scan that threw away a working replay.
  Memory is bounded by the LOOP instead (`classify_ingestion_pressure`, `INGESTION_HARD_CAP` = 5,000,000 ≈ 1.5 GB → our
  own `IngestionBacklog` fallback).
- **`process_live_batch` is three-phase, flushing between phases** so later phases see committed state: (1) directory
  creations depth-sorted, (2) the `detect_renames_by_inode` pre-pass, (3) the rest, with removal-storm coalescing. Both
  live loops call it.
- **Renames are detected by INODE, not intent** (`detect_renames_by_inode` → `MoveEntryV2`, preserves `entry_id` and
  `dir_stats`). ❌ Never revert to `DeleteSubtreeById` + `UpsertEntryV2`: that wipes the renamed dir's `dir_stats` and
  drops its subtree until a full scan heals it. (Inode is nulled on FAT/exFAT — `../paths/DETAILS.md`.)
- **A removal storm is coalesced to ONE subtree rescan** (`storm.rs`), anchored at the deepest common ancestor, NOT the
  capped grouping prefix. The deleted root's own `rmdir` takes the normal per-file path (only STRICT descendants are
  dropped), and every dropped event re-queues the anchor.
- **The churn monitor must hook BOTH live loops.** `process_live_batch` takes the `ChurnObserver` by `&mut` so both are
  covered by construction; hooking only one measured nothing on replay. Guarded by
  `churn_monitor/tests.rs::every_live_loop_owns_a_real_churn_observer`.
- **Every live loop carries a `WatchScope`, and an event in ground a cover walk is covering RIGHT NOW is BUFFERED, not
  written** — on a scanned volume too. Writing it orphans a subtree (`INSERT OR IGNORE` drops the walker's fresh ids);
  discarding it drifts the branch's sizes with no signal. Released when the walk ends. ❌ Don't make the whole-volume
  arm skip the branch set.
- **A search-built index (`WriterOnly`) has NO watcher until `ensure_branch_watch` starts one**, scoped to the covered
  branches; a scanned volume watches everything, so a volume is branch-watched or whole-watched, never both. A coalesced
  `MustScanSubDirs` above the branches is RE-ANCHORED, never dropped; the branch-confined reconciler never walks outside
  them nor routes a shallow anchor to the whole-volume scanner. Gate: `master::branch_watch_allowed` (master switch +
  `user_disabled`) — a vetoed drive stops being kept current.
- **`AfterWalk::Forget` means "the loop already answers for this ground", ❌ never "no watcher is up"**: a failed or
  vetoed watcher leaves ground covered, record included. Collapse only via `collapse_to` — ❌ `branches::clear` +
  begin/finish mints a set the loop isn't reading, and fails silently at runtime.
- **Linux watches the BRANCHES, macOS the volume root.** `notify`'s recursive mode costs an inotify watch per directory
  against `max_user_watches`; an FSEvents stream costs nothing per directory, and its volume-rooted `sinceWhen` replays
  a branch covered last session.
- **Background verification is post-replay and boot-disk only.** Its cost-bounding (`verify_guard.rs`) is canonical in
  `../reconcile/DETAILS.md`; ❌ don't restate it here.

Architecture, the ingestion-pressure trend model, removal storms, rename-by-inode, and verification: `DETAILS.md`. Read
it before any non-trivial work here: editing, planning, reorganizing, or advising.
