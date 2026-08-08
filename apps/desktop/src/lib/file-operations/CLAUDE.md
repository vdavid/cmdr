# File operations

Umbrella over the transfer (copy/move), delete/trash, new-file, and new-folder dialogs, triggered by Shift+F4 (new
file), F5 (copy), F6 (move), F7 (new folder), and F8 / Shift+F8 (trash / delete). Depth: `DETAILS.md`.

## Module map

- `transfer/CLAUDE.md`: copy + move dialogs, plus `TransferProgressDialog` (reused by delete/trash, parameterized by
  `operationType: 'copy' | 'move' | 'delete' | 'trash'`), error rendering, and shared utilities.
- `delete/CLAUDE.md`: F8 / Shift+F8 delete + trash confirmation dialog and pure utilities.
- `mkdir/CLAUDE.md`: F7 new-folder dialog with AI suggestions.
- `mkfile/CLAUDE.md`: Shift+F4 new-file dialog.
- `queue/CLAUDE.md`: the standalone operation-queue window (lists every running/waiting operation with per-row
  pause/resume/cancel, multi-select + Cancel selected, global pause/resume). Renders from the operations store that
  merges the thin `operations-changed` snapshot with the live `write-progress` stream.
- `TransferProgressReadout.svelte`: the dual-bar readout (size + count, each with amount, percent, rate, plus one
  time-left cell) shared by the progress dialog and the queue rows. Two densities, one layout.
- `scan-throughput.ts`: rolling-window scan-rate estimator (see below).
- `foreground-operation.svelte.ts`: two module-scoped slots naming what the foreground owns — the operation its progress
  dialog is running, and the failure its error dialog is showing — plus the claim marking a dispatch whose operation has
  no name yet, so ambient main-window surfaces stay quiet about all three.
- `operation-conflict.svelte.ts` + `OperationConflictDialog.svelte`: the main window's conflict prompt for an operation
  no progress dialog is showing. Its two rules are pure, in `operation-conflict-rules.ts`.

## Must-knows

- **Dialog copy lives in the i18n catalog, not in the components.** Every user-facing string in the copy/move, delete,
  new-file, and new-folder dialogs resolves from `messages/en/fileOperations.json` via `t()`/`tString()`/`<Trans>`
  (`$lib/intl`); hardcoding one fails `cmdr/no-raw-user-facing-string` on `transfer/`, `delete/`, `mkdir/`, `mkfile/`.
  ⚠️ The transfer ERROR prose (`transfer-error-messages.ts`) is NOT ICU: it belongs to the `errors.write.*` pipeline
  (`$lib/error-messages/CLAUDE.md`), with per-operation variant keys selected by `operationType` rather than a slotted
  verb. en output is parity-pinned (`file-operations-i18n-parity.test.ts`,
  `transfer/transfer-error-messages.parity.test.ts`): a copy edit lands in the catalog AND the test together.
- **One dual-bar readout, two surfaces.** The progress dialog and the operation queue's rows both render
  `TransferProgressReadout.svelte`, so what a running operation looks like is defined once. Its readout cells are
  fixed-width by design (the bars must follow the window, not the digits), which puts a floor under whatever hosts it:
  the queue window's `MIN_WIDTH` and the dialog's 580 px both exist for it. Depth: `DETAILS.md`.
- **The foreground slot is released on EVERY route out of the dialog**, Queue and auto-queue included — that handoff is
  exactly when ambient surfaces must start speaking about the operation. Release with `clearForegroundOperation(id)`, ❌
  never an unconditional `setForegroundOperationId(null)`: a late teardown would silence the next dialog's operation.
- **An error dialog is a HANDOVER, not a release.** The progress dialog drops its slot as it unmounts and the retained
  failure row lands only after, so `handleTransferError` passes the id to `setForegroundFailureId` while it still can;
  closing the dialog releases it and dismisses the failure. Skip it and the chip and the toast both announce what the
  user is already reading.
- **A conflict for an operation no dialog owns is answered on the MAIN window** (`operation-conflict.svelte.ts`): pause
  what's running, prompt with the same `TransferConflictDialog`, resume exactly the ids paused. ❌ Never `resumeAll()`
  (it restarts a pause the USER made); ❌ never decide ownership while `isForegroundClaimPending()` — defer, or you
  double-prompt or re-wedge the operation. DETAILS § Conflict prompts.
- **`ScanThroughput` is SCAN-phase only** (the backend `EtaEstimator` owns every write phase), returns nulls until two
  samples land, and must be `reset()` between scans. Pure, no Svelte / Tauri coupling. DETAILS § `scan-throughput.ts`.

Backend counterpart for everything here: `apps/desktop/src-tauri/src/file_system/write_operations/CLAUDE.md` (plus its
`apps/desktop/src-tauri/src/file_system/write_operations/transfer/CLAUDE.md` and
`apps/desktop/src-tauri/src/file_system/write_operations/delete/CLAUDE.md` subdirs).
