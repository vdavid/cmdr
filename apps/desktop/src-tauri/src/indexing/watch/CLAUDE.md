# Local FS watch

Watch the boot disk (and `LocalExternal` drives) and keep the index live between full scans: the drive-level watcher
plus the event loop that turns its stream into index writes.

## Module map

- **watcher.rs** — the drive watcher: macOS FSEvents via `cmdr-fsevent-stream` (event IDs + `sinceWhen` replay), Linux
  inotify via `notify`. `supports_event_replay()` gates journal replay.
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
- **Background verification is post-replay and boot-disk only.** Its cost-bounding (the two teeth in `verify_guard.rs`)
  is canonical in [`../reconcile/DETAILS.md`](../reconcile/DETAILS.md) — don't restate it here.

Architecture, the ingestion-pressure trend model, removal-storm rules, rename-by-inode, and the verification structure:
[DETAILS.md](DETAILS.md). Read it before any non-trivial work here: editing, planning, reorganizing, or advising.
