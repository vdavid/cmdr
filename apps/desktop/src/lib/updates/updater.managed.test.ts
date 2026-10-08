/**
 * What the macOS update flow does with the organization's policy. The backend decides (`check_for_update` answers a
 * typed outcome, `download_update` / `install_update` refuse a version the policy no longer allows); these tests pin
 * that the frontend renders each answer as a terminal phase, never as a failure, and that a background check never
 * toasts a held update.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

const {
  addToastMock,
  checkForUpdateMock,
  downloadUpdateMock,
  installUpdateMock,
  trackEventMock,
  getSettingMock,
  logger,
} = vi.hoisted(() => ({
  addToastMock: vi.fn(),
  checkForUpdateMock: vi.fn(),
  downloadUpdateMock: vi.fn(() => Promise.resolve()),
  installUpdateMock: vi.fn(() => Promise.resolve()),
  trackEventMock: vi.fn(),
  getSettingMock: vi.fn((id: string): unknown => {
    if (id === 'onboarding.completed') return true
    if (id === 'updates.autoCheck') return true
    return 60 * 60 * 1000
  }),
  logger: { debug: vi.fn(), info: vi.fn(), warn: vi.fn(), error: vi.fn() },
}))

vi.mock('$lib/tauri-commands', () => ({
  checkForUpdate: checkForUpdateMock,
  downloadUpdate: downloadUpdateMock,
  installUpdate: installUpdateMock,
  updateWriteBlocker: vi.fn(() => Promise.resolve(null)),
  updateCheckDueIn: vi.fn(() => Promise.resolve(0)),
  recordUpdateCheck: vi.fn(() => Promise.resolve()),
  trackEvent: trackEventMock,
}))

vi.mock('$lib/shortcuts/key-capture', () => ({ isMacOS: () => true }))
vi.mock('$lib/ui/toast', () => ({ addToast: addToastMock, dismissToast: vi.fn() }))
vi.mock('$lib/settings/settings-store', () => ({
  getSetting: getSettingMock,
  setSetting: vi.fn(),
  forceSave: vi.fn(() => Promise.resolve(true)),
  onSpecificSettingChange: vi.fn(() => () => {}),
}))
vi.mock('@tauri-apps/api/app', () => ({ getVersion: vi.fn(() => Promise.resolve('0.51.0')) }))
vi.mock('$lib/logging/logger', () => ({ getAppLogger: () => logger }))

import {
  UPDATE_WAKE_TICK_MS,
  _resetUpdaterStateForTest,
  checkForUpdates,
  runMenuTriggeredCheck,
  startUpdateChecker,
  updateState,
} from './updater.svelte'
import { formatUpdateStatus } from './update-status-text'
import { UpdateDownloadFailure } from './update-download-failure'
import { UpdateInstallFailure } from './update-install-failure'
import UpdateCheckToastContent from './UpdateCheckToastContent.svelte'

function lastReport(): Record<string, string> | undefined {
  const reports = trackEventMock.mock.calls.filter(([name]) => name === 'update_check')
  return reports.at(-1)?.[1] as Record<string, string> | undefined
}

/** Nothing a managed answer does may look like something went wrong. */
function expectNothingLoggedAsAProblem(): void {
  expect(logger.warn).not.toHaveBeenCalled()
  expect(logger.error).not.toHaveBeenCalled()
}

beforeEach(() => {
  _resetUpdaterStateForTest()
  vi.clearAllMocks()
})

afterEach(() => {
  vi.useRealTimers()
  _resetUpdaterStateForTest()
})

describe('updates turned off by the organization', () => {
  it('ends the check on the managed answer, downloads nothing, and reports no failure', async () => {
    checkForUpdateMock.mockResolvedValueOnce({ kind: 'updatesDisabledByPolicy' })

    await checkForUpdates('settings')

    expect(downloadUpdateMock).not.toHaveBeenCalled()
    expect(updateState.status).toBe('idle')
    expect(updateState.failure).toBeNull()
    expect(updateState.managed).toEqual({ kind: 'updatesDisabledByPolicy' })
    expect(formatUpdateStatus(updateState)).toBe('Your organization manages updates for Cmdr.')
    expect(lastReport()).toEqual({
      trigger: 'settings',
      outcome: 'updates_disabled_by_policy',
      failure: 'none',
      staged_version: 'none',
    })
    expectNothingLoggedAsAProblem()
  })

  it('shows the managed sentence in the menu check’s toast instead of a check', async () => {
    checkForUpdateMock.mockResolvedValueOnce({ kind: 'updatesDisabledByPolicy' })

    await runMenuTriggeredCheck()

    expect(checkForUpdateMock).toHaveBeenCalledWith('command')
    expect(addToastMock).toHaveBeenCalledWith(UpdateCheckToastContent, expect.objectContaining({ id: 'update-check' }))
    // The toast renders `formatUpdateStatus(updateState)`.
    expect(formatUpdateStatus(updateState)).toBe('Your organization manages updates for Cmdr.')
  })

  it('clears the managed answer when the next check starts', async () => {
    checkForUpdateMock.mockResolvedValueOnce({ kind: 'updatesDisabledByPolicy' })
    await checkForUpdates('settings')
    checkForUpdateMock.mockResolvedValueOnce({ kind: 'upToDate' })
    await checkForUpdates('settings')

    expect(updateState.managed).toBeNull()
    expect(formatUpdateStatus(updateState)).toBe('No updates found. Current version: v0.51.0')
  })
})

