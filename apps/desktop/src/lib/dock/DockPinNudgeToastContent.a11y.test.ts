import { describe, it, vi } from 'vitest'
import { mount, tick, type Component } from 'svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

vi.mock('./dock-pin-answer', () => ({
  declineDockPin: vi.fn(),
  acceptDockPin: vi.fn(() => Promise.resolve()),
}))

import DockPinNudgeToastContent from './DockPinNudgeToastContent.svelte'

/** Mounts a toast body into a detached target and runs axe over it. */
async function expectClean<P extends Record<string, unknown>>(component: Component<P>, props: P): Promise<void> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(component, { target, props })
  await tick()
  await expectNoA11yViolations(target)
}

describe('the "keep Cmdr in your Dock" toast', () => {
  it('has no a11y violations', async () => {
    await expectClean(DockPinNudgeToastContent, { toastId: 'dock-pin-nudge' })
  })
})
