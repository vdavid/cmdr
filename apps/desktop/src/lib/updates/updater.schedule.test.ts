/**
 * The background check's schedule: the loop wakes on a short tick, asks the backend whether a check
 * is due (the backend remembers the last one across relaunches), and checks only then. So a relaunch
 * within the interval doesn't check, and a burst of wakes collapses into one check.
 *
 * The backend's side of the throttle is pinned in Rust (`send_schedule.rs`, `update_schedule.rs`);
 * here it's a fake that answers "due" or "not due", and records what the updater reports back.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

const { checkForUpdateMock, downloadUpdateMock, updateCheckDueInMock, recordUpdateCheckMock } = vi.hoisted(() => ({
  checkForUpdateMock: vi.fn(),
  downloadUpdateMock: vi.fn(() => Promise.resolve()),
  updateCheckDueInMock: vi.fn<(intervalMs: number) => Promise<number | null>>(),
  recordUpdateCheckMock: vi.fn<(answered: boolean) => Promise<void>>(() => Promise.resolve()),
}))

vi.mock('$lib/tauri-commands', () => ({
  checkForUpdate: checkForUpdateMock,
  downloadUpdate: downloadUpdateMock,
  installUpdate: vi.fn(() => Promise.resolve()),
  updateWriteBlocker: vi.fn(() => Promise.resolve(null)),
  updateCheckDueIn: updateCheckDueInMock,
  recordUpdateCheck: recordUpdateCheckMock,
  trackEvent: vi.fn(),
}))

vi.mock('$lib/shortcuts/key-capture', () => ({ isMacOS: () => true }))

vi.mock('$lib/ui/toast', () => ({ addToast: vi.fn(), dismissToast: vi.fn() }))

const THREE_HOURS_MS = 3 * 60 * 60 * 1000

vi.mock('$lib/settings/settings-store', () => ({
  getSetting: vi.fn((id: string) => {
    if (id === 'onboarding.completed') return true
    if (id === 'updates.autoCheck') return true
    return THREE_HOURS_MS
  }),
  setSetting: vi.fn(),
  forceSave: vi.fn(() => Promise.resolve(true)),
  onSpecificSettingChange: vi.fn(() => () => {}),
}))

vi.mock('@tauri-apps/api/app', () => ({ getVersion: vi.fn(() => Promise.resolve('0.28.0')) }))

vi.mock('$lib/logging/logger', () => ({
  getAppLogger: () => ({ debug: () => {}, info: () => {}, warn: () => {}, error: () => {} }),
}))

import {
  UPDATE_WAKE_TICK_MS,
  _resetUpdaterStateForTest,
  applyAutoCheckEnabled,
  checkForUpdates,
  startUpdateChecker,
} from './updater.svelte'

/** Answers `dueIn` as a backend would that last heard of an answered check at `lastAnsweredAt`. */
function scheduleAnsweredAt(lastAnsweredAt: number | null): void {
  let last = lastAnsweredAt
  updateCheckDueInMock.mockImplementation((intervalMs) =>
    Promise.resolve(last === null ? 0 : Math.max(0, last + intervalMs - Date.now())),
  )
  recordUpdateCheckMock.mockImplementation((answered) => {
    if (answered) last = Date.now()
    return Promise.resolve()
  })
}

