import { describe, it, vi } from 'vitest'
import { mount, tick, type Component } from 'svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

vi.mock('./reveal-nudge-answer', () => ({
  declineRevealNudge: vi.fn(),
  acceptRevealNudge: vi.fn(() => Promise.resolve()),
}))

import RevealNudgeToastContent from './RevealNudgeToastContent.svelte'

/** Mounts a toast body into a detached target and runs axe over it. */
async function expectClean<P extends Record<string, unknown>>(component: Component<P>, props: P): Promise<void> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(component, { target, props })
  await tick()
  await expectNoA11yViolations(target)
}

describe('the "open Show in Finder in Cmdr" toast', () => {
  it('has no a11y violations', async () => {
    await expectClean(RevealNudgeToastContent, { toastId: 'reveal-handler-nudge' })
  })
})
