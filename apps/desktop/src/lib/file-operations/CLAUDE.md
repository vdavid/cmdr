# File operations

Umbrella over the transfer (copy/move), delete/trash, new-file, and new-folder dialogs, triggered by Shift+F4 (new
file), F5 (copy), F6 (move), F7 (new folder), and F8 / Shift+F8 (trash / delete). Depth: [DETAILS.md](DETAILS.md).

## Module map

- [`transfer/`](transfer/CLAUDE.md): copy + move dialogs, plus `TransferProgressDialog` (reused by delete/trash,
  parameterized by `operationType: 'copy' | 'move' | 'delete' | 'trash'`), error rendering, and shared utilities.
- [`delete/`](delete/CLAUDE.md): F8 / Shift+F8 delete + trash confirmation dialog and pure utilities.
- [`mkdir/`](mkdir/CLAUDE.md): F7 new-folder dialog with AI suggestions.
- [`mkfile/`](mkfile/CLAUDE.md): Shift+F4 new-file dialog.
- [`queue/`](queue/CLAUDE.md): the standalone transfer-queue window (lists every running/waiting operation with
  per-row pause/resume/cancel, multi-select + Cancel selected, global pause/resume). Renders from the operations store
  that merges the thin `operations-changed` snapshot with the live `write-progress` stream.
- `scan-throughput.ts`: rolling-window scan-rate estimator (see below).

## Must-knows

- **Dialog copy lives in the i18n catalog, not in the components.** Every user-facing string in the copy/move, delete,
  new-file, and new-folder dialogs (titles, buttons, phase labels, conflict-policy labels, scan-stat nouns, notices)
  resolves from `messages/en/fileOperations.json` via `t()`/`tString()`/`<Trans>` (`$lib/intl`). Don't hardcode copy
  here, enforced by `cmdr/no-raw-user-facing-string` on `transfer/`, `delete/`, `mkdir/`, `mkfile/`. The transfer
  ERROR-MESSAGE prose (`transfer-error-messages.ts`, rendered in `TransferErrorDialog`/`FallbackErrorContent`) belongs
  to the `lib/errors` pipeline, so it resolves from the `errors.write.*` catalog via `getMessage()` (RAW lookup, no ICU
  — write apostrophes normally), NOT through ICU `t()`: the strings carry interpolated paths/sizes (`escapeHtml`,
  `colorizeSizeString`) the .ts composes. Verb-dependent messages use per-operation variant keys
  (`errors.write.<field>.<copy|move|delete|trash>`) selected by `operationType` (NOT a slotted verb token — that was an
  i18n anti-pattern), so each locale phrases each operation naturally. en output is parity-pinned
  (`file-operations-i18n-parity.test.ts` + the count-phrase unit tests for dialog copy;
  `transfer/transfer-error-messages.parity.test.ts` for the write-error copy); a copy edit lands in the catalog AND the
  test together. See [`$lib/intl/messages/CLAUDE.md`](../intl/messages/CLAUDE.md).
- **`scan-throughput.ts` covers the scan phase only.** The backend `EtaEstimator` covers write phases, so `DeleteDialog`
  and `TransferProgressDialog` use `ScanThroughput` to show `filesPerSecond` / `bytesPerSecond` during the scan. It
  returns nulls until two samples land, clamps negative deltas to zero, and must be `reset()` between scans. Pure, no
  Svelte / Tauri coupling.

Backend counterpart for everything here:
[`src-tauri/src/file_system/write_operations/`](../../../src-tauri/src/file_system/write_operations/CLAUDE.md) (plus its
[`transfer/`](../../../src-tauri/src/file_system/write_operations/transfer/CLAUDE.md) and
[`delete/`](../../../src-tauri/src/file_system/write_operations/delete/CLAUDE.md) subdirs).