describe('the background check schedule', () => {
  let stop: (() => void) | undefined

  beforeEach(() => {
    _resetUpdaterStateForTest()
    vi.clearAllMocks()
    vi.useFakeTimers()
    checkForUpdateMock.mockResolvedValue({ kind: 'upToDate' })
  })

  afterEach(() => {
    stop?.()
    stop = undefined
    vi.useRealTimers()
    _resetUpdaterStateForTest()
  })

  it('checks at launch when a check is due, and reports it answered', async () => {
    scheduleAnsweredAt(null)
    stop = startUpdateChecker()
    await vi.advanceTimersByTimeAsync(0)

    expect(checkForUpdateMock).toHaveBeenCalledTimes(1)
    expect(recordUpdateCheckMock).toHaveBeenCalledWith(true)
    expect(updateCheckDueInMock).toHaveBeenCalledWith(THREE_HOURS_MS)
  })

  it("doesn't check at a relaunch within the interval", async () => {
    scheduleAnsweredAt(Date.now() - 60 * 60 * 1000)
    stop = startUpdateChecker()
    await vi.advanceTimersByTimeAsync(0)

    expect(checkForUpdateMock).not.toHaveBeenCalled()
  })

  it('checks once the rest of the interval has passed, and not before', async () => {
    scheduleAnsweredAt(Date.now() - 60 * 60 * 1000)
    stop = startUpdateChecker()

    await vi.advanceTimersByTimeAsync(2 * 60 * 60 * 1000 - UPDATE_WAKE_TICK_MS)
    expect(checkForUpdateMock).not.toHaveBeenCalled()

    await vi.advanceTimersByTimeAsync(UPDATE_WAKE_TICK_MS)
    expect(checkForUpdateMock).toHaveBeenCalledTimes(1)
  })

  it('wakes on the short tick and checks once per interval', async () => {
    scheduleAnsweredAt(null)
    stop = startUpdateChecker()

    await vi.advanceTimersByTimeAsync(THREE_HOURS_MS - 1)
    expect(checkForUpdateMock).toHaveBeenCalledTimes(1)
    // It asked on every tick meanwhile, which is how a shortened interval takes effect within minutes.
    expect(updateCheckDueInMock.mock.calls.length).toBeGreaterThanOrEqual(THREE_HOURS_MS / UPDATE_WAKE_TICK_MS)

    await vi.advanceTimersByTimeAsync(UPDATE_WAKE_TICK_MS)
    expect(checkForUpdateMock).toHaveBeenCalledTimes(2)
  })

  it('reports a check that got no answer, so the backend holds the retry to its floor', async () => {
    scheduleAnsweredAt(null)
    checkForUpdateMock.mockRejectedValueOnce(new Error('offline'))
    stop = startUpdateChecker()
    await vi.advanceTimersByTimeAsync(0)

    expect(recordUpdateCheckMock).toHaveBeenCalledWith(false)
  })

  it('counts a check whose download failed as answered: the check is what the schedule is about', async () => {
    scheduleAnsweredAt(null)
    checkForUpdateMock.mockResolvedValueOnce({ kind: 'available', version: '0.33.0' })
    downloadUpdateMock.mockRejectedValueOnce(new Error('disk full'))
    stop = startUpdateChecker()
    await vi.advanceTimersByTimeAsync(0)

    expect(recordUpdateCheckMock).toHaveBeenCalledTimes(1)
    expect(recordUpdateCheckMock).toHaveBeenCalledWith(true)
  })

  it('lets a manual check reset the clock too', async () => {
    scheduleAnsweredAt(null)
    await checkForUpdates('command')
    expect(recordUpdateCheckMock).toHaveBeenCalledWith(true)

    stop = startUpdateChecker()
    await vi.advanceTimersByTimeAsync(0)
    expect(checkForUpdateMock).toHaveBeenCalledTimes(1)
  })

  it("stays quiet when the backend can't say, rather than checking every tick", async () => {
    updateCheckDueInMock.mockResolvedValue(null)
    stop = startUpdateChecker()
    await vi.advanceTimersByTimeAsync(3 * UPDATE_WAKE_TICK_MS)

    expect(checkForUpdateMock).not.toHaveBeenCalled()
  })

  it('stops asking once stopped', async () => {
    scheduleAnsweredAt(Date.now())
    stop = startUpdateChecker()
    await vi.advanceTimersByTimeAsync(0)
    stop()
    stop = undefined
    updateCheckDueInMock.mockClear()

    await vi.advanceTimersByTimeAsync(3 * UPDATE_WAKE_TICK_MS)
    expect(updateCheckDueInMock).not.toHaveBeenCalled()
  })

  it('checks right away when auto-check is turned on, and not again on the same wake', async () => {
    scheduleAnsweredAt(null)
    applyAutoCheckEnabled(true)
    await vi.advanceTimersByTimeAsync(UPDATE_WAKE_TICK_MS)

    expect(checkForUpdateMock).toHaveBeenCalledTimes(1)
    applyAutoCheckEnabled(false)
  })
})
