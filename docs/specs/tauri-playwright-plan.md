# Replace WebDriverIO + CrabNebula with tauri-playwright

## Goal

Replace the current dual E2E setup (WebDriverIO + tauri-driver on Linux, WebDriverIO + CrabNebula on macOS) with a
single Playwright-based test suite using a forked and improved `tauri-playwright` plugin. This eliminates the CrabNebula
dependency, removes platform-specific WebDriver quirks, and enables running the same tests on both macOS and Linux.

## Why

- **CrabNebula is a commercial dependency** with API key requirement, limited control, and known WebDriver quirks (keys
  don't work, element refs don't serialize). It's a black box.
- **WebDriverIO on WebKitGTK has painful quirks**: native clicks fail, Space key doesn't work, Backspace needs
  dispatchEvent, navigation needs `<a>` element hacks. Tests are full of platform workarounds.
- **Two separate test suites** (e2e-linux, e2e-macos) with different configs, different quirks, and diverging helper
  code. Maintenance burden grows with every new test.
- **tauri-playwright's architecture can be made elegant**: the existing polling approach is a workaround for not using
  Tauri's native JS injection APIs. Once fixed, the plugin becomes a thin, fast bridge between Playwright tests and the
  real Tauri webview.

## Architecture: current vs target

### Current (polling-based, two suites)

```
Linux:   WebDriverIO ──HTTP:4444──▶ tauri-driver ──▶ WebKitWebDriver ──▶ WebKitGTK
macOS:   WebDriverIO ──HTTP:4444──▶ CN tauri-driver ──HTTP:3000──▶ test-runner-backend ──▶ tauri-plugin-automation
```

### Target (direct injection, one suite)

```
Playwright ──Unix socket──▶ tauri-plugin-playwright (Rust)
                                  │
                                  ├── webview.eval(js) ────▶ JS executes in webview
                                  │                              │
                                  └── Tauri IPC ◀────────────────┘
                                      (plugin:playwright|result)
```

**Key insight**: Tauri's `WebviewWindow::eval()` injects JS directly into the webview (no polling needed).
`__TAURI_INTERNALS__.invoke()` is always available in the webview (regardless of `withGlobalTauri`) and provides the
return path. No HTTP server needed at all.

## Critical decisions

### Why Tauri IPC for results instead of HTTP callbacks or Tauri events

The current plugin uses an HTTP server on port 6275 for the JS→Rust result path. We considered three alternatives:

1. **Tauri events (`window.__TAURI__.event.emit`)**: Requires `withGlobalTauri: true`. Cmdr's release/E2E builds have
   `withGlobalTauri: false` in `tauri.conf.json`. Forcing it on for E2E would be a config divergence risk.
2. **HTTP callback server**: Works but adds unnecessary infrastructure (a TCP listener, CORS headers, port allocation).
3. **Tauri IPC invoke (`__TAURI_INTERNALS__.invoke`)**: Always available in any Tauri webview (it's how the framework's
   own IPC works). No extra servers, no config requirements. The plugin registers an invoke handler and the injected JS
   calls it.

We chose option 3. The plugin registers a `result` command via `invoke_handler`, and each injected script wraps user
code in a try-catch that calls `invoke('plugin:playwright|result', { id, ok, data })`.

### Why keep Unix socket for test runner communication

The test runner (Node.js Playwright process) needs to send commands to the Rust plugin. Unix sockets are fast,
localhost-only, and already work well in the existing code. TCP fallback stays for Windows/cross-machine scenarios.

### Why a single unified test suite replaces both e2e-linux and e2e-macos

tauri-playwright's Tauri mode works identically on all platforms (same JS injection, same IPC). Platform-specific
WebDriver quirks disappear entirely because we bypass WebDriver. The few remaining platform differences (keyboard
modifier keys: Ctrl on Linux vs Meta on macOS) are trivial to handle with a helper function.

### What about `withGlobalTauri` for E2E builds?

Not needed. The plugin uses `__TAURI_INTERNALS__` which is always present. However, the plugin needs its permission
declared in the app's capabilities — we'll add a `playwright:default` capability, similar to how `mcp-bridge:default` is
already configured for dev builds.

---

## Milestones

### Milestone 1: Improve the plugin (fork at /tmp/tauri-playwright)

**Intention**: Make the plugin architecturally sound — remove polling, use direct webview injection, eliminate the HTTP
server. This is the foundational work that makes everything else possible.

#### 1.1 Replace the command execution path

**What**: Thread `AppHandle` into `execute_command`, use `webview.eval()` for JS injection, register an IPC `result`
command handler for the return path.

**Why**: The current polling loop adds ~16ms latency per command and introduces flakiness. Direct injection is
instantaneous.

