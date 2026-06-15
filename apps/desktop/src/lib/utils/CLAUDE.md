# Utils

Small stateless utility functions. Pure, no Svelte state, safe to import from plain `.ts` files.

## Files

- **`filename-validation.ts`**: pure client-side filename validation for instant keystroke feedback.
- **`confirm-dialog.ts`**: wrapper around Tauri's native dialog `ask()`.
- **`timing.ts`**: `withTimeout`, `createDebounce`, `createThrottle`.
- **`shorten-middle.ts`**: `shortenMiddle` mid-truncation + `createPretextMeasure` factory.
- **`shorten-middle-action.ts`**: Svelte action wrapping `shortenMiddle` with ResizeObserver + async pretext.
- **`pluralize.ts`**: count + noun formatting ("1 user" / "3 users").
- **`srgb-mix.ts`**: sRGB color helpers (`mixSrgb`, `withAlpha`, `parseHex`, `toHex`, `relativeLuminance`,
  `contrastRatio`, `readableFgOn`).
- **`webkit-compat.ts`**: one-shot `color-mix()` feature detection + boot-time telemetry log.

## Must-knows

- **Validation runs on the frontend (pure TS), not via Rust round-trips.** Keystroke feedback needs sub-millisecond
  latency; an IPC round-trip per keystroke would stutter. All rules (length, chars, conflicts) are deterministic given
  the sibling list, so no filesystem access is needed. Don't move validation to the backend.
- **Length limits are `>= 255` bytes (name) and `>= 1024` bytes (path), strictly**, not `> 255`: the filesystem reserves
  the last byte. Byte length is measured with `TextEncoder`, not `.length`, to handle multi-byte characters.
- **`validateConflict` is case-insensitive (APFS).** A case-only rename (`foo` → `Foo`) passes without warning. Pass
  `originalName` correctly or you get false positives. This assumes macOS/APFS; Linux support will need a per-filesystem
  case-sensitivity flag.
- **`getExtension(filename)` returns the extension WITH the dot** (`.txt`), or `''` for dotfiles without an extension
  (`.gitignore` → `''`), via `lastIndexOf('.') <= 0`.
- **`extensionsDifferMeaningfully(oldName, newName)` decides whether an extension change needs confirmation.** Returns
  false for case-only changes (`.JPG` → `.jpg`) and known equivalents (`.jpeg` → `.jpg`, `.md` → `.txt`); the
  equivalence groups live in `EQUIVALENT_EXTENSION_GROUPS` in the same file. Extend that constant to add aliases. Used
  by both `validateExtensionChange` and the rename save flow's "ask" gate.
- **Use `confirmDialog` everywhere instead of `window.confirm()`** (unreliable in Tauri). It wraps Tauri's `ask()` with
  an explicit `cancelLabel: 'Cancel'`: macOS `NSAlert` only assigns Escape to a button labeled "Cancel", so without the
  override Escape does nothing in confirmation dialogs.
- **The CSS ships `color-mix()` heavily, which Safari < 16.2 (still on macOS 12 Monterey) doesn't parse.** Two safety
  nets must both stay: `app.css` static fallbacks inside `@supports not (color: color-mix(...))` blocks, and
  `accent-color.ts` / `volume-tint.svelte.ts` computing runtime-derived colors in JS via `mixSrgb` / `withAlpha` (the
  tokens that depend on the live macOS accent color). Don't introduce `color-mix()` for accent-derived tokens.
- **`readableFgOn(accentHex)` (returns `#000000` / `#ffffff` by WCAG contrast) is mirrored in
  `scripts/check-a11y-contrast/accent_matrix.go`.** Keep the JS and Go logic in sync, or the design-time contrast
  checker tests a different fg than the app renders.

## Gotchas

- **`validateFilename` returns the FIRST error or warning, not a list.** Checks run in priority order (errors before
  warnings): `validateNotEmpty` → `validateDisallowedChars` (`/` or `\0`) → `validateNameLength` → `validatePathLength`
  → `validateExtensionChange` → `validateConflict`. Inline rename UI has space for one message.
- **`validateDirectoryPath(path)` validates full paths, not filenames** (empty, must-be-absolute, null byte, total
  length, per-component length). Used by TransferDialog; composable in NewFolderDialog.
- **`'ask'` extension setting returns `ok` at validation time**; the save dialog handles the confirmation separately.
- **`createDebounce` exposes `flush()`** (for `beforeunload` cleanup, e.g. the log bridge) and `createThrottle`
  guarantees a trailing call. Both are hand-rolled (<35 lines each) deliberately, not lodash.
- **`useShortenMiddle` action takes `tooltipWhenTruncated?` (default `false`)**: when set, the native `title` applies
  only when truncation actually trimmed the string. `VITE_CMDR_FORCE_OLD_WEBKIT=1 pnpm dev` forces the old-WebKit
  fallback path on modern WebKit (see DETAILS.md and `docs/guides/releasing.md` § old-macOS smoke test).

Full details (full export catalogs, validator/decision rationale, the old-WebKit dev override mechanism):
[DETAILS.md](DETAILS.md).
