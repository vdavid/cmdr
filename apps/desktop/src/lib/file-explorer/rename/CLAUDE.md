# Rename

Inline rename with validation, conflict resolution, and extension-change confirmation. Activated by F2, Shift+F6, the
context menu, or click-to-rename. Operates on the cursor item only; selection is untouched.

## Module map

- **InlineRenameEditor.svelte**: the inline input that replaces the name cell.
- **RenameConflictDialog.svelte** / **ExtensionChangeDialog.svelte**: the mid-flow confirmation dialogs.
- **rename-state.svelte.ts**: reactive state, and the session ids.
- **rename-operations.ts**: pure save flow, returns the `RenameResult` union.
- **rename-activation.ts**: click-to-rename timer. **rename-step.ts**: the pure halves of a chained rename.
  **sibling-names.ts**: the directory's names, read once per chain.

Full details (save flow, validation tiers, cursor tracking, decisions): `DETAILS.md`.

## Must-knows

- **Same-name edit (`trimmedName === originalName`) is a cancel/no-op.** Don't emit a watcher event or refresh the pane;
  a whitespace-only edit would refresh for nothing.
- **Case-only (`.JPG` → `.jpg`) and known-equivalent (`.jpeg` → `.jpg`, `.md` → `.txt`) extension changes count as no
  change under every extension policy**: no dialog, no red border. What a policy does with a real one: `DETAILS.md`.
- **Conflict detection on local FS compares `dev+ino` via `symlink_metadata()`, never `exists()`**: on case-insensitive
  APFS `readme.txt` → `README.txt` is the same file, which `exists()` would call a conflict.
- **A `renameFile` / `moveToTrash` timeout is not a failure**: the rename may have landed on disk, so warn honestly
  ("may have succeeded"), never as a kept name, and refresh. Chained ones share ONE toast and ONE debounced refresh.
- **Thread `volumeId` through `renameFile` / `checkRenameValidity` / `checkRenamePermission`.** Conflict checks work on
  every volume via the Volume trait; permission checks are skipped for MTP (Unix `access()` can't reach MTP paths).
- **Async work carries the session id it started with; a superseded session may only toast and refresh.** A save,
  permission check, or editor cancel landing after a newer activation must never cancel, focus, shake, move the cursor,
  or open a dialog. `DETAILS.md` § Rename sessions.
- **A bare arrow chains the rename to the next row; five things inside that step fail silently.** The save fires BEFORE
  the next activation (that's what tags it with the session that typed it); the new editor opens on the entry captured
  at keypress time, never on `entryUnderCursor` (still the file just left); that entry is the row BESIDE the editor's
  own, never one at an index (backend, cursor, and window disagree about indices mid-chain); a conflict is dropped on
  the BACKEND's answer only, never on the cached sibling names the chain makes stale (the keypress tests
  `severity === 'error'`, which also honors the "no" extension policy); and kept names go into ONE running toast,
  unconfirmed ones into another, since a persistent toast each is dropped past the fifth. `DETAILS.md` § Chaining.
- **Clicking outside the editor SAVES; losing focus any other way discards.** The commit hangs off a document
  `mousedown` (capture), never off blur: blur can't tell a click from the row scrolling out of the virtual window, and
  committing on a scroll would rename a file nobody chose. Enter saves too. Guards and the invalid-name toast:
  `DETAILS.md` § Ending a rename session.
- **Clicks INSIDE the editor must reach the input.** `FilePane.handlePaneClick` and both views' row `mousedown` skip
  targets under `.rename-input`; without that the pane grabs focus and the rename dies mid-caret-placement.
- **The editor mounts BY PATH** (`shouldMountRenameEditor`), so a watcher diff that shifts OTHER rows makes it follow
  its file; only a diff removing the renamed file itself cancels (`listing-diff-sync`). Other watcher events don't (the
  backend catches issues on save).
- **Double-click opens, never renames**: the click-to-rename timer cancels on it.
- **While rename is active, Cmd+C/A/Z/X/V edit text** (same flag as dialogs); every other shortcut is suppressed.
