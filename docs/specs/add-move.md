# Add move feature to frontend

This spec describes adding the "move" file operation to the Cmdr frontend. The backend is already complete
(`moveFiles()` Tauri command in `move_op.rs`). The goal is to reuse the copy UI architecture while providing an
excellent UX for both operations.

## Current state

### Backend (complete)

The `moveFiles()` command is fully implemented in `apps/desktop/src-tauri/src/file_system/write_operations/move_op.rs`:

- **Same filesystem**: Uses atomic `fs::rename()` — instant, no progress events needed
- **Cross filesystem**: Uses staging pattern:
  1. Scan sources (emits `scanning` phase)
  2. Copy to `.cmdr-staging-{operation_id}` (emits `copying` phase)
  3. Atomic rename from staging to final destination
  4. Delete source files (emits `deleting` phase)
  5. Remove staging directory

The command is exposed in `tauri-commands.ts`:

```typescript
export async function moveFiles(
    sources: string[],
    destination: string,
    config?: WriteOperationConfig,
): Promise<WriteOperationStartResult>
```

### Frontend copy (complete, to be refactored)

The copy feature is fully implemented with these components:

| File | Purpose |
|------|---------|
| `src/lib/write-operations/CopyDialog.svelte` | Destination picker with scan preview |
| `src/lib/write-operations/CopyProgressDialog.svelte` | Progress bar, conflict resolution, cancel/rollback |
| `src/lib/write-operations/DirectionIndicator.svelte` | Visual arrow showing source → destination |
| `src/lib/write-operations/copy-dialog-utils.ts` | Title generation, index conversion utilities |
| `src/lib/file-explorer/DualPaneExplorer.svelte` | Orchestrates dialogs, handles F5, manages state |

## Architecture decision: unified components

Rather than duplicating CopyDialog/CopyProgressDialog for move, we'll create unified components that accept an
`operationType` prop. This maximizes code reuse (95%+) and guarantees consistent UX.

### Rationale

| Aspect | Copy | Move | Shared? |
|--------|------|------|---------|
| Dialog layout | Destination input, volume selector, scan preview | Same | ✅ |
| Progress UI | Progress bar, ETA, speed, conflict resolution | Same | ✅ |
| Event system | `write-progress`, `write-complete`, `write-error`, etc. | Same | ✅ |
| Backend config | `WriteOperationConfig` | Same | ✅ |
| Title/labels | "Copy 3 files" | "Move 3 files" | Parameterize |
| Keyboard shortcut | F5 | F6 | Separate handlers |
| Progress phases | `scanning` → `copying` | Same FS: instant, Cross FS: `scanning` → `copying` → `deleting` | Handle dynamically |

The only meaningful differences are text labels and that move can have a third phase (`deleting`) for cross-filesystem
operations.

## Requirements

### R1: Unified dialog component

Create `WriteOperationDialog.svelte` to replace `CopyDialog.svelte`.

**Props:**

```typescript
interface Props {
    operationType: 'copy' | 'move'
    sourcePaths: string[]
    destinationPath: string
    direction: 'left' | 'right'
    volumes: VolumeInfo[]
    currentVolumeId: string
    fileCount: number
    folderCount: number
    sortColumn: SortColumn
    sortOrder: SortOrder
    onConfirm: (destination: string, volumeId: string, previewId: string | null) => void
    onCancel: () => void
}
```

**Differences from CopyDialog:**

1. Title uses `generateTitle(operationType, files, folders)` — returns "Copy 3 files" or "Move 3 files"
2. Confirm button text: "Copy" or "Move" based on `operationType`
3. All other behavior identical

### R2: Unified progress dialog component

Create `WriteOperationProgressDialog.svelte` to replace `CopyProgressDialog.svelte`.

**Props:**

```typescript
interface Props {
    operationType: 'copy' | 'move'
    sourcePaths: string[]
    sourceFolderPath: string
    destinationPath: string
    direction: 'left' | 'right'
    sortColumn: SortColumn
    sortOrder: SortOrder
    previewId: string | null
    onComplete: (filesProcessed: number, bytesProcessed: number) => void
    onCancelled: (filesProcessed: number) => void
    onError: (error: string) => void
}
```

**Differences from CopyProgressDialog:**

1. Title: "Copying..." or "Moving..." based on `operationType`
2. Backend call: `copyFiles()` or `moveFiles()` based on `operationType`
3. Stage indicator handles three phases for cross-FS move:
   - `scanning` → "Scanning"
   - `copying` → "Copying" (both copy and move use this label during data transfer)
   - `deleting` → "Cleaning up" (move only, cross-FS)
4. Same-FS move: dialog may close instantly if no progress events are emitted (operation completes immediately)

**Stage handling logic:**

