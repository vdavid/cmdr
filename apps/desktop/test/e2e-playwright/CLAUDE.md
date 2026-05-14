# Playwright E2E tests (tauri-playwright)

Playwright-based E2E tests for Cmdr, using tauri-playwright in Tauri mode. Commands are injected directly into the real
Tauri webview via Unix socket. No WebDriver, no platform-specific quirks.

## Architecture

```
Playwright (Node.js) --Unix socket--> tauri-plugin-playwright (Rust)
                                            |
                                            +-- webview.eval(js) --> JS executes in webview
                                                                          |
                                                                          +-- Tauri IPC (result callback)
```

Same tests run on macOS and Linux. Platform differences (Ctrl vs Meta) are handled by a single `CTRL_OR_META` constant
in `helpers.ts`.

## Running on macOS

**Via the checker (recommended):** The checker handles the full lifecycle automatically — build, fixture creation, app
startup, test execution, and cleanup:

```bash
./scripts/check.sh --check desktop-e2e-playwright
```

The checker runs the suite as **N parallel shards**: one dedicated MTP lane (sequential, `mtp.spec.ts` +
`mtp-conflicts.spec.ts`) plus 2 non-MTP lanes split by Playwright's `--shard X/2`. Each shard gets its own Tauri
instance with a distinct `CMDR_DATA_DIR`, MCP port (9429 + offset), and Unix socket path. The MTP shard runs alone
because the virtual MTP backing dir (`/tmp/cmdr-mtp-e2e-fixtures`) is shared by every Tauri instance — running MTP specs
from two shards at once would corrupt it. Per-shard logs go to `/tmp/cmdr-e2e-playwright-<shard>-<timestamp>.log`.

The socket path is overridable via the `CMDR_PLAYWRIGHT_SOCKET` env var (read in `src-tauri/src/lib.rs` and passed to
`tauri_plugin_playwright::init_with_config`). When unset, the plugin falls back to `/tmp/tauri-playwright.sock` so
manual / Linux-Docker runs keep working unchanged.

`RUST_LOG` is forwarded to the app, so trace-level output is one shell-prefix away. The chosen value is echoed at the
top of the log so it's visible at a glance:

```bash
RUST_LOG=cmdr_lib::file_system::volume::mtp=trace ./scripts/check.sh --check desktop-e2e-playwright
```

**Manually (for debugging):**

```bash
cd apps/desktop

# Build the Tauri binary with the playwright plugin
pnpm test:e2e:playwright:build

# Start the app (in a separate terminal)
CMDR_E2E_START_PATH=/tmp/cmdr-e2e-fixtures /path/to/target/.../release/Cmdr

# Run the tests (app must be running with socket at /tmp/tauri-playwright.sock)
CMDR_E2E_START_PATH=/tmp/cmdr-e2e-fixtures pnpm test:e2e:playwright
```

When running manually, the test suite does NOT launch the app. The app must be started with `CMDR_E2E_START_PATH`
pointing to a fixture directory created by `e2e-shared/fixtures.ts`.

## Running a single spec

When iterating on one spec, **run only that spec**. The full suite takes ~10 minutes and produces noisy cascade failures
when the broken test takes the app down with it (subsequent specs fail with connection errors). Save the full run for
the final CI-green check.

With the app already running (see "Manually" above), filter by file or by name:

```bash
cd apps/desktop

# By file path (relative to apps/desktop/)
CMDR_E2E_START_PATH=/tmp/cmdr-e2e-fixtures pnpm test:e2e:playwright test/e2e-playwright/brief-cursor-visibility.spec.ts

# By test-name substring (matches `test('...')` titles)
CMDR_E2E_START_PATH=/tmp/cmdr-e2e-fixtures pnpm test:e2e:playwright --grep "cursor stays in view"
```

The checker invocation (`./scripts/check.sh --check desktop-e2e-playwright`) doesn't support filtering — it always runs
the whole suite. So during iteration, prefer the manual flow.

