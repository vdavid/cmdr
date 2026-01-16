# File selection spec

It's time to add a "select files" feature to the app.

## Context

- Until now, the app was read-only, so we didn't need file selection.
- Now we're adding the first write operations, and we need file selection for the user to specify which files to copy.
- In preparation to this development, we've cleared out the terminology: the cursor position was used to be called
  "selection". We went through the codebase and removed all uses of the terms "select/selection/selected" in this sense,
  so the term is now fully available for actual file selection.

## UI plan

### Ways to select files:

#### 1. Single file selection with space

When the user presses Space, it should toggle selecting the file under the cursor. Simple.

#### 2. Range selection with Shift+click or Shift+arrows

- It'd like to introduce the notation of
  - `A` is the selection anchor index (named `selectionAnchorIndex`)
  - `B` is the current selection end index (named `selectionEndIndex`)
  - `()` denotes a range of indexes in an exclusive manner
  - `[]` same as above, but inclusive.
  - `(AB]` means from A (exclusive) to B (inclusive)
- On Shift+click or Shift+arrow (no global Shift tracking needed)
  - If `A` is not set, set `A` = current cursor position **before** moving the cursor
  - This also works for Shift-clicking into a pane: the old cursor position becomes `A`
  - Shift-clicking on the current cursor position also works: both `A` and `B` become that position
- When the user clicks some file or moves the cursor (arrow keys, pgup/pgdown, home/end) with Shift held
  - `B` should be marked
  - Selection `[AB]` should be applied, and the UI updated visually.
  - When moving the cursor, `[AB]` should be updated live.
  - It might happen that the user holds Shift and scrolls the file list with mouse wheel, touchpad, or scrollbar, or
    even with the keys if the user moves a lot with the cursor. In this case, the standard mechanism will pull the
    visible items from the back end. The job of the front end is to keep updating `B`. This is probably not hard,
    because cursorIndex is absolute for the whole file list, so we can just set `B` = `cursorIndex`.
  - On the next non-Shift action, `A` and `B` are cleared, but the selection remains. This is equivalent to "Shift was
    released." The selection is stored as a `Set<number>` in Svelte state (`$state<Set<number>>`), which gives O(1)
    add/remove/has operations—performant even with 500k files.
- If there is an existing selection, then it's a bit trickier. If the user starts on a file that is not yet selected,
  then the logic is the same as above, just that the existing selection remains, and we add to it.
  - But: When the user selects `[AB]`, then changes the selection to `[AB']` where `A < B' < B`, then we need to
    remove `(B'B]` from the selection, EVEN IF the selection pre-existed. So the user is effectively able to deselect
    files this way.
  - Similarly in the negative direction: if the user changes to `[AB']` where `B < B' < A`, then we need to remove
    `[BB')` from the selection.
- If there is an existing selection, AND the user starts on a file that is already selected, then the logic is inverted:
  - Selecting `[AB]` should remove `[AB]` from selection.
  - But it's not _completely_ inverted: in the case of deselection, when the user changes selection to `[AB']` where
    `A < B' < B`, then we keep `[B'B]` deselected, regardless of whether it was selected or unselected in the big state
    before our current selection action. (So basically, both select and unselect actions taint existing selection,
    in the ways described above.)
  - Similarly in the negative direction.

#### 3. Select all / Deselect all

- Cmd+A should select all files in the current directory, overwriting any previous selection.
- Cmd+Shift+A should deselect all files in the current directory.

#### 4. The ".." entry

- The ".." (parent directory) entry can't be selected—you can't copy a parent reference.
- When a parent exists, selection indices are offset by 1 (frontend index = backend index + 1).

#### 5. Clear selection on navigation

- When the user navigates to another directory, the selection should be cleared.

### Preserve selection on sort/filter

- When the user changes sort order or filter settings, the selection should be preserved.
- The `resortListing` Tauri command will be extended to accept `selectedIndices` and return `newSelectedIndices`.
- Flow:
  1. Frontend sends `(listingId, sortBy, sortOrder, cursorFilename, selectedIndices[], includeHidden)`
  2. Rust looks up filenames for the selected indices from the cached listing
  3. Rust re-sorts the listing
  4. Rust finds new indices for those filenames
  5. Rust returns `{ newCursorIndex, newSelectedIndices[] }`
  6. Frontend updates its selection Set with the new indices
- Optimization: if all files are selected, the frontend can send `allSelected: true` instead of the full list.
- After sort, `A` and `B` are cleared (sorting is a "new context").

### Later: select by patterns
- There will be a "Select..." menu that opens a window and lets the user select files based on criteria
- There will also be a chat interface with an AI agent that can select files based on user instructions
- This is out of scope for now.

### Visual indication of selection

- Selected files should have a different foreground color (the mustard yellow we use for the accent color).
- Use a dedicated CSS variable `--color-selection-fg` (same value as `--color-accent`, but logically separate).

## MCP server

Resources:
- `cmdr://selection` - returns the list of selected file indices in the focused pane.

Tools (using underscore naming to match existing conventions like `nav_open`, `file_copyPath`):
- `selection_clear` - clears the current selection.
- `selection_selectAll` - selects all files in the current directory.
- `selection_deselectAll` - deselects all files in the current directory.
- `selection_toggleAtCursor` - toggles selection of the file under the cursor.
- `selection_selectRange` - selects a range of files between two indices.

## Architecture decisions

### Selection lives on the frontend

Yes, selection is frontend-only. The Rust backend doesn't need selection knowledge except:
- MCP state sync (for AI agents to read/control selection)
- Write operations receive `(listingId, selectedIndices[])` and resolve paths internally

This matches how `cursorIndex` already works: it lives in `FilePane.svelte` as `$state`, and the backend only
learns about it via MCP sync.

### State model

```typescript
// In FilePane.svelte
let selectedIndices = $state<Set<number>>(new Set())
let selectionAnchorIndex = $state<number | null>(null)  // A
let selectionEndIndex = $state<number | null>(null)     // B
let isDeselecting = $state<boolean>(false)  // true if anchor was already selected
```

### Write operations receive indices, not paths

For performance with large selections (up to 500k files), write operation commands accept `(listingId, indices[])`
instead of file paths. The Rust backend resolves indices to paths from the cached listing. Sending 500k numbers
over IPC is fine (~3-4MB JSON). If needed later, we can add an `allSelected: true` flag.

### Drag & drop

Drag & drop will need to consider selection (drag selected set vs. drag single file), but that's out of scope
for this spec.