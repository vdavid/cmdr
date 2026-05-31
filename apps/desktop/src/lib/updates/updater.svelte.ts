import { commands } from '$lib/ipc/bindings'
import { getVersion } from '@tauri-apps/api/app'
import { getSetting, onSpecificSettingChange } from '$lib/settings/settings-store'
import { getAppLogger } from '$lib/logging/logger'
import { pluralize } from '$lib/utils/pluralize'
import UpdateToastContent from './UpdateToastContent.svelte'
import UpdateCheckToastContent from './UpdateCheckToastContent.svelte'
import { addToast, dismissToast } from '$lib/ui/toast'
import { loadSettings, saveSettings } from '$lib/settings-store'
import { isMacOS } from '$lib/shortcuts/key-capture'
// `updateState` lives in its own module to avoid an import cycle: toast components read it directly,
// and this module also imports those toast components. Re-exported here so existing consumers
// (Settings section, command-dispatch, tests) keep using the old import path.
import { updateState, type UpdateInfo, type UpdateState } from './update-state.svelte'
export { updateState }
export type { UpdateInfo, UpdateState }

const log = getAppLogger('updater')

/** Gets the update check interval from settings (in milliseconds) */
function getCheckIntervalMs(): number {
  return getSetting('advanced.updateCheckInterval')
}

// Module-level gating flags. The toast for "update ready, restart now" must NOT show during
// onboarding (the user just downloaded the app, so they'd be confused) nor while any of the
// onboarding wizard's steps are on screen. `onboardingShowing` covers the legacy FDA modal AND
// the new wizard's full lifecycle (all three steps); the renamed setter reflects that.
let onboarded = $state(false)
let onboardingShowing = $state(false)

/**
 * Pure predicate for whether the "update ready" toast should show right now.
 * Exported for unit testing the truth table.
 */
export function shouldShowUpdateToast(args: {
  onboarded: boolean
  onboardingShowing: boolean
  status: UpdateState['status']
}): boolean {
  return args.onboarded && !args.onboardingShowing && args.status === 'ready'
}

/**
 * Show the update-ready toast, but only if gating allows. Called from the download-complete branches
 * and from the onboarding/FDA hooks below. When suppressed, we leave `updateState.status === 'ready'`
 * so the download stays applied; the toast just doesn't render until the gate opens.
 */
function showUpdateToast(): void {
  if (!shouldShowUpdateToast({ onboarded, onboardingShowing, status: updateState.status })) {
    return
  }
  addToast(UpdateToastContent, { id: 'update', dismissal: 'persistent' })
}

/**
 * Mark onboarding as complete. Persists the flag and, if an update is already ready, shows the toast.
 * Called by the parent route once FDA onboarding finishes (either Allow or Deny path) or for users
 * who already had FDA granted before this flag existed.
 */
export async function notifyOnboardingComplete(): Promise<void> {
  onboarded = true
  if (!(await saveSettings({ isOnboarded: true }))) {
    log.warn('Could not persist isOnboarded=true; onboarding may re-run on next launch')
  }
  showUpdateToast()
}

/**
 * Track whether the onboarding wizard (or legacy FDA modal) is on screen. While it's up, suppress
 * the update toast so we don't pile two modals on top of each other. When it closes and an update
 * is ready, re-attempt the toast. The flag spans all three wizard steps, not just step 1: the
 * user is still onboarding while picking an AI provider or flipping optional toggles, and the
 * "restart to update" toast would be just as confusing landing on step 2 as on step 1.
 */
export function setOnboardingShowing(value: boolean): void {
  const wasShowing = onboardingShowing
  onboardingShowing = value
  if (wasShowing && !value) {
    showUpdateToast()
  }
}

export async function checkForUpdates(): Promise<void> {
  if (updateState.status === 'downloading' || updateState.status === 'installing' || updateState.status === 'ready') {
    return // Don't interrupt ongoing download/install or ready state
  }

  const currentVersion = await getVersion()
  updateState.previousVersion = currentVersion
  updateState.nextVersion = null
  updateState.status = 'checking'
  updateState.error = null

  log.debug('Checking for updates (current: v{version})...', { version: currentVersion })

  // Platform branches diverge significantly: macOS runs three custom commands (split download +
  // install phases, preserves TCC), non-macOS uses the Tauri plugin's fused `downloadAndInstall`.
  // The two-phase error handling (warn on check, error on download/install) lives inside each.
  if (isMacOS()) {
    await runMacUpdateFlow(currentVersion)
  } else {
    await runPluginUpdateFlow(currentVersion)
  }
}

