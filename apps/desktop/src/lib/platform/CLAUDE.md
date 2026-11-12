# Platform (`lib/platform/`)

Which OS this window runs on (`'macos' | 'linux' | 'windows'`), and what that OS lets Cmdr offer, as the backend says,
read synchronously.

- `index.ts`: `getPlatform()`, `isMacOS()`, `hasCapability()`, `platformCapabilities()`, `applyPlatformAttributes()`.
- `test-platform.ts`: `withPlatform()`, `withCapabilities()`, `capabilitiesWith()`, `withCapabilitiesPossibleOn()`, the
  test helpers. A per-OS test (what a user on that OS gets) uses `withCapabilitiesPossibleOn(os)`, ❌ never
  `withEveryCapability()`; `capabilities-possible-on.test.ts` pins it to the Rust arms.

## Must-knows

- **To show or hide a feature, ask `hasCapability('quickLook')`, ❌ never `isMacOS()`.** The set is
  `PlatformCapabilities`, generated from Rust (`apps/desktop/src-tauri/src/platform/capabilities.rs`, one answer per OS
  arm). New OS-backed surface: add a field there, answer it in every arm, regen bindings. `isMacOS()` stays for
  genuinely per-platform answers: key glyphs, the OS version. ❌ Never to pick copy: per-OS wording is one catalog block
  (`../intl/DETAILS.md` § Platform variants).
- **Both values are injected, never sniffed.** The backend's init script defines `window.__CMDR_PLATFORM__` and
  `window.__CMDR_CAPABILITIES__` in every webview before any script runs, so module-load reads are safe. ❌ Never read
  `navigator.userAgent` or `navigator.platform` to decide the OS.
- **No backend, no injection: `'linux'` and no capabilities** (Vitest, a plain browser), so no OS-backed surface shows
  by accident. The browser demo answers for the visitor's computer (`apps/desktop/src/lib/demo/platform.ts`).
- **The two move independently in tests.** `withPlatform()` moves the platform (key formatting follows),
  `withCapabilities('fullDiskAccess')` what the OS offers; ❌ never mock `isMacOS` / `hasCapability` or stub the user
  agent. Module-load reads need `vi.hoisted`; the forms are in `test-platform.ts`.
- **CSS follows `data-overlay-title-bar`** (and `data-platform`), set by `hooks.client.ts` before the first route
  renders. `--titlebar-height` is 0 without an overlay title bar (`app.css` § Window chrome).

Mechanics, the fallback, and decisions: `DETAILS.md`.
