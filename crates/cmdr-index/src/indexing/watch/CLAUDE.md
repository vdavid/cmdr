# Local FS watch

Watch the boot disk (and `LocalExternal` drives) and keep the index live between full scans: the drive-level watcher
plus the event loop that turns its stream into index writes.

## Module map

- **watcher.rs** — the drive watcher: macOS FSEvents via `cmdr-fsevent-stream` (event IDs + `sinceWhen` replay), Linux
  inotify via `notify`. `supports_event_replay()` gates journal replay.
- **branches.rs** — `WatchScope` + `BranchWatch`: how much of a volume its loop answers for, and the buffer that keeps a
  cover walk and a live loop off each other's ground.
- **event_loop.rs + event_loop/** — three non-calling responsibilities plus shared primitives: `live.rs`
  (`run_live_event_loop`, `process_live_batch`), `replay.rs` (`run_replay_event_loop`, cold-start journal replay),
  `verification.rs` + `verify_guard.rs` (post-replay diff), `storm.rs` (removal-storm coalescing), `tests/`.
- **churn_monitor.rs (+churn_monitor/)** — off-by-default per-subtree churn rollup (env `CMDR_CHURN_SPIKE`).

## Must-knows

- **The watcher→loop channel is UNBOUNDED** (`mpsc::unbounded_channel`, created in `lifecycle/manager.rs`). ❌ Don't
  re-bound it: a bounded channel backpressured a slow replay drain into an upstream FSEvents overflow → a forced full
  scan that threw away a working replay. Memory is bounded by the LOOP instead, via `classify_ingestion_pressure`
  (`INGESTION_HARD_CAP` = 5,000,000 ≈ 1.5 GB → our own `IngestionBacklog` full-scan fallback).
- **`process_live_batch` is three-phase, flushing between phases** so later phases see committed state: (1) directory
  creations depth-sorted, (2) `detect_renames_by_inode` rename pre-pass, (3) remaining events with removal-storm
  coalescing. Both live loops (`live.rs` post-scan, `replay.rs` Phase 3 post-replay) call it.
- **Renames are detected by INODE, not intent** (`detect_renames_by_inode` → `MoveEntryV2`, preserves `entry_id` and
  `dir_stats`). ❌ Never revert to `DeleteSubtreeById` + `UpsertEntryV2`: that wipes the renamed dir's `dir_stats` and
  drops its subtree until a full scan heals it. (Inode is nulled wholesale on FAT/exFAT — see `../paths/DETAILS.md`.)
- **A removal storm is coalesced to ONE subtree rescan** (`storm.rs`), anchored at the deepest common ancestor, NOT the
  capped grouping prefix. The deleted root's own `rmdir` must take the normal per-file path (only STRICT descendants are
  dropped), and every dropped event re-queues the anchor.
- **The churn monitor must hook BOTH live loops.** `process_live_batch` takes the `ChurnObserver` by `&mut` so both are
  covered by construction; hooking only one measured nothing on the cold-start replay route. Guarded by
  `churn_monitor/tests.rs::every_live_loop_owns_a_real_churn_observer`.
- **Every live loop carries a `WatchScope`, and an event in ground a cover walk is covering RIGHT NOW is BUFFERED, not
  written** — on a scanned volume too. Writing it lets the parallel walker's fresh ids lose to `INSERT OR IGNORE` and
  orphan a subtree; discarding it drifts the branch's sizes with no signal. Released when the walk ends. ❌ Don't make
  the whole-volume arm skip the branch set.
- **A search-built index (`WriterOnly`) has NO watcher until `ensure_branch_watch` starts one**, scoped to the covered
  branches; a scanned volume already watches everything, so a volume is branch-watched or whole-watched, never both. A
  coalesced `MustScanSubDirs` above the branches is RE-ANCHORED onto them, never dropped. The branch-confined reconciler
  never walks outside them and never routes a shallow anchor to the whole-volume scanner. Gate:
  `master::branch_watch_allowed` (master switch + `user_disabled` only) — a vetoed drive's walked ground stays covered
  but stops being kept current.
- **`AfterWalk::Forget` means "the loop already answers for this ground", ❌ not "no branch watcher is up".** A failed or
  vetoed watcher still leaves ground a walk COVERED, and dropping its entry erases the only record of that.
- **Linux watches the BRANCHES, macOS the volume root.** `notify`'s recursive mode costs one inotify watch per directory
  against `max_user_watches`; an FSEvents stream costs nothing per directory and its volume-rooted `sinceWhen` is what
  replays a branch covered last session.
- **Background verification is post-replay and boot-disk only.** Its cost-bounding (the two teeth in `verify_guard.rs`)
  is canonical in `../reconcile/DETAILS.md` — don't restate it here.

Architecture, the ingestion-pressure trend model, removal-storm rules, rename-by-inode, and the verification structure:
`DETAILS.md`. Read it before any non-trivial work here: editing, planning, reorganizing, or advising.
