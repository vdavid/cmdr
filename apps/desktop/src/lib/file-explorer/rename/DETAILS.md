# Rename details

Depth and rationale for inline rename. `CLAUDE.md` holds the must-knows.

## Components

- **InlineRenameEditor.svelte**: input field that replaces the static name cell. Green/yellow/red border by validation
  state, 300 ms glow/zoom on activation, pre-selects the filename excluding the extension.
- **RenameConflictDialog.svelte**: side-by-side file comparison (size, modified) when a conflict is detected. Options:
  "Overwrite and trash old file" (`NSFileManager.trashItem`), "Overwrite and delete old file" (permanent), "Cancel",
  "Continue renaming".
- **ExtensionChangeDialog.svelte**: confirmation when the extension changed and policy is "ask". Buttons: "Keep .{old}",
  "Use .{new}". Checkbox: "Always allow extension changes" (sets policy to "yes").
- **rename-state.svelte.ts**: reactive `$state` for active/target/currentName/validation/shaking/focusTrigger. Must be
  `.svelte.ts` for Svelte 5 reactivity.
- **rename-operations.ts**: pure save-flow logic returning a `RenameResult` discriminated union instead of side effects.
- **rename-activation.ts**: click-to-rename timer logic (800 ms hold, 10 px threshold, cancel on double-click).

## Three-stage save flow (`rename-operations.ts::executeRenameSave()`)

`RenameResult` variants: `noop`, `error`, `timeout`, `extension-ask`, `conflict`, `success`.

1. **Extension check**: if `extensionPolicy === 'ask'` and extensions differ meaningfully
   (`extensionsDifferMeaningfully()` from `filename-validation.ts`), return `{ type: 'extension-ask' }`; the caller
   shows ExtensionChangeDialog. "Keep" retries with `skipExtensionCheck=true`. Case-only changes (`photo.JPG` →
   `photo.jpg`) and known-equivalent changes (`.jpeg` → `.jpg`, `.md` → `.txt`) skip the dialog entirely.
2. **Backend validity check**: `checkRenameValidity(parentPath, originalName, trimmedName)`. `valid: false` →
   `{ type: 'error' }`. `hasConflict: true, isCaseOnlyRename: false` → `{ type: 'conflict', validity }`.
   `hasConflict: true, isCaseOnlyRename: true` → proceed (same inode, just case). `hasConflict: false` → proceed.
3. **Perform rename**: `renameFile(from, to, force)`. Success → `{ type: 'success', newName }`. Timeout →
   `{ type: 'timeout', message }`; the caller shows a persistent warning toast (the rename may have succeeded) and
   auto-refreshes the listing.

Conflict resolution calls `performRename(target, newName, force: true)` after "Overwrite and trash/delete". The
`moveToTrash` call in the overwrite-trash path also has timeout detection (persistent toast + refresh).

## Permission check on activation (`checkRenamePermission(path)`)

Verifies: parent dir writable (Unix `access(W_OK)`), file not immutable (`UF_IMMUTABLE`), file not SIP-protected
(`SF_IMMUTABLE`). On failure, auto-cancel and notify. On read-only volumes, show modal alert "This is a read-only
volume. Renaming isn't possible here." Skipped for MTP volumes (Unix `access()` doesn't work on MTP virtual paths).

## Validation

- **Frontend (instant)** `filename-validation.ts`: disallowed chars (slash, null on macOS), empty/whitespace-only, byte
  limits (255 name, 1024 path), extension change vs setting.
- **Backend (authoritative)** `validation.rs` + `check_rename_validity` command, accepts an optional `volumeId`: local
  FS (`None` or `"root"`) uses `symlink_metadata` + inode comparison for case-only detection; non-local volumes (MTP)
  use `Volume::get_metadata()` for conflict detection, `is_case_only_rename` always `false` (MTP is case-sensitive).

## Post-rename cursor tracking

File watcher emits `directory-diff` → `findFileIndex(listingId, newName)` → frontend index → `setCursorIndex()`. If
renamed to a dot-prefixed name while hidden files are off, show "Your file disappeared from view because hidden files
aren't shown." The `moveCursorToNewFolder()` pattern: subscribe to `directory-diff`, wait 50 ms after the event for the
listing cache to update, query `findFileIndex()`, clean up the listener after a 3 s timeout.

## Decisions

- **Separate components in `file-explorer/rename/`**: rename is tightly coupled to FilePane rendering (replaces the name
  cell inline) and uses `$state()` (requires `.svelte.ts`). Transfer operations are self-contained dialogs that don't
  touch FilePane internals. The separation reflects the architectural boundary.
- **Inode comparison for conflict detection**: on case-insensitive APFS, `readme.txt` → `README.txt` is valid (same
  file). A naive `exists()` check flags a false positive; comparing `dev+ino` via `symlink_metadata()` detects case-only
  renames correctly.
- **Three separate dialogs (conflict, extension, permission)**: each triggers at a different stage (permission on
  activation; extension and conflict mid-flow, can continue editing). Combining them would need complex multi-state
  logic; separate dialogs keep each concern isolated.
- **Trim silently instead of erroring**: leading/trailing whitespace is almost always unintentional. The input preserves
  what the user typed (transparency) while save logic uses the trimmed value.
