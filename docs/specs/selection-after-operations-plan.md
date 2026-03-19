# Selection after file operations

Fix: selection persists on stale indices after move/copy/delete/trash, pointing at wrong files.

## Problem

Selection is index-based (`SvelteSet<number>`) for performance (200k files at 4-8 bytes each vs ~100 bytes per path).
After a file operation completes, indices are never cleared or adjusted. When the listing changes (files removed by
watcher, or new files added), the same indices now refer to different files.

## Design

Two complementary deselection mechanisms, chosen based on whether the operation changes the source listing:

### Mechanism 1: diff-driven index adjustment (move, delete, trash)

When files disappear from the source pane, the file watcher emits `directory-diff` events. The diff handler in
`FilePane.svelte` already processes these (bumps `cacheGeneration`, refetches `totalCount`), but never touches selection.

**The fix**: after receiving a diff, ask the backend which of the previously-selected files still exist. The backend
cache is already updated with the new listing by the time the diff event reaches the frontend (see
`handle_directory_change` in `watcher.rs`: it calls `update_listing_entries` BEFORE emitting the event). So
`find_file_index(listingId, name)` returns correct results.

**Why not compute index shifts locally?** The frontend doesn't maintain a full file array — it uses the backend cache
with virtual scroll. The diff event carries `FileEntry` objects but no positional indices. Computing shifts would require
either (a) maintaining a parallel frontend array (duplicates the backend cache, defeats the non-reactive store design) or
(b) adding index fields to `DiffChange` (the backend replaces the entire vector, so the "old index" concept doesn't
exist in the current architecture). Asking the backend for survivors is simpler and correct.

**Algorithm**: use a batch IPC `findFileIndices(listingId, names[], includeHidden) → Record<string, number>` to avoid N
sequential IPC calls. The backend iterates the cached listing once, building a name→index map for the requested names.
O(listing + requested names). Returns a map (not a vec of tuples) so frontend lookups are O(1).

**Flow**:
1. When the user confirms the operation (before the backend starts), snapshot the selected file names. Store on the
   FilePane as `operationSelectedNames: string[]`.
2. On each `directory-diff` event (while `operationSelectedNames` is set), after the existing
   `totalCount`/`cacheGeneration` update, call `findFileIndices(listingId, operationSelectedNames)`.
3. Replace selection with the returned indices (adjusted for `hasParent` offset). Use a diff generation counter to
   discard results from stale `findFileIndices` calls (multiple rapid diffs can cause out-of-order async resolution).
4. Do NOT prune `operationSelectedNames` when names aren't found. The cost of re-querying absent names is negligible,
   and pruning risks losing track of files that temporarily disappear (e.g., rename-in-place across debounce windows).
5. On operation complete/error: clear `operationSelectedNames` and clear selection.
6. On operation cancel: clear `operationSelectedNames`. Selection now reflects survivors.

**Why snapshot at confirm time (not when the progress dialog opens)?** Same-FS moves execute synchronously in
`move_with_rename` — all renames happen before the progress dialog even opens. If we snapshot after the dialog opens,
the listing may already be stale (renames done, watcher refresh triggered), and the snapshot captures wrong names.
Snapshotting at confirm time guarantees the listing and selection are consistent.

**Snapshot must also happen for clipboard paste operations.** `startTransferProgress` in `dialog-state.svelte.ts`
opens the progress dialog directly (bypassing `handleTransferConfirm`), used by clipboard paste. The snapshotting call
must be placed in `startTransferProgress` too, or extracted to a shared "begin operation" helper that both paths call.

**Performance for `allSelected` case**: for 200k selected files, the snapshot would be 200k `getFileAt` calls. Instead,
use the existing `allSelected` optimization: if all files were selected at operation start, set
`operationSelectedNames` to a sentinel value `'all'` instead of an array. This skips the snapshot and the per-diff
`findFileIndices` calls. On complete/error: clear selection. On cancel (for move/delete/trash): call `selectAll()` with
the new `totalCount` to re-select all surviving files. On cancel (for copy): the source listing didn't change, so the
existing `SvelteSet` indices are still valid — leave selection untouched.

### Mechanism 2: new backend event for gradual copy deselection

Copy doesn't change the source listing — no diffs arrive on the source pane. So we need explicit deselection.

**Why not reuse `write-progress.files_done`?** `files_done` counts individual files from the recursive scan (a directory
with 500 files counts as 500, not 1), so there's no 1:1 mapping between `files_done` and top-level `sourcePaths` items.

