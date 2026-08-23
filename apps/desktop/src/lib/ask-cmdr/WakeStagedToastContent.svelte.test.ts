/**
 * The notice that Ask Cmdr staged something on its own: what it says, and the two ways in.
 *
 * The pair of actions is the load-bearing part. "Review" answers WHAT the agent wants to do;
 * "See why" answers WHY, and nobody asked for any of this, so both have to be reachable.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, flushSync, tick } from 'svelte'

const openSuggestedOps = vi.fn(() => Promise.resolve())
vi.mock('$lib/suggested-ops/suggested-ops-trigger.svelte', () => ({
  openSuggestedOps: (): Promise<void> => openSuggestedOps(),
}))

const switchToThread = vi.fn((_id: number) => Promise.resolve())
const openRail = vi.fn(() => Promise.resolve())
vi.mock('./ask-cmdr-trigger.svelte', () => ({
  switchToThread: (id: number): Promise<void> => switchToThread(id),
  openRail: (): Promise<void> => openRail(),
}))

const dismissToast = vi.fn<(id: string) => void>()
vi.mock('$lib/ui/toast', () => ({
  dismissToast: (id: string): void => {
    dismissToast(id)
  },
}))

import WakeStagedToastContent from './WakeStagedToastContent.svelte'

let target: HTMLElement

function render(proposals = 1): void {
  target = document.createElement('div')
  document.body.appendChild(target)
  mount(WakeStagedToastContent, { target, props: { toastId: 'toast-1', conversationId: 42, proposals } })
  flushSync()
}

beforeEach(() => {
  document.body.innerHTML = ''
  openSuggestedOps.mockClear()
  switchToThread.mockClear()
  openRail.mockClear()
  dismissToast.mockClear()
})

describe('WakeStagedToastContent', () => {
  it('names the agent as the one making the offer, and counts what is waiting', () => {
    render(3)
    expect(target.querySelector('.title')?.textContent.trim()).toBe('Ask Cmdr has 3 suggestions for you')
  })

  it('speaks of one suggestion in the singular', () => {
    render(1)
    expect(target.querySelector('.title')?.textContent.trim()).toBe('Ask Cmdr has 1 suggestion for you')
  })

  it('Review opens the suggestions and gets out of the way', () => {
    render()
    target.querySelector<HTMLButtonElement>('.actions button')?.click()
    expect(openSuggestedOps).toHaveBeenCalledOnce()
    expect(dismissToast).toHaveBeenCalledWith('toast-1')
  })

  /** ⚠️ `switchToThread` BEFORE `openRail`: the other order bootstraps the most recent thread
   *  on a closed→open transition and wastes a fetch on one we are about to replace. */
  it('See why opens the wake thread, switching before it opens the rail', async () => {
    render()
    target.querySelector<HTMLButtonElement>('.thread-link')?.click()
    await tick()
    expect(switchToThread).toHaveBeenCalledWith(42)
    expect(openRail).toHaveBeenCalledOnce()
    expect(switchToThread.mock.invocationCallOrder[0]).toBeLessThan(openRail.mock.invocationCallOrder[0])
  })
})
