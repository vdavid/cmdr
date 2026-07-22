# Playwright E2E tests (tauri-playwright)

Playwright E2E for Cmdr in Tauri mode: commands inject into the real Tauri webview over a Unix socket. The same specs
run on macOS (native) and Linux (Docker); platform differences (Ctrl vs Meta) ride the `CTRL_OR_META` constant in
`helpers.ts`. Architecture, per-spec table, run recipes, and decisions: `DETAILS.md`.

## Module map

- `playwright.config.ts`, `fixtures.ts`, `global-setup.ts` / `global-teardown.ts`: config, window-title decoration,
  fixture-tree lifecycle.
- `helpers.ts` re-exports `helpers/` (`core`, `app-lifecycle`, `cursor`, `overlays-and-dialogs`, `windows`,
  `navigation`); specs import `from './helpers.js'`.
- `*.spec.ts`: the suites (per-spec table in DETAILS.md).

## Must-knows

- **Run only the spec you're iterating on.** The full suite takes ~10 min and a broken test takes the app down with it,
  cascading connection-error failures into later specs. `pnpm test:e2e:playwright <spec-path>` filters by file, `--grep`
  by name. ❌ Keep the script's `--project=tauri` in the `=` form: with a space, playwright's multi-value `--project`
  swallows the spec path. DETAILS.md § "Running a single spec".
- **Scattered failures across unrelated specs, different every run, mean machine saturation, not a regression**: re-run
  the failing slow checks one at a time before believing them. DETAILS.md § "Slow-check results are unreliable".
- **`npx playwright test` alone fails with `ECONNREFUSED`.** The suite doesn't launch the app; it connects to a running
  one over the socket. Use `pnpm check desktop-e2e-playwright` (full lifecycle), or launch manually and ALWAYS pair it
  with `; pkill -f 'target.*Cmdr'` (`;`, not `&&`, so cleanup runs on failure too): nothing else stops the main process.
- **Never `tauriPage.keyboard.press('Escape')` to close a dialog/popover/dropdown/palette.** Under Linux Xvfb, X11 focus
  delivery is unreliable and the keystroke can vanish, failing with an opaque timeout that looks like a flake. Use
  `dismissOverlay(tauriPage)` and `expectAndDismissToast(tauriPage, substring)` (asserts then dismisses; the wording IS
  the contract). `fixtures.ts`'s global `afterEach` fails and auto-cleans any test leaking an overlay/toast, so no
  defensive double-Escape in `beforeEach`.
- **Bare `await pollUntil(...)` is silent on timeout** (returns `false`, doesn't throw), so the test passes green even
  when the condition never holds. Use `await expect.poll(() => cond(), { timeout }).toBeTruthy()` or
  `expect(await pollUntil(...)).toBe(true)`. Same trap for every `Promise<boolean>` helper (`pollFs`, `pollActiveMode`).
  The `bare-poll` check flags these; opt out with `// allowed-bare-poll: <reason>`.
- **Exercise viewer + settings through the production multi-window flow** (`openViewerWindow` /
  `openSettingsWindowViaProd` / `closeScopedWindow`, via the scoped page), not by routing the main window to `/viewer`
  or `/settings` (that skips label uniqueness, restricted capabilities, focus/close lifecycle). A scoped page that can't
  call a Tauri command is a REAL bug: fix the capability or use a permitted command.
- **`ensureAppReady()` resets route, volume, AND directories, in that order.** The volume reset is required:
  `navigateToPath` rejects `mcp-nav-to-path` for non-local panes (pane state persists across tests), so without it nav
  silently no-ops and the readiness poll times out on an empty pane. By return, `document.activeElement` is inside
  `.dual-pane-explorer` AND the LEFT pane is active (DETAILS.md § focus contract).
- **File-op specs must recreate fixtures** (`recreateFixtures()` from `../e2e-shared/fixtures.js` in `test.beforeEach`):
  copy/move/rename/create mutate the shared tree, and stale artifacts break later tests.
- **The clipboard is mocked, not real** under the `playwright-e2e` feature: `Cmd+C/X/V` go through the same IPC but the
  bytes live in a Rust `Mutex`, not `NSPasteboard`. `pbpaste` won't see test contents; read mock state through the
  clipboard IPC commands (`src-tauri/src/clipboard/CLAUDE.md`).
- **`tauri-plugin-store` stores read your REAL store files unless redirected**, so a locally-flipped setting leaks in.
  `getStore()` resolves through `resolveStorePath(name)`, which a `CMDR_DATA_DIR` instance redirects to isolated data. A
  persisted-UI-state spec passing in CI but failing locally usually means a stale value in your real store.

Read `DETAILS.md` before any non-trivial work here: editing, planning, reorganizing, or advising.
