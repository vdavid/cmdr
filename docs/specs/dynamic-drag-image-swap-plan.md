# Dynamic drag image swapping for self-drags

## Context

When dragging files between panes (self-drag), Cmdr currently shows the PNG-based OS drag image
(canvas-rendered via `drag-image-renderer.ts`). External drops INTO Cmdr show a nicer DOM overlay
(`DragOverlay.svelte`) with live-updating target info, file icons, and operation labels.

The goal: show the DOM overlay when dragging inside the window, and the rich PNG when dragging outside
(to Finder, etc.). Swap dynamically as the cursor enters/exits the window.

## Approach

Use macOS's `NSDraggingItem.setDraggingFrame:contents:` to hide the OS drag image over our window:

1. Start the OS drag with the **rich PNG** (canvas-rendered, same as before)
2. On `draggingEntered:` → swap drag items to a **transparent 1x1 NSImage** (hides OS image
   over our window). The session-level image (rich PNG) remains visible outside all windows.
3. Show the **DOM overlay** for self-drags (previously suppressed)
4. On `draggingExited:` → no image swap needed; the OS automatically shows the session-level
   rich PNG when the cursor is outside all windows

Key insight: `setDraggingFrame:contents:` only affects rendering over a destination view,
not the "floating" session image shown outside all windows. So we start with the rich image
(visible outside) and swap to transparent only when over our window.

All new native code follows the existing hardening pattern: `catch_unwind` + `warn_once` + graceful
fallback to current behavior.

## Files to modify

### Rust

**`apps/desktop/src-tauri/src/drag_image_detection.rs`** — main changes:
- New statics: `SELF_DRAG_STATE: Mutex<Option<SelfDragState>>` (stores rich image path),
  `ORIGINAL_EXITED_IMP: OnceLock<Imp>`, `WARNED_EXITED_PANIC: AtomicBool`
- `install()`: add swizzle for `draggingExited:` (same pattern as entered/updated)
- New `swizzled_dragging_exited`: if self-drag state set → load rich image as NSImage →
  enumerate items → `setDraggingFrame:contents:` with rich image. Always call original.
- Modify `swizzled_dragging_entered`: after existing logic, if self-drag state set →
  enumerate items → set transparent NSImage (1x1 with no representations)
- New helpers: `swap_drag_items_to_image(drag_info, NSImage)`, `create_transparent_nsimage()`,
  `load_nsimage_from_path(path) -> Option<*mut AnyObject>`
- New public API: `set_self_drag_image_path(path: String)`, `clear_self_drag_state()`

**`apps/desktop/src-tauri/src/commands/file_system.rs`** — add two Tauri commands:
- `prepare_self_drag_overlay(rich_image_path: String)` → calls
  `drag_image_detection::set_self_drag_image_path()`
- `clear_self_drag_overlay()` → calls `drag_image_detection::clear_self_drag_state()`
- Both macOS-only with no-op stubs on other platforms

**`apps/desktop/src-tauri/src/lib.rs`** — register the two new commands in `invoke_handler`

**`apps/desktop/src-tauri/Cargo.toml`** — add `"NSImage"` feature to `objc2-app-kit`

### TypeScript

**`apps/desktop/src/lib/file-explorer/drag-drop.ts`**:
- New `writeTransparentPngToTemp()`: writes a hardcoded 1x1 transparent PNG (67 bytes) to temp dir
- Modify `performSingleFileDrag()`: render rich image as usual → call
  `prepareSelfDragOverlay(richPath)` → start drag with transparent PNG path
- Modify `performSelectionDrag()`: same pattern
- New `cleanupTransparentPng()` in the finally block
- New temp filename constant: `TEMP_TRANSPARENT_FILENAME = 'drag-transparent.png'`

**`apps/desktop/src/lib/tauri-commands/file-listing.ts`** — add `prepareSelfDragOverlay()`
and `clearSelfDragOverlay()` wrappers

**`apps/desktop/src/lib/tauri-commands/index.ts`** — re-export the two new functions

