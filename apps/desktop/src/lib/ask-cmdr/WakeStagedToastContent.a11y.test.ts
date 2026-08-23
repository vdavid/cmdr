/**
 * Tier 3 a11y tests for `WakeStagedToastContent.svelte`.
 *
 * The notice that Ask Cmdr staged something on its own. Both actions carry visible text, so
 * what this guards is contrast and the link-shaped second button staying a real button.
 */

import { describe, it, vi, beforeEach } from 'vitest'
import { mount, tick } from 'svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

vi.mock('$lib/suggested-ops/suggested-ops-trigger.svelte', () => ({
  openSuggestedOps: (): Promise<void> => Promise.resolve(),
}))
vi.mock('./ask-cmdr-trigger.svelte', () => ({
  switchToThread: (): Promise<void> => Promise.resolve(),
  openRail: (): Promise<void> => Promise.resolve(),
}))
vi.mock('$lib/ui/toast', () => ({ dismissToast: (): void => {} }))

import WakeStagedToastContent from './WakeStagedToastContent.svelte'

function mountToast(proposals: number): HTMLElement {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(WakeStagedToastContent, { target, props: { toastId: 'toast-1', conversationId: 42, proposals } })
  return target
}

beforeEach(() => {
  document.body.innerHTML = ''
})

describe('WakeStagedToastContent a11y', () => {
  it('one staged suggestion has no a11y violations', async () => {
    const target = mountToast(1)
    await tick()
    await expectNoA11yViolations(target)
  })

  it('several staged suggestions have no a11y violations', async () => {
    const target = mountToast(12)
    await tick()
    await expectNoA11yViolations(target)
  })
})
