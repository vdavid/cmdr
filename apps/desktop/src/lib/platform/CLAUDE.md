# Platform (`lib/platform/`)

Which OS this window runs on (`'macos' | 'linux' | 'windows'`), as the backend says, read synchronously.

- `index.ts`: `getPlatform()`, `isMacOS()`, `applyPlatformAttribute()` (`data-platform` on `<html>`).
- `test-platform.ts`: `withPlatform()`, the test helper.

## Must-knows

- **The value is injected, never sniffed.** The backend's init script defines `window.__CMDR_PLATFORM__` in every
  webview before any script runs (`src-tauri/src/tauri_builder.rs`), so module-load reads are safe. ❌ Never read
  `navigator.userAgent` or `navigator.platform` to decide the OS.
- **No backend, no injection: the answer is `'linux'`** (Vitest, a plain browser), so no Mac-only surface shows by
  accident. The browser demo answers for the visitor's computer (`apps/desktop/src/lib/demo/platform.ts`).
- **Tests switch platforms with `withPlatform()`**, ❌ never by mocking `isMacOS` or stubbing the user agent: it moves
  everything that reads the platform at once, key formatting included. A module-load read needs it in `vi.hoisted`
  before the imports; the forms are in `test-platform.ts`.
- **CSS follows `data-platform`**, set by `hooks.client.ts` before the first route renders. `--titlebar-height` is 0 off
  macOS (`app.css` § Window chrome off macOS).

Mechanics, the fallback, and decisions: `DETAILS.md`.
