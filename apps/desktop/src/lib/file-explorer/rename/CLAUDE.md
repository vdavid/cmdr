# Rename

Inline rename with validation, conflict resolution, and extension-change confirmation. Activated by F2, Shift+F6, the
context menu, or click-to-rename. Operates on the cursor item only; selection is untouched.

## Module map

- **InlineRenameEditor.svelte** in **RenameOverlay.svelte**: the inline input, laid over its row.
- **RenameConflictDialog.svelte** / **ExtensionChangeDialog.svelte**: the mid-flow confirmation dialogs.
- **rename-state.svelte.ts**: reactive state, and the session ids.
- **rename-operations.ts**: pure save flow, returns the `RenameResult` union.
- **rename-activation.ts**: click-to-rename timer. **rename-step.ts**: the pure halves of a chained rename.
  **sibling-names.ts**: the directory's names, read once per chain. **rename-selection.ts**: F2 cycle, dot stops.

Full details (save flow, validation tiers, cursor tracking, decisions): `DETAILS.md`.

## Must-knows

- **Same-name edit (`trimmedName === originalName`) is a no-op**: no watcher event, no refresh.
- **Case-only (`.JPG` → `.jpg`) and known-equivalent (`.jpeg` → `.jpg`, `.md` → `.txt`) extension changes count as no
  change under every extension policy**, as do non-package folders: no dialog, no red border. Details: `DETAILS.md`.
- **Conflict detection on local FS compares `dev+ino` via `symlink_metadata()`, never `exists()`**: on case-insensitive
  APFS `readme.txt` → `README.txt` is the same file, which `exists()` would call a conflict.
- **A slow rename is not a failure**: past the backend's deadline it's `still-renaming` and reports its real end later,
  never as a kept name until the volume refuses. Chained ones share ONE running toast. A `moveToTrash` timeout warns
  that it may have succeeded, and refreshes.
- **Thread `volumeId` through `renameFile` / `checkRenameValidity` / `checkRenamePermission`.** Conflict checks ride the
  Volume trait; the permission check is LOCAL, so whether it applies is Rust's call per volume. ❌ Never re-classify
  volumes here by id prefix (ERR-KVERS). `DETAILS.md` § Permission check on activation.
- **Async work carries the session id it started with; a superseded session may only toast and refresh.** A save,
  permission check, or editor cancel landing after a newer activation must never cancel, focus, shake, move the cursor,
  or open a dialog over the live editor (a `confirm-move` Move dialog waits for it to close). `DETAILS.md` § Rename
  sessions.
- **A bare arrow chains the rename to the next row, and five orderings inside that step fail silently**: the save fires
  BEFORE the next activation; the editor opens on the entry captured at keypress time, which is the row BESIDE it, ❌
  never one at an index; a conflict is dropped on the BACKEND's answer, ❌ never on cached sibling names; and kept vs
  still-renaming names pool into two running toasts. Each one's why: `DETAILS.md` § Chaining.
- **Clicking outside the editor SAVES; losing focus any other way discards.** The commit hangs off a document
  `mousedown` (capture), never off blur: blur can't tell a click from the row scrolling out of the virtual window, and
  committing on a scroll would rename a file nobody chose. Enter saves too. Guards and the invalid-name toast:
  `DETAILS.md` § Ending a rename session.
- **The field sits beside the listbox, ❌ never in the row** (`nested-interactive`), over the row's empty slot.
  `FilePane.handlePaneClick` and `isFileListBackgroundClick` skip `.rename-input`, or a click kills the rename.
- **The editor mounts BY PATH** (`shouldMountRenameEditor`), so it follows its file through a watcher diff; only a diff
  removing that file cancels (`listing-diff-sync`).
- **An open editor is the soft dialog `rename`** (`rename-editor-presence.ts`), synchronously, ❌ never from an effect.
  `DETAILS.md` § The editor as a state.
- **Double-click opens, never renames**: the click-to-rename timer cancels on it.
- **The ⌥ arrow dot stop fires only where the native word move would cross the dot**: ❌ never reimplement word
  movement. `DETAILS.md` § Selection.
- **While rename is active, Cmd+C/A/Z/X/V edit text** (same flag as dialogs); every other shortcut is suppressed.
