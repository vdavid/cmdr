# Rename

Inline file/folder rename with validation, conflict resolution, and extension change confirmation.

## Purpose

Provides inline rename activated by F2 (or Shift+F6), context menu, or click-to-rename (800ms hold on cursor item name).
Operates on cursor item only; selection is preserved and irrelevant.

## Architecture

### Components

- **InlineRenameEditor.svelte**: Input field that replaces static name cell. Green/yellow/red border based on validation
  state. 300ms glow/zoom animation on activation. Pre-selects filename excluding extension.
- **RenameConflictDialog.svelte**: Side-by-side file comparison (size, modified) when conflict detected. Options:
  "Overwrite and trash old file" (NSFileManager.trashItem), "Overwrite and delete old file" (permanent), "Cancel",
  "Continue renaming".
- **ExtensionChangeDialog.svelte**: Confirmation when extension changed and policy is "ask". Buttons: "Keep .{old}",
  "Use .{new}". Checkbox: "Always allow extension changes" (sets policy to "yes").
- **rename-state.svelte.ts**: Reactive state ($state) for active/target/currentName/validation/shaking/focusTrigger.
  Must be .svelte.ts for Svelte 5 reactivity.
- **rename-operations.ts**: Pure logic for save flow. Returns instructions (`RenameResult` discriminated union) instead
  of side effects.
- **rename-activation.ts**: Click-to-rename timer logic (800ms hold, 10px threshold, cancel on double-click).

### Three-stage save flow

Implemented in `rename-operations.ts::executeRenameSave()`:

1. **Extension check**: If `extensionPolicy === 'ask'` and `getExtension(originalName) !== getExtension(newName)`,
   return `{ type: 'extension-ask' }`. Caller shows ExtensionChangeDialog. If user clicks "Keep", retry with
   `skipExtensionCheck=true`.

2. **Backend validity check**: Call `checkRenameValidity(parentPath, originalName, trimmedName)`. Returns:
    - `{ valid: false, error }` → return `{ type: 'error' }`
    - `{ valid: true, hasConflict: true, isCaseOnlyRename: false }` → return `{ type: 'conflict', validity }`
    - `{ valid: true, hasConflict: true, isCaseOnlyRename: true }` → proceed (same inode, just case change)
    - `{ valid: true, hasConflict: false }` → proceed

3. **Perform rename**: Call `renameFile(from, to, force)`. On success, return `{ type: 'success', newName }`.

Conflict resolution calls `performRename(target, newName, force: true)` after user chooses "Overwrite and trash/delete".

### Permission check on activation

`checkRenamePermission(path)` verifies:

- Parent dir writable (Unix `access(W_OK)`)
- File not immutable (`UF_IMMUTABLE` flag)
- File not SIP-protected (`SF_IMMUTABLE` flag)

Called on activation. If fails, auto-cancel and show notification. On read-only volumes, show modal alert "This is a
read-only volume. Renaming isn't possible here."

### Validation

**Frontend (instant feedback)**: `filename-validation.ts` checks:

- Disallowed chars (slash, null on macOS)
- Empty/whitespace-only
- Byte limits (255 for name, 1024 for path)
- Extension change vs setting

**Backend (authoritative)**: `validation.rs` + `check_rename_validity` Tauri command. Uses inode comparison to detect
case-only renames (same dev+ino on case-insensitive APFS).

Both use platform-dependent logic with TODOs for future OSes.

### Post-rename cursor tracking

File watcher emits `directory-diff` event → `findFileIndex(listingId, newName)` → convert to frontend index →
`setCursorIndex()`. If renamed to dot-prefixed name while hidden files off, show info notification "Your file
disappeared from view because hidden files aren't shown."

## Key decisions

### Why separate components in file-explorer/rename/?

Rename is tightly coupled to FilePane rendering (replaces name cell inline) and uses Svelte 5 $state() (requires
.svelte.ts). Transfer operations are self-contained dialogs that don't touch FilePane internals. Separation reflects
architectural boundary.

### Why inode comparison for conflict detection?

On case-insensitive APFS, renaming "readme.txt" → "README.txt" is valid (same file). Naive `exists()` check would flag
false positive. Comparing `dev+ino` via `symlink_metadata()` detects case-only renames correctly.

### Why three separate dialogs (conflict, extension, permission)?

Each triggers at different stage:

- Permission: on activation (before editing starts)
- Extension: after user attempts save (mid-flow, can continue editing)
- Conflict: after validity check (mid-flow, can continue editing)

Combining into one dialog would require complex multi-state logic. Separate dialogs keep each concern isolated.

### Why trim silently instead of showing error?

Leading/trailing whitespace in filenames is almost always unintentional. Silently trimming when checking/saving avoids
false errors while preserving whitespace in the input for transparency. User sees what they typed but save logic uses
trimmed value.

## Gotchas

- **Cancel triggers**: Escape, click elsewhere, Tab, drag start, scroll >200px cumulative, sort/hidden toggle all
  discard rename. File watcher events during editing don't cancel (backend will catch issues on save).
- **Extension validation gotcha**: If setting is "no", changing extension shows red border during editing. If setting is
  "ask", no red border (waits for save to show dialog). If setting is "yes", never validates extension.
- **Same-name edge case**: If `trimmedName === originalName`, treat as cancel (no-op). Don't emit file watcher event or
  refresh pane. Avoids spurious refresh on whitespace-only edits.
- **Click-to-rename interference**: Double-click on name area must open file/folder (normal behavior), not activate
  rename. Timer checks for double-click event and cancels activation if detected.
- **App-level shortcut suppression**: While rename active, Cmd+C/A/Z/X/V work as text editing shortcuts (not app
  commands). Implemented by setting flag in keyboard handler (same mechanism as dialogs). Other shortcuts (Cmd+O, arrow
  keys, etc.) are suppressed.
- **Refresh timing**: File watcher event arrives asynchronously. `moveCursorToNewFolder()` pattern: subscribe to
  `directory-diff`, wait 50ms after event for listing cache update, then query `findFileIndex()`. Cleanup listener after
  3s timeout.
