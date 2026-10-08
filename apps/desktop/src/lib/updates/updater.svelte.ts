import {
  checkForUpdate,
  downloadUpdate,
  installUpdate,
  recordUpdateCheck,
  updateCheckDueIn,
  updateWriteBlocker,
} from '$lib/tauri-commands'
import type { BundleWriteBlocker } from '$lib/tauri-commands'
import { failureOf } from '$lib/ipc/typed-failure'
import { UpdateDownloadFailure } from './update-download-failure'
import { UpdateInstallFailure } from './update-install-failure'
import type { ServerRequestError, UpdateCheckOutcome } from '$lib/ipc/bindings'
import { getVersion } from '@tauri-apps/api/app'
import { forceSave, getSetting, setSetting } from '$lib/settings/settings-store'
import { getAppLogger } from '$lib/logging/logger'
import { LogOnceGate } from '$lib/logging/log-once'
import { serverRequestFailureOf, serverRequestLogLevel } from '$lib/error-messages/server-request'
import { compareVersions } from '$lib/utils/version'
import { blockerFailure, reportUpdateCheck, type UpdateCheckFailure, type UpdateCheckTrigger } from './update-analytics'
import UpdateToastContent from './UpdateToastContent.svelte'
import UpdateCheckToastContent from './UpdateCheckToastContent.svelte'
import { addToast, dismissToast } from '$lib/ui/toast'
import { isMacOS } from '$lib/shortcuts/key-capture'
// `updateState` lives in its own module to avoid an import cycle: toast components read it directly,
// and this module also imports those toast components. Re-exported here so existing consumers
// (Settings section, command-dispatch, tests) keep using the old import path.
import {
  updateBlockerNotice,
  updateState,
  type ManagedUpdateOutcome,
  type UpdateFailure,
  type UpdateInfo,
  type UpdateState,
} from './update-state.svelte'
export { updateBlockerNotice, updateState }
export type { UpdateState }

const log = getAppLogger('updater')

/**
 * A check that keeps failing the same way logs once, until a check gets an answer. An offline laptop would otherwise
 * write the same warn every poll tick, and a manifest this build can't read would auto-send an error report every interval.
 */
const checkFailureLog = new LogOnceGate()

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
 * The "move Cmdr to Applications" nudge is raised at most once per session: a modal reappearing
 * every poll interval would be its own problem, and the answer doesn't change until the user does
 * something about it. `pendingMoveNudge` holds one that arrived while onboarding owned the screen,
 * which is exactly the population this nudge is for (a download opened straight from `~/Downloads`
 * is what makes macOS translocate it), so it's remembered rather than dropped.
 */
let moveNudgeShown = false
let pendingMoveNudge: BundleWriteBlocker | null = null

function showMoveToApplicationsNudge(blocker: BundleWriteBlocker): void {
  if (moveNudgeShown) return
  if (!onboarded || onboardingShowing) {
    pendingMoveNudge = blocker
    return
  }
  moveNudgeShown = true
  pendingMoveNudge = null
  updateBlockerNotice.blocker = blocker
}

function flushPendingMoveNudge(): void {
  if (pendingMoveNudge !== null) {
    showMoveToApplicationsNudge(pendingMoveNudge)
  }
}

/** Closes the nudge. It doesn't come back this session; the next launch asks again. */
export function dismissMoveToApplicationsNudge(): void {
  updateBlockerNotice.blocker = null
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
  flushPendingMoveNudge()
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
    flushPendingMoveNudge()
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

/**
 * Runs one update check. `trigger` names the entry point that asked and rides the `update_check`
 * event; it has no default so a new caller has to decide what it reports rather than landing in
 * whichever bucket happened to be first.
 */
export async function checkForUpdates(trigger: UpdateCheckTrigger): Promise<void> {
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
    updateState.failure = null
    updateState.managed = null
  }

  log.debug('Checking for updates (current: v{version})...', { version: currentVersion })

  // Platform branches diverge significantly: macOS runs three custom commands (split download +
  // install phases, preserves TCC), non-macOS uses the Tauri plugin's fused `downloadAndInstall`.
  // Both hand their failures to `finishCheckWithFailure`, which picks the log level per phase.
  if (isMacOS()) {
    await runMacUpdateFlow(trigger, currentVersion, staged)
  } else {
    await runPluginUpdateFlow(trigger, currentVersion, staged)
  }
}

