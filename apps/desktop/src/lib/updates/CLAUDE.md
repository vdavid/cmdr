# Updates module

Auto-update checker, restart notification, and the user-triggered "Check for updates" affordances for the Cmdr desktop
app.

## Key files

- **`updater.svelte.ts`**: Module-level `$state` singleton, update check loop, download logic
- **`update-status-text.ts`**: Pure formatter: state → user-facing status string (shared by Settings and toast)
- **`UpdateToastContent.svelte`**: Toast body shown when an update is ready to install (`id: 'update'`, persistent)
- **`UpdateCheckToastContent.svelte`**: Toast body for the menu-triggered phase status (`id: 'update-check'`, 10 s
  timeout)

## Architecture

`startUpdateChecker()` is called once from `+layout.svelte` on app start. It:

1. If `updates.autoCheck` is `true` (the default), fires an immediate `checkForUpdates()` call and schedules a
   `setInterval` using `advanced.updateCheckInterval`. If `false`, skips both — the user has opted out of the background
   poll loop.
2. Listens for `advanced.updateCheckInterval` changes; clears and re-creates the interval when the value changes (only
   if the loop is currently running).
3. Returns a cleanup function that `+layout.svelte` calls in `onDestroy`.

`applyAutoCheckEnabled(enabled)` is exported so the live-apply hook in `settings-applier.ts`'s
`passthroughBackendHandlers` can flip the poll loop on or off in place when the user toggles `updates.autoCheck` (from
the Settings UI switch, the onboarding wizard's step 3, or any MCP/IPC writer). On enable, it fires one immediate check
so the user doesn't wait the full cadence for the first tick; on disable, it stops the loop but leaves
`updateState.status` alone so any in-flight update isn't lost.

`checkForUpdates()` transitions the state machine: `idle → checking → downloading → installing → ready` (macOS) or
`idle → checking → downloading → ready` (non-macOS; see asymmetry below). If an update is found, it downloads and
installs automatically with no user confirmation needed. The user is only asked at the `ready` stage whether to restart
now or later.

```
idle ──invoke──► checking ──update found──► downloading ──► installing ──► ready
  ▲                  │                                      (macOS only)
  └──────error/no update
```

`updateState` (the module-level `$state` singleton) is exported so UIs can read the current phase reactively. It carries
`status`, `error`, `previousVersion` (snapshot of `getVersion()` taken when entering `checking`), and `nextVersion` (the
target version, set when an update is found). The Settings > Updates section and `UpdateCheckToastContent.svelte` both
read the singleton and format their status string through `formatUpdateStatus()` in `update-status-text.ts`.

The macOS path runs `download_update` and `install_update` as two separate Tauri commands, so we expose distinct
`downloading` and `installing` phases. The non-macOS path uses the Tauri updater plugin's fused `downloadAndInstall()`
call, so it stays in `downloading` for the whole step. UIs treat both phases identically (different status strings, same
button-disabled rule).

The frontend branches on platform at the top of `checkForUpdates()`:

- **macOS**: calls three custom Tauri commands via `invoke()`: `check_for_update`, `download_update`, `install_update`.
  The Rust backend at `src-tauri/src/updater/` syncs files _into_ the existing `.app` bundle, preserving the inode and
  TCC/Full Disk Access permissions.
- **Non-macOS**: dynamically imports `@tauri-apps/plugin-updater` and calls `check()` / `downloadAndInstall()`. The
  custom updater Rust module is not compiled on these platforms.

When `status` becomes `'ready'`, the updater funnels through the `showUpdateToast()` helper instead of calling
`addToast` directly. The helper consults `shouldShowUpdateToast({ onboarded, onboardingShowing, status })`, a pure,
unit-tested predicate, and only fires `addToast(UpdateToastContent, { id: 'update', dismissal: 'persistent' })` when all
three conditions hold. `UpdateToastContent.svelte` renders the toast body, calls `relaunch()` directly from
`@tauri-apps/plugin-process` for the restart action, and handles the "Later" button by calling `dismissToast('update')`.
There is no local `$state` dismissed flag; dismissal is managed entirely by the toast infrastructure.

### Menu-triggered "Check for updates"

Two affordances let the user manually run a check and watch its progress:

- **Settings > Updates**: a "Check for updates" button at the top of the section, disabled while
  `updateState.status !== 'idle'`, with a status string below it derived from `formatUpdateStatus(updateState)`. The
  error case renders a follow-up "Send error report" link that calls `openErrorReportDialog(\`Update check failed:
  ${error}\`)`.
- **Cmdr menu > Check for updates…**: dispatched as the `app.checkForUpdates` command. The frontend handler calls
  `runMenuTriggeredCheck()` which fires `addToast(UpdateCheckToastContent, { id: 'update-check', timeoutMs: 10000 })`,
  then awaits `checkForUpdates()`. Because `addToast` deduplicates by id, the toast updates in place as the phase
  changes (`checking…` → `downloading v… (current: v…)…` → `installing v…`). When `status` flips to `ready` the helper
  dismisses `'update-check'` so it doesn't overlap with the persistent restart toast (`id: 'update'`).

The native menu item lives in the Cmdr submenu (macOS) right after "Enter license key…", wired through
`menu_id_to_command` / `command_id_to_menu_id` in `src-tauri/src/menu/mod.rs`, with the SF Symbol `arrow.down.circle`
mapped in `macos.rs`. On Linux the same command appears at the bottom of the Edit submenu after the license item.

### Onboarding gating

The toast must NOT show during first-launch onboarding (the user just downloaded the app; telling them to "restart to
update" is confusing) nor while the onboarding wizard's later steps are on screen (it'd stack two prompts). Two
module-level `$state` flags drive this:

- `onboarded`: seeded from `loadSettings().isOnboarded` on `startUpdateChecker()` start, then flipped by
  `notifyOnboardingComplete()` (also persists `isOnboarded: true`).
- `onboardingShowing`: flipped by `setOnboardingShowing(value)` from `routes/(main)/+page.svelte` whenever the
  onboarding wizard opens or closes. The flag spans the whole wizard lifecycle (step 1 FDA, step 2 AI, step 3 optional)
  — the "restart to update" toast would land just as awkwardly on the AI step as on the FDA step.

When a gate opens (`notifyOnboardingComplete()` runs, or `setOnboardingShowing(false)` flips), the helper re-attempts
the toast. If the download completed during onboarding, `updateState.status` stays `'ready'` and the toast shows on
unblock. Nothing is lost.

Two test-only hooks (`_resetUpdaterStateForTest`, `_setUpdateStatusForTest`) exist for the unit tests in
`updater.test.ts`. Production code must not call them.

## Key decisions

**Decision**: Platform branching in the frontend (`navigator.platform` check). **Why**: The custom updater Rust module
is macOS-only (`#[cfg(target_os = "macos")]`). On non-macOS, the three `invoke()` commands don't exist, so the frontend
dynamically imports `@tauri-apps/plugin-updater` and uses its API instead. This is a small if/else; the state machine
and toast logic are shared across both paths.

**Decision**: Auto-download without user confirmation; only prompt for restart. **Why**: Updates are small (~63 MB).
Asking "download now?" adds a decision point that most users will always accept. Downloading silently in the background
respects the user's time. The restart prompt is necessary because the app must quit to apply the update; that's the only
destructive action.

**Decision**: State machine guards against re-checking during download or ready states. **Why**: `checkForUpdates`
returns early if status is `downloading` or `ready`. Without this, a periodic interval tick could start a second
download or overwrite the `ready` state with a new `checking` cycle, losing the pending update reference.

**Decision**: Update toast uses `dismissal: 'persistent'` with the global `id: 'update'`. **Why**: Transient toasts
auto-dismiss after 4 seconds. A "restart to update" prompt that vanishes would frustrate users who weren't looking. The
stable `id` means re-checking doesn't create duplicate toasts: the existing one is updated in place.

**Decision**: Interval re-creation on setting change instead of dynamic delay. **Why**: `setInterval` doesn't support
changing the delay after creation. When the user changes `advanced.updateCheckInterval`, the old interval is cleared and
a new one is created. This is simpler than a recursive `setTimeout` chain and the edge case of one extra tick at the old
interval is acceptable.

## Key patterns and gotchas

- `.svelte.ts` extension is required because `$state` can only live in `.svelte` or `.svelte.ts` files.
- The update manifest endpoint is hardcoded in the Rust backend (`https://getcmdr.com/latest.json`), not in TypeScript.
- The `check_for_update` command returns `None` when `CI` env var is set, so there are no network calls in CI.
- No retry or backoff on error; the next interval fires a fresh attempt.
- The catch in `checkForUpdates()` logs at `warn`, not `error`, so transient network failures during the periodic
  background check don't trip the auto error reporter (Flow B). The Settings UI still surfaces the message via
  `updateState.error`. See `apps/desktop/src-tauri/src/error_reporter/DETAILS.md` § convention.
- Default interval: 60 minutes. Configurable in settings from 5 minutes to 24 hours.
- Unit tests in `updater.test.ts` cover the gating logic via the pure `shouldShowUpdateToast` predicate plus the
  `notifyOnboardingComplete` and `setOnboardingShowing` triggers. The download-and-install path is still untested; it
  has hard Tauri/network dependencies.
- Cleanup is mandatory: the return value of `startUpdateChecker()` must be called in `onDestroy`.

## Dependencies

- `@tauri-apps/api/core`: `invoke()` for calling custom Tauri commands (macOS path)
- `@tauri-apps/plugin-updater`: `check()`, `downloadAndInstall()` (non-macOS path, dynamically imported)
- `@tauri-apps/plugin-process`: `relaunch()`
- `@tauri-apps/api/app`: `getVersion()`
- `$lib/settings/settings-store`: `getSetting`, `onSpecificSettingChange`
- `$lib/settings-store`: `loadSettings`, `saveSettings` (for the `isOnboarded` flag)
- `$lib/logging/logger`: `getAppLogger` (logs via unified LogTape bridge)
