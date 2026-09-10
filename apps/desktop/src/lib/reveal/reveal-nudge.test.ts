/**
 * Raising the "Show in Finder" offer (`reveal-nudge.ts`) and answering it
 * (`reveal-nudge-answer.ts`), covered together because they're one flow.
 *
 * Two things earn most of the rows. The three events are the point of the
 * exercise: months from now the question is "how many people saw this, and how
 * many said yes". And accepting renders what the OS was LEFT holding, never
 * what the click asked for, because another app can take the key in between.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import type { RevealHandlerState, RevealHandlerStatus } from '$lib/ipc/bindings'

/**
 * What the command answers on a machine where nothing stands in the way. The
 * Applications-folder gate has its own coverage in `should-show-reveal-nudge.test.ts`;
 * the offer never reaches this file's flow while it's set.
 */
function unblocked(state: RevealHandlerState): RevealHandlerStatus {
  return { state, blockedBy: null }
}

const mocks = vi.hoisted(() => ({
  addToast: vi.fn(),
  dismissToast: vi.fn(),
  setSetting: vi.fn(),
  trackEvent: vi.fn(),
  setRevealHandlerEnabled: vi.fn(),
  warn: vi.fn(),
}))

vi.mock('$lib/ui/toast', () => ({ addToast: mocks.addToast, dismissToast: mocks.dismissToast }))
vi.mock('$lib/settings', () => ({ setSetting: mocks.setSetting, getSetting: vi.fn(() => '') }))
vi.mock('$lib/tauri-commands', () => ({
  trackEvent: mocks.trackEvent,
  setRevealHandlerEnabled: mocks.setRevealHandlerEnabled,
}))
vi.mock('$lib/intl/messages.svelte', () => ({
  tString: (key: string, params?: Record<string, string>) => (params ? `${key}:${JSON.stringify(params)}` : key),
}))
vi.mock('$lib/logging/logger', () => ({
  getAppLogger: () => ({ warn: mocks.warn, info: vi.fn(), debug: vi.fn(), error: vi.fn() }),
}))

import { offerRevealHandler, REVEAL_NUDGE_TOAST_ID } from './reveal-nudge'
import { acceptRevealNudge, declineRevealNudge } from './reveal-nudge-answer'

/** The options `offerRevealHandler` handed the toast store on the last raise. */
function raiseOptions(): { onDismiss?: () => void; level?: string; dismissal?: string; id?: string } {
  expect(mocks.addToast).toHaveBeenCalledOnce()
  return mocks.addToast.mock.calls[0][1] as ReturnType<typeof raiseOptions>
}

beforeEach(() => {
  vi.clearAllMocks()
  mocks.setRevealHandlerEnabled.mockResolvedValue(unblocked({ kind: 'registered' }))
})

describe('offerRevealHandler', () => {
  it('stamps the nudge ledger as the toast goes UP, never when it is answered', () => {
    offerRevealHandler()

    expect(mocks.setSetting).toHaveBeenCalledWith(
      'behavior.revealNudgeOfferedAt',
      // An instant, so the Dock offer can measure how long ago this one spoke.
      expect.stringMatching(/^\d{4}-\d{2}-\d{2}T/),
    )
  })

  it('raises a persistent info toast, so a glance away cannot eat the offer', () => {
    offerRevealHandler()

    expect(raiseOptions()).toMatchObject({ level: 'info', dismissal: 'persistent', id: REVEAL_NUDGE_TOAST_ID })
  })

  it('records that the offer was made', () => {
    offerRevealHandler()

    expect(mocks.trackEvent).toHaveBeenCalledWith('reveal_handler_offered')
  })

  it('counts the toast frame X as a dismissal, distinct from an active no', () => {
    offerRevealHandler()

    raiseOptions().onDismiss?.()

    expect(mocks.trackEvent).toHaveBeenCalledWith('reveal_handler_answered', { answer: 'dismissed' })
  })
})

describe('declineRevealNudge', () => {
  it('retires the toast and records the no', () => {
    declineRevealNudge(REVEAL_NUDGE_TOAST_ID)

    expect(mocks.dismissToast).toHaveBeenCalledWith(REVEAL_NUDGE_TOAST_ID)
    expect(mocks.trackEvent).toHaveBeenCalledWith('reveal_handler_answered', { answer: 'no' })
    expect(mocks.setRevealHandlerEnabled).not.toHaveBeenCalled()
  })
})