/**
 * macOS path: custom updater that preserves TCC/Full Disk Access permissions by syncing files
 * into the existing `.app` bundle. Three Tauri commands; download and install are distinct
 * phases so the UI can show separate `downloading` and `installing` states.
 */
async function runMacUpdateFlow(currentVersion: string): Promise<void> {
  let update: UpdateInfo | null
  try {
    const checkRes = await commands.checkForUpdate()
    if (checkRes.status === 'error') throw new Error(checkRes.error)
    update = checkRes.data
  } catch (error) {
    finishCheckWithFailure(error, 'check')
    return
  }

  if (update === null) {
    finishCheckWithNoUpdate(currentVersion)
    return
  }

  log.info('Update available: v{current} -> v{next}', { current: currentVersion, next: update.version })
  updateState.nextVersion = update.version
  updateState.status = 'downloading'

  try {
    const dlRes = await commands.downloadUpdate(update.url, update.signature)
    if (dlRes.status === 'error') throw new Error(dlRes.error)
    updateState.status = 'installing'
    const installRes = await commands.installUpdate()
    if (installRes.status === 'error') throw new Error(installRes.error)
  } catch (error) {
    finishCheckWithFailure(error, 'download-install')
    return
  }

  log.info('v{version} installed, restart to apply', { version: update.version })
  updateState.status = 'ready'
  updateState.update = update
  showUpdateToast()
}

/**
 * Non-macOS path: Tauri updater plugin. `downloadAndInstall()` is fused so we stay in
 * `downloading` throughout the second phase (no separate `installing` state).
 */
async function runPluginUpdateFlow(currentVersion: string): Promise<void> {
  let update: Awaited<ReturnType<typeof import('@tauri-apps/plugin-updater').check>>
  try {
    const { check } = await import('@tauri-apps/plugin-updater')
    update = await check()
  } catch (error) {
    finishCheckWithFailure(error, 'check')
    return
  }

  if (!update) {
    finishCheckWithNoUpdate(currentVersion)
    return
  }

  log.info('Update available: v{current} -> v{next}', { current: currentVersion, next: update.version })
  updateState.nextVersion = update.version
  updateState.status = 'downloading'

  try {
    await update.downloadAndInstall()
  } catch (error) {
    finishCheckWithFailure(error, 'download-install')
    return
  }

  log.info('v{version} installed, restart to apply', { version: update.version })
  updateState.status = 'ready'
  updateState.update = { version: update.version, url: '', signature: '' }
  showUpdateToast()
}

function finishCheckWithNoUpdate(currentVersion: string): void {
  log.debug('v{version} is up to date', { version: currentVersion })
  updateState.status = 'idle'
  updateState.nextVersion = null
}

/**
 * Reset state and log the failure at the right level for the phase.
 *
 * - `'check'` failures (network, DNS, bad manifest) are transient and expected on the periodic
 *   background tick; log at warn so they don't trip the auto error reporter on a momentary blip.
 * - `'download-install'` failures (signature mismatch, FS errors, partial writes) reach a code
 *   path the user already opted into, so log at error so they DO trip auto-report. The Settings
 *   UI surfaces both via `updateState.error` regardless of log level.
 *
 * See `apps/desktop/src-tauri/src/error_reporter/CLAUDE.md` § convention.
 */
function finishCheckWithFailure(error: unknown, phase: 'check' | 'download-install'): void {
  updateState.status = 'idle'
  updateState.nextVersion = null
  updateState.error = error instanceof Error ? error.message : String(error)
  if (phase === 'check') {
    log.warn('Check failed: {error}', { error: updateState.error })
  } else {
    log.error('Download/install failed: {error}', { error: updateState.error })
  }
}