## Running on Linux (Docker)

```bash
cd apps/desktop

pnpm test:e2e:linux          # Build binary + run tests in Docker (Ubuntu 24.04 + Xvfb)
pnpm test:e2e:linux:shell    # Interactive shell for debugging
pnpm test:e2e:linux:vnc      # VNC mode with hot reload
```

The Docker setup (`../e2e-linux/docker/`) builds the Tauri binary with `--features playwright-e2e,virtual-mtp`, launches
it inside the container, waits for the socket, and runs these same test files. See `../e2e-linux/CLAUDE.md` for Docker
details.

## Files

| File                          | Purpose                                                                                                                                                                                                                                                     |
| ----------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `playwright.config.ts`        | Playwright config: Tauri mode only, sequential execution                                                                                                                                                                                                    |
| `fixtures.ts`                 | Test fixture using `createTauriTest` from tauri-playwright                                                                                                                                                                                                  |
| `global-setup.ts`             | Creates or refreshes the fixture directory tree (~170 MB)                                                                                                                                                                                                   |
| `global-teardown.ts`          | Cleans up the fixture directory (if created by globalSetup)                                                                                                                                                                                                 |
| `helpers.ts`                  | Ported helpers: `ensureAppReady`, `pollUntil`, DOM queries, etc.                                                                                                                                                                                            |
| `conflict-helpers.ts`         | Shared fixtures and UI helpers for conflict resolution tests                                                                                                                                                                                                |
| `app.spec.ts`                 | 14 tests: rendering, keyboard nav, mouse interaction, dialogs                                                                                                                                                                                               |
| `file-operations.spec.ts`     | 8 tests: copy, move, rename, mkdir, view modes, hidden, palette                                                                                                                                                                                             |
| `conflict-copy.spec.ts`       | 7 tests: copy conflict policies, per-file decisions, rename                                                                                                                                                                                                 |
| `conflict-move.spec.ts`       | 3 tests: move merge, skip, rollback                                                                                                                                                                                                                         |
| `conflict-edge-cases.spec.ts` | 7 tests: rollback, sequential conflicts, symlinks, type mismatch                                                                                                                                                                                            |
| `file-watching.spec.ts`       | 11 tests: external CRUD, batch/threshold, cross-pane, dedup, hidden                                                                                                                                                                                         |
| `indexing.spec.ts`            | 3 tests: directory sizes from index, size updates on create/delete                                                                                                                                                                                          |
| `settings.spec.ts`            | 5 tests: settings page rendering, sidebar, search                                                                                                                                                                                                           |
| `network-toggle.spec.ts`      | 4 tests: volume picker shows "Network" / "Network (disabled)" depending on the `network.enabled` setting; toggling silences mDNS without losing the visible entry                                                                                           |
| `viewer.spec.ts`              | 10 tests: file viewer, search, error handling                                                                                                                                                                                                               |
| `mtp.spec.ts`                 | MTP E2E tests: volume selection, navigation, file ops, large file transfer via virtual device. Uses `e2e-shared/mcp-client.ts` (MCP client helper) and `e2e-shared/mtp-fixtures.ts` (MTP fixtures). Requires `virtual-mtp` feature.                         |
| `mtp-conflicts.spec.ts`       | MTP conflict resolution: cross-volume move (MTP↔local) and same-volume move (MTP→MTP) with overwrite/skip policies. Requires `virtual-mtp` feature.                                                                                                         |
| `smb.spec.ts`                 | SMB E2E tests: virtual host discovery (14 hosts), share listing, mounting, cross-storage copy, 50-share enumeration, unicode share rendering. Uses `e2e-shared/smb-fixtures.ts` and smb2's consumer Docker containers. Requires `smb-e2e` feature + Docker. |

## Multi-window testing