**Approach**: add a new backend event `write-source-item-done` emitted when all files belonging to a top-level source
item have been processed. Payload: `{ operation_id, source_path }`. The frontend extracts the filename, calls
`findFileIndex` to get its current index, and deselects it. This is safe during copy because the source listing is
stable — no concurrent diffs shifting indices.

**Source boundary detection after scan sort**: `scan_sources` sorts `scan_result.files` by the user's chosen sort column,
interleaving files from different top-level sources. You cannot detect source boundaries by iterating `files` in order.
Solution: `FileInfo` already has a `source_root` field (the *parent* of the top-level source, not the source itself).
The top-level source path for a `FileInfo` is reconstructed as
`source_root.join(path.strip_prefix(source_root).components().next())`. Pre-compute a `HashMap<PathBuf, usize>` keyed
by the **reconstructed top-level source path** (NOT by `source_root`, which is shared by all sources in the same
directory). In the copy loop, maintain a parallel counter `HashMap<PathBuf, usize>` using the same key. Increment after
each `copy_single_item`. When a source's count reaches its pre-computed total, emit `write-source-item-done`.

Note: skipped files (conflict resolution = Skip) still increment `files_done` in the copy loop, so the per-source
counter correctly reaches the total even when files are skipped — both the scan and the loop count the same items.

For trash and same-FS move, the scan sort issue doesn't apply: trash iterates `sources` directly (one `trashItemAtURL`
per top-level item), and `move_with_rename` iterates `sources` directly (one `fs::rename` per top-level item). Emission
is straightforward — after each successful item.

For delete, the same sorted-scan issue applies. Use the same per-source counter approach as copy. Note:
`delete_files_with_progress` has two loops (files first, then directories). The per-source counter based on
`scan_result.files` reaches its total after the last *file* inside a directory is deleted, before the directory entry
itself is removed (second loop). This is acceptable — the directory removal is a trivial follow-up.

For cross-FS move (`move_with_staging`), the copy phase writes to a staging dir (source files untouched), so
`write-source-item-done` during the copy phase would trigger deselection before the source file actually disappears.
This is acceptable — the deselection is a hint that the item has been processed. During the subsequent delete-sources
phase, the file watcher diffs handle the actual removal (mechanism 1). The alternative (not emitting during copy phase)
would mean zero gradual deselection during the potentially long copy phase.

### Source pane identification

For copy/move: the source pane is the opposite of `transferProgressProps.direction` (direction = where files go, so
source is the other side).

For delete/trash: there's no `direction` concept. The source pane must be stored at operation-start time since focus may
shift during the operation (the progress dialog itself takes focus). Add a `sourcePaneSide: 'left' | 'right'` field to
`TransferProgressPropsData`, set from the focused pane when the operation is confirmed.

### Clear on complete (safety net)

`handleTransferComplete` in `dialog-state.svelte.ts` clears selection on the source pane. This is the safety net that
ensures selection is always clean after any operation, even if the gradual deselection missed something.

### Cancel behavior

Falls out naturally from mechanisms 1 and 2:
- Move/delete/trash cancel: processed files were already deselected by diff-driven updates. Remaining files are still
  selected at their correct (adjusted) indices.
- Copy cancel: processed files were deselected by source-item-done events. Unprocessed files remain selected (listing
  unchanged, indices stable).
- `allSelected` + cancel (move/delete/trash): call `selectAll()` with the new `totalCount` to re-select all survivors.
- `allSelected` + cancel (copy): leave selection untouched — the source listing didn't change, so existing indices are
  still valid.
- No special cancel logic beyond the above.

### Error behavior

Same as complete: clear selection. The operation is over; keeping stale selection would be confusing.

### Both panes showing the same directory

