# Transfer (copy and move)

Frontend for copy (F5) and move (F6): destination picker, dry-run conflict scan, progress dialog with dual bars, rich
error rendering. Parameterized by `operationType: 'copy' | 'move'` so one set of components serves both; the progress
dialog is also reused by delete/trash (`operationType: 'delete' | 'trash'`).

Backend:
[`src-tauri/src/file_system/write_operations/transfer/CLAUDE.md`](../../../../src-tauri/src/file_system/write_operations/transfer/CLAUDE.md)
(copy/move semantics) and
[`write_operations/CLAUDE.md`](../../../../src-tauri/src/file_system/write_operations/CLAUDE.md) (shared state machine,
ETA/throughput, settle contract).

## Module map

- `TransferDialog.svelte`: destination picker, Copy/Move toggle, dry-run scan, conflict-policy radios. Thin shell over
  `transfer-scan-state.svelte.ts` (deep scan preview) + `transfer-conflict-check.svelte.ts` (cheap top-level conflict
  check) + `transfer-dialog-logic.ts` (pure helpers).
- `TransferProgressDialog.svelte`: execution, dual bars, cancel/rollback, conflict dialog, scan-phase body.
- `TransferErrorDialog.svelte` + `FallbackErrorContent`: error display (renders from the typed `WriteOperationError`).
  `ScanPhaseBody`, `DirectionIndicator`, `transfer-dialog-utils.ts`, `transfer-error-messages.ts`,
  `transfer-complete-toast.ts`.

## Must-knows

- **One transfer entry seam.** F5/F6, drag-and-drop, and paste all prepare through `pane/transfer-entry.ts`
  (`checkTransferDestinationGuard` + `resolveSourceVolumeId`) so they can't drift. The destination-guard copy is the
  E2E-asserted contract: don't reword it. `resolveSourceVolumeId` NEVER returns a knowingly-wrong id (falls back to
  `root`). The paste path's MTP refusal stays SEPARATE and BEFORE the shared guard.
- **Always use batch IPC for selection lookups.** `get_paths_at_indices` / `get_files_at_indices` fetch all selected
  items in one call. Never loop `getFileAt` per-index: with 50k files that's 5-10 s vs ~1 ms.
- **Rollback is DISABLED for same-volume moves** (`isSameVolumeMove`: move where source and dest are the SAME
  non-default volume). The backend does a server-side rename-merge with no rollback. Both Rollback affordances render
  disabled (tooltip "Rollback is not available for same-volume moves"); plain Cancel stays reachable. Local→local
  same-FS moves keep a live Rollback, so `DEFAULT_VOLUME_ID` is excluded.
- **Same-volume move skips the deep scan preview** (zero-byte server-side rename; the scan only feeds the Size bar). The
  `DEFAULT_VOLUME_ID` exclusion is load-bearing and mirrors the guard in `TransferProgressDialog`: cancelling the
  preview for a local→local move zeroes the dialog counters AND forces a backend re-scan (the local move path consumes
  the preview cache via `config.preview_id`). The cheap conflict check stays decoupled and keeps running so merge info +
  policy radios still appear.
- **Rollback / Cancel buttons disable during the settle window.** `TransferProgressDialog` holds open for
  `MIN_DISPLAY_MS = 400 ms` after `write-complete`; clicking Rollback/Cancel then hits an already-removed op and falsely
  flashes "Rolling back...". Gate on `disabled={isCancelling || operationSettled}`.
- **Cancel close waits for both `write-cancelled` AND `write-settled`** for the `operationId` before closing. Don't
  close on cancel alone: the original incident was an MTP delete cancel followed by an immediate second F8 that wedged
  the USB session mid-teardown.
- **Source pane refresh.** Move refreshes BOTH panes post-completion (source files gone); copy only refreshes
  destination.
- **Flushing phase** (`phase: 'flushing'`): title shows "Writing the last piece..." (exact copy). It's the backend's
  closing `fdatasync`, a real multi-second pause on slow media. Don't let the bar sit frozen at 100%.
- **`data-scan-state` marker** (`counting` | `done` | `skipped`) on `.scan-stats` is the race-free "counting done"
  signal E2E polls. Don't remove it.
- **MTP move is interleaved copy + delete per file** (not copy-all-then-delete-all): on partial failure only the current
  file exists in both places. Rollback hidden during the delete phase.
- **Progress-dialog Pause/Queue (full flow + auto-queue in DETAILS).** Pause/Resume + the "Paused" title follow the
  `operations-changed` snapshot status, never `is_running`. Queue + the dialog-scoped F2 are FRONTEND-ONLY: set
  `backgrounded`, open the queue window, unmount via `onQueue` WITHOUT cancelling. `backgrounded` also makes `onDestroy`
  skip its safety-net cancel — don't break that gate or a backgrounded op dies on unmount. F2 is scoped by
  `ModalDialog`'s overlay `stopPropagation` (stays global `file.rename` when closed); negative test pins it.

Architecture, flows, and decision detail: [DETAILS.md](DETAILS.md). Read it before any non-trivial work here: editing,
planning, reorganizing, or advising.
