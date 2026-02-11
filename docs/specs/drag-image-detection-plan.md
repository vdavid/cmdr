# Drag image detection plan

Detect the incoming drag image size from external apps so the frontend can decide whether to show its own
rich DOM overlay (tiny/blank source image) or hide it (source already provides a meaningful preview like Finder).

## How Tauri 2 handles drag-and-drop on macOS today

Wry defines `WryWebView` (a `WKWebView` subclass) that implements `NSDraggingDestination` with four methods:
`draggingEntered:`, `draggingUpdated:`, `performDragOperation:`, `draggingExited:`.

Each receives `NSDraggingInfo`, extracts file paths and cursor position, then calls a `drag_drop_handler` closure.
Tauri converts those into `DragDropEvent::Enter/Over/Drop/Leave` events for `onDragDropEvent` on the frontend.

**The drag image size is never extracted.** We need to add that.

## The deprecated `draggedImage` problem

`NSDraggingInfo.draggedImage()` returns `nil` for **cross-process drags** starting macOS 10.12 — exactly the case we
care about (files from Finder). Dead end for our main use case.

## The modern API: `enumerateDraggingItems`

`enumerateDraggingItemsWithOptions_forView_classes_searchOptions_usingBlock` iterates `NSDraggingItem` objects, each
with `draggingFrame` (an `NSRect` with position and size). Union of all frames gives total drag image bounds.