Known limitation: if both panes show the same directory, only the source pane's selection is adjusted. The other pane
also receives diffs but its selection (if any) becomes stale. Milestone 1's clear-on-complete partially mitigates this
(the source pane is cleared), but the other pane is not touched. This is acceptable because:
- Dual-pane-same-directory is uncommon during file operations (you'd typically have source and destination open)
- The other pane's selection was made independently and isn't related to the operation
- If it becomes an issue, a future enhancement could clear selection on any pane that receives a diff while an operation
  is active

## Scenario table

| Scenario | Source pane | Selection |
|---|---|---|
| Move complete | Files gone (via watcher) | Cleared on complete |
| Move cancel (same-FS) | Some files gone, some remain | Survivors stay selected (diff-driven) |
| Move cancel (cross-FS, during copy phase) | All files still there | Full selection remains |
| Move cancel (cross-FS, during delete-source) | Some files gone, some remain | Survivors stay selected (diff-driven) |
| Copy complete | No change | Cleared on complete |
| Copy cancel | No change | Unprocessed files stay selected (source-item-done) |
| Delete/trash complete | Files gone | Cleared on complete |
| Delete/trash cancel | Some files gone, some remain | Survivors stay selected (diff-driven) |
| allSelected + cancel (move/del/trash) | Some files gone | `selectAll()` called with new count |
| allSelected + cancel (copy) | No change | Selection untouched (indices still valid) |

## Implementation

### Milestone 1: clear on complete/error/cancel (the quick fix)

Fixes the most visible bug: selection pointing at wrong files after operation.

**1.1 Add `sourcePaneSide` to `TransferProgressPropsData`**
- New field: `sourcePaneSide: 'left' | 'right'`
- Set when building progress props: for copy/move it's derived from `direction` (opposite side). For delete/trash,
  it's the focused pane side at confirm time.

**1.2 Add `clearSourcePaneSelection` helper to `dialog-state.svelte.ts`**
- Use `transferProgressProps.sourcePaneSide` to get the correct pane ref
- Call `paneRef.clearSelection()`

**1.3 Call it from `handleTransferComplete`, `handleTransferError`, `handleTransferCancelled`**
- Place the call after `refreshPanesAfterTransfer()` but before `deps.onRefocus()`

**1.4 Tests**
- Unit test: verify `clearSelection` is called with correct pane for each direction/operation type
- Manual test: select files → move/copy/delete → verify selection cleared

### Milestone 2: batch `findFileIndices` IPC command

Enables efficient bulk name→index resolution without N sequential IPC calls.

**2.1 Add `find_file_indices` to Rust listing operations**
- Signature: `fn find_file_indices(listing_id: &str, names: &[String], include_hidden: bool) → Result<HashMap<String, usize>, String>`
- Single pass over cached entries, O(entries + names)
- Returns only found names as keys (removed files are simply absent from the map)
- Returns backend indices (0-based). Frontend applies `hasParent` +1 offset, same convention as `find_file_index`.

**2.2 Add Tauri command wrapper**
- `find_file_indices(listing_id: String, names: Vec<String>, include_hidden: bool)`
- Frontend TS wrapper in `tauri-commands/`

**2.3 Tests**
- Rust unit test: verify correct index mapping with hidden files, empty names, names not in listing, duplicate names

### Milestone 3: diff-driven selection adjustment (move, delete, trash)

The gradual deselection for operations that change the source listing.

**3.1 Add `operationSelectedNames` to FilePane**
- New state: `let operationSelectedNames = $state<string[] | 'all' | null>(null)`
- Plain `let diffGeneration = 0` (NOT `$state` — it's only used inside async callbacks, not for rendering)
- Public methods: `snapshotSelectionForOperation()` and `clearOperationSnapshot()`
- `snapshotSelectionForOperation()`:
  - If `allSelected`, set to `'all'`
  - Otherwise, resolve selected indices → names via `getFileAt` calls (fine for typical 1-100 selections)

**3.2 Snapshot selection when operation is confirmed**
- Create a shared `beginOperation()` helper in `dialog-state.svelte.ts` that calls
  `sourcePaneRef.snapshotSelectionForOperation()`
- Call it from both `handleTransferConfirm` and `startTransferProgress` (clipboard paste path)
- Also call it from `handleDeleteConfirm`
- The source pane ref is identified by `sourcePaneSide` (milestone 1)

**3.3 Adjust selection in diff handler**
- In the `directory-diff` listener in FilePane, after the existing `totalCount`/`cacheGeneration` update:
  - If `operationSelectedNames` is `null`, skip (no operation active)
  - If `operationSelectedNames` is `'all'`, skip (allSelected mode, handled on complete/cancel)
  - Increment `diffGeneration`. Capture the current value as `myGeneration`.
  - Call `findFileIndices(listingId, operationSelectedNames, includeHidden)`
  - Before applying result, check `myGeneration === diffGeneration`. If not, discard (a newer diff superseded this one).
  - Build new selection from returned indices (each + 1 if `hasParent`)
  - Call `selection.setSelectedIndices(newIndices)`

**3.4 Clear snapshot on operation end**
- `clearOperationSnapshot()` must also bump `diffGeneration` to invalidate any in-flight `findFileIndices` callbacks.
  Without this, a stale callback arriving after clearSelection() would re-populate selection with garbage.
- In `handleTransferComplete`, `handleTransferError`: call `sourcePaneRef.clearOperationSnapshot()` and
  `sourcePaneRef.clearSelection()`
- In `handleTransferCancelled`:
  - Call `sourcePaneRef.clearOperationSnapshot()`
  - If the snapshot was `'all'` and operation is move/delete/trash: call `sourcePaneRef.selectAll()` to re-select all
    survivors
  - If the snapshot was `'all'` and operation is copy: leave selection untouched (source listing unchanged)
  - Otherwise: don't clear selection — it already reflects survivors from the last diff-driven adjustment

**3.5 Tests**
- Pure function test: given `operationSelectedNames` and `findFileIndices` result, compute correct new selection indices
  (with hasParent offset)
- Test: out-of-order diff resolution correctly discarded via generation counter
- Edge cases: all files removed in one batch, files removed that weren't selected, file added during operation, empty
  selection, allSelected + cancel (move vs copy)

### Milestone 4: `write-source-item-done` event and gradual deselection

Gradual deselection during operations, providing visual feedback as each top-level source item completes.

**4.1 Add `WriteSourceItemDoneEvent` to Rust types**
- New event struct: `{ operation_id: String, source_path: String }`
- Emitted as `write-source-item-done`

**4.2 Emit from operations — simple cases (iterate `sources` directly)**
- Same-FS move (`move_with_rename`): emit after each successful `fs::rename`. Straightforward — the loop iterates
  `sources` directly.
- Trash (`trash_files_with_progress`): emit after each successful `trashItemAtURL`. Same — iterates `sources`.

**4.3 Emit from operations — sorted-scan cases (files interleaved across sources)**
- `scan_sources` sorts `scan_result.files` by the user's sort column, interleaving files from different top-level
  sources. Cannot detect source boundaries by iteration order.
- Solution: during scan, pre-compute a `HashMap<PathBuf, usize>` mapping each top-level source path to its total file
  count. The source path for each `FileInfo` is derived from `file_info.source_root` + first path component relative to
  `source_root`.
- In the copy/delete loop, maintain a parallel `HashMap<PathBuf, usize>` of files processed per source. After each
  `copy_single_item` or `remove_file`, increment the counter. When it reaches the pre-computed total, emit
  `write-source-item-done`.
- Applies to: `copy_files_with_progress`, `move_with_staging` (copy phase), `delete_files_with_progress`.

**4.4 Cross-FS move note**
- During the copy-to-staging phase of `move_with_staging`, the source files are untouched. Emitting
  `write-source-item-done` during this phase is acceptable — it signals "this item has been processed" even though the
  source hasn't been deleted yet. During the subsequent `delete_sources_after_move` phase, the file watcher diffs
  (mechanism 1) handle the actual source removal. There is zero gradual deselection during the copy phase if we don't
  emit here, which would be a worse UX for long cross-FS moves.
- For `delete_sources_after_move`: this already iterates `sources` directly (not the sorted scan), so emitting after
  each deletion is straightforward.

**4.5 Frontend listener in FilePane**
- Add `write-source-item-done` event listener in FilePane (parallel to the `directory-diff` listener)
- Only active when `operationSelectedNames` is set and is an array (not `'all'`)
- Extract filename from `source_path`, call `findFileIndex(listingId, name, includeHidden)` to get current index
- If found, deselect that index: `selection.selectedIndices.delete(frontendIndex)`
- For move/delete/trash, this provides an earlier deselection than waiting for the next file watcher diff. The
  diff-driven adjustment (milestone 3) will reconcile if needed.

**4.6 Tests**
- Rust test: verify `write-source-item-done` is emitted at correct boundaries for all operation types, especially
  with sorted scan results where files from different sources are interleaved
- Frontend test: given source-item-done event, verify correct index is deselected
- Edge case: event arrives after diff already removed the file (deselection is a no-op, should not error)

### Milestone 5: docs and polish

**5.1 Update CLAUDE.md files**
- `apps/desktop/src/lib/file-explorer/CLAUDE.md` — update Selection section: add "Operation lifecycle" subsection
  documenting snapshot, diff-driven adjustment, clear-on-complete
- `apps/desktop/src-tauri/src/file_system/write_operations/CLAUDE.md` — add `write-source-item-done` to events table
- `apps/desktop/src-tauri/src/file_system/listing/CLAUDE.md` — document `find_file_indices`

**5.2 Add gotchas to file-explorer CLAUDE.md**
- `allSelected` + cancel: calls `selectAll()` for move/delete/trash, leaves untouched for copy
- Both-panes-same-directory: only source pane selection is adjusted
- Snapshot timing: must happen at confirm, not when progress dialog opens (same-FS moves are instant)
- Snapshot must cover clipboard paste path (`startTransferProgress`), not just `handleTransferConfirm`
- Hidden files toggle during operation: if user toggles hidden files off, hidden files in the snapshot will silently
  lose their selection (extreme edge case, acceptable)
- Pane navigation during operation: if user navigates away from the source directory while an operation runs,
  `operationSelectedNames` persists but `findFileIndices` finds nothing (harmless). On complete, `clearSelection()`
  clears the new directory's selection, which is wrong but unlikely (the progress dialog covers most of the UI)
- `allSelected` + gradual deselection: with `allSelected`, there is zero gradual deselection during the operation
  (the `'all'` sentinel skips per-diff/per-item adjustments). For a long copy of 200k files, the selection visually
  doesn't change until complete/cancel. This is acceptable — the alternative (tracking 200k names) is too expensive.
- Cross-FS move: during the copy-to-staging phase, `write-source-item-done` fires but the source file hasn't been
  deleted yet. During the delete-sources phase, both mechanism 1 (diff-driven) and mechanism 2 (source-item-done) may
  fire for the same item. The diff handler replaces the entire selection, which could momentarily re-add an item that
  mechanism 2 just deselected. This self-corrects on the next diff and is not visually noticeable.

## What NOT to do

- **Don't switch to path-based selection.** 200k paths at ~100 bytes = ~20MB in the selection set, plus serialization
  overhead on the IPC pipe. Indices are 10-20x more memory-efficient.
- **Don't maintain a parallel file array in the frontend.** The non-reactive `FileDataStore` pattern exists specifically
  to avoid Svelte tracking 20k+ items. Duplicating the backend cache defeats this.
- **Don't add index fields to `DiffChange`.** The backend replaces the entire listing vector on diff. The concept of "old
  index" doesn't exist cleanly. And it would couple the watcher to the selection system.
- **Don't try to handle move rollback re-selection.** Move operations emit `rolled_back: false` always. There's no
  "files reappearing in source" scenario.
- **Don't use `write-progress.files_done` to track top-level source completion.** `files_done` counts recursive files,
  not top-level source items. A directory with 500 files counts as 500.
- **Don't prune `operationSelectedNames` on diff.** Removing names that aren't found risks losing track of files that
  temporarily disappear across debounce windows. Re-querying absent names is cheap.
- **Don't try to deselect during the scan phase.** `scan-progress` events don't correspond to actual file operations.
- **Don't detect source boundaries by comparing paths in iteration order.** `scan_result.files` is sorted by the user's
  sort column, interleaving files from different sources. Use per-source counters instead.
- **Don't make `diffGeneration` reactive (`$state`).** It's only used inside async callbacks to discard stale results,
  never for rendering. Making it `$state` would cause unnecessary re-renders.

## Testing strategy

- **Unit tests (Vitest)**: pure functions for selection adjustment, hasParent offset, generation counter logic
- **Rust unit tests**: `find_file_indices` correctness, `write-source-item-done` emission boundaries (especially with
  sorted/interleaved scan results)
- **Manual testing**: all scenarios in the scenario table above, plus edge cases (select-all + delete, scattered
  selection + cancel, operation during sort change, both panes same directory, clipboard paste)
- **E2E**: not needed for this change — the MCP-based manual testing covers the interaction adequately

## Execution notes

- Milestones are sequential. Each one is independently shippable and improves the situation.
- Milestone 1 alone fixes the most visible bug. Milestones 2-4 add the gradual deselection polish.
- Milestone 4 is the most involved (new backend event + emission from 5 operation types with per-source counters). If
  scope needs to be cut, skip milestone 4 and rely on clear-on-complete for copy operations and diff-driven adjustment
  for move/delete/trash. The gradual deselection from milestone 4 is visual polish, not correctness.
