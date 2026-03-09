# Context menu actions plan

Add View (F3), Copy (F5), Move (F6), New folder (F7), and Delete (F8) to the right-click context
menu in file panes. These actions already work via keyboard shortcuts and the function key bar but
are missing from the context menu.

## Current state

`build_context_menu` in `src-tauri/src/menu/mod.rs` builds a native context menu with: Open, Edit,
Show in Finder, Rename, Copy filename, Copy path, Get info, and Quick look. All F-key file actions
are absent.

## Proposed menu structure

```
Open                    (files only)
View              F3    (files only)
Edit              F4    (files only — already exists)
────────────────────
Copy              F5
Move              F6
Rename            F2    (already exists)
────────────────────
New folder        F7
────────────────────
Delete            F8
────────────────────
Show in Finder
Copy "filename"   ⌘C
Copy path
────────────────────
Get info          ⌘I   (macOS only)
Quick look              (macOS only)
```

Grouping: Open/View/Edit = "look at this". Copy/Move/Rename = "transform". New folder = creation.
Delete = destructive (isolated). Clipboard/Finder/info = utility.

## Implementation

All work is in Rust — the frontend command handlers (`file.view`, `file.copy`, `file.move`,
`file.newFolder`, `file.delete`) already exist in `handleCommandExecute`.

1. Add ID constants for the new items (e.g., `VIEW_ID`, `COPY_FILE_ID`, `MOVE_ID`,
   `NEW_FOLDER_ID`, `DELETE_ID`).
2. Create `MenuItem::with_id` entries in `build_context_menu` with accelerator labels.
3. Add entries in `menu_id_to_command()` mapping each new ID to the corresponding command registry
   ID.
4. Restructure existing items into the grouping above (move Rename into the transform group, etc.).

Accelerator labels (F3, F5, etc.) are safe on both macOS and Linux — context menus are ephemeral and
don't register persistent GTK accelerators, so the F-key interception issue that affects the menu bar
doesn't apply here.

## Edge cases

- **Multi-selection:** Copy, Move, and Delete operate on all selected items (the dialog handlers
  already support this). View opens the viewer for the right-clicked item specifically. Rename starts
  renaming the right-clicked (cursor) item regardless of selection.
- **".." entry:** Context menu is already suppressed for ".." — no change needed.
- **Directories:** View is omitted for directories (same as Open and Edit), since Enter already
  navigates into them. All other actions (Copy, Move, Rename, Delete, New folder) apply.
- **Read-only / network locations:** Currently `build_context_menu` receives `path`, `filename`, and
  `is_directory` — no writability flag. Copy/Move/Rename/Delete/New folder should ideally be disabled
  on read-only mounts. This can be addressed as a follow-up by passing a `writable: bool` parameter.

## Future: empty-space context menu

Right-clicking empty pane space could offer New folder, Paste, and other pane-level actions. Out of
scope here, but the architecture supports it — `handleContextMenu` in `FilePane.svelte` can detect
empty-space clicks and call a different builder or pass a flag. No structural changes needed.

## Tasks

- [x] Add new menu item ID constants and `MenuItem::with_id` entries in `build_context_menu`
- [x] Add `menu_id_to_command()` mappings for new IDs
- [x] Restructure menu item ordering to match proposed grouping
- [x] Conditionally hide View for directories (like Open and Edit)
- [ ] Manual test on macOS: verify all new items appear and dispatch correctly
- [ ] Manual test on Linux: verify accelerator labels display without GTK issues
- [x] Run `./scripts/check.sh --rust`