**Changes in `packages/plugin/src/`**:

- `lib.rs`:
    - Remove the polling JS init script (the `js_init_script()` function that injects the fetch-poll loop).
    - Replace with a minimal init script: just `window.__PW_ACTIVE__ = true` as a readiness flag for the test fixture
      (the fixture's `waitForFunction('window.__PW_ACTIVE__')` checks this before running tests).
    - Add `.invoke_handler(tauri::generate_handler![pw_result])` to the plugin builder chain.
    - Add `.manage(pending_for_setup)` to the plugin builder chain — **required**, otherwise accessing
      `tauri::State<'_, PendingResults>` in the command handler will panic at runtime.
    - Pass `AppHandle` into the socket server's `start()` function (it's already received but stored as `_app`).
- `server.rs`:
    - Delete `run_http_server()`, `CommandQueue`, `QueuedCommand`, `CALLBACK_PORT` — all polling infrastructure.
    - Update `handle_connection` to pass `app` through to `execute_command` (currently receives `_app` but doesn't use
      it).
    - Update `execute_command` to accept `app: &AppHandle<R>` and pass it to `eval_js`.
    - Rewrite `eval_js()`:
        - Get the webview: `app.get_webview_window(&window_label).ok_or("window not found")?`
        - Call `webview.eval(&wrapped_script)` (fire-and-forget injection)
        - Wait on the oneshot receiver for the result (same timeout as before)
    - Remove the `queue` parameter from all functions.
- `commands.rs`:
    - Add a Tauri command for receiving results from the webview:
        ```rust
        #[tauri::command]
        async fn pw_result(
            pending: tauri::State<'_, PendingResults>,
            id: String,
            ok: bool,
            data: Option<String>,
            error: Option<String>,
        ) -> Result<(), String> { ... }
        ```
        This looks up `id` in the pending map and sends the result through the oneshot channel. **Important**: Don't
        apply `#[serde(rename_all = "camelCase")]` — the field names must match the JS object keys exactly (`id`, `ok`,
        `data`, `error`).

**The wrapped JS pattern** (generated by `eval_js`):

```js
;(async () => {
    try {
        const __pw_result = await USER_SCRIPT
        window.__TAURI_INTERNALS__.invoke('plugin:playwright|pw_result', {
            id: 'pw42',
            ok: true,
            data: JSON.stringify(__pw_result),
        })
    } catch (e) {
        window.__TAURI_INTERNALS__.invoke('plugin:playwright|pw_result', {
            id: 'pw42',
            ok: false,
            error: String((e && e.message) || e),
        })
    }
})()
```

#### 1.2 Add plugin permissions

**What**: Create `permissions/` directory with default permission set, so Tauri apps can grant the plugin's IPC
commands.

**Why**: Tauri 2 requires explicit permission grants for plugin commands. Without this, the `invoke()` call from the
webview will be rejected.

**Files**:

- `packages/plugin/permissions/default.toml` — grants `allow-pw-result` (the IPC result callback)
- `packages/plugin/build.rs` — calls `tauri_plugin::Builder::new("playwright").build()` to generate permission schemas

#### 1.3 Make window label configurable

**What**: Add `window_label: Option<String>` to `PluginConfig`, defaulting to `"main"`.

**Why**: Not all apps use "main" as their window label. Cmdr does, so this isn't blocking, but it's trivial and makes
the plugin more reusable.

#### 1.4 Keep native_capture.rs and recording intact

**What**: Don't touch `native_capture.rs` or the `RecordingState`/`RecordingSession` types. These support screenshot and
video recording features that other users of the plugin may need.

**Why**: Cmdr doesn't need video recording in E2E, but breaking it would make the upstream PR harder to merge. The
native screenshot commands (CoreGraphics on macOS) are also useful — they capture the real window including title bar.

#### 1.5 Clean up and test

**What**: Remove dead code, update the example app, verify the plugin compiles and the example's tests pass.

**Why**: Validate the architecture change works end-to-end before integrating into Cmdr.

**Checks**:

- `cargo check` on the plugin crate
- `cargo build --features e2e-testing` on the example app
- Run the example's Playwright tests in Tauri mode

---

### Milestone 2: Integrate plugin into Cmdr

**Intention**: Wire up the improved plugin in Cmdr's Rust backend, create a new Playwright-based test suite, and port
all existing E2E tests.

#### 2.1 Add plugin to Cmdr's Cargo.toml

**What**: Add `tauri-plugin-playwright` as a git dependency (pointing to the fork) behind a `playwright-e2e` feature
flag. Register it in `lib.rs` with `#[cfg(feature = "playwright-e2e")]`.

**Why**: Feature-gated so it never ships in production. Same pattern as the existing `automation` feature for
CrabNebula.

