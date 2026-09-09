import { describe, it, vi } from 'vitest'
import { mount, tick, type Component } from 'svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

import NudgeToastContent from './NudgeToastContent.svelte'

/** Mounts a toast body into a detached target and runs axe over it. */
async function expectClean<P extends Record<string, unknown>>(component: Component<P>, props: P): Promise<void> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(component, { target, props })
  await tick()
  await expectNoA11yViolations(target)
}

describe('the shared offer-toast body', () => {
  it('has no a11y violations', async () => {
    await expectClean(NudgeToastContent, {
      title: 'Keep Cmdr in your Dock?',
      body: "You've been using Cmdr for a while now.",
      note: 'You can switch this back any time in Settings.',
      declineLabel: 'No, thanks',
      acceptLabel: 'Yes, please',
      onDecline: vi.fn(),
      onAccept: vi.fn(),
    })
  })
})
