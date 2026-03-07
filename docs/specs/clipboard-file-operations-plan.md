# Clipboard file operations plan

Add Cmd+C / Cmd+V / Cmd+X for files, working both within Cmdr and with Finder.

## Goal

Users expect Cmd+C, Cmd+V, and Cmd+X to work on files, not just text. This feature puts file references on the macOS
system clipboard so files can be pasted within Cmdr, from Cmdr into Finder, and from Finder into Cmdr. It reuses the
existing transfer infrastructure (TransferDialog, conflict resolution, progress tracking) to keep behavior consistent
with F5/F6 operations.

## How Finder does it (macOS clipboard primer)

Understanding Finder's behavior is essential because we must interoperate with it.

- **Cmd+C**: Writes file URLs (`public.file-url` type) to the general pasteboard (`NSPasteboard.generalPasteboard`).
  Also writes plain-text paths for text editor paste. No file data is copied, only references.
- **Cmd+V**: Reads file URLs from the pasteboard and performs a **copy** operation.
- **Option+Cmd+V**: Reads file URLs from the pasteboard and performs a **move** operation. Finder's Edit menu
  dynamically changes "Paste Item" to "Move Item Here" when Option is held.
- **Cmd+X**: **Does not work for files in Finder.** This is intentional. The move is decided at paste time via the
  Option key, not at copy time.
- **No cut marker on the pasteboard.** The pasteboard contents are identical for copy and "move-on-paste". The decision
  to move vs copy is based on the modifier key held during paste.

## Design decisions

### 1. Match Finder's model (copy-at-source, decide-at-paste) plus support Cmd+X

**What**: Cmd+C copies file references to the clipboard. Cmd+V pastes as copy. Option+Cmd+V pastes as move. Cmd+X also
copies to clipboard but additionally sets an internal "cut" flag, so Cmd+V after Cmd+X performs a move.

**Why**: Following Finder's core model ensures interop. Adding Cmd+X addresses user expectations from Windows/Linux.
The Cmd+X cut state is internal to Cmdr only: if you Cmd+X in Cmdr and paste in Finder, it's a copy (Finder doesn't
know about our cut flag). This is a known, acceptable limitation, and matches how other third-party file managers
(Path Finder, ForkLift) handle it.

**Cut state validation**: On paste, the backend checks whether the clipboard paths still match the stored cut state
paths. If another app replaced the clipboard between Cmd+X and Cmd+V, the cut state is stale and gets cleared. The
paste then proceeds as a regular copy.

### 2. Direct NSPasteboard access via objc2 (no new dependency)

**What**: Write and read file URLs using `objc2` / `objc2-app-kit` / `objc2-foundation` to call NSPasteboard APIs
directly from Rust Tauri commands.

**Why**: The codebase already uses `objc2` for drag image detection and swizzling (`drag_image_detection.rs`,
`drag_image_swap.rs`). The official `tauri-plugin-clipboard-manager` only supports text/images, not file URLs. A
community plugin (`tauri-plugin-clipboard`) exists but adds an external dependency for something achievable with existing
tools. Direct NSPasteboard access gives full control and keeps the dependency footprint small.

### 3. Replace PredefinedMenuItems with custom MenuItems for Cut/Copy/Paste

**What**: Replace `PredefinedMenuItem::cut()`, `PredefinedMenuItem::copy()`, and `PredefinedMenuItem::paste()` in the
Edit menu with custom `MenuItem`s that have the same accelerators (Cmd+X, Cmd+C, Cmd+V). Route them through the
`execute-command` dispatch. In the frontend handler, check `document.activeElement` to decide between text clipboard and
file clipboard operations.

**Why**: This is the exact same pattern used for Cmd+A (Select All), which already replaces
`PredefinedMenuItem::select_all` with a custom MenuItem and checks focus context. PredefinedMenuItems go through the
native responder chain, which is opaque and can't be intercepted. Custom MenuItems route through our unified command
dispatch, giving us full control.

