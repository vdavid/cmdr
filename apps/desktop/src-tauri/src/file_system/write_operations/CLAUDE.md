# Write operations

Copy, move, delete, and trash with streaming progress, cancellation, conflict resolution, and rollback. macOS and
Linux. This file documents the cross-cutting machinery both subdirs share: the `OperationIntent` state machine, the
`WriteOperationState` cache, the `OperationEventSink` trait, scan + scan-preview, the `EtaEstimator`, and the settle
contract.

## Module map

- Subdirs: [`transfer/`](transfer/CLAUDE.md) (copy + move, conflict resolution, transfer driver, platform copy
  backends), [`delete/`](delete/CLAUDE.md) (delete walker, trash, oracle-aware fast path).
- Top level: `mod.rs` (public API + `start_write_operation` spawn lifecycle), `types.rs` (DTOs; re-exports the behavior
  items so `types::…` paths stay valid), `event_sinks.rs` (`OperationEventSink`, sinks, builders),
  `analytics.rs` (analytics), `error_classification.rs` (`classify_io_error`, `IoResultExt`), `state.rs` (the
  two caches, `WriteOperationState`, `CopyTransaction`, settle guard), `validation.rs`, `conflict.rs`, `overwrite.rs`,
  `durability.rs`, `cancellable.rs`, `scan.rs`, `scan_preview.rs`, `eta.rs`. The behavior modules depend on `types`, not
  the reverse.
- Frontend counterpart: [`src/lib/file-operations/CLAUDE.md`](../../../../src/lib/file-operations/CLAUDE.md) plus
  colocated child docs.

## Must-knows (invariants and guardrails)

- **All blocking work runs in `spawn_blocking`** (including validation), never on the async executor. The
  `*_files_start` functions return an `operationId` immediately so the dialog can open and offer cancel even if a mount
  is stalled.
- **`OperationIntent` is a single `AtomicU8` state machine** (`Running → RollingBack/Stopped`, `Stopped` terminal).
  Drive it through the public interface in tests, never `state.intent.store(...)` directly (that bypasses the
  validation guard). Cancel keeps fully-copied files (deletes only the last partial); Rollback deletes all copied
  files in reverse with progress.
- **Stop-mode conflict resolution must store the oneshot sender BEFORE emitting `write-conflict`.** A responder can
  only answer a conflict it has observed; emit-first races the take and hangs the recv. Both the local-FS and volume
  branches order it this way.
- **The conflict-dispatch mutex serializes the one human across concurrent/nested merges.** Under it: check
  `is_cancelled` (bail with `Cancelled`), re-check the apply-to-all latch, emit + await, store latch, release. NEVER
  hold it across the subsequent file write.
- **`write-settled` fires exactly once per op, AFTER the terminal event**, via a `WriteSettledGuard` whose `Drop` runs
  at the end of the spawn-task scope (panic-safe). Cache cleanup from both maps must also survive a panic
  (`OperationStateGuard` for the volume-delete branch, which can't clean up after its `.await`). The FE gates the
  "Cancelling…" dialog close on this.
- **Every write-op driver MUST register its destination with the downloads watcher's ignore set BEFORE the syscall**
  (`crate::downloads::note_pending_write_for_cmdr`). Renames register BOTH halves. Don't add `if
  path.starts_with(downloads_dir)` guards at call sites; the scoping lives inside the helper.
- **Safe overwrite is temp + rename-aside + rename**; the original stays intact until the new content is fully in
  place. Temp files use the `.cmdr-` prefix (crash-recoverable).
- **Symlinks are never dereferenced** (all stat calls use `symlink_metadata`; loop detection via canonicalized-path
  `HashSet`).
- **On macOS never use `statvfs` alone for disk-space checks**; use the NSURL purgeable-aware API
  (`crate::volumes::get_volume_space()`), falling back to `statvfs` only on Linux. `statvfs` rejects copies that would
  actually succeed (APFS purgeable space).
- **Every scan reports two byte totals**: `total_bytes` (write footprint, un-dedup'd) and `dedup_bytes`
  (`du`-equivalent). Delete consumes `dedup_bytes`; copy/move consume `total_bytes`. Don't "fix" copy to show the
  dedup'd number; it under-reserves disk space and stalls the bar on hardlink trees.
- **All write ops emit via `OperationEventSink`, not `tauri::AppHandle` directly** (`emit_progress_via_sink` is the only
  progress-emit path; it calls `enrich_progress` internally). This is what makes the pipelines testable without a Tauri
  runtime.
- **The busy-volumes set disables Eject mid-op**; source AND destination volume IDs go in. The `eject_volume`
  server-side guard is the real safety net (the picker disable is only UX).
- **Volume-aware ops must not emit `write-error` on `Cancelled`** (the inner handler already emitted
  `write-cancelled`); the outer wrapper matches `WriteOperationError::Cancelled` and skips.

Architecture, flows, and decision detail: [DETAILS.md](DETAILS.md). Read it before any non-trivial work here: editing, planning, reorganizing, or advising.
