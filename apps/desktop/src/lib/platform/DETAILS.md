# Platform details

## How the value arrives

- `src-tauri/src/tauri_builder.rs` registers a tiny plugin (`cmdr-platform`) whose only job is a `js_init_script`:
  `Object.defineProperty(window, '__CMDR_PLATFORM__', { value: '<os>' })`. Tauri runs a plugin's init script in every
  webview the app creates, before the page's own scripts: the config's main window and each window the frontend opens
  later (viewer, Settings, the queue, the help and debug windows). So no window can render, or evaluate a module,
  without it.
- The property is read-only (`defineProperty` without `writable`), so nothing in the page can change its mind later.
- `getPlatform()` reads the global on every call and checks it names one of the three platforms. A missing or unknown
  value answers `FALLBACK_PLATFORM` (`'linux'`).
- `hooks.client.ts` calls `applyPlatformAttribute()` at module evaluation, which every window runs before its first
  route renders, so `:root[data-platform=…]` selectors hold from the first paint.

Decision: an init script, not an IPC command. Why: several modules read the platform AT MODULE LOAD (the command
registry's `showInPalette` and `shortcuts` arrays, the settings registry), and an awaited `invoke` can't answer before
they evaluate. The other option, a `tauri-plugin-os` call, is async for the same reason and adds a dependency.

Decision: one value per process, typed in TS, and Rust's own `std::env::consts::OS` on the backend. Why: Rust's names
for the three targets are exactly the `Platform` values, so nothing maps or mirrors them, and the crate root refuses
every other target (`docs/guides/platform-seams.md` § Unknown targets) instead of silently reporting "linux".

## The fallback

Contexts with no backend get `'linux'`:

- **Vitest**: the default test world is "not macOS", as it was when the check read happy-dom's Linux user agent. A test
  that needs another platform says so with `withPlatform()`.
- **A plain browser** (`vite dev` without Tauri): nothing answers IPC there anyway.
- **The browser demo** is the exception: its fake backend defines the global itself from the visitor's user agent
  (`apps/desktop/src/lib/demo/platform.ts`), so a Mac visitor gets ⌘ shortcuts and everyone else Ctrl, as the keyboard
  under their hands expects. A page has no other signal; the sniff lives in the demo's backend, never in app code.

Why not macOS as the fallback: off macOS is the safe direction. A Mac-only surface (Quick Look, Full Disk Access, the
Finder extension) hiding in an odd context is harmless; one showing where nothing can answer it isn't.

## `withPlatform()`

It sets the same global the init script does and returns the undo, so everything that reads the platform follows at
once, `key-capture.ts`'s formatting included. Mocking `isMacOS` instead flipped only the importers the mock reached and
left the rest on the host's answer, a mixed world no real machine has.

- Bare: the switch holds until the next one. Each test file starts on the fallback (Vitest isolates files).
- Per test: `beforeEach(() => withPlatform('macos'))` (Vitest runs the returned undo as the teardown).
- Inside one test: `onTestFinished(withPlatform('windows'))`.
- Before the imports, for module-load reads: `vi.hoisted(() => vi.stubGlobal('__CMDR_PLATFORM__', 'macos'))`. Hoisted
  code runs before any import, so it can't call the helper and writes the same global itself. Decision: not
  `await vi.hoisted(async () => …import…)`, which reads better but breaks in a `.svelte.test.ts` (the Svelte transform
  nests the top-level `await`, and Vitest refuses a nested `vi.hoisted`; verified on Vitest 5.1.0, 2026-11-11).

## Window chrome

`--titlebar-height` is the height of the macOS overlay title bar: the main window's title bar sizes itself to it, and
every full-window backdrop starts below it so the window-drag region stays live under a dialog. Off macOS the OS draws
the title bar outside the webview, so `app.css` sets it to 0 under `:root:not([data-platform='macos'])` and a backdrop
covers the whole window. The viewer's 35px override and the queue window's in-webview title line are macOS-only for the
same reason.
