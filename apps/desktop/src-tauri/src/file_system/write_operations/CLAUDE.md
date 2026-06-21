# Write operations

Copy, move, delete, and trash with streaming progress, cancellation, conflict resolution, and rollback. macOS and Linux.
Documents the cross-cutting machinery both subdirs share: the operation manager (queue/lanes), the `OperationIntent`
state machine, the caches, the `OperationEventSink` trait, scan/scan-preview, the `EtaEstimator`, and the settle
contract.

## Module map

- Subdirs: [`transfer/`](transfer/CLAUDE.md) (copy + move, conflict resolution, driver, copy backends),
  [`delete/`](delete/CLAUDE.md) (delete walker, trash, oracle-aware fast path).
- Top level: `mod.rs` (public API + `start_write_operation` spawn lifecycle), `manager.rs` (the operation manager:
  registry + lane admission every spawn path flows through), `state.rs` (caches, `WriteOperationState`,
  `CopyTransaction`, settle guard), plus `types.rs`, `event_sinks.rs`, `validation.rs`, `conflict.rs`, `scan.rs`, and
  others (full inventory in DETAILS). Behavior modules depend on `types`, not the reverse.
- Frontend counterpart: [`src/lib/file-operations/CLAUDE.md`](../../../../src/lib/file-operations/CLAUDE.md).

## Must-knows (invariants and guardrails)

- **Every write op spawns through `manager::spawn_managed`** (all five paths). The manager owns the registry + lane
  admission: an op holds a slot in each lane it touches (`Volume::lane_key()`, source AND dest), runs only when all are
  free (budget 1), else Queued. Lanes free + next op admits on the explicit `on_settled`, NEVER in `Drop`
  (`ManagedTaskGuard` Drop frees lanes/caches, never spawns). Busy set = Running ops' volumes ∪ external seam. Full
  model, lane derivation, deferred-thunk rationale: DETAILS § "Operation manager".
- **All blocking work runs in `spawn_blocking`** (including validation), never on the async executor. The
  `*_files_start` functions return an `operationId` immediately so the dialog opens and offers cancel even on a stalled
  mount.
- **`OperationIntent` is a single `AtomicU8` state machine** (`Running → RollingBack/Stopped`, `Stopped` terminal).
  Drive it through the public interface in tests, never `state.intent.store(...)` (bypasses the validation guard). Cancel
  keeps copied files (deletes the last partial); Rollback deletes all copied files in reverse with progress.
- **Pause is a separate `PauseGate`, orthogonal to `OperationIntent`.** Drivers gate between files AFTER the
  `is_cancelled` check (cancel-before-destructive ordering); cancel wins (`cancel_*` `wake()`s a parked op). Full rules
  + gotchas: DETAILS § "Pause / resume".
- **Stop-mode conflict resolution must store the oneshot sender BEFORE emitting `write-conflict`** (both local-FS and
  volume branches). Emit-first races the take and hangs the recv.
- **The conflict-dispatch mutex serializes the one human across concurrent/nested merges**; NEVER hold across the file
  write. Sequence (check `is_cancelled`, re-check latch, emit + await, store latch, release): DETAILS.
- **`write-settled` fires exactly once per op, AFTER the terminal event**, via a `WriteSettledGuard` whose `Drop` runs at
  end of the spawn-task scope (panic-safe). Cache cleanup (via `manager::on_settled` or the `ManagedTaskGuard` Drop)
  runs first. The FE gates the "Cancelling…" dialog close on this.
- **Every write-op driver MUST register its destination with the downloads watcher's ignore set BEFORE the syscall**
  (`crate::downloads::note_pending_write_for_cmdr`; renames register BOTH halves). Scoping is inside the helper — no
  `path.starts_with(downloads_dir)` guards at call sites.
- **Safe overwrite is temp + rename-aside + rename** (original intact until the new content is fully in place); temp
  files use the `.cmdr-` prefix (crash-recoverable).
- **Symlinks are never dereferenced** (`symlink_metadata`; loop detection via canonicalized-path `HashSet`).
- **On macOS never use `statvfs` alone for disk-space checks**; use the NSURL purgeable-aware
  `crate::volumes::get_volume_space()` (`statvfs` only on Linux). `statvfs` rejects copies APFS purgeable space allows.
- **Every scan reports two byte totals**: `total_bytes` (write footprint, used by copy/move) and `dedup_bytes`
  (`du`-equivalent, used by delete). Don't "fix" copy to the dedup'd number — it under-reserves disk space.
- **All write ops emit via `OperationEventSink`, not `tauri::AppHandle`** (`emit_progress_via_sink`, the only
  progress-emit path, calls `enrich_progress`). Makes the pipelines testable without a Tauri runtime.
- **The busy-volumes set disables Eject mid-op** (source AND destination volume IDs). The `eject_volume` server-side
  guard is the real safety net; the picker disable is only UX.
- **Volume-aware ops must not emit `write-error` on `Cancelled`** (the inner handler already emitted `write-cancelled`);
  the outer wrapper matches `Cancelled` and skips.

Architecture, flows, and decision detail: [DETAILS.md](DETAILS.md). Read it before any non-trivial work here: editing, planning, reorganizing, or advising.
