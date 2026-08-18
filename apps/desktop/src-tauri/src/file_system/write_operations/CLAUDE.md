# Write operations

Copy, move, delete, trash, and zip edits as managed background ops: progress, cancellation, conflicts, rollback.

## Module map

- Spine: `manager.rs` (registry, lanes, admission), `state.rs` (op state, `CopyTransaction`, cancel/abort),
  `status_cache.rs` (status + the busy-volume set that disables Eject; reach it through `state::`), `mod.rs` (public
  API). Scan preview: `scan_{preview,cache,bridge,watchdog}.rs`. Subdirs with their own docs: `transfer/`, `delete/`,
  `archive_edit/`. Frontend counterpart: `apps/desktop/src/lib/file-operations/CLAUDE.md`.

## Must-knows

- **`spawn_managed` for copy/move/delete/trash, `run_instant` for rename/mkdir/mkfile**, every blocking step inside
  `spawn_blocking` so `*_files_start` returns an `operationId` before any I/O. A zip edit is managed, not instant, and
  owns its own rules: `archive_edit/CLAUDE.md`.
- **A spawned op reserves every lane it touches** (source AND dest) or waits Queued; the next admits on the explicit
  `on_settled`, ❌ never in `Drop`.
- **`OperationIntent` is one `AtomicU8`**; ❌ never `store(...)` it directly. Cancel keeps copied files, Rollback
  deletes them in reverse; `PauseGate` is orthogonal and cancel wins.
- **Parking on a PERSON has two duties**: open the `human_wait.rs` clock (or the ETA collapses on resume) AND call
  `state.announce_human_wait(sink)` on both edges (a parked op emits nothing, so surfaces would keep a speed on screen
  over a stopped copy). DETAILS § "Parking on a person".
- **Stopping has two tiers.** User cancels use the cooperative `backend_cancel`; `backend_abort` and
  `cancel_all_write_operations` belong to the quit deadline alone, ❌ never a click or a teardown hook.
  `transfer/DETAILS.md` § "Two tiers of cancel".
- **Conflicts arm `state.conflict_slot` with the QUESTION before emitting `write-conflict`** (emit-first hangs the
  recv), and the dispatch mutex ❌ never spans the file write. DETAILS § "Stop-mode conflict resolution".
- **An answer NAMES its clash** (`ConflictId`), `resolve_write_conflict` REPORTS a `ConflictResolutionOutcome`,
  announced from where it LANDS (`emit_conflict_resolved`). ❌ Never answer the slot by hand, collapse
  `AlreadyResolved` / `StaleAnswer` / `NoPendingConflict`, let a retired clash's answer reach the one parked now, or
  leave a prompt up for a settled question (a modal blocks every new operation).
- **Emit through `OperationEventSink`, ❌ never `AppHandle`.** `write-settled` fires once, AFTER the terminal event.
- **Register a destination with the downloads watcher's ignore set BEFORE the syscall**
  (`downloads::note_pending_write_for_cmdr`; renames register both halves).
- **EVERY local write lands via temp + rename** (`overwrite::stage_and_land_file`), rename-aside only when replacing.
  Temps carry the recoverable `.cmdr-` marker and register via `in_flight_temps`. Symlinks are never followed.
- **❌ Never `statvfs` for macOS disk space** (it rejects copies APFS purgeable space allows):
  `crate::volumes::get_volume_space()`.
- **Scans report `total_bytes` (copy/move) and `dedup_bytes` (delete)**: ❌ don't point copy at the dedup'd one; it
  under-reserves disk.
- **Every managed mutation journals by `op_id`**, and a VOLUME op passes the REAL volume id, never `"root"`.
  `../../operation_log/DETAILS.md` § Capture.
- **Bulk rename journals every hop as it lands.** ❌ Never batch to the end; never put a rotation temp in
  `in_flight_temps`, whose sweep DELETES. DETAILS § "Bulk rename's hop log".
- **`LifecycleStatus` is the ONE lifecycle answer**, carried to the query API as `OperationStatus.lifecycle`. ❌ Never
  re-derive one from a presence test like `WRITE_OPERATION_STATE.contains`, and ❌ no new variant. DETAILS §
  "Lifecycle status", § "The scan-wait".
- **Every preview runs under a `ScanWatchdog`, and whoever settles it CLAIMS the outcome first**
  (`watchdog.claim_outcome()`), or a late walk contradicts a timeout the user already saw. It bounds the walk by
  INACTIVITY (60 s counting nothing), not duration, so feed the progress callback. DETAILS § "Bounding the scan".
- **A FAILED op is retained out-of-band**, the one exception to removal-on-terminal; `record_failure` emits only once
  the record is GONE.
- **Op state in a test hangs off a guard, not a `static`** (`test_support::TestOperationGuard`,
  `operation_log::TestJournalGuard`); ❌ never `cancel_all_write_operations()` in a test.

Architecture, flows, and decisions: `DETAILS.md`. Read it before any non-trivial work here.
