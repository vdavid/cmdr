# Move feature plan

## Context

The Rust backend already fully supports Move (`move_op.rs`, `move_files` command, `moveFiles()` TS wrapper). The
frontend only has Copy UI — Move (F6) is disabled. Copy and Move share ~95% of their UI and flow, differing only in
labels, which Tauri command to call, and whether the source pane needs refresh after completion. This plan introduces
Move on the frontend by generalizing the existing Copy UI into shared "transfer" components parameterized by operation
type.

## Approach: rename `copy/` → `transfer/`, parameterize by operation type

### 1. Introduce `TransferOperationType`

Add to `apps/desktop/src/lib/file-explorer/types.ts`:

```typescript
export type TransferOperationType = 'copy' | 'move'
```

### 2. Rename and generalize `file-operations/copy/` → `file-operations/transfer/`

| Old file                      | New file                          | Key changes                                                                              |
|-------------------------------|-----------------------------------|------------------------------------------------------------------------------------------|
| `CopyDialog.svelte`           | `TransferDialog.svelte`           | Add `operationType` prop; title says "Copy"/"Move"                                       |
| `CopyProgressDialog.svelte`   | `TransferProgressDialog.svelte`   | Add `operationType` prop; call `moveFiles()` for move; phase label "Moving" vs "Copying" |
| `CopyErrorDialog.svelte`      | `TransferErrorDialog.svelte`      | Add `operationType` prop; pass to error messages                                         |
| `DirectionIndicator.svelte`   | `DirectionIndicator.svelte`       | No changes (already operation-agnostic)                                                  |
| `copy-dialog-utils.ts`        | `transfer-dialog-utils.ts`        | `generateTitle(files, folders)` → `generateTitle(operationType, files, folders)`         |
| `copy-error-messages.ts`      | `transfer-error-messages.ts`      | Parameterize all "copy"/"Copy" strings by operation type                                 |
| `copy-dialog-utils.test.ts`   | `transfer-dialog-utils.test.ts`   | Update test descriptions, add move cases                                                 |
| `copy-error-messages.test.ts` | `transfer-error-messages.test.ts` | Add move-specific message tests                                                          |

In `copy-operations.ts` (→ `transfer-operations.ts`):

- Rename `CopyContext` → `TransferContext`, `CopyDialogPropsData` → `TransferDialogPropsData`
- Add `operationType: TransferOperationType` to `TransferDialogPropsData`
- Rename functions: `buildCopyPropsFromSelection` → `buildTransferPropsFromSelection`, etc.
- `copy-operations.test.ts` → `transfer-operations.test.ts`

### 3. Update `DualPaneExplorer.svelte`

Generalize copy-specific state to transfer state:

```
showCopyDialog          → showTransferDialog
copyDialogProps         → transferDialogProps  (now includes operationType)
showCopyProgressDialog  → showTransferProgressDialog
copyProgressProps       → transferProgressProps (now includes operationType)
showCopyErrorDialog     → showTransferErrorDialog
copyErrorProps          → transferErrorProps
```

Generalize handlers:

- `openCopyDialog()` → `openTransferDialog(operationType)` — called with `'copy'` for F5, `'move'` for F6
- `handleCopyConfirm` → `handleTransferConfirm`
- `handleCopyComplete` → `handleTransferComplete` — for move, refresh **both** panes (source files disappeared)
- `handleCopyCancel`, `handleCopyError`, etc. → rename to `handleTransfer*`

For F6 handler: wire to `openTransferDialog('move')`.

MTP limitation: when either volume is MTP, show alert "Move between MTP devices isn't supported yet" (same pattern as
read-only check). The `moveFiles()` backend command only works with local filesystem paths.

### 4. Update `DialogManager.svelte`

- Rename all copy props to transfer props
- Import `TransferDialog`, `TransferProgressDialog`, `TransferErrorDialog`
- Pass `operationType` through

### 5. Update `FunctionKeyBar.svelte`

- Enable F6 button (remove `disabled`)
- Add `onMove` callback prop
- Wire F6 button to `onMove`

