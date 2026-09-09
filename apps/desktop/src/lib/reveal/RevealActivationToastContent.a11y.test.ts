import { describe, it, vi } from 'vitest'
import { mount, tick, type Component } from 'svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

vi.mock('./reveal-settings-link', () => ({
  openSettingsToRevealHandler: vi.fn(() => Promise.resolve()),
}))

import RevealActivationToastContent from './RevealActivationToastContent.svelte'

/** Mounts a toast body into a detached target and runs axe over it. */
async function expectClean<P extends Record<string, unknown>>(component: Component<P>, props: P): Promise<void> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(component, { target, props })
  await tick()
  await expectNoA11yViolations(target)
}

describe('the first-reveal notice', () => {
  it('has no a11y violations', async () => {
    await expectClean(RevealActivationToastContent, {})
  })
})
