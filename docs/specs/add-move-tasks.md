# Add move feature — task list

Implementation checklist for [add-move.md](./add-move.md).

## Milestone 1: Utilities refactor

- [ ] Rename `copy-dialog-utils.ts` → `write-operation-utils.ts`
- [ ] Update `generateTitle()` to accept `operationType: 'copy' | 'move'` parameter
- [ ] Update all imports in `CopyDialog.svelte` and `CopyProgressDialog.svelte`
- [ ] Rename `copy-dialog-utils.test.ts` → `write-operation-utils.test.ts`
- [ ] Add unit tests for move title generation (`'Move 3 files'`, `'Move 1 file and 2 folders'`)
- [ ] Run `pnpm vitest run -t "generateTitle"` — must pass

## Milestone 2: Unified dialog component

- [ ] Create `WriteOperationDialog.svelte` based on `CopyDialog.svelte`
- [ ] Add `operationType: 'copy' | 'move'` prop
- [ ] Update title to use `generateTitle(operationType, ...)`
- [ ] Update confirm button text: `{operationType === 'copy' ? 'Copy' : 'Move'}`
- [ ] Update `DualPaneExplorer.svelte` to use `WriteOperationDialog` for copy (keep working)
- [ ] Run `./scripts/check.sh --check svelte-check` — must pass
- [ ] Manual test: F5 still opens copy dialog and works end-to-end

## Milestone 3: Unified progress dialog component

- [ ] Create `WriteOperationProgressDialog.svelte` based on `CopyProgressDialog.svelte`
- [ ] Add `operationType: 'copy' | 'move'` prop
- [ ] Update title: `{operationType === 'copy' ? 'Copying...' : 'Moving...'}`
- [ ] Update backend call to use `operationType === 'copy' ? copyFiles : moveFiles`
- [ ] Add `hasSeenDeletingPhase` state to track cross-FS moves
- [ ] Update stage indicator to show "Cleaning up" stage when `deleting` phase received
- [ ] Update `DualPaneExplorer.svelte` to use `WriteOperationProgressDialog` for copy
- [ ] Run `./scripts/check.sh --check svelte-check` — must pass
- [ ] Manual test: F5 copy still works with progress dialog

## Milestone 4: DualPaneExplorer refactor

- [ ] Rename state: `showCopyDialog` → `showWriteDialog`
- [ ] Rename state: `copyDialogProps` → `writeDialogProps`
- [ ] Rename state: `showCopyProgressDialog` → `showWriteProgressDialog`
- [ ] Rename state: `copyProgressProps` → `writeProgressProps`
- [ ] Add `operationType` field to `WriteDialogPropsData` type
- [ ] Add `operationType` field to `WriteProgressPropsData` type
- [ ] Rename `openCopyDialog()` → `openWriteOperationDialog(operationType)`
- [ ] Rename `buildCopyPropsFromSelection()` → `buildWritePropsFromSelection()`
- [ ] Rename `buildCopyPropsFromCursor()` → `buildWritePropsFromCursor()`
- [ ] Rename handlers: `handleCopyConfirm` → `handleWriteConfirm`, etc.
- [ ] Keep `openCopyDialog()` as wrapper calling `openWriteOperationDialog('copy')`
- [ ] Run `./scripts/check.sh --check svelte-check` — must pass
- [ ] Manual test: F5 copy still works after refactor

## Milestone 5: Add move support

- [ ] Add F6 keyboard handler calling `openWriteOperationDialog('move')`
- [ ] Add `openMoveDialog()` to `ExplorerAPI` interface
- [ ] Implement source pane refresh in `handleWriteComplete()` for move operations
- [ ] Update log messages to use dynamic verb ("Copy complete" / "Move complete")
- [ ] Run `./scripts/check.sh --check svelte-check` — must pass
- [ ] Manual test: F6 opens move dialog
- [ ] Manual test: Move dialog shows "Move X files" title
- [ ] Manual test: Move button labeled "Move"
- [ ] Manual test: Same-FS move completes instantly
- [ ] Manual test: Source pane refreshes (moved files disappear)
- [ ] Manual test: Destination pane refreshes (moved files appear)

## Milestone 6: Function key bar

- [ ] Locate function key bar component (`FunctionKeyBar.svelte` or similar)
- [ ] Add F6 Move entry alongside F5 Copy
- [ ] Wire F6 action to `explorerApi?.openMoveDialog()`
- [ ] Manual test: Function key bar shows F6 Move
- [ ] Manual test: Clicking F6 bar entry opens move dialog

## Milestone 7: Cleanup

- [ ] Delete `CopyDialog.svelte`
- [ ] Delete `CopyProgressDialog.svelte`
- [ ] Delete `copy-dialog-utils.ts`
- [ ] Remove any unused imports
- [ ] Run `./scripts/check.sh --check knip` — no dead code warnings
- [ ] Run `./scripts/check.sh --check desktop-svelte-eslint` — must pass
- [ ] Run `./scripts/check.sh --check desktop-svelte-prettier` — must pass

## Milestone 8: Unit tests

- [ ] Update `write-operation-utils.test.ts` with full coverage for both operations
- [ ] Add test: `generateTitle('move', 1, 0)` returns `'Move 1 file'`
- [ ] Add test: `generateTitle('move', 0, 1)` returns `'Move 1 folder'`
- [ ] Add test: `generateTitle('move', 2, 3)` returns `'Move 2 files and 3 folders'`
- [ ] Add test: `generateTitle('move', 0, 0)` returns `'Move'`
- [ ] Run `./scripts/check.sh --check svelte-tests` — must pass

## Milestone 9: E2E smoke tests

- [ ] Add test: F6 opens move dialog
- [ ] Add test: Move dialog title shows "Move" not "Copy"
- [ ] Add test: Move dialog confirm button says "Move"
- [ ] Add test: ESC closes move dialog
- [ ] Run `./scripts/check.sh --check desktop-e2e` — must pass

## Milestone 10: E2E Linux tests (optional but recommended)

- [ ] Add test: Move file to other pane, verify source removed
- [ ] Add test: Move folder recursively, verify contents moved
- [ ] Add test: Cancel cross-FS move, verify source intact
- [ ] Add test: Move with conflict, test overwrite resolution
- [ ] Run `./scripts/check.sh --check desktop-e2e-linux` — must pass

## Milestone 11: Final verification

- [ ] Run `./scripts/check.sh --svelte` — all Svelte checks pass
- [ ] Run full `./scripts/check.sh` — CI would pass
- [ ] Manual test: Complete copy workflow still works (regression)
- [ ] Manual test: Complete move workflow works (new feature)
- [ ] Manual test: Cross-filesystem move shows all three progress stages
- [ ] Manual test: Conflict resolution works for move
- [ ] Code review: No copy-specific naming left in unified components

## Definition of done

All checkboxes complete AND:

1. F5 Copy works exactly as before (no regression)
2. F6 Move works with same UX quality as Copy
3. Same-FS move is instant (no lag)
4. Cross-FS move shows proper progress
5. Both panes refresh appropriately after move
6. All CI checks pass
