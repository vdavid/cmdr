# Updates module — details

Read this before any non-trivial work here: editing, planning, reorganizing, or advising. `CLAUDE.md` is the always-loaded must-knows; this is the depth.

## Lifecycle

`startUpdateChecker()` runs once from `+layout.svelte`:

1. If `updates.autoCheck` is `true` (default), fires an immediate `checkForUpdates()` and schedules a `setInterval` from
   `advanced.updateCheckInterval`. If `false`, skips both (opted out of the background poll).
2. Listens for `advanced.updateCheckInterval` changes; clears and re-creates the interval on change (only if the loop is
   running). `setInterval` can't change its delay after creation, so re-creating is simpler than a recursive
   `setTimeout` chain; one extra tick at the old interval is acceptable.
3. Returns a cleanup function that `+layout.svelte` calls in `onDestroy`.

`applyAutoCheckEnabled(enabled)` lets the live-apply hook in `settings-applier.ts`'s `passthroughBackendHandlers` flip
the poll loop in place when the user toggles `updates.autoCheck` (Settings switch, onboarding step 3, or any MCP/IPC
writer). On enable it fires one immediate check; on disable it stops the loop but leaves `updateState.status` alone so
an in-flight update isn't lost.

## State machine

`checkForUpdates()` transitions `idle → checking → downloading → installing → ready` (macOS) or
`idle → checking → downloading → ready` (non-macOS). If an update is found it downloads and installs automatically with
no confirmation; the user is only asked at `ready` whether to restart now or later.

```
idle ──invoke──► checking ──update found──► downloading ──► installing ──► ready
  ▲                  │                                      (macOS only)
  └──────error/no update
```

`updateState` carries `status`, `error`, `previousVersion` (snapshot of `getVersion()` taken when entering `checking`),
and `nextVersion` (set when an update is found). Settings > Updates and `UpdateCheckToastContent.svelte` both read the
singleton and format via `formatUpdateStatus()`.

The macOS path runs `download_update` and `install_update` as two commands (distinct `downloading` / `installing`
phases); the non-macOS path uses the plugin's fused `downloadAndInstall()` (stays in `downloading`). The Rust backend at
`src-tauri/src/updater/` syncs files into the existing `.app` bundle, preserving the inode and TCC/Full Disk Access
permissions.

When `status` becomes `'ready'`, the updater funnels through `showUpdateToast()`, which consults the pure, unit-tested
`shouldShowUpdateToast({ onboarded, onboardingShowing, status })` and only fires
`addToast(UpdateToastContent, { id: 'update', dismissal: 'persistent' })` when all three hold.
`UpdateToastContent.svelte` renders the body, calls `relaunch()` from `@tauri-apps/plugin-process` for the restart
action, and dismisses via `dismissToast('update')` for "Later". There's no local dismissed flag; the toast
infrastructure manages dismissal.

## Menu-triggered "Check for updates"

- **Settings > Updates**: a "Check for updates" button at the top of the section, disabled while
  `updateState.status !== 'idle'`, with the status string from `formatUpdateStatus(updateState)` below. The error case
  renders a "Send error report" link calling `openErrorReportDialog("Update check failed: ${error}")`.
- **Cmdr menu > Check for updates…**: dispatched as `app.checkForUpdates`. The handler calls `runMenuTriggeredCheck()`,
  which fires `addToast(UpdateCheckToastContent, { id: 'update-check', timeoutMs: 10000 })` then awaits
  `checkForUpdates()`. `addToast` deduplicates by id, so the toast updates in place as the phase changes. When `status`
  flips to `ready` the helper dismisses `'update-check'` so it doesn't overlap the persistent restart toast.

The native menu item sits in the Cmdr submenu (macOS) right after "Enter license key…", wired through
`menu_id_to_command` / `command_id_to_menu_id` in `src-tauri/src/menu/mod.rs`, SF Symbol `arrow.down.circle` mapped in
`macos.rs`. On Linux the same command appears at the bottom of the Edit submenu after the license item.

## Onboarding gating

The toast must not show during first-launch onboarding (telling a fresh download to "restart to update" is confusing)
nor while the wizard's later steps are on screen (would stack two prompts). Two module `$state` flags drive this:

- `onboarded`: seeded from `loadSettings().isOnboarded` at `startUpdateChecker()`, flipped by
  `notifyOnboardingComplete()` (which also persists `isOnboarded: true`).
- `onboardingShowing`: flipped by `setOnboardingShowing(value)` from `routes/(main)/+page.svelte` across the whole
  wizard lifecycle (FDA, AI, optional steps).

When a gate opens, the helper re-attempts the toast; if the download finished during onboarding, `status` stays
`'ready'` and the toast shows on unblock. Nothing is lost.

## Key decisions

- **Auto-download without confirmation; only prompt for restart.** Updates are small (~63 MB); a "download now?" prompt
  adds a decision most users always accept. Restart is the only destructive action, so that's the only prompt.
- **Persistent toast with stable `id: 'update'`.** Transient toasts auto-dismiss after 4 s; a vanishing "restart to
  update" prompt would frustrate. The stable id means re-checking updates the existing toast in place rather than
  duplicating.

## Patterns and gotchas

- No retry or backoff on error; the next interval fires a fresh attempt.
- Default interval 60 minutes; configurable 5 minutes to 24 hours.
- Unit tests (`updater.test.ts`) cover the gating logic via `shouldShowUpdateToast` plus the `notifyOnboardingComplete`
  and `setOnboardingShowing` triggers. The download-and-install path stays untested (hard Tauri/network deps).
- The `warn`-not-`error` logging convention is documented in `src-tauri/src/error_reporter/DETAILS.md` § convention.

## Dependencies

- `@tauri-apps/api/core` `invoke()` (macOS custom commands).
- `@tauri-apps/plugin-updater` `check()` / `downloadAndInstall()` (non-macOS, dynamically imported).
- `@tauri-apps/plugin-process` `relaunch()`; `@tauri-apps/api/app` `getVersion()`.
- `$lib/settings/settings-store` (`getSetting`, `onSpecificSettingChange`); `$lib/settings-store` (`loadSettings`,
  `saveSettings` for `isOnboarded`); `$lib/logging/logger` (`getAppLogger`).
