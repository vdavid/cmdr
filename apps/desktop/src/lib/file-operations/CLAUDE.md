# File operations

Umbrella over the transfer (copy/move), delete/trash, new-file, and new-folder dialogs, triggered by Shift+F4 (new
file), F5 (copy), F6 (move), F7 (new folder), and F8 / Shift+F8 (trash / delete). Depth: [DETAILS.md](DETAILS.md).

## Module map

- [`transfer/`](transfer/CLAUDE.md): copy + move dialogs, plus `TransferProgressDialog` (reused by delete/trash,
  parameterized by `operationType: 'copy' | 'move' | 'delete' | 'trash'`), error rendering, and shared utilities.
- [`delete/`](delete/CLAUDE.md): F8 / Shift+F8 delete + trash confirmation dialog and pure utilities.
- [`mkdir/`](mkdir/CLAUDE.md): F7 new-folder dialog with AI suggestions.
- [`mkfile/`](mkfile/CLAUDE.md): Shift+F4 new-file dialog.
- `scan-throughput.ts`: rolling-window scan-rate estimator (see below).

## Must-knows

- **`scan-throughput.ts` covers the scan phase only.** The backend `EtaEstimator` covers write phases, so `DeleteDialog`
  and `TransferProgressDialog` use `ScanThroughput` to show `filesPerSecond` / `bytesPerSecond` during the scan. It
  returns nulls until two samples land, clamps negative deltas to zero, and must be `reset()` between scans. Pure, no
  Svelte / Tauri coupling.

Backend counterpart for everything here:
[`src-tauri/src/file_system/write_operations/`](../../../src-tauri/src/file_system/write_operations/CLAUDE.md) (plus its
[`transfer/`](../../../src-tauri/src/file_system/write_operations/transfer/CLAUDE.md) and
[`delete/`](../../../src-tauri/src/file_system/write_operations/delete/CLAUDE.md) subdirs).