The viewer (label `viewer-<timestamp>`) and settings (label `settings`) UIs run in their own Tauri `WebviewWindow` in
production. Tests that exercise them must do so through the production multi-window flow, not by routing the main window
to `/viewer` or `/settings`. The latter exercises the page component but skips label uniqueness, restricted
capabilities, and the cross-window focus/close lifecycle.

The plugin (`tauri-plugin-playwright` 0.3.0+) supports scoping a `TauriPage` to any open window:

- `tauriPage.listWindows()` — returns `WindowInfo[]` (`{ label, url, title, visible }`).
- `tauriPage.window(label)` — fork a new TauriPage scoped to the given label. Shares the socket; cheap.
- `tauriPage.waitForWindow(predicate, { timeout? })` — poll `listWindows()` every 100 ms, return a scoped page once a
  window matches. Default 5 s timeout.

**Canonical pattern** (also see `helpers.ts` for the three helpers that wrap the boilerplate):

```ts
import { openViewerWindow, openSettingsWindowViaProd, closeScopedWindow } from './helpers.js'

// Viewer: open via the same `open-file-viewer` event prod / MCP use.
const viewer = await openViewerWindow(tauriPage as TauriPage, filePath)
await viewer.waitForSelector('.viewer-container', 15000)
// ... interact only via `viewer`, never `tauriPage`, for window-scoped DOM ...
await closeScopedWindow(tauriPage as TauriPage, viewer, viewer.targetWindow!)

// Settings: stable label.
const settings = await openSettingsWindowViaProd(tauriPage as TauriPage)
await settings.waitForSelector('.settings-window', 15000)
// ... interact via `settings` ...
await closeScopedWindow(tauriPage as TauriPage, settings, 'settings')
```

**Capabilities**: the viewer and settings windows have RESTRICTED capability files
(`src-tauri/capabilities/viewer.json`, `settings.json`). When a test fails because the scoped page can't call a Tauri
command, that's a real bug — production hits the same wall. Fix by either adding the missing permission to the
capability file or changing the test to use a permitted command.

The auto-generated `playwright.json` capability (`src-tauri/build.rs`) now includes `"main"`, `"settings"`, and
`"viewer-*"` so the plugin's `pw_result` IPC callback works from all three.

## Key decisions

**Decision**: `accessibility.spec.ts` disables axe's `color-contrast` rule. **Why**: Contrast is checked at design time
by `scripts/check-a11y-contrast` (deterministic, ~300 ms). Axe's `color-contrast` read `getComputedStyle().color` and
different browser engines disagreed on how to resolve nested `color-mix(var(...))` chains on translucent overlays,
producing environment-dependent ratios. Axe stays on for structural rules — ARIA, focus order, labels, keyboard nav —
where a running browser is genuinely needed. See `docs/design-system.md` § Automated contrast checks.

**Note on tier 3 overlap:** Most of the structural audits here (ARIA, labels, roles, accessible names) now also run at
the component level in tier 3 — see `apps/desktop/src/**/*.a11y.test.ts` and the helper at `src/lib/test-a11y.ts`. Tier
3 is fast (milliseconds per component) and catches regressions during dev; this E2E tier still earns its keep for
cross-component flows jsdom can't model (focus traps, Escape return-focus, keyboard nav integration). Once tier 3
coverage is broad, we can consider slimming this suite to those flow-level scenarios. Until then, the overlap is
intentional — tier 3 is proving itself.

**Decision**: Use `tauriPage.evaluate()` with string expressions instead of function callbacks. **Why**: TauriPage's
`evaluate()` sends a JS string over the socket to be executed in the webview via `webview.eval()`. Unlike Playwright's
`page.evaluate()`, it doesn't support function serialization. All DOM queries must be written as string expressions.

**Decision**: Use `pollUntil()` for complex conditions, `tauriPage.waitForFunction()` for simple JS expressions.
**Why**: In Tauri mode, Playwright's auto-waiting and locator assertions don't work because there's no real Playwright
`Page` object. `tauriPage.waitForFunction()` works now that the plugin embeds expressions directly instead of using
`eval()` (fixed in plugin commit `4f39e3e9`). For conditions that need Node.js-side logic, use `pollUntil()` with
`tauriPage.evaluate()`.

