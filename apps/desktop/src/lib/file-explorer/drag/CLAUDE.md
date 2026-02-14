# Drag and drop

Handles native drag-and-drop for files: dragging out to other apps, receiving drops from external sources, and
pane-to-pane drags.

## Architecture

### Drag-out (initiating drags)

Two code paths for performance:

- **Single file**: Frontend `@crabnebula/tauri-plugin-drag` — simple, direct
- **Multi-file selection**: Backend Rust command — avoids IPC overhead for large selections

Key files:

- `drag-drop.ts` — Mouse tracking, threshold detection, drag initiation
- `drag-image-renderer.ts` — Canvas rendering for rich OS drag preview (retina-aware, fading edges)
- `file_system.rs` (`start_selection_drag` command) — Resolves paths from cache, dispatches to main thread

### Drop-in (receiving drops)

Uses Tauri 2's `onDragDropEvent` (window-level) with DOM hit-testing to resolve target pane/folder.

Key files:

- `drop-target-hit-testing.ts` — Pure logic: `document.elementFromPoint()` + `data-drop-target-path` walk
- `DragOverlay.svelte` + `drag-overlay.svelte.ts` — Floating label near cursor
- `modifier-key-tracker.svelte.ts` — Alt/Option state (DragDropEvent doesn't include modifiers)
- `drag-position.ts` — Corrects Tauri coords for docked DevTools (dev-only, zero overhead in prod)
- Integration in `DualPaneExplorer.svelte`

### Drag image detection (macOS-specific hack)

**Problem**: Tauri's `DragDropEvent` doesn't include drag image size. Need size to decide whether to suppress Cmdr's
overlay (large source preview) or show it (tiny/no preview).

**Solution**: Method swizzling on `WryWebView`'s `draggingEntered:`. Reads `NSDraggingItem.draggingFrame()` via
`enumerateDraggingItems`, emits `drag-image-size` event. Feature request to wry:
[wry#1669](https://github.com/tauri-apps/wry/issues/1669). If accepted, swizzle can be removed.

Key files:

- `drag_image_detection.rs` — Swizzle install + `draggingEntered:`/`draggingUpdated:`/`draggingExited:` overrides
- `drag_image_swap.rs` — Image swapping logic for self-drags (transparent inside window, rich outside)

## Key decisions

**Decision**: Always show confirmation dialog on drop **Why**: Drag-and-drop is imprecise. Default operation is copy
(safer than move).

**Decision**: Same-pane pane-level drops are no-ops **Why**: Dropping onto a subfolder within the same pane is valid.

**Decision**: Rich PNG drag image for external visibility, transparent 1x1 inside window **Why**: Self-drags swap images
mid-drag via `setDraggingFrame:contents:` (entered → transparent, exited → rich). DOM overlay provides feedback inside.

**Decision**: Modifier keys tracked via NSEvent.modifierFlags **Why**: Tauri doesn't expose modifier state in
DragDropEvent. Emits `drag-modifiers` event only when state changes.

**Decision**: Viewport position correction only in dev mode **Why**: DevTools docked mode shrinks viewport but Tauri
reports window-relative positions. Offset computed via `outerSize()` vs `innerHeight`. Zero overhead in prod.

## Gotchas

**Gotcha**: `startDrag()` resolves before macOS delivers drag events **Why**: Self-drag state (rich image path, active
flag) must NOT be cleared from async JS after `startDrag`. Only cleared on drop/leave via `endSelfDragSession()`. Temp
PNG files survive entire drag session (cleanup deferred to `pendingImageCleanup`).

**Gotcha**: Swizzle must catch panics across FFI boundary **Why**: All native calls wrapped in `catch_unwind` +
`warn_once` to prevent crashes mid-drag. Gracefully degrades if wry renames `WryWebView` or Apple removes deprecated
APIs.

**Gotcha**: `setDraggingFrame:contents:` modifications persist globally **Why**: Transparent swap in `draggingEntered:`
would remain visible outside without swap-back in `draggingExited:`. Session-level image (from `startDrag`) is separate.

**Gotcha**: Re-entry detection uses fingerprint (count + first 5 paths) **Why**: O(1) check on drag enter, avoids
iterating 50k+ paths. Restores `isDraggingFromSelf` flag if match.

**Gotcha**: Icon loading is async **Why**: Canvas renderer preloads all icons in parallel. Falls back to geometric
shapes (filled rect = file, open rect = folder) on cache miss.

**Gotcha**: Middle-truncation preserves extensions **Why**: `"very-long-filename.txt"` → `"very-lon…me.txt"`. Splits
basename, keeps extension intact.

**Gotcha**: External drags with large images suppress overlay **Why**: If source preview is > 32x32, hide Cmdr's
overlay. Self-drags always show overlay (OS image is transparent inside).

**Gotcha**: Drag image shows 12 names max, overlay shows 20 max **Why**: Different limits: canvas is fixed-size
retina-aware, DOM is flexible. Truncation: first 8/10 + "and N more".

## Platform support

macOS only. Backend commands and swizzle gated `#[cfg(target_os = "macos")]`. Drop receiving uses cross-platform Tauri
API but untested on other platforms. `drag-image-size` event never fires on non-macOS (frontend defaults to showing
overlay).
