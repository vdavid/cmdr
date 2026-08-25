/**
 * Unit tests for the updater module's gating logic.
 *
 * The "update ready, restart now" toast must be suppressed during onboarding (the user just downloaded
 * the app, they'd be confused) and while the FDA-revoked re-prompt is showing. These tests cover the
 * pure predicate plus the two trigger paths (`notifyOnboardingComplete` and `setOnboardingShowing`).
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

// `vi.mock` is hoisted to the top of the file. Module-scope mocks captured via `vi.hoisted` so the
// references survive that hoist and stay accessible from the test bodies for assertions.
const {
  addToastMock,
  dismissToastMock,
  getSettingMock,
  setSettingMock,
  forceSaveMock,
  invokeMock,
  getVersionMock,
  pluginCheckMock,
} = vi.hoisted(() => ({
  addToastMock: vi.fn(),
  dismissToastMock: vi.fn(),
  // `advanced.updateCheckInterval` for the poll loop, `onboarding.completed` for the gate.
  getSettingMock: vi.fn((id: string) => (id === 'onboarding.completed' ? false : 60 * 60 * 1000)),
  setSettingMock: vi.fn(),
  forceSaveMock: vi.fn(() => Promise.resolve(true)),
  invokeMock: vi.fn(),
  getVersionMock: vi.fn(() => Promise.resolve('0.0.0-test')),
  pluginCheckMock: vi.fn(),
}))

vi.mock('$lib/ui/toast', () => ({
  addToast: addToastMock,
  dismissToast: dismissToastMock,
}))

vi.mock('$lib/settings/settings-store', () => ({
  getSetting: getSettingMock,
  setSetting: setSettingMock,
  forceSave: forceSaveMock,
  onSpecificSettingChange: vi.fn(() => () => {}),
}))

vi.mock('@tauri-apps/api/core', () => ({
  invoke: invokeMock,
}))

vi.mock('@tauri-apps/api/app', () => ({
  getVersion: getVersionMock,
}))

// jsdom's userAgent does not include "Macintosh", so the updater takes the non-macOS branch and
// dynamically imports `@tauri-apps/plugin-updater`. Mock that here so the test environment doesn't
// try to load the real Tauri plugin.
vi.mock('@tauri-apps/plugin-updater', () => ({
  check: pluginCheckMock,
}))

vi.mock('$lib/logging/logger', () => ({
  getAppLogger: () => ({
    debug: () => {},
    info: () => {},
    warn: () => {},
    error: () => {},
  }),
}))

// Now safe to import.
import {
  RESTART_NUDGE_INTERVAL_MS,
  _resetUpdaterStateForTest,
  _setUpdateStatusForTest,
  applyAutoCheckEnabled,
  checkForUpdates,
  notifyOnboardingComplete,
  runMenuTriggeredCheck,
  setOnboardingShowing,
  shouldShowUpdateToast,
  supersedesStagedUpdate,
  updateState,
} from './updater.svelte'
import { formatUpdateStatus } from './update-status-text'

describe('shouldShowUpdateToast', () => {
  it('returns true only when onboarded, FDA prompt closed, and status is ready', () => {
    expect(shouldShowUpdateToast({ onboarded: true, onboardingShowing: false, status: 'ready' })).toBe(true)
  })

  it('returns false while not onboarded', () => {
    expect(shouldShowUpdateToast({ onboarded: false, onboardingShowing: false, status: 'ready' })).toBe(false)
  })

  it('returns false while the FDA prompt is showing', () => {
    expect(shouldShowUpdateToast({ onboarded: true, onboardingShowing: true, status: 'ready' })).toBe(false)
  })

  it.each(['idle', 'checking', 'downloading'] as const)('returns false when status is %s', (status) => {
    expect(shouldShowUpdateToast({ onboarded: true, onboardingShowing: false, status })).toBe(false)
  })

  it('handles every cell of the truth table', () => {
    const statuses = ['idle', 'checking', 'downloading', 'ready'] as const
    for (const onboarded of [false, true]) {
      for (const onboardingShowing of [false, true]) {
        for (const status of statuses) {
          const expected = onboarded && !onboardingShowing && status === 'ready'
          expect(shouldShowUpdateToast({ onboarded, onboardingShowing, status })).toBe(expected)
        }
      }
    }
  })
})

describe('notifyOnboardingComplete', () => {
  beforeEach(() => {
    _resetUpdaterStateForTest()
    addToastMock.mockClear()
    setSettingMock.mockClear()
  })

  afterEach(() => {
    _resetUpdaterStateForTest()
  })

  it('persists onboarding.completed: true, without waiting out the save debounce', async () => {
    await notifyOnboardingComplete()
    expect(setSettingMock).toHaveBeenCalledWith('onboarding.completed', true)
    expect(forceSaveMock).toHaveBeenCalledOnce()
  })

  it('triggers the toast when an update is already ready', async () => {
    _setUpdateStatusForTest('ready')
    await notifyOnboardingComplete()
    expect(addToastMock).toHaveBeenCalledTimes(1)
    expect(addToastMock.mock.calls[0][1]).toMatchObject({ id: 'update', dismissal: 'persistent' })
  })

  it('does NOT trigger the toast when status is idle', async () => {
    _setUpdateStatusForTest('idle')
    await notifyOnboardingComplete()
    expect(addToastMock).not.toHaveBeenCalled()
  })

  it('does NOT trigger the toast when status is downloading', async () => {
    _setUpdateStatusForTest('downloading')
    await notifyOnboardingComplete()
    expect(addToastMock).not.toHaveBeenCalled()
  })
})

describe('setOnboardingShowing', () => {
  beforeEach(() => {
    _resetUpdaterStateForTest()
    addToastMock.mockClear()
  })

  afterEach(() => {
    _resetUpdaterStateForTest()
  })

  it('does not show the toast on its own when flipped to true', () => {
    _setUpdateStatusForTest('ready')
    setOnboardingShowing(true)
    expect(addToastMock).not.toHaveBeenCalled()
  })

  it('re-shows the toast when flipped from true to false if onboarded and ready', async () => {
    _setUpdateStatusForTest('ready')
    await notifyOnboardingComplete()
    addToastMock.mockClear()

    setOnboardingShowing(true)
    expect(addToastMock).not.toHaveBeenCalled()

    setOnboardingShowing(false)
    expect(addToastMock).toHaveBeenCalledTimes(1)
    expect(addToastMock.mock.calls[0][1]).toMatchObject({ id: 'update', dismissal: 'persistent' })
  })

  it('does not show the toast on flip-to-false when not onboarded', () => {
    _setUpdateStatusForTest('ready')
    setOnboardingShowing(true)
    setOnboardingShowing(false)
    expect(addToastMock).not.toHaveBeenCalled()
  })

  it('does not show the toast on flip-to-false when status is not ready', async () => {
    await notifyOnboardingComplete() // onboarded=true, status=idle
    addToastMock.mockClear()

    setOnboardingShowing(true)
    setOnboardingShowing(false)
    expect(addToastMock).not.toHaveBeenCalled()
  })
})

describe('formatUpdateStatus', () => {
  it('returns checking… string while checking', () => {
    expect(formatUpdateStatus({ status: 'checking', error: null, previousVersion: '1.2.3', nextVersion: null })).toBe(
      'Checking…',
    )
  })

  it('returns no-updates string for idle after a successful check', () => {
    expect(formatUpdateStatus({ status: 'idle', error: null, previousVersion: '1.2.3', nextVersion: null })).toBe(
      'No updates found. Current version: v1.2.3',
    )
  })

  it('returns empty string for idle before any check has run', () => {
    expect(formatUpdateStatus({ status: 'idle', error: null, previousVersion: null, nextVersion: null })).toBe('')
  })

  it('returns downloading string with both versions', () => {
    expect(
      formatUpdateStatus({ status: 'downloading', error: null, previousVersion: '1.2.3', nextVersion: '1.3.0' }),
    ).toBe('Update found, downloading v1.3.0 (current: v1.2.3)…')
  })

  it('returns installing string with both versions', () => {
    expect(
      formatUpdateStatus({ status: 'installing', error: null, previousVersion: '1.2.3', nextVersion: '1.3.0' }),
    ).toBe('Installing v1.3.0 (current: v1.2.3)…')
  })

  it('returns null when error is set so the caller can render its own error UI', () => {
    expect(
      formatUpdateStatus({ status: 'idle', error: 'boom', previousVersion: '1.2.3', nextVersion: null }),
    ).toBeNull()
  })
})

describe('applyAutoCheckEnabled', () => {
  beforeEach(() => {
    _resetUpdaterStateForTest()
    invokeMock.mockReset()
    pluginCheckMock.mockReset()
  })

  afterEach(() => {
    _resetUpdaterStateForTest()
  })

  it('fires one immediate check when flipped on', async () => {
    pluginCheckMock.mockResolvedValueOnce(null)
    applyAutoCheckEnabled(true)
    // Settle the async `checkForUpdates()` triggered inline.
    await new Promise((r) => setTimeout(r, 0))
    expect(pluginCheckMock).toHaveBeenCalledTimes(1)
  })

  it('does NOT fire a check when flipped off', async () => {
    applyAutoCheckEnabled(false)
    await new Promise((r) => setTimeout(r, 0))
    expect(pluginCheckMock).not.toHaveBeenCalled()
  })
})

describe('runMenuTriggeredCheck', () => {
  beforeEach(() => {
    _resetUpdaterStateForTest()
    addToastMock.mockClear()
    dismissToastMock.mockClear()
    invokeMock.mockReset()
    getVersionMock.mockClear()
    pluginCheckMock.mockReset()
  })

  afterEach(() => {
    _resetUpdaterStateForTest()
  })

  it('adds a status toast with id "update-check" and a 10s timeout, then runs checkForUpdates', async () => {
    pluginCheckMock.mockResolvedValueOnce(null) // no update
    await runMenuTriggeredCheck()
    expect(addToastMock).toHaveBeenCalledTimes(1)
    expect(addToastMock.mock.calls[0][1]).toMatchObject({ id: 'update-check', timeoutMs: 10000 })
    expect(pluginCheckMock).toHaveBeenCalledTimes(1)
  })

  it('dismisses the status toast when status flips to ready', async () => {
    pluginCheckMock.mockResolvedValueOnce({
      version: '1.3.0',
      downloadAndInstall: vi.fn(async () => {}),
    })
    await notifyOnboardingComplete() // ensures the persistent toast is eligible too
    addToastMock.mockClear()
    await runMenuTriggeredCheck()
    expect(updateState.status).toBe('ready')
    expect(dismissToastMock).toHaveBeenCalledWith('update-check')
  })

  it('does not dismiss when status stays idle (no update found)', async () => {
    pluginCheckMock.mockResolvedValueOnce(null)
    await runMenuTriggeredCheck()
    expect(updateState.status).toBe('idle')
    expect(dismissToastMock).not.toHaveBeenCalled()
  })

  it('surfaces the error string on the state when the check rejects', async () => {
    pluginCheckMock.mockRejectedValueOnce(new Error('network down'))
    await runMenuTriggeredCheck()
    expect(updateState.error).toBe('network down')
    expect(updateState.status).toBe('idle')
    expect(dismissToastMock).not.toHaveBeenCalled()
  })
})

describe('supersedesStagedUpdate', () => {
  it('accepts anything when nothing is staged yet', () => {
    expect(supersedesStagedUpdate('0.29.0', null)).toBe(true)
  })

  it('rejects the build already staged, so a tick never re-syncs identical bytes', () => {
    expect(supersedesStagedUpdate('0.29.0', '0.29.0')).toBe(false)
  })

  it('rejects an older offer than the staged build', () => {
    expect(supersedesStagedUpdate('0.28.0', '0.29.0')).toBe(false)
  })

  it('accepts a release that shipped after the staged one', () => {
    expect(supersedesStagedUpdate('0.33.0', '0.29.0')).toBe(true)
  })
})

describe('checking again while an update is staged', () => {
  /** One offer from the update server, with a download that succeeds. */
  function offer(version: string) {
    return { version, downloadAndInstall: vi.fn(async () => {}) }
  }

  /** Gets the module to `ready` with `version` synced into the bundle, onboarding done. */
  async function stage(version: string): Promise<void> {
    await notifyOnboardingComplete()
    pluginCheckMock.mockResolvedValueOnce(offer(version))
    await checkForUpdates('poll')
    expect(updateState.status).toBe('ready')
  }

  beforeEach(() => {
    _resetUpdaterStateForTest()
    addToastMock.mockClear()
    dismissToastMock.mockClear()
    getVersionMock.mockClear()
    pluginCheckMock.mockReset()
  })

  afterEach(() => {
    vi.useRealTimers()
    _resetUpdaterStateForTest()
  })

  it('still asks the update server once a build is staged', async () => {
    await stage('0.29.0')
    pluginCheckMock.mockResolvedValueOnce(offer('0.29.0'))

    await checkForUpdates('poll')

    // Pre-fix the `ready` guard returned here, so the hourly poll went silent for the rest of the
    // session and the install sat on whatever was staged however long that session ran.
    expect(pluginCheckMock).toHaveBeenCalledTimes(2)
  })

  it('leaves the staged bytes alone when the server offers the same build', async () => {
    await stage('0.29.0')
    const repeat = offer('0.29.0')
    pluginCheckMock.mockResolvedValueOnce(repeat)

    await checkForUpdates('poll')

    expect(repeat.downloadAndInstall).not.toHaveBeenCalled()
    expect(updateState.status).toBe('ready')
    expect(updateState.update?.version).toBe('0.29.0')
  })

  it('re-stages when a newer release ships, and prompts for it again', async () => {
    await stage('0.29.0')
    addToastMock.mockClear()
    const newer = offer('0.33.0')
    pluginCheckMock.mockResolvedValueOnce(newer)

    await checkForUpdates('poll')

    expect(newer.downloadAndInstall).toHaveBeenCalledTimes(1)
    expect(updateState.status).toBe('ready')
    expect(updateState.update?.version).toBe('0.33.0')
    expect(updateState.nextVersion).toBe('0.33.0')
    // A newer build earns its own prompt, even if the previous one was dismissed with "Later".
    expect(addToastMock).toHaveBeenCalledTimes(1)
    expect(addToastMock.mock.calls[0][1]).toMatchObject({ id: 'update', dismissal: 'persistent' })
  })

  it('keeps the staged update ready when the re-check fails', async () => {
    await stage('0.29.0')
    pluginCheckMock.mockRejectedValueOnce(new Error('network down'))

    await checkForUpdates('poll')

    // The staged build is still in the bundle and still worth restarting for, so a transient
    // network blip must not downgrade the state machine or raise a message at the user.
    expect(updateState.status).toBe('ready')
    expect(updateState.update?.version).toBe('0.29.0')
    expect(updateState.error).toBeNull()
  })

  it('keeps the staged update ready when downloading the newer build fails', async () => {
    await stage('0.29.0')
    pluginCheckMock.mockResolvedValueOnce({
      version: '0.33.0',
      downloadAndInstall: vi.fn(() => Promise.reject(new Error('signature mismatch'))),
    })

    await checkForUpdates('poll')

    expect(updateState.status).toBe('ready')
    expect(updateState.update?.version).toBe('0.29.0')
    expect(updateState.nextVersion).toBe('0.29.0')
  })

  it.each(['downloading', 'installing', 'checking'] as const)('does not interrupt an in-flight %s', async (status) => {
    _setUpdateStatusForTest(status)

    await checkForUpdates('poll')

    expect(pluginCheckMock).not.toHaveBeenCalled()
  })

  it('brings the restart prompt back after a day, not on every tick', async () => {
    vi.useFakeTimers()
    vi.setSystemTime(new Date('2026-08-25T09:00:00Z'))
    await stage('0.29.0')
    addToastMock.mockClear()

    // An hour later: the poll runs, the answer is unchanged, and the user is left alone.
    vi.setSystemTime(Date.now() + 60 * 60 * 1000)
    pluginCheckMock.mockResolvedValueOnce(offer('0.29.0'))
    await checkForUpdates('poll')
    expect(addToastMock).not.toHaveBeenCalled()

    // A full nudge interval on: the prompt comes back, so "Later" can't silence it for good.
    vi.setSystemTime(Date.now() + RESTART_NUDGE_INTERVAL_MS)
    pluginCheckMock.mockResolvedValueOnce(offer('0.29.0'))
    await checkForUpdates('poll')
    expect(addToastMock).toHaveBeenCalledTimes(1)
    expect(addToastMock.mock.calls[0][1]).toMatchObject({ id: 'update', dismissal: 'persistent' })
  })
})
