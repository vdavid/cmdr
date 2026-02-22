# Utils

Small stateless utility functions. Pure, no Svelte state, safe to import from plain `.ts` files.

## Files

| File                          | Purpose                                                             |
| ----------------------------- | ------------------------------------------------------------------- |
| `filename-validation.ts`      | Pure client-side filename validation for instant keystroke feedback |
| `filename-validation.test.ts` | Vitest tests covering all validators                                |
| `confirm-dialog.ts`           | Wrapper around Tauri's native dialog API                            |

## filename-validation.ts

`validateFilename()` is the main orchestrator. It runs checks in priority order: errors first, then warnings. Returns
the first non-ok result, or `{ severity: 'ok', message: '' }`.

```
validateFilename()
  ├── validateNotEmpty()          — error if blank after trim
  ├── validateDisallowedChars()   — error if / or \0 present
  ├── validateNameLength()        — error if >= 255 bytes (UTF-8)
  ├── validatePathLength()        — error if >= 1024 bytes (UTF-8)
  ├── validateExtensionChange()   — error/ok depending on 'yes'|'no'|'ask' setting
  └── validateConflict()          — warning if a sibling already has that name (case-insensitive)
```

Key types:

```ts
type ValidationSeverity = 'error' | 'warning' | 'ok'
interface ValidationResult {
    severity: ValidationSeverity
    message: string
}
```

### Gotchas

- Limits are `>= 255` and `>= 1024` (strictly), not `> 255` — the filesystem reserves the last byte.
- `TextEncoder` is used for byte length, not `.length`, to handle multi-byte characters correctly.
- `validateConflict` is case-insensitive (APFS). A case-only rename of the same file (e.g. `foo` → `Foo`) passes without
  warning. Pass `originalName` correctly or you'll get false positives.
- `getExtension(filename)` returns the extension including the dot (e.g. `.txt`), or `''` for dotfiles without extension
  (e.g. `.gitignore` → `''`). Implemented as `lastIndexOf('.') <= 0`.
- Extension change behavior is controlled by the `allowExtensionChanges` user setting (`yes`/`no`/`ask`). `'ask'`
  returns `ok` at validation time — the save dialog handles it separately.

## confirm-dialog.ts

Thin wrapper around `@tauri-apps/plugin-dialog`'s `ask()`. Use this everywhere instead of `window.confirm()`, which is
unreliable in Tauri.

```ts
confirmDialog(message: string, title?: string): Promise<boolean>
```

Shows a native warning dialog with OK/Cancel. Resolves `true` on confirm.

## Dependencies

- `filename-validation.ts` — zero external dependencies
- `confirm-dialog.ts` — `@tauri-apps/plugin-dialog`
