# Drag-and-drop target — phase 1 plan

Implements pane-level drop targeting: drop files onto a pane to trigger the copy confirmation dialog.

- Spec: [drag-and-drop-target-spec.md](drag-and-drop-target-spec.md) (phase 1 section)
- Research: [drag-and-drop-target-thinking.md](drag-and-drop-target-thinking.md)

## Architecture

Two new modules, one modified module, and integration in `DualPaneExplorer.svelte`.

### New: `drop-target-hit-testing.ts`

Pure function module — no DOM, no Svelte, no Tauri deps. Fully unit-testable.

```ts
type PaneId = 'left' | 'right'

interface PaneRect { left: number; top: number; right: number; bottom: number }

function identifyTargetPane(
    cursorX: number, cursorY: number,
    leftRect: PaneRect, rightRect: PaneRect,
): PaneId | null
```

Takes physical cursor position (from Tauri's `DragDropEvent`) and pane bounding rects, returns which pane the cursor is over or `null`.

### New: `buildTransferPropsFromDroppedPaths` in `transfer-operations.ts`

The existing `buildTransferPropsFromSelection` / `buildTransferPropsFromCursor` get source paths from a listing ID. For dropped files, we already have absolute paths. Add:

```ts
async function buildTransferPropsFromDroppedPaths(
    operationType: TransferOperationType,
    droppedPaths: string[],
    destPath: string,
    direction: 'left' | 'right',
    destVolumeId: string,
    sortColumn: SortColumn,
    sortOrder: SortOrder,
): Promise<TransferDialogPropsData>
```

This counts files vs folders from the paths (via a Tauri command or `stat`) and builds the props struct.

### Modified: `drag-drop.ts`

Add an exported `isDraggingFromSelf` flag:

```ts
export let isDraggingFromSelf = false
```

Set to `true` when `performSingleFileDrag` or `performSelectionDrag` starts, cleared in their `finally` blocks and in `cancelDragTracking`.

### Integration: `DualPaneExplorer.svelte`

1. Import `onDragDropEvent` from `@tauri-apps/api/webview` and `getCurrentWebview`.
2. In `onMount`, register the listener; in `onDestroy`, unregister.
3. On `enter`/`over`: get pane element refs' `getBoundingClientRect()`, call `identifyTargetPane`, toggle a `dropTargetActive` state per pane.
4. On `leave`: clear highlights.
5. On `drop`: resolve target pane, build props via `buildTransferPropsFromDroppedPaths`, set `transferDialogProps` and `showTransferDialog = true`.
6. Guard: if `isDraggingFromSelf` and target pane is the source pane, no-op.

### CSS: pane drop highlight

Add a `.drop-target-active` class (or pass a prop to `FilePane`). Uses `box-shadow` inset or `outline` — must not shift layout. Use existing color variables from `app.css` (an accent color at low opacity).

## Task list

### Milestone 1: Hit-testing module
- [x] Create `apps/desktop/src/lib/file-explorer/drop-target-hit-testing.ts` with `identifyTargetPane`
- [x] Create unit tests in `apps/desktop/src/lib/file-explorer/drop-target-hit-testing.test.ts`

### Milestone 2: Integration
- [x] Add `isDraggingFromSelf` flag to `drag-drop.ts` (set on drag start, clear on end)
- [x] Add `buildTransferPropsFromDroppedPaths` to `transfer-operations.ts` + unit tests
- [x] Register `onDragDropEvent` in `DualPaneExplorer.svelte` (enter/over/leave/drop handling)
- [x] Add pane highlight CSS (no layout shift, uses existing color vars)
- [x] Wire drop → `transferDialogProps` assignment, with same-pane and empty-paths guards

### Milestone 3: Validation
- [x] Run `./scripts/check.sh --svelte` — all checks pass
- [x] Update `coverage-allowlist.json` if needed for new files with Tauri/DOM deps (none needed)
- [ ] Manual test: external drop from Finder onto each pane
- [ ] Manual test: pane-to-pane drag, same-pane no-op