/**
 * macOS path: custom updater that preserves TCC/Full Disk Access permissions by syncing files
 * into the existing `.app` bundle. Three Tauri commands; download and install are distinct
 * phases so the UI can show separate `downloading` and `installing` states.
 */
async function runMacUpdateFlow(
  trigger: UpdateCheckTrigger,
  currentVersion: string,
  staged: string | null,
): Promise<void> {
  let outcome: UpdateCheckOutcome
  try {
    outcome = await checkForUpdate(trigger)
  } catch (error) {
    void recordUpdateCheck(false)
    finishCheckWithFailure(trigger, error, 'check', staged)
    return
  }
  // A managed answer counts as an answer too: the schedule then asks the backend once per interval rather than on every
  // wake, each of which would log the same refusal.
  void recordUpdateCheck(true)
  // The check got an answer, so the next breakage speaks.
  checkFailureLog.clear()

  switch (outcome.kind) {
    case 'upToDate':
      finishCheckWithNoUpdate(trigger, currentVersion, staged)
      return
    case 'updatesDisabledByPolicy':
    case 'heldByPolicy':
      finishCheckWithManagedOutcome(trigger, outcome, staged)
      return
    case 'automaticChecksDisabledByPolicy':
      finishRefusedAutomaticCheck(trigger, staged)
      return
    case 'available':
      break
  }
  const update: UpdateInfo = { version: outcome.version }

  if (!supersedesStagedUpdate(update.version, staged)) {
    keepStagedUpdate(trigger, update.version)
    return
  }

  const blocker = await readWriteBlocker()
  if (blocker !== null) {
    finishCheckWithUnwritableBundle(trigger, blocker, staged)
    return
  }

  log.info('Update available: v{current} -> v{next}', { current: currentVersion, next: update.version })
  updateState.nextVersion = update.version
  updateState.status = 'downloading'

  try {
    await downloadUpdate()
    updateState.status = 'installing'
    await installUpdate()
  } catch (error) {
    if (refusedByPolicy(error)) {
      finishDownloadInstallRefusedByPolicy(trigger, staged)
      return
    }
    finishCheckWithFailure(trigger, error, 'download-install', staged)
    return
  }

  finishCheckWithStagedUpdate(trigger, update)
}

/** Whether the backend refused the download or the install because a policy arrived after the check. */
function refusedByPolicy(error: unknown): boolean {
  return (
    failureOf(UpdateDownloadFailure, error)?.type === 'blockedByPolicy' ||
    failureOf(UpdateInstallFailure, error)?.type === 'blockedByPolicy'
  )
}

/**
 * The organization's policy answered the check: updates are off, or the newest release is past its ceiling. A terminal
 * phase, not a failure: nothing to log above info, no failure copy, and no toast of its own (a background check that
 * finds only a held release stays silent; Settings and a manual check's toast read the sentence off `managed`).
 *
 * A build already staged keeps its state: it's in the bundle and the next restart applies it whatever the policy says now.
 */
function finishCheckWithManagedOutcome(
  trigger: UpdateCheckTrigger,
  outcome: ManagedUpdateOutcome,
  staged: string | null,
): void {
  log.info('The organization’s policy answered the update check: {kind}', { kind: outcome.kind })
  reportUpdateCheck({
    trigger,
    outcome: outcome.kind === 'heldByPolicy' ? 'held_by_policy' : 'updates_disabled_by_policy',
    stagedVersion: staged,
  })
  if (staged !== null) {
    renudgeRestartIfDue()
    return
  }
  updateState.status = 'idle'
  updateState.nextVersion = null
  updateState.managed = outcome
}

