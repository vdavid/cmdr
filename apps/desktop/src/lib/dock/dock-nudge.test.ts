/**
 * Raising the Dock offer (`dock-nudge.ts`) and answering it
 * (`dock-pin-answer.ts`), covered together because they're one flow.
 *
 * The three events are the point of the exercise: two months from now the
 * question is "how many people saw this, and how many said yes", so `offered`
 * has to fire exactly once per raise and every way out of the toast has to land
 * on an `answered`.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import type { DockPinFailure } from '$lib/ipc/bindings'

const mocks = vi.hoisted(() => ({
  addToast: vi.fn(),
  dismissToast: vi.fn(),
  setSetting: vi.fn(),
  trackEvent: vi.fn(),
  addCmdrToDock: vi.fn(),
  warn: vi.fn(),
}))

vi.mock('$lib/ui/toast', () => ({ addToast: mocks.addToast, dismissToast: mocks.dismissToast }))
vi.mock('$lib/settings', () => ({ setSetting: mocks.setSetting, getSetting: vi.fn(() => '') }))
vi.mock('$lib/tauri-commands', () => ({ trackEvent: mocks.trackEvent, addCmdrToDock: mocks.addCmdrToDock }))
vi.mock('$lib/intl/messages.svelte', () => ({ tString: (key: string) => key }))
vi.mock('$lib/logging/logger', () => ({
  getAppLogger: () => ({ warn: mocks.warn, info: vi.fn(), debug: vi.fn(), error: vi.fn() }),
}))

import { offerDockPin, DOCK_PIN_NUDGE_TOAST_ID } from './dock-nudge'
import { acceptDockPin, declineDockPin } from './dock-pin-answer'

/** The options `offerDockPin` handed the toast store on the last raise. */
function raiseOptions(): { onDismiss?: () => void; level?: string; dismissal?: string; id?: string } {
  expect(mocks.addToast).toHaveBeenCalledOnce()
  return mocks.addToast.mock.calls[0][1] as ReturnType<typeof raiseOptions>
}

beforeEach(() => {
  vi.clearAllMocks()
  mocks.addCmdrToDock.mockResolvedValue(null)
})

describe('offerDockPin', () => {
  it('stamps the nudge ledger as the toast goes UP, never when it is answered', () => {
    offerDockPin()

    expect(mocks.setSetting).toHaveBeenCalledWith(
      'behavior.dockPinNudgeOfferedAt',
      // An instant, so the next nudge can measure how long ago this one spoke.
      expect.stringMatching(/^\d{4}-\d{2}-\d{2}T/),
    )
  })

  it('raises a persistent info toast, so a glance away cannot eat the offer', () => {
    offerDockPin()

    expect(raiseOptions()).toMatchObject({ level: 'info', dismissal: 'persistent', id: DOCK_PIN_NUDGE_TOAST_ID })
  })

  it('records that the offer was made', () => {
    offerDockPin()

    expect(mocks.trackEvent).toHaveBeenCalledWith('dock_pin_offered')
  })

  it('counts the toast frame X as a dismissal, distinct from an active no', () => {
    offerDockPin()

    raiseOptions().onDismiss?.()

    expect(mocks.trackEvent).toHaveBeenCalledWith('dock_pin_answered', { answer: 'dismissed' })
  })
})

describe('declineDockPin', () => {
  it('retires the toast and records the no', () => {
    declineDockPin('dock-pin-nudge')

    expect(mocks.dismissToast).toHaveBeenCalledWith('dock-pin-nudge')
    expect(mocks.trackEvent).toHaveBeenCalledWith('dock_pin_answered', { answer: 'no' })
  })
})

describe('acceptDockPin', () => {
  it('retires the toast and records the yes before the Dock starts blinking', async () => {
    await acceptDockPin('dock-pin-nudge')

    expect(mocks.dismissToast).toHaveBeenCalledWith('dock-pin-nudge')
    expect(mocks.trackEvent).toHaveBeenCalledWith('dock_pin_answered', { answer: 'yes' })
  })

  it('says so briefly when the tile lands', async () => {
    await acceptDockPin('dock-pin-nudge')

    expect(mocks.addToast).toHaveBeenCalledWith('main.dockPinNudge.added', { level: 'success' })
    expect(mocks.trackEvent).not.toHaveBeenCalledWith('dock_pin_failed', expect.anything())
  })

  /**
   * Typed on purpose: renaming a `DockPinFailure` variant has to break this
   * table rather than quietly leave the funnel reporting a name nothing sends.
   */
  const refusals: [failure: DockPinFailure, reason: string, messageKey: string][] = [
    [{ kind: 'blocked', reason: 'managedDock' }, 'managedDock', 'main.dockPinNudge.managedDock'],
    [{ kind: 'blocked', reason: 'outsideApplications' }, 'outsideApplications', 'main.dockPinNudge.notAdded'],
    [{ kind: 'writeRejected' }, 'writeRejected', 'main.dockPinNudge.notAdded'],
    [{ kind: 'timedOut' }, 'timedOut', 'main.dockPinNudge.notAdded'],
    [{ kind: 'dockNotRestarted' }, 'dockNotRestarted', 'main.dockPinNudge.addedButDockDidNotRestart'],
  ]

  it.each(refusals)('reports %o as a typed reason and words it for the user', async (failure, reason, key) => {
    mocks.addCmdrToDock.mockResolvedValue(failure)

    await acceptDockPin('dock-pin-nudge')

    expect(mocks.trackEvent).toHaveBeenCalledWith('dock_pin_failed', { reason })
    expect(mocks.addToast).toHaveBeenCalledWith(key, expect.objectContaining({ level: 'warn' }))
  })

  /**
   * The tile IS stored when only the restart failed, so the copy must not say
   * the pin didn't happen. It's still a `dock_pin_failed`: the person didn't get
   * what they asked for on the spot.
   */
  it('tells the truth when the tile is stored but the Dock never reloaded', async () => {
    mocks.addCmdrToDock.mockResolvedValue({ kind: 'dockNotRestarted' })

    await acceptDockPin('dock-pin-nudge')

    expect(mocks.addToast).toHaveBeenCalledWith('main.dockPinNudge.addedButDockDidNotRestart', { level: 'warn' })
  })

  it('answers yes exactly once, even when the pin comes back refused', async () => {
    mocks.addCmdrToDock.mockResolvedValue({ kind: 'writeRejected' })

    await acceptDockPin('dock-pin-nudge')

    const answers = mocks.trackEvent.mock.calls.filter(([name]) => name === 'dock_pin_answered')
    expect(answers).toEqual([['dock_pin_answered', { answer: 'yes' }]])
  })
})
