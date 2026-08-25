# Updates module — details

Read this before any non-trivial work here: editing, planning, reorganizing, or advising. `CLAUDE.md` is the
always-loaded must-knows; this is the depth.

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

## Re-checking while staged

`ready` means the new build is already synced INTO the bundle (macOS) or installed by the plugin (elsewhere), and only a
restart is missing. That state can last as long as the session does, so the poll keeps running through it.

What makes that safe is `supersedesStagedUpdate(offered, staged)`: the update server compares against the version the
process is RUNNING, not the one staged, so a check made while `0.29.0` waits for a restart keeps offering `0.29.0`
forever. Only a strictly newer release passes the predicate and reaches the download; an equal or older offer takes
`keepStagedUpdate()`, which touches nothing but the nudge clock. The bundle is therefore never rewritten with bytes
identical to the ones in it, which is the clobber hazard the old blanket `ready` guard was defending against.

The other three statuses still return early. `checking` / `downloading` / `installing` each own an operation in flight,
and a second tick landing on one would race the same fetch, temp file, or bundle sync.

Two consequences worth knowing:

- **A staged build survives a failed re-check.** The download writes to a temp dir, so a check or download failure
  leaves the staged bytes untouched: `finishCheckWithFailure` puts the state machine back on `ready` with the staged
  version, logs, and says nothing to the user. They already have something worth restarting for.
- **A failed re-INSTALL is the one case the state can flatter.** `install_update` syncs into the live bundle, so a
  failure partway leaves a mixed bundle that we report as `ready` on the previously staged version. That hazard is the
  first install's too, and the alternative (never re-staging) means shipping people a build weeks out of date.

**The restart prompt re-raises itself.** `showUpdateToast()` stamps `lastRestartToastAt`, and every check that finds a
staged update calls `renudgeRestartIfDue()`, which re-adds the toast once `RESTART_NUDGE_INTERVAL_MS` (24 h) has passed.
Staging a NEWER build clears the stamp so it prompts immediately. Before this, `showUpdateToast()` was reachable only
from the download-complete branch and the two onboarding hooks, so one click of "Later" removed the last prompt for the
rest of the session; the measured cost was installs 25-38 days stale with a newer version already downloaded, and one
that restarted from 0.28.0 onto the staged 0.29.0 the day after 0.33.0 shipped. A day is the slowest cadence that still
fixes it, and since the toast is persistent, a user who leaves it on screen is never re-prompted.

When `status` becomes `'ready'`, the updater funnels through `showUpdateToast()`, which consults the pure, unit-tested
`shouldShowUpdateToast({ onboarded, onboardingShowing, status })` and only fires
`addToast(UpdateToastContent, { id: 'update', dismissal: 'persistent' })` when all three hold.
`UpdateToastContent.svelte` renders the body, calls `relaunch()` from `@tauri-apps/plugin-process` for the restart
action, and dismisses via `dismissToast('update')` for "Later". There's no local dismissed flag; the toast
infrastructure manages dismissal.

## What a check reports

Every exit of `checkForUpdates()` fires one `update_check` event through `update-analytics.ts`. The catalog entry (the
prop vocabulary and why each value exists) is `src-tauri/src/analytics/DETAILS.md` § Starter event set; what belongs
here is where each one is raised:

- `finishCheckWithNoUpdate` → `up_to_date`
- `keepStagedUpdate` → `already_staged`, carrying the version sitting in the bundle
- `finishCheckWithStagedUpdate` → `staged`, carrying the version just written
- `finishCheckWithUnwritableBundle` → `blocked`, with the arrangement as `failure`
- `finishCheckWithFailure` → `failed`, with the phase as `failure`

`trigger` names the entry point instead, and comes in as `checkForUpdates()`'s required first parameter so the finishers
never have to guess:

- `startUpdateChecker()`'s immediate launch check → `startup`
- a `startPollLoop()` tick → `poll`
- `applyAutoCheckEnabled(true)`, the `updates.autoCheck` switch or the onboarding wizard's step 3 → `auto_check_on`
- `runMenuTriggeredCheck()`, the `app.checkForUpdates` command from the menu, the palette, or a shortcut → `command`
- the "Check for updates" button on Settings > Updates → `settings`

The phase comes off `updateState.status`, read BEFORE the finisher moves it: macOS runs the download and the install
inside one `try`, and the status is the typed record of which was in flight. ❌ Never ask the error message, here or
anywhere (`error-string-match`).