```typescript
// Dynamically determine stages based on operation type and observed phases
const stages = $derived.by(() => {
    const base = [
        { id: 'scanning', label: 'Scanning' },
        { id: 'copying', label: 'Copying' },
    ]
    // Add deleting stage only for move operations that go cross-filesystem
    // We know this if we receive a 'deleting' phase event
    if (operationType === 'move' && hasSeenDeletingPhase) {
        base.push({ id: 'deleting', label: 'Cleaning up' })
    }
    return base
})
```

### R3: Refactor utilities

Rename `copy-dialog-utils.ts` to `write-operation-utils.ts` and update `generateTitle()`:

```typescript
export function generateTitle(
    operationType: 'copy' | 'move',
    files: number,
    folders: number,
): string {
    const verb = operationType === 'copy' ? 'Copy' : 'Move'
    const parts: string[] = []
    if (files > 0) {
        parts.push(`${String(files)} ${files === 1 ? 'file' : 'files'}`)
    }
    if (folders > 0) {
        parts.push(`${String(folders)} ${folders === 1 ? 'folder' : 'folders'}`)
    }
    if (parts.length === 0) {
        return verb
    }
    return `${verb} ${parts.join(' and ')}`
}
```

### R4: DualPaneExplorer integration

Update `DualPaneExplorer.svelte` to:

1. Add F6 keyboard handler for move
2. Refactor state to use unified dialog components
3. Keep backward compatibility during transition

**State changes:**

```typescript
// Before (copy-specific)
let showCopyDialog = $state(false)
let copyDialogProps = $state<CopyDialogPropsData | null>(null)
let showCopyProgressDialog = $state(false)
let copyProgressProps = $state<CopyProgressPropsData | null>(null)

// After (unified)
let showWriteDialog = $state(false)
let writeDialogProps = $state<WriteDialogPropsData | null>(null)
let showWriteProgressDialog = $state(false)
let writeProgressProps = $state<WriteProgressPropsData | null>(null)

// Props now include operationType
type WriteDialogPropsData = {
    operationType: 'copy' | 'move'
    sourcePaths: string[]
    destinationPath: string
    direction: 'left' | 'right'
    currentVolumeId: string
    fileCount: number
    folderCount: number
    sourceFolderPath: string
    sortColumn: SortColumn
    sortOrder: SortOrder
}
```

**Keyboard handler:**

```typescript
'F5': () => void openWriteOperationDialog('copy'),
'F6': () => void openWriteOperationDialog('move'),
```

**Unified opener:**

```typescript
export async function openWriteOperationDialog(operationType: 'copy' | 'move') {
    // Same logic as current openCopyDialog(), but sets operationType
    const props = hasSelection
        ? await buildWritePropsFromSelection(listingId, selectedIndices, hasParent, isLeft, operationType)
        : await buildWritePropsFromCursor(listingId, sourcePaneRef, hasParent, isLeft, operationType)

    if (props) {
        writeDialogProps = props
        showWriteDialog = true
    }
}
```

### R5: ExplorerAPI extension

Add move to the exported API in `DualPaneExplorer.svelte`:

```typescript
export interface ExplorerAPI {
    openCopyDialog(): Promise<void>   // Keep for backward compat, calls openWriteOperationDialog('copy')
    openMoveDialog(): Promise<void>   // New: calls openWriteOperationDialog('move')
    // ... other methods
}
```

### R6: Function key bar integration

If the function key bar exists, add F6 entry for move alongside F5 for copy.

Check `FunctionKeyBar.svelte` or equivalent component and add:

```typescript
{ key: 'F6', label: 'Move', action: () => explorerApi?.openMoveDialog() }
```

## UX considerations

### Same-filesystem move is instant

When moving files within the same filesystem, `fs::rename()` is atomic and instant. The backend emits
`write-complete` immediately without progress events. The frontend should handle this gracefully:

- If the operation completes before the progress dialog can render, skip showing it
- Show a brief success toast instead
- Refresh destination pane to show moved files

**Implementation hint:**

```typescript
async function startOperation() {
    const result = await (operationType === 'copy' ? copyFiles : moveFiles)(
        sourcePaths,
        destinationPath,
        config,
    )
    operationId = result.operationId

    // Subscribe to events
    const unsubscribe = await onOperationEvents(operationId, {
        onProgress: handleProgress,
        onComplete: handleComplete,
        // ...
    })

    // For instant operations (same-FS move), complete event fires immediately
    // The handleComplete callback will close the dialog and refresh panes
}
```

### Rollback semantics differ

- **Copy rollback**: Deletes all files that were copied
- **Move rollback (cross-FS)**: Stops operation, source files remain intact (staging dir is deleted)
- **Move rollback (same-FS)**: N/A — operation is atomic, no partial state possible

The rollback button label can stay "Rollback" for both, but the tooltip could clarify:
- Copy: "Remove copied files"
- Move: "Cancel operation (source files preserved)"

### Log messages

Update completion/cancellation log messages:

