# Copy dialog implementation plan

## Summary

Add a "Copy" dialog to the UI that allows users to copy files/folders to a destination path. The dialog follows existing modal patterns but with key differences: movable, non-blurred background, and a direction indicator graphic.

## Design requirements (from user)

1. **Title**: Show count of both files and folders with proper pluralization (not "Copy 1 file(s) to")
2. **No Queue feature** (skip for now)
3. **Buttons**: Center-aligned
4. **Volume selector**: Show volume name with available space
5. **Target path**: Pre-filled with destination pane path, text selected for easy editing
6. **Keyboard**: ESC cancels and closes, ENTER confirms
7. **Background**: No blur - user needs to see what's behind
8. **Movable**: Dialog draggable by title bar so user can see content behind it
9. **Direction graphic**: Arrow pointing left/right with source/destination folder names
10. **Confirm action**: Just close dialog for now (TODO for actual copy operation)

## File structure

New files:
- `apps/desktop/src/lib/write-operations/CopyDialog.svelte` - Main dialog component
- `apps/desktop/src/lib/write-operations/DirectionIndicator.svelte` - Arrow graphic showing source → destination

## Implementation steps

### Step 1: Create DirectionIndicator component

A small visual component showing copy direction:
```
[Source folder name] ──→ [Destination folder name]
           or
[Destination folder name] ←── [Source folder name]
```

Props:
- `sourcePath: string` - Full path to source folder
- `destinationPath: string` - Full path to destination folder
- `direction: 'left' | 'right'` - Which way the arrow points (based on active pane)

Extract folder name from path for display.

### Step 2: Create CopyDialog component

**Props:**
```typescript
interface Props {
    sourcePaths: string[]           // Paths of items being copied
    destinationPath: string         // Initial destination (opposite pane's path)
    direction: 'left' | 'right'     // Copy direction for arrow
    volumes: VolumeInfo[]           // Available volumes for selector
    currentVolumeId: string         // Initially selected volume
    fileCount: number               // Number of files selected
    folderCount: number             // Number of folders selected
    onConfirm: (destination: string, volumeId: string) => void
    onCancel: () => void
}
```

**State:**
- `editedPath: string` - Editable destination path
- `selectedVolumeId: string` - Currently selected volume
- `dialogPosition: { x: number, y: number }` - For dragging
- `isDragging: boolean` - Drag state

**Title generation:**
```typescript
function generateTitle(fileCount: number, folderCount: number): string {
    const parts: string[] = []
    if (fileCount > 0) {
        parts.push(`${fileCount} ${fileCount === 1 ? 'file' : 'files'}`)
    }
    if (folderCount > 0) {
        parts.push(`${folderCount} ${folderCount === 1 ? 'folder' : 'folders'}`)
    }
    return `Copy ${parts.join(' and ')}`
}
// Examples: "Copy 1 file", "Copy 3 files", "Copy 2 folders", "Copy 1 file and 2 folders"
```

**Drag implementation** (similar to PaneResizer):
```typescript
let dialogPosition = $state({ x: 0, y: 0 })  // Centered by default
let isDragging = $state(false)

function handleTitleMouseDown(event: MouseEvent) {
    event.preventDefault()
    isDragging = true
    const startX = event.clientX - dialogPosition.x
    const startY = event.clientY - dialogPosition.y

    const handleMouseMove = (e: MouseEvent) => {
        dialogPosition = {
            x: e.clientX - startX,
            y: e.clientY - startY
        }
    }

    const handleMouseUp = () => {
        isDragging = false
        document.removeEventListener('mousemove', handleMouseMove)
        document.removeEventListener('mouseup', handleMouseUp)
    }

    document.addEventListener('mousemove', handleMouseMove)
    document.addEventListener('mouseup', handleMouseUp)
}
```

**Keyboard handling:**
```typescript
function handleKeydown(event: KeyboardEvent) {
    event.stopPropagation()  // Critical: prevent file explorer handling

    if (event.key === 'Escape') {
        onCancel()
    } else if (event.key === 'Enter') {
        handleConfirm()
    }
}
```

