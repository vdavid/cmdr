# Playwright E2E tests (tauri-playwright)

Playwright E2E in Tauri mode: commands inject into the real webview over a Unix socket. The same specs run on macOS and
Linux (Docker), so a modifier key comes from `CTRL_OR_META`, ❌ never a hardcoded ⌘.

## Must-knows

- **The suite connects to a running app, it never launches one.** `pnpm check desktop-e2e-playwright` runs the whole
  lifecycle; a hand launch ALWAYS records its pid and chains `; kill "$(cat /tmp/cmdr-e2e-app.pid)"`. ❌ Never
  `pkill -f 'target.*Cmdr'`: every Cmdr shares that argv (shards differ only by ENV), so it SIGTERMs a concurrent suite.
  Recipes: DETAILS § "Running on macOS".
- **Run only the spec you're iterating on** (~10 min, and one broken test cascades). ❌ Keep `--project=tauri` in the
  `=` form; a space swallows the path.
- **Scattered failures across unrelated specs, different every run, mean saturation, not a regression.**
- **❌ Never `keyboard.press('Escape')`** to close an overlay: under Linux Xvfb it can vanish as an opaque timeout. Use
  `dismissOverlay` / `expectAndDismissToast` / `dismissAllToasts`; no double-Escape in `beforeEach`.
- **Two ways a helper claims success it never got.** Bare `await pollUntil(...)` returns `false` on timeout, so the test
  goes green: use `expect.poll(...).toBeTruthy()` (`bare-poll` flags it). And `.click()` on a `disabled` button
  dispatches NOTHING yet returns normally: press via `clickButtonByText` / `resolveConflict`, which wait for
  actionability (a dialog disables its buttons for the previous answer's whole IPC round trip, and back-to-back answers
  race it). One lost answer wedged 196 tests.
- **Exercise viewer + settings through the production multi-window flow** (`openViewerWindow` /
  `openSettingsWindowViaProd` / `closeScopedWindow`), ❌ never by routing the main window to `/viewer` or `/settings`:
  that hides a scoped page that can't call a Tauri command.
- **`ensureAppReady()` resets route, volume, AND directories, in that order** (without it, navigation silently no-ops).
  File-op specs also need `recreateFixtures()`: the tree is shared and they mutate it.
- **Need the other pane focused? Click its `.file-pane` and read `.is-focused` back.** ❌ Never dispatch the
  `pane.switch` TOGGLE at it, nor steer by `cmdr://state`'s `focused:`: a toggle on that stale mirror lands in the wrong
  pane. DETAILS § "Claiming a pane's focus".
- **One global `afterEach` guards TWO leaks: UI artifacts, and a dirty `left/` + `right/`.** A mutating spec restores
  the tree (`restoreFixtureTree(getFixtureRoot())`) or the guard names it. ❌ Don't relax it. **Holding an op? Drain it
  BEFORE the restore, in ONE hook** (`afterEach`s run in DECLARATION order): a restore under a live op deletes its
  source, and the retained `SourceNotFound` poisons the next one.
- **"Rows appeared" doesn't prove a WALK**: the instance indexes its tree at launch, so a spec needing one takes the
  index away first (`search-walk-ground.ts`).
- **Two fakes**: the clipboard is mocked (a Rust `Mutex`, not `NSPasteboard`), and `tauri-plugin-store` reads your REAL
  store files unless redirected, so a locally flipped setting becomes a CI-only failure.
- **Every harness pins BOTH locale halves before launch**: `appearance.language`, plus `-AppleLocale` args for the
  formatting no setting reaches. DETAILS § "The locale pin".
- **`emitBackendEvent` drives UI off a synthetic backend event.** The app is SHARED: emit the terminal event that clears
  it (test AND `afterEach`), under an id nothing real claims. DETAILS § "Synthetic backend events".
- **The marketing capture (`marketing-shots.spec.ts`) photographs real folders, with NO fixture tree.** ❌ Never point
  it at a fixture root or set `CMDR_E2E_START_PATH`: the guard deletes anything outside the manifest. It shoots via
  `screencapture -l`, not the plugin (no shadow), and ❗ needs the machine left alone; say both first. Contract:
  DETAILS.

Run recipes, architecture, sharding, app modes, the overlay and capture contracts, and decisions: `DETAILS.md`. Read it
before any non-trivial work here.
