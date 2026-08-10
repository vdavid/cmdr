# Write operations

Copy, move, delete, trash, and zip edits as managed background ops: progress, cancellation, conflicts, rollback.

## Module map

- Spine: `manager.rs` (registry, lanes, admission), `state.rs` (op state, status cache, `CopyTransaction`, busy
  volumes), `mod.rs` (public API). Subdirs `transfer/`, `delete/`, `archive_edit/`. Full inventory: DETAILS § Files.
  Frontend counterpart: `apps/desktop/src/lib/file-operations/CLAUDE.md`.

## Must-knows

- **`spawn_managed` for copy/move/delete/trash, `run_instant` for rename/mkdir/mkfile**, and all blocking work
  (validation included) inside `spawn_blocking`, so `*_files_start` returns an `operationId` before any I/O. A spawned
  op reserves every lane it touches (source AND dest) or waits Queued; the next admits on the explicit `on_settled`,
  NEVER in `Drop`.
- **A zip edit is managed, not instant**, and every apply site goes through `run_managed_edit`. ❌ No in-place remote
  edit: SMB and MTP pull the `.zip`, edit a copy, swap.
- **`OperationIntent` is one `AtomicU8`**; ❌ never `store(...)` it directly. Cancel keeps copied files, Rollback
  deletes them in reverse. **`PauseGate` is separate** and orthogonal; cancel wins.
- **Stop-mode conflicts store the oneshot sender BEFORE emitting `write-conflict`** (emit-first hangs the recv); the
  dispatch mutex serializes merges and ❌ never spans the file write.
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
- **A `preview_id` alone doesn't authorize acting on a path set**: a source mismatch is a cache miss and the caller rescans.
- **A FAILED op is retained out-of-band**, the one exception to removal-on-terminal; lanes and records free as before,
  and ❌ `record_failure` emits only once the record is GONE.
- **Op state hangs off a struct, not a `static`**: `test_support::TestOperationGuard`,
  `operation_log::TestJournalGuard`, ❌ never `cancel_all_write_operations()` in a test.

Architecture, flows, and decisions: `DETAILS.md`. Read it before any non-trivial work here: editing, planning,
reorganizing, or advising.