/**
 * Menu-triggered "Check for updates" flow: render a status toast that mirrors `updateState`,
 * run `checkForUpdates()`, and dismiss the status toast once we hit `ready` so it doesn't
 * overlap with the persistent "Restart to update" toast (id `'update'`).
 */
export async function runMenuTriggeredCheck(): Promise<void> {
  addToast(UpdateCheckToastContent, { id: 'update-check', timeoutMs: 10000 })
  try {
    await checkForUpdates()
  } finally {
    if (updateState.status === 'ready') {
      dismissToast('update-check')
    }
  }
}

/**
 * Module-scoped interval handle for the auto-check poll loop. Lifted to module scope so
 * `applyAutoCheckEnabled()` can stop and restart the loop in response to live
 * `updates.autoCheck` flips, without restarting the whole checker. `undefined` means
 * "no poll loop active right now" (either auto-check is off, or the checker hasn't
 * started yet).
 */
let pollIntervalId: ReturnType<typeof setInterval> | undefined

function startPollLoop(): void {
  if (pollIntervalId !== undefined) return
  pollIntervalId = setInterval(() => {
    void checkForUpdates()
  }, getCheckIntervalMs())
}

function stopPollLoop(): void {
  if (pollIntervalId === undefined) return
  clearInterval(pollIntervalId)
  pollIntervalId = undefined
}

/**
 * Live-apply hook for `updates.autoCheck`. Off cancels the background poll loop in
 * place (the user keeps whatever update state we last computed; we just stop asking).
 * On re-starts the loop and fires one immediate check, so users who turn the toggle
 * back on don't have to wait an interval for the first tick. Called from
 * `settings-applier.ts`'s `passthroughBackendHandlers` lookup whenever the setting
 * flips, including from the onboarding wizard's step 3.
 *
 * Safe to call before `startUpdateChecker()` has run (only matters in tests today,
 * but cheap insurance): `startPollLoop()` is idempotent, and `checkForUpdates()`
 * tolerates an early call (it just transitions through `checking` → `idle`).
 */
export function applyAutoCheckEnabled(enabled: boolean): void {
  if (enabled) {
    startPollLoop()
    void checkForUpdates()
  } else {
    stopPollLoop()
  }
}

export function startUpdateChecker(): () => void {
  log.debug('Started')

  // Seed onboarded flag from persisted settings so returning users aren't gated.
  void loadSettings().then((settings) => {
    onboarded = settings.isOnboarded
    // Edge case: an interval tick fired and reached 'ready' before this resolved.
    showUpdateToast()
  })

  const autoCheckEnabled = getSetting('updates.autoCheck')

  if (autoCheckEnabled) {
    // Check immediately on start
    void checkForUpdates()
    startPollLoop()
  } else {
    log.debug('Auto-check disabled; skipping initial check and poll loop')
  }

  // Re-create interval when the cadence changes (only if the loop is running).
  const unsubscribeInterval = onSpecificSettingChange('advanced.updateCheckInterval', () => {
    if (pollIntervalId === undefined) return
    stopPollLoop()
    const newInterval = getCheckIntervalMs()
    const minutes = newInterval / 60000
    log.info('Interval changed to {minutes} {minutesNoun}', {
      minutes,
      minutesNoun: pluralize(minutes, 'minute'),
    })
    startPollLoop()
  })

  // Live-apply for `updates.autoCheck` lives in `settings-applier.ts`'s
  // `passthroughBackendHandlers`, calling `applyAutoCheckEnabled()` above. One source
  // of truth keeps the wizard's step 3 toggle, the Settings UI switch, and any future
  // MCP/IPC writer all going through the same hook.

  // Return cleanup function
  return () => {
    stopPollLoop()
    unsubscribeInterval()
  }
}

/**
 * Test-only hook: reset module-level gating flags. Production code should never call this.
 */
export function _resetUpdaterStateForTest(): void {
  onboarded = false
  onboardingShowing = false
  updateState.status = 'idle'
  updateState.update = null
  updateState.error = null
  updateState.previousVersion = null
  updateState.nextVersion = null
}

/**
 * Test-only hook: directly set the update state's status. Production code should never call this.
 */
export function _setUpdateStatusForTest(status: UpdateState['status']): void {
  updateState.status = status
}