**Layout structure:**
```svelte
<div class="modal-overlay" role="dialog" aria-modal="true" tabindex="-1" onkeydown={handleKeydown}>
    <div
        class="copy-dialog"
        style="transform: translate({dialogPosition.x}px, {dialogPosition.y}px)"
    >
        <!-- Draggable title bar -->
        <div class="dialog-title-bar" onmousedown={handleTitleMouseDown}>
            <h2 id="dialog-title">{title}</h2>
        </div>

        <!-- Direction indicator -->
        <DirectionIndicator {sourcePath} {destinationPath} {direction} />

        <!-- Volume selector -->
        <div class="volume-selector">
            <select bind:value={selectedVolumeId}>
                {#each volumes as volume}
                    <option value={volume.id}>{volume.name}</option>
                {/each}
            </select>
            <!-- Note: Free space display requires backend API addition (out of scope) -->
        </div>

        <!-- Path input -->
        <input
            type="text"
            bind:value={editedPath}
            bind:this={pathInputRef}
            aria-label="Destination path"
        />

        <!-- Buttons (centered) -->
        <div class="button-row">
            <button onclick={onCancel}>Cancel</button>
            <button class="primary" onclick={handleConfirm}>Copy</button>
        </div>
    </div>
</div>
```

**Styling notes:**
- NO `backdrop-filter: blur()` on overlay - just semi-transparent dark
- Dialog uses `position: fixed` but offset by `dialogPosition`
- Title bar gets `cursor: move` on hover
- Use existing CSS variables from `app.css`
- Border-radius: 12px, consistent with other dialogs

### Step 3: Integrate into DualPaneExplorer

Add to `+page.svelte` or `DualPaneExplorer.svelte`:

**State:**
```typescript
let showCopyDialog = $state(false)
let copyDialogProps = $state<CopyDialogProps | null>(null)
```

**Open dialog function:**
```typescript
async function openCopyDialog() {
    const focusedPane = getFocusedPane()
    const oppositePanePath = focusedPane === 'left' ? rightPath : leftPath
    const sourcePanePath = focusedPane === 'left' ? leftPath : rightPath

    // Get selected files from active pane
    const paneRef = focusedPane === 'left' ? leftPaneRef : rightPaneRef
    const selectedIndices = paneRef?.getSelectedIndices() ?? []

    // Get stats to know file vs folder counts
    const listingId = paneRef?.getListingId()
    const stats = await getListingStats(listingId, includeHidden, selectedIndices)

    // Build source paths (would need to get actual paths from listing)
    const sourcePaths = await getSelectedPaths(listingId, selectedIndices)

    copyDialogProps = {
        sourcePaths,
        destinationPath: oppositePanePath,
        direction: focusedPane === 'left' ? 'right' : 'left',
        volumes,
        currentVolumeId: focusedPane === 'left' ? rightVolumeId : leftVolumeId,
        fileCount: stats.selectedFiles ?? 0,
        folderCount: stats.selectedDirs ?? 0,
        onConfirm: handleCopyConfirm,
        onCancel: () => { showCopyDialog = false }
    }
    showCopyDialog = true
}

function handleCopyConfirm(destination: string, volumeId: string) {
    // TODO: Implement actual copy operation using copyFiles() from tauri-commands
    showCopyDialog = false
}
```

**Keyboard shortcut** (likely F5 based on traditional file manager conventions):
Add to keyboard handler to trigger `openCopyDialog()`.

### Step 4: Add helper function for getting selected paths

Need a way to get full paths from selected indices. Check if this already exists in `tauri-commands.ts` or add:

```typescript
export async function getSelectedFilePaths(
    listingId: string,
    selectedIndices: number[]
): Promise<string[]>
```

This may require a new Tauri command or using existing listing cache.

## Out of scope (noted for future)

1. **Volume free space display**: The current `VolumeInfo` type doesn't include free space. Would require:
   - Rust: Add `fs2` crate or use `statvfs` to get volume space
   - Backend: Add `available_bytes` to `VolumeInfo` struct
   - Frontend: Display formatted space in volume selector

2. **Actual copy execution**: The confirm action just closes the dialog. The real `copyFiles()` integration with progress UI is a separate task.

3. **Queue feature**: Not implementing the Queue button shown in Commander One.

## Testing approach

1. **Unit tests** (Vitest):
   - Title generation with various file/folder counts
   - Path extraction from full path for DirectionIndicator

2. **Manual testing**:
   - Dialog opens with correct pre-filled values
   - ESC closes, ENTER confirms
   - Dialog is draggable
   - Background is visible (no blur)
   - Volume selector changes update path prefix appropriately

## CSS variables to use

From `app.css`:
- `--color-bg-secondary` - Dialog background
- `--color-border-primary` - Dialog border
- `--color-text-primary` - Title text
- `--color-text-secondary` - Labels
- `--color-accent` - Primary button
- `--color-button-hover` - Button hover state