/**
 * The backend refused a background check under the organization's `DisableAutomaticUpdateChecks`. That key locks
 * `updates.autoCheck` off, so the settings overlay keeps the loop from starting; this is the backstop for a loop that
 * started anyway (say, before the policy arrived), so it stops the loop. A check the person asks for still runs. Leaves
 * nothing on screen: nobody asked.
 */
function finishRefusedAutomaticCheck(trigger: UpdateCheckTrigger, staged: string | null): void {
  log.info('The organization turned automatic update checks off; stopping the background loop')
  reportUpdateCheck({ trigger, outcome: 'automatic_checks_disabled_by_policy', stagedVersion: staged })
  stopPollLoop()
  if (staged !== null) return
  updateState.status = 'idle'
  updateState.previousVersion = null
  updateState.nextVersion = null
}

/**
 * A policy that arrived between the check and the download (or the install) refused the version. Quiet: the next check
 * gets the organization's answer and says it. A staged build stays staged.
 */
function finishDownloadInstallRefusedByPolicy(trigger: UpdateCheckTrigger, staged: string | null): void {
  const failure: UpdateCheckFailure = updateState.status === 'installing' ? 'install' : 'download'
  log.info('The organization’s policy refused the update {phase}', { phase: failure })
  reportUpdateCheck({ trigger, outcome: 'blocked_by_policy', failure, stagedVersion: staged })
  if (staged !== null) {
    updateState.status = 'ready'
    updateState.nextVersion = staged
    return
  }
  updateState.status = 'idle'
  updateState.nextVersion = null
}

/**
 * Non-macOS path: Tauri updater plugin. `downloadAndInstall()` is fused so we stay in
 * `downloading` throughout the second phase (no separate `installing` state).
 */
async function runPluginUpdateFlow(
  trigger: UpdateCheckTrigger,
  currentVersion: string,
  staged: string | null,
): Promise<void> {
  let update: Awaited<ReturnType<typeof import('@tauri-apps/plugin-updater').check>>
  try {
    const { check } = await import('@tauri-apps/plugin-updater')
    update = await check()
  } catch (error) {
    void recordUpdateCheck(false)
    finishCheckWithFailure(trigger, error, 'check', staged)
    return
  }
  void recordUpdateCheck(true)
  // The check got an answer, so the next breakage speaks.
  checkFailureLog.clear()

  if (!update) {
    finishCheckWithNoUpdate(trigger, currentVersion, staged)
    return
  }

  if (!supersedesStagedUpdate(update.version, staged)) {
    keepStagedUpdate(trigger, update.version)
    return
  }

  log.info('Update available: v{current} -> v{next}', { current: currentVersion, next: update.version })
  updateState.nextVersion = update.version
  updateState.status = 'downloading'

  try {
    await update.downloadAndInstall()
  } catch (error) {
    finishCheckWithFailure(trigger, error, 'download-install', staged)
    return
  }

  finishCheckWithStagedUpdate(trigger, { version: update.version })
}

/**
 * Asks the backend whether this install can write its own bundle.
 *
 * A failure to ANSWER is not a blocker. The classification is a courtesy that saves a doomed
 * download; treating an IPC hiccup as "can't update" would stop updates that would have worked.
 * macOS-only, like the rest of `runMacUpdateFlow`: the Tauri plugin owns the install elsewhere.
 */
async function readWriteBlocker(): Promise<BundleWriteBlocker | null> {
  try {
    return await updateWriteBlocker()
  } catch (error) {
    log.warn("Couldn't classify where the bundle lives: {error}", {
      error: error instanceof Error ? error.message : String(error),
    })
    return null
  }
}

