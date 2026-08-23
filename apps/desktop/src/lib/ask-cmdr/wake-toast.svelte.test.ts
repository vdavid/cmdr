/**
 * When the proactive agent is allowed to interrupt.
 *
 * `askCmdr.wakeToast` is the user saying whether an agent that noticed something may say so
 * out loud. The setting is read at ANNOUNCE time, not at subscribe time, so turning it off
 * silences the wake already in flight — which is the only reading of the switch that does what
 * somebody flipping it mid-wake meant.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'

const addToast = vi.fn()
vi.mock('$lib/ui/toast', () => ({
  // eslint-disable-next-line @typescript-eslint/no-explicit-any -- mirrors addToast's heterogenous props map
  addToast: (content: unknown, options: any): void => {
    addToast(content, options)
  },
}))

const settings = { 'askCmdr.wakeToast': true as boolean }
vi.mock('$lib/settings', () => ({
  getSetting: (id: keyof typeof settings): boolean => settings[id],
}))

const unlisten = vi.fn()
let staged: ((payload: { conversationId: number; proposals: number }) => void) | null = null
const onAgentWakeStaged = vi.fn((callback: (payload: { conversationId: number; proposals: number }) => void) => {
  staged = callback
  return Promise.resolve(unlisten)
})
vi.mock('$lib/tauri-commands', () => ({
  onAgentWakeStaged: (callback: (payload: { conversationId: number; proposals: number }) => void) =>
    onAgentWakeStaged(callback),
}))

// The toast's content component reaches the rail and the review surface, and the rail's entry
// point builds the whole explorer state on import. Neither matters to what is under test here.
vi.mock('$lib/suggested-ops/suggested-ops-trigger.svelte', () => ({
  openSuggestedOps: (): Promise<void> => Promise.resolve(),
}))
vi.mock('./ask-cmdr-trigger.svelte', () => ({
  switchToThread: (): Promise<void> => Promise.resolve(),
  openRail: (): Promise<void> => Promise.resolve(),
}))

import { announceStagedWake, startWakeToast, stopWakeToast, WAKE_TOAST_GROUP } from './wake-toast.svelte'
import WakeStagedToastContent from './WakeStagedToastContent.svelte'

beforeEach(() => {
  stopWakeToast()
  vi.clearAllMocks()
  staged = null
  settings['askCmdr.wakeToast'] = true
})

describe('announceStagedWake', () => {
  it('raises one grouped toast carrying the thread and the count', () => {
    announceStagedWake(42, 3)
    expect(addToast).toHaveBeenCalledOnce()
    const [content, options] = addToast.mock.calls[0] as [unknown, Record<string, unknown>]
    expect(content).toBe(WakeStagedToastContent)
    expect(options.toastGroup).toBe(WAKE_TOAST_GROUP)
    expect(options.props).toMatchObject({ conversationId: 42, proposals: 3 })
  })

  /** Auto-dismissing on purpose: the proposals sit in the suggestions badge until reviewed, so
   *  nothing is lost when the toast goes. A persistent one would just be a thing to close. */
  it('does not make the toast persistent', () => {
    announceStagedWake(42, 1)
    const [, options] = addToast.mock.calls[0] as [unknown, Record<string, unknown>]
    expect(options.dismissal).toBeUndefined()
  })

  it('says nothing at all when the user turned the toast off', () => {
    settings['askCmdr.wakeToast'] = false
    announceStagedWake(42, 3)
    expect(addToast).not.toHaveBeenCalled()
  })

  /** One toast per thread, so a re-emit for the same wake replaces rather than stacks. */
  it('keys the toast on the thread it came from', () => {
    announceStagedWake(42, 1)
    announceStagedWake(43, 1)
    const ids = addToast.mock.calls.map((call) => (call[1] as Record<string, unknown>).id)
    expect(ids).toEqual(['agent-wake-staged:42', 'agent-wake-staged:43'])
  })
})

describe('the subscription', () => {
  it('turns a staged wake into its toast', async () => {
    await startWakeToast()
    staged?.({ conversationId: 7, proposals: 2 })
    const [, options] = addToast.mock.calls[0] as [unknown, Record<string, unknown>]
    expect(options.props).toMatchObject({ conversationId: 7, proposals: 2 })
  })

  /** HMR re-runs the window's `onMount`, so a second start that stacked a listener would
   *  double every toast from then on. */
  it('subscribes once however many times it is started', async () => {
    await startWakeToast()
    await startWakeToast()
    expect(onAgentWakeStaged).toHaveBeenCalledOnce()
  })

  it('lets go of the listener on teardown, and can be started again after', async () => {
    await startWakeToast()
    stopWakeToast()
    expect(unlisten).toHaveBeenCalledOnce()
    await startWakeToast()
    expect(onAgentWakeStaged).toHaveBeenCalledTimes(2)
  })

  it('is a no-op to stop when it never started', () => {
    stopWakeToast()
    expect(unlisten).not.toHaveBeenCalled()
  })
})
