# Playwright E2E tests (tauri-playwright)

Playwright-based E2E tests for Cmdr, using tauri-playwright in Tauri mode. Commands are injected directly into the real
Tauri webview via Unix socket. No WebDriver, no platform-specific quirks.

## Architecture

```
Playwright (Node.js) ──Unix socket──> tauri-plugin-playwright (Rust)
                                            │
                                            └── webview.eval(js) ──> JS executes in webview
                                                                          │
                                                                          └── Tauri IPC (result callback)
```

Same tests run on macOS and Linux. Platform differences (Ctrl vs Meta) are handled by a single `CTRL_OR_META` constant
in `helpers.ts`.

## Running

```bash
cd apps/desktop

# Build the Tauri binary with the playwright plugin
pnpm test:e2e:playwright:build

# Run the tests (assumes the built binary is available)
pnpm test:e2e:playwright
```

## Files

| File                    | Purpose                                                          |
| ----------------------- | ---------------------------------------------------------------- |
| `playwright.config.ts`  | Playwright config: Tauri mode only, sequential execution         |
| `fixtures.ts`           | Test fixture using `createTauriTest` from tauri-playwright       |
| `global-setup.ts`       | Creates the shared fixture directory tree (~170 MB)              |
| `global-teardown.ts`    | Cleans up the fixture directory                                  |
| `helpers.ts`            | Ported helpers: `ensureAppReady`, `pollUntil`, DOM queries, etc. |
| `app.spec.ts`           | 14 tests: rendering, keyboard nav, mouse interaction, dialogs    |
| `file-operations.spec.ts` | 8 tests: copy, move, rename, mkdir, view modes, hidden, palette |
| `settings.spec.ts`      | 5 tests: settings page rendering, sidebar, search                |
| `viewer.spec.ts`        | 10 tests: file viewer, search, error handling                    |

## Key decisions

**Decision**: Use `tauriPage.evaluate()` with string expressions instead of function callbacks.
**Why**: TauriPage's `evaluate()` sends a JS string over the socket to be executed in the webview via `webview.eval()`.
Unlike Playwright's `page.evaluate()`, it doesn't support function serialization. All DOM queries must be written as
string expressions.

**Decision**: Use `pollUntil()` helper instead of Playwright's built-in `expect().toPass()`.
**Why**: In Tauri mode, Playwright's auto-waiting and locator assertions don't work because there's no real Playwright
`Page` object. The `pollUntil()` helper provides the same behavior (retry with timeout) for TauriPage.

**Decision**: Use `keyboard.press()` for all key inputs.
**Why**: The WebDriverIO tests needed platform-specific workarounds for Space, Backspace, and modifier keys due to
WebKitGTK WebDriver quirks. tauri-playwright dispatches `KeyboardEvent` directly in the webview via JS injection, so
all keys work uniformly on all platforms.

## Gotchas

**Gotcha**: Navigation destroys page context.
**Why**: After triggering SvelteKit navigation (settings, viewer), any in-flight `evaluate()` result will be lost.
Always `waitForSelector()` on the target page's element before evaluating further JS.

**Gotcha**: `test.beforeAll` runs per-worker, not globally.
**Why**: With `workers: 1`, this is effectively global. But if workers are increased, `beforeAll` runs per-worker.
Use `globalSetup`/`globalTeardown` for truly global setup (fixture creation/cleanup).

**Gotcha**: `playwright:default` capability not in `default.json`.
**Why**: The capability can only be validated when the plugin crate is compiled (feature-gated). Adding it to
`default.json` breaks builds without the `playwright-e2e` feature. The plugin handles its own permission grants when
initialized. If IPC calls are rejected at runtime, add the capability to a separate JSON file and include it
conditionally.
