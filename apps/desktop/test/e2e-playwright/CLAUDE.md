# Playwright E2E tests (tauri-playwright)

Playwright in Tauri mode: commands inject into the real webview over a Unix socket. The same specs run on macOS and
Linux (Docker), so a modifier key comes from `CTRL_OR_META`, ❌ never a hardcoded ⌘.

## Must-knows

- **The suite connects to a running app; it never launches one.** `pnpm check desktop-e2e-playwright` runs the whole
  lifecycle. A hand launch records its pid and chains `; kill "$(cat /tmp/cmdr-e2e-app.pid)"`. ❌ Never
  `pkill -f 'target.*Cmdr'`: every Cmdr shares that argv, so it SIGTERMs a concurrent suite. DETAILS § "Running on
  macOS".
- **Iterate on one spec**, keeping `--project=tauri` in the `=` form (a space swallows the path). Scattered failures
  that differ every run mean saturation, not a regression.
- **❌ Never `keyboard.press('Escape')` to close an overlay**: under Xvfb it can vanish as an opaque timeout. Use
  `dismissOverlay`, `expectAndDismissToast`, or `escapeOverlayUntilGone`.
- **A helper can return normally having done nothing.** Assert a poll with `expect.poll(...).toBeTruthy()`, never a bare
  `pollUntil`. Press buttons with `clickButtonByText` (a disabled `.click()` dispatches nothing) and Ark widgets with
  `pointerClick`. After a write, `waitForOperationsToSettle`, then end with `expectAndDismissToast`. DETAILS § "Waiting
  for a write to settle".
- **Close the onboarding wizard from a `finally`** (`closeOnboardingWizardIfOpen`): left open, it refuses every MCP call
  and fails every later test on the shard. Match its rows by `data-checklist-item`, ❌ never by label.
- **Drive viewer and settings through the real multi-window flow** (`openViewerWindow`, `openSettingsWindowViaProd`,
  `closeScopedWindow`), ❌ never by routing the main window there.
- **`ensureAppReady()` resets route, volume, and directories.** File-op specs add `recreateFixtures()` and pass
  `expectedLeftPaneEntries(fixtureRoot)`. DETAILS § "Fixture-churn readiness".
- **To focus a pane, click its `.file-pane` and read `.is-focused` back**, ❌ never the `pane.switch` toggle or
  `cmdr://state`'s stale `focused:`. DETAILS § "Claiming a pane's focus".
- **The global `afterEach` fails a spec that leaks UI or leaves `left/` or `right/` dirty**; ❌ don't relax it. Restore
  with `restoreFixtureTree`, and when holding an op, `drainOperations()` first in the SAME hook.
- **"STOPPED ANSWERING" means read up**: an earlier test killed the app. DETAILS § "The dead-app circuit breaker".
- **The harness walls the app off from your machine**: Downloads and `tauri-plugin-store` are redirected (a new store
  needs the redirect too, or your local settings leak into runs), the clipboard is a Rust fake, and both locale halves
  are pinned. DETAILS § "The locale pin".
- **`emitBackendEvent` state is shared**: emit the clearing event in the test AND `afterEach`. Rows appearing doesn't
  prove a walk (`search-walk-ground.ts`). DETAILS § "Synthetic backend events".
- **`marketing-shots.spec.ts` shoots real folders with NO fixture tree**: ❌ never set `CMDR_E2E_START_PATH` for it (the
  guard deletes anything outside the manifest), and it needs the machine left alone; say both first.

Run recipes, architecture, sharding, app modes, contracts, and decisions: `DETAILS.md`. Read it before any non-trivial
work here: editing, planning, reorganizing, or advising.
