# File selection implementation tasks

Implementation plan for the [file selection spec](../specs/file-selection.md).

## Phase 1: Frontend state and basic selection

- [x] Add selection state to FilePane.svelte
  - [x] `selectedIndices: Set<number>`
  - [x] `selectionAnchorIndex: number | null`
  - [x] `selectionEndIndex: number | null`
  - [x] `isDeselecting: boolean`
- [x] Implement Space key toggle selection at cursor
- [x] Clear selection on navigation (in `loadDirectory()`)
- [x] Add `--color-selection-fg` CSS variable to app.css

## Phase 2: Visual rendering

- [x] Update BriefList.svelte to show selected state
  - [x] Pass `selectedIndices` prop
  - [x] Add `is-selected` class to file entries
  - [x] Style with `--color-selection-fg`
- [x] Update FullList.svelte similarly
- [x] Handle ".." entry (index 0 when hasParent can't be selected)

## Phase 3: Range selection (Shift+click/arrow)

- [x] Add Shift+arrow key handlers in FilePane.svelte
  - [x] Set anchor before moving cursor if not set
  - [x] Update selectionEndIndex on each move
  - [x] Apply range to selectedIndices
- [x] Add Shift+click handlers in BriefList.svelte and FullList.svelte
- [x] Implement deselection mode (when anchor is already selected)
- [x] Implement range shrinking (removing items when range contracts)

## Phase 4: Select all / deselect all

- [x] Implement Cmd+A (select all except "..")
- [x] Implement Cmd+Shift+A (deselect all)

## Phase 5: Preserve selection on sort/filter

- [x] Extend `resortListing` Rust command
  - [x] Accept `selected_indices: Vec<usize>` parameter
  - [x] Accept `all_selected: bool` optimization flag
  - [x] Return `new_selected_indices: Vec<usize>`
- [x] Update frontend to send/receive selection on resort
- [x] Clear A and B after sort (keep selectedIndices)

## Phase 6: MCP integration

- [x] Add `cmdr://selection` resource in resources.rs
- [x] Add selection tools in tools.rs and executor.rs
  - [x] `selection_clear`
  - [x] `selection_selectAll`
  - [x] `selection_deselectAll`
  - [x] `selection_toggleAtCursor`
  - [x] `selection_selectRange`
- [x] Sync selection state to PaneState for MCP access

## Phase 7: Testing

- [x] Existing tests pass after adding `selected_indices` to PaneState
- [x] Manual test: keyboard selection flows
- [x] Manual test: mouse selection flows
- [x] Manual test: MCP tools via agent
- [x] Add Svelte tests for selection state logic (in integration.test.ts)

## Phase 8: Documentation

- [x] Write docs/features/file-selection.md (feature doc, not a spec!)
- [x] Add keyboard shortcuts to user docs
- [x] Also add the keyboard shortcuts to the Command Palette