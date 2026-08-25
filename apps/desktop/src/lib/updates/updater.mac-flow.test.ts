/**
 * The macOS branch of `checkForUpdates()`, which `updater.test.ts` can't reach: jsdom's userAgent
 * has no "Macintosh", so that file exercises the Tauri-plugin branch throughout.
 *
 * What's covered here is the read-only-bundle gate. An install that can't write its own `.app`
 * (App Translocation, or a still-mounted disk image) used to download ~63 MB every poll interval
 * and fail the install silently, forever; now the check stops before the download and the user
 * gets told what to do about it.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

const {
  addToastMock,
  checkForUpdateMock,
  downloadUpdateMock,
  installUpdateMock,
  updateWriteBlockerMock,
  trackEventMock,
  getVersionMock,
} = vi.hoisted(() => ({
  addToastMock: vi.fn(),
  checkForUpdateMock: vi.fn(),
  downloadUpdateMock: vi.fn(() => Promise.resolve()),
  installUpdateMock: vi.fn(() => Promise.resolve()),
  updateWriteBlockerMock: vi.fn<() => Promise<'translocated' | 'readOnlyVolume' | null>>(() => Promise.resolve(null)),
  trackEventMock: vi.fn(),
  getVersionMock: vi.fn(() => Promise.resolve('0.28.0')),
}))

vi.mock('$lib/tauri-commands', () => ({
  checkForUpdate: checkForUpdateMock,
  downloadUpdate: downloadUpdateMock,
  installUpdate: installUpdateMock,
  updateWriteBlocker: updateWriteBlockerMock,
  trackEvent: trackEventMock,
}))

// The whole point of this file: take the macOS branch, where the three custom commands live.
vi.mock('$lib/shortcuts/key-capture', () => ({ isMacOS: () => true }))

vi.mock('$lib/ui/toast', () => ({ addToast: addToastMock, dismissToast: vi.fn() }))

vi.mock('$lib/settings/settings-store', () => ({
  getSetting: vi.fn((id: string) => (id === 'onboarding.completed' ? false : 60 * 60 * 1000)),
  setSetting: vi.fn(),
  forceSave: vi.fn(() => Promise.resolve(true)),
  onSpecificSettingChange: vi.fn(() => () => {}),
}))

vi.mock('@tauri-apps/api/app', () => ({ getVersion: getVersionMock }))

vi.mock('$lib/logging/logger', () => ({
  getAppLogger: () => ({ debug: () => {}, info: () => {}, warn: () => {}, error: () => {} }),
}))

import {
  _resetUpdaterStateForTest,
  checkForUpdates,
  dismissMoveToApplicationsNudge,
  notifyOnboardingComplete,
  setOnboardingShowing,
  updateBlockerNotice,
  updateState,
} from './updater.svelte'

const anUpdate = { version: '0.33.0', url: 'https://example.invalid/Cmdr.tar.gz', signature: 'sig' }

describe('an update found on an install that can’t write its own bundle', () => {
  beforeEach(() => {
    _resetUpdaterStateForTest()
    addToastMock.mockClear()
    checkForUpdateMock.mockReset()
    downloadUpdateMock.mockClear()
    installUpdateMock.mockClear()
    updateWriteBlockerMock.mockReset()
    updateWriteBlockerMock.mockResolvedValue(null)
    trackEventMock.mockClear()
  })

  afterEach(() => {
    _resetUpdaterStateForTest()
  })

  it('downloads and installs when nothing is in the way', async () => {
    await notifyOnboardingComplete()
    checkForUpdateMock.mockResolvedValueOnce(anUpdate)

    await checkForUpdates()

    expect(downloadUpdateMock).toHaveBeenCalledWith(anUpdate.url, anUpdate.signature)
    expect(installUpdateMock).toHaveBeenCalledTimes(1)
    expect(updateState.status).toBe('ready')
    expect(updateBlockerNotice.blocker).toBeNull()
    expect(trackEventMock).toHaveBeenCalledWith('update_check', {
      outcome: 'staged',
      failure: 'none',
      staged_version: '0.33.0',
    })
  })

  it('skips the download entirely and raises the nudge instead', async () => {
    await notifyOnboardingComplete()
    checkForUpdateMock.mockResolvedValueOnce(anUpdate)
    updateWriteBlockerMock.mockResolvedValueOnce('translocated')

    await checkForUpdates()

    // ~63 MB pulled once an hour for an install that can never apply it is the thing to avoid.
    expect(downloadUpdateMock).not.toHaveBeenCalled()
    expect(installUpdateMock).not.toHaveBeenCalled()
    expect(updateBlockerNotice.blocker).toBe('translocated')
    expect(updateState.status).toBe('idle')
    // The dashboard is how David finds out this population exists at all.
    expect(trackEventMock).toHaveBeenCalledWith('update_check', {
      outcome: 'blocked',
      failure: 'translocated',
      staged_version: 'none',
    })
  })

  it('raises the nudge once, not once per poll', async () => {
    await notifyOnboardingComplete()
    checkForUpdateMock.mockResolvedValue(anUpdate)
    updateWriteBlockerMock.mockResolvedValue('readOnlyVolume')

    await checkForUpdates()
    dismissMoveToApplicationsNudge()
    await checkForUpdates()

    // The check itself keeps running (it's what keeps the install counted as active); the modal
    // doesn't come back, because the answer can't change until the user moves the app.
    expect(checkForUpdateMock).toHaveBeenCalledTimes(2)
    expect(updateBlockerNotice.blocker).toBeNull()
  })

  it('holds the nudge back during onboarding, then raises it when the wizard closes', async () => {
    // A download opened straight from ~/Downloads is exactly what macOS translocates, so this
    // population meets the nudge on its FIRST launch, with onboarding still on screen.
    setOnboardingShowing(true)
    checkForUpdateMock.mockResolvedValueOnce(anUpdate)
    updateWriteBlockerMock.mockResolvedValueOnce('translocated')

    await checkForUpdates()
    expect(updateBlockerNotice.blocker).toBeNull()

    await notifyOnboardingComplete()
    setOnboardingShowing(false)
    expect(updateBlockerNotice.blocker).toBe('translocated')
  })

  it('reports which phase a failed install stopped at, without carrying its message', async () => {
    await notifyOnboardingComplete()
    checkForUpdateMock.mockResolvedValueOnce(anUpdate)
    installUpdateMock.mockRejectedValueOnce(new Error('/Users/dave/Applications/Cmdr.app is not writable'))

    await checkForUpdates()

    expect(trackEventMock).toHaveBeenCalledWith('update_check', {
      outcome: 'failed',
      failure: 'install',
      staged_version: 'none',
    })
  })

  it('updates anyway when the classification itself is unavailable', async () => {
    await notifyOnboardingComplete()
    checkForUpdateMock.mockResolvedValueOnce(anUpdate)
    updateWriteBlockerMock.mockRejectedValueOnce(new Error('IPC went missing'))

    await checkForUpdates()

    // The classification saves a doomed download; it isn't permission to update. Treating a
    // hiccup as "can't update" would stop updates that would have worked.
    expect(downloadUpdateMock).toHaveBeenCalledTimes(1)
    expect(updateState.status).toBe('ready')
  })
})