/**
 * An update exists, but this install can't write it into its own bundle: it's translocated, or on
 * a read-only volume. Downloading ~63 MB it could never apply, once an hour, forever, is the thing
 * to avoid, so the flow stops here and asks the user to move Cmdr to Applications instead.
 *
 * The manifest check itself keeps running on the poll: it's what keeps the install counted as
 * active, and it costs a few hundred bytes.
 */
function finishCheckWithUnwritableBundle(
  trigger: UpdateCheckTrigger,
  blocker: BundleWriteBlocker,
  staged: string | null,
): void {
  log.info("An update is out, but this install can't write its own bundle ({blocker}); asking the user to move Cmdr", {
    blocker,
  })
  reportUpdateCheck({ trigger, outcome: 'blocked', failure: blockerFailure(blocker), stagedVersion: staged })
  showMoveToApplicationsNudge(blocker)

  if (staged !== null) {
    updateState.status = 'ready'
    updateState.nextVersion = staged
    return
  }
  updateState.status = 'idle'
  updateState.nextVersion = null
}

/**
 * A build is now synced into the bundle and only a restart away. A newer one earns a fresh prompt
 * even when the previous version's was dismissed, so the nudge clock resets here.
 */
function finishCheckWithStagedUpdate(trigger: UpdateCheckTrigger, update: UpdateInfo): void {
  log.info('v{version} installed, restart to apply', { version: update.version })
  updateState.status = 'ready'
  updateState.update = update
  updateState.nextVersion = update.version
  updateState.failure = null
  lastRestartToastAt = null
  reportUpdateCheck({ trigger, outcome: 'staged', stagedVersion: update.version })
  showUpdateToast()
}

/**
 * A check that ran while a build was already staged, and found nothing that beats it. The state
 * machine stays exactly where it was: moving it would either rewrite the bundle with the bytes
 * already in it, or make Settings claim the app is up to date while a restart is still pending.
 * All that's left is keeping the restart prompt reachable.
 */
function keepStagedUpdate(trigger: UpdateCheckTrigger, staged: string): void {
  log.debug('v{version} is still the newest build staged for restart', { version: staged })
  reportUpdateCheck({ trigger, outcome: 'already_staged', stagedVersion: staged })
  renudgeRestartIfDue()
}

function finishCheckWithNoUpdate(trigger: UpdateCheckTrigger, currentVersion: string, staged: string | null): void {
  if (staged !== null) {
    keepStagedUpdate(trigger, staged)
    return
  }
  log.debug('v{version} is up to date', { version: currentVersion })
  reportUpdateCheck({ trigger, outcome: 'up_to_date' })
  updateState.status = 'idle'
  updateState.nextVersion = null
}

/**
 * Reset state and log the failure at the right level for the phase.
 *
 * - `'check'` failures log once per condition until a check gets an answer (`checkFailureLog`). The macOS check is
 *   typed, and its level follows `serverRequestLogLevel`: no network, a timeout, or a server having a bad moment stay at
 *   warn, so a background tick on a flaky network doesn't trip the auto error reporter, and only a manifest Cmdr's own
 *   server refused or served unreadable logs at error. The plugin's check elsewhere isn't typed, and its failures are
 *   the network's as often as not, so it stays at warn.
 * - `'download-install'` failures follow the same rule where they're typed (`downloadInstallLogLevel`): a macOS
 *   download that the network or the host's bad moment stopped logs at warn. A signature mismatch, a disk failure, a
 *   404 for the tarball, an install that broke, and the plugin's untyped failures log at error, so they DO trip
 *   auto-report: they mean something is wrong with the release or this machine.
 *
 * Both reach the UI as `updateState.failure`, a typed value the toast and Settings word from the catalog. See
 * `apps/desktop/src-tauri/src/error_reporter/CLAUDE.md` § convention.
 *
 * `staged` is the version already synced into the bundle, if any. A build waiting for a restart
 * outlives a failed attempt at a newer one: the download writes to a temp dir, so a failure there
 * leaves the staged bytes untouched, and the user still has something worth restarting for. So the
 * state machine returns to `ready` and no message reaches them; the failure is in the log and in
 * the `update_check` event instead.
 */
