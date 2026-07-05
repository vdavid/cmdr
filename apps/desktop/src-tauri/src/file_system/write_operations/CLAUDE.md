# Write operations

Copy, move, delete, and trash with streaming progress, cancellation, conflict resolution, and rollback. macOS and Linux.
Documents the cross-cutting machinery both subdirs share (see the module map and must-knows below).

## Module map

- Subdirs: [`transfer/`](transfer/CLAUDE.md) (copy + move, conflict resolution, driver, copy backends),
  [`delete/`](delete/CLAUDE.md) (delete walker, trash, oracle-aware fast path).
- Top level: `mod.rs` (public API + `start_write_operation` spawn lifecycle), `manager.rs` (the operation manager:
  registry + lane admission every spawn path flows through), `state.rs` (status cache, `WriteOperationState`,
  `CopyTransaction`, busy-volumes, settle guard), `operation_intent.rs` (`OperationIntent` + `PauseGate` state
  machines), `scan_cache.rs` (scan-preview caches, `FileInfo`, `ScanResult`) — the last two re-exported via `state`, so
  `state::…` paths still resolve — plus `types.rs`, `event_sinks.rs`, `validation.rs`, `conflict.rs`, `scan.rs`, and
  others (full inventory in DETAILS). Behavior modules depend on `types`, not the reverse.
- Frontend counterpart: [`src/lib/file-operations/CLAUDE.md`](../../../../src/lib/file-operations/CLAUDE.md).

## Must-knows

- **A zip edit (`ArchiveEdit`) is a managed op, NOT instant.** Editing a `.zip` (mkdir/mkfile/rename/delete inside, or
  copy/move INTO one) routes to the `archive_edit/` driver, which runs the `ArchiveMutator` (temp+rename, an
  O(archive) rewrite) via `spawn_managed` on the PARENT drive's lane — a real progress bar, not the instant path. The
  instant-op forks (`create`/`rename`) and the delete/copy-into seams detect an archive target and route there; a
  `create`/`rename` return becomes the operation id, not a path. Pre-resolved policies resolve non-interactively; Stop
  prompts per file (planning runs inside the op so the oneshot is reachable; files prompt, dir-vs-dir merges silently).
  **Move OUT** is a compound Move op (`route_archive_move_out`): extract via the copy engine, then a batch `{ delete }`
  rewrite only on a fully clean extract (all-or-nothing — any skip/error/cancel deletes nothing; delete runs only after
  the extract is durable). See DETAILS § "Archive edits".
- **Every archive apply site runs through `run_managed_edit`, never a bare `spawn_blocking(mutator::apply)`.** It
  dispatches on `parent.supports_local_fs_access()`: a LOCAL parent edits the real file in place (byte-identical to
  before); a REMOTE parent (SMB / MTP) pulls the `.zip` local, edits the copy, uploads it under a remote temp name, and
  swaps (`archive_remote_edit.rs`). The remote original is untouched until the final swap; a cancel anywhere before it
  leaves it intact. The swap prefers atomic rename-overwrite, else delete-then-rename (MTP always, and its one crash
  window keeps the NEW data under the temp name — never lost). Don't reintroduce an in-place remote edit: `raw_copy_file`
  needs `Read + Seek`. See DETAILS § "Remote edit: the data-safety contract".
- **Copy/move/delete/trash spawn through `manager::spawn_managed`; rename/mkdir/mkfile run through `manager::run_instant`.**
  A spawned op holds a slot in each lane it touches (`Volume::lane_key()`, source AND dest), runs when all are free
  (budget 1), else Queued; the next admits on the explicit `on_settled`, NEVER in `Drop`. Instant ops register +
  mark-busy but reserve NO lane and never queue (a metadata syscall must not wait behind a transfer); don't fold them
  into `spawn_managed`. Full model + rationale: DETAILS § "Operation manager".
- **All blocking work runs in `spawn_blocking`** (including validation). `*_files_start` returns an `operationId`
  immediately so the dialog opens and offers cancel.
- **`OperationIntent` is a single `AtomicU8` state machine** (`Running → RollingBack/Stopped`, `Stopped` terminal).
  Cancel keeps copied files (deletes the last partial); Rollback deletes all copied files in reverse with progress.
  Never `state.intent.store(...)` directly: DETAILS.
- **Pause is a separate `PauseGate`, orthogonal to `OperationIntent`.** Drivers gate between files after the
  `is_cancelled` check; the cross-volume streaming path also parks between chunks. Cancel wins (`wake()`s a parked op).
  DETAILS § "Pause / resume".
- **Stop-mode conflict resolution must store the oneshot sender BEFORE emitting `write-conflict`** (both local-FS and
  volume branches). Emit-first races the take and hangs the recv.
- **The conflict-dispatch mutex serializes the one human across concurrent/nested merges**; NEVER hold across the file
  write. Acquire-check-emit-await-store-release sequence: DETAILS.
- **`write-settled` fires exactly once per op, AFTER the terminal event**, via a `WriteSettledGuard` whose `Drop` runs at
  end of the spawn-task scope (panic-safe). Cache cleanup (via `manager::on_settled` or the `ManagedTaskGuard` Drop)
  runs first. The FE gates the "Cancelling…" dialog close on this.
- **Every write-op driver MUST register its destination with the downloads watcher's ignore set BEFORE the syscall**
  (`crate::downloads::note_pending_write_for_cmdr`; renames register BOTH halves). Scoping lives inside the helper — no
  call-site guards.
- **Safe overwrite is temp + rename-aside + rename** (original intact until the new content is fully in place); temp
  files use the `.cmdr-` prefix (crash-recoverable).
- **Symlinks are never dereferenced** (`symlink_metadata`; loop detection via canonicalized-path `HashSet`).
- **On macOS never use `statvfs` alone for disk-space checks**; use the NSURL purgeable-aware
  `crate::volumes::get_volume_space()` (`statvfs` only on Linux). `statvfs` rejects copies APFS purgeable space allows.
- **Every scan reports two byte totals**: `total_bytes` (write footprint, used by copy/move) and `dedup_bytes`
  (`du`-equivalent, used by delete). Don't "fix" copy to the dedup'd number — it under-reserves disk space.
- **All write ops emit via `OperationEventSink`, not `tauri::AppHandle`** (`emit_progress_via_sink` calls
  `enrich_progress`). Built only at the IPC edge and injected in; nothing here constructs one.
- **The busy-volumes set disables Eject mid-op** (source AND dest volume IDs). The `eject_volume` server-side guard is
  the real safety net; the picker disable is only UX.
- **Volume-aware ops must not emit `write-error` on `Cancelled`** (the inner handler already emitted `write-cancelled`);
  the outer wrapper matches `Cancelled` and skips.

Architecture, flows, and decisions: [DETAILS.md](DETAILS.md). Read before non-trivial work here.
