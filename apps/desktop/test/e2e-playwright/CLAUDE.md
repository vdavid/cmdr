# Playwright E2E tests (tauri-playwright)

Playwright in Tauri mode: commands inject into the real webview over a Unix socket. The same specs run on macOS and
Linux (Docker), so a modifier key comes from `CTRL_OR_META`, ❌ never a hardcoded ⌘.

## Must-knows

- **The suite connects to a running app; it never launches one.** `pnpm check desktop-e2e-playwright` runs the whole
  lifecycle. A hand launch kills its recorded pid, ❌ never `pkill -f 'target.*Cmdr'` (it hits a concurrent suite).
  DETAILS § "Running on macOS".
- **Iterate on one spec** with `--project=tauri` in the `=` form. A red lane re-runs each failure alone: `contention`
  isn't yours, `real` is.
- **❌ Never `keyboard.press('Escape')` to close an overlay**: use `dismissOverlay`, `expectAndDismissToast`, or
  `escapeOverlayUntilGone`.
- **A helper can return having done nothing.** Assert polls with `expect.poll(...).toBeTruthy()`, never a bare
  `pollUntil`. Press buttons with `clickButtonByText`, Ark widgets with `pointerClick`.
- **A wait helper names the boundary it waits on.** After a write, a UI assertion wants `waitForTransferUiToSettle`, not
  only `waitForBackendOperationsToSettle`. DETAILS § "Waiting for a write to settle".
- **Every wait budget goes through `waitBudget(N)`**; ❌ never pin a bare number or one half of a scaled pair. DETAILS §
  "The load-scaled wait budget".
- **Close the onboarding wizard from a `finally`** (`closeOnboardingWizardIfOpen`), or it wedges the shard. Match its
  rows by `data-checklist-item`, ❌ never by label.
- **Drive viewer and settings through the real multi-window flow** (`openViewerWindow`, `openSettingsWindowViaProd`,
  `closeScopedWindow`), ❌ never by routing the main window there.
- **`ensureAppReady()` resets route, volume, and directories.** File-op specs add `recreateFixtures()` and
  `expectedLeftPaneEntries(fixtureRoot)`. DETAILS § "Fixture-churn readiness".
- **To focus a pane, click its `.file-pane` and read `.is-focused` back**, ❌ never `pane.switch` or `cmdr://state`'s
  `focused:`. DETAILS § "Claiming a pane's focus".
- **The global `afterEach` fails a spec that leaks UI or dirties `left/` or `right/`**; ❌ don't relax it. Restore with
  `restoreFixtureTree`, after `drainOperations()` in the SAME hook.
- **"STOPPED ANSWERING" means read up**: an earlier test killed the app. DETAILS § "The dead-app circuit breaker".
- **The harness walls the app off from your machine** (Downloads, `tauri-plugin-store`, clipboard, locale). A new store
  needs the redirect too. DETAILS § "The locale pin".
- **`emitBackendEvent` state is shared**: emit the clearing event in the test AND `afterEach`. DETAILS § "Synthetic
  backend events".
- **A spec that writes the `CMDR_MANAGED_PREFS_FILE` policy removes it and waits for unmanaged**, or the shard's later
  specs inherit it. DETAILS § "The managed-policy (MDM) specs".
- **`marketing-shots.spec.ts` shoots real folders**: ❌ never set `CMDR_E2E_START_PATH` for it (its guard deletes
  anything outside the manifest), and it needs the machine left alone; say both first.
- **A `*.test.ts` here runs under Vitest** through `vitest-playwright-shim.ts`, without the Tauri matchers. DETAILS §
  "The Vitest shim".

Run recipes, architecture, sharding, app modes, contracts, and decisions: `DETAILS.md`. Read it before any non-trivial
work here: editing, planning, reorganizing, or advising.