**Decision**: Use `pressKey()` helper for Space key instead of `tauriPage.keyboard.press('Space')`. **Why**:
TauriKeyboard dispatches key names as-is (sends `key: "Space"`), but the DOM spec uses `key: " "` for the space bar. The
`pressKey()` helper maps Playwright key names to their DOM-correct values.

**Decision**: `build.rs` conditionally generates `capabilities/playwright.json`. **Why**: The plugin's IPC permissions
(`playwright:default`) are only available when the `playwright-e2e` Cargo feature is enabled. Adding it to
`default.json` breaks non-feature builds. So `build.rs` generates the capability file when the feature is active and
removes it when the feature is not active. The file is gitignored.

## Gotchas

**Gotcha**: `npx playwright test` alone will fail with `ECONNREFUSED`. **Why**: The test suite does NOT launch the Cmdr
binary — it connects to an already-running app via `/tmp/tauri-playwright.sock`. Use
`./scripts/check.sh --check desktop-e2e-playwright` which handles the full lifecycle (build → launch → test → cleanup),
or start the app manually first (see "Manually" section above).

**Gotcha**: Navigation destroys page context. **Why**: After triggering SvelteKit navigation (settings, viewer), any
in-flight `evaluate()` result will be lost. Always `waitForSelector()` on the target page's element before evaluating
further JS.

**Gotcha**: `ensureAppReady()` must reset both the route AND the directories. **Why**: Navigating to SvelteKit route `/`
only ensures we're on the file explorer page — it does NOT change which directory either pane is showing. Pane
directories are persistent app state. So `ensureAppReady()` also emits `mcp-nav-to-path` Tauri events via
`window.__TAURI_INTERNALS__` to navigate both panes back to the fixture root's `left/` and `right/` directories.

**Gotcha**: File-operation tests need fixture recreation. **Why**: Tests that copy, move, rename, or create files mutate
the shared fixture directory. Without cleanup, later tests see stale artifacts. `recreateFixtures()` runs in
`test.beforeEach` in `file-operations.spec.ts` to reset text files and directories (bulk .dat files persist).

## `ensureAppReady` focus contract

By the time `ensureAppReady` returns, `document.activeElement` is inside `.dual-pane-explorer`. Tests that rely on
container-level keyboard handlers (Tab, ArrowDown, etc., as opposed to document-level F-key dispatch) can rely on this
invariant.

The helper enforces it with a poll-and-recover loop instead of a one-shot `waitForFunction`. Any `ModalDialog`-based
component (`CrashReportDialog`, `PtpcameradDialog`, `MtpPermissionDialog`, `ExpirationModal`, `CommercialReminderModal`,
`ErrorReportDialog`) calls `overlayElement?.focus()` in its `onMount`. The `(main)/+layout.svelte` onMount chain
(settings init → AI config → crash-report check → updater → AI state) runs in parallel with `(main)/+page.svelte` and
can mount one of these overlays _after_ `data-app-ready === 'true'`, stealing focus from the explorer. The explorer's
own `onfocusin` focus-guard can't reclaim it: the overlay sits outside `.dual-pane-explorer`, so the bubbling event
never reaches the guard.

On each poll iteration the helper dismisses any visible `.modal-overlay` via synthetic Escape, re-issues
`explorer.focus()`, then re-checks. The 99 % path costs one extra `evaluate()` over the old one-shot wait. On timeout
the helper throws with a snapshot of `activeElement` plus visible overlays so a future regression names the culprit
directly.

**If you add a new auto-mounted modal** in `(main)/+layout.svelte` or anywhere that can flip a render to a `ModalDialog`
after onboarding finishes, the recovery loop covers you — but consider whether the dialog should be gated on a user
gesture in production too, so it doesn't fight focus with the explorer in real use.
