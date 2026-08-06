# Playwright E2E tests (tauri-playwright)

Playwright E2E for Cmdr in Tauri mode: commands inject into the real Tauri webview over a Unix socket. The same specs
run on macOS (native) and Linux (Docker); platform differences ride `CTRL_OR_META` in `helpers.ts`.
`playwright.config.ts`, `fixtures.ts`, and `global-{setup,teardown}.ts` own config, title decoration, and the fixture
tree; `helpers.ts` re-exports `helpers/`; a spec's filename picks its shard (DETAILS § Files).

## Must-knows

- **Run only the spec you're iterating on.** The full suite takes ~10 min and one broken test cascades connection errors
  into the rest. `pnpm test:e2e:playwright <spec-path>` filters by file, `--grep` by name; ❌ keep `--project=tauri` in
  the `=` form (a space swallows the spec path).
- **Scattered failures across unrelated specs, different every run, mean machine saturation, not a regression.** Re-run
  the failing slow checks one at a time before believing them.
- **The instance INDEXES its fixture tree at launch**, so ❌ "rows appeared" doesn't prove a WALK: a spec needing one
  takes the index away first (`search-walk-ground.ts`).
- **`npx playwright test` alone fails with `ECONNREFUSED`** — the suite connects to a running app, it doesn't launch
  one. Use `pnpm check desktop-e2e-playwright`, or launch manually and ALWAYS pair it with `; pkill -f 'target.*Cmdr'`
  (`;`, not `&&`, so cleanup runs on failure too).
- **❌ Never `keyboard.press('Escape')` to close a dialog, popover, dropdown, or palette.** Under Linux Xvfb the
  keystroke can vanish and fail as an opaque timeout that looks like a flake. Use `dismissOverlay` and
  `expectAndDismissToast` (the wording IS the contract). `fixtures.ts`'s global `afterEach` fails and cleans any leak,
  so ❌ no defensive double-Escape in `beforeEach`.
- **Bare `await pollUntil(...)` is silent on timeout** (returns `false`), so the test goes green when the condition
  never held. Use `expect.poll(...).toBeTruthy()` or `expect(await pollUntil(...)).toBe(true)`; same trap for every
  `Promise<boolean>` helper. The `bare-poll` check flags these; opt out with `// allowed-bare-poll: <reason>`.
- **Exercise viewer + settings through the production multi-window flow** (`openViewerWindow` /
  `openSettingsWindowViaProd` / `closeScopedWindow`), ❌ never by routing the main window to `/viewer` or `/settings`:
  that skips label uniqueness, restricted capabilities, and the focus/close lifecycle. A scoped page that can't call a
  Tauri command is a REAL bug.
- **`ensureAppReady()` resets route, volume, AND directories, in that order.** The volume reset is required:
  `navigateToPath` rejects `mcp-nav-to-path` for non-local panes and pane state persists across tests, so without it nav
  silently no-ops. By return, focus is inside `.dual-pane-explorer` and the LEFT pane is active.
- **File-op specs must recreate fixtures** (`recreateFixtures()` in `test.beforeEach`): copy/move/rename/create mutate
  the shared tree.
- **The clipboard is mocked, not real** under the `playwright-e2e` feature: the bytes live in a Rust `Mutex`, not
  `NSPasteboard`, so `pbpaste` won't see them. Read mock state through the clipboard IPC commands.
- **The i18n capture harness screenshots ONLY through `shoot()`** (`i18n-capture-helpers.ts`), which makes the window
  frontmost, confirms it, waits for real frames, and then verifies the PNG carries content. macOS composites only a
  frontmost window, so a bare `page.screenshot()` on a backgrounded one silently writes the empty startup frame with
  every other signal healthy; one run shipped 31 blank images that way. ❌ Don't call `page.screenshot()` directly here,
  don't loosen the pixel check, and don't answer a blank surface with a longer sleep.
- **`tauri-plugin-store` reads your REAL store files unless redirected** (`getStore()` → `resolveStorePath`, isolated by
  `CMDR_DATA_DIR`), so a persisted-UI-state spec that passes in CI but fails locally usually means a stale local value.

Architecture, run recipes, sharding, and decisions: `DETAILS.md`. Read it before any non-trivial work here: editing,
planning, reorganizing, or advising.
