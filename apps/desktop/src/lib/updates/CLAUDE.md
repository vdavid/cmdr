# Updates module

Frontend auto-update checker, restart toast, and manual "Check for updates" affordances.

## File map

- `updater.svelte.ts`: orchestration. The check loop, `checkForUpdates()` state machine, `startUpdateChecker()` (called
  once from `+layout.svelte`), the toast/onboarding gating, and `runMenuTriggeredCheck()`.
- `update-state.svelte.ts`: the module-level `updateState` `$state` singleton (`status`, `error`, version snapshots),
  re-exported from `updater.svelte.ts`.
- `update-status-text.ts`: pure `formatUpdateStatus()` (state → user-facing string), shared by Settings and toasts.
- `update-analytics.ts`: the `update_check` event's vocabulary and its one emitter.
- `UpdateToastContent.svelte` (`id: 'update'`, persistent): restart prompt. `UpdateCheckToastContent.svelte`
  (`id: 'update-check'`, 10 s): menu-triggered phase status.
- `MoveToApplicationsDialog.svelte` (`dialogId: 'move-to-applications'`): the nudge for an install that can't write its
  own bundle. Mounted by `routes/(main)/+layout.svelte` off `updateBlockerNotice`.

## Must-knows

- **Copy lives in the `updates.*` catalog**, resolved via `t()`/`tString()`; don't hardcode user-facing strings
  (`cmdr/no-raw-user-facing-string` is enforced here). `DETAILS.md` § i18n.
- **Cleanup is mandatory.** `startUpdateChecker()` returns a teardown fn that `+layout.svelte` must call in `onDestroy`,
  or the interval leaks. `.svelte.ts` is required wherever `$state` lives.
- **Platform asymmetry.** The code branches on `isMacOS()` (from `$lib/shortcuts/key-capture`), not
  `navigator.platform`. macOS calls three custom `invoke()` commands (`check_for_update`, `download_update`,
  `install_update`) so it exposes distinct `downloading` and `installing` phases; non-macOS dynamically imports
  `@tauri-apps/plugin-updater` and uses its fused `downloadAndInstall()`, staying in `downloading`. The custom updater
  Rust module isn't compiled off macOS. UIs treat both phases identically.
- **Nothing may show during onboarding.** The restart toast is gated by the pure
  `shouldShowUpdateToast({ onboarded, onboardingShowing, status })`; only `showUpdateToast()` calls `addToast`, never
  `addToast` directly. Reopening a gate re-attempts both the toast and a held-back move-to-Applications nudge, so
  neither is lost.
- **The error catch logs `warn`, not `error`,** so transient background-check network failures don't trip the auto error
  reporter (Flow B). Don't raise it to `error`. Settings still shows the message via `updateState.error`.
- **The poll keeps running while an update is staged, and only `supersedesStagedUpdate` may write.** `checkForUpdates()`
  returns early on `checking` / `downloading` / `installing` (in-flight work owns the state machine), ❌ never on
  `ready`: that guard let installs sit 25-38 days on a build newer releases had passed. A re-check from `ready` touches
  nothing unless the server offers a strictly NEWER version. `DETAILS.md` § Re-checking while staged.
- **"Later" can't silence the prompt for good.** A staged update re-raises its toast every `RESTART_NUDGE_INTERVAL_MS`
  (24 h) off the poll; a newer staged build resets the clock. ❌ Don't shorten it into nagging.
- **An install that can't write its own bundle never downloads.** macOS-only: `update_write_blocker` says whether App
  Translocation or a read-only volume is in the way, and a blocker raises the move-to-Applications dialog (once a
  session) instead of ~63 MB the install could never apply. ❌ Don't treat a FAILED classification as a blocker: it
  saves a doomed download, it isn't permission to update. `DETAILS.md` § When the bundle can't be written.
- **Every exit of `checkForUpdates()` fires one `update_check` event** (`update-analytics.ts`), and the failing phase is
  read off `updateState.status`, ❌ never off the error message. A new exit without an event is a hole in the one view
  of the update path there is. Catalog: `src-tauri/src/analytics/DETAILS.md`.
- **The update manifest endpoint is hardcoded in Rust** (via the API server), not in TypeScript.
- **Non-production guard:** `check_for_update` returns `None` unless the process is a real user's production install
  (inside a `.app` bundle, none of `prod_instance::NON_PROD_ENV_VARS` set), so no dev, CI, E2E, or capture run reaches
  the endpoint. The loop still runs there; only the Rust command no-ops. `src-tauri/src/updater/CLAUDE.md`.
- Test-only hooks `_resetUpdaterStateForTest` / `_setUpdateStatusForTest` exist for `updater.test.ts`; production must
  not call them.

Full details (state-machine diagram, menu wiring, decision rationale, dependencies): `DETAILS.md`.
