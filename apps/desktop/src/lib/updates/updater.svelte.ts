import { invoke } from '@tauri-apps/api/core'
import { getVersion } from '@tauri-apps/api/app'
import { getSetting, onSpecificSettingChange } from '$lib/settings/settings-store'
import { getAppLogger } from '$lib/logging/logger'
import UpdateToastContent from './UpdateToastContent.svelte'
import { addToast } from '$lib/ui/toast'
import { loadSettings, saveSettings } from '$lib/settings-store'

const log = getAppLogger('updater')

const isMacOS = navigator.userAgent.includes('Macintosh')

/** Gets the update check interval from settings (in milliseconds) */
function getCheckIntervalMs(): number {
  return getSetting('advanced.updateCheckInterval')
}

/** Metadata returned by the `check_for_update` Tauri command */
interface UpdateInfo {
  version: string
  url: string
  signature: string
}

interface UpdateState {
  status: 'idle' | 'checking' | 'downloading' | 'ready'
  update: UpdateInfo | null
  error: string | null
}

const updateState = $state<UpdateState>({
  status: 'idle',
  update: null,
  error: null,
})

// Module-level gating flags. The toast for "update ready, restart now" must NOT show during onboarding
// (the user just downloaded the app — they'd be confused) nor while the FDA-revoked re-prompt is on screen.
let onboarded = $state(false)
let fdaPromptShowing = $state(false)

/**
 * Pure predicate for whether the "update ready" toast should show right now.
 * Exported for unit testing the truth table.
 */
export function shouldShowUpdateToast(args: {
  onboarded: boolean
  fdaPromptShowing: boolean
  status: UpdateState['status']
}): boolean {
  return args.onboarded && !args.fdaPromptShowing && args.status === 'ready'
}

/**
 * Show the update-ready toast, but only if gating allows. Called from the download-complete branches
 * and from the onboarding/FDA hooks below. When suppressed, we leave `updateState.status === 'ready'`
 * so the download stays applied — the toast just doesn't render until the gate opens.
 */
function showUpdateToast(): void {
  if (!shouldShowUpdateToast({ onboarded, fdaPromptShowing, status: updateState.status })) {
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
  await saveSettings({ isOnboarded: true })
  showUpdateToast()
}

/**
 * Track whether the FDA prompt is on screen. While it's up, suppress the update toast so we don't
 * pile two modals on top of each other. When it closes and an update is ready, re-attempt the toast.
 */
export function setFdaPromptShowing(value: boolean): void {
  const wasShowing = fdaPromptShowing
  fdaPromptShowing = value
  if (wasShowing && !value) {
    showUpdateToast()
  }
}

export async function checkForUpdates(): Promise<void> {
  if (updateState.status === 'downloading' || updateState.status === 'ready') {
    return // Don't interrupt ongoing download or ready state
  }

  updateState.status = 'checking'
  updateState.error = null

  try {
    const currentVersion = await getVersion()
    log.debug('Checking for updates (current: v{version})...', { version: currentVersion })

    if (isMacOS) {
      // macOS: custom updater preserves TCC/Full Disk Access permissions
      const update = await invoke<UpdateInfo | null>('check_for_update')

      if (update !== null) {
        log.info('Update available: v{current} -> v{next}', { current: currentVersion, next: update.version })
        updateState.status = 'downloading'
        await invoke('download_update', { url: update.url, signature: update.signature })
        await invoke('install_update')
        log.info('v{version} installed, restart to apply', { version: update.version })
        updateState.status = 'ready'
        updateState.update = update
        showUpdateToast()
      } else {
        log.debug('v{version} is up to date', { version: currentVersion })
        updateState.status = 'idle'
      }
    } else {
      // Non-macOS: delegate to Tauri updater plugin
      const { check } = await import('@tauri-apps/plugin-updater')
      const update = await check()

      if (update) {
        log.info('Update available: v{current} -> v{next}', { current: currentVersion, next: update.version })
        updateState.status = 'downloading'
        await update.downloadAndInstall()
        log.info('v{version} installed, restart to apply', { version: update.version })
        updateState.status = 'ready'
        updateState.update = { version: update.version, url: '', signature: '' }
        showUpdateToast()
      } else {
        log.debug('v{version} is up to date', { version: currentVersion })
        updateState.status = 'idle'
      }
    }
  } catch (error) {
    updateState.status = 'idle'
    updateState.error = error instanceof Error ? error.message : String(error)
    log.error('Check failed: {error}', { error: updateState.error })
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

  // Check immediately on start
  void checkForUpdates()

  // Check periodically using the interval from settings
  let intervalId = setInterval(() => {
    void checkForUpdates()
  }, getCheckIntervalMs())

  // Re-create interval when setting changes
  const unsubscribe = onSpecificSettingChange('advanced.updateCheckInterval', () => {
    clearInterval(intervalId)
    const newInterval = getCheckIntervalMs()
    log.info('Interval changed to {minutes} minutes', { minutes: newInterval / 60000 })
    intervalId = setInterval(() => {
      void checkForUpdates()
    }, newInterval)
  })

  // Return cleanup function
  return () => {
    clearInterval(intervalId)
    unsubscribe()
  }
}

/**
 * Test-only hook: reset module-level gating flags. Production code should never call this.
 */
export function _resetUpdaterStateForTest(): void {
  onboarded = false
  fdaPromptShowing = false
  updateState.status = 'idle'
  updateState.update = null
  updateState.error = null
}

/**
 * Test-only hook: directly set the update state's status. Production code should never call this.
 */
export function _setUpdateStatusForTest(status: UpdateState['status']): void {
  updateState.status = status
}
