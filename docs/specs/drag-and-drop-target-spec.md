# Drag-and-drop target

Make both panes (Full and Brief views) accept file drops from external apps and from the other pane.

See [drag-and-drop-target-thinking.md](drag-and-drop-target-thinking.md) for research, rationale, and sources.

## How it works

Tauri 2's `onDragDropEvent` (from `@tauri-apps/api/webview`) fires at the **window** level with these events:

| Event   | Data                              | When                          |
|---------|-----------------------------------|-------------------------------|
| `enter` | `paths: string[]`, `position`     | Files enter the webview       |
| `over`  | `position`                        | Cursor moves while dragging   |
| `drop`  | `paths: string[]`, `position`     | User releases the mouse       |
| `leave` | (nothing)                         | Cursor leaves window or Esc   |

Since events fire at window level (not per-element), we use **DOMRect hit-testing** to determine which pane (and later, which directory entry) the cursor is over.

Both external drops (from Finder, etc.) and internal drops (pane-to-pane) arrive through the same `onDragDropEvent` API with file paths. The only difference is UX: for internal drops, we can detect the source via a module-level `isDraggingFromSelf` flag set in the existing drag-out code (`drag-drop.ts`).

On drop, the existing transfer infrastructure handles the operation:
- `openTransferDialog()` in `DualPaneExplorer.svelte` triggers confirmation
- `TransferDialog.svelte` shows source/destination and scan preview
- `TransferProgressDialog.svelte` shows progress, ETA, speed, and conflicts

## Key decisions

- **Always show the confirmation dialog on drop.** Drag-and-drop is imprecise — the user should confirm before files move. The dialog also lets the user switch between copy and move.
- **Default operation:** Copy. Unlike Finder's "move on same volume" convention, copy is safer for an imprecise gesture. The confirmation dialog makes switching easy.
- **Same-pane drops are no-ops.** If the user drops onto the same pane they dragged from (and not onto a subfolder), nothing happens.
- **Only file paths are handled.** Tauri intercepts native OS drags before the browser sees them and only provides file paths. Text/image drags from browsers may arrive as temp files, which is fine — they get copied like any other file.

## Existing code to build on

| What | Where |
|------|-------|
| Drag-out (native drag initiation) | `src/lib/file-explorer/drag-drop.ts` — `startSelectionDragTracking()`, `performSelectionDrag()` |
| Transfer dialog trigger | `DualPaneExplorer.svelte` — `openTransferDialog('copy'\|'move')` |
| Transfer confirmation | `src/lib/file-operations/transfer/TransferDialog.svelte` |
| Transfer progress | `src/lib/file-operations/transfer/TransferProgressDialog.svelte` |
| Virtual scroll math | `src/lib/file-explorer/views/virtual-scroll.ts` — `calculateVirtualWindow()` |
| Pane views | `FullList.svelte` (vertical scroll), `BriefList.svelte` (horizontal scroll) |

---

## Phase 1: Pane-level drop with confirmation

The core feature: drop files onto a pane, get the standard copy confirmation and progress UI.

### Behavior

1. Register `onDragDropEvent` listener on the webview (in `DualPaneExplorer.svelte` or a new `drop-target.ts` module).
2. On `enter`/`over`: hit-test cursor position against left and right pane bounding rects.
   - If over a pane: highlight that pane's border/background with a subtle glow (CSS class toggle, no layout shift).
   - If not over either pane: no highlight.
3. On `leave`: clear all highlights.
4. On `drop`:
   - Determine target pane from cursor position.
   - If no valid pane target, ignore.
   - If same pane as drag source (internal drag), ignore (no-op).
   - Otherwise: call `openTransferDialog('copy')` with the dropped `paths` as source and the target pane's current directory as destination.
5. Set `isDraggingFromSelf = true` in `drag-drop.ts` when an outbound drag starts, clear on drag end. On drop, if `isDraggingFromSelf`, pre-fill the dialog with the known source pane context.

### Visual feedback

