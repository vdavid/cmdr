/**
 * The once-ever notice for the first reveal that actually lands here.
 *
 * Two things are worth pinning. It fires ONCE: a person who reveals a file
 * fifty times a day must be told the first time and never again. And it
 * self-dismisses, because nothing on it needs an answer.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'

const mocks = vi.hoisted(() => ({
  addToast: vi.fn(),
  getSetting: vi.fn(),
  setSetting: vi.fn(),
  onRevealDelivered: vi.fn(),
  info: vi.fn(),
}))

vi.mock('$lib/ui/toast', () => ({ addToast: mocks.addToast }))
vi.mock('$lib/settings', () => ({ getSetting: mocks.getSetting, setSetting: mocks.setSetting }))
vi.mock('$lib/tauri-commands', () => ({ onRevealDelivered: mocks.onRevealDelivered }))
vi.mock('$lib/logging/logger', () => ({
  getAppLogger: () => ({ info: mocks.info, warn: vi.fn(), debug: vi.fn(), error: vi.fn() }),
}))

import {
  REVEAL_ACTIVATION_TOAST_ID,
  REVEAL_ACTIVATION_TOAST_MS,
  showRevealActivationNoticeOnce,
  startRevealActivationNotice,
} from './reveal-activation-notice'

/** Whether the flag has been spent, so a second call sees what the first wrote. */
let seen: boolean

beforeEach(() => {
  vi.clearAllMocks()
  seen = false
  mocks.getSetting.mockImplementation((id: string) => {
    if (id === 'behavior.revealActivationNoticeSeen') return seen
    throw new Error(`Unexpected getSetting(${id})`)
  })
  mocks.setSetting.mockImplementation((id: string, value: boolean) => {
    if (id === 'behavior.revealActivationNoticeSeen') seen = value
  })
  mocks.onRevealDelivered.mockResolvedValue(() => undefined)
})

describe('showRevealActivationNoticeOnce', () => {
  it('raises a self-dismissing notice the first time a reveal lands', () => {
    showRevealActivationNoticeOnce()

    expect(mocks.addToast).toHaveBeenCalledOnce()
    expect(mocks.addToast.mock.calls[0][1]).toMatchObject({
      level: 'info',
      dismissal: 'transient',
      timeoutMs: REVEAL_ACTIVATION_TOAST_MS,
      id: REVEAL_ACTIVATION_TOAST_ID,
    })
  })

  it('spends the flag as the notice goes up, so a crash cannot repeat it forever', () => {
    showRevealActivationNoticeOnce()

    expect(mocks.setSetting).toHaveBeenCalledWith('behavior.revealActivationNoticeSeen', true)
  })

  it('says nothing on the second reveal, or the fiftieth', () => {
    showRevealActivationNoticeOnce()
    showRevealActivationNoticeOnce()
    showRevealActivationNoticeOnce()

    expect(mocks.addToast).toHaveBeenCalledOnce()
  })

  it('stays quiet on a machine that was already told, launches later', () => {
    seen = true

    showRevealActivationNoticeOnce()

    expect(mocks.addToast).not.toHaveBeenCalled()
    expect(mocks.setSetting).not.toHaveBeenCalled()
  })
})

describe('startRevealActivationNotice', () => {
  it('subscribes to the delivered-reveal event and hands back its unsubscribe', async () => {
    const unlisten = vi.fn()
    mocks.onRevealDelivered.mockResolvedValue(unlisten)

    await expect(startRevealActivationNotice()).resolves.toBe(unlisten)
    expect(mocks.onRevealDelivered).toHaveBeenCalledOnce()
  })

  /**
   * The flag is read per event, ❌ never snapshotted at subscribe time: the
   * subscription outlives the window's whole session, and a snapshot taken
   * before the first reveal would arm a second notice for the rest of it.
   */
  it('reads the flag per event rather than at subscribe time', async () => {
    await startRevealActivationNotice()
    const onDelivered = mocks.onRevealDelivered.mock.calls[0][0] as () => void

    onDelivered()
    onDelivered()

    expect(mocks.addToast).toHaveBeenCalledOnce()
  })
})
