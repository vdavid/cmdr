# Dialog path validation plan

## Context

When users edit the destination path in Copy/Move (TransferDialog) or the folder name in Create dir (NewFolderDialog),
there's no validation for invalid characters, empty input, or length limits. The backend catches these on submit, but
the user gets no real-time feedback. Meanwhile, the Rename feature already has well-architected, reusable validation in
`filename-validation.ts` (frontend) and `validation.rs` (backend). The goal is to reuse that existing logic.

## What exists today

### Reusable validators (`$lib/utils/filename-validation.ts`)
- `validateNotEmpty(name)` — rejects empty/whitespace-only
- `validateDisallowedChars(name)` — rejects `/` and `\0`
- `validateNameLength(name)` — rejects >= 255 bytes
- `validatePathLength(parentPath, name)` — rejects >= 1024 bytes total
- `validateConflict(newName, siblingNames, originalName)` — case-insensitive APFS conflict

All return `{ severity: 'error' | 'warning' | 'ok', message: string }`. Pure TS, zero deps.

### NewFolderDialog (`$lib/file-operations/mkdir/NewFolderDialog.svelte`)
- Lines 59-62: inline `includes('/')` and `includes('\0')` check — duplicates `validateDisallowedChars()`
- Lines 64-82: async `findFileIndex()` conflict check — richer than `validateConflict()` (distinguishes file vs folder)
- **Missing**: name length, path length

### TransferDialog (`$lib/file-operations/transfer/TransferDialog.svelte`)
- Lines 118-141: `getPathValidationError()` — only checks logical conflicts (subfolder, same location)
- **Missing**: empty check, null byte check, path length check, per-component length check
- Note: slashes are valid here (it's a full path), so `validateDisallowedChars()` doesn't apply directly

## Plan

### 1. Add a `validateDirectoryPath()` function to `filename-validation.ts`

New function for validating full directory paths (not filenames). Checks, in order:
- Not empty after trim
- Must start with `/` (absolute path required)
- No `\0` characters (slashes are valid in paths)
- Total path length < 1024 bytes (reuse `MAX_PATH_BYTES` constant)
- Each non-empty path component < 255 bytes (split on `/`, filter out empty segments from leading/trailing/double slashes)

Error messages should be path-appropriate: "Path can't be empty", "Path must be absolute (start with /)",
"Path contains a null character", "Path is too long (X/1024 bytes)", "A folder name in the path is too long (X/255
bytes)".

This keeps all validation in one place. The existing `validateDisallowedChars` stays filename-specific (rejects `/`).

### 2. Wire `validateDirectoryPath()` into TransferDialog

In `TransferDialog.svelte`:
- Import `validateDirectoryPath` from `$lib/utils/filename-validation`
- Change `pathError` derived to run `validateDirectoryPath(editedPath)` first; if that returns an error, use it.
  Otherwise fall through to the existing `getPathValidationError()` for logical checks (subfolder, same location).
  Structural errors (empty, invalid, too long) take priority over logical errors.
- The confirm button is already disabled when `pathError` is truthy, so no button logic changes needed
- Validation is synchronous — no debounce needed, instant feedback on every keystroke

### 3. Replace inline validation in NewFolderDialog with shared validators

In `NewFolderDialog.svelte`:
- Import `validateDisallowedChars`, `validateNameLength`, `validatePathLength` from `$lib/utils/filename-validation`
- Replace the inline char check (lines 59-62) with these three validators run synchronously at the top of `validateName()`
- If any sync validator returns an error, set `errorMessage` immediately and return (skip the async check)
- If sync validators pass, clear `errorMessage`, then run the existing async `findFileIndex()` conflict check as before.
  Keep it — it's richer than `validateConflict()` (distinguishes "file" vs "folder" in the message) and doesn't need a
  preloaded sibling list.
- This gains name length and path length validation for free

### 4. Add tests

- Add `validateDirectoryPath()` tests to `filename-validation.test.ts`: empty, relative path, null byte, long total path,
  long single component, trailing slashes, double slashes, valid paths
- Verify existing NewFolderDialog and TransferDialog tests still pass (if any)

### 5. Update docs

- Update `$lib/utils/CLAUDE.md` — add `validatePath()` to the function tree and document its purpose
- Update `$lib/file-operations/CLAUDE.md` — note that NewFolderDialog now uses shared validators

## Files to modify

| File | Change |
|------|--------|
| `apps/desktop/src/lib/utils/filename-validation.ts` | Add `validateDirectoryPath()` |
| `apps/desktop/src/lib/utils/filename-validation.test.ts` | Add tests for `validatePath()` |
| `apps/desktop/src/lib/file-operations/transfer/TransferDialog.svelte` | Wire in `validateDirectoryPath()` |
| `apps/desktop/src/lib/file-operations/mkdir/NewFolderDialog.svelte` | Replace inline checks with shared validators |
| `apps/desktop/src/lib/utils/CLAUDE.md` | Document `validatePath()` |
| `apps/desktop/src/lib/file-operations/CLAUDE.md` | Note shared validator usage |

## Verification

1. `cd apps/desktop && pnpm vitest run -t "filename-validation"` — new + existing tests pass
2. `./scripts/check.sh --check desktop-svelte-eslint --check svelte-check --check desktop-svelte-prettier` — no lint/type errors
3. Manual testing with MCP:
   - Open TransferDialog (F5), edit path to empty / path with null byte / very long path — error shows, button disabled
   - Open NewFolderDialog (F7), type a name > 255 bytes / path that would exceed 1024 — error shows, button disabled
   - Confirm happy paths still work (normal copy, normal mkdir)