- Pane highlight: a colored border or background tint, applied via a CSS class (for example, `drop-target-active`). Must not shift layout — use `outline` or `box-shadow`, not `border`.
- No floating labels or cursor changes in this phase.

### Edge cases

- Dropping on toolbar, status bar, or between panes → ignored, no highlight.
- Dropping with no files (shouldn't happen via Tauri, but guard against empty `paths`).
- Multiple rapid enter/leave events → debounce or use latest-wins for highlight state.

### Testing

- Vitest: unit test hit-testing logic (given pane rects and cursor position, which pane is targeted?).
- Manual test with MCP: drag files from Finder onto left/right pane, verify dialog appears with correct paths.
- Manual test: drag from pane A to pane B, verify dialog appears. Drag from pane A back to pane A, verify no-op.

---

## Phase 2: Folder-level targeting

Drop onto a directory entry to copy/move files into that specific folder instead of the pane's current directory.

### Behavior

1. On `over`: after pane-level hit-testing, do **row-level hit-testing** within the target pane.
   - Use virtual scroll offset + row height to compute which row index the cursor is over.
   - If that row is a directory entry: highlight that row (outline/glow, no layout shift) and use its path as the drop target.
   - If that row is a file or the cursor is between rows: fall back to pane-level highlight and use the pane's current directory.
2. On `drop`: use the resolved target (specific folder path, or pane current directory).

### Visual feedback

- Directory entry highlight: subtle outline or background tint on the hovered row. Must not shift content or affect virtual scroll positioning.
- When a directory row is highlighted, the pane-level highlight should be suppressed (only one highlight active at a time).

### Hit-testing details

- `FullList.svelte` (vertical): row index = `Math.floor((cursorY - paneTop + scrollOffset) / rowHeight)`
- `BriefList.svelte` (horizontal): needs column + row calculation based on grid layout and scroll offset.
- Expose a function from each view (or from `virtual-scroll.ts`) like `getEntryAtPosition(cursorX, cursorY): FileEntry | null`.

### Edge cases

- Cursor on a directory row's border/gap → favor the directory (generous hit area).
- Fast mouse movement skipping rows → `over` events may skip rows; just highlight whatever the latest position resolves to.
- Scrolling while dragging → recalculate on each `over` event using current scroll offset.

### Testing

- Vitest: unit test row-level hit-testing with various scroll offsets, cursor positions, and grid layouts.
- Manual test: drag files from Finder, hover over directory entries, verify highlight changes. Drop onto a folder, verify dialog shows that folder as destination.

---

## Phase 3: Polish

Floating label, modifier keys, and refined UX.

### Floating drop label

Show a positioned label near the cursor during drag, such as "Copy 3 files here" or "Copy to Documents".

- Render a small absolutely-positioned element, updated on each `over` event.
- Position it offset from the cursor (for example, 16px right and 16px below) so it doesn't obscure the target.
- Content: `Copy {n} file(s) to {targetName}` — dynamically pluralized, showing the resolved target folder name.
- Fade in on `enter`, fade out on `leave`/`drop`.

### Modifier keys

Hold Alt/Option during drag to force a specific operation:
- **Alt held:** force move (label updates to "Move ...").
- **No modifier:** default copy.

Implementation: track Alt key state via `keydown`/`keyup` listeners on `document`. Read the state on `drop` and on each `over` (to update the floating label). Note that `DragDropEvent` doesn't include modifier state — this must be tracked separately.

### "Not allowed" visual feedback

When the cursor is over non-droppable areas (toolbar, status bar):
- Show a dimmed overlay or "no drop" icon in the floating label. Tauri doesn't give cursor control during native drags, so all feedback must be in-app.

### Internal drag refinements

- For internal pane-to-pane drags: show directional context in the floating label (for example, "Copy 3 files →" or "← Move to Documents").
- Skip the scan step in the confirmation dialog when dragging internally (we already know the files).

### Testing

- Manual test: verify floating label tracks cursor, updates text on target change, fades on leave.
- Manual test: hold Alt while dragging, verify label says "Move". Release Alt, verify it says "Copy".
- Manual test: drag over toolbar area, verify "not allowed" feedback.