**Changes**:

- `apps/desktop/src-tauri/Cargo.toml`:
    - Add feature: `playwright-e2e = ["dep:tauri-plugin-playwright"]`
    - Add dependency:
      `tauri-plugin-playwright = { git = "https://github.com/vdavid/tauri-playwright", optional = true }`
- `apps/desktop/src-tauri/src/lib.rs`:
    ```rust
    #[cfg(feature = "playwright-e2e")]
    let builder = builder.plugin(tauri_plugin_playwright::init());
    ```
- `apps/desktop/src-tauri/capabilities/default.json`: Add `"playwright:default"` to the permission list

**Check**: `./scripts/check.sh --check clippy` to verify compilation.

#### 2.2 Create the new Playwright test suite

**What**: New directory `apps/desktop/test/e2e-playwright/` with Playwright config, fixtures, helpers, and test specs.

**Why**: Fresh start with Playwright API, no WebDriver quirks to work around.

**Structure**:

```
apps/desktop/test/e2e-playwright/
  playwright.config.ts     ← Playwright config with Tauri mode
  fixtures.ts              ← Test fixture (wraps shared fixture system)
  helpers.ts               ← Ported helpers using Playwright API
  app.spec.ts              ← 14 tests from e2e-linux/app.spec.ts
  file-operations.spec.ts  ← 8 tests from e2e-linux/file-operations.spec.ts
  settings.spec.ts         ← 5 tests from e2e-linux/settings.spec.ts
  viewer.spec.ts           ← 10 tests from e2e-linux/viewer.spec.ts
  tsconfig.json
  CLAUDE.md                ← Architecture doc for the new suite
```

**Key changes in the port**:

| WebDriverIO pattern             | Playwright equivalent                                                                 |
| ------------------------------- | ------------------------------------------------------------------------------------- |
| `browser.$('.sel')`             | `tauriPage.locator('.sel')`                                                           |
| `browser.$$('.sel')`            | `tauriPage.locator('.sel').all()`                                                     |
| `element.waitForExist()`        | `tauriPage.waitForSelector('.sel')` or `locator.waitFor()`                            |
| `element.getAttribute('class')` | `tauriPage.getAttribute('.sel', 'class')`                                             |
| `browser.keys('ArrowDown')`     | `tauriPage.keyboard.press('ArrowDown')` (TauriKeyboard class on `tauriPage.keyboard`) |
| `browser.execute(() => ...)`    | `tauriPage.evaluate('...')`                                                           |
| `jsClick(element)`              | `tauriPage.click('.sel')` (no workaround needed)                                      |
| `browser.pause(100)`            | Playwright auto-waiting handles most cases                                            |

**Platform-specific keyboard helper**:

```typescript
const CTRL_OR_META = process.platform === 'darwin' ? 'Meta' : 'Control'
```

This single constant replaces the Linux-specific `ctrlKey: true` and macOS `dispatchKey()` workarounds.

#### 2.3 Install npm dependencies

**What**: Add `@srsholmes/tauri-playwright` and `@playwright/test` to devDependencies. No Chromium install needed —
Tauri mode runs against the real app webview, not a browser.

**Changes in `apps/desktop/package.json`**:

- Add: `@srsholmes/tauri-playwright`, `@playwright/test`
- Add scripts: `test:e2e:playwright`, `test:e2e:playwright:build`
- Keep existing WDIO scripts during transition (remove in milestone 3)

#### 2.4 Port each spec file

Port tests one at a time, verifying each runs before moving to the next. Order matters — start with the simplest:

1. **`app.spec.ts`** (basic rendering, keyboard nav, mouse clicks) — validates the fundamental connection and
   interaction model works
2. **`viewer.spec.ts`** (file viewer) — tests SvelteKit navigation, which was the most hacky part in WebDriverIO
3. **`settings.spec.ts`** (settings page) — another navigation test, plus form interactions
4. **`file-operations.spec.ts`** (copy, move, rename, mkdir) — the most complex, touches filesystem and dialogs

**For each spec**: write the Playwright version, build Cmdr with `--features playwright-e2e`, run the tests, fix any
issues.

#### 2.5 Update build scripts and Docker

**What**: Add npm scripts for the new test suite. Update the Docker Dockerfile to include Playwright dependencies if
needed for CI.

**Build command**: `pnpm tauri build --no-bundle -- --features playwright-e2e`

**Docker considerations**: The current Docker image has WebKitGTK + Xvfb + tauri-driver. For Playwright in Tauri mode,
we still need Xvfb (real GUI required) and the Tauri runtime deps, but NOT tauri-driver or WebKitWebDriver. Playwright
doesn't use WebDriver at all.

**Check**: Build in Docker, run tests.

