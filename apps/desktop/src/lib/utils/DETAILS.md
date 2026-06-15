# Utils details

Depth for the shared utilities. `CLAUDE.md` holds the must-knows.

## filename-validation.ts

`validateFilename()` orchestrates single-file renames, running checks in priority order (errors first, then warnings),
returning the first non-ok result or `{ severity: 'ok', message: '' }`:

```
validateFilename()
  ├── validateNotEmpty()          : error if blank after trim
  ├── validateDisallowedChars()   : error if / or \0 present
  ├── validateNameLength()        : error if >= 255 bytes (UTF-8)
  ├── validatePathLength()        : error if >= 1024 bytes (UTF-8)
  ├── validateExtensionChange()   : error/ok depending on 'yes'|'no'|'ask' setting
  └── validateConflict()          : warning if a sibling already has that name (case-insensitive)
```

`validateDirectoryPath(path)` validates full directory paths (used by TransferDialog, composable with individual
validators in NewFolderDialog):

```
validateDirectoryPath(path)
  ├── empty check                 : error if blank after trim
  ├── absolute check              : error if doesn't start with /
  ├── null byte check             : error if contains \0
  ├── total path length           : error if >= 1024 bytes (UTF-8)
  └── per-component length         : error if any segment >= 255 bytes (splits on /, filters empty)
```

Key types:

```ts
type ValidationSeverity = 'error' | 'warning' | 'ok'
interface ValidationResult {
  severity: ValidationSeverity
  message: string
}
```

Extension-change behavior is controlled by the `allowExtensionChanges` user setting (`yes`/`no`/`ask`). `'ask'` returns
`ok` at validation time; the save dialog handles it separately. `extensionsDifferMeaningfully` (with
`EQUIVALENT_EXTENSION_GROUPS`) gates the "ask" confirmation so users aren't pestered over case-only or known-equivalent
changes.

## confirm-dialog.ts

Thin wrapper around `@tauri-apps/plugin-dialog`'s `ask()`. `confirmDialog(message, title?): Promise<boolean>` shows a
native warning dialog with OK/Cancel and resolves `true` on confirm.

## shorten-middle.ts

`shortenMiddle()` truncates text in the middle with an ellipsis, using pixel-accurate width measurement via an injected
`measureWidth` function. Supports `preferBreakAt` (snap cuts to a delimiter like `/` or `.`), `startRatio` (bias budget
toward start or end), and custom ellipsis strings. `createPretextMeasure()` creates a `measureWidth` backed by
`@chenglou/pretext`'s `prepareWithSegments` + `measureNaturalWidth`, caching prepared texts for repeated measurements.

## srgb-mix.ts / webkit-compat.ts

`webkit-compat.ts` exposes `hasColorMix` (computed once at module load) so consumers can branch, and `logWebkitCompat()`
which the main layout calls at boot, emitting one log line so affected users show up in error reports. `srgb-mix.ts`
also exports `relativeLuminance`, `contrastRatio`, and `readableFgOn`. `readableFgOn(accentHex)` returns `#000000` or
`#ffffff` by whichever has the higher WCAG contrast against the accent; used by `accent-color.ts` to compute
`--color-accent-fg` per runtime accent, and mirrored in `scripts/check-a11y-contrast/accent_matrix.go`.

**Dev override**: `VITE_CMDR_FORCE_OLD_WEBKIT=1 pnpm dev` forces the fallback path on modern WebKit. At module load it
flips `hasColorMix` to `false` (routing the JS-mix branches) and sets `data-force-old-webkit` on `<html>` (activating
the `:root[data-force-old-webkit]` blocks in `app.css` that mirror the `@supports not (...)` fallbacks). Use it to
verify the old-WebKit look without a real Safari 15.x environment. See `docs/guides/releasing.md` § "Pre-release smoke
test on old macOS".

## Decisions

- **Frontend (pure TS) validation, not Rust round-trips**: keystroke feedback needs sub-millisecond latency; the rules
  are deterministic given the sibling list.
- **First error/warning, not a full list**: inline rename UI has space for one message; errors before warnings so a
  blocking issue takes precedence.
- **Case-insensitive conflict check (APFS default)**: macOS is the only supported platform today; Linux will need a
  per-filesystem case-sensitivity flag.
- **`confirmDialog` overrides `cancelLabel: 'Cancel'`**: macOS `NSAlert` assigns the Escape equivalent only to a button
  labeled "Cancel".
- **Custom `createDebounce`/`createThrottle` over lodash**: both under 35 lines; the throttle guarantees a trailing call
  (lodash's default doesn't), and the debounce exposes `flush()` for `beforeunload` cleanup. No 70 KB dependency.

## Dependencies

- `filename-validation.ts`: zero external dependencies.
- `confirm-dialog.ts`: `@tauri-apps/plugin-dialog`.
- `shorten-middle.ts`: `@chenglou/pretext` (type import only; runtime import via the `createPretextMeasure` caller).
