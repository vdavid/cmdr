# Write operations

Copy, move, delete, and trash with progress, cancellation, conflicts, and rollback (macOS and Linux). The
cross-cutting machinery both subdirs share.

## Module map

- Subdirs: [`transfer/`](transfer/CLAUDE.md) (copy + move, conflict resolution, driver, copy backends),
  [`delete/`](delete/CLAUDE.md) (delete walker, trash, oracle-aware fast path).
- Top level: `mod.rs` (public API + `start_write_operation` lifecycle), `manager.rs` (registry + lane admission),
  `state.rs` (status cache, `WriteOperationState`, `CopyTransaction`, busy-volumes, settle guard), `operation_intent.rs`
  (`OperationIntent`, `PauseGate`), `archive_edit/` (zip-edit driver), plus `scan_cache`,
  `types`, `event_sinks`, `validation`, `conflict`, `scan`, and others (full inventory in DETAILS). `operation_intent` +
  `scan_cache` re-export via `state`.
- Frontend counterpart: [`src/lib/file-operations/CLAUDE.md`](../../../../src/lib/file-operations/CLAUDE.md).

## Must-knows

- **A zip edit (`ArchiveEdit`) is a managed op, NOT instant.** Editing a `.zip` (mutations inside, or copy/move INTO
  one) routes to the `archive_edit/` driver, running `ArchiveMutator` (temp+rename, O(archive) rewrite) via
  `spawn_managed` on the PARENT drive's lane. **Move OUT converges per-source** (batch-delete only
  fully-extracted sources; cancel/rollback delete nothing). **Compress** = seed a
  valid empty zip (`seed_empty_zip`) then copy-into; load-bearing since `ZipArchive::new` rejects a 0-byte
  target. DETAILS § "Archive edits".
- **Every archive apply site runs through `run_managed_edit`, never a bare `spawn_blocking(mutator::apply)`.** It
  dispatches on `parent.supports_local_fs_access()`: a LOCAL parent edits in place; a REMOTE parent (SMB / MTP) pulls the
  `.zip`, edits a local copy, and swaps — original untouched until the swap. Don't reintroduce an in-place remote edit
  (`raw_copy_file` needs `Read + Seek`). DETAILS § "Remote edit".
- **Copy/move/delete/trash spawn through `manager::spawn_managed`; rename/mkdir/mkfile run through
  `manager::run_instant`.** A spawned op reserves a slot in each lane it touches (source AND dest), else Queued; the next
  admits on the explicit `on_settled`, NEVER in `Drop`. Instant ops reserve NO lane and never queue — a metadata syscall
  must not wait behind a transfer. DETAILS § "Operation manager".
- **All blocking work runs in `spawn_blocking`** (including validation). `*_files_start` returns an `operationId`
  immediately so the dialog opens and offers cancel.
- **`OperationIntent` is a single `AtomicU8`** (`Running → RollingBack/Stopped`, `Stopped` terminal); never
  `state.intent.store(...)` directly. Cancel keeps copied files (deletes the last partial); Rollback deletes them all in
  reverse. **Pause is a separate `PauseGate`**, orthogonal to intent; cancel wins (`wake()`s a parked op).
- **Stop-mode conflict resolution must store the oneshot sender BEFORE emitting `write-conflict`** — emit-first races
  the take and hangs the recv. **The conflict-dispatch mutex serializes the one human across concurrent/nested merges**;
  NEVER hold it across the file write.
- **`write-settled` fires exactly once per op, AFTER the terminal event** (a `WriteSettledGuard` Drop, panic-safe). The
  FE gates the "Cancelling…" dialog close on this.
- **Every write-op driver MUST register its destination with the downloads watcher's ignore set BEFORE the syscall**
  (`crate::downloads::note_pending_write_for_cmdr`; renames register BOTH halves). Scoping lives inside the helper — no
  call-site guards.
- **Safe overwrite is temp + rename-aside + rename** (original intact until the new content is fully in place); temp
  files use the `.cmdr-` prefix (crash-recoverable). **Symlinks are never dereferenced** (`symlink_metadata`, with loop
  detection).
- **On macOS never use `statvfs` alone for disk-space checks** (it rejects copies APFS purgeable space allows); use
  `crate::volumes::get_volume_space()`, `statvfs` only on Linux.
- **Every scan reports two byte totals**: `total_bytes` (write footprint, copy/move) and `dedup_bytes` (`du`-equivalent,
  delete). Don't "fix" copy to the dedup'd number — it under-reserves disk space.
- **All write ops emit via `OperationEventSink`, not `tauri::AppHandle`** — built only at the IPC edge, injected in.
- **The busy-volumes set disables Eject mid-op** (source AND dest IDs); the `eject_volume` server-side guard is the real
  safety net, the picker disable is UX.
- **Volume-aware ops must not emit `write-error` on `Cancelled`** — the inner handler already emitted `write-cancelled`,
  so the outer wrapper skips it.

Architecture, flows, and decisions: [DETAILS.md](DETAILS.md). Read it before any non-trivial work here.
