# Utils

Small stateless utility functions. Pure, no Svelte state, safe to import from plain `.ts` files.

## Files

| File                          | Purpose                                                                    |
| ----------------------------- | -------------------------------------------------------------------------- |
| `filename-validation.ts`      | Pure client-side filename validation for instant keystroke feedback        |
| `filename-validation.test.ts` | Vitest tests covering all validators                                       |
| `confirm-dialog.ts`           | Wrapper around Tauri's native dialog API                                   |
| `timing.ts`                   | `withTimeout`, `createDebounce`, and `createThrottle` for timing control   |
| `timing.test.ts`              | Vitest tests for withTimeout, debounce, and throttle                       |
| `shorten-middle.ts`           | `shortenMiddle` mid-truncation + `createPretextMeasure` factory            |
| `shorten-middle.test.ts`      | Vitest tests for shortenMiddle (mock measureWidth, 14 tests)               |
| `shorten-middle-action.ts`    | Svelte action wrapping `shortenMiddle` with ResizeObserver + async pretext |

## filename-validation.ts

`validateFilename()` is the main orchestrator for single-file renames. It runs checks in priority order: errors first,
then warnings. Returns the first non-ok result, or `{ severity: 'ok', message: '' }`.

```
validateFilename()
  ├── validateNotEmpty()          — error if blank after trim
  ├── validateDisallowedChars()   — error if / or \0 present
  ├── validateNameLength()        — error if >= 255 bytes (UTF-8)
  ├── validatePathLength()        — error if >= 1024 bytes (UTF-8)
  ├── validateExtensionChange()   — error/ok depending on 'yes'|'no'|'ask' setting
  └── validateConflict()          — warning if a sibling already has that name (case-insensitive)
```

`validateDirectoryPath()` validates full directory paths (not filenames). Used by TransferDialog and composable with
individual validators in NewFolderDialog.

```
validateDirectoryPath(path)
  ├── empty check                 — error if blank after trim
  ├── absolute check              — error if doesn't start with /
  ├── null byte check             — error if contains \0
  ├── total path length           — error if >= 1024 bytes (UTF-8)
  └── per-component length        — error if any segment >= 255 bytes (splits on /, filters empty)
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
- `extensionsDifferIgnoringCase(oldName, newName)` is the shared helper that decides whether an extension change is
  meaningful. Case-only changes (e.g. `.JPG` → `.jpg`) are treated as no change so users aren't pestered to confirm a
  metadata tweak. Used by both `validateExtensionChange` and the rename save flow's "ask" gate.

## confirm-dialog.ts

Thin wrapper around `@tauri-apps/plugin-dialog`'s `ask()`. Use this everywhere instead of `window.confirm()`, which is
unreliable in Tauri.

```ts
confirmDialog(message: string, title?: string): Promise<boolean>
```

Shows a native warning dialog with OK/Cancel. Resolves `true` on confirm.

## Key decisions

**Decision**: Validation runs on the frontend (pure TS) instead of round-tripping to Rust. **Why**: Keystroke-level
feedback needs sub-millisecond latency. An IPC round-trip per keystroke would add ~1-5ms and could stutter during fast
typing. All the rules (length, chars, conflicts) are deterministic given the sibling list, so there is no need for
filesystem access.

**Decision**: `validateFilename` returns the first error or warning, not a list of all issues. **Why**: Inline rename UI
has space for one message. Showing the highest-priority issue keeps the feedback focused. Errors are checked before
warnings so a blocking issue always takes precedence over an advisory one.

**Decision**: Case-insensitive conflict check (APFS default) rather than per-filesystem logic. **Why**: macOS (APFS) is
the only supported platform today. The check is case-insensitive to match the default APFS case-insensitive behavior.
When Linux support ships, this will need a per-filesystem case-sensitivity flag.

**Decision**: `confirmDialog` wraps Tauri's `ask()` with explicit `cancelLabel: 'Cancel'` instead of the default.
**Why**: The default label is "No", but macOS `NSAlert` only assigns the Escape key equivalent to a button labeled
"Cancel". Without this override, Escape does nothing in confirmation dialogs — a jarring UX break.

**Decision**: Custom `createDebounce`/`createThrottle` instead of lodash or a library. **Why**: Both are <35 lines. The
throttle guarantees a trailing call (last value always fires), which lodash's default does not. The debounce exposes
`flush()` for `beforeunload` cleanup (e.g. log bridge). No need for a 70KB dependency.

## shorten-middle.ts

`shortenMiddle()` truncates text in the middle with an ellipsis, using pixel-accurate width measurement via an injected
`measureWidth` function. Supports `preferBreakAt` (snap cuts to a delimiter like `/`), `startRatio` (bias budget toward
start or end), and custom ellipsis strings.

`createPretextMeasure()` creates a `measureWidth` function backed by `@chenglou/pretext`'s `prepareWithSegments` +
`measureNaturalWidth`. Caches prepared texts for repeated measurements of the same string.

## Dependencies

- `filename-validation.ts` — zero external dependencies
- `confirm-dialog.ts` — `@tauri-apps/plugin-dialog`
- `shorten-middle.ts` — `@chenglou/pretext` (type import only; runtime import via `createPretextMeasure` caller)
