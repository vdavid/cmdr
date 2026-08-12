# Write operations

Copy, move, delete, trash, and zip edits as managed background ops: progress, cancellation, conflicts, rollback.

## Module map

- Spine: `manager.rs` (registry, lanes, admission), `state.rs` (op state, `CopyTransaction`, the cancel/abort
  commands), `status_cache.rs` (the status cache, the busy-volume set it drives, and the queries over it; reached
  through `state::`), `mod.rs` (public API). Scan preview: `scan_preview.rs` (workers), `scan_cache.rs` (the map),
  `scan_bridge.rs` (the op's wait + progress bridge). Subdirs `transfer/`, `delete/`, `archive_edit/`. Full inventory:
  DETAILS § Files. Frontend counterpart: `apps/desktop/src/lib/file-operations/CLAUDE.md`.

## Must-knows

- **`spawn_managed` for copy/move/delete/trash, `run_instant` for rename/mkdir/mkfile**, and all blocking work
  (validation included) inside `spawn_blocking`, so `*_files_start` returns an `operationId` before any I/O. A spawned
  op reserves every lane it touches (source AND dest) or waits Queued; the next admits on the explicit `on_settled`,
  NEVER in `Drop`.
- **A zip edit is managed, not instant**, and every apply site goes through `run_managed_edit`. ❌ No in-place remote
  edit: SMB and MTP pull the `.zip`, edit a copy, swap.
- **`OperationIntent` is one `AtomicU8`**; ❌ never `store(...)` it directly. Cancel keeps copied files, Rollback
  deletes them in reverse. **`PauseGate` is separate** and orthogonal; cancel wins.
- **Stopping has TWO tiers.** Tier 1 is `backend_cancel` (cooperative, the backend cleans up after itself) and is what
  EVERY user-initiated cancel uses. Tier 2 is `backend_abort` (`abort_all_write_operations`): it stops WAITING, skips
  backend cleanup, and has exactly ONE caller, the quit deadline (`crate::quit`) — ❌ never anything a person clicked.
  Both triggers fire tier 1 first. `transfer/DETAILS.md` § "Two tiers of cancel".
- **A window going away never stops work.** `cancel_all_write_operations` belongs to the quit gate alone; ❌ no
  frontend teardown hook may call it. `../../quit/CLAUDE.md`.
- **Stop-mode conflicts arm `state.conflict_slot` BEFORE emitting `write-conflict`** (emit-first hangs the recv); the
  dispatch mutex serializes merges and ❌ never spans the file write.
- **`resolve_write_conflict` REPORTS its arbitration** (`ConflictResolutionOutcome`, across IPC): the event reaches every
  webview, so a second surface must be able to tell `AlreadyResolved` from `NoPendingConflict` / `UnknownOperation` and
  take its prompt down. ❌ Never answer the slot by hand or collapse those variants.
- **Emit through `OperationEventSink`, never `AppHandle`.** `write-settled` fires once, AFTER the terminal event; a
  volume-aware op doesn't re-emit `write-error` on `Cancelled`.
- **Register a destination with the downloads watcher's ignore set BEFORE the syscall**
  (`crate::downloads::note_pending_write_for_cmdr`; renames register both halves).
- **EVERY local write lands via temp + rename** (`overwrite::stage_and_land_file`), with a rename-aside only when
  replacing; ❌ never write at the destination name. Temps carry the recoverable `.cmdr-` marker and are registered via
  `in_flight_temps` (op list + a persisted log swept with NO age gate at startup). Symlinks are never dereferenced.
- **❌ Never `statvfs` for macOS disk space** (it rejects copies APFS purgeable space allows):
  `crate::volumes::get_volume_space()`.
- **Scans report `total_bytes` (write footprint, copy/move) and `dedup_bytes` (delete).** ❌ Don't point copy at the
  dedup'd one; it under-reserves disk.
- **Every managed mutation journals by `op_id`** through an open/record/finalize bracket, and a VOLUME op passes the
  REAL volume id, never `"root"`. `../../operation_log/DETAILS.md` § Capture.
- **The busy-volumes set disables Eject mid-op**; `eject_volume`'s server-side guard is the real safety net.
- **A confirmed transfer is registered BEFORE its preview finishes**; its task awaits the walk
  (`scan_bridge::await_claimed_preview`, first in every deferred), staying `Running` with `phase: 'scanning'` — ❌ no
  new `LifecycleStatus`. ONE claiming op per preview; a miss re-walks, ❌ never hangs. `Cancelled` comes from the
  worker's FLAG, ❌ not the event. `set_paused` refuses a scan-waiting record and LATCHES it. A `preview_id` alone
  doesn't authorize a path set. DETAILS § "The scan-wait".
- **A FAILED op is retained out-of-band**, the one exception to removal-on-terminal; lanes and records free as before,
  and ❌ `record_failure` emits only once the record is GONE.
- **Op state hangs off a struct, not a `static`**: `test_support::TestOperationGuard`,
  `operation_log::TestJournalGuard`, ❌ never `cancel_all_write_operations()` in a test.

Architecture, flows, and decisions: `DETAILS.md`. Read it before any non-trivial work here: editing, planning,
reorganizing, or advising.
