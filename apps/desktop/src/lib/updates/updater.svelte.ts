import { checkForUpdate, downloadUpdate, installUpdate } from '$lib/tauri-commands'
import { getVersion } from '@tauri-apps/api/app'
import { forceSave, getSetting, onSpecificSettingChange, setSetting } from '$lib/settings/settings-store'
import { getAppLogger } from '$lib/logging/logger'
import { pluralize } from '$lib/utils/pluralize'
import { compareVersions } from '$lib/utils/version'
import UpdateToastContent from './UpdateToastContent.svelte'
import UpdateCheckToastContent from './UpdateCheckToastContent.svelte'
import { addToast, dismissToast } from '$lib/ui/toast'
import { isMacOS } from '$lib/shortcuts/key-capture'
// `updateState` lives in its own module to avoid an import cycle: toast components read it directly,
// and this module also imports those toast components. Re-exported here so existing consumers
// (Settings section, command-dispatch, tests) keep using the old import path.
import { updateState, type UpdateInfo, type UpdateState } from './update-state.svelte'
export { updateState }
export type { UpdateState }

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
 * How long a staged-but-unapplied update stays quiet after the last restart prompt before that
 * prompt comes back.
 *
 * "Later" used to be permanent: the toast was reachable only from the download-complete branch,
 * so one dismissal took away the last prompt for the rest of the session, and installs sat 25-38
 * days on a version they had already downloaded past. A day is the slowest cadence that still
 * fixes that, and the toast is persistent, so someone who simply leaves it up is never
 * re-prompted at all.
 */
export const RESTART_NUDGE_INTERVAL_MS = 24 * 60 * 60 * 1000

/** When the restart toast last actually rendered, driving the re-nudge cadence. `null` = never. */
let lastRestartToastAt: number | null = null

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
  lastRestartToastAt = Date.now()
}

/**
 * Bring the restart prompt back when a staged update has gone unapplied for a whole nudge
 * interval. Driven from the check loop, so it rides the poll cadence the user already chose
 * rather than a timer of its own. `addToast` dedupes by id, so a toast the user never dismissed
 * is refreshed in place instead of stacking.
 */
function renudgeRestartIfDue(): void {
  if (lastRestartToastAt !== null && Date.now() - lastRestartToastAt < RESTART_NUDGE_INTERVAL_MS) {
    return
  }
  showUpdateToast()
}

/**
 * Mark onboarding as complete. Persists the flag and, if an update is already ready, shows the toast.
 * Called by the parent route once FDA onboarding finishes (either Allow or Deny path) or for users
 * who already had FDA granted before this flag existed.
 */
