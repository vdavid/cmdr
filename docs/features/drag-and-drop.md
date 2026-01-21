# Drag and drop

Cmdr supports native drag-and-drop for files, both for dragging files out to other applications (like Finder or other
file managers) and receiving files dropped from external sources.

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

### Single file vs selection drag

- **No selection**: Dragging a file selects it first, then initiates drag
- **Existing selection**: Dragging from within selection drags all selected files

### Drag preview

The drag preview shows the icon of the first file being dragged. For multi-file drags, macOS shows a badge with the
count.

## Receiving drops (external drag)

Files dragged from Finder or other applications can be dropped into Cmdr panes. The drop operation:

- Copies files if source is a different volume
- Moves files if source is the same volume
- Respects modifier keys (Alt/Option forces copy)

## Implementation

### Architecture

Drag-and-drop uses two different code paths for performance reasons:

| Scenario             | Implementation                               | Reason                                  |
|----------------------|----------------------------------------------|-----------------------------------------|
| Single file          | Frontend via `@crabnebula/tauri-plugin-drag` | Simple, direct                          |
| Multi-file selection | Backend Rust command                         | Avoids transferring file paths over IPC |

### Frontend (`drag-drop.ts`)

The frontend handles:

- Tracking mouse movement and drag threshold
- Detecting modifier keys for copy/move mode
- Writing the drag icon to a temp file
- Deciding which code path to use

Key functions:

- `startSelectionDragTracking()` - Entry point from UI components
- `performSingleFileDrag()` - Uses plugin directly
- `performSelectionDrag()` - Calls backend command

### Backend (`file_system.rs`)

The `start_selection_drag` command:

1. Looks up file paths from `LISTING_CACHE` using indices
2. Creates a `DragItem::Files` with the resolved paths
3. Calls `drag::start_drag()` on the main thread (required by macOS)

This avoids serializing potentially hundreds of file paths over IPC for large selections.

### Icon handling

1. Frontend gets icon from `icon-cache` (already loaded for file display)
2. Writes icon as PNG to temp file (`drag-icon.png`)
3. Passes path to native drag API
4. Cleans up temp file after drag completes

## Platform support

**Current status**: macOS only.

The `start_selection_drag` Rust command is gated with `#[cfg(target_os = "macos")]`. Other platforms return an error.

### Cross-platform strategy (future)

When adding Linux/Windows support, platform-specific implementations are recommended over a unified abstraction layer.
This decision optimizes for performance and UX rather than code simplicity.

**Rationale:**

| Platform    | Native features worth preserving                                           |
|-------------|----------------------------------------------------------------------------|
| **macOS**   | Spring-loaded folders, drag promises, Finder-style visual feedback         |
| **Windows** | Shell drag images with thumbnails, drop descriptions, Explorer integration |
| **Linux**   | Desktop-environment-specific behaviors (Nautilus vs Dolphin vs Thunar)     |

A generic abstraction (like using `tauri-plugin-drag` everywhere) would work but can't expose these platform-specific
features. For a file manager where drag-drop is a core interaction, native feel matters.

**Recommended approach:**

1. Ship Linux/Windows support using `tauri-plugin-drag` for quick cross-platform drag
2. Keep native macOS implementation (already more polished)
3. Replace with native implementations per-platform based on user feedback

## Testing

### Manual testing

1. **Single file drag**: Click and drag an unselected file to Finder
2. **Selection drag**: Select multiple files, drag to Finder
3. **Copy mode**: Hold Alt/Option while dragging, verify files are copied not moved
4. **Cancel**: Start drag, press Escape, verify no operation occurs
5. **Threshold**: Click and release without moving, verify no drag starts

### Automated testing

Drag-and-drop involves native OS APIs that are difficult to test in automated environments. The Playwright e2e tests
don't cover drag operations due to WebDriver limitations on macOS.

Unit tests cover the supporting logic (index resolution, path lookup) but not the native drag itself.