### 6. Update `+page.svelte` (route)

Wire the new `onMove` prop from `FunctionKeyBar` through to `DualPaneExplorer`.

### 7. Key behavioral differences for move

| Aspect               | Copy                                   | Move                   |
|----------------------|----------------------------------------|------------------------|
| Dialog title         | "Copy 3 files"                         | "Move 3 files"         |
| Progress phase label | "Copying..."                           | "Moving..."            |
| Error title          | "Copy failed"                          | "Move failed"          |
| Cancelled title      | "Copy cancelled"                       | "Move cancelled"       |
| Tauri command        | `copyFiles()` / `copyBetweenVolumes()` | `moveFiles()`          |
| After completion     | Refresh dest pane                      | Refresh **both** panes |
| MTP cross-volume     | Supported                              | Not yet (show alert)   |
| F-key                | F5                                     | F6                     |

### 8. Files to modify (full list)

**Rename + modify:**

- `src/lib/file-operations/copy/*` → `src/lib/file-operations/transfer/*` (8 files)
- `src/lib/file-explorer/pane/copy-operations.ts` → `transfer-operations.ts`
- `src/lib/file-explorer/pane/copy-operations.test.ts` → `transfer-operations.test.ts`

**Modify only:**

- `src/lib/file-explorer/pane/DualPaneExplorer.svelte` — generalize copy state/handlers to transfer
- `src/lib/file-explorer/pane/DialogManager.svelte` — update props and imports
- `src/lib/file-explorer/pane/FunctionKeyBar.svelte` — enable F6
- `src/lib/file-explorer/pane/FunctionKeyBar.test.ts` — update test for F6 enabled
- `src/lib/file-explorer/types.ts` — add `TransferOperationType`
- `src/routes/(main)/+page.svelte` — update if it passes copy-specific props
- `coverage-allowlist.json` — update file paths if needed
- `docs/features/write-actions.md` — document move UI support

### 9. Out of scope

- MTP cross-volume move (`moveBetweenVolumes`) — future work
- Delete feature (F8) — separate effort
- Drag-and-drop integration with move — covered by the existing drag-and-drop spec

## Milestones

### Milestone 1: Rename and parameterize

- [x] Add `TransferOperationType` to types
- [x] Rename `file-operations/copy/` → `file-operations/transfer/` and all files within
- [x] Add `operationType` prop to all three dialog components
- [x] Parameterize `generateTitle()` and error messages by operation type
- [x] Rename `copy-operations.ts` → `transfer-operations.ts` with updated interfaces/functions
- [x] Update all imports across the codebase
- [x] Run `./scripts/check.sh --svelte` — everything should pass (no behavioral changes yet)

### Milestone 2: Wire up move

- [x] Enable F6 in `FunctionKeyBar.svelte`, add `onMove` prop
- [x] Generalize DualPaneExplorer state from copy → transfer
- [x] Add F6 key handler → `openTransferDialog('move')`
- [x] In progress dialog: call `moveFiles()` when `operationType === 'move'`
- [x] In completion handler: refresh both panes for move
- [x] Add MTP move guard (show alert when trying to move to/from MTP)
- [x] Update `DialogManager.svelte` with new prop names

### Milestone 3: Tests and docs

- [x] Update renamed test files with move-specific test cases
- [x] Update `FunctionKeyBar.test.ts` for F6 enabled state
- [x] Manual testing via MCP: copy still works, move works for local files
- [x] Update `docs/features/write-actions.md` to document move UI
- [x] Run `./scripts/check.sh --svelte` — all checks pass

## Verification

1. `./scripts/check.sh --svelte` passes (lint, type-check, tests, knip)
2. Manual test via MCP: F5 copy works as before (no regressions)
3. Manual test via MCP: F6 move works — files appear at destination, disappear from source
4. Manual test: F6 with MTP volume shows "not supported yet" alert
5. Manual test: move conflict resolution works (skip, overwrite, rename)
6. Manual test: move cancellation works, source files remain intact
