# Write operations

Copy, move, delete, trash, and zip edits, as managed background ops.

## Module map

- Spine: `manager.rs` (registry, lanes, admission), `state.rs` (op state, cancel/abort), `ledger.rs` + `reversal.rs`
  (the in-flight ledgers), `status_cache.rs` (status + the busy-volume set behind
  Eject; reach it through `state::`), `types.rs`, `mod.rs`. Everything else: DETAILS § "Files (top level)".
  Frontend: `apps/desktop/src/lib/file-operations/CLAUDE.md`.

## Must-knows

- **`spawn_managed` for copy/move/delete/trash, `run_instant` for rename/mkdir/mkfile**; blocking work goes inside
  `spawn_blocking`, so a starter returns an id before any I/O.
- **An instant op produces no `WriteCompleteEvent`**, so its driver is WRAPPED to report analytics. ❌ Never emit
  inside one: the in-archive early return drops every in-zip op.
- **An instant mutation refuses with a typed `MutationError`, ❌ never a sentence** (the frontend words it in ten
  locales); only `Unexpected` holds free text. `docs/guides/error-handling.md`.
- **A spawned op reserves every lane it touches or waits Queued**; the next admits on `on_settled`, ❌ not `Drop`.
- **`OperationIntent` is one `AtomicU8`**; ❌ never `store(...)` it. Cancel keeps copied files, Rollback removes the
  ones it still recognizes: a reversal VERIFIES before each destructive act (`reversal.rs`; ❌ never a batch or a fork,
  only the `Drop` net is unconditional), and reports what it left on `write-cancelled`. `PauseGate` is orthogonal; cancel wins. ❌ A REVERSAL never asks
  `is_cancelled`: `RollingBack` means "reverse" to the cleanup under it, so it names its reading (`StopMeans`).
- **Parking on a PERSON owes two calls** on both edges: the `human_wait.rs` clock AND `announce_human_wait(sink)`.
  Miss either and the ETA lies.
- **Stopping has two tiers**: clicks use `backend_cancel`; `backend_abort` and `cancel_all_write_operations` are the
  quit deadline's.
- **Arm `state.conflict_slot` with the QUESTION before emitting `write-conflict`** (emit-first hangs the recv); ❌ the
  dispatch mutex never spans a write.
- **An answer NAMES its clash** (`ConflictId`) and `resolve_write_conflict` REPORTS where it LANDS. ❌ Never fuse
  `AlreadyResolved`, `StaleAnswer`, or `NoPendingConflict`, nor leave a settled prompt up: a modal blocks every op.
- **Emit through `OperationEventSink`, ❌ never `AppHandle`**; `write-settled` fires once, AFTER the terminal event.
- **`write-source-item-done` is the per-source verdict**; a cross-FS move speaks TWICE per source, so the LAST wins.
  ❌ Nothing asks WHO started an op; ❌ no `Extract` op type. Binding and routing: DETAILS.
- **Register a destination with `downloads::note_pending_write_for_cmdr` BEFORE the syscall** (renames: both ends).
- **EVERY local write lands via temp+rename** (`overwrite::stage_and_land_file`), rename-aside when replacing. Temps
  carry the `.cmdr-` marker and register via `in_flight_temps` with their `TempHome`, ❌ never a bare path.
  ❌ Symlinks are never followed.
- **❌ Never `statvfs` for macOS disk space** (it rejects copies APFS purgeable space allows): use
  `volumes::get_volume_space()`.
- **Scans report `total_bytes` (copy/move) and `dedup_bytes` (delete)**: ❌ never point copy at the dedup'd one.
- **Every managed mutation journals by `op_id`**; a VOLUME op passes its REAL volume id. Bulk rename journals each hop
  as it lands: ❌ never batch to the end, nor put a rotation temp in `in_flight_temps`, whose sweep DELETES it.
- **❌ The `types` vocabulary floor `use`s no sibling**, `types/events.rs` included (one upward import re-welds 11
  modules). It holds
  `LifecycleStatus`, the ONE lifecycle answer: ❌ never re-derive it from a presence test, no new variant.
- **A SCAN parks too** (`ScanPause`): ❌ only where it checks cancel, and owes `note_parked`.
- **Every preview runs under a `ScanWatchdog`**; whoever settles it CLAIMS the outcome, and it bounds by INACTIVITY:
  feed the progress callback.
- **A FAILED op is retained out-of-band**, the one exception to removal-on-terminal; `record_failure` emits only after
  the record is GONE.
- **Op state in a test hangs off a guard**, ❌ never a `static`, and ❌ never `cancel_all_write_operations()`.

Architecture, flows, and decisions: `DETAILS.md`. Read before non-trivial work here.
