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
  ├── absolute check              : error unless it starts with / or is the home shortcut (`~` or `~/…`);
  │                                 a bare `~foo` stays rejected (the backend expands `~`)
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

`validateDirectoryPath` is used by TransferDialog and composes with the individual validators in NewFolderDialog.

Extension-change behavior is controlled by the `allowExtensionChanges` user setting (`yes`/`no`/`ask`). `'ask'` returns
`ok` at validation time; the save dialog handles it separately. `extensionsDifferMeaningfully(oldName, newName)` gates
that confirmation so users aren't pestered over case-only changes (`.JPG` → `.jpg`) or known equivalents (`.jpeg` →
`.jpg`, `.md` → `.txt`); add an alias by extending `EQUIVALENT_EXTENSION_GROUPS` in the same file. It backs both
`validateExtensionChange` and the rename save flow's "ask" gate.

## confirm-dialog.ts

Thin wrapper around `@tauri-apps/plugin-dialog`'s `ask()`. `confirmDialog(message, title?): Promise<boolean>` shows a
native warning dialog with OK/Cancel and resolves `true` on confirm.

## timing.ts

`withTimeout(promise, ms, fallback)` races an IPC call and returns the fallback. `waitForNextPaint(timeoutMs)` resolves
`'painted' | 'timeout'`. `createDebounce(fn, delayMs)` exposes `flush()` (for `beforeunload` cleanup) and `cancel()`;
`createThrottle(fn, delayMs)` guarantees a trailing call. `createCoalesced(run)` bounds CONCURRENCY: at most one call in
flight, with the latest argument queued behind it. A debounce bounds only how often work STARTS, so slower calls stack
up. The two compose.

## pluralize.ts / text-input-focus.ts

`pluralize(count, singular, plural?)` formats a count with its noun ("1 user" / "3 users"). `text-input-focus.ts`
carries two predicates: `isTextInputFocused()` reads `document.activeElement` (keyboard events), and
`isTextInputTarget(target)` inspects an event target (mouse, where a right-click can land on an unfocused field).

## version.ts

`compareVersions(a, b)` orders two release strings by their numeric `major.minor.patch` core (negative / zero /
positive, `Array.sort`-shaped). A leading `v` and any pre-release or build suffix are ignored, so `v0.26.0-beta.1` and
`0.26.0` compare equal; we only ever order released versions.

Two callers, which is why it isn't private to either: `$lib/whats-new` asks whether this launch is an upgrade over the
version it last showed, and `$lib/updates` asks whether the build the manifest offers is newer than the one already
staged in the bundle. A comparator that disagreed with itself would let one surface call a release an upgrade while the
other called it a downgrade.

## shorten-middle.ts

`shortenMiddle()` truncates text in the middle with an ellipsis, using pixel-accurate width measurement via an injected
`measureWidth` function. Supports `preferBreakAt` (snap cuts to a delimiter like `/` or `.`), `startRatio` (bias budget
toward start or end), and custom ellipsis strings. `createPretextMeasure()` creates a `measureWidth` backed by
`@chenglou/pretext`'s `prepareWithSegments` + `measureNaturalWidth`, caching prepared texts for repeated measurements.

## shorten-middle-action.ts

`useShortenMiddle(node, params)` wraps `shortenMiddle` in a Svelte action: a ResizeObserver re-fits on width changes,
`@chenglou/pretext` loads async (CSS `text-overflow: ellipsis` covers the gap), and the full text is reachable on hover
through the HOUSE tooltip. Never a native `title`: its delay and chrome are the OS's, and this action feeds pane rows,
dialogs, and result lists alike, which would otherwise hover three different ways. `tooltipWhenTruncated?: boolean`
narrows the tooltip to strings truncation actually trimmed (default `false`: hover always shows the full text).

## inline-size-action.ts

`useInlineSize(node, { onResize })` is the house stand-in for a CSS container query: it reports an element's content-box
inline size at mount and on every resize, so Svelte can branch in markup where CSS would have used `@container`.

Why it exists: `@container` / `container-type` need Safari 16, and Cmdr's WebKit floor is Safari 15 (`build.target` in
`apps/desktop/vite.config.js`, guarded by the `desktop-vite-build-target` check). Old WebKit drops an unsupported
at-rule block whole and in silence, so the styling inside it never applies and nothing reports it. `ResizeObserver` is
Safari 13.1+, below every floor Cmdr targets. Stylelint's `at-rule-disallowed-list` / `property-disallowed-list` keep
the CSS form from coming back.

It reports `entry.contentRect.width`, the same content box a size query reads, so a threshold ported from
`@container (max-width: Npx)` keeps its meaning. (`entry.contentBoxSize` says the same thing, but only from Safari 15.4,
above the floor this action exists for.) The seed call at mount reads `clientWidth` minus the computed horizontal
padding, because the observer's own first callback lands after the next layout and one frame of unmeasured styling is
visible. A reported `0` means "not measured", not "narrower than everything", so callers gate on `> 0`; `TabBar.svelte`
is the reference caller.

## srgb-mix.ts / webkit-compat.ts

`webkit-compat.ts` exposes `hasColorMix` (computed once at module load) so consumers can branch, and `logWebkitCompat()`
which the main layout calls at boot, emitting one log line so affected users show up in error reports. `srgb-mix.ts`
also exports `relativeLuminance`, `contrastRatio`, and `readableFgOn`. `readableFgOn(accentHex)` returns `#000000` or
`#ffffff` by whichever has the higher WCAG contrast against the accent; used by `accent-color.ts` to compute
`--color-accent-fg` per runtime accent, and mirrored in `scripts/check-a11y-contrast/accent_matrix.go`.

