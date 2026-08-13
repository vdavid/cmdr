# Write operations

Copy, move, delete, trash, and zip edits as managed background ops: progress, cancellation, conflicts, rollback.

## Module map

- Spine: `manager.rs` (registry, lanes, admission), `state.rs` (op state, `CopyTransaction`, cancel/abort),
  `status_cache.rs` (status cache + the busy-volume set that disables Eject; reach it through `state::`), `mod.rs`
  (public API). Scan preview: `scan_preview.rs`, `scan_cache.rs`, `scan_bridge.rs`. Subdirs with their own docs:
  `transfer/`, `delete/`, `archive_edit/` (`archive_edit/CLAUDE.md`). Frontend counterpart:
  `apps/desktop/src/lib/file-operations/CLAUDE.md`.

## Must-knows

- **`spawn_managed` for copy/move/delete/trash, `run_instant` for rename/mkdir/mkfile**, with every blocking step
  (validation included) inside `spawn_blocking`, so `*_files_start` returns an `operationId` before any I/O.
- **A spawned op reserves every lane it touches** (source AND dest) or waits Queued; the next admits on the explicit
  `on_settled`, ❌ never in `Drop`.
- **A zip edit is managed, not instant**, and it owns its own rules: `archive_edit/CLAUDE.md`.
- **`OperationIntent` is one `AtomicU8`**; ❌ never `store(...)` it directly. Cancel keeps copied files, Rollback deletes
  them in reverse. `PauseGate` is orthogonal; cancel wins.
- **Parking on a PERSON has two duties**, and a new way to park owes both: open the `human_wait.rs` clock (`PauseGate`
  and `conflict_slot` do, and the ETA's rate window subtracts it, or the estimate collapses on resume) AND call
  `state.announce_human_wait(sink)` on both edges (a parked op emits nothing, so every surface keeps a speed on screen
  over a copy that has stopped). DETAILS § "Parking on a person".
- **Stopping has two tiers.** `backend_cancel` (cooperative) is what EVERY user-initiated cancel uses; `backend_abort`
  and `cancel_all_write_operations` belong to the quit deadline alone, ❌ never to a click or a teardown hook.
  `transfer/DETAILS.md` § "Two tiers of cancel".
- **Conflicts arm `state.conflict_slot` BEFORE emitting `write-conflict`** (emit-first hangs the recv), and the dispatch
  mutex ❌ never spans the file write.
- **An answer NAMES its clash** (`ConflictId`) and `resolve_write_conflict` REPORTS a `ConflictResolutionOutcome`; ❌
  never answer the slot by hand, collapse `AlreadyResolved` / `StaleAnswer` / `NoPendingConflict`, or let a retired
  clash's answer reach the one parked now.
- **Emit through `OperationEventSink`, never `AppHandle`.** `write-settled` fires once, AFTER the terminal event.
- **Register a destination with the downloads watcher's ignore set BEFORE the syscall**
  (`crate::downloads::note_pending_write_for_cmdr`; renames register both halves).
- **EVERY local write lands via temp + rename** (`overwrite::stage_and_land_file`), rename-aside only when replacing.
  Temps carry the recoverable `.cmdr-` marker and register via `in_flight_temps`. Symlinks are never dereferenced.
- **❌ Never `statvfs` for macOS disk space** (it rejects copies APFS purgeable space allows):
  `crate::volumes::get_volume_space()`.
- **Scans report `total_bytes` (copy/move) and `dedup_bytes` (delete)** — ❌ don't point copy at the dedup'd one, it
  under-reserves disk.
- **Every managed mutation journals by `op_id`** (open/record/finalize), and a VOLUME op passes the REAL volume id,
  never `"root"`. `../../operation_log/DETAILS.md` § Capture.
- **A confirmed transfer registers BEFORE its preview finishes**, staying `Running` with `phase: 'scanning'` while its
  task awaits the walk — ❌ no new `LifecycleStatus`. DETAILS § "The scan-wait".
- **A FAILED op is retained out-of-band**, the one exception to removal-on-terminal; ❌ `record_failure` emits only once
  the record is GONE.
- **Op state hangs off a struct, not a `static`**: `test_support::TestOperationGuard`,
  `operation_log::TestJournalGuard`; ❌ never `cancel_all_write_operations()` in a test.

Architecture, flows, and decisions: `DETAILS.md`. Read it before any non-trivial work here: editing, planning,
reorganizing, or advising.
