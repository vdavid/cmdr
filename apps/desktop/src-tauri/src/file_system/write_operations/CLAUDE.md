# Write operations

Copy, move, delete, trash, and zip edits as managed background ops.

## Module map

- Spine: `manager.rs` (registry, lanes, admission), `state.rs` (op state, cancel/abort), `ledger.rs` + `reversal.rs`
  (the in-flight ledgers), `status_cache.rs` (status + Eject's busy-volume set; reach it through `state::`), `types.rs`,
  `mod.rs`. Rest: DETAILS § "Files (top level)". Frontend: `apps/desktop/src/lib/file-operations/CLAUDE.md`.

## Must-knows

- **`spawn_managed` for copy/move/delete/trash, `run_instant` for rename/mkdir/mkfile**; blocking work goes inside
  `spawn_blocking`, so a starter returns an id before I/O.
- **An instant op emits no `WriteCompleteEvent`** (analytics WRAPS its driver): ❌ never emit inside one, or the
  in-archive early return drops in-zip ops. It refuses with a typed `MutationError`, ❌ never a sentence.
- **A spawned op reserves every lane it touches or waits Queued**; the next admits on `on_settled`, ❌ never `Drop`.
- **`OperationIntent` is one `AtomicU8`**; ❌ never `store(...)` it. Cancel keeps copied files; Rollback removes what it
  still recognizes, VERIFYING before each destructive act (❌ never a batch or a fork; only the `Drop` net is
  unconditional). Pause is orthogonal, cancel wins. ❌ A reversal never asks `is_cancelled`; it reads `StopMeans`.
- **A loop parks where it checks cancel**, via `state.stop_or_park_sync()` / `_async()`, ❌ never hand-rolled; a SCAN's
  own is `ScanPause`, owing `note_parked`. **Parking on a PERSON owes both edges**, the `human_wait.rs` clock AND
  `announce_human_wait(sink)`, or the ETA lies. Clicks stop via `backend_cancel`; `backend_abort` /
  `cancel_all_write_operations` are the quit deadline's.
- **Arm `state.conflict_slot` with the QUESTION before emitting `write-conflict`** (emit-first hangs the recv); ❌ the
  dispatch mutex never spans a write. An answer NAMES its clash (`ConflictId`): ❌ never fuse `AlreadyResolved`,
  `StaleAnswer`, or `NoPendingConflict`, nor leave a settled prompt up.
- **Crossing types takes a person's consent, per SHAPE** (`conflict::resolution_for_clash`, ALL THREE engines): only a
  plain Overwrite answered for that `ClashKind` replaces, ITS "* all" carry included; the config policy, same-kind
  carries, and `OverwriteSmaller`/`Older` `Skip`. ❌ Never reduce a conditional before it. What ARRIVES is the caller's
  word (`IncomingItem`).
- **Emit through `OperationEventSink`, ❌ never `AppHandle`**; `write-settled` fires once, AFTER the terminal event. A
  cross-FS move speaks TWICE per source on `write-source-item-done`, so the LAST wins.
- **EVERY local write lands via `overwrite::stage_and_land_file`** (temp+rename, rename-aside when replacing); temps
  register in `in_flight_temps` with a `TempHome`, ❌ never a bare path. Register a destination via
  `downloads::note_pending_write_for_cmdr` BEFORE the syscall (renames: both ends).
- **Every managed mutation journals by `op_id`**; a VOLUME op passes its REAL volume id. Bulk rename journals each hop
  as it lands: ❌ never batch to the end, nor put a rotation temp in `in_flight_temps` (its sweep DELETES it).
- **❌ Never `statvfs` for macOS disk space** (it rejects copies APFS purgeable space permits): use
  `volumes::get_volume_space()`. Scans report `total_bytes` (copy/move) and `dedup_bytes` (delete): ❌ never point copy
  at the dedup'd one.
- **❌ The `types` vocabulary floor `use`s no sibling**, `types/events.rs` included. It holds `LifecycleStatus`, the ONE
  lifecycle answer: ❌ never re-derive it from a presence test, no new variant.
- **Every preview runs under a `ScanWatchdog`**, bounded by INACTIVITY: feed the progress callback, and whoever settles
  it CLAIMS the outcome.
- **A FAILED op is retained out-of-band**, the one exception to removal-on-terminal; `record_failure` emits only after
  the record is GONE. **Test op state hangs off a guard**, ❌ never a `static`, ❌ never
  `cancel_all_write_operations()`.

Architecture, flows, and decisions: `DETAILS.md`. Read it before any non-trivial work here: editing, planning,
reorganizing, or advising.
