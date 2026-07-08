# Transfer (copy and move)

Frontend for copy (F5) and move (F6): destination picker, dry-run conflict scan, dual-bar progress dialog, error
rendering. Parameterized by `operationType: 'copy' | 'move'` so one component set serves both; the progress dialog is
reused by delete/trash too (`'delete' | 'trash'`).

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
- **Always use batch IPC for selection lookups.** `get_paths_at_indices` / `get_files_at_indices` fetch all selected
  items in one call. Never loop `getFileAt` per-index: with 50k files that's 5-10 s vs ~1 ms.
- **Rollback is DISABLED for same-volume moves** (`isSameVolumeMove`: move where source and dest are the SAME
  non-default volume; the backend does a server-side rename-merge with no rollback). Both Rollback affordances render
  disabled (tooltip "Rollback is not available for same-volume moves"); plain Cancel stays reachable. Local→local
  same-FS moves keep a live Rollback, so `DEFAULT_VOLUME_ID` is excluded.
- **Same-volume move skips the deep scan preview** (zero-byte server-side rename; the scan only feeds the Size bar). The
  `DEFAULT_VOLUME_ID` exclusion is load-bearing (local→local keeps a live scan). The cheap conflict check stays
  decoupled and keeps running so merge info + policy radios still appear. DETAILS § "Same-volume move skips the deep
  scan preview".
- **Rollback / Cancel buttons disable during the settle window.** `TransferProgressDialog` holds open for
  `MIN_DISPLAY_MS = 400 ms` after `write-complete`; a click then hits an already-removed op and falsely flashes "Rolling
  back...". Gate on `disabled={isCancelling || operationSettled}`.
- **Cancel close waits for both `write-cancelled` AND `write-settled`** for the `operationId` before closing. Don't
  close on cancel alone: a fast second F8 mid-teardown once wedged an MTP USB session.
- **The `archive_needs_password` write-error is intercepted UPSTREAM, not rendered by `TransferErrorDialog`.**
  `handleTransferError` (`pane/dialog-state.svelte.ts`) shows `ArchivePasswordDialog`, keeps `transferProgressProps`
  alive, and on unlock re-dispatches the same op (`previewId: null`); on cancel it forgets the password and settles.
  Don't route it into the generic dialog (its `errorDisplayMetaMap` entry is a fallback only). DETAILS §
  "Archive-password prompt".
- **Source pane refresh.** Move refreshes BOTH panes post-completion (source files gone); copy only refreshes
  destination.
- **Flushing phase** (`phase: 'flushing'`): title shows "Writing the last piece..." (exact copy). It's the backend's
  closing `fdatasync`, a real multi-second pause on slow media. Don't let the bar sit frozen at 100%.
- **`data-scan-state` marker** (`counting` | `done` | `skipped`) on `.scan-stats` is the race-free "counting done"
  signal E2E polls. Don't remove it.
- **MTP move is interleaved copy + delete per file** (not copy-all-then-delete-all): on partial failure only the current
  file is in both places. Rollback hidden during the delete phase.
- **Progress-dialog Pause/Queue** (full flow + auto-queue in DETAILS). Pause/Resume + the "Paused" title follow the
  `operations-changed` snapshot status, never `is_running`. Queue + the dialog-scoped F2 are FRONTEND-ONLY: set
  `backgrounded`, open the queue window, unmount via `onQueue` WITHOUT cancelling. `backgrounded` also makes `onDestroy`
  skip its safety-net cancel — don't break that gate or a backgrounded op dies on unmount.

Architecture, flows, and decisions: [DETAILS.md](DETAILS.md). Read before non-trivial work here.
