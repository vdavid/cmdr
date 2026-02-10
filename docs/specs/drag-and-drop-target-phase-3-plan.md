# Drag-and-drop target: phase 3 plan

Phase 3 of drag-and-drop target support. See [drag-and-drop-target-spec.md](drag-and-drop-target-spec.md) and
[drag-and-drop-target-thinking.md](drag-and-drop-target-thinking.md) for context.

Phases 1 (pane-level drop) and 2 (folder-level targeting) are complete and working.

## A. Custom outbound drag image (canvas to PNG)

When dragging files OUT of the app, replace the current single-file icon with a rich rendered image:

- Dark semi-transparent card with rounded corners
- File/folder names with emoji icons, smart middle-truncation (preserve extension)
- 12 or fewer files: show all names. More than 12: show first 8 + "and N more"
- Total file count at bottom
- All edges fade to alpha=0 via canvas gradient compositing
- Retina-aware (2x devicePixelRatio)
- Performant: `names.slice(0, 12)` + `names.length` — no iteration of 50k paths

New file: `drag-image-renderer.ts` in `src/lib/file-explorer/`.
Modifications to `drag-drop.ts`: use rendered canvas image instead of the simple icon.
Need to pipe file names from FilePane into the drag functions.

## B. In-app floating overlay (inbound + pane-to-pane drags)

When files are dragged INTO our window (from external apps or from the other pane), show a DOM overlay near cursor:

- Same visual style as the drag image (semi-transparent dark card, fading edges via CSS `mask-image`)
- File names: 20 or fewer: show all. More than 20: show first 10 + "and N more"
- Action line at bottom: "Copy to Documents" / "Move to Documents" / "Can't drop here"
- Positioned 16px right + 16px below cursor
- CSS fade-in/out transitions
- Updates on every `over` event (target name changes as cursor moves)

New files: `DragOverlay.svelte` and `drag-overlay.svelte.ts` in `src/lib/file-explorer/`.
Modifications to `DualPaneExplorer.svelte`: render overlay, manage state, pass data.

## C. Modifier key tracking

- Track Alt/Option via `keydown`/`keyup` on document
- Alt held: operation switches to "Move" (overlay label updates live)
- State consumed on `drop` to set default operation in transfer dialog
- New file: `modifier-key-tracker.svelte.ts` in `src/lib/file-explorer/`

## D. "Not allowed" feedback

When cursor is over toolbar, status bar, or between panes during a drag:

- Overlay shows dimmed/grayed state with "Can't drop here"
- No pane/folder highlight active

Handled within the DragOverlay component logic.

## E. Testing

- Unit tests for `drag-image-renderer.ts` (name truncation, count formatting, edge cases)
- Unit tests for `drag-overlay.svelte.ts` (state management)
- Manual testing with MCP servers
- Run `./scripts/check.sh --svelte` after implementation

## Task list

- [x] Create `drag-image-renderer.ts` with canvas rendering, middle-truncation, fading edges
- [x] Modify `drag-drop.ts` to use rendered drag image instead of simple icon
- [x] Pipe file names from FilePane/DualPaneExplorer into drag functions
- [x] Create `drag-overlay.svelte.ts` reactive state module
- [x] Create `DragOverlay.svelte` component
- [x] Create `modifier-key-tracker.svelte.ts`
- [x] Integrate overlay + modifier tracking into `DualPaneExplorer.svelte`
- [x] Add "not allowed" feedback for non-droppable areas
- [x] Write unit tests for drag-image-renderer
- [x] Write unit tests for drag-overlay state
- [x] Add new files to `coverage-allowlist.json` as needed
- [x] Run `./scripts/check.sh --svelte` and fix any issues