function finishCheckWithFailure(
  trigger: UpdateCheckTrigger,
  error: unknown,
  phase: 'check' | 'download-install',
  staged: string | null,
): void {
  const message = error instanceof Error ? error.message : String(error)
  // Read the phase off the state machine BEFORE moving it. macOS runs the download and the install
  // in one try block, and `status` is the typed record of which one was in flight; the message
  // that came back is never asked, here or anywhere.
  const failure: UpdateCheckFailure =
    phase === 'check' ? 'check' : updateState.status === 'installing' ? 'install' : 'download'
  reportUpdateCheck({ trigger, outcome: 'failed', failure, stagedVersion: staged })

  let standing: UpdateFailure
  if (failure === 'check') {
    const request = serverRequestFailureOf(error)
    standing = { phase: 'check', request }
    logCheckFailure(request, message)
  } else {
    standing = { phase: failure === 'install' ? 'install' : 'download' }
    const level = downloadInstallLogLevel(error)
    if (level === 'error') {
      log.error('Download/install failed: {error}', { error: message })
    } else if (level === 'warn') {
      log.warn('Download/install failed: {error}', { error: message })
    } else {
      log.info('Download/install not run: {error}', { error: message })
    }
  }

  if (staged !== null) {
    updateState.status = 'ready'
    updateState.nextVersion = staged
    renudgeRestartIfDue()
    return
  }

  updateState.status = 'idle'
  updateState.nextVersion = null
  updateState.failure = standing
}

/**
 * A download the network or the tarball host's bad moment stopped follows the check's rule (`serverRequestLogLevel`).
 * Everything else stays at error: a signature mismatch, a disk failure, an install, and the plugin's untyped failures.
 */
function downloadInstallLogLevel(error: unknown): 'info' | 'warn' | 'error' {
  const download = failureOf(UpdateDownloadFailure, error)
  return download?.type === 'request' ? serverRequestLogLevel(download.failure) : 'error'
}

/** One line per failing condition until a check gets an answer, at the level the failure earns. */
function logCheckFailure(request: ServerRequestError | null, message: string): void {
  const condition =
    request === null ? 'untyped' : request.type === 'refused' ? `refused ${String(request.status)}` : request.type
  if (!checkFailureLog.shouldLog(condition)) return
  const level = request === null ? 'warn' : serverRequestLogLevel(request)
  if (level === 'error') {
    log.error('Check failed: {error}', { error: message })
  } else if (level === 'warn') {
    log.warn('Check failed: {error}', { error: message })
  } else {
    log.info('Check not run: {error}', { error: message })
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
    // The one caller is the `app.checkForUpdates` command handler, and the status toast is what
    // makes this wrapper the command's own path, so the trigger is fixed here rather than passed in.
    await checkForUpdates('command')
  } finally {
    if (updateState.status === 'ready') {
      dismissToast('update-check')
    }
  }
}

/**
 * How often the background loop wakes to ask the backend whether a check is due. The backend holds
 * the last answered check across relaunches (`src-tauri/src/update_schedule.rs`), so the loop never
 * sleeps a whole interval: a relaunch, a wake from sleep, or a changed interval is picked up within
 * one tick, and a burst of them collapses into one check.
 */
export const UPDATE_WAKE_TICK_MS = 5 * 60 * 1000

/**
 * The poll loop's pending wake. Module-scoped so `applyAutoCheckEnabled()` can stop and restart the
 * loop in response to live `updates.autoCheck` flips, without restarting the whole checker.
 * `pollGeneration` names the running loop (0 = stopped). A tick carries the generation it was
 * scheduled under, so one still awaiting the backend when the loop stops, or stops and restarts,
 * schedules nothing and can't leave two loops running.
 */
