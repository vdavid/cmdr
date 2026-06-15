# File operations

Transfer (copy/move), delete/trash, mkfile, and mkdir dialogs with progress tracking and conflict resolution.

## Purpose

Provides unified UI for file operations triggered by Shift+F4 (new file), F5 (copy), F6 (move), F7 (new folder), and
F8/Shift+F8 (trash/delete). Transfer and delete operations share `TransferProgressDialog`, parameterized by
`operationType: 'copy' | 'move' | 'delete' | 'trash'`.

Backend counterpart for everything in this directory:
[`apps/desktop/src-tauri/src/file_system/write_operations/CLAUDE.md`](../../../src-tauri/src/file_system/write_operations/CLAUDE.md)
(plus its [`transfer/`](../../../src-tauri/src/file_system/write_operations/transfer/CLAUDE.md) and
[`delete/`](../../../src-tauri/src/file_system/write_operations/delete/CLAUDE.md) subdirs).

## Subdirs

- [`transfer/CLAUDE.md`](transfer/CLAUDE.md) — copy + move dialogs, progress dialog (reused by delete/trash), error
  rendering, scan-phase body, direction indicator, and the shared transfer utilities.
- [`delete/CLAUDE.md`](delete/CLAUDE.md) — delete/trash confirmation dialog and pure utilities.
- [`mkdir/CLAUDE.md`](mkdir/CLAUDE.md) — F7 new-folder dialog with AI suggestions.
- [`mkfile/CLAUDE.md`](mkfile/CLAUDE.md) — Shift+F4 new-file dialog.

## Top-level files

- **`scan-throughput.ts`**: `ScanThroughput`, a tiny rolling-window estimator (default 2 s window) that turns scan-event
  tally deltas into `filesPerSecond` / `bytesPerSecond`. Used by `DeleteDialog` and `TransferProgressDialog` to show
  throughput during the scan phase, since `EtaEstimator` (backend) only covers write phases. Returns nulls until two
  samples land, clamps negative deltas to zero, and resets cleanly between scans. Pure module, no Svelte/Tauri coupling.
- **`scan-throughput.test.ts`**: Vitest tests for the estimator.

Full details: [DETAILS.md](DETAILS.md).
