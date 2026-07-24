# Write operations

Copy, move, delete, and trash with progress, cancellation, conflicts, and rollback (macOS and Linux). The
cross-cutting machinery both subdirs share.

## Module map

- Subdirs: `transfer/CLAUDE.md` (copy + move, conflict resolution, driver, backends), `delete/CLAUDE.md` (delete
  walker, trash, oracle-aware fast path).
- Top level: `mod.rs` (public API + `start_write_operation` lifecycle), `manager.rs` (registry + lane admission),
  `state.rs` (status cache, `WriteOperationState`, `CopyTransaction`, busy-volumes, settle guard), `operation_intent.rs`
  (`OperationIntent`, `PauseGate`), `archive_edit/` (zip-edit driver), plus `scan_cache`, `types`, `event_sinks`,
  `validation`, `conflict`, `scan`, `test_support`, and others (full inventory in DETAILS). `operation_intent` +
  `scan_cache` re-export via `state`.
- Frontend counterpart: `apps/desktop/src/lib/file-operations/CLAUDE.md`.

## Must-knows

- **A zip edit (`ArchiveEdit`) is a managed op, NOT instant.** Editing a `.zip` (mutations inside, or copy/move INTO
  one) routes to the `archive_edit/` driver, running `ArchiveMutator` (temp+rename) via `spawn_managed` on the PARENT
  drive's lane. **Compress** = seed a valid empty zip, then copy-into (`ZipArchive::new` rejects a 0-byte target).
  DETAILS § "Archive edits".
- **Every archive apply site runs through `run_managed_edit`, never a bare `spawn_blocking(mutator::apply)`.** It
  dispatches on `parent.supports_local_fs_access()`: a LOCAL parent edits in place; a REMOTE one (SMB / MTP) pulls the
  `.zip`, edits a local copy, and swaps. Don't reintroduce an in-place remote edit. DETAILS § "Remote edit".
- **Copy/move/delete/trash spawn through `manager::spawn_managed`; rename/mkdir/mkfile run through
  `manager::run_instant`.** A spawned op reserves a slot in each lane it touches (source AND dest), else Queued; the next
  admits on the explicit `on_settled`, NEVER in `Drop`. Instant ops reserve no lane. DETAILS § Operation manager.
- **All blocking work runs in `spawn_blocking`** (including validation). `*_files_start` returns an `operationId`
  immediately (dialog opens, offers cancel).
- **`OperationIntent` is a single `AtomicU8`** (`Running → RollingBack/Stopped`, `Stopped` terminal); never
  `state.intent.store(...)` directly. Cancel keeps copied files (deletes the last partial); Rollback deletes all in
  reverse. **Pause is a separate `PauseGate`**, orthogonal to intent; cancel wins (`wake()`s a parked op).
- **Stop-mode conflict resolution stores the oneshot sender BEFORE emitting `write-conflict`** (emit-first hangs the
  recv). **The conflict-dispatch mutex serializes concurrent/nested merges**; NEVER hold it across the file write.
- **`write-settled` fires once per op, AFTER the terminal event** (a `WriteSettledGuard` Drop, panic-safe).
- **Every driver MUST register its destination with the downloads watcher's ignore set BEFORE the syscall**
  (`crate::downloads::note_pending_write_for_cmdr`; renames register BOTH halves). Scoping lives inside the helper.
- **Safe overwrite is temp + rename-aside + rename** (original intact until the new content lands); temp files use the
  crash-recoverable `.cmdr-` prefix. **Symlinks are never dereferenced** (`symlink_metadata` + loop detection).
- **On macOS never use `statvfs` alone for disk-space checks** (it rejects copies APFS purgeable space allows); use
  `crate::volumes::get_volume_space()`. `statvfs` is Linux-only.
- **Every scan reports two byte totals**: `total_bytes` (write footprint, copy/move) and `dedup_bytes` (`du`-equivalent,
  delete). Don't "fix" copy to the dedup'd number; it under-reserves disk space.
- **All write ops emit via `OperationEventSink`, not `tauri::AppHandle`**: built at the IPC edge, injected in.
- **Every managed mutation journals to the operation log** (`journal.rs`, by `op_id`); a new op kind / record point needs
  an open/record/finalize bracket or it won't appear in history. Local ops use the `_local_` helpers (baking in
  `"root"`); VOLUME (SMB/MTP) ops use `open_volume_op` / `record_volume_*` with the REAL volume id. Journaling one under
  `"root"` silently corrupts history. DETAILS § Capture.
- **The busy-volumes set disables Eject mid-op** (source AND dest IDs); the `eject_volume` server-side guard is the
  real safety net.
- **New op state hangs off a struct, not a `static`.** Fixtures: `test_support::TestOperationGuard` (unique id,
  drop-unregisters), never a literal id + manual remove; journal installs: `operation_log::TestJournalGuard`, never
  `set_journal`. DETAILS § "Test isolation".
- **Volume-aware ops must not emit `write-error` on `Cancelled`**: the inner handler already emitted `write-cancelled`.

Architecture, flows, decisions: `DETAILS.md`. Read before non-trivial work here.
