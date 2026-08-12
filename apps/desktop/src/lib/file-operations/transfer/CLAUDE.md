# Transfer (copy and move)

Frontend for copy (F5), move (F6), and compress (⌥F5): destination picker, dry-run conflict scan, dual-bar progress
dialog, error rendering. One set serves all via `operationType`; delete/trash reuse the progress dialog. Backend
counterpart: `apps/desktop/src-tauri/src/file_system/write_operations/CLAUDE.md` (state machine, ETA, settle contract)
and its `transfer/` subdir (copy/move semantics).

## Module map

- `TransferDialog.svelte`: shell over `transfer-scan-state.svelte.ts` (deep scan preview),
  `transfer-conflict-check.svelte.ts` (cheap top-level check), and `transfer-dialog-logic.ts` (pure helpers).
- `TransferProgressDialog.svelte`: execution shell over `transfer-progress-state.svelte.ts` (the headless
  event/phase/cancel/pause/queue/conflict machine), plus `TransferConflictDialog.svelte`, `transfer-stall.ts`.
- `TransferErrorDialog.svelte` + `FallbackErrorContent` for the typed `WriteOperationError`, and the rest:
  `ArchivePasswordDialog`, `ScanPhaseBody`, `DirectionIndicator`, the `transfer-*.ts` helpers.

## Must-knows

- **One transfer entry seam.** F5/F6, drag-and-drop, and paste all prepare through `pane/transfer-entry.ts`. The
  destination-guard copy is an E2E-asserted contract, so don't reword it, and the paste path's MTP refusal stays
  SEPARATE and BEFORE the shared guard.
- **Batch IPC for selection lookups** (`get_paths_at_indices` / `get_files_at_indices`), ❌ never a per-index
  `getFileAt` loop: 50k files is 5-10 s vs ~1 ms.
- **Same-volume move disables Rollback and skips the deep scan preview** (the backend rename-merges server-side). ⚠️ A
  CROSS-volume move can't roll back either, and this dialog doesn't know yet. DETAILS § Gotchas.
- **Speed, ETA, and the bars are backend-owned, SHARED with the queue window** (`../progress-readout.ts`,
  `../TransferProgressReadout.svelte`). ❌ No second instantaneous rate here; `ScanThroughput` is SCAN-phase only. Its
  fixed-width columns are why this dialog is 580 px wide; don't narrow it without them.
- **A stall drops the ETA and says why** (`transfer-stall.ts`). The BACKEND classifies; this side owns the threshold. ❌
  Never infer a stall from event timing: a wedge emits no events at all.
- **Rollback / Cancel disable during the settle window** (`disabled={isCancelling || operationSettled}`): a click in the
  400 ms hold-open hits an already-removed op and falsely flashes "Rolling back...".
- **Cancel close waits for both `write-cancelled` AND `write-settled`** (a fast second F8 mid-teardown once wedged an
  MTP session), ❌ but never as the ONLY exit: `progress.dismiss()` backs a Close button that leaves at once.
- **`archive_needs_password` is intercepted UPSTREAM** by `handleTransferError` (`pane/dialog-state.svelte.ts`).
- **Move refreshes BOTH panes** (source files gone); copy only the destination.
- **Confirm waits on the conflict check ONLY under `skip`**: elsewhere the backend ignores `pre_known_conflicts`, so
  `conflicts: []` costs information, not safety. ❌ Don't drop `handleCancel`'s `confirmed` guard: it's also `onclose`,
  and would free the preview under a pending dispatch.
- **The progress dialog does NOT wait for the scan; the BACKEND does**
  (`apps/desktop/src-tauri/src/file_system/write_operations/scan_bridge.rs`). It dispatches on mount, so a
  still-counting transfer has an `operationId`, a queue row, and Background from frame one. ❌ Never cancel the preview
  on teardown — the operation owns it. Confirm ALWAYS awaits `scan.scanStarted` (and `DeleteDialog` its own): a null
  `previewId` means a concurrent re-walk plus an orphaned preview. DETAILS § Scan.
- **`data-scan-state` on `.scan-stats`** is E2E's only race-free "counting done" signal; `DeleteDialog` mirrors it.
- **Compress swaps the conflict-policy UI for a dest-exists overwrite check**; its auto-confirm (MCP) path must NEVER
  silently overwrite.
- **Pause/Resume and the "Paused" title follow the `operations-changed` snapshot status, ❌ never `is_running`.** Queue
  and the dialog-scoped F2 are FRONTEND-ONLY: they set `backgrounded` (so `onDestroy` skips its safety-net cancel), open
  the queue window, and unmount via `onQueue` without cancelling.

Flows, the phase catalog (`flushing`, MTP's interleaved move), decisions, and gotchas: `DETAILS.md`. Read it before any
non-trivial work here: editing, planning, reorganizing, or advising.
