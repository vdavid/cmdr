# Drag and drop

Cmdr supports native drag-and-drop for files: dragging files out to other apps (Finder, other file managers), receiving
files dropped from external sources, and dragging between the two panes.

## User interaction

### Dragging files out

1. **Click and hold** on a file or selection
2. **Move the mouse** past the drag threshold (5 pixels)
3. **Drop** on target application or folder

**Modifier keys:**

- **Hold Alt/Option** while dragging = copy mode
- **No modifier** = move mode (matches Finder behavior)

**Cancel drag:**

- Press **Escape** before releasing
- Release mouse before crossing threshold

**Single file vs selection:**

- **No selection**: Dragging a file selects it first, then initiates drag
- **Existing selection**: Dragging from within the selection drags all selected files

**Drag preview:**

A rich canvas-rendered image shows file/folder names with emoji icons and smart middle-truncation (preserving the
extension). Up to 12 names are shown; beyond that, the first eight plus "and N more". The image has semi-transparent
dark background with rounded corners and fading edges, rendered at 2x for Retina displays. See
`drag-image-renderer.ts`.

### Dropping files in

Files can be dropped into Cmdr from external apps (Finder, etc.) or from the other pane.

**Pane-level drop:** Drop onto a pane to copy/move files into that pane's current directory.

**Folder-level drop:** Hover over a directory entry within a pane — it highlights, and dropping there uses that folder
as the destination instead of the pane's current directory. The `..` entry is excluded.

**Modifier keys:**

- **Hold Alt/Option** while dragging over Cmdr = move mode (overlay label updates live)
- **No modifier** = default copy

**Floating overlay:** A positioned label near the cursor shows "Copy N files to FolderName" (or "Move..." with Alt
held). Over non-droppable areas (toolbar, status bar), it shows "Can't drop here" in a dimmed state. See
`DragOverlay.svelte`.

**On drop:** The standard transfer confirmation dialog opens (same as F5/F6), with full progress display, ETA, speed,
and conflict resolution.

**Same-pane behavior:**

- Dropping onto the same pane (pane-level) is a no-op
- Dropping onto a subfolder within the same pane works (moves/copies into that folder)

## Implementation

### Architecture

Dragging out uses two code paths for performance:

| Scenario             | Implementation                               | Reason                                  |
|----------------------|----------------------------------------------|-----------------------------------------|
| Single file          | Frontend via `@crabnebula/tauri-plugin-drag` | Simple, direct                          |
| Multi-file selection | Backend Rust command                         | Avoids transferring file paths over IPC |

Dropping in uses Tauri 2's `onDragDropEvent` at the window level with DOMRect hit-testing to determine which pane and
which directory entry the cursor is over.

### Key modules

**Dragging out:**

- `drag-drop.ts` — Mouse tracking, drag threshold, modifier keys, code path selection. Entry point:
  `startSelectionDragTracking()`. Also exports `isDraggingFromSelf` flag for internal drag detection.
- `drag-image-renderer.ts` — Canvas-based drag preview rendering (retina-aware, fading edges, truncation).
- `file_system.rs` — `start_selection_drag` command: resolves paths from `LISTING_CACHE`, calls `drag::start_drag()` on
  the main thread (required by macOS).

**Dropping in:**

- `drop-target-hit-testing.ts` — Pure logic for pane-level and folder-level hit-testing. Uses
  `document.elementFromPoint()` and `data-drop-target-path` attributes on directory rows.
- `DualPaneExplorer.svelte` — Registers `onDragDropEvent` listener. Handles enter/over/leave/drop events, manages pane
  and folder highlights, wires drops to the transfer dialog.
- `DragOverlay.svelte` + `drag-overlay.svelte.ts` — Floating overlay component and its reactive state.
- `modifier-key-tracker.svelte.ts` — Tracks Alt/Option state via `keydown`/`keyup` (since `DragDropEvent` doesn't
  include modifier state).
- `transfer-operations.ts` — `buildTransferPropsFromDroppedPaths()` builds transfer dialog props from dropped file
  paths.
- `drag-position.ts` — Corrects Tauri `DragDropEvent` coordinates for the viewport. Tauri reports positions relative to
  the full window frame, but when the Web Inspector (DevTools) is docked, the viewport is smaller. Uses
  `getCurrentWindow().outerSize()` (full window, stable) vs `window.innerHeight` (viewport, shrinks with DevTools) to
  compute and apply the offset. Without DevTools, the offset is zero.

### Drag image detection (hack — wry swizzle)