let pollTimer: ReturnType<typeof setTimeout> | undefined
let pollGeneration = 0
let lastPollGeneration = 0

/** When to wake next, given how long until a check is due. */
function nextWakeMs(dueInMs: number | null): number {
  return dueInMs === null || dueInMs === 0 ? UPDATE_WAKE_TICK_MS : Math.min(dueInMs, UPDATE_WAKE_TICK_MS)
}

/**
 * One wake: ask whether a check is due, run it if so, schedule the next wake. A backend that can't
 * answer (`null`) means no check this time, rather than a check on every tick.
 */
async function pollTick(generation: number, trigger: UpdateCheckTrigger): Promise<void> {
  pollTimer = undefined
  const dueInMs = await updateCheckDueIn(getCheckIntervalMs())
  if (generation !== pollGeneration) return
  if (dueInMs === 0) {
    await checkForUpdates(trigger)
    if (generation !== pollGeneration) return
  }
  pollTimer = setTimeout(() => void pollTick(generation, 'poll'), nextWakeMs(dueInMs))
}

function startPollLoop(firstWakeMs: number, firstTrigger: UpdateCheckTrigger): void {
  if (pollGeneration !== 0) return
  lastPollGeneration += 1
  const generation = lastPollGeneration
  pollGeneration = generation
  pollTimer = setTimeout(() => void pollTick(generation, firstTrigger), firstWakeMs)
}

function stopPollLoop(): void {
  pollGeneration = 0
  if (pollTimer === undefined) return
  clearTimeout(pollTimer)
  pollTimer = undefined
}

/**
 * Live-apply hook for `updates.autoCheck`. Off cancels the background poll loop in
 * place (the user keeps whatever update state we last computed; we just stop asking).
 * On fires one immediate check, whatever the schedule says (the user just asked for
 * updates), and restarts the loop from the next wake. Called from
 * `settings-applier.ts`'s `passthroughBackendHandlers` lookup whenever the setting
 * flips, including from the onboarding wizard's step 3.
 *
 * Safe to call before `startUpdateChecker()` has run (only matters in tests today,
 * but cheap insurance): `startPollLoop()` is idempotent, and `checkForUpdates()`
 * tolerates an early call (it just transitions through `checking` → `idle`).
 */
export function applyAutoCheckEnabled(enabled: boolean): void {
  if (enabled) {
    void checkForUpdates('auto_check_on')
    startPollLoop(UPDATE_WAKE_TICK_MS, 'poll')
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
    // The first wake is right away; it checks only if the schedule says one is due.
    startPollLoop(0, 'startup')
  } else {
    log.debug('Auto-check disabled; skipping the poll loop')
  }

  // Live-apply for `updates.autoCheck` lives in `settings-applier.ts`'s
  // `passthroughBackendHandlers`, calling `applyAutoCheckEnabled()` above. One source
  // of truth keeps the wizard's step 3 toggle, the Settings UI switch, and any future
  // MCP/IPC writer all going through the same hook.

  // Return cleanup function
  return () => {
    stopPollLoop()
  }
}

/**
 * Test-only hook: reset module-level gating flags. Production code should never call this.
 */
export function _resetUpdaterStateForTest(): void {
  stopPollLoop()
  onboarded = false
  onboardingShowing = false
  lastRestartToastAt = null
  moveNudgeShown = false
  pendingMoveNudge = null
  checkFailureLog.clear()
  updateBlockerNotice.blocker = null
  updateState.status = 'idle'
  updateState.update = null
  updateState.failure = null
  updateState.managed = null
  updateState.previousVersion = null
  updateState.nextVersion = null
}

/**
 * Test-only hook: directly set the update state's status. Production code should never call this.
 */
export function _setUpdateStatusForTest(status: UpdateState['status']): void {
  updateState.status = status
}
