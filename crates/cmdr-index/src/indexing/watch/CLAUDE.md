# Local FS watch

Keep the index live between full scans on the boot disk and `LocalExternal` drives: the drive-level watcher plus the
event loop that turns its stream into index writes.

## Module map

- **watcher.rs** — the drive watcher: macOS FSEvents via `cmdr-fsevent-stream` (event ids + `sinceWhen` replay), Linux
  inotify via `notify`. `supports_event_replay()` is the gate.
- **branches.rs** — `WatchScope` + `BranchWatch`: how much of a volume its loop answers for, and the buffer keeping a
  cover walk off a live loop's ground.
- **event_loop.rs + event_loop/** — `live.rs`, `rename_detect.rs`, `replay.rs` (cold start), `verification.rs` +
  `verify_guard.rs` (post-replay diff), `storm.rs`, `tests/`.
- **churn_monitor.rs** — off-by-default churn rollup (`CMDR_CHURN_SPIKE`). **activity_monitor.rs** — the per-folder
  activity tap, plus `BatchObservers`.

## Must-knows

- **The watcher→loop channel is UNBOUNDED** (`lifecycle/manager.rs`). ❌ Don't re-bound it: a bounded channel
  backpressured a slow replay drain into an FSEvents overflow → a forced full scan, throwing away a working replay. The
  LOOP bounds memory (`classify_ingestion_pressure`, `INGESTION_HARD_CAP` = 5,000,000 ≈ 1.5 GB).
- **`process_live_batch` is three-phase, flushing between phases** so later phases see committed state: (1) dir
  creations depth-sorted, (2) the `detect_renames_by_inode` pre-pass, (3) the rest, with removal-storm coalescing.
- **Renames are detected by INODE, not intent** (`detect_renames_by_inode` → `MoveEntryV2`, preserving `entry_id` and
  `dir_stats`). ❌ Never revert to `DeleteSubtreeById` + `UpsertEntryV2`: that wipes the renamed dir's `dir_stats` and
  drops its subtree until a full scan heals it. (Inode is null on FAT/exFAT — `../paths/DETAILS.md`.)
- **A removal storm coalesces to subtree rescans** (`storm.rs`), anchored at each storm CLUSTER's deepest common
  ancestor, ❌ never the capped grouping prefix, and never widened by a few stray deletes (that walked whole worktrees).
  Only STRICT descendants drop, and each dropped event re-queues the anchor.
- **Both observers hook BOTH live loops, by construction**: `process_live_batch` takes `BatchObservers` (churn + the
  activity tap) by `&mut`, and two scanners key on `BatchObservers::from_env(`.
- **The activity tap reads the CORRECTED stream, and three of its four counters are unreachable there**: matched renames
  are `retain`ed out of the batch, storm removals drop for a rescan, and dir creations sit in their own Vec, so each is
  wired in explicitly. Break one and a rename-only batch reports nothing. `DETAILS.md` § "The activity tap".
- **Every live loop carries a `WatchScope`, and an event in ground a cover walk is covering RIGHT NOW is BUFFERED, not
  written** — on a scanned volume too. Writing it orphans a subtree (`INSERT OR IGNORE` drops the walker's fresh ids);
  discarding it drifts the branch's sizes silently. ❌ Never let the whole-volume arm skip the branch set.
- **A search-built index (`WriterOnly`) has NO watcher until `ensure_branch_watch` starts one**, scoped to its covered
  branches; a volume is branch-watched or whole-watched, never both. A coalesced `MustScanSubDirs` above them is
  RE-ANCHORED, never dropped. Gate: `master::branch_watch_allowed`.
- **`AfterWalk::Forget` means "the loop already answers for this ground", ❌ never "no watcher is up"**: a failed or
  vetoed watcher still leaves ground covered. Collapse only via `collapse_to` — ❌ `branches::clear` + begin/finish
  mints a set the loop isn't reading, and fails silently.
- **Linux watches the BRANCHES, macOS the volume root.** ❌ Don't unify them: inotify costs a watch per directory.
  `DETAILS.md` § "Platform split".
- **Background verification is post-replay and boot-disk only.** Cost-bounding (`verify_guard.rs`) is canonical in
  `../reconcile/DETAILS.md`.

Architecture, the ingestion-pressure trend model, removal storms, rename-by-inode, the activity tap, and verification:
`DETAILS.md`. Read it before any non-trivial work here.
