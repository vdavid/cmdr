# Local FS watch details

Read this before any non-trivial work in `watch/`: editing, planning, reorganizing, or advising. Must-know guardrails
are in `CLAUDE.md`.

This area owns the drive watcher, the live/replay event loops, the unbounded ingestion buffer + pressure model,
removal-storm coalescing, rename-detection-by-inode, and the churn-monitor spike. Points outward: the registry / phase
machine / manager wiring in `../lifecycle/DETAILS.md`; the reconciler, the per-subtree rescan
throttle, and the post-replay verification COST-BOUNDING (the two teeth) in
`../reconcile/DETAILS.md`; the writer message protocol (`MoveEntryV2` / `DeleteSubtreeById` /
delta propagation) and the honest-sizes model in `../writer/DETAILS.md`; `IndexPathSpace`,
firmlink normalization, and the FAT/exFAT inode-nulling rule in `../paths/DETAILS.md`; the
`extract_metadata` primitive at `../metadata.rs` (documented in the [hub](../DETAILS.md)).

## Module structure

- **watcher.rs** — the drive-level filesystem watcher. macOS: FSEvents via `cmdr-fsevent-stream` with event IDs and
  `sinceWhen` replay. Linux: `notify` (inotify) with recursive watching and a synthetic event counter. Other platforms:
  stub. `supports_event_replay()` lets callers branch on whether journal replay is available.
- **event_loop.rs** — holds only what more than one loop uses: `merge_fs_events` (deduplication with flag priority),
  `open_read_conn_with_retry` (read-connection open at each loop's start), `ReplayConfig` (the manager→replay bridge
  struct), the cross-loop flush/gap constants (`LIVE_FLUSH_INTERVAL_MS`, `THROTTLE_SWEEP_INTERVAL_MS`,
  `JOURNAL_GAP_THRESHOLD`), and the ingestion-pressure model (`INGESTION_BACKLOG_WARN`, `INGESTION_HARD_CAP`,
  `classify_ingestion_pressure`, `BacklogTracker` / `report_backlog`). Re-exports `run_live_event_loop` /
  `process_live_batch` / `run_replay_event_loop` so external callers (`lifecycle/manager.rs`, `scan_completion.rs`, the
  stress tests) keep stable paths.
- **event_loop/live.rs** — `run_live_event_loop` (real-time processing after scan completes), `process_live_batch`
  (three-phase; below), and the live-path helpers `mark_pending_and_drain` / `split_parent_and_name`.
  `detect_renames_by_inode` lives here.
- **event_loop/replay.rs** — `run_replay_event_loop` (cold-start journal replay, two-phase, boot disk only; hands off to
  live mode and spawns verification), the replay-only bounded-buffer constants (`MAX_AFFECTED_PATHS`,
  `REPLAY_EVENT_COUNT_LIMIT`, `REPLAY_DEDUP_BATCH_SIZE`), and `defer_replay_rescan` / `flush_replay_batch`. Deferred
  `MustScanSubDirs` anchors are collected into a `HashSet` (dedup) and handed to the live drain after replay; NO
  subtree-count cap and no full-scan escalation on churn (the live drain dedups, ancestor-collapses, and
  per-subtree-throttles them). Full-scan fallback stays only for a genuine journal purge, >10M replayed events, or a
  watcher-channel overflow; its cause rides the `oneshot::<RescanReason>`.
- **event_loop/verification.rs** — `run_background_verification` + `verify_affected_dirs` (below).
- **event_loop/verify_guard.rs** — the two pure cost-bounding decisions for verification (`VerifyVerdict`,
  `HUGE_DIR_CHILDREN`). Structural role below; the cost-bounding RATIONALE is canonical in
  `../reconcile/DETAILS.md` § Bounding verification cost.
