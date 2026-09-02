# Utils

Small stateless helpers. Pure, no Svelte state, safe to import from plain `.ts` files.

## Files

`filename-validation.ts` (client-side name and path validation), `timing.ts` (`withTimeout`, `createDebounce`,
`createThrottle`, `createCoalesced`, `waitForNextPaint`), `shorten-middle.ts` + `shorten-middle-action.ts`
(mid-truncation and its Svelte action), `srgb-mix.ts` + `webkit-compat.ts` (sRGB color math, `color-mix()` detection),
`confirm-dialog.ts`, `pluralize.ts`, `text-input-focus.ts`, `version.ts` (semver `major.minor.patch` ordering),
`inline-size-action.ts` (the container-query stand-in), `boot-guard-keys.ts` (the three catalog keys the `app.html`
boot guard shows). Per-file export catalogs: `DETAILS.md`.

## Must-knows

- **Filename validation stays frontend-pure.** Keystroke feedback needs sub-millisecond latency; an IPC hop per
  keystroke stutters. Don't move it to Rust.
- **Limits are `>= 255` bytes (name) and `>= 1024` bytes (path)**, strictly, measured with `TextEncoder`, ❌ never
  `.length` (multi-byte characters).
- **`validateConflict` is case-insensitive (APFS)**: pass `originalName` correctly, or a case-only rename false-flags.
- **`validateFilename` returns the FIRST error or warning**, never a list: inline rename UI has room for one message.
- **`getExtension` includes the dot** (`.txt`) and returns `''` for dotfiles (`.gitignore`).
- **Use `confirmDialog`, ❌ never `window.confirm()`** (unreliable in Tauri); it also labels Cancel so Escape works.
- **Two old-WebKit answers, don't confuse them.** `hasColorMix` is "degraded but working"; `meetsWebkitFloor` is "below
  Safari 15.4, can't work at all", and `isBelowSupportedMacOs(major)` is "older than the macOS we test on", which is the
  signal a best-effort warning should hang off. The real block for the floor case is the ES5 guard in `src/app.html`,
  because a screen inside the bundle can't render on a WebKit that can't parse the bundle.
- **Two `color-mix()` safety nets must both stay**: the `@supports not` fallbacks in `app.css`, and the runtime JS mixes
  (`mixSrgb` / `withAlpha`) in `accent-color.ts` / `volume-tint.svelte.ts`. Safari < 16.2 (still on macOS 12) can't
  parse `color-mix()`, so ❌ never use it for accent-derived tokens.
- **`readableFgOn` is mirrored in `scripts/check-a11y-contrast/accent_matrix.go`.** Keep both in sync, or the
  design-time contrast checker tests a different fg than the app renders.
- **A debounce does NOT bound concurrency: use `createCoalesced`** when a repeated async call can outlast its own delay.
  Stacked calls once took the backend's whole blocking pool (image-index badge fetch) and froze the app. `cancel()` on
  teardown.
- **`useShortenMiddle` reveals the full text through the HOUSE tooltip**, ❌ never a native `title`.
- **Sizing a component off its own width? `useInlineSize`, never `@container`.** Container queries need Safari 16 and
  the floor is Safari 15, where the block is dropped in silence. Stylelint rejects the CSS form; the action reports the
  same content box, so a size-query threshold ports over unchanged.
- **Two focus predicates, ❌ don't re-roll either**: `isTextInputFocused()` for keyboard events,
  `isTextInputTarget(target)` for mouse (a right-click can land on an unfocused field).

Export catalogs, validator chains, decisions, and the old-WebKit dev override: `DETAILS.md`. Read it before any
non-trivial work here: editing, planning, reorganizing, or advising.
