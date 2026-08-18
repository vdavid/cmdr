# Rename

Inline file/folder rename with validation, conflict resolution, and extension-change confirmation. Activated by F2,
Shift+F6, the context menu, or click-to-rename (800 ms hold on the name). Operates on the cursor item only; selection is
preserved and irrelevant.

## Module map

- **InlineRenameEditor.svelte**: the inline input that replaces the name cell.
- **RenameConflictDialog.svelte** / **ExtensionChangeDialog.svelte**: the two mid-flow confirmation dialogs.
- **rename-state.svelte.ts**: reactive state (`.svelte.ts` for Svelte 5 reactivity).
- **rename-operations.ts**: pure save flow, returns a `RenameResult` discriminated union (`noop` / `error` / `timeout` /
  `extension-ask` / `conflict` / `success`).
- **rename-activation.ts**: click-to-rename timer. **rename-step.ts**: the pure halves of a chained rename.

Full details (save flow, permission/validation tiers, post-rename cursor tracking, decisions): `DETAILS.md`.

## Must-knows

- **Same-name edit (`trimmedName === originalName`) is a cancel/no-op.** Don't emit a watcher event or refresh the pane;
  it avoids a spurious refresh on whitespace-only edits.
- **Case-only (`.JPG` → `.jpg`) and known-equivalent (`.jpeg` → `.jpg`, `.md` → `.txt`) extension changes count as no
  change under every extension policy**: no dialog, no red border. What each policy does with a real one: `DETAILS.md`.
- **Conflict detection on local FS compares `dev+ino` via `symlink_metadata()`, never `exists()`**: on case-insensitive
  APFS, `readme.txt` → `README.txt` is the same file, and `exists()` would false-positive.
- **A `renameFile` / `moveToTrash` timeout on a slow mount is not a failure**: the rename may have landed on disk, so
  warn honestly ("may have succeeded") and auto-refresh.
- **Thread `volumeId` through `renameFile` / `checkRenameValidity` / `checkRenamePermission`.** Conflict checks work on
  every volume via the Volume trait; permission checks are skipped for MTP (Unix `access()` doesn't reach MTP virtual
  paths).
- **Async work carries the session id it started with; a superseded session may only toast and refresh.** A save,
  permission check, or editor cancel landing after a newer activation must never cancel, focus, shake, move the cursor,
  or open a dialog. `DETAILS.md` § Rename sessions.
- **A bare arrow chains the rename to the next row; three things inside that step fail silently.** The save fires BEFORE
  the next activation (that's what tags it with the session that typed it); the new editor opens on the entry captured
  at keypress time, never on `entryUnderCursor` (which still names the file just left); and a conflict is dropped on the
  BACKEND's answer only, never on the frontend's warning, whose sibling snapshot the chain itself makes stale. The
  keypress tests `severity === 'error'`, which is also what honors the "no" extension policy. `DETAILS.md` § Chaining.
- **Clicking outside the editor SAVES; losing focus any other way discards.** The commit hangs off a document
  `mousedown` (capture) that `InlineRenameEditor` owns, never off blur: blur can't tell a click from the row scrolling
  out of the virtual window (which UNMOUNTS the input, at no distance threshold), and committing on a scroll would
  rename a file nobody chose to. Enter saves too. Guards and the invalid-name toast: `DETAILS.md` § Ending a rename
  session.
- **Clicks INSIDE the editor must reach the input.** `FilePane.handlePaneClick` and both views' row `mousedown` skip
  targets under `.rename-input`; without that the pane grabs focus and the rename dies mid-caret-placement. Pinned by
  E2E in `file-operations.spec.ts`.
- **The editor mounts BY PATH** (`shouldMountRenameEditor`), so a watcher diff that shifts OTHER rows makes it follow
  its file; only a diff removing the renamed file itself cancels (`listing-diff-sync`). Other watcher events don't (the
  backend catches issues on save).
- **Double-click on the name area opens the file/folder, never rename**: the click-to-rename timer cancels on a
  double-click.
- **While rename is active, Cmd+C/A/Z/X/V act as text-editing shortcuts, not app commands** (same flag mechanism as
  dialogs). Every other app shortcut is suppressed.