> **This is a workaround.** We submitted a feature request to add drag image size to wry's `DragDropEvent` natively:
> [wry#1669](https://github.com/tauri-apps/wry/issues/1669). If accepted, the swizzle can be removed.

When files are dragged into Cmdr, the frontend needs to know if the source app provides a meaningful drag preview (like
Finder's file thumbnails) or a tiny/blank image. If the source already shows a good preview, Cmdr hides its own overlay
to avoid visual clutter. If the preview is small or missing, Cmdr shows the rich DOM overlay instead.

**The problem:** Tauri's `DragDropEvent` doesn't include drag image dimensions. The deprecated `draggedImage()` API
returns `nil` for cross-process drags (since macOS 10.12) — exactly our main use case.

**The solution:** Method swizzling on wry's `WryWebView` class. On app startup, `drag_image_detection.rs` replaces
`WryWebView`'s `draggingEntered:` method with a custom implementation that:

1. Calls `enumerateDraggingItems` on the `NSDraggingInfo` to read each `NSDraggingItem`'s `draggingFrame`
2. Computes the union bounding box of all frames
3. Emits a `drag-image-size` Tauri event with `{ width, height }`
4. Forwards to the original implementation so wry's normal handling continues

The swizzled method runs before wry emits the `DragDropEvent::Enter`, so the size event arrives first — no race
condition.

**Dependencies added for the swizzle:** `objc2`, `objc2-foundation`, `objc2-app-kit` (with `NSDragging`,
`NSDraggingItem`, `NSPasteboard`, `NSView`, `NSResponder` features), and `block2`.

**Risks:**

- Wry renaming `WryWebView` would break the swizzle (mitigated: logs a warning and degrades gracefully)
- Not App Store compatible (Cmdr is distributed directly)
- `enumerateDraggingItems` may return zero items for some apps (falls back to deprecated `draggedImage()`, then 0x0)

**Files:** `drag_image_detection.rs` (macOS-only, `#[cfg(target_os = "macos")]`), wired up in `lib.rs` setup.

## Platform support

**Current status**: macOS only.

The backend drag commands and the drag image detection swizzle are gated with `#[cfg(target_os = "macos")]`. Drop
receiving works via Tauri's cross-platform `onDragDropEvent` API, but has only been tested on macOS. On non-macOS, the
`drag-image-size` event never fires, and the frontend defaults to showing its own overlay.

### Cross-platform strategy (future)

Platform-specific implementations are recommended over a unified abstraction layer, because native feel matters for a
file manager's core interaction.

| Platform    | Native features worth preserving                                           |
|-------------|----------------------------------------------------------------------------|
| **macOS**   | Spring-loaded folders, drag promises, Finder-style visual feedback         |
| **Windows** | Shell drag images with thumbnails, drop descriptions, Explorer integration |
| **Linux**   | Desktop-environment-specific behaviors (Nautilus vs Dolphin vs Thunar)     |

**Recommended approach:**

1. Ship Linux/Windows support using `tauri-plugin-drag` for quick cross-platform drag
2. Keep native macOS implementation (already more polished)
3. Replace with native implementations per-platform based on user feedback

## Testing

### Automated tests

Unit tests cover:

- Hit-testing logic (pane-level and folder-level targeting with various cursor positions)
- Transfer props building from dropped paths
- Drag image renderer (name truncation, count formatting, edge cases)
- Drag overlay state management

### Manual testing

**Dragging out:**

1. **Single file drag**: Click and drag an unselected file to Finder
2. **Selection drag**: Select multiple files, drag to Finder — verify rich preview image
3. **Copy mode**: Hold Alt/Option while dragging, verify files are copied not moved
4. **Cancel**: Start drag, press Escape, verify no operation occurs
5. **Threshold**: Click and release without moving, verify no drag starts

**Dropping in:**

1. **Pane-level drop**: Drag files from Finder onto each pane, verify dialog appears
2. **Folder-level drop**: Hover over a directory entry, verify highlight, drop and verify destination
3. **Pane-to-pane**: Drag from one pane to the other, verify dialog. Same-pane drop should be a no-op
4. **Modifier keys**: Hold Alt while dragging over Cmdr, verify overlay says "Move"
5. **Overlay**: Verify floating label tracks cursor, updates text on target change, fades on leave
6. **Non-droppable area**: Drag over toolbar, verify "Can't drop here" feedback

Native OS APIs make full automated drag-and-drop testing infeasible. Playwright e2e tests don't cover drag operations
due to WebDriver limitations on macOS.