- **event_loop/storm.rs** — removal-storm coalescing helpers (`REMOVAL_STORM_THRESHOLD`, `STORM_GROUP_PREFIX_DEPTH`).
- **event_loop/tests/** — `ingestion` / `merge` / `rename` / `split_parent` clusters plus shared fixtures in `mod.rs`.
- **churn_monitor.rs (+churn_monitor/)** — the off-by-default per-subtree churn observability spike (below).

## Data flow (live + replay)

```
Live mode:
  |-- macOS: FSEvents -> reconciler (resolve_path -> entry IDs) -> UpsertEntryV2/MoveEntryV2/
  |          DeleteEntryById/DeleteSubtreeById -> writer -> SQLite
  |-- Linux: inotify (via notify) -> same pipeline
  |-- The loop holds a READ connection for integer-keyed path resolution (never a write one)
  |-- Events deduplicated by normalized path, flushed every 1s; writer flush before emit ensures atomic dir_stats
  |-- process_live_batch is three-phase: dir creates (depth-sorted) -> rename pre-pass (inode -> MoveEntryV2)
  |          -> remaining events (removal-storm coalescing). Flushes between phases so later phases see committed state.

Cold-start replay (boot disk only, has_event_journal()):
  |-- sinceWhen replay -> two-phase drain -> hands off to live mode -> spawns run_background_verification
```

## Unbounded ingestion buffer

The watcher→loop channel (`mpsc::unbounded_channel`, created in `lifecycle/manager.rs`'s `start_scan` / `start_replay`)
is UNBOUNDED so the FSEvents forward task (`watcher.rs`) NEVER blocks. A bounded 20K channel used to backpressure:
during a long replay the loop drains slower than FSEvents produces → the channel fills → `send().await` blocks the
forward task → the upstream cmdr-fsevent-stream buffer overflows and sets its flag → `WatcherChannelOverflow` → a forced
full scan (measured firing at a 100M-event replay). So a slow drain, not real data loss, threw away a working replay.
Decoupling ingestion from processing removes that cascade. Memory is bounded by the loop instead of the channel, via
`classify_ingestion_pressure(event_rx.len())` (checked on each loop's flush tick and at the replay dedup-batch
boundary):

- **Healthy** (`<= INGESTION_BACKLOG_WARN = 20_000`): do nothing. Steady state sits well under this (each event ~300 B).
- **FallingBehind** (`> 20_000`): REPORT the backlog (rate-limited to one line per `INGESTION_WARN_INTERVAL = 5 s`),
  never drop. This is a metric, not an action — the old forced-scan point is now merely a signal. The report is decided
  by TREND, not depth: `BacklogTracker::sample` compares each sample against the previous one and returns
  `(warn, message)` — a shrinking queue is progress and goes out at `info` with the drain rate and an ETA ("working
  through a backlog of 787,194 events (down 43,866 in 5.0s, ~89s left at this rate)"), while only a flat-or-growing one
  warns ("ingestion queue not draining"). Why: depth alone can't distinguish a healthy cold start from a stuck queue, so
  an 800k-event replay that drained monotonically to completion emitted ~90 "falling behind" warnings while nothing was
  wrong — the surest way to train everyone to ignore the log. `IngestionPressure::Healthy` calls `reset()`, ending the
  episode so a later backlog is never compared against a depth from minutes ago. Both replay phases and the live loop
  share one tracker each via `report_backlog`.
- **Overflowing** (`> INGESTION_HARD_CAP = 5_000_000` ≈ 1.5 GB): DELIBERATELY fall back to a full scan (RescanReason
  `IngestionBacklog`) — OUR decision that we're hopelessly behind, at a far higher threshold than the old OS overflow,
  and comfortably below the global 16 GB memory watchdog. The live loop spawns `manager::perform_registry_rescan`; the
  replay loop uses its `fallback_tx`. The genuine upstream-drop `WatcherChannelOverflow` path is preserved
  (cmdr-fsevent-stream can still drop before our forward task reads). `classify_ingestion_pressure` is
  pure/unit-tested; the repro (a backlog past the old 20K cap absorbs without forcing a scan) lives in
  `event_loop/tests/ingestion.rs`.

## Rename detection by inode (FS identity, not intent tracking)

A rename used to land as `DeleteSubtreeById(old_path)` + `UpsertEntryV2(new_path)`, which wiped the renamed dir's
`dir_stats` and dropped the entire subtree from the index. Result: the dir's size column flipped to the "dir"
placeholder until the next full scan or per-navigation verification healed it. The fix uses inode as stable identity:
`process_live_batch` runs a `detect_renames_by_inode` pre-pass between the dir-create phase and the general phase. For
each `item_renamed` event whose new path stat'd OK, it looks up the inode via `find_entry_by_inode()` and, if the
existing row is at a different `(parent_id, name)`, sends a `MoveEntryV2` that rewrites the row in place (preserving
`entry_id`, preserving `dir_stats`). The OLD-path event of the same rename pair stays in the batch but resolves to None
after the post-pre-pass flush, so it silent-no-ops. This is filesystem-driven, not intent-driven: no rename buffer, no
cross-batch state, no Cmdr-vs-external rename detection. Just `stat()` + a DB lookup per `item_renamed` event. The
kernel preserves directory inodes across rename on APFS, HFS+, ext4/btrfs/XFS, and NTFS, which covers every internal
Mac/Linux disk and most external drives.

**Inode is NOT trusted on FAT/exFAT.** A derived `st_ino` there is unstable and a delete+create aliases a fresh file
onto a freed inode, so the pre-pass would FALSE-MATCH it as a move and re-home the deleted entry's `dir_stats` onto an
unrelated file. Every local write path stores `inode: None` on such a `LocalExternal` volume, making `find_entry_by_inode`
inert and every change fall back to the safe delete+create. The volume-level trust decision (`trust_inode` /
`inodes_trustworthy`) is canonical in `../paths/DETAILS.md`.

## Removal-storm coalescing (`event_loop/storm.rs`)

`rm -rf` is depth-first (unlink all files, then rmdir each emptied dir, the root LAST), and FSEvents reports that order
faithfully, so the cheap one-`DeleteSubtreeById` path used to fire only at the very END — after the reconciler chewed
through hundreds of thousands of per-file removals (2–5 minutes on a 60 GB tree). `process_live_batch` now synthesizes
the coalescing the kernel didn't: per 1 s batch it groups removal events by a component-capped prefix
(`STORM_GROUP_PREFIX_DEPTH = 8`, the GROUPING KEY only) and, when a group exceeds `REMOVAL_STORM_THRESHOLD` (200),
queues ONE `queue_must_scan_sub_dirs` anchored at the group's **deepest common ancestor** — NOT the capped prefix,
which on a deep incident path (~11 components) would re-list a whole worktree instead of just `target`. From then on,
removal events under a queued-or-active rescan prefix are dropped, with three load-bearing rules:

- the reconciler reads the active rescan path from a shared slot (`active_rescan_path`, set at spawn / cleared on
  completion — `start_next_rescan` pops the path out of `pending_rescans` before spawning);
- only STRICT descendants are dropped (the deleted root's own `rmdir` must take the normal per-file path →
  `DeleteSubtreeById`, because `reconcile_subtree` on a root gone from disk deletes nothing);
- every dropped event re-queues the anchor (set-dedup makes it idempotent) so a sub-threshold tail batch after the walk
  already listed those dirs still gets a follow-up.

A cheap complement below the threshold: each batch's removals are sorted dirs-before-files, shallower-first, so a small
dir's `rmdir` processes before its children's unlinks and turns them into cheap unknown-path skips. Net: index latency
≈ 15–30 s after the `rm` finishes instead of minutes, and ~20× less CPU/IO. Routing through the rescan queue (not a
bespoke "big delete" path) inherits dedup, ancestor-collapse, 1-concurrency, the held-hourglass tier, and the
completion emit for free. The rescan queue itself and its per-subtree throttle live in
`../reconcile/DETAILS.md`.

## Background verification (structure)

`run_background_verification(affected_paths, writer, app)` runs off the async pool AFTER live mode starts (so the app is
responsive immediately) and readdir-diffs each directory the replay touched — FSEvents journal replay coalesces events,
so a child deletion may only show as "parent dir modified" and a new child may get no individual creation event, so each
affected parent is re-listed and reconciled with the DB. Corrections go through the writer channel, which serializes
them with live writes. It is **root-scoped (boot disk only)**: it reads the ROOT `ReadPool` (`get_read_pool()`),
resolves against root's index, and publishes under `ROOT_VOLUME_ID` — post-replay, and replay is gated on
`has_event_journal()`, so it never runs for a mount-rooted volume.

`verify_affected_dirs` is the lock-free, two-phase DB-vs-disk reconcile it calls (it acquires NO lifecycle lock): Phase
1 (sync, SQLite) materializes each affected path's DB children off the `ReadPool`; Phase 2 (`spawn_blocking`) readdirs
disk and diffs, sending `UpsertEntryV2` / `DeleteEntryById` / `DeleteSubtreeById` / `PropagateDeltaById` corrections. It
consults `verify_guard.rs`'s two pure decisions to cap the per-directory cost. **The cost-bounding rationale (the two
teeth: the `LIMIT`-probe before the snapshot and the `read_dir` iteration cap, and why a declined dir must NOT be marked
`listed_epoch = 0`) is canonical in `../reconcile/DETAILS.md` § Bounding verification cost.**
Don't restate it here.

## Churn-monitor spike (`churn_monitor.rs`)

Read-only per-subtree churn observability for the sealed-subtrees spike, off unless `CMDR_CHURN_SPIKE` is set. It hooks
`process_live_batch`, which takes a `ChurnObserver` by `&mut` so BOTH live loops (`live.rs` post-scan and `replay.rs`
Phase 3 post-replay) are covered by construction — hooking only one of them measured nothing on the whole cold-start
replay route, and `churn_monitor/tests.rs::every_live_loop_owns_a_real_churn_observer` now guards that. It rolls every
path's churn up the ancestor chain and logs one `indexing::churn` rollup per period (top-N directories by rolled-up
count, with a distinct-churny-children signal). Writes no index state and changes no behaviour. Pure and clock-injected,
so it's promotable into real churn accounting rather than throwaway. Collection and analysis handover:
`docs/notes/churn-observability-spike.md`.
