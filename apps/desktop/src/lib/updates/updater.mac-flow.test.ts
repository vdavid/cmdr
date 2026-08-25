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
  getSetting: vi.fn((id: string) => {
    if (id === 'onboarding.completed') return false
    if (id === 'updates.autoCheck') return true
    return 60 * 60 * 1000
  }),
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
  applyAutoCheckEnabled,
  checkForUpdates,
  dismissMoveToApplicationsNudge,
  notifyOnboardingComplete,
  runMenuTriggeredCheck,
  setOnboardingShowing,
  startUpdateChecker,
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

    await checkForUpdates('poll')

    expect(downloadUpdateMock).toHaveBeenCalledWith(anUpdate.url, anUpdate.signature)
    expect(installUpdateMock).toHaveBeenCalledTimes(1)
    expect(updateState.status).toBe('ready')
    expect(updateBlockerNotice.blocker).toBeNull()
    expect(trackEventMock).toHaveBeenCalledWith('update_check', {
      trigger: 'poll',
      outcome: 'staged',
      failure: 'none',
      staged_version: '0.33.0',
    })
  })

  it('skips the download entirely and raises the nudge instead', async () => {
    await notifyOnboardingComplete()
    checkForUpdateMock.mockResolvedValueOnce(anUpdate)
    updateWriteBlockerMock.mockResolvedValueOnce('translocated')

    await checkForUpdates('startup')

    // ~63 MB pulled once an hour for an install that can never apply it is the thing to avoid.
    expect(downloadUpdateMock).not.toHaveBeenCalled()
    expect(installUpdateMock).not.toHaveBeenCalled()
    expect(updateBlockerNotice.blocker).toBe('translocated')
    expect(updateState.status).toBe('idle')
    // The dashboard is how David finds out this population exists at all.
    expect(trackEventMock).toHaveBeenCalledWith('update_check', {
      trigger: 'startup',
      outcome: 'blocked',
      failure: 'translocated',
      staged_version: 'none',
    })
  })

  it('raises the nudge once, not once per poll', async () => {
    await notifyOnboardingComplete()
    checkForUpdateMock.mockResolvedValue(anUpdate)
    updateWriteBlockerMock.mockResolvedValue('readOnlyVolume')

    await checkForUpdates('poll')
    dismissMoveToApplicationsNudge()
    await checkForUpdates('poll')

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

    await checkForUpdates('startup')
    expect(updateBlockerNotice.blocker).toBeNull()

    await notifyOnboardingComplete()
    setOnboardingShowing(false)
    expect(updateBlockerNotice.blocker).toBe('translocated')
  })

  it('reports which phase a failed install stopped at, without carrying its message', async () => {
    await notifyOnboardingComplete()
    checkForUpdateMock.mockResolvedValueOnce(anUpdate)
    installUpdateMock.mockRejectedValueOnce(new Error('/Users/dave/Applications/Cmdr.app is not writable'))

    await checkForUpdates('settings')

    expect(trackEventMock).toHaveBeenCalledWith('update_check', {
      trigger: 'settings',
      outcome: 'failed',
      failure: 'install',
      staged_version: 'none',
    })
  })

  it('tells a manual check apart from the background loop on an otherwise identical outcome', async () => {
    // Same install, same answer, two entry points. Without `trigger` a run of manual checks (a
    // user hunting for a fix) and the loop ticking are one indistinguishable number.
    await notifyOnboardingComplete()
    checkForUpdateMock.mockResolvedValue(null)

    await checkForUpdates('poll')
    await checkForUpdates('command')

    const triggers = trackEventMock.mock.calls
      .filter(([name]) => name === 'update_check')
      .map(([, props]) => (props as Record<string, string>).trigger)
    expect(triggers).toEqual(['poll', 'command'])
  })

  it('updates anyway when the classification itself is unavailable', async () => {
    await notifyOnboardingComplete()
    checkForUpdateMock.mockResolvedValueOnce(anUpdate)
    updateWriteBlockerMock.mockRejectedValueOnce(new Error('IPC went missing'))

    await checkForUpdates('poll')

    // The classification saves a doomed download; it isn't permission to update. Treating a
    // hiccup as "can't update" would stop updates that would have worked.
    expect(downloadUpdateMock).toHaveBeenCalledTimes(1)
    expect(updateState.status).toBe('ready')
  })
})

/**
 * `trigger` is what keeps five entry points from collapsing into one number. Each is driven the
 * way production drives it, so a new call site that forgets to name itself doesn't compile, and one
 * wired to the wrong token shows up here rather than months later on a dashboard.
 */
describe('what set a check going', () => {
  /** The `trigger` on every `update_check` fired so far, oldest first. */
  function reportedTriggers(): string[] {
    return trackEventMock.mock.calls
      .filter(([name]) => name === 'update_check')
      .map(([, props]) => (props as Record<string, string>).trigger)
  }

  beforeEach(() => {
    _resetUpdaterStateForTest()
    checkForUpdateMock.mockReset()
    checkForUpdateMock.mockResolvedValue(null)
    updateWriteBlockerMock.mockReset()
    updateWriteBlockerMock.mockResolvedValue(null)
    trackEventMock.mockClear()
    addToastMock.mockClear()
  })

  afterEach(() => {
    vi.useRealTimers()
    _resetUpdaterStateForTest()
  })

  it('names the launch check `startup` and every later tick `poll`', async () => {
    vi.useFakeTimers()
    const stop = startUpdateChecker()
    await vi.advanceTimersByTimeAsync(0)
    expect(reportedTriggers()).toEqual(['startup'])

    // One poll interval on. The launch check and the loop are different questions about the
    // population: one says how many installs came up, the other how long they stay up.
    await vi.advanceTimersByTimeAsync(60 * 60 * 1000)
    expect(reportedTriggers()).toEqual(['startup', 'poll'])
    stop()
  })

  it('names the check that follows turning auto-check back on', async () => {
    applyAutoCheckEnabled(true)
    await vi.waitFor(() => {
      expect(reportedTriggers()).toEqual(['auto_check_on'])
    })
  })

  it('names the `app.checkForUpdates` command', async () => {
    await runMenuTriggeredCheck()
    expect(reportedTriggers()).toEqual(['command'])
  })

  it('names the Settings > Updates button apart from the command', async () => {
    // Both are a person asking on purpose, but only one of them costs a trip into Settings, so
    // they answer different questions about where the affordance is worth having.
    await checkForUpdates('settings')
    await checkForUpdates('command')
    expect(reportedTriggers()).toEqual(['settings', 'command'])
  })
})