For cross-process drags, `imageComponents` may use `CASlotProxy` objects (can't read pixel data), but `draggingFrame`
reliably provides **size**.

## Recommended approach: method swizzling on WryWebView

| Approach | Feasibility | Risk | Notes |
|----------|------------|------|-------|
| **Method swizzling** | High | Medium | Intercept `draggingEntered:`, read image size, forward |
| NSView subclass | Low | High | Can't replace wry's WryWebView |
| Tauri plugin | Low | High | Plugin API has no hook for NSDraggingInfo |
| Event monitor | Low | N/A | `NSEvent.addLocalMonitorForEvents` doesn't cover drags |
| Tauri command during drag | Low | N/A | No global accessor for current NSDraggingInfo |

**Swizzling is the only viable approach.** WryWebView already implements `NSDraggingDestination` (compiled into wry).
We can't use plugins, delegates, or event monitors because the drag info is only available inside the protocol methods.

## Implementation plan

### Step 1: New macOS-specific module

Create `apps/desktop/src-tauri/src/drag_image_detection.rs`, gated with `#[cfg(target_os = "macos")]`.

### Step 2: Dependencies (Cargo.toml)

Already have `objc2`, `objc2-foundation`, `objc2-app-kit`. Add features to `objc2-app-kit`:
`NSDragging`, `NSDraggingItem`, `NSPasteboard`, `NSImage`, `NSView`, `NSResponder`.
May need a direct `block2` dependency for the enumeration block.

### Step 3: Swizzle implementation

**A. Swizzle installation** (called once in `lib.rs` `setup` closure):

1. Get `WryWebView` class via `AnyClass::get("WryWebView")`
2. Get selector for `draggingEntered:`
3. Store original IMP in a static
4. Replace with our custom implementation via `class_replaceMethod`

**B. Custom `draggingEntered:`**:

1. Receives `&AnyObject` (self) + `&ProtocolObject<dyn NSDraggingInfo>` (drag info)
2. Calls `enumerateDraggingItems` on the drag info to iterate items
3. For each `NSDraggingItem`, reads `draggingFrame()` → `NSRect { origin, size }`
4. Computes union bounding box
5. Stores `(width, height)` in a thread-safe global
6. Emits Tauri event `drag-image-size` with `{ width, height }`
7. Calls original IMP to preserve wry's behavior

**C. Fallback**: if `enumerateDraggingItems` yields zero items, try `draggedImage()` (deprecated but works for
same-process). If that's also nil, emit `{ width: 0, height: 0 }` (unknown).

### Step 4: Data flow — Tauri event emission

```rust
app_handle.emit("drag-image-size", DragImageSize { width, height });
```

The swizzled `draggingEntered:` runs before wry's handler emits the Tauri `enter` event, so the size arrives first.
No polling needed.

### Step 5: Frontend consumption

In `DualPaneExplorer.svelte`, listen for `drag-image-size`:

```typescript
listen<{ width: number; height: number }>('drag-image-size', (event) => {
    const { width, height } = event.payload
    const isSmallImage = width <= 32 && height <= 32
    // Show rich overlay if small, minimal/hidden if large
})
```

### Step 6: Non-macOS

Module is `#[cfg(target_os = "macos")]` — no stub needed. Frontend defaults to "show overlay" if event never arrives.

## Risk assessment

| Risk | Severity | Mitigation |
|------|----------|------------|
| Tauri/wry renaming WryWebView | Medium | Pin wry version, log warning if class not found |
| Wry changing `draggingEntered:` signature | Low | Signature is dictated by the ObjC protocol, not wry |
| App Store rejection of swizzling | Low | Cmdr is distributed directly, not via App Store |
| Race condition (event ordering) | Low | Both on main thread; swizzled method runs first |
| `enumerateDraggingItems` returning zero items | Medium | Fall back to `draggedImage()`, then 0x0 (unknown) |
| Cross-process `draggingFrame` returning zeros | Low | Treat as "unknown", default to show overlay |

## Effort estimate

- **Rust code:** ~120-180 lines
- **Cargo.toml changes:** ~5 lines
- **lib.rs changes:** ~5 lines
- **Frontend changes:** ~20-30 lines
- **Testing:** Manual only (drag from Finder, other apps, self)
- **Complexity:** Medium — ObjC interop and unsafe code require care

## Alternative: upstream contribution to wry

Add `image_size: Option<(u32, u32)>` to `DragDropEvent::Enter` in wry. Cleanest long-term solution, but requires
upstream approval and may take weeks/months. Recommendation: implement swizzling now, optionally submit wry PR in
parallel, remove swizzling if accepted.

## Task list

### Milestone 1: Rust-side detection

- [x] Add `NSDragging`, `NSDraggingItem`, `NSPasteboard`, `NSView`, `NSResponder` features to Cargo.toml
- [x] Create `src/drag_image_detection.rs` with swizzle setup and custom implementation
- [x] Wire up in `lib.rs` (mod declaration, call setup in `setup` closure)
- [x] Emit `drag-image-size` Tauri event with `{ width: f64, height: f64 }`
- [x] Test: drag from Finder, confirm event appears in console

### Milestone 2: Frontend integration

- [x] Listen for `drag-image-size` event in `DualPaneExplorer.svelte`
- [x] Store image size alongside drop-target state
- [x] Conditionally adjust overlay behavior in `handleDragEnter`
- [x] Test: Finder (large image → hide overlay), minimal app (small image → show overlay)

### Milestone 3: Polish

- [x] Handle missing event gracefully (non-macOS, class not found → default to show overlay)
- [x] Add logging for swizzle setup success/failure
- [x] Run `./scripts/check.sh --rust` and `./scripts/check.sh --svelte`
- [ ] Update `docs/specs/drag-and-drop-target-spec.md` with a note about drag image detection

## Key reference files

- `src-tauri/src/lib.rs` — entry point, setup closure where swizzle is installed
- `src-tauri/Cargo.toml` — add objc2-app-kit features
- `~/.cargo/registry/.../wry-0.54.1/src/wkwebview/class/wry_web_view.rs` — WryWebView class (swizzle target)
- `~/.cargo/registry/.../wry-0.54.1/src/wkwebview/drag_drop.rs` — drag_drop functions (forwarding target)
- `src/lib/file-explorer/pane/DualPaneExplorer.svelte` — frontend event consumer