describe('an update held back by the organization’s ceiling', () => {
  const held = { kind: 'heldByPolicy', available: '0.53.0', ceiling: '0.52' } as const

  it('says a release exists and the organization holds this Mac back, without downloading', async () => {
    checkForUpdateMock.mockResolvedValueOnce(held)

    await checkForUpdates('settings')

    expect(downloadUpdateMock).not.toHaveBeenCalled()
    expect(updateState.status).toBe('idle')
    expect(formatUpdateStatus(updateState)).toBe(
      'Cmdr 0.53.0 is out, but your organization keeps this Mac on 0.52 or earlier.',
    )
    // Categorical only: neither version rides the event.
    expect(lastReport()).toEqual({
      trigger: 'settings',
      outcome: 'held_by_policy',
      failure: 'none',
      staged_version: 'none',
    })
    expectNothingLoggedAsAProblem()
  })

  it('never toasts from a background check', async () => {
    checkForUpdateMock.mockResolvedValue(held)

    await checkForUpdates('poll')
    await checkForUpdates('startup')

    expect(addToastMock).not.toHaveBeenCalled()
  })

  it('leaves a build already staged for restart alone', async () => {
    checkForUpdateMock.mockResolvedValueOnce({ kind: 'available', version: '0.52.1' })
    await checkForUpdates('poll')
    expect(updateState.status).toBe('ready')

    checkForUpdateMock.mockResolvedValueOnce(held)
    await checkForUpdates('poll')

    expect(updateState.status).toBe('ready')
    expect(updateState.nextVersion).toBe('0.52.1')
    expect(updateState.managed).toBeNull()
    expect(lastReport()).toEqual({
      trigger: 'poll',
      outcome: 'held_by_policy',
      failure: 'none',
      staged_version: '0.52.1',
    })
  })
})

describe('automatic checks turned off by the organization', () => {
  it('never starts the background loop when the overlay reads auto-check as off', async () => {
    vi.useFakeTimers()
    getSettingMock.mockImplementation((id: string) => (id === 'updates.autoCheck' ? false : true))

    const stop = startUpdateChecker()
    await vi.advanceTimersByTimeAsync(UPDATE_WAKE_TICK_MS * 3)

    expect(checkForUpdateMock).not.toHaveBeenCalled()
    stop()
  })

  it('stops the background loop at the backend’s first refusal', async () => {
    vi.useFakeTimers()
    getSettingMock.mockImplementation((id: string) => (id === 'updates.autoCheck' ? true : 60 * 60 * 1000))
    checkForUpdateMock.mockResolvedValue({ kind: 'automaticChecksDisabledByPolicy' })

    const stop = startUpdateChecker()
    await vi.advanceTimersByTimeAsync(0)
    await vi.advanceTimersByTimeAsync(UPDATE_WAKE_TICK_MS * 3)

    expect(checkForUpdateMock).toHaveBeenCalledTimes(1)
    expect(checkForUpdateMock).toHaveBeenCalledWith('startup')
    expect(updateState.status).toBe('idle')
    expect(updateState.failure).toBeNull()
    expect(formatUpdateStatus(updateState)).toBe('')
    expect(lastReport()?.outcome).toBe('automatic_checks_disabled_by_policy')
    expectNothingLoggedAsAProblem()
    stop()
  })
})

describe('a policy that arrives after the check', () => {
  it('ends a refused download quietly, with no failure to show', async () => {
    checkForUpdateMock.mockResolvedValueOnce({ kind: 'available', version: '0.53.0' })
    downloadUpdateMock.mockRejectedValueOnce(new UpdateDownloadFailure({ type: 'blockedByPolicy' }))

    await checkForUpdates('poll')

    expect(installUpdateMock).not.toHaveBeenCalled()
    expect(updateState.status).toBe('idle')
    expect(updateState.failure).toBeNull()
    expect(lastReport()).toEqual({
      trigger: 'poll',
      outcome: 'blocked_by_policy',
      failure: 'download',
      staged_version: 'none',
    })
    expectNothingLoggedAsAProblem()
  })

  it('ends a refused install quietly, with no failure to show', async () => {
    checkForUpdateMock.mockResolvedValueOnce({ kind: 'available', version: '0.53.0' })
    installUpdateMock.mockRejectedValueOnce(new UpdateInstallFailure({ type: 'blockedByPolicy' }))

    await checkForUpdates('poll')

    expect(updateState.status).toBe('idle')
    expect(updateState.failure).toBeNull()
    expect(lastReport()?.outcome).toBe('blocked_by_policy')
    expect(lastReport()?.failure).toBe('install')
    expectNothingLoggedAsAProblem()
  })

  it('still logs a real install failure at error', async () => {
    checkForUpdateMock.mockResolvedValueOnce({ kind: 'available', version: '0.53.0' })
    installUpdateMock.mockRejectedValueOnce(new UpdateInstallFailure({ type: 'failed', detail: 'sync broke' }))

    await checkForUpdates('poll')

    expect(updateState.failure).toEqual({ phase: 'install' })
    expect(logger.error).toHaveBeenCalledTimes(1)
  })
})
