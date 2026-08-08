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
- `foreground-operation.svelte.ts`: the module-scoped slot naming the operation the foreground progress dialog owns, so
  ambient main-window surfaces can stay quiet about it.

## Must-knows

- **Dialog copy lives in the i18n catalog, not in the components.** Every user-facing string in the copy/move, delete,
  new-file, and new-folder dialogs (titles, buttons, phase labels, conflict-policy labels, scan-stat nouns, notices)
  resolves from `messages/en/fileOperations.json` via `t()`/`tString()`/`<Trans>` (`$lib/intl`). Don't hardcode copy
  here, enforced by `cmdr/no-raw-user-facing-string` on `transfer/`, `delete/`, `mkdir/`, `mkfile/`. The transfer
  ERROR-MESSAGE prose (`transfer-error-messages.ts`, rendered in `TransferErrorDialog`/`FallbackErrorContent`) belongs
  to the `lib/error-messages` pipeline, so it resolves from the `errors.write.*` catalog via `getMessage()` (RAW lookup,
  no ICU — write apostrophes normally), NOT through ICU `t()`: the strings carry interpolated paths/sizes (`escapeHtml`,
  `colorizeSizeString`) the .ts composes. Verb-dependent messages use per-operation variant keys
  (`errors.write.<field>.<copy|move|delete|trash>`) selected by `operationType` (NOT a slotted verb token — that was an
  i18n anti-pattern), so each locale phrases each operation naturally. en output is parity-pinned
  (`file-operations-i18n-parity.test.ts` + the count-phrase unit tests for dialog copy;
  `transfer/transfer-error-messages.parity.test.ts` for the write-error copy); a copy edit lands in the catalog AND the
  test together. See [`$lib/intl/messages/CLAUDE.md`](../intl/messages/CLAUDE.md).
- **One dual-bar readout, two surfaces.** The progress dialog and the operation queue's rows both render
  `TransferProgressReadout.svelte`, so what a running operation looks like is defined once. Its readout cells are
  fixed-width by design (the bars must follow the window, not the digits), which puts a floor under whatever hosts it:
  the queue window's `MIN_WIDTH` and the dialog's 580 px both exist for it. Depth: `DETAILS.md`.
- **The foreground slot is released on EVERY route out of the dialog**, including the Queue button and the auto-queue
  path — that handoff is precisely when the ambient surfaces must start speaking about the operation. Release with
  `clearForegroundOperation(id)`, never an unconditional `setForegroundOperationId(null)`: a late teardown would
  otherwise silence the next dialog's operation. DETAILS § Foreground-operation slot.
- **`scan-throughput.ts` covers the scan phase only.** The backend `EtaEstimator` covers write phases, so `DeleteDialog`
  and `TransferProgressDialog` use `ScanThroughput` to show `filesPerSecond` / `bytesPerSecond` during the scan. It
  returns nulls until two samples land, clamps negative deltas to zero, and must be `reset()` between scans. Pure, no
  Svelte / Tauri coupling.

Backend counterpart for everything here: `apps/desktop/src-tauri/src/file_system/write_operations/CLAUDE.md` (plus its
`apps/desktop/src-tauri/src/file_system/write_operations/transfer/CLAUDE.md` and
`apps/desktop/src-tauri/src/file_system/write_operations/delete/CLAUDE.md` subdirs).
