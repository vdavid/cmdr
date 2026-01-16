# File selection

Cmdr supports selecting multiple files for batch operations like copy, move, and delete.

## User interaction

### Single file selection

Press `Space` to toggle selection of the file under the cursor.

### Range selection

Hold `Shift` while navigating to select a range of files:

- `Shift+↑/↓/←/→` - Extend selection in that direction
- `Shift+Click` - Select all files between current cursor and clicked file

The range updates live as you move. Release `Shift` to finalize the selection.

**Deselection mode**: If you start on an already-selected file, the range will deselect instead of select.

**Range shrinking**: Moving back toward the anchor deselects files that are no longer in the range.

### Select/deselect all

- `⌘A` - Select all files (except ".." entry)
- `⌘⇧A` - Deselect all files

### Special cases

- The ".." (parent directory) entry cannot be selected
- Selection is cleared when navigating to a different directory
- Selection is preserved when sorting or filtering

## Visual indication

Selected files are displayed with a yellow foreground color (`--color-selection-fg`).

## Implementation

### Frontend (Svelte)

Selection state lives in `FilePane.svelte`:

```typescript
const selectedIndices: SvelteSet<number> = new SvelteSet()
let selectionAnchorIndex = $state<number | null>(null)
let selectionEndIndex = $state<number | null>(null)
let isDeselecting = $state<boolean>(false)
```

**Safety contract**: `selectedIndices` uses SvelteSet with direct mutations (`.add()`, `.delete()`, `.clear()`). Never reassign the variable—SvelteSet only tracks mutations for reactivity.

### Backend (Rust)

The backend doesn't store selection state. It receives `(listingId, selectedIndices[])` from the frontend when performing write operations, resolving indices to file paths from the cached listing.

For sort/filter operations, the `resort_listing` command accepts selection indices and returns their new positions after re-sorting.

### MCP integration

Resource:
- `cmdr://selection` - Returns selected file indices in the focused pane

Tools:
- `selection_clear` - Clear selection
- `selection_selectAll` - Select all files
- `selection_deselectAll` - Deselect all files
- `selection_toggleAtCursor` - Toggle selection at cursor
- `selection_selectRange` - Select a range of indices