**Text input fallback**: When `document.activeElement` is an `<input>`, `<textarea>`, or `[contenteditable]` element,
delegate to text clipboard. For copy/cut, use `document.execCommand('copy')` / `document.execCommand('cut')` — these
are deprecated in the web spec but fully functional in WKWebView and triggered by a user gesture (menu accelerator).
For paste, use `navigator.clipboard.readText()` (available in Tauri's secure context) with
`document.execCommand('insertText', false, text)` to insert. If `document.execCommand('paste')` works reliably in
WKWebView (test early!), prefer it for simplicity. Have a fallback path ready in case `execCommand('paste')` is
blocked by WebKit security policies.

**Risk**: `document.execCommand('paste')` may be restricted even in user-gesture contexts in WKWebView. Test this in
milestone 2 before building on it. If blocked, the `navigator.clipboard.readText()` + `insertText` path is the
reliable alternative.

### 4. Which files to copy/cut

**What**: Copy/cut the **selected files** if a selection exists. If no selection, copy/cut the **cursor file**. The ".."
entry is never copied.

**Why**: Matches the behavior of all existing file operations (F5, F6, F8). The "selection or cursor" pattern is the
standard in dual-pane file managers.

### 5. Paste destination is always the current directory

**What**: Paste into the current directory of the focused pane. Not into a subfolder under the cursor, not into the
opposite pane.

**Why**: Simple, predictable, matches Finder. If the user wants to paste into a specific subfolder, they navigate there
first. "Paste into cursor folder" would be ambiguous (what if the cursor is on a file?) and error-prone.

### 6. Reuse the existing transfer infrastructure

**What**: Clipboard paste triggers the same TransferProgressDialog flow as F5/F6, with the same conflict resolution
(dry-run, Stop mode, overwrite/skip/rename), progress tracking, cancellation, and rollback.

**Why**: The transfer infrastructure handles dozens of edge cases (cross-volume, same-name conflicts, disk space
validation, atomic operations, network mount timeouts). Reimplementing any of it would be fragile and inconsistent.

**Difference from F5/F6**: Clipboard paste skips the TransferDialog destination picker (destination is pre-set to the
current directory). It goes directly to the `TransferProgressDialog` with dry-run + execution. For fast operations
that complete before the dialog renders, the dialog closes immediately and a toast appears instead — this already
works in the existing transfer infrastructure for same-fs moves.

**Source paths, not indices**: The existing `copyFiles()` / `moveFiles()` backend commands accept paths, not listing
indices. The index-to-path resolution happens earlier (via `get_paths_at_indices` from `LISTING_CACHE`). For clipboard
paste, paths come directly from the clipboard — no listing cache involved. The frontend builds a `TransferContext` with
source paths from the clipboard, matching the pattern already used by the drag-and-drop handler.

### 7. MTP paths excluded from clipboard

**What**: Clipboard copy/cut is disabled for MTP device paths. Show a notification: "Clipboard copy isn't supported
for MTP devices yet."

**Why**: MTP files use a virtual path scheme and go through a separate code path. The NSPasteboard file URL format
expects real filesystem paths. MTP support can be added later by copying files to a temp directory first.

### 8. Write both file URLs and plain text to the clipboard

**What**: When copying files to the clipboard, write both `public.file-url` entries (for file paste) and a plain-text
representation of the paths (newline-separated, for text editor paste).

**Why**: This is what Finder does. If a user copies files in Cmdr and pastes into a terminal or text editor, they get
the file paths. Both representations coexist on the NSPasteboard.

### 9. No visual dimming for cut files in V1

**What**: When files are cut (Cmd+X), show a notification toast: "N files ready to move. Paste to complete." No dimming
or transparency effect on the cut files in the file list.

**Why**: Visual dimming across virtual-scrolled lists with 100k+ files, surviving navigation, sort changes, and file
watcher updates, adds significant complexity. The toast provides adequate feedback for V1. Dimming can be added later
by tracking cut paths in a `Set<string>` and applying an opacity CSS class during render.

### 10. Cmd+X visual feedback: notification-based

After Cmd+X, show a toast: "3 files ready to move. Paste to complete." with a brief info icon.
After successful paste of cut files, the toast auto-dismisses and a new "Moved 3 files to /dest" toast appears
(standard transfer completion toast).
If the user does Cmd+C after Cmd+X (overriding the cut), show no extra notification; the cut state silently clears.

### 11. Internal cut state lives in Rust backend

**What**: A `LazyLock<RwLock<Option<CutState>>>` in the clipboard module tracks the cut source paths.

**Why**: The Rust backend is the authoritative state for file operations. Keeping cut state in Rust avoids
synchronization issues between frontend state and backend operations. The frontend queries cut state via IPC when
needed.

### 12. Linux: stub for now

**What**: Gate all NSPasteboard code with `#[cfg(target_os = "macos")]`. On Linux, the Tauri commands return errors or
empty results. File clipboard commands are hidden from the command palette and menu on Linux.

**Why**: The codebase is macOS-first with Linux as a secondary target. Linux clipboard file operations use different
mechanisms (X11 selections or Wayland clipboard with `text/uri-list` MIME type). These can be added later following the
same architecture.

## Implementation

### Backend: new Rust module `clipboard/`

Create `apps/desktop/src-tauri/src/clipboard/` with:

**`mod.rs`** — Public API, module structure.

**`pasteboard.rs`** (macOS only) — NSPasteboard FFI:

```
write_file_urls_to_clipboard(paths: &[PathBuf]) -> Result<(), String>
  - NSPasteboard.generalPasteboard().clearContents()
  - Create NSURL array from paths via NSURL::fileURLWithPath()
  - pasteboard.writeObjects(&urls)
  - Also write newline-joined paths as NSPasteboardTypeString (plain text)

read_file_urls_from_clipboard() -> Result<Vec<PathBuf>, String>
  - NSPasteboard.generalPasteboard()
  - pasteboard.readObjectsForClasses_options([NSURL.class], {fileURLsOnly: true})
  - Convert NSURL array to Vec<PathBuf>

clipboard_has_files() -> Result<bool, String>
  - Check if pasteboard canReadObjectForClasses with NSURL + fileURLsOnly
```

**`state.rs`** — Cut state management:

```rust
struct CutState {
    source_paths: Vec<PathBuf>,
}

static CUT_STATE: LazyLock<RwLock<Option<CutState>>> = ...;

set_cut_state(paths: Vec<PathBuf>)   // Called on Cmd+X
clear_cut_state()                     // Called after paste-move, new copy, or new cut
get_cut_state() -> Option<Vec<PathBuf>>  // Called on paste to check if move is needed
```

### Backend: new Tauri commands

In `apps/desktop/src-tauri/src/commands/`, add or extend:

```
copy_files_to_clipboard(listing_id, selected_indices, cursor_index, has_parent, include_hidden) -> Result<usize, String>
  - Resolve paths from LISTING_CACHE (same pattern as start_selection_drag)
  - Call pasteboard::write_file_urls_to_clipboard()
  - Clear cut state
  - Return number of files copied

cut_files_to_clipboard(listing_id, selected_indices, cursor_index, has_parent, include_hidden) -> Result<usize, String>
  - Same path resolution as above
  - Call pasteboard::write_file_urls_to_clipboard()
  - Set cut state with resolved paths
  - Return number of files cut

read_clipboard_files() -> Result<ClipboardReadResult, String>
  - Call pasteboard::read_file_urls_from_clipboard()
  - Check cut state: if set, verify clipboard paths match cut state paths (order-insensitive set comparison)
  - If clipboard paths diverged (another app replaced clipboard), clear stale cut state → isCut = false
  - Return { paths: Vec<String>, isCut: bool }

clear_cut_state() -> ()
  - Exposed for frontend to clear cut state when needed (for example, on paste cancellation)
```

**Threading**: Path resolution from cache is instant, but NSPasteboard is NOT thread-safe — it must be accessed from
the main thread. Use the same `run_on_main_thread` pattern as `start_selection_drag` in the drag code. The Tauri
commands should be async, dispatching the NSPasteboard calls to the main thread via `app.run_on_main_thread()` (or the
equivalent pattern used in `drag_image_swap.rs`). The path resolution from `LISTING_CACHE` can happen on any thread
before the main-thread dispatch.

### Frontend: TypeScript wrappers

In `apps/desktop/src/lib/tauri-commands/`, add `clipboard-files.ts`:

```typescript
interface ClipboardReadResult {
    paths: string[]
    isCut: boolean
}

copyFilesToClipboard(listingId, selectedIndices, cursorIndex, hasParent, includeHidden): Promise<number>
cutFilesToClipboard(listingId, selectedIndices, cursorIndex, hasParent, includeHidden): Promise<number>
readClipboardFiles(): Promise<ClipboardReadResult>
clearClipboardCutState(): Promise<void>
```

### Frontend: command registry

Add to `command-registry.ts`:

```typescript
{ id: 'edit.copy', name: 'Copy to clipboard', scope: 'Main window/File list', shortcuts: ['⌘C'], showInPalette: true,
  description: 'Copy selected files to clipboard for pasting' }
{ id: 'edit.cut', name: 'Cut to clipboard', scope: 'Main window/File list', shortcuts: ['⌘X'], showInPalette: true,
  description: 'Cut selected files (paste will move them)' }
{ id: 'edit.paste', name: 'Paste', scope: 'Main window/File list', shortcuts: ['⌘V'], showInPalette: true,
  description: 'Paste files from clipboard into current folder' }
{ id: 'edit.pasteAsMove', name: 'Move here', scope: 'Main window/File list', shortcuts: ['⌥⌘V'], showInPalette: true,
  description: 'Paste files from clipboard as a move (like Finder Option+Cmd+V)' }
```

The existing `file.copy` (F5) and `file.move` (F6) remain unchanged — they open a destination picker dialog. The new
`edit.*` commands are clipboard operations that always target the current directory. The "Copy to clipboard" and "Cut to
clipboard" names disambiguate from F5 "Copy" in the command palette search results.

### Frontend: command execution in +page.svelte

Add cases to `handleCommandExecute`:

```
case 'edit.copy': {
    // Context check: if text input focused, delegate to native text copy
    const active = document.activeElement
    const isTextInput = active instanceof HTMLInputElement
        || active instanceof HTMLTextAreaElement
        || active?.closest('[contenteditable]')
    if (isTextInput) {
        document.execCommand('copy')
        return
    }
    // File copy: resolve selection/cursor, call copyFilesToClipboard()
    const count = await copyFilesToClipboard(...)
    showNotification(`Copied ${count} ${count === 1 ? 'item' : 'items'}`)
    break
}

case 'edit.cut': {
    // Same activeElement check for text inputs
    // Call cutFilesToClipboard()
    showNotification(`${count} ${count === 1 ? 'item' : 'items'} ready to move. Paste to complete.`)
    break
}

case 'edit.paste': {
    // Same activeElement check for text inputs
    // Call readClipboardFiles()
    // If isCut: trigger move flow (reuse transfer infrastructure)
    // If !isCut: trigger copy flow
    // Destination = current directory of focused pane
    break
}

case 'edit.pasteAsMove': {
    // Always file-level, no text input check needed (Option+Cmd+V isn't a text shortcut)
    // Call readClipboardFiles()
    // Always trigger move flow, regardless of isCut
    break
}
```

### Frontend: paste flow

The paste handler should:

1. Read clipboard files via `readClipboardFiles()`
2. If empty, show brief notification "No files on the clipboard" and return
3. Validate: if current pane is an MTP path, show "Paste isn't supported for MTP devices yet" and return
4. Determine operation type: if `isCut` or if command is `edit.pasteAsMove`, operation = move; otherwise copy
5. Build source paths array from the clipboard paths (same format as the drop handler in DualPaneExplorer)
6. Set destination to current pane's directory
7. Open `TransferProgressDialog` with `skipDestinationPicker: true` (a new flag) — this skips the TransferDialog
   destination picker and goes straight to dry-run + execution. The progress dialog already handles conflicts via
   dry-run mode, and fast operations complete instantly with a toast.
8. On success: if was a cut operation, call `clearClipboardCutState()`
9. Refresh relevant panes (both panes for move, destination pane only for copy)

The key reuse point: the paste flow reuses `TransferProgressDialog` with `operationType: 'copy' | 'move'`. The only
difference from F5/F6 is that source paths come directly from the clipboard (not from listing cache indices), and the
destination is pre-set to the current directory. This matches the pattern already used by the drag-and-drop handler,
which also receives arbitrary paths from external sources.

### Menu changes

**macOS Edit menu** (in `menu/mod.rs`):

Replace:
```rust
&PredefinedMenuItem::cut(app, None)?,
&PredefinedMenuItem::copy(app, None)?,
&PredefinedMenuItem::paste(app, None)?,
```

With custom MenuItems:
```rust
MenuItem::with_id_and_accelerator(app, EDIT_CUT_ID, "Cut", true, Some("Cmd+X"))?,
MenuItem::with_id_and_accelerator(app, EDIT_COPY_ID, "Copy", true, Some("Cmd+C"))?,
MenuItem::with_id_and_accelerator(app, EDIT_PASTE_ID, "Paste", true, Some("Cmd+V"))?,
```

Add a new "Move here" item:
```rust
MenuItem::with_id_and_accelerator(app, EDIT_PASTE_MOVE_ID, "Move here", true, Some("Alt+Cmd+V"))?,
```

Add the new IDs to `menu_id_to_command()` and `command_id_to_menu_id()`.

Keep `PredefinedMenuItem::undo()` and `PredefinedMenuItem::redo()` as-is (no custom handling needed for undo/redo).

**Linux Edit menu**: Currently has no PredefinedMenuItems. Add custom MenuItems for Cut/Copy/Paste/Move here with
Ctrl-based accelerators. Commands hidden if clipboard file support isn't available.

**Viewer window Edit menu**: Keep `PredefinedMenuItem::copy()` for the viewer window, since the viewer is read-only text
and doesn't need file clipboard. Or replace it with the same custom MenuItem and have the viewer handler always do text
copy.

### Capabilities

Add clipboard read permission if not already present. The current `clipboard-manager:default` should cover it, but
verify. No additional Tauri capabilities needed since the NSPasteboard access is direct FFI, not through a Tauri plugin.

## Edge cases

### Same-directory paste
Paste into the same directory the files came from. Conflict resolution handles this (rename to "file (copy).txt",
overwrite, skip). Same behavior as dragging a file onto its own parent folder in Finder.

### Source file deleted before paste
The transfer infrastructure already handles missing source files: the copy/move backend validates sources and reports
errors for missing files. The user sees "File not found" in the progress dialog error list.

### Paste cut files twice
First paste moves the files. Cut state is cleared. Second paste: `readClipboardFiles()` returns `isCut: false` (cut
state cleared) and the file URLs are still on the pasteboard — but they point to the OLD location. The second paste
fails with "File not found" because the sources were moved. This is correct behavior.

### Clipboard replaced by another app
If the user copies files in Cmdr, then copies text in another app, then pastes in Cmdr: `readClipboardFiles()` returns
empty (no file URLs on pasteboard). Cmdr shows "No files on the clipboard." The text paste falls through to the text
input handler if focused on an input.

### Very large selection (10k+ files)
Path resolution from `LISTING_CACHE` is O(n) and fast. Writing 10k URLs to NSPasteboard is fast (URLs are just
strings). The paste operation goes through the normal transfer progress UI with scan, progress bar, and cancellation.
No special handling needed.

### Network mount source files
Clipboard stores paths (file URLs). If the network mount disconnects between copy and paste, the paste operation fails
with the standard network timeout handling (2s `blocking_with_timeout`). Same as any file operation on a dead mount.

### Mixed file types (files + folders)
The clipboard contains file URLs for all selected items. The paste operation handles recursive copy/move of folders
the same way as F5/F6. No special clipboard handling needed.

### Paste from Finder
Finder writes `public.file-url` entries to the pasteboard. `readClipboardFiles()` reads these. The `isCut` flag is
false (Finder doesn't set our internal cut state). Paste performs a copy. Option+Cmd+V performs a move. Full interop.

### Copy from Cmdr, paste in Finder
Cmdr writes `public.file-url` entries. Finder reads them normally. Cmd+V in Finder copies. Option+Cmd+V in Finder
moves. Full interop.

### Copy from Cmdr, paste in a text editor
Cmdr also writes plain-text paths to the pasteboard. Text editors read the plain text and paste file paths
(newline-separated). Same as Finder behavior.

### Cmd+C then Cmd+X (override)
Cmd+X clears the previous clipboard content, writes new file URLs, and sets cut state. Previous Cmd+C is fully
replaced.

### Cmd+X then Cmd+C (override)
Cmd+C clears cut state and writes new file URLs. The cut is abandoned, no files are moved.

### Paste on a read-only volume
The transfer infrastructure already detects read-only destinations and shows an error. No special clipboard handling.

### Paste with ".." as cursor
If cursor is on ".." and there's no selection, there's nothing to paste FROM. But paste is about the clipboard
content, not the cursor. The cursor position doesn't affect paste. Paste always puts files into the current directory.
(Copy/cut of ".." is ignored since ".." can't be selected and copy of cursor at ".." is a no-op.)

### MTP device paths
Copy/cut is disabled for MTP paths (notification shown). Paste into an MTP pane is also disabled. MTP uses virtual
paths that don't correspond to real filesystem paths, which NSPasteboard file URLs require.

### Rename dialog or other modal open
When a modal dialog or inline rename is active, Cmd+C/V/X route to text clipboard (the activeElement check handles
this). File clipboard operations only fire when the file list has focus.

### Symlinks
Copying a symlink to the clipboard stores the symlink's own path, not the target's path. The paste operation copies or
moves the symlink itself (preserving it as a symlink), same as F5/F6. No special handling needed.

### Both panes showing the same directory
If both panes show the same directory and the user pastes, the pasted files appear in both panes automatically. The
file watcher triggers `directory-diff` events for both listings. Handled by the existing architecture.

### Copy files then quit Cmdr, reopen, paste
The system clipboard persists across app restarts. File URLs on the clipboard remain valid. Paste works. The cut state
does NOT persist (it's in-memory only), so if you Cmd+X, quit, reopen, Cmd+V: it pastes as copy, not move. This is
acceptable — cutting files and quitting is an unusual flow, and copy is the safer default.

## Out of scope (future enhancements)

- **Undo paste**: Would require tracking paste operations and reversing them. Complex, not needed for V1.
- **Visual dimming of cut files**: Opacity effect on cut files in the list. Requires path tracking across virtual scroll
  and navigation. Nice-to-have for V2.
- **Linux clipboard support**: Requires `text/uri-list` MIME type handling via X11/Wayland. Architecture is ready
  (cfg-gated), implementation deferred.
- **Paste into folder under cursor**: "Smart paste" that puts files into the folder the cursor is on. Ambiguous UX,
  deferred.
- **Clipboard history**: Track multiple clipboard entries. Not standard macOS behavior, deferred.

## Task list

### Milestone 1: backend clipboard infrastructure
- [ ] Create `clipboard/` module with `mod.rs`, `pasteboard.rs`, `state.rs`
- [ ] Verify `objc2-app-kit` and `objc2-foundation` feature flags in Cargo.toml cover NSPasteboard, NSURL, NSArray
- [ ] Implement NSPasteboard FFI: write file URLs + plain text, read file URLs, has-files check
- [ ] Ensure all NSPasteboard calls dispatch to main thread (same pattern as drag code)
- [ ] Implement cut state management (set, clear, get)
- [ ] Add Tauri commands: `copy_files_to_clipboard`, `cut_files_to_clipboard`, `read_clipboard_files`, `clear_cut_state`
- [ ] Add `#[cfg(target_os = "macos")]` gates and Linux stubs
- [ ] Write Rust tests for cut state management and pasteboard round-trip (write then read)

### Milestone 2: frontend wiring
- [ ] **First**: Test `document.execCommand('copy'/'cut'/'paste')` in WKWebView with a custom MenuItem accelerator
      — if paste is blocked, implement the `navigator.clipboard.readText()` + `insertText` fallback before continuing
- [ ] Add TypeScript IPC wrappers in `clipboard-files.ts`
- [ ] Add `edit.copy`, `edit.cut`, `edit.paste`, `edit.pasteAsMove` to command registry
- [ ] Implement command handlers in `+page.svelte` with `document.activeElement` context routing
- [ ] Implement paste flow: read clipboard, determine op type, invoke transfer infrastructure
- [ ] Add notification toasts for copy, cut, paste, and empty clipboard
- [ ] Verify text input fallback works (Cmd+C/V/X in rename field, search, dialogs)

### Milestone 3: menu integration
- [ ] Replace PredefinedMenuItem cut/copy/paste with custom MenuItems on macOS
- [ ] Add "Move here" (Option+Cmd+V) menu item
- [ ] Update `menu_id_to_command()` and `command_id_to_menu_id()` mappings
- [ ] Do NOT add Cut/Copy/Paste to `set_menu_context` enable/disable — they must stay enabled always (text clipboard
      needs to work in all contexts; the frontend's `activeElement` check handles routing)
- [ ] Update Linux menu to add custom Cut/Copy/Paste items
- [ ] Update viewer window menu (keep text-only clipboard behavior)
- [ ] Verify accelerator sync works for the new custom MenuItems
- [ ] Verify `cleanup_macos_menus` doesn't remove the new custom MenuItems (it strips items by title; "Cut"/"Copy"/
      "Paste" aren't in the removal list, but verify no new system-injected duplicates appear)

### Milestone 4: testing and polish
- [ ] Manual test: Cmd+C files in Cmdr, Cmd+V in Cmdr (same pane, other pane, different directory)
- [ ] Manual test: Cmd+X in Cmdr, Cmd+V in Cmdr (verify move, verify cut state cleared)
- [ ] Manual test: Cmd+C in Cmdr, Cmd+V in Finder (verify copy)
- [ ] Manual test: Cmd+C in Finder, Cmd+V in Cmdr (verify copy)
- [ ] Manual test: Cmd+C in Finder, Option+Cmd+V in Cmdr (verify move)
- [ ] Manual test: Cmd+C/V/X in rename field, new folder dialog, search (verify text clipboard works)
- [ ] Manual test: paste with conflicts (same-name files, overwrite/skip/rename)
- [ ] Manual test: paste on read-only volume (verify error)
- [ ] Manual test: cut then navigate away then paste (verify works across directories)
- [ ] Manual test: cut then copy (verify cut state cleared)
- [ ] Manual test: copy 10k+ files, paste (verify performance)
- [ ] Run `./scripts/check.sh` — verify all checks pass
- [ ] Update `file-operations/CLAUDE.md` and `menu/CLAUDE.md` to document clipboard operations
- [ ] Update `file-explorer/CLAUDE.md` if selection behavior docs need updates