describe('acceptRevealNudge', () => {
  it('retires the toast and records the yes before touching the OS', async () => {
    await acceptRevealNudge(REVEAL_NUDGE_TOAST_ID)

    expect(mocks.dismissToast).toHaveBeenCalledWith(REVEAL_NUDGE_TOAST_ID)
    expect(mocks.trackEvent).toHaveBeenCalledWith('reveal_handler_answered', { answer: 'yes' })
    expect(mocks.setRevealHandlerEnabled).toHaveBeenCalledWith(true)
  })

  it('says so briefly when the key becomes ours', async () => {
    await acceptRevealNudge(REVEAL_NUDGE_TOAST_ID)

    expect(mocks.addToast).toHaveBeenCalledWith('main.revealNudge.turnedOn', { level: 'success' })
    expect(mocks.trackEvent).not.toHaveBeenCalledWith('reveal_handler_not_taken', expect.anything())
  })

  /**
   * Typed on purpose: renaming a `RevealHandlerState` variant has to break this
   * table rather than quietly leave the funnel reporting a name nothing sends.
   * `null` is the wrapper's "no answer at all", reported as `unavailable`.
   */
  const refusals: [status: RevealHandlerStatus | null, reason: string, messageKey: string][] = [
    [unblocked({ kind: 'notRegistered' }), 'notRegistered', 'main.revealNudge.notTurnedOn'],
    [null, 'unavailable', 'main.revealNudge.notTurnedOn'],
  ]

  it.each(refusals)('reports %o as a typed reason and words it for the user', async (status, reason, key) => {
    mocks.setRevealHandlerEnabled.mockResolvedValue(status)

    await acceptRevealNudge(REVEAL_NUDGE_TOAST_ID)

    expect(mocks.trackEvent).toHaveBeenCalledWith('reveal_handler_not_taken', { reason })
    expect(mocks.addToast).toHaveBeenCalledWith(key, expect.objectContaining({ level: 'warn' }))
  })

  /**
   * The race the whole "render what the OS reports" rule exists for: Path Finder
   * can claim the key between the offer being drawn and the button being
   * pressed, and saying "done" then would be a lie.
   */
  it('names the app that got there first rather than claiming success', async () => {
    mocks.setRevealHandlerEnabled.mockResolvedValue(
      unblocked({ kind: 'heldByOtherApp', bundleId: 'com.cocoatech.PathFinder', displayName: 'Path Finder' }),
    )

    await acceptRevealNudge(REVEAL_NUDGE_TOAST_ID)

    expect(mocks.trackEvent).toHaveBeenCalledWith('reveal_handler_not_taken', { reason: 'heldByOtherApp' })
    expect(mocks.addToast).toHaveBeenCalledWith(
      'main.revealNudge.heldByOtherApp:{"app":"Path Finder"}',
      expect.objectContaining({ level: 'warn' }),
    )
  })

  /** A holder that isn't installed any more has no name, so the raw id is all there is. */
  it('falls back to the bundle id when the holder has no display name', async () => {
    mocks.setRevealHandlerEnabled.mockResolvedValue(
      unblocked({ kind: 'heldByOtherApp', bundleId: 'com.example.Gone', displayName: null }),
    )

    await acceptRevealNudge(REVEAL_NUDGE_TOAST_ID)

    expect(mocks.addToast).toHaveBeenCalledWith(
      'main.revealNudge.heldByOtherApp:{"app":"com.example.Gone"}',
      expect.objectContaining({ level: 'warn' }),
    )
  })

  it('answers yes exactly once, even when the key does not end up ours', async () => {
    mocks.setRevealHandlerEnabled.mockResolvedValue(unblocked({ kind: 'notRegistered' }))

    await acceptRevealNudge(REVEAL_NUDGE_TOAST_ID)

    const answers = mocks.trackEvent.mock.calls.filter(([name]) => name === 'reveal_handler_answered')
    expect(answers).toEqual([['reveal_handler_answered', { answer: 'yes' }]])
  })
})