### The two old-WebKit answers

`webkit-compat.ts` answers two different questions, and mixing them up is the easy mistake.

- `hasColorMix` is **degraded but working**: Safari below 16.2 can't parse `color-mix()`, so the accent and volume-tint
  paths mix in JS. Everything else about the app is fine.
- `meetsWebkitFloor` is **below the floor**: `crypto.randomUUID`, `Object.hasOwn`, `Array.prototype.findLast`, and
  `:has()` all arrived in Safari 15.4, the app calls them unconditionally, and esbuild's `build.target` lowers syntax
  and never runtime APIs. `missingWebkitCapabilities` names which ones are absent, and `WEBKIT_FLOOR_CAPABILITIES` is
  the list both this module and the `app.html` guard probe.

In a shipped build `meetsWebkitFloor` is effectively always `true`, because the guard in `src/app.html` replaces the
page before the bundle loads. It exists for the dev override and so app code can reason about the floor without
re-probing. `logWebkitCompat()` (called from the main layout at boot) emits `error` when it's false and the usual
`debug`/`info` line for `hasColorMix`, so an affected user shows up in error reports.

`isBelowSupportedMacOs(macosMajor)` is a third, unrelated answer: whether this Mac is older than `SUPPORTED_MACOS_MAJOR`
(12), the range Cmdr is developed and tested against. The bundle's `minimumSystemVersion` is 10.15, deliberately lower,
so 10 and 11 are best-effort and this predicate is where "best effort" is defined. It takes the number rather than
fetching it, which keeps the module free of IPC: callers pass what `commands.getMacosMajorVersion()` returned (Catalina
reports `10`, not `10.15`), and a version that didn't parse reads as supported. Floor rationale and the version
evidence: `docs/notes/system-requirements-and-es2025.md`.

**Dev override**, two levels, both read at module load so they must be set before `pnpm dev` starts:

- `VITE_CMDR_FORCE_OLD_WEBKIT=1` (or `=old`) forces the `color-mix()` fallback path on modern WebKit. It flips
  `hasColorMix` to `false` (routing the JS-mix branches) and sets `data-force-old-webkit` on `<html>` (activating the
  `:root[data-force-old-webkit]` blocks in `app.css` that mirror the `@supports not (...)` fallbacks).
- `VITE_CMDR_FORCE_OLD_WEBKIT=unsupported` does that AND forces `meetsWebkitFloor` to `false`.

Use them to verify the old-WebKit paths without a real Safari 15.x environment. See `docs/guides/releasing.md` §
"Pre-release smoke test on old macOS".

## Old-WebKit boot guard

The screen a below-floor Mac gets instead of a white window. It lives in `src/app.html`, not here, and that placement is
the whole design: Safari 13.x can't PARSE the bundle (measured, `docs/notes/system-requirements-and-es2025.md` § The two
floors old WebKit crosses), so anything shipped inside the bundle is unreachable on exactly the Mac that needs it. The
guard is an inline ES5 `<script>` in `<head>` that probes the four Safari 15.4 capabilities and, if any is missing,
empties `<body>` and paints a title, a sentence, and a Quit button.

Emptying the body is what makes it deterministic. SvelteKit's boot script captures `#app-root` while the body parses and
mounts into that captured element later; once the body is emptied, a bundle that DID parse mounts into a detached node
and never reaches the screen. So the guard wins whichever order the two run in.

**Its copy comes from the message catalog**, which is the other half of the design. The guard runs before any module, so
it can't call `t()`. `svelte.config.js` resolves the three `boot-guard-keys.ts` keys against every shipped catalog at
config-load time, formats the ICU away (the values carry `''` escaping the guard would otherwise show verbatim), and
splices the answers into a copy of the shell at `.svelte-kit/cmdr-app.html`, which is what `kit.files.appTemplate`
points at. Nothing is committed, so nothing can go stale; a missing marker fails the build rather than shipping a
stringless guard. Locale matching is precomputed too, because the guard can't run the script-boundary rule: the payload
carries a flat lowercase alias map so `zh-TW` lands on Traditional and never falls through to Simplified. Details:
`apps/desktop/scripts/gen-boot-guard-lib.ts`.

**Gotcha**: because `appTemplate` is the generated path, `pnpm dev` does NOT hot-reload an edit to `src/app.html`.
Restart the dev server after touching the shell.

**❗ ES5 only in the guard.** One arrow function or template literal and it dies silently on the WebKit it exists for.
`apps/desktop/scripts/app-boot-guard.test.ts` extracts the real script, sweeps it for the ES6 syntax you'd reach for,
and runs it against a stubbed environment with each capability removed in turn (globals arrive as function parameters,
so a test can take `Object.hasOwn` away from the guard without taking it away from vitest). That test is also what holds
the guard's probes and `WEBKIT_FLOOR_CAPABILITIES` together, since the guard can't import the list.

The Quit button calls `window.__TAURI_INTERNALS__.invoke('plugin:process|exit')`, the only raw invoke in the tree, and
it still routes through the quit gate: `src-tauri/capabilities/DETAILS.md` § The boot guard's exit.

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
