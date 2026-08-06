# Playwright E2E tests (tauri-playwright)

Playwright E2E in Tauri mode: commands inject into the real webview over a Unix socket. The same specs run on macOS and
Linux (Docker), so a modifier key comes from `CTRL_OR_META`, never a hardcoded ⌘.

## Must-knows

- **The suite connects to a running app, it never launches one**, so `npx playwright test` alone dies with
  `ECONNREFUSED`. Use `pnpm check desktop-e2e-playwright`, or launch by hand and ALWAYS chain
  `; pkill -f 'target.*Cmdr'` (`;`, not `&&`, so cleanup survives a failure).
- **Run only the spec you're iterating on**: the full suite takes ~10 min and one broken test cascades into the rest. ❌
  Keep `--project=tauri` in the `=` form; a space swallows the spec path.
- **Scattered failures across unrelated specs, different every run, mean machine saturation, not a regression.**
- **❌ Never `keyboard.press('Escape')`** to close a dialog, popover, dropdown, or palette: under Linux Xvfb the
  keystroke can vanish as an opaque timeout. Use `dismissOverlay` / `expectAndDismissToast` / `dismissAllToasts`, and no
  defensive double-Escape in `beforeEach` (the global `afterEach` fails and cleans any leak).
- **Bare `await pollUntil(...)` is silent on timeout** (returns `false`), so the test goes green when the condition
  never held. Use `expect.poll(...).toBeTruthy()`; same trap for every `Promise<boolean>` helper (`bare-poll` flags it).
- **Exercise viewer + settings through the production multi-window flow** (`openViewerWindow` /
  `openSettingsWindowViaProd` / `closeScopedWindow`), ❌ never by routing the main window to `/viewer` or `/settings`: a
  scoped page that can't call a Tauri command is a REAL bug that route would hide.
- **`ensureAppReady()` resets route, volume, AND directories, in that order**; without the volume reset, navigation
  silently no-ops. File-op specs also need `recreateFixtures()`: the tree is shared and they mutate it.
- **"Rows appeared" doesn't prove a WALK**: the instance indexes its fixture tree at launch, so a spec needing a real
  walk takes the index away first (`search-walk-ground.ts`).
- **Two fakes**: the clipboard is mocked (a Rust `Mutex`, not `NSPasteboard`), and `tauri-plugin-store` reads your REAL
  store files unless redirected, so a locally flipped setting becomes a failure CI never sees.
- **❗ A capture run needs the machine left alone, so say so before starting one**, and it photographs only through
  `shoot()` — ❌ never `page.screenshot()`, never a looser pixel check, never a longer sleep for a blank surface.

Run recipes, architecture, sharding, app modes, the overlay and capture contracts, and decisions: `DETAILS.md`. Read it
before any non-trivial work here: editing, planning, reorganizing, or advising.