export async function notifyOnboardingComplete(): Promise<void> {
  onboarded = true
  setSetting('onboarding.completed', true)
  if (!(await forceSave())) {
    log.warn('Could not persist onboarding.completed=true; onboarding may re-run on next launch')
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

/**
 * Statuses that own the state machine for as long as they last. A tick landing on one of these
 * would race the fetch, the download, or the bundle sync already under way, so it turns around.
 *
 * `ready` is deliberately absent: a build already synced into the bundle is finished work, and
 * blocking the poll on it is what let an install sit for weeks on a version newer releases had
 * long since passed. Re-checking from `ready` is safe because `supersedesStagedUpdate` decides
 * whether anything gets written; see `DETAILS.md` § Re-checking while staged.
 */
const IN_FLIGHT_STATUSES: readonly UpdateState['status'][] = ['checking', 'downloading', 'installing']

/**
 * The version already synced into the bundle and waiting for a restart, or `null` when nothing is
 * staged. Every branch of a re-check reads this: a staged install keeps its state, and its bytes,
 * unless the server offers something strictly newer.
 */
function stagedVersion(): string | null {
  return updateState.status === 'ready' ? (updateState.update?.version ?? null) : null
}

/**
 * Whether an offered build is worth writing over what's already staged. Pure, so the one rule that
 * keeps a re-check from clobbering a pending update is unit-testable on its own.
 *
 * The server compares against the version we're RUNNING, not the one we staged, so a check made
 * while `0.29.0` waits for a restart keeps offering `0.29.0`. Re-syncing that would rewrite the
 * bundle with bytes identical to the ones in it, for nothing; only a genuinely newer release earns
 * another install.
 */
export function supersedesStagedUpdate(offered: string, staged: string | null): boolean {
  return staged === null || compareVersions(offered, staged) > 0
}

export async function checkForUpdates(): Promise<void> {
  if (IN_FLIGHT_STATUSES.includes(updateState.status)) {
    return // Don't interrupt an ongoing check, download, or install
  }

  const staged = stagedVersion()
  const currentVersion = await getVersion()

  // A re-check made on top of a staged update must not disturb the state machine on its way
  // through: Settings would flash "Checking…" over a standing "restart to apply", and a failure
  // partway would land on `idle` while a perfectly good build sits in the bundle.
  if (staged === null) {
    updateState.previousVersion = currentVersion
    updateState.nextVersion = null
    updateState.status = 'checking'
    updateState.error = null
  }

  log.debug('Checking for updates (current: v{version})...', { version: currentVersion })

  // Platform branches diverge significantly: macOS runs three custom commands (split download +
  // install phases, preserves TCC), non-macOS uses the Tauri plugin's fused `downloadAndInstall`.
  // The two-phase error handling (warn on check, error on download/install) lives inside each.
  if (isMacOS()) {
    await runMacUpdateFlow(currentVersion, staged)
  } else {
    await runPluginUpdateFlow(currentVersion, staged)
  }
}

/**
 * macOS path: custom updater that preserves TCC/Full Disk Access permissions by syncing files
 * into the existing `.app` bundle. Three Tauri commands; download and install are distinct
 * phases so the UI can show separate `downloading` and `installing` states.
 */
async function runMacUpdateFlow(currentVersion: string, staged: string | null): Promise<void> {
  let update: UpdateInfo | null
  try {
    update = await checkForUpdate()
  } catch (error) {
    finishCheckWithFailure(error, 'check', staged)
    return
  }

  if (update === null) {
    finishCheckWithNoUpdate(currentVersion, staged)
    return
  }

  if (!supersedesStagedUpdate(update.version, staged)) {
    keepStagedUpdate(update.version)
    return
  }

  log.info('Update available: v{current} -> v{next}', { current: currentVersion, next: update.version })
  updateState.nextVersion = update.version
  updateState.status = 'downloading'

  try {
    await downloadUpdate(update.url, update.signature)
    updateState.status = 'installing'
    await installUpdate()
  } catch (error) {
    finishCheckWithFailure(error, 'download-install', staged)
    return
  }

  finishCheckWithStagedUpdate(update)
}

/**
 * Non-macOS path: Tauri updater plugin. `downloadAndInstall()` is fused so we stay in
 * `downloading` throughout the second phase (no separate `installing` state).
 */
async function runPluginUpdateFlow(currentVersion: string, staged: string | null): Promise<void> {
  let update: Awaited<ReturnType<typeof import('@tauri-apps/plugin-updater').check>>
  try {
    const { check } = await import('@tauri-apps/plugin-updater')
    update = await check()
  } catch (error) {
    finishCheckWithFailure(error, 'check', staged)
    return
  }

  if (!update) {
    finishCheckWithNoUpdate(currentVersion, staged)
    return
  }

  if (!supersedesStagedUpdate(update.version, staged)) {
    keepStagedUpdate(update.version)
    return
  }

  log.info('Update available: v{current} -> v{next}', { current: currentVersion, next: update.version })
  updateState.nextVersion = update.version
  updateState.status = 'downloading'

  try {
    await update.downloadAndInstall()
  } catch (error) {
    finishCheckWithFailure(error, 'download-install', staged)
    return
  }

  finishCheckWithStagedUpdate({ version: update.version, url: '', signature: '' })
}

/**
 * A build is now synced into the bundle and only a restart away. A newer one earns a fresh prompt
 * even when the previous version's was dismissed, so the nudge clock resets here.
 */
function finishCheckWithStagedUpdate(update: UpdateInfo): void {
  log.info('v{version} installed, restart to apply', { version: update.version })
  updateState.status = 'ready'
  updateState.update = update
  updateState.nextVersion = update.version
  updateState.error = null
  lastRestartToastAt = null
  showUpdateToast()
}

/**
 * A check that ran while a build was already staged, and found nothing that beats it. The state
 * machine stays exactly where it was: moving it would either rewrite the bundle with the bytes
 * already in it, or make Settings claim the app is up to date while a restart is still pending.
 * All that's left is keeping the restart prompt reachable.
 */
function keepStagedUpdate(staged: string): void {
  log.debug('v{version} is still the newest build staged for restart', { version: staged })
  renudgeRestartIfDue()
}

function finishCheckWithNoUpdate(currentVersion: string, staged: string | null): void {
  if (staged !== null) {
    keepStagedUpdate(staged)
    return
  }
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
 *
 * `staged` is the version already synced into the bundle, if any. A build waiting for a restart
 * outlives a failed attempt at a newer one: the download writes to a temp dir, so a failure there
 * leaves the staged bytes untouched, and the user still has something worth restarting for. So the
 * state machine returns to `ready` and no message reaches them; the failure is in the log and in
 * the `update_check` event instead.
 */
function finishCheckWithFailure(error: unknown, phase: 'check' | 'download-install', staged: string | null): void {
  const message = error instanceof Error ? error.message : String(error)
  if (phase === 'check') {
    log.warn('Check failed: {error}', { error: message })
  } else {
    log.error('Download/install failed: {error}', { error: message })
  }

  if (staged !== null) {
    updateState.status = 'ready'
    updateState.nextVersion = staged
    renudgeRestartIfDue()
    return
  }

  updateState.status = 'idle'
  updateState.nextVersion = null
  updateState.error = message
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

  // Seed the onboarded flag from settings so returning users aren't gated. Settings
  // are initialized before this runs (`(main)/+layout.svelte` starts the checker after
  // `settingsReady`), so this is a synchronous read.
  onboarded = getSetting('onboarding.completed')

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
  lastRestartToastAt = null
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
