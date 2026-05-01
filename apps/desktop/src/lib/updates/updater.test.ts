/**
 * Unit tests for the updater module's gating logic.
 *
 * The "update ready, restart now" toast must be suppressed during onboarding (the user just downloaded
 * the app — they'd be confused) and while the FDA-revoked re-prompt is showing. These tests cover the
 * pure predicate plus the two trigger paths (`notifyOnboardingComplete` and `setFdaPromptShowing`).
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

// `vi.mock` is hoisted to the top of the file. Module-scope mocks captured via `vi.hoisted` so the
// references survive that hoist and stay accessible from the test bodies for assertions.
const {
  addToastMock,
  dismissToastMock,
  loadSettingsMock,
  saveSettingsMock,
  invokeMock,
  getVersionMock,
  pluginCheckMock,
} = vi.hoisted(() => ({
  addToastMock: vi.fn(),
  dismissToastMock: vi.fn(),
  loadSettingsMock: vi.fn(() =>
    Promise.resolve({
      showHiddenFiles: true,
      fullDiskAccessChoice: 'notAskedYet' as const,
      isOnboarded: false,
    }),
  ),
  saveSettingsMock: vi.fn(() => Promise.resolve()),
  invokeMock: vi.fn(),
  getVersionMock: vi.fn(() => Promise.resolve('0.0.0-test')),
  pluginCheckMock: vi.fn(),
}))

vi.mock('$lib/ui/toast', () => ({
  addToast: addToastMock,
  dismissToast: dismissToastMock,
}))

vi.mock('$lib/settings-store', () => ({
  loadSettings: loadSettingsMock,
  saveSettings: saveSettingsMock,
}))

// The settings registry hook is only used by `startUpdateChecker`, but importing the module pulls it in.
vi.mock('$lib/settings/settings-store', () => ({
  getSetting: vi.fn(() => 60 * 60 * 1000),
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
  _resetUpdaterStateForTest,
  _setUpdateStatusForTest,
  notifyOnboardingComplete,
  runMenuTriggeredCheck,
  setFdaPromptShowing,
  shouldShowUpdateToast,
  updateState,
} from './updater.svelte'
import { formatUpdateStatus } from './update-status-text'

describe('shouldShowUpdateToast', () => {
  it('returns true only when onboarded, FDA prompt closed, and status is ready', () => {
    expect(shouldShowUpdateToast({ onboarded: true, fdaPromptShowing: false, status: 'ready' })).toBe(true)
  })

  it('returns false while not onboarded', () => {
    expect(shouldShowUpdateToast({ onboarded: false, fdaPromptShowing: false, status: 'ready' })).toBe(false)
  })

  it('returns false while the FDA prompt is showing', () => {
    expect(shouldShowUpdateToast({ onboarded: true, fdaPromptShowing: true, status: 'ready' })).toBe(false)
  })

  it.each(['idle', 'checking', 'downloading'] as const)('returns false when status is %s', (status) => {
    expect(shouldShowUpdateToast({ onboarded: true, fdaPromptShowing: false, status })).toBe(false)
  })

  it('handles every cell of the truth table', () => {
    const statuses = ['idle', 'checking', 'downloading', 'ready'] as const
    for (const onboarded of [false, true]) {
      for (const fdaPromptShowing of [false, true]) {
        for (const status of statuses) {
          const expected = onboarded && !fdaPromptShowing && status === 'ready'
          expect(shouldShowUpdateToast({ onboarded, fdaPromptShowing, status })).toBe(expected)
        }
      }
    }
  })
})

describe('notifyOnboardingComplete', () => {
  beforeEach(() => {
    _resetUpdaterStateForTest()
    addToastMock.mockClear()
    saveSettingsMock.mockClear()
  })

  afterEach(() => {
    _resetUpdaterStateForTest()
  })

  it('persists isOnboarded: true', async () => {
    await notifyOnboardingComplete()
    expect(saveSettingsMock).toHaveBeenCalledWith({ isOnboarded: true })
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

describe('setFdaPromptShowing', () => {
  beforeEach(() => {
    _resetUpdaterStateForTest()
    addToastMock.mockClear()
  })

  afterEach(() => {
    _resetUpdaterStateForTest()
  })

  it('does not show the toast on its own when flipped to true', () => {
    _setUpdateStatusForTest('ready')
    setFdaPromptShowing(true)
    expect(addToastMock).not.toHaveBeenCalled()
  })

  it('re-shows the toast when flipped from true to false if onboarded and ready', async () => {
    _setUpdateStatusForTest('ready')
    await notifyOnboardingComplete()
    addToastMock.mockClear()

    setFdaPromptShowing(true)
    expect(addToastMock).not.toHaveBeenCalled()

    setFdaPromptShowing(false)
    expect(addToastMock).toHaveBeenCalledTimes(1)
    expect(addToastMock.mock.calls[0][1]).toMatchObject({ id: 'update', dismissal: 'persistent' })
  })

  it('does not show the toast on flip-to-false when not onboarded', () => {
    _setUpdateStatusForTest('ready')
    setFdaPromptShowing(true)
    setFdaPromptShowing(false)
    expect(addToastMock).not.toHaveBeenCalled()
  })

  it('does not show the toast on flip-to-false when status is not ready', async () => {
    await notifyOnboardingComplete() // onboarded=true, status=idle
    addToastMock.mockClear()

    setFdaPromptShowing(true)
    setFdaPromptShowing(false)
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
