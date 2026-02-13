I want to add a "rename" feature to the app. Here are the details.

Access:
- It should have an Edit → Rename (Shift+F6) menu item (shortcut configurable in Settings like all)
- Context menu item when right-clicking a file or folder. "Rename / Shift+F6" (or current shortcut)
- Clicking the name area of the entry which is already under the cursor, and then not moving the mouse more than a certain threshold (~10px) and not leaving the entry for about 800 ms, should also activate renaming. (Pretty much Total Commander's behavior) If doubleclick happens in the meantime, we should cancel the action.
- Selection is irrelevant to the "Rename" feature, it always works with the item under the cursor. Selection state should be preserved for other files, and I don't care about the selection state of the renamed file.
- Rename should always be a no-op on the parent entry (..).

Activating:
- When activated, it should turn the current entry into a textbox, the same size of the name part of the entry.
  In Brief mode, it's the full size of the entry, in Full mode, it's only the size of the name part.
- Add a quick glow/zoom kinda anim for 300 ms around the textbox so the user sees that it got edit focus.
- At the same time, the front end should check with the back end for write permission for the parent dir. It might happen that we're on a writable volume but the user still lacks write permission. The frontend should auto-cancel the renaming state in this case and show a notification at the top-right about this. macOS also has the immutable flag (uchg), SIP protection, and file locks. Checking parent dir write access isn't actually sufficient. We need a logic that checks writability overall.
  backend check should verify renameability of the specific file.
- While the "renaming" mode is active, app-level shortcuts should be suppressed, just like when we have dialogs open. For example, Cmd+C should copy text, Cmd+A should select all text, etc.
- Add a green outline+glow around the text field while editing
- The textbox should be focused, the filename except .extension should be selected, and the cursor should be at the end of the text.
- The whole thing should render in a way that it doesn't shift the pane content anywhere, and hopefully even the file name text itself should be positioned precisely in the place of the static filename.
- When the filename is longer and clipped, the whole filename should be in the textbox of course. It'll scroll, no problem.
- Accessibility: We should use ARIA roles/live regions for the editing state. Screen readers should announce that the entry is now editable, and announce validation errors.
- On read-only volumes, we should show a dialog saying something like "This is a read-only volume. Rename is not possible here. [OK]"

Saving/closing:
- Pressing `Enter` ends renaming and saves progress.
- ESC discards the rename.
- Clicking somewhere else in the app discards rename. Similarly for keyboard focus moving to the other pane (`Tab`, or clicking a toolbar button, etc.)
- If any drag&drop event happens (starts internally or comes from an external app while editing), cancel/discards the rename.
- Scroll action should cancel renaming, but with a cumulative threshold of 200px (horizontally in Brief mode, vertically in Full mode). This should probably not interfere with the virtual scrolling, but I'll test.

Validation and conflicts:
- The mechanism should always trim leading/trailing whitespaces when doing _anything_ with the filename. Like, in the textbox, we should keep the whitespaces, but they should be silently trimmed when checking/saving, always.
- If the user gives a name that's invalid, The editing border+glow should turn red ("error" state).
  - Invalid states:
    - It contains a disallowed char, e.g. slash or \0 on macOS, but this might also depend on the file system I guess?
    - Empty string (incl. whitespace-only)
    - 255+ bytes (not chars!), I think, but this might be FS-dependent. Also 1024-byte or similar _path_ limits.
    - Changed file extension, if the setting for it is "No" in settings. (If that setting is `Yes` or `Always ask`, there should be no validation error/waning)
  - The whole invalid filename check should be **extracted for reuse** (check it, maybe we already have it for file transfers or something?). It should have logic per file system if needed, but I think definitely per-OS later (add TODO in the code), so we don't reimplement it for each future feature.
  - If the user presses Enter in the invalid state, we should shake the textbox in a no-no motion and show a notification at the top-right that describes the reason, with the text being in line with out style guide.
    - The error message should disappear on the next keypress or mouse click.
  - If the user clicks elsewhere, the editing should end with the change reverted.
- If the new name clashes with an existing file, the border+glow should turn yellow. ("warning" state)
  - If the user initiates saving, either by clicking away or pressing Enter, we should show a dialog asking the user if they want to overwrite the existing file.
    - The dialog should show the original file and the file that exists by its new name, displaying their size and last mod time with UI elements that are consistent with the rest of the app. The conflict handler window for Copy/Move should be good inspiration, although maybe we can do even better. From the dialog, it should be intuitively clear which file is bigger and which one is newer. There should also be these buttons next to each other: `[Overwrite and Trash old file] [Overwrite and delete old file] [Cancel] [Continue renaming]` in this order. Pressing Enter should Overwrite, ESC should Continue renaming.
    - If they say Overwrite, we should proceed with the rename, giving some "force" param to the backend for the action.
    - If they say Cancel, we should revert the rename and keep the existing file name.
    - If they say Rename, we throw them back to the "editing state"
  - Some file systems like APFS are case-insensitive. Renaming Readme.md → README.md is valid, but exists() will return true for the new name since it's the same file. We need to design the conflict detection so that it doesn't incorrectly flag it as a name clash (yellow border, overwrite dialog). We need inode comparison or a simple case-sensitive sting comparison for case-only renames. This is one of the most common rename operations.
- If the old name is the same name, we should do nothing (No virtual in-place rename), same as if the user canceled the action
- If the new name is a dot-prefixed name, and hidden files are not shown, we should show a notification at the top right, something like "Your file disappeared from view because hidden files are not shown.". Notifications like this can be [x] closed and auto-disappear after the first navigation event (changing dirs). Use a standard Ark JS notification, or reuse it if we already have one. Style should be [info], not warning or anything.
- File extension changes should trigger a confirmation, with a dialog with this content: `Are you sure you want to change the extension from “.A” to “.B”? Your file may open in a different app next time you open it. \n [Keep .{A}] [Use .{B}] \n [x] Always allow extension changes`. "Always allow" should set an "Allow file extension changes: Yes/No/Always Ask" setting that the user can find in Settings > General > File operations.

Edge cases and conflict resolution:
- File watching and updates should still work during renaming! 100% reliable. The entry being edited should be kept in view at all times. If the file gets deleted/renamed externally, then the action should stop, otherwise continue. Validation may not re-run
  on watcher events, we don't care about such an edge case. The backend will fail it anyway
- While renaming, other features like sorting and showing/hiding hidden files should discard the rename.
- After the rename, the list should be re-sorted to reflect the new order, but it shouldn't reload entirely so it's fast even for network drives and huge folders. We can probably rely on the file watcher mechanism to pick up the file name change. However, it's important that the cursor remains on the renamed file so after the file watcher update, we need to select the file by its new name. Note that the file might now be out of view or not even loaded on the front end if it's a large folder.
- MTP volumes — the Volume trait has rename() for MTP too, but MTP rename might be slow or have different error modes. We'll need to test this, but it should work. Can wait till later.
- If the backend responds with an error to the rename action (e.g. file name conflict or unmount appeared since our file watcher last flagged changes), the front end should discard the rename and show an error notification at the top-right. This might happen for many reasons (I/O error, disk full, who knows), we should display the best error message we can, but I bet we have limitations.
- If renaming is slow (e.g. network drive), the experience will suck. We accept this for now.

Back-end logic
- The `fs.rename()` call already exists on the back end in `fn rename(&self, from: &Path, to: &Path) -> Result<(), VolumeError>`, but we need to extend this with a `force: bool` param.
- `std::fs::rename` on POSIX silently overwrites, BUT our backend shouldn't just do a `std::fs::rename`, it should actually check first if there is a conflict and reject it with an error unless it got the "force" param.
- Renaming a symlink should rename the link, not follow it. `std::fs::rename` does the right thing but wanted to be explicit.