```typescript
function handleComplete(filesProcessed: number, bytesProcessed: number) {
    const verb = operationType === 'copy' ? 'Copy' : 'Move'
    log.info(`${verb} complete: ${String(filesProcessed)} files (${formatBytes(bytesProcessed)})`)
    // ...
}
```

### Source pane refresh for move

Unlike copy, move removes files from the source. After a successful move, refresh both panes:

```typescript
function handleMoveComplete(filesProcessed: number, bytesProcessed: number) {
    // Refresh destination to show new files
    const destPaneRef = writeProgressProps?.direction === 'right' ? rightPaneRef : leftPaneRef
    destPaneRef?.refreshView?.()

    // Refresh source to remove moved files
    const sourcePaneRef = writeProgressProps?.direction === 'right' ? leftPaneRef : rightPaneRef
    sourcePaneRef?.refreshView?.()

    // Close dialog, refocus
    showWriteProgressDialog = false
    writeProgressProps = null
    containerElement?.focus()
}
```

## Files to create/modify

| File | Action |
|------|--------|
| `src/lib/write-operations/WriteOperationDialog.svelte` | Create — unified dialog |
| `src/lib/write-operations/WriteOperationProgressDialog.svelte` | Create — unified progress dialog |
| `src/lib/write-operations/write-operation-utils.ts` | Create — rename from `copy-dialog-utils.ts`, update |
| `src/lib/file-explorer/DualPaneExplorer.svelte` | Modify — add F6, refactor to use unified components |
| `src/lib/write-operations/CopyDialog.svelte` | Delete after migration |
| `src/lib/write-operations/CopyProgressDialog.svelte` | Delete after migration |
| `src/lib/write-operations/copy-dialog-utils.ts` | Delete after migration |
| `src/lib/write-operations/DirectionIndicator.svelte` | Keep as-is (already generic) |
| `src/lib/FunctionKeyBar.svelte` | Modify — add F6 Move entry |

## Testing

### Unit tests (Vitest)

Add to existing `copy-dialog-utils.test.ts` (rename to `write-operation-utils.test.ts`):

```typescript
describe('generateTitle', () => {
    it('generates copy title for files', () => {
        expect(generateTitle('copy', 3, 0)).toBe('Copy 3 files')
    })

    it('generates move title for files', () => {
        expect(generateTitle('move', 3, 0)).toBe('Move 3 files')
    })

    it('generates move title for mixed', () => {
        expect(generateTitle('move', 1, 2)).toBe('Move 1 file and 2 folders')
    })

    it('returns bare verb when no items', () => {
        expect(generateTitle('move', 0, 0)).toBe('Move')
    })
})
```

### E2E smoke tests (Playwright)

Add to `apps/desktop/test/e2e-smoke/`:

```typescript
test('F6 opens move dialog', async ({ page }) => {
    await page.keyboard.press('F6')
    await expect(page.getByRole('dialog')).toContainText('Move')
})

test('move dialog shows correct title', async ({ page }) => {
    // Select 2 files
    await selectFiles(page, 2)
    await page.keyboard.press('F6')
    await expect(page.getByRole('heading')).toContainText('Move 2 files')
})
```

### E2E Linux tests (WebDriverIO + tauri-driver)

Add move operation tests to `apps/desktop/test/e2e-linux/`:

```typescript
it('moves file to other pane', async () => {
    // Create test file, move it, verify source gone and destination has it
})

it('handles same-filesystem move instantly', async () => {
    // Move within same volume, verify no visible progress dialog
})

it('handles cross-filesystem move with progress', async () => {
    // Move to different volume, verify progress stages appear
})
```

### Manual testing checklist

1. [ ] F6 opens move dialog with correct title ("Move X files")
2. [ ] Move dialog destination pre-filled with opposite pane path
3. [ ] Move button confirms and starts operation
4. [ ] ESC cancels move dialog
5. [ ] Same-FS move completes instantly (no progress dialog or very brief)
6. [ ] Cross-FS move shows progress with all three stages
7. [ ] Cancel during cross-FS move preserves source files
8. [ ] Conflict resolution works for move (overwrite, skip, rename)
9. [ ] Source pane refreshes after move (files disappear)
10. [ ] Destination pane refreshes after move (files appear)
11. [ ] Function key bar shows F6 Move

## Acceptance criteria

1. F6 keyboard shortcut triggers move dialog
2. Move dialog is visually identical to copy dialog except for title/button text
3. Move operation uses `moveFiles()` backend command
4. Same-filesystem moves complete instantly
5. Cross-filesystem moves show progress through all three phases
6. Both panes refresh after move completion
7. All existing copy tests still pass
8. New move tests pass
9. `./scripts/check.sh --check svelte-tests` passes
10. `./scripts/check.sh --check svelte-check` passes
11. `./scripts/check.sh --check desktop-svelte-eslint` passes