---

### Milestone 3: Validate, clean up, document

**Intention**: Ensure everything works reliably on both platforms, remove the old test infrastructure, and update docs.

#### 3.1 Run full test suite on macOS

**What**: Build Cmdr with `playwright-e2e` feature, run all Playwright tests locally on macOS.

**Why**: This is the main value — macOS E2E without CrabNebula.

#### 3.2 Run full test suite in Docker (Linux)

**What**: Update Dockerfile, run tests in Docker with Xvfb.

**Why**: Validate CI compatibility. This replaces the current `desktop-e2e-linux` check.

#### 3.3 Remove old test infrastructure

**What**: Once the new suite passes on both platforms:

- Delete `apps/desktop/test/e2e-linux/` (old WebDriverIO Linux suite)
- Delete `apps/desktop/test/e2e-macos/` (old CrabNebula macOS suite)
- Remove npm deps: `@wdio/*`, `webdriverio`, `@crabnebula/tauri-driver`, `@crabnebula/test-runner-backend`
- Remove Cargo feature: `automation` and `tauri-plugin-automation` dependency
- Remove `.env.example` (no more CN_API_KEY)
- Update CI workflow (`.github/workflows/ci.yml`) to use new test command

#### 3.4 Update documentation

- Update `apps/desktop/test/CLAUDE.md` — new single-suite architecture
- Create `apps/desktop/test/e2e-playwright/CLAUDE.md` — running instructions, architecture, gotchas
- Update `AGENTS.md` — E2E section
- Update `docs/architecture.md` — E2E testing reference

#### 3.5 Prepare upstream PR

**What**: Clean up the fork's commit history, write a PR description for the original `srsholmes/tauri-playwright` repo.

**Scope of upstream PR**: Only the plugin architecture changes (milestone 1). Cmdr-specific integration stays in our
repo.

---

## Gotchas

### Navigation destroys page context

The old polling approach survived SvelteKit navigations because the init script re-injected on every page load (via
`MutationObserver`). With `webview.eval()`, each call is a one-shot injection into the current page. If a test triggers
a navigation (for example, going to `/settings` or `/viewer`), any in-flight `eval_js` result will never arrive because
the page context is destroyed.

**Practical impact**: After triggering navigation, tests must `waitForSelector` or `waitForFunction` before evaluating
further JS. This is standard Playwright practice and the old tests already do this (they wait for `.settings-window` or
`.viewer-container` after navigating). But it's worth calling out because the polling model was more forgiving — it
would just retry on the new page.

The `__PW_ACTIVE__` flag in the init script is also relevant here: it re-initializes on each page load (since init
scripts run on every navigation), so the fixture can detect when the new page is ready.

### Vite dev server proxy not needed

The old plugin's HTTP endpoints (`/pw-poll`, `/pw`) used relative URLs that went through Vite's dev server proxy. Since
we're removing the HTTP server entirely, there's no proxy path to worry about. The Node.js test runner communicates
directly via Unix socket, and the JS in the webview communicates via Tauri IPC. Verify that no code in
`@srsholmes/tauri-playwright`'s npm package (client-side) still tries to hit HTTP endpoints.

---

## Risks and mitigations

| Risk                                                                                                         | Impact                                | Mitigation                                                                                                              |
| ------------------------------------------------------------------------------------------------------------ | ------------------------------------- | ----------------------------------------------------------------------------------------------------------------------- |
| `__TAURI_INTERNALS__.invoke()` might not be available in release builds for plugin commands                  | Blocks the whole approach             | Test early with a minimal Tauri app. Fallback: use a tiny HTTP server for results only (still much better than polling) |
| Playwright assertions (`expect(locator).toBeVisible()`) use polling internally — may be slower than expected | Flaky tests                           | tauri-playwright's `expect.ts` already has configurable polling intervals and timeouts                                  |
| Linux Docker container needs Tauri runtime deps but not WebDriver                                            | Build/CI issues                       | The current Dockerfile already has all Tauri deps. We just stop installing tauri-driver                                 |
| Some existing tests rely on WebDriver-specific behavior (element serialization, session reuse)               | Port difficulty                       | The port is a rewrite, not a translation. We'll use Playwright idioms, not WebDriver workarounds                        |
| SvelteKit navigation destroys page context mid-eval                                                          | Timeout errors in tests that navigate | Always `waitForSelector` after navigation before doing more evaluations (see Gotchas above)                             |

## What's NOT in scope

- Video recording support
- CDP mode (Windows-only, we don't need it)
- Browser mode (IPC mocking — useful but separate concern)
- Updating the tauri-playwright npm package on npm (that's the upstream author's job)
- Network mocking, dialog handling (Cmdr doesn't need these for E2E)