The event rides the analytics consent (`analytics.enabled`, default-on), NOT the crash/error-report consent. It carries
no URL, no bundle path, and no failure text.

## When the bundle can't be written

macOS only. Once a check finds a build worth installing, `runMacUpdateFlow` asks `update_write_blocker` whether this
install can write into its own `.app`. Two arrangements say no: App Translocation (Cmdr opened straight from where it
was downloaded) and a read-only volume (a `.dmg` still mounted). The classification and its rationale live next to the
code that does it, `src-tauri/src/updater/DETAILS.md` § A bundle that can't be written.

A blocker takes `finishCheckWithUnwritableBundle`, which skips the download and raises `MoveToApplicationsDialog`
through the `updateBlockerNotice` singleton. The manifest check itself keeps running on the poll: it's what keeps the
install counted as active on the dashboard, and it costs a few hundred bytes rather than ~63 MB.

Three rules shape the nudge:

- **A failed classification is not a blocker.** `readWriteBlocker()` swallows the IPC failure and answers `null`.
  Treating a hiccup as "can't update" would stop updates that would have worked; the worst a false negative costs is one
  doomed download.
- **Once per session.** The answer can't change until the user moves the app, so a modal per poll interval would be its
  own problem. `moveNudgeShown` latches; the next launch asks again.
- **It waits out onboarding rather than stacking on it.** A fresh download opened from `~/Downloads` is exactly what
  macOS translocates, so this population meets the nudge on their FIRST launch. `pendingMoveNudge` holds it, and
  `notifyOnboardingComplete` / `setOnboardingShowing(false)` flush it, the same way the restart toast is re-attempted.

**Cmdr does not move itself**, and the reason is cost, not risk. Every step of a self-move was measured on macOS 26.5.2
and each one works: `docs/notes/self-move-to-applications-2026-08-25.md`. Two results are worth carrying here because
they contradict what the code around them might suggest:

- **A move does NOT cost the user their FDA grant.** No TCC table has a path column, and the requirement stored in
  Cmdr's own FDA row is bundle id plus Developer ID team, which a code requirement can satisfy from anywhere. A moved
  bundle reuses its row rather than growing a second one.
- **Copying the bundle is not enough.** The copy inherits `com.apple.quarantine`, and a quarantined bundle is
  translocated again even from `/Applications`, so the move would buy nothing until the xattr is stripped.

What holds the feature back is what it costs: a detached relaunch helper, an already-installed-in-`/Applications`
branch, copy in ten locales, a capture run for the new dialog state, and an assembled flow that can only be exercised
against a notarized build. The failure modes themselves are handled by ordering: copy, verify, dequarantine, relaunch,
and only then trash the original, so a helper dying at any point leaves two copies rather than none.

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
  and `setOnboardingShowing` triggers, and the staged re-check matrix through the mocked plugin flow (same build, newer
  build, failed check, failed download, in-flight statuses, the nudge cadence under fake timers). The macOS
  download-and-install path stays untested (hard Tauri/network deps).
- Version ordering comes from `compareVersions` (`$lib/utils/version.ts`), shared with `$lib/whats-new`. Don't re-roll
  it here: two comparators that disagree would let the updater call a release newer while What's New calls it older.
- The `warn`-not-`error` logging convention is documented in `src-tauri/src/error_reporter/DETAILS.md` § convention.

## Dependencies

- `@tauri-apps/api/core` `invoke()` (macOS custom commands).
- `@tauri-apps/plugin-updater` `check()` / `downloadAndInstall()` (non-macOS, dynamically imported).
- `@tauri-apps/plugin-process` `relaunch()`; `@tauri-apps/api/app` `getVersion()`.
- `$lib/settings/settings-store` (`getSetting`, `setSetting`, `forceSave`, `onSpecificSettingChange`; `forceSave` is
  what makes the `onboarding.completed` write survive an immediate quit); `$lib/logging/logger` (`getAppLogger`).

## i18n

Updates copy lives in the `updates.*` catalog (`$lib/intl/messages/en/updates.json`), resolved via `tString()` / `t()`;
`cmdr/no-raw-user-facing-string` is enforced on `lib/updates/`. `formatUpdateStatus()` (`update-status-text.ts`) returns
catalog-resolved strings with `{next}` / `{prev}` / `{version}` interpolation per status; it's a plain `.ts` so the
`t()` calls are snapshots (the component re-derives reactively, which is correct for this transient status). Runtime
rules: [`$lib/intl/CLAUDE.md`](../intl/CLAUDE.md).
