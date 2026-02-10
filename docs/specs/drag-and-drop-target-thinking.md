# Drag&drop target

## Overview

We already have drag&drop FROM our panes TO external apps.
I want the panes (both Full and Brief) to be drop targets for files from both external apps and and the other pane.

I want an amazing UX and full progress display in the app during the operation, in the same UI we use for copying.

## What events do we see?

IIUC, Tauri 2 provides `onDragDropEvent` on the webview with these events:

| Event   | Data                                            | When                                |
|---------|-------------------------------------------------|-------------------------------------|
| `enter` | `paths: string[]`, `position: PhysicalPosition` | Files enter the webview area        |
| `over`  | `position: PhysicalPosition`                    | Cursor moves while dragging         |
| `drop`  | `paths: string[]`, `position: PhysicalPosition` | User releases the mouse             |
| `leave` | (nothing)                                       | Cursor leaves window or ESC pressed |

Element targeting problem: Tauri's drop event fires at the **window** level with a cursor position, but doesn't tell us which DOM element is underneath. The known workaround is DOMRect hit-testing: store the bounding rects of the panes and file entries, then check which pane the `position` falls within. This is reliable for two large panes. But we also need to make it work for directory entries: the user needs to be able to drop a file into a folder, not just onto the pane. The dir entries must get an outline/subtle glow while the cursor is over it while dragging (in a way that doesn't shift the content). If the mouse is not over a dir entry then the whole pane should get an outline/glow.
(We'll need row-level hit testing in addition to pane-level, using the virtual scroll offset to determine which row is under the cursor.)

On `drop`, the copy/move operation window should come up, the same we're getting when pressing F5/F6.

## What should happen if a user just pulls a string or image to the pane?

IIUC, Tauri gives me **file paths**. No text, no image blobs, no `dataTransfer` types. It intercepts the native OS drag before the browser sees it. If someone drags a text selection or an image from a browser, Tauri either:
- Gets a temp file path (some apps write temp files for drag content, like browsers do for images)
- Gets nothing useful

So text/string drops are essentially not a concern for Tauri — we'll only receive actual file paths.

| Source content               | What happens                                                   |
|------------------------------|----------------------------------------------------------------|
| Files from Finder/Explorer   | Works — you get absolute paths                                 |
| Image dragged from browser   | Usually works — browser creates a temp file, you get that path |
| Text selection from browser  | Not meaningful — Tauri won't fire, or you get a temp `.txt`    |
| URL from browser address bar | May get a `.webloc`/`.url` temp file on macOS                  |

For a file manager, the only case worth handling is actual files. Everything else should show a "not allowed" cursor (we control this via the drop effect). Text/image drags from browsers creating temp files would be a great extra. If a user drags an image from Safari, they'll get a temp PNG copied into their folder, which is actually reasonable behavior. It's what Finder does too, I think. We should probably handle that.

## Pane-to-pane (internal) and external-to-pane

In my understanding, this is logically a single feature: if dragging from one side to the other, that's just initiating a copy/move command as if we pressed F5 or F6. And when dragging from an external app, we get paths to stuff.

The existing drag-out code already uses `tauri-plugin-drag` to initiate a **native OS drag**. Once that native drag is active, the OS manages it. If the user drops it back onto our own window, Tauri's `onDragDropEvent` fires with those same file paths. So the receiving side maybe doesn't even need to distinguish "is this from me or from Finder" — it gets file paths either way.

1. Both external and internal drops arrive via the same `onDragDropEvent` API
2. Both result in the same operation: copy/move files to the target pane's current directory
3. The hit-testing logic is identical (which pane is the cursor over?)
4. The file operation is identical (copy or move, with progress)

But in reality, we'll probably want to provide some UX extras detecting the source is our own app, like showing the "right to left" or "left to right" arrow icon that I think we're already showing for normal F5/F6.

The only difference is a **small optimization** for the internal case: since we already know the source paths from `LISTING_CACHE`, we could skip re-resolving them from the drop event. But functionally the drop event gives us the same paths anyway.

## Proposed UX

### Drop feedback

1. **Hover highlight**: When `enter`/`over` fires and cursor is over a pane or folder, highlight that pane's background (subtle border glow)
2. **Drop label**: Showing a floating label like "Copy 3 files here" or "Move to ~/Documents" near the cursor would be extremly nice
3. **Modifier keys**: Alt/Option = force copy (matching the existing drag-out behavior). No modifier = move if same volume, copy if different volume.
4. **Invalid targets**: If cursor is over the toolbar or other non-pane areas, show "not allowed" styling.

### Operation after drop

Since the drop triggers the same copy/move operation as F5/F6, we get full control over progress:
- Use our existing file operation infrastructure, asking for confirmation first
- Show the progress indicator/bar per our design guidelines
- Show file count, bytes transferred, and time estimate as usual

### Detecting internal vs external

We can set a module-level flag when our own drag starts (`isDraggingFromSelf = true`) and clear it on drag end. When a drop arrives:
- If `isDraggingFromSelf`: we already know the source, can skip confirmation dialog, show "Move to other pane" wording
- If external: show source path info, maybe ask copy vs move if ambiguous

---

Sources:
- [Tauri DragDropEvent Rust docs](https://docs.rs/tauri/latest/x86_64-apple-ios/tauri/enum.DragDropEvent.html)
- [Tauri issue #13835 — Element targeting with DOMRect](https://github.com/tauri-apps/tauri/issues/13835)
- [Tauri issue #14134 — Duplicate events bug](https://github.com/tauri-apps/tauri/issues/14134)
- [Tauri Discussion #4736 — Drag file to Tauri app](https://github.com/tauri-apps/tauri/discussions/4736)
- [MDN — File drag and drop](https://developer.mozilla.org/en-US/docs/Web/API/HTML_Drag_and_Drop_API/File_drag_and_drop)
- [MDN — Recommended drag types](https://developer.mozilla.org/en-US/docs/Web/API/HTML_Drag_and_Drop_API/Recommended_drag_types)
- [Total Commander drag behavior](https://www.ghisler.ch/board/viewtopic.php?t=28216)