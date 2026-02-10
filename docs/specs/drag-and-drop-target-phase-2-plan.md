# Phase 2: Folder-level drop targeting

## Context

Phase 1 (pane-level drops) is complete: dropping files onto a pane opens the transfer dialog with the pane's current
directory as destination. Phase 2 adds folder-level precision: hovering over a directory row during drag highlights it
and uses its path as the drop destination instead.

## Approach

Extend Phase 1's `document.elementFromPoint()` hit-testing with a single `data-drop-target-path` attribute on
directory rows. If the attribute exists on the hovered element, it's a valid folder drop target. If not, fall back to
pane-level behavior.

### Key decision: `data-drop-target-path` (single attribute)

Instead of separate `data-path` + `data-is-directory`, use one attribute that's only present on valid drop targets:

```svelte
data-drop-target-path={file.isDirectory && file.name !== '..' ? file.path : undefined}
```

- Present = folder drop target, value is the destination path
- Absent = not a target, fall back to pane-level
- `..` is excluded
- Symlinks to directories have `isDirectory: true` (backend resolves), so they work automatically

### Self-drag + folder targeting

- Same pane + pane-level target = no-op (suppress highlight)
- Same pane + folder target = **valid** (dragging files into a subfolder within the same pane)
- Different pane + any target = valid

## File changes

### New: `drop-target-hit-testing.ts`

Pure logic module with `DropTarget` type and `resolveDropTarget` function. Walks up from
`document.elementFromPoint()` to find `.file-entry` with `data-drop-target-path`, falls back to pane-level.

### Modified: `FullList.svelte` and `BriefList.svelte`

Added `data-drop-target-path` attribute to `.file-entry` divs.

### Modified: `DualPaneExplorer.svelte`

- Added `dropTargetFolderPath` and `dropTargetFolderEl` state
- Replaced `resolveDropTargetPane` with imported `resolveDropTarget`
- Extracted `handleDragOver` and `handleDrop` to keep complexity under 15
- Updated `handleFileDrop` to accept optional `targetFolderPath`
- Added `$effect` for imperative `.folder-drop-target` class management
- Added `:global(.file-entry.folder-drop-target)` CSS rule (outline + bg highlight)

### Modified: `allowlist.go`

Added `folder-drop-target` to CSS class allowlist (applied imperatively, not visible to static analysis).
