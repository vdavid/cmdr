# Write operations

Copy, move, delete, trash, and zip edits as managed background ops.

## Module map

- Spine: `manager.rs` (registry, lanes, admission), `state.rs` (op state, `CopyTransaction`, cancel/abort),
  `status_cache.rs` (status + the busy-volume set behind Eject; reach it through `state::`), `types.rs`, `mod.rs`
  (public API). Also `scan_{preview,cache,bridge,watchdog}.rs`, `routing.rs`, `source_binding.rs`, `mutation_error.rs`. Own
  docs: `transfer/`, `delete/`, `archive_edit/`, plus `apps/desktop/src/lib/file-operations/CLAUDE.md`.

## Must-knows

- **`spawn_managed` for copy/move/delete/trash, `run_instant` for rename/mkdir/mkfile**; blocking work inside
  `spawn_blocking`, so a starter returns an id before any I/O. Zip edits: `archive_edit/`.
- **An instant op produces no `WriteCompleteEvent`**, so its driver is WRAPPED to report analytics (`analytics.rs`).
  ❌ Never emit inside one: the in-archive early return would drop every in-zip op.
- **An instant mutation refuses with a typed `MutationError` (`mutation_error.rs`), ❌ never a sentence.** The frontend
  words it in ten locales; the `Volume` variant carries the whole `VolumeError`, and only `Unexpected` holds free
  text. `docs/guides/error-handling.md`.

- **A spawned op reserves every lane it touches or waits Queued**; the next admits on `on_settled`, ❌ not `Drop`.
- **`OperationIntent` is one `AtomicU8`**; ❌ never `store(...)` it. Cancel keeps copied files, Rollback deletes them
  in reverse. `PauseGate` is orthogonal; cancel wins.
- **Parking on a PERSON owes two calls** on both edges: the `human_wait.rs` clock AND
  `state.announce_human_wait(sink)`. Miss either and the ETA or the shown speed lies.
- **Stopping has two tiers**: clicks use `backend_cancel`; `backend_abort` and `cancel_all_write_operations` are the
  quit deadline's.
- **Arm `state.conflict_slot` with the QUESTION before emitting `write-conflict`** (emit-first hangs the recv); ❌ the
  dispatch mutex never spans a write.
- **An answer NAMES its clash** (`ConflictId`); `resolve_write_conflict` REPORTS where it LANDS
  (`ConflictResolutionOutcome`). ❌ Never fuse `AlreadyResolved`, `StaleAnswer`, `NoPendingConflict`, or leave a settled prompt
  up: a modal blocks every new op.
- **Emit through `OperationEventSink`, ❌ never `AppHandle`**, built at the IPC edge. `write-settled` fires once, AFTER
  the terminal event.
- **`write-source-item-done` is the per-source verdict** (`Done`, `Skipped`, `Failed`). A cross-FS move speaks twice
  per source, so the LAST wins; `source_removed` drives snapshot purge.
- **A caller may BIND the sources an op may touch** (`source_binding.rs`, an `Option` on every starter): one no longer
  matching its promise is dropped before any I/O, reported `Skipped`. ❌ Nothing asks WHO started an op.
- **One routing for every cross-volume transfer** (`routing.rs`: `start_volume_{copy,move,compress}`); the IPC command
  only builds the sink. Extract IS a copy with an `ArchiveVolume` source, ❌ no `Extract` op type.
- **Register a destination with `downloads::note_pending_write_for_cmdr` BEFORE the syscall** (renames register both).
- **EVERY local write lands via temp+rename** (`overwrite::stage_and_land_file`), rename-aside when replacing. Temps
  carry the `.cmdr-` recovery marker and register via `in_flight_temps`. Symlinks are never followed.
- **❌ Never `statvfs` for macOS disk space** (it rejects copies APFS purgeable space allows): use
  `crate::volumes::get_volume_space()`.
- **Scans report `total_bytes` (copy/move) and `dedup_bytes` (delete)**: ❌ never point copy at the dedup'd one, which
  under-reserves disk.
- **Every managed mutation journals by `op_id`**; a VOLUME op passes its REAL volume id.
- **Bulk rename journals every hop as it lands.** ❌ Never batch to the end, nor put a rotation temp in
  `in_flight_temps`, whose sweep DELETES it.
- **`types.rs` is the vocabulary floor: ❌ it `use`s no sibling** (one upward import re-welds 11 modules);
  take a sink from `event_sinks`, a classifier from `error_classification`.
- **`LifecycleStatus` is the ONE lifecycle answer** (in `types.rs`), carried as `OperationStatus.lifecycle`. ❌ Never
  re-derive it from a presence test; no new variant.
- **Every preview runs under a `ScanWatchdog`; whoever settles it CLAIMS the outcome**
  (`watchdog.claim_outcome()`). It bounds by INACTIVITY (60 s): feed the progress callback.
- **A FAILED op is retained out-of-band** (the one exception to removal-on-terminal); `record_failure` emits only
  after the record is GONE.
- **Op state in a test hangs off a guard**, ❌ never a `static`; ❌ never `cancel_all_write_operations()` there.

Architecture, flows, and decisions: `DETAILS.md`. Read before non-trivial work here.
