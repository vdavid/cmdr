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
  dialog is running, and the failure its error dialog is showing — so ambient main-window surfaces stay quiet about
  both.

## Must-knows

- **Dialog copy lives in the i18n catalog, not in the components.** Every user-facing string in the copy/move, delete,
  new-file, and new-folder dialogs resolves from `messages/en/fileOperations.json` via `t()`/`tString()`/`<Trans>`
  (`$lib/intl`); hardcoding one fails `cmdr/no-raw-user-facing-string` on `transfer/`, `delete/`, `mkdir/`, `mkfile/`.
  The transfer ERROR-MESSAGE prose (`transfer-error-messages.ts`) belongs to the `lib/error-messages` pipeline instead:
  the `errors.write.*` catalog via `getMessage()` (RAW lookup, no ICU — write apostrophes normally), NOT ICU `t()`,
  because the .ts composes interpolated paths and sizes (`escapeHtml`, `colorizeSizeString`) into markup. Verb-dependent
  messages use per-operation variant keys (`errors.write.<field>.<copy|move|delete|trash>`) selected by `operationType`,
  never a slotted verb token (an i18n anti-pattern), so each locale phrases each operation naturally. en output is
  parity-pinned (`file-operations-i18n-parity.test.ts`, `transfer/transfer-error-messages.parity.test.ts`): a copy edit
  lands in the catalog AND the test together. See [`$lib/intl/messages/CLAUDE.md`](../intl/messages/CLAUDE.md).
- **One dual-bar readout, two surfaces.** The progress dialog and the operation queue's rows both render
  `TransferProgressReadout.svelte`, so what a running operation looks like is defined once. Its readout cells are
  fixed-width by design (the bars must follow the window, not the digits), which puts a floor under whatever hosts it:
  the queue window's `MIN_WIDTH` and the dialog's 580 px both exist for it. Depth: `DETAILS.md`.
- **The foreground slot is released on EVERY route out of the dialog**, including the Queue button and the auto-queue
  path — that handoff is precisely when the ambient surfaces must start speaking about the operation. Release with
  `clearForegroundOperation(id)`, never an unconditional `setForegroundOperationId(null)`: a late teardown would
  otherwise silence the next dialog's operation. DETAILS § Foreground-operation slot.
- **An error dialog is a HANDOVER, not a release.** The progress dialog drops its slot as it unmounts, and the retained
  failure row reaches the snapshot only afterwards, so `handleTransferError` passes the id to `setForegroundFailureId`
  while it still can; closing the dialog releases it and dismisses the retained failure. Skip that, and the corner chip
  and the failure toast both announce what the user is already reading. Why:
  `apps/desktop/src/lib/status-corner/DETAILS.md`.
- **`scan-throughput.ts` covers the scan phase only.** The backend `EtaEstimator` covers write phases, so `DeleteDialog`
  and `TransferProgressDialog` use `ScanThroughput` to show `filesPerSecond` / `bytesPerSecond` during the scan. It
  returns nulls until two samples land, clamps negative deltas to zero, and must be `reset()` between scans. Pure, no
  Svelte / Tauri coupling.

Backend counterpart for everything here: `apps/desktop/src-tauri/src/file_system/write_operations/CLAUDE.md` (plus its
`apps/desktop/src-tauri/src/file_system/write_operations/transfer/CLAUDE.md` and
`apps/desktop/src-tauri/src/file_system/write_operations/delete/CLAUDE.md` subdirs).
