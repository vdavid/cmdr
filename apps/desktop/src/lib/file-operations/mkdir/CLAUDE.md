# New folder

F7 dialog to create a folder in the focused pane: name validation, conflict detection, optional AI name suggestions.

Backend: `create_directory` lives directly under
[`write_operations/`](../../../../src-tauri/src/file_system/write_operations/CLAUDE.md) (no dedicated subdir).

## Module map

- **`NewFolderDialog.svelte`**: dialog UI, debounced validation + async conflict check, AI-suggestion streaming, timeout
  warning, post-create cursor positioning.
- **`new-folder-operations.ts`**: `getInitialFolderName()` (from cursor entry) and `moveCursorToNewFolder()`.
- **`new-folder-utils.ts`**: pure `removeExtension()` for deriving the initial name.

## Gotchas

- **In `moveCursorToNewFolder`, `paneRef.setPendingCursorName(name)` MUST run before the optimistic `setCursorIndex`.**
  `create_directory` queues a synthetic `directory-diff` (~50 ms trailing-window coalesce in `diff_emitter`). When that
  deferred diff lands, `FilePane`'s handler runs the new entry through `adjustSelectionIndices`, and an `add` at the
  cursor's index shifts the cursor +1. `setPendingCursorName` writes the same field the diff handler checks for the
  rename flow, so it re-pins by name and `return`s before the structural shift. Regression guard:
  `file-operations.spec.ts › Create folder round-trip › cursor lands on the newly created folder`.
- **`moveCursorToNewFolder` is shared with `mkfile`**: cursor positioning is entry-type-agnostic, so don't fork it.

Architecture, flows, and decisions: [DETAILS.md](DETAILS.md). Read it before any non-trivial work here: editing, planning, reorganizing, or advising.
