# Drag and drop

Handles native drag-and-drop for files: dragging out to other apps, receiving drops from external sources, and
pane-to-pane drags.

## Architecture

### Drag-out (initiating drags)

Two backend commands, both routed through `native_drag.rs` so the pasteboard payload is identical:

- **Single file** → `start_drag_paths` (frontend passes the file path directly)
- **Multi-file selection** → `start_selection_drag` (backend resolves paths from the listing cache, avoiding IPC
  overhead for large selections)

Pasteboard layout per drag (one `NSPasteboardItem` per file):

- Every item: `public.file-url` (the URL's `absoluteString`) — Finder, IntelliJ, etc. iterate items reading this.
- First item only: `public.utf8-plain-text` with the full shell-escaped path list joined by spaces — terminals (Warp,
  etc.) read this via `pasteboard.string(forType:)` and insert the text at the cursor.
- Later items: `public.utf8-plain-text` with their own escaped path (so item-iterating consumers don't see duplicates).
- First item only: `NSFilenamesPboardType` (legacy `NSArray<NSString>` of all paths). Required for stock wry's
  `collect_paths`, which reads only this type and `unwrap()`s if absent — see
  [wry#1723](https://github.com/tauri-apps/wry/pull/1723) for the upstream fix. Drop this once wry ships a release
  containing the fix and we bump our `tauri-runtime-wry`.

Source operation mask: permissive (`Copy | Link | Generic | Move`). macOS arbitrates the actual operation via modifier
keys (Alt → Copy, Cmd → Move, Ctrl-Alt → Link) and destination preference. Restricting the mask to a single op breaks
terminals (which only accept Copy).

Key files:

- `drag-drop.ts` — Mouse tracking, threshold detection, drag initiation
- `drag-image-renderer.ts` — Canvas rendering for rich OS drag preview (retina-aware, fading edges)
- `commands/file_system/drag.rs` — `start_drag_paths` and `start_selection_drag` Tauri commands; both hop to the AppKit
  main thread before calling into `native_drag::start_drag`
- `native_drag.rs` — Builds `NSPasteboardItem`s as above, wraps each in an `NSDraggingItem`, and begins the dragging
  session via a custom `CmdrDragSource` that returns the permissive op mask

### Drop-in (receiving drops)

Uses Tauri 2's `onDragDropEvent` (window-level) with DOM hit-testing to resolve target pane/folder.

Key files:

- `drop-target-hit-testing.ts` — Pure logic: `document.elementFromPoint()` + `data-drop-target-path` walk
- `drop-target-validation.ts` — Pure logic: blocks drops onto the source itself or into a descendant
- `DragOverlay.svelte` + `drag-overlay.svelte.ts` — Floating label near cursor
- `../modifier-key-tracker.svelte.ts` — Alt/Option state (DragDropEvent doesn't include modifiers; lives in parent
  `file-explorer/` directory)
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

- **Decision**: Always show confirmation dialog on drop
  - **Why**: Drag-and-drop is imprecise. Default operation is copy (safer than move).
- **Decision**: Same-pane pane-level drops are no-ops
  - **Why**: Dropping onto a subfolder within the same pane is valid.
- **Decision**: Block drops onto the source itself or into a descendant
  - **Why**: Dragging `/a/b` onto `/a/b` or into `/a/b/c` can't produce a sensible result. Invalid targets don't
    highlight and show "Can't drop here" in the overlay, matching the pre-existing same-pane no-op behavior. The check
    applies to both folder-row and pane-level targets, and covers external drags too.
- **Decision**: The ".." row is a regular folder drop target pointing to the parent path
  - **Why**: Gives users a quick "drop into parent" gesture. The source-descendant check exempts it naturally (the
    ancestor isn't a descendant of its children).
- **Decision**: Rich PNG drag image for external visibility, transparent 1x1 inside window
  - **Why**: Self-drags swap images mid-drag via `setDraggingFrame:contents:` (entered → transparent, exited → rich).
    DOM overlay provides feedback inside.
- **Decision**: Custom `native_drag.rs` instead of the upstream `drag` crate
  - **Why**: The upstream crate writes only `public.file-url` per item and uses a single-op mask (Move OR Copy). That
    failed three ways: (1) terminals like Warp listen for `public.utf8-plain-text`, not file URLs, so drops were
    silently dropped; (2) wry's `collect_paths` reads `NSFilenamesPboardType` and panics if the auto-derivation fails —
    see [wry#1723](https://github.com/tauri-apps/wry/pull/1723) for the upstream fix; (3) terminals only accept
    `NSDragOperationCopy`, so a Move-only source mask makes them reject the drop entirely. Our version advertises
    file-URL + shell-escaped text + legacy filenames per the layout above, and publishes a permissive op mask.
    Finder/IntelliJ behavior is unchanged (they read file URLs); terminals get working text drops; macOS modifier keys
    arbitrate the operation natively.
- **Decision**: Modifier keys tracked via NSEvent.modifierFlags
  - **Why**: Tauri doesn't expose modifier state in DragDropEvent. Emits `drag-modifiers` event only when state changes.
- **Decision**: Viewport position correction only in dev mode
  - **Why**: DevTools docked mode shrinks viewport but Tauri reports window-relative positions. Offset computed via
    `outerSize()` vs `innerHeight`. Zero overhead in prod.

## Gotchas

- **Gotcha**: `startDrag()` resolves before macOS delivers drag events
  - **Why**: Self-drag state (rich image path, active flag) must NOT be cleared from async JS after `startDrag`. Only
    cleared on drop/leave via `endSelfDragSession()`. Temp PNG files survive entire drag session (cleanup deferred to
    `pendingImageCleanup`).
- **Gotcha**: Swizzle must catch panics across FFI boundary
  - **Why**: All native calls wrapped in `catch_unwind` + `warn_once` to prevent crashes mid-drag. Gracefully degrades
    if wry renames `WryWebView` or Apple removes deprecated APIs.
- **Gotcha**: `setDraggingFrame:contents:` modifications persist globally
  - **Why**: Transparent swap in `draggingEntered:` would remain visible outside without swap-back in `draggingExited:`.
    Session-level image (from `startDrag`) is separate.
- **Gotcha**: Re-entry detection uses fingerprint (count + first 5 paths)
  - **Why**: O(1) check on drag enter, avoids iterating 50k+ paths. Restores `isDraggingFromSelf` flag if match.
- **Gotcha**: Icon loading is async
  - **Why**: Canvas renderer preloads all icons in parallel. Falls back to geometric shapes (filled rect = file, open
    rect = folder) on cache miss.
- **Gotcha**: Middle-truncation preserves extensions
  - **Why**: `"very-long-filename.txt"` → `"very-lon…me.txt"`. Splits basename, keeps extension intact.
- **Gotcha**: External drags with large images suppress overlay
  - **Why**: If source preview is > 32x32, hide Cmdr's overlay. Self-drags always show overlay (OS image is transparent
    inside).
- **Gotcha**: Drag image shows 12 names max, overlay shows 20 max
  - **Why**: Different limits: canvas is fixed-size retina-aware, DOM is flexible. Truncation: first 8/10 + "and N
    more".
- **Gotcha**: `public.file-url` must be set with `setString:` not `setPropertyList:`
  - **Why**: Setting it via the property-list path (e.g., from `NSURL.pasteboardPropertyListForType:`) produces a value
    AppKit can't parse back into a URL — logs "An invalid URL was found on the pasteboard" and breaks downstream
    derivations like `NSFilenamesPboardType`. Use `[item setString: url.absoluteString forType: "public.file-url"]`.
- **Gotcha**: Stock wry panics on self-drag re-entry without `NSFilenamesPboardType`
  - **Why**: `wry-0.54.x::wkwebview::drag_drop::collect_paths` reads only the legacy `NSFilenamesPboardType` and
    `unwrap()`s. AppKit advertises the type as "available" via auto-derivation but fails to produce the property list,
    hitting the unwrap. We always publish `NSFilenamesPboardType` explicitly on the first dragging item.
    [wry#1723](https://github.com/tauri-apps/wry/pull/1723) fixes this upstream by switching `collect_paths` to
    `readObjectsForClasses:[NSURL]options:`. Once that PR lands and the fix ships in a wry release we consume, drop our
    `NSFilenamesPboardType` publishing.
- **Gotcha**: Source op mask must include the destination's required operation
  - **Why**: The destination's `draggingEntered:` is constrained to the source's mask. Terminals only accept
    `NSDragOperationCopy`; if the source publishes Move-only, the drop is rejected and the drag animates back. Publish a
    permissive mask (`Copy | Link | Generic | Move`) and let macOS modifier keys arbitrate.

## Platform support

macOS only. Backend commands and swizzle gated `#[cfg(target_os = "macos")]`. Drop receiving uses cross-platform Tauri
API but untested on other platforms. `drag-image-size` event never fires on non-macOS (frontend defaults to showing
overlay).
