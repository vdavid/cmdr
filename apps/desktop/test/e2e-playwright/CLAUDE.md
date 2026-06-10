# Playwright E2E tests (tauri-playwright)

Playwright-based E2E tests for Cmdr, using tauri-playwright in Tauri mode. Commands inject directly into the real Tauri
webview via Unix socket. The same tests run on macOS (native) and Linux (Docker); platform differences (Ctrl vs Meta)
are handled by the `CTRL_OR_META` constant in `helpers.ts`.

For the architecture diagram, the full per-spec file table, run recipes, multi-window patterns, and decisions, see
[DETAILS.md](DETAILS.md).

## Module map

- `playwright.config.ts`, `fixtures.ts`, `global-setup.ts` / `global-teardown.ts`: config, window-title decoration,
  fixture-tree lifecycle.
- `helpers.ts` re-exports `helpers/` (`core.ts`, `app-lifecycle.ts`, `cursor.ts`, `overlays-and-dialogs.ts`,
  `windows.ts`, `navigation.ts`); specs import `from './helpers.js'`.
- `*.spec.ts`: the suites (app, file-operations, conflict-*, file-watching, focus-trap, viewer-*, settings, mtp-*, smb,
  drag-drop-*).

## Must-knows

- **Run only the spec you're iterating on.** The full suite takes ~10 min and a broken test takes the app down with it,
  cascading connection-error failures into later specs. See DETAILS.md § "Running a single spec" for the exact
  invocations (the `pnpm` script hardcodes `--project tauri`, so a positional spec path collides; use
  `npx playwright test` for file filters).
- **`npx playwright test` alone fails with `ECONNREFUSED`.** The suite doesn't launch the app; it connects to an
  already-running one via the socket. Use `pnpm check desktop-e2e-playwright` (full lifecycle), or start the app
  manually first. When manual, ALWAYS pair the launch with `; pkill -f 'target.*Cmdr'` (use `;`, not `&&`, so cleanup
  runs even on failure): nothing else kills the main process, so leaks pile up fast.
- **Never `tauriPage.keyboard.press('Escape')` to close a dialog/popover/dropdown/palette.** Under Linux Xvfb, X11 focus
  delivery is unreliable and the keystroke can vanish, failing with an opaque timeout that looks like a flake. Use
  `dismissOverlay(tauriPage)` (synthetic Escape on the topmost overlay) and `expectAndDismissToast(tauriPage, substring)`
  (asserts the toast, then dismisses: the wording IS the contract). `fixtures.ts`'s global `afterEach` fails any test
  that leaks an overlay/toast and auto-cleans, so no defensive double-Escape cleanups in `beforeEach`.
- **Bare `await pollUntil(...)` is silent on timeout** (returns `false`, doesn't throw), so the test passes green even
  when the condition never holds. Use `await expect.poll(() => cond(), { timeout }).toBeTruthy()` (preferred) or
  `expect(await pollUntil(...)).toBe(true)`. Same trap for every `Promise<boolean>` helper (`pollFs`, `pollUntilValue`,
  `pollActiveMode`, `pollOverlayGone`, `pollFocusedPane`). The `bare-poll` fast-lane check flags these; opt out only for
  genuine best-effort cleanups with `// allowed-bare-poll: <reason>`.
- **Exercise viewer + settings through the production multi-window flow**, not by routing the main window to `/viewer` or
  `/settings` (that skips label uniqueness, restricted capabilities, the cross-window focus/close lifecycle). Use
  `openViewerWindow` / `openSettingsWindowViaProd` / `closeScopedWindow` and interact via the scoped page. When a scoped
  page can't call a Tauri command, that's a REAL bug (production hits the same restricted-capability wall): fix the
  capability file or use a permitted command, don't route around it.
- **`ensureAppReady()` resets route, volume, AND directories, in that order.** The volume reset is required:
  `navigateToPath` rejects `mcp-nav-to-path` for non-local panes, so without it nav silently no-ops and the readiness
  poll times out on an empty pane. Pane state persists across tests, so `/` alone isn't a clean pane. By return,
  `document.activeElement` is inside `.dual-pane-explorer` (a poll-and-recover loop reclaims focus from auto-mounted
  modals, covering any new `ModalDialog` in `(main)/+layout.svelte`).
- **File-op specs must recreate fixtures** (`recreateFixtures()` in `test.beforeEach`): copy/move/rename/create mutate
  the shared fixture tree, and stale artifacts break later tests.
- **The clipboard is mocked, not real** under the `playwright-e2e` feature: `Cmd+C/X/V` go through the same IPC but the
  bytes live in a Rust `Mutex`, not `NSPasteboard`. `pbpaste` won't see test contents; read mock state through the
  clipboard IPC commands. See `clipboard/CLAUDE.md`.
- **Frontend `tauri-plugin-store` stores read your REAL store files unless redirected.** The checker launches the
  pre-built binary directly, so `app_data_dir()` keeps the prod identifier and a bare store name resolves to your real
  data; a locally-flipped setting then leaks into tests (passes in CI, fails locally). `getStore()` resolves through
  `resolveStorePath(name)`, which a `CMDR_DATA_DIR` instance redirects. If a spec on persisted UI state fails locally
  but passes in CI, suspect a stale value in your real store.

Architecture, flows, and decision detail: [DETAILS.md](DETAILS.md). Read it in whole before structural changes here.
