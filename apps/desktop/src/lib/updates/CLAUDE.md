# Updates module

Frontend auto-update checker, restart toast, and manual "Check for updates" affordances.

## File map

- `updater.svelte.ts`: orchestration. The check loop, `checkForUpdates()` state machine, `startUpdateChecker()` (called
  once from `+layout.svelte`), the toast/onboarding gating, and `runMenuTriggeredCheck()`.
- `update-state.svelte.ts`: the module-level `updateState` `$state` singleton (`status`, `error`, version snapshots),
  re-exported from `updater.svelte.ts`.
- `update-status-text.ts`: pure `formatUpdateStatus()` (state → user-facing string), shared by Settings and toasts.
- `UpdateToastContent.svelte` (`id: 'update'`, persistent): restart prompt. `UpdateCheckToastContent.svelte`
  (`id: 'update-check'`, 10 s): menu-triggered phase status.

## Must-knows

- **Copy lives in the `updates.*` catalog**, resolved via `t()`/`tString()`; don't hardcode user-facing strings
  (`cmdr/no-raw-user-facing-string` is enforced here). See `DETAILS.md` § i18n.
- **Cleanup is mandatory.** `startUpdateChecker()` returns a teardown fn that `+layout.svelte` must call in `onDestroy`,
  or the interval leaks.
- **`.svelte.ts` extension is required** wherever `$state` lives (`updater.svelte.ts`, `update-state.svelte.ts`).
- **Platform asymmetry.** The code branches on `isMacOS()` (from `$lib/shortcuts/key-capture`), not
  `navigator.platform`. macOS calls three custom `invoke()` commands (`check_for_update`, `download_update`,
  `install_update`) so it exposes distinct `downloading` and `installing` phases; non-macOS dynamically imports
  `@tauri-apps/plugin-updater` and uses its fused `downloadAndInstall()`, staying in `downloading`. The custom updater
  Rust module isn't compiled off macOS. UIs treat both phases identically.
- **The restart toast must NOT show during onboarding.** Gated by the pure
  `shouldShowUpdateToast({ onboarded, onboardingShowing, status })` predicate; only `showUpdateToast()` calls
  `addToast`, never `addToast` directly. `onboarded` and `onboardingShowing` are module `$state` flags; reopening a gate
  re-attempts the toast, so a download finished during onboarding still surfaces.
- **The error catch logs `warn`, not `error`,** so transient background-check network failures don't trip the auto error
  reporter (Flow B). Don't raise it to `error`. Settings still shows the message via `updateState.error`.
- **The poll keeps running while an update is staged, and only `supersedesStagedUpdate` may write.** `checkForUpdates()`
  returns early on `checking` / `downloading` / `installing` (in-flight work owns the state machine), ❌ never on
  `ready`: guarding `ready` is what let installs sit 25-38 days on a build newer releases had passed. A re-check from
  `ready` leaves `updateState` untouched unless the server offers a strictly NEWER version, so an identical offer never
  rewrites the bundle. `DETAILS.md` § Re-checking while staged.
- **"Later" can't silence the prompt for good.** A staged update re-raises its toast every `RESTART_NUDGE_INTERVAL_MS`
  (24 h) off the poll, and a newly staged NEWER build resets the clock for a fresh prompt. ❌ Don't shorten the interval
  into nagging; the toast is persistent, so leaving it up costs the user nothing.
- **The update manifest endpoint is hardcoded in Rust** (via the API server), not in TypeScript.
- **Non-production guard:** `check_for_update` returns `None` unless the process is a real user's production install
  (inside a `.app` bundle, with none of `prod_instance::NON_PROD_ENV_VARS` set), so no dev, CI, E2E, or capture run
  reaches the update-check endpoint. The whole loop still runs in those instances; only the Rust command no-ops. See
  `src-tauri/src/updater/CLAUDE.md`.
- Test-only hooks `_resetUpdaterStateForTest` / `_setUpdateStatusForTest` exist for `updater.test.ts`; production must
  not call them.

Full details (state-machine diagram, menu wiring, decision rationale, dependencies): `DETAILS.md`.
