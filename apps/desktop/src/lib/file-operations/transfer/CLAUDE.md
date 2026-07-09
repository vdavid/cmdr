# Transfer (copy and move)

Frontend for copy (F5), move (F6), and compress (⌥F5): destination picker, dry-run conflict scan, dual-bar progress
dialog, error rendering. Parameterized by `operationType: 'copy' | 'move' | 'compress'` so one set serves all; the
progress dialog is reused by delete/trash too (`'delete' | 'trash'`).

Backend:
[`src-tauri/src/file_system/write_operations/transfer/CLAUDE.md`](../../../../src-tauri/src/file_system/write_operations/transfer/CLAUDE.md)
(copy/move semantics) and
[`write_operations/CLAUDE.md`](../../../../src-tauri/src/file_system/write_operations/CLAUDE.md) (shared state machine,
ETA/throughput, settle contract).

## Module map

- `TransferDialog.svelte`: thin shell over `transfer-scan-state.svelte.ts` (deep scan preview),
  `transfer-conflict-check.svelte.ts` (cheap top-level conflict check), and `transfer-dialog-logic.ts` (pure helpers).
- `TransferProgressDialog.svelte`: execution shell over `transfer-progress-state.svelte.ts`
  (`createTransferProgressState`: the headless event/phase/cancel/pause/queue/conflict/scan-wait state machine) +
  `TransferConflictDialog.svelte` (conflict-resolution UI).
- `TransferErrorDialog.svelte` + `FallbackErrorContent`: error display from the typed `WriteOperationError`. Plus
  `ArchivePasswordDialog`, `ScanPhaseBody`, `DirectionIndicator`, and the `transfer-*.ts` helpers.

## Must-knows

- **One transfer entry seam.** F5/F6, drag-and-drop, and paste all prepare through `pane/transfer-entry.ts`
  (`checkTransferDestinationGuard` + `resolveSourceVolumeId`). The destination-guard copy is the E2E-asserted contract:
  don't reword it. `resolveSourceVolumeId` NEVER returns a knowingly-wrong id (falls back to `root`). The paste path's
  MTP refusal stays SEPARATE and BEFORE the shared guard.
- **Always use batch IPC for selection lookups** (`get_paths_at_indices` / `get_files_at_indices`, one call). Never loop
  `getFileAt` per-index: with 50k files that's 5-10 s vs ~1 ms.
- **Rollback is DISABLED for same-volume moves** (`isSameVolumeMove`: source and dest the SAME non-default volume; the
  backend rename-merges server-side with no rollback). Both Rollback affordances disable (tooltip "Rollback is not
  available for same-volume moves"); plain Cancel stays reachable. `DEFAULT_VOLUME_ID` is excluded (local→local keeps a
  live Rollback).
- **Same-volume move skips the deep scan preview** (zero-byte server-side rename; the scan only feeds the Size bar). The
  `DEFAULT_VOLUME_ID` exclusion is load-bearing (local→local keeps a live scan). The cheap conflict check stays
  decoupled and running so merge info + policy radios still appear. DETAILS § "Same-volume move skips the deep scan
  preview".
- **Rollback / Cancel buttons disable during the settle window.** `TransferProgressDialog` holds open for
  `MIN_DISPLAY_MS = 400 ms` after `write-complete`; a click then hits an already-removed op and falsely flashes "Rolling
  back...". Gate on `disabled={isCancelling || operationSettled}`.
- **Cancel close waits for both `write-cancelled` AND `write-settled`** for the `operationId` before closing. Don't
  close on cancel alone: a fast second F8 mid-teardown once wedged an MTP session.
- **The `archive_needs_password` write-error is intercepted UPSTREAM, not rendered by `TransferErrorDialog`.**
  `handleTransferError` (`pane/dialog-state.svelte.ts`) shows `ArchivePasswordDialog` and on unlock re-dispatches the op
  (`previewId: null`). Don't route it into the generic dialog (its `errorDisplayMetaMap` entry is a fallback). DETAILS §
  "Archive-password prompt".
- **Source pane refresh.** Move refreshes BOTH panes (source files gone); copy only the destination.
- **Flushing phase** (`phase: 'flushing'`): title shows "Writing the last piece..." (exact copy) — the backend's closing
  `fdatasync`, a real multi-second pause on slow media. Don't let the bar sit frozen at 100%.
- **`data-scan-state` marker** (`counting` | `done` | `skipped`) on `.scan-stats` is the race-free "counting done"
  signal E2E polls. Don't remove it.
- **Compress is the third mode** (`operationType: 'compress'`, packs sources into a NEW zip): swaps the conflict-policy
  UI for a dest-exists overwrite check, and its auto-confirm (MCP) path must NEVER silently overwrite an existing target
  — `handleConfirm` keeps the dialog open. DETAILS § "Compress mode".
- **MTP move is interleaved copy + delete per file** (not copy-all-then-delete-all): on partial failure only the current
  file is in both places; Rollback hidden during the delete phase.
- **Progress-dialog Pause/Queue** (full flow in DETAILS). Pause/Resume + the "Paused" title follow the
  `operations-changed` snapshot status, never `is_running`. Queue + the dialog-scoped F2 are FRONTEND-ONLY: set
  `backgrounded`, open the queue window, unmount via `onQueue` without cancelling. `backgrounded` also makes `onDestroy`
  skip its safety-net cancel — don't break that gate.

Architecture, flows, and decisions: [DETAILS.md](DETAILS.md). Read before non-trivial work here.