**`apps/desktop/src/lib/file-explorer/pane/DualPaneExplorer.svelte`**:
- `handleDragEnter()`: change overlay suppression from
  `getIsDraggingFromSelf() || externalDragHasLargeImage` to
  `externalDragHasLargeImage && !getIsDraggingFromSelf()`
  (self-drags always show overlay; external drags with large previews still suppressed)
- Drop handler: add `clearSelfDragOverlay()` call
- Leave handler: add `clearSelfDragOverlay()` call

## Key implementation details

### Transparent NSImage creation (Rust)
```rust
fn create_transparent_nsimage() -> Option<Id<NSImage>> {
    // NSImage initWithSize: with no representations = transparent
    msg_send_id![NSImage::alloc(), initWithSize: NSSize { width: 1.0, height: 1.0 }]
}
```

### Rich image loading (Rust)
```rust
fn load_nsimage_from_path(path: &str) -> Option<*mut AnyObject> {
    let ns_string: *mut AnyObject = msg_send![AnyClass::get(c"NSString")?, ...];
    let image: *mut AnyObject = msg_send![AnyClass::get(c"NSImage")?, alloc];
    let image: *mut AnyObject = msg_send![image, initWithContentsOfFile: ns_string];
    // null check
}
```

### Drag item image swapping (Rust)
Enumerate items via `enumerateDraggingItemsWithOptions:` (same pattern as `enumerate_dragging_frames`),
then call `setDraggingFrame:contents:` on each item with the new image and an appropriately sized frame.

Frame computation: use the NSImage's `size` property for dimensions. Position relative to the
cursor's `draggingLocation` from the `drag_info`.

### Transparent PNG bytes (TypeScript)
Hardcode the 67 bytes of a minimal valid 1x1 transparent PNG as a `Uint8Array` constant.

### draggingExited: signature
```
- (void)draggingExited:(id<NSDraggingInfo>)sender
```
Returns void, not `NSDragOperation`. Swizzle pattern is the same but with no return value.

## Event sequence for self-drag

1. User crosses drag threshold → `performSelectionDrag()` runs
2. Rich image rendered & written to temp → `prepareSelfDragOverlay(richPath)` called (await)
3. Transparent PNG written to temp → `startDrag({ icon: transparentPath })` called
4. macOS `draggingEntered:` fires → existing logic emits `drag-image-size` (tiny) →
   new logic swaps items to transparent (no-op, already transparent) → original impl called →
   Tauri emits `enter` event
5. Frontend `handleDragEnter()` → self-drag detected → overlay shown (new behavior)
6. User moves cursor outside window → macOS `draggingExited:` fires → new swizzle swaps
   items to rich PNG → user sees rich preview outside window
7. Cursor re-enters → `draggingEntered:` fires → swaps to transparent → overlay re-shown
8. Drop or cancel → cleanup: `clearSelfDragOverlay()` called, temp files cleaned

## Graceful degradation

- If `draggingExited:` swizzle fails to install → no image swapping, drag starts with transparent
  image. Inside: DOM overlay works fine. Outside: user sees tiny/invisible drag image. Functional
  but slightly degraded for the external-app case.
- If `setDraggingFrame:contents:` call fails → caught by `catch_unwind`, logged via `warn_once`,
  drag continues normally with whatever image was set.
- If `NSImage` class not found → swap skipped, logged. Same degradation as above.
- Net effect of any failure: behavior is equivalent to or slightly worse than current (never crashes).

## Verification

1. `cargo clippy` and `cargo nextest run` in `apps/desktop/src-tauri`
2. `./scripts/check.sh --svelte` for TypeScript/lint checks
3. Manual testing with MCP:
   - Self-drag between panes → should see DOM overlay (not PNG)
   - Drag files outside window → should see rich PNG preview
   - Drag back in → DOM overlay reappears
   - Drop on other pane → transfer dialog opens normally
   - External drag from Finder → behavior unchanged (overlay or no overlay based on image size)
   - Press Escape during self-drag → drag cancelled cleanly
